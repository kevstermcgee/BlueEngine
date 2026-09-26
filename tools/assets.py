#!/usr/bin/env python3
"""BlueEngine's JSON asset catalog API. Python 3.10+, standard library only."""

import argparse
import copy
import json
import math
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
REGISTRY = ROOT / "assets" / "catalog.json"
API_VERSION = 1
ID_RE = re.compile(r"^[a-z0-9]+(?:[._/-][a-z0-9]+)*$")

CATEGORY_TERMS = {
    "seating": {"chair", "bench", "sofa", "seat"},
    "surfaces": {"desk", "table", "shelf", "bookcase", "cubby", "stand"},
    "lighting": {"lamp", "light", "candle"},
    "food-and-drink": {"apple", "bread", "carrot", "cereal", "coffee", "food", "fruit", "mug", "orange", "pear", "snack"},
    "storage": {"backpack", "basket", "bin", "cabinet", "carton", "locker", "storage"},
    "decor": {"art", "botanical", "clock", "decor", "flower", "painting", "plant", "poster", "print", "sculpture", "trophy", "vase"},
    "architecture": {"board", "door", "wall", "window"},
    "office": {"computer", "file", "office", "papers", "pencil", "printer"},
    "education": {"blackboard", "math", "reading", "school", "student"},
    "bathroom": {"bathroom", "dispenser", "sink", "soap", "toilet"},
    "retail": {"market", "produce", "register", "retail", "shopping"},
    "appliances": {"coffee-machine", "computer", "dispenser", "machine", "microwave", "printer"},
}

ALIASES = {
    "sofa": ["couch", "loveseat"],
    "waste-bin": ["trash can", "garbage bin"],
    "bookcase": ["bookshelf"],
    "table-lamp": ["desk lamp"],
    "framed-art": ["wall art", "picture"],
    "closed-double-door": ["double doors", "entrance"],
    "student-desk": ["school desk"],
    "register": ["checkout", "cash register"],
}


def read(path):
    return json.loads(Path(path).read_text(encoding="utf-8-sig"))


def write_new(path, value):
    payload = json.dumps(value, indent=2, ensure_ascii=False, allow_nan=False) + "\n"
    with Path(path).open("x", encoding="utf-8") as stream:
        stream.write(payload)


def tokens(value):
    return set(re.findall(r"[a-z0-9]+", value.lower()))


def taxonomy(asset_id, label, supplied=()):
    base = tokens(asset_id + " " + label + " " + " ".join(supplied))
    categories = sorted(name for name, terms in CATEGORY_TERMS.items() if base & tokens(" ".join(terms)))
    if not categories:
        categories = ["general"]
    aliases = list(ALIASES.get(asset_id, []))
    return {
        "categories": categories,
        "tags": sorted(base | set(supplied) | {"prefab"}),
        "aliases": aliases,
    }


def qualify(pack_id, local_id):
    return f"{pack_id}/{local_id}"


def adapt_native(pack, source):
    data = read(source)
    records = []
    for item in data["assets"]:
        dimensions = item.get("dimensions_m")
        half = [round(value / 2, 6) for value in dimensions] if dimensions else None
        native = item["native_kind"]
        tax = taxonomy(native, item["label"], item.get("tags", []))
        tax["aliases"] = sorted(set(tax["aliases"] + [item["id"], native, Path(item["scene"]).stem]))
        records.append({
            "id": qualify(pack["id"], item["id"]),
            "local_id": item["id"],
            "pack": pack["id"],
            "label": item["label"],
            "description": f"Reusable native {item['label'].lower()} prop with engine-authored geometry.",
            "type": "prefab",
            "source": {"path": item["scene"], "format": "vesper-scene-v1", "method": "generated"},
            "taxonomy": tax,
            "geometry": {"units": "meters", "origin": "bottom-center", "half_extents": half,
                         **({"forward": "+z"} if "+Z" in item.get("origin", "") else {})},
            "physics": {"mobility": "runtime-policy", "collision": "conservative-bounds"},
            "placement": {"api": "add_prop", "kind": native, "rotation_y_degrees": [0]},
            "compatibility": {"map_document": 1, "client": True, "headless": True},
            "provenance": {"project": "BlueEngine", "license": pack["license"]},
            "lifecycle": {"status": pack["stability"]},
        })
    return records


def adapt_interiors(pack, source):
    records = []
    for item in read(source):
        local_id = item["id"]
        tax = taxonomy(local_id, item["label"], ["interior"])
        tax["aliases"] = sorted(set(tax["aliases"] + [Path(item["scene"]).stem]))
        records.append({
            "id": qualify(pack["id"], local_id),
            "local_id": local_id,
            "pack": pack["id"],
            "label": item["label"],
            "description": f"Reusable data-only {item['label'].lower()} interior prefab.",
            "type": "prefab",
            "source": {"path": f"assets/props/interiors/{item['scene']}", "format": "vesper-scene-v1", "method": "generated"},
            "taxonomy": tax,
            "geometry": {"units": "meters", "origin": "bottom-center", "half_extents": item["half_extents"]},
            "physics": {"mobility": "static", "collision": "aabb"},
            "placement": {"api": "place_interior", "rotation_y_degrees": [0, 90, 180, 270]},
            "compatibility": {"map_document": 1, "client": True, "headless": True},
            "provenance": {"project": "BlueEngine", "license": pack["license"]},
            "lifecycle": {"status": pack["stability"]},
        })
    return records


def require(condition, message):
    if not condition:
        raise ValueError(message)


def validate_external_manifest(path, data):
    require(data.get("schema_version") == 1, f"{path}: schema_version must be 1")
    require(set(data) <= {"$schema", "schema_version", "pack", "assets"}, f"{path}: unknown top-level field")
    pack = data.get("pack")
    require(isinstance(pack, dict), f"{path}: pack must be an object")
    for key in ("id", "name", "version", "scope", "license"):
        require(pack.get(key), f"{path}: pack.{key} is required")
    require(ID_RE.fullmatch(pack["id"]) is not None, f"{path}: invalid pack.id")
    require(pack["scope"] in ("game-local", "shared"), f"{path}: invalid pack.scope")
    require(re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", pack["version"]) is not None,
            f"{path}: pack.version must be semantic x.y.z")
    assets = data.get("assets")
    require(isinstance(assets, list), f"{path}: assets must be an array")
    seen = set()
    base = path.resolve().parent
    required = {"id", "label", "description", "type", "source", "taxonomy", "geometry", "physics", "lifecycle"}
    for index, asset in enumerate(assets):
        prefix = f"{path}: assets[{index}]"
        require(isinstance(asset, dict), f"{prefix} must be an object")
        require(required <= set(asset), f"{prefix} missing {sorted(required - set(asset))}")
        require(ID_RE.fullmatch(asset["id"]) is not None, f"{prefix}.id is invalid")
        require(asset["id"] not in seen, f"{prefix}.id is duplicated")
        seen.add(asset["id"])
        require(asset["type"] in ("prefab", "mesh", "material", "texture", "audio", "animation"),
                f"{prefix}.type is invalid")
        require(isinstance(asset["label"], str) and asset["label"] and isinstance(asset["description"], str) and asset["description"],
                f"{prefix} requires a non-empty label and description")
        source = asset["source"]
        require(isinstance(source.get("format"), str) and source["format"], f"{prefix}.source.format is required")
        require(source.get("method") in ("reused", "modified", "generated", "imported"),
                f"{prefix}.source.method is invalid")
        require(source.get("path") and not Path(source["path"]).is_absolute(),
                f"{prefix}.source.path must be relative")
        resolved = (base / source["path"]).resolve()
        require(resolved == base or base in resolved.parents, f"{prefix}.source.path escapes its pack")
        require(resolved.is_file(), f"{prefix}.source.path does not exist: {source['path']}")
        if source["method"] == "imported":
            require(source.get("attribution") and source.get("source_url"),
                    f"{prefix}: imported assets require attribution and source_url")
        tax = asset["taxonomy"]
        require(tax.get("categories") and isinstance(tax.get("tags"), list) and isinstance(tax.get("aliases"), list),
                f"{prefix}.taxonomy requires categories, tags and aliases")
        require(all(isinstance(value, str) and ID_RE.fullmatch(value) for value in tax["categories"] + tax["tags"]),
                f"{prefix}.taxonomy categories and tags must be normalized IDs")
        geometry = asset["geometry"]
        require(geometry.get("units") == "meters" and geometry.get("origin") in ("bottom-center", "center", "custom"),
                f"{prefix}.geometry requires supported units and origin")
        half = geometry.get("half_extents")
        require(isinstance(half, list) and len(half) == 3 and all(isinstance(v, (int, float)) and math.isfinite(v) and v > 0 for v in half),
                f"{prefix}.geometry.half_extents requires three positive finite numbers")
        physics = asset["physics"]
        require(physics.get("mobility") in ("static", "dynamic", "runtime-policy", "none") and
                physics.get("collision") in ("aabb", "compound", "conservative-bounds", "mesh", "none"),
                f"{prefix}.physics is invalid")
        require(asset["lifecycle"].get("status") in ("experimental", "candidate", "stable", "deprecated"),
                f"{prefix}.lifecycle.status is invalid")
    return pack, assets


def adapt_external(path):
    data = read(path)
    pack, assets = validate_external_manifest(path, data)
    records = []
    for item in assets:
        record = copy.deepcopy(item)
        record["local_id"] = item["id"]
        record["id"] = qualify(pack["id"], item["id"])
        record["pack"] = pack["id"]
        record["source"]["path"] = str((path.resolve().parent / item["source"]["path"]).resolve())
        record["provenance"] = {"project": pack["name"], "license": pack["license"]}
        records.append(record)
    return {**pack, "stability": "mixed", "source": str(path)}, records


def load_catalog(includes=()):
    registry = read(REGISTRY)
    packs = []
    records = []
    adapters = {"authoring-assets-v1": adapt_native, "interior-prefabs-v1": adapt_interiors}
    for pack in registry["packs"]:
        source = ROOT / pack["source"]
        require(source.is_file(), f"Missing pack source: {pack['source']}")
        packs.append(pack)
        records.extend(adapters[pack["adapter"]](pack, source))
    for include in includes:
        pack, extra = adapt_external(Path(include))
        require(all(existing["id"] != pack["id"] for existing in packs), f"Duplicate pack ID: {pack['id']}")
        packs.append(pack)
        records.extend(extra)
    ids = [item["id"] for item in records]
    require(len(ids) == len(set(ids)), "Duplicate qualified asset ID")
    return registry, packs, records


def resolve(records, value):
    exact = [item for item in records if item["id"] == value]
    if exact:
        return exact[0]
    matches = []
    for item in records:
        names = [item["local_id"], *item["taxonomy"]["aliases"]]
        if value in names:
            matches.append(item)
    require(matches, f"Unknown asset '{value}'; use search")
    require(len(matches) == 1, f"Ambiguous asset '{value}': {', '.join(item['id'] for item in matches)}")
    return matches[0]


def filter_records(records, pack=None, tag=None, asset_type=None, status=None):
    result = []
    for item in records:
        searchable_tags = set(item["taxonomy"]["tags"] + item["taxonomy"]["categories"])
        if pack and item["pack"] != pack:
            continue
        if tag and tag not in searchable_tags:
            continue
        if asset_type and item["type"] != asset_type:
            continue
        if status and item["lifecycle"]["status"] != status:
            continue
        result.append(item)
    return result


def search(records, query, limit, **filters):
    terms = tokens(query)
    require(terms, "Search query must contain letters or numbers")
    hits = []
    for item in filter_records(records, **filters):
        identity = tokens(item["id"] + " " + item["local_id"] + " " + item["label"])
        tags = set(item["taxonomy"]["tags"] + item["taxonomy"]["categories"])
        aliases = tokens(" ".join(item["taxonomy"]["aliases"]))
        prose = tokens(item["description"])
        score = 8 * len(terms & identity) + 5 * len(terms & aliases) + 3 * len(terms & tags) + len(terms & prose)
        phrase = query.lower() in (item["label"] + " " + " ".join(item["taxonomy"]["aliases"])).lower()
        if score or phrase:
            hits.append((score + (10 if phrase else 0), item))
    hits.sort(key=lambda pair: (-pair[0], pair[1]["id"]))
    return [{"score": score, **item} for score, item in hits[:limit]], len(hits)


def validate_catalog(packs, records):
    issues = []
    for item in records:
        half = item["geometry"].get("half_extents")
        if not (isinstance(half, list) and len(half) == 3 and all(isinstance(v, (int, float)) and math.isfinite(v) and v > 0 for v in half)):
            issues.append({"id": item["id"], "field": "geometry.half_extents", "error": "expected three positive finite numbers"})
        path = Path(item["source"]["path"])
        if not path.is_absolute():
            path = ROOT / path
        if not path.is_file():
            issues.append({"id": item["id"], "field": "source.path", "error": "file does not exist"})
        elif path.suffix.lower() == ".json":
            try:
                read(path)
            except (OSError, ValueError) as error:
                issues.append({"id": item["id"], "field": "source.path", "error": str(error)})
    return {"valid": not issues, "pack_count": len(packs), "asset_count": len(records), "issues": issues}


def init_pack(path, pack_id, name, license_name):
    require(ID_RE.fullmatch(pack_id) is not None, "Pack ID must be lowercase path-like text")
    value = {
        "$schema": "https://blueengine.dev/schemas/asset-pack-v1.json",
        "schema_version": 1,
        "pack": {"id": pack_id, "name": name, "version": "0.1.0", "description": "Game-local assets proven through development.", "scope": "game-local", "license": license_name},
        "assets": [],
    }
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    write_new(path, value)
    return value


def promotion(manifest, asset_id, output):
    pack, records = adapt_external(Path(manifest))
    require(pack["scope"] == "game-local", "Only game-local assets need promotion")
    asset = resolve(records, asset_id)
    asset["source"]["path"] = str(Path(asset["source"]["path"]).resolve().relative_to(Path(manifest).resolve().parent)).replace("\\", "/")
    proposal = {
        "proposal_version": 1,
        "status": "review-required",
        "source_pack": pack["id"],
        "asset": asset,
        "suggested_shared_id": f"community/{asset['local_id']}",
        "review": {
            "reuse_evidence": [],
            "checks": [
                "Used by at least two games or is a broadly useful primitive",
                "Visual and collision behavior reviewed in the client",
                "License and attribution verified",
                "Stable name, origin, dimensions, aliases and tags reviewed",
                "No game-specific behavior or naming leaked into the shared asset",
            ],
        },
        "note": "This is a curation proposal. Shared files and registries were not modified.",
    }
    write_new(output, proposal)
    return proposal


def add_filters(parser):
    parser.add_argument("--pack")
    parser.add_argument("--tag")
    parser.add_argument("--type", dest="asset_type")
    parser.add_argument("--status")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--include", action="append", default=[], metavar="PACK_JSON",
                        help="include a game-local or external asset pack")
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("describe")
    listing = commands.add_parser("list"); add_filters(listing)
    finding = commands.add_parser("search"); finding.add_argument("query"); finding.add_argument("--limit", type=int, default=10); add_filters(finding)
    showing = commands.add_parser("show"); showing.add_argument("id")
    commands.add_parser("validate")
    schema = commands.add_parser("schema"); schema.add_argument("kind", choices=["pack"])
    initialize = commands.add_parser("init-pack")
    initialize.add_argument("path", type=Path); initialize.add_argument("--id", required=True); initialize.add_argument("--name", required=True); initialize.add_argument("--license", default="Proprietary")
    promote = commands.add_parser("promote")
    promote.add_argument("manifest", type=Path); promote.add_argument("asset"); promote.add_argument("output", type=Path)
    args = parser.parse_args()

    if args.command == "schema":
        return {"schema": read(ROOT / "assets" / "asset-pack.schema.json")}
    if args.command == "init-pack":
        return {"path": str(args.path.resolve()), "manifest": init_pack(args.path, args.id, args.name, args.license)}
    if args.command == "promote":
        return {"path": str(args.output.resolve()), "proposal": promotion(args.manifest, args.asset, args.output)}

    registry, packs, records = load_catalog(args.include)
    if args.command == "describe":
        return {
            "catalog": {"id": registry["id"], "name": registry["name"], "schema_version": registry["schema_version"]},
            "policy": registry["policy"],
            "counts": {"packs": len(packs), "assets": len(records)},
            "packs": [{key: pack[key] for key in ("id", "name", "scope") if key in pack} for pack in packs],
            "commands": ["describe", "list", "search", "show", "validate", "schema", "init-pack", "promote"],
            "id_contract": "PACK/LOCAL_ID; unqualified IDs and aliases must resolve uniquely",
        }
    if args.command == "list":
        matches = filter_records(records, args.pack, args.tag, args.asset_type, args.status)
        return {"assets": matches, "count": len(matches)}
    if args.command == "search":
        require(1 <= args.limit <= 50, "limit must be 1..50")
        matches, total = search(records, args.query, args.limit, pack=args.pack, tag=args.tag,
                                asset_type=args.asset_type, status=args.status)
        return {"matches": matches, "returned": len(matches), "total": total, "method": "local weighted metadata search"}
    if args.command == "show":
        return {"asset": resolve(records, args.id)}
    if args.command == "validate":
        report = validate_catalog(packs, records)
        return report
    raise ValueError("Unknown command")


if __name__ == "__main__":
    try:
        result = main()
        ok = result.get("valid", True)
        print(json.dumps({"api_version": API_VERSION, "ok": ok, **result}, ensure_ascii=False, allow_nan=False))
        raise SystemExit(0 if ok else 1)
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(json.dumps({"api_version": API_VERSION, "ok": False, "error": str(error)}, ensure_ascii=False))
        raise SystemExit(1)
