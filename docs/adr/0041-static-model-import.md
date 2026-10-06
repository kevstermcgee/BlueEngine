# ADR 0041: Optional static glTF authoring and a CC0 catalog

Status: implemented; measurements and final verification in [the model import report](../perf/MODEL_IMPORT_REPORT.md).

The existing asset catalog had an import policy but no model importer. Deadfall's
feedback attributed 67% of its approximately 2.4M development tokens to primitive-based
art. That historical observation motivates a benchmark; it is not a measured saving here.
The old 9,000-vertex Template trap is already fixed by the current batcher and is preserved.

Use pure-Rust gltf 1.4.1 behind a nondefault native authoring feature. Manual bounded
resource loading avoids implicit remote/file traversal and the convenience importer's
second image dependency. Preprocessed JSON is renderer-neutral; the existing portable
mesh presentation supports native/browser, while servers use separate collider metadata.
Preserve kit glow UVs and full custom-material access rather than changing their contract.

The existing assets.py remains the catalog front door. Imports are CC0-only, atomic new
packs, with provenance hashes, bounds, conservative box, taxonomy and triangle counts.
Checksum fetching selects a tiny reviewed subset of the official
[Kenney Furniture Kit](https://kenney.nl/assets/furniture-kit), whose license is CC0.
Do not commit full packs or introduce an independent cache/index/gameplay system.

Run full kit::lint before chunking, reject unresolved or truncated findings, and make
degenerate repair an explicit recorded action followed by another full lint. The stock
Kenney models demonstrate why this explicit repair is needed. Large validated geometry
splits without triangle loss; beyond the full-lint u16 budget the converter rejects it.
Unsupported glTF features give an export/custom presentation route rather than silently
simplifying the game. This pass deliberately does not implement skeletal animation/PBR.

Use existing Cargo invalidation on every authoring invocation. No importer/codec feature
is required for games, browser rendering or headless collision use. Full verification adds
importer tests and warning-free Clippy on Linux and Windows; existing gates remain.

Acceptance compares three fresh procedural scene-authoring trials with three fresh
imported-art trials, serially compiles/renders their outputs, and records context/files,
commands, authored bytes and wall time. Token usage is reported only if actually available.
Cold converter setup and warm imports are separated from authoring time. No projection
from a tiny furnished scene to full-game completion or smaller-model performance is valid.
