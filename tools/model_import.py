"""CC0-only static model authoring; Cargo owns converter freshness, packs are atomic."""
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[1]
BUDGET = 16 * 1024 * 1024
ID = re.compile(r"^[a-z0-9]+(?:[._/-][a-z0-9]+)*$")


def convert(source, scale, repair=False):
    started = time.monotonic()
    build = subprocess.run(["cargo", "build", "--locked", "--profile", "itest",
                            "--no-default-features", "--features", "model-import",
                            "--bin", "be2-model-import", "--message-format=json"], cwd=ROOT, capture_output=True, text=True)
    build_seconds = time.monotonic() - started
    if build.returncode:
        raise ValueError("model-import build failed; inspect Cargo diagnostics:\n" + build.stderr[-6000:])
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target.is_absolute():
        target = ROOT / target
    exe = target / "itest" / ("be2-model-import.exe" if os.name == "nt" else "be2-model-import")
    conversion_started = time.monotonic()
    run = subprocess.run([str(exe), str(source), str(scale), *(["--repair-degenerate"] if repair else [])], cwd=ROOT, capture_output=True, text=True)
    conversion_seconds = time.monotonic() - conversion_started
    try:
        result = json.loads(run.stdout)
    except ValueError as error:
        raise ValueError("converter failed without JSON; existing packs remain valid: " + run.stderr[-2000:]) from error
    if run.returncode or not result.get("ok"):
        raise ValueError(result.get("error", "converter failed"))
    artifacts = [json.loads(line) for line in build.stdout.splitlines() if line.startswith('{')]
    result["build"] = {"seconds": build_seconds, "fresh": sum(v.get("reason")=="compiler-artifact" and v.get("fresh",False) for v in artifacts), "built": sum(v.get("reason")=="compiler-artifact" and not v.get("fresh",False) for v in artifacts), "profile": "itest", "features": ["model-import"]}
    result["conversion_seconds"] = conversion_seconds
    return result


def import_model(args, converter=convert):
    # Check policy before invoking Cargo or touching output. License is a user assertion,
    # not a license-recognition heuristic; retained provenance makes it reviewable.
    if args.license != "CC0-1.0":
        raise ValueError("import-model accepts CC0-1.0 only; obtain permission/CC0 art or use an independently reviewed external pack")
    if not ID.fullmatch(args.id) or not ID.fullmatch(args.pack_id):
        raise ValueError("id and pack-id must be normalized catalog IDs")
    if not args.tags or any(not ID.fullmatch(tag) for tag in args.tags):
        raise ValueError("at least one normalized tag is required")
    if not args.attribution or len(args.attribution) > 500:
        raise ValueError("attribution must be 1..500 characters")
    if urlparse(args.source_url).scheme not in ("https", "http") or not urlparse(args.source_url).netloc:
        raise ValueError("source-url must be a public HTTP(S) provenance URL")
    if not math.isfinite(args.scale) or args.scale <= 0:
        raise ValueError("scale must be positive and finite")
    source = args.source.resolve(strict=True)
    output = args.output.absolute()
    if output.exists():
        raise ValueError("output already exists; choose a new pack directory (previous imports preserved)")
    if source.stat().st_size > 32 * 1024 * 1024:
        raise ValueError("source exceeds 32 MiB; split the model")
    before = hashlib.sha256(source.read_bytes()).hexdigest()
    result = converter(source, args.scale, getattr(args, "repair_degenerate", False))
    if hashlib.sha256(source.read_bytes()).hexdigest() != before:
        raise ValueError("source changed during import; retry (no pack written)")
    model = (json.dumps(result["model"], separators=(",", ":"), allow_nan=False) + "\n").encode()
    collider = (json.dumps(result["collider"], indent=2, allow_nan=False) + "\n").encode()
    c = result["collider"]
    manifest = {
        "schema_version": 1,
        "pack": {"id": args.pack_id, "name": args.pack_id, "version": "0.1.0", "scope": "game-local", "license": args.license},
        "assets": [{"id": args.id, "label": args.id.replace("-", " ").title(), "description": "Imported static CC0 prop; base-color presentation and precomputed box collider.", "type": "mesh",
                    "source": {"path": "model.json", "format": "blue-static-model-v1", "method": "imported", "attribution": args.attribution, "source_url": args.source_url, "sha256": hashlib.sha256(model).hexdigest(), "original_sha256": before},
                    "taxonomy": {"categories": ["imported"], "tags": sorted(set(args.tags)), "aliases": []},
                    "geometry": {"units": "meters", "origin": "bottom-center", "half_extents": c["half_extents"], "bounds_min": c["bounds_min"], "bounds_max": c["bounds_max"], "triangle_count": c["triangle_count"]},
                    "physics": {"mobility": "static", "collision": "aabb", "metadata": "collider.json"}, "lifecycle": {"status": "candidate"}}],
    }
    if getattr(args, "forward", None):
        manifest["assets"][0]["geometry"]["forward"] = args.forward
    provenance = {"version": 1, "license": args.license, "source_url": args.source_url, "attribution": args.attribution, "original_sha256": before, "model_sha256": hashlib.sha256(model).hexdigest(), "scale": args.scale, "warnings": result.get("warnings", [])}
    files = {"model.json": model, "collider.json": collider,
             "pack.json": (json.dumps(manifest, indent=2) + "\n").encode(),
             "provenance.json": (json.dumps(provenance, indent=2) + "\n").encode()}
    size = sum(len(value) for value in files.values())
    if size > BUDGET:
        raise ValueError("processed pack exceeds 16 MiB budget; split the asset (no output written)")
    output.parent.mkdir(parents=True, exist_ok=True)
    # Reserve destination before staging. An empty reservation cannot hide earlier work.
    output.mkdir()
    stage = Path(tempfile.mkdtemp(prefix=".model-import-", dir=output.parent))
    try:
        for name, value in files.items():
            (stage / name).write_bytes(value)
        try:
            from .assets import validate_external_manifest
        except ImportError:
            from assets import validate_external_manifest
        validate_external_manifest(stage / "pack.json", manifest)
        output.rmdir()
        stage.rename(output)
    except BaseException:
        if output.exists() and not any(output.iterdir()):
            output.rmdir()
        raise
    finally:
        if stage.exists():
            shutil.rmtree(stage)
    return {"pack": str(output / "pack.json"), "asset": args.pack_id + "/" + args.id,
            "bytes": size, "triangles": c["triangle_count"], "chunks": c["chunk_count"], "warnings": result.get("warnings", []), "build": result.get("build",{}), "conversion_seconds": result.get("conversion_seconds"),
            "next": ["python3", "tools/assets.py", "--include", str(output / "pack.json"), "search", args.id]}
