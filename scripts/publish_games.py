#!/usr/bin/env python3
"""Export curated engine-made content into the companion Games repository.

Native projects export source and authored assets, excluding target, dist and
check-output directories. Player saves and compiler artifacts remain local.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path, PurePosixPath


CATEGORIES = ("games", "prototypes", "tests", "demos")
MANIFEST_NAME = "games-publish.json"
CATALOG_NAME = ".games-catalog.json"


class PublishError(RuntimeError):
    """A manifest or export is unsafe or invalid."""


def safe_relative(value: object, field: str, *, allow_dot: bool = False) -> Path:
    if not isinstance(value, str) or not value.strip():
        raise PublishError(f"{field} must be a non-empty string")
    normalized = value.replace("\\", "/")
    path = PurePosixPath(normalized)
    if path.is_absolute() or ".." in path.parts:
        raise PublishError(f"{field} must stay inside the repository: {value!r}")
    if not allow_dot and normalized in (".", "./"):
        raise PublishError(f"{field} cannot be the repository root")
    return Path(*path.parts)


def load_manifest(root: Path) -> dict:
    path = root / MANIFEST_NAME
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PublishError(f"cannot read {MANIFEST_NAME}: {error}") from error
    if data.get("version") != 2:
        raise PublishError("games-publish.json must have version 2")
    if not isinstance(data.get("target_repository"), str):
        raise PublishError("target_repository must be a string")
    collections = data.get("collections")
    if not isinstance(collections, dict):
        raise PublishError("collections must be an object")
    unknown = sorted(set(collections) - set(CATEGORIES))
    if unknown:
        raise PublishError(f"unknown collections: {', '.join(unknown)}")
    for category in CATEGORIES:
        if not isinstance(collections.get(category), list) or not collections[category]:
            raise PublishError(f"collections.{category} must be a non-empty array")
    playables = data.get("playables")
    if not isinstance(playables, list) or not playables:
        raise PublishError("playables must be a non-empty array")
    slugs = set()
    for index, playable in enumerate(playables):
        if not isinstance(playable, dict):
            raise PublishError(f"playables[{index}] must be an object")
        slug = playable.get("slug")
        if not isinstance(slug, str) or not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", slug):
            raise PublishError(f"playables[{index}].slug must be lowercase kebab-case")
        if slug in slugs:
            raise PublishError(f"duplicate playable slug: {slug}")
        slugs.add(slug)
        if not isinstance(playable.get("name"), str) or not playable["name"].strip():
            raise PublishError(f"playables[{index}].name must be a non-empty string")
        safe_relative(playable.get("entry"), f"playables[{index}].entry")
        files = playable.get("files")
        if not isinstance(files, list) or not files:
            raise PublishError(f"playables[{index}].files must be a non-empty array")
        for file_index, value in enumerate(files):
            safe_relative(value, f"playables[{index}].files[{file_index}]")
        arguments = playable.get("arguments")
        if not isinstance(arguments, list) or any(
            not isinstance(value, str) or "\n" in value or "\r" in value for value in arguments
        ):
            raise PublishError(f"playables[{index}].arguments must be an array of single-line strings")
    preserve = data.get("preserve", [])
    if not isinstance(preserve, list):
        raise PublishError("preserve must be an array")
    preserved_paths = []
    for index, value in enumerate(preserve):
        path = safe_relative(value, f"preserve[{index}]")
        if len(path.parts) < 2 or path.parts[0] not in CATEGORIES:
            raise PublishError(f"preserve[{index}] must name content inside a managed collection")
        preserved_paths.append(path)
    if len(set(preserved_paths)) != len(preserved_paths):
        raise PublishError("preserve paths must be unique")
    for index, path in enumerate(preserved_paths):
        for other in preserved_paths[index + 1 :]:
            if path in other.parents or other in path.parents:
                raise PublishError("preserve paths cannot contain one another")
    return data


def files_under(source: Path, root: Path | None = None):
    if source.is_symlink():
        raise PublishError(f"symlinks are not published: {source}")
    if source.is_file():
        yield source, Path(source.name)
        return
    if not source.is_dir():
        raise PublishError(f"source does not exist: {source}")
    for directory, folders, names in os.walk(source, followlinks=False):
        parent = Path(directory)
        excluded = {".git", "__pycache__"}
        if (parent / "Cargo.toml").is_file():
            excluded.update({"target", "dist", ".blue-check", ".be2-work"})
        folders[:] = sorted(name for name in folders if name not in excluded)
        for name in folders:
            if (parent / name).is_symlink():
                raise PublishError(f"symlinks are not published: {parent / name}")
        if root and parent == root / "assets/games/leo/assets":
            folders[:] = [name for name in folders if name != "audio" and not name.startswith((".audio-render-", ".audio-old-"))]
        for name in sorted(names):
            candidate = parent / name
            if root and candidate == root / "assets/games/leo/assets/audio-source/leaves.wav":
                continue
            if candidate.is_symlink():
                raise PublishError(f"symlinks are not published: {candidate}")
            if candidate.is_file():
                yield candidate, candidate.relative_to(source)


def export_tree(root: Path, staging: Path, manifest: dict, revision: str) -> dict:
    emitted: dict[str, Path] = {}
    catalog_files = []
    counts = {category: 0 for category in CATEGORIES}

    for category in CATEGORIES:
        (staging / category).mkdir(parents=True, exist_ok=True)
        for index, entry in enumerate(manifest["collections"][category]):
            if not isinstance(entry, dict):
                raise PublishError(f"collections.{category}[{index}] must be an object")
            source_rel = safe_relative(entry.get("source"), f"{category}[{index}].source")
            destination_rel = safe_relative(
                entry.get("destination", "."),
                f"{category}[{index}].destination",
                allow_dot=True,
            )
            source = (root / source_rel).resolve()
            try:
                source.relative_to(root.resolve())
            except ValueError as error:
                raise PublishError(f"source escapes repository: {source_rel}") from error

            is_file = source.is_file()
            for candidate, nested in files_under(source, root=root):
                suffix = Path(candidate.name) if is_file and destination_rel == Path(".") else nested
                target_rel = Path(category) / destination_rel / suffix
                target_key = target_rel.as_posix()
                if target_key in emitted:
                    raise PublishError(
                        f"two entries publish {target_key}: {emitted[target_key]} and {candidate}"
                    )
                emitted[target_key] = candidate
                target = staging / target_rel
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(candidate, target, follow_symlinks=False)
                digest = hashlib.sha256(target.read_bytes()).hexdigest()
                catalog_files.append({"path": target_key, "sha256": digest})
                counts[category] += 1

    catalog = {
        "schema_version": 1,
        "source_repository": manifest["source_repository"],
        "source_revision": revision,
        "target_repository": manifest["target_repository"],
        "collections": counts,
        "playables": manifest["playables"],
        "preserved_paths": manifest.get("preserve", []),
        "files": sorted(catalog_files, key=lambda item: item["path"]),
    }
    published_paths = set(emitted)
    for value in catalog["preserved_paths"]:
        preserved_path = safe_relative(value, "preserve")
        prefix = preserved_path.as_posix().rstrip("/") + "/"
        if preserved_path.as_posix() in published_paths or any(path.startswith(prefix) for path in published_paths):
            raise PublishError(f"preserved path overlaps published content: {preserved_path.as_posix()}")
    for playable in catalog["playables"]:
        if playable["entry"] not in published_paths:
            raise PublishError(f"playable entry is not published: {playable['entry']}")
        for value in playable["files"]:
            prefix = value.rstrip("/") + "/"
            if value not in published_paths and not any(path.startswith(prefix) for path in published_paths):
                raise PublishError(f"playable file is not published: {value}")
    (staging / CATALOG_NAME).write_text(
        json.dumps(catalog, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return catalog


def git_revision(root: Path) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), "rev-parse", "HEAD"],
        check=False,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip() if result.returncode == 0 else "working-tree"


SHA256 = re.compile(r"[0-9a-f]{64}")

# Replacement point for one file; tests substitute it to simulate a failure part-way through an export.
_replace = os.replace


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def contained(output: Path, relative: str, what: str) -> Path:
    """`output / relative`, refusing any path whose existing parents are symlinks or that leaves `output`."""
    rel = safe_relative(relative, what)
    if not rel.parts or rel.parts[0] not in CATEGORIES:
        raise PublishError(f"{what} is outside the managed collections: {relative!r}")
    current = output
    for part in rel.parts[:-1]:
        current = current / part
        if current.is_symlink():
            raise PublishError(f"{what} goes through a symlinked directory: {current}")
    target = output / rel
    try:
        target.parent.resolve().relative_to(output.resolve())
    except ValueError as error:
        raise PublishError(f"{what} escapes the output directory: {relative!r}") from error
    return target


def read_ownership(output: Path) -> dict[str, str]:
    """Paths a previous export wrote, from the catalog it left. No catalog means nothing is owned.

    A catalog that exists but cannot be trusted stops the export: guessing would widen what may be deleted.
    """
    path = output / CATALOG_NAME
    if not path.exists() and not path.is_symlink():
        return {}
    where = f"{CATALOG_NAME} in the output repository"
    if path.is_symlink() or not path.is_file():
        raise PublishError(f"{where} is not a regular file; fix or remove it to start a fresh ownership record")
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise PublishError(
            f"{where} is unreadable ({error}); nothing was changed. Restore it from Git, or delete it to "
            "export without deleting anything stale"
        ) from error
    files = data.get("files") if isinstance(data, dict) else None
    if not isinstance(files, list):
        raise PublishError(f"{where} has no 'files' list; nothing was changed. Restore it from Git or delete it")
    owned: dict[str, str] = {}
    for index, item in enumerate(files):
        value = item.get("path") if isinstance(item, dict) else None
        digest = item.get("sha256") if isinstance(item, dict) else None
        if not isinstance(value, str) or not isinstance(digest, str) or not SHA256.fullmatch(digest):
            raise PublishError(f"{where}: files[{index}] needs a path and a 64-character sha256; nothing was changed")
        try:
            contained(output, value, f"{CATALOG_NAME} files[{index}].path")
        except PublishError as error:
            raise PublishError(f"{error} (the catalog is malformed; nothing was changed)") from error
        if value in owned:
            raise PublishError(f"{where}: duplicate path {value!r}; nothing was changed")
        owned[value] = digest
    return owned


def plan_export(output: Path, staging: Path, catalog: dict, owned: dict[str, str], preserved: list[Path]) -> dict:
    """Decide every change without making any. Conflicts are collected so one run reports them all."""
    write: list[tuple[str, Path]] = []
    unchanged = 0
    conflicts: list[str] = []
    new_paths = {item["path"]: item["sha256"] for item in catalog["files"]}
    for path, digest in new_paths.items():
        target = contained(output, path, path)
        if target.is_symlink():
            conflicts.append(f"{path}: the destination is a symlink")
        elif not target.exists():
            write.append((path, target))
        elif not target.is_file():
            conflicts.append(f"{path}: the destination exists and is not a file")
        else:
            current = sha256_file(target)
            if current == digest:
                unchanged += 1
            elif path not in owned:
                conflicts.append(f"{path}: exists in the output but no earlier export wrote it (unowned file)")
            elif current != owned[path]:
                conflicts.append(f"{path}: edited in the output since the last export (hash differs from the record)")
            else:
                write.append((path, target))
    remove: list[tuple[str, Path]] = []
    for path, digest in owned.items():
        if path in new_paths:
            continue
        target = contained(output, path, path)
        if any(part in Path(path).parents or part == Path(path) for part in preserved):
            continue  # a path the manifest says is independently maintained is never removed
        if target.is_symlink():
            conflicts.append(f"{path}: a stale exported file was replaced by a symlink")
        elif target.is_file():
            if sha256_file(target) == digest:
                remove.append((path, target))
            else:
                conflicts.append(f"{path}: stale, but edited in the output since the last export; not deleting it")
    if conflicts:
        raise PublishError(
            "refusing to export; nothing was changed. Resolve these in the output repository "
            "(revert the edit, move the file, or delete it), then run the export again:\n  "
            + "\n  ".join(sorted(conflicts))
        )
    return {"write": write, "remove": remove, "unchanged": unchanged}


def publish(root: Path, output: Path, revision: str, *, dry_run: bool = False) -> dict:
    """Export into `output`, touching only files this exporter owns.

    Ownership is the previous catalog's path and hash list. Unowned files, unknown games and preserved paths are never
    deleted; an unowned or edited file where an export wants to write is a reported conflict, not overwritten.

    Guarantees: everything is planned and validated, and every new file is written to a staging directory inside the
    output, before the first change. Each file is then swapped in with an atomic rename, so no file is ever half
    written. The set of files is NOT one atomic transaction: a failure part-way leaves some files new and the rest
    old, with the catalog (written last) still describing the previous export; running the export again converges.
    """
    manifest = load_manifest(root)
    output.mkdir(parents=True, exist_ok=True)
    preserved = [safe_relative(value, "preserve") for value in manifest.get("preserve", [])]
    owned = read_ownership(output)
    with tempfile.TemporaryDirectory(prefix="games-publish-", dir=output.parent) as temp:
        staging = Path(temp)
        catalog = export_tree(root, staging, manifest, revision)
        changes = plan_export(output, staging, catalog, owned, preserved)
        catalog_target = output / CATALOG_NAME
        if catalog_target.is_symlink():
            raise PublishError(f"{CATALOG_NAME} in the output is a symlink")
        summary = {
            "created": sum(1 for _, target in changes["write"] if not target.exists()),
            "updated": sum(1 for _, target in changes["write"] if target.exists()),
            "removed": len(changes["remove"]),
            "unchanged": changes["unchanged"],
        }
        if dry_run:
            return {**catalog, "summary": summary, "dry_run": True}
        holding = Path(tempfile.mkdtemp(prefix=".games-publish-", dir=output))
        try:
            ready: list[tuple[Path, Path]] = []
            for index, (path, target) in enumerate(changes["write"]):
                held = holding / str(index)
                shutil.copy2(staging / path, held, follow_symlinks=False)
                ready.append((held, target))
            shutil.copy2(staging / CATALOG_NAME, holding / "catalog")
            # Nothing in the output has changed yet. From here each step is one atomic rename or unlink.
            for held, target in ready:
                target.parent.mkdir(parents=True, exist_ok=True)
                _replace(held, target)
            for _, target in changes["remove"]:
                target.unlink()
                parent = target.parent
                while parent != output and parent.parent != output and not any(parent.iterdir()):
                    parent.rmdir()  # only directories this export emptied, never a collection root
                    parent = parent.parent
            _replace(holding / "catalog", catalog_target)
        finally:
            shutil.rmtree(holding, ignore_errors=True)
    return {**catalog, "summary": summary}


def check(root: Path, revision: str) -> dict:
    manifest = load_manifest(root)
    with tempfile.TemporaryDirectory(prefix="games-publish-check-") as temp:
        return export_tree(root, Path(temp), manifest, revision)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("check", "export"))
    parser.add_argument("--output", type=Path, help="Games repository checkout (export only)")
    parser.add_argument("--dry-run", action="store_true", help="plan the export and report conflicts without changing anything")
    parser.add_argument("--revision", help="source Git revision recorded in the catalog")
    args = parser.parse_args()

    root = Path(__file__).resolve().parents[1]
    revision = args.revision or git_revision(root)
    try:
        if args.command == "check":
            if args.output or args.dry_run:
                raise PublishError("--output and --dry-run are only valid with export")
            catalog = check(root, revision)
        else:
            if not args.output:
                raise PublishError("export requires --output")
            catalog = publish(root, args.output.resolve(), revision, dry_run=args.dry_run)
    except PublishError as error:
        print(f"publish-games: {error}", file=sys.stderr)
        return 2

    total = sum(catalog["collections"].values())
    print(
        json.dumps(
            {
                "ok": True,
                "files": total,
                "collections": catalog["collections"],
                **({"changes": catalog["summary"], "dry_run": bool(catalog.get("dry_run"))} if "summary" in catalog else {}),
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
