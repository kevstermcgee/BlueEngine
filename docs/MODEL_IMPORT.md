# Static CC0 model import

Find art before writing geometry: `python3 tools/assets.py search furniture`.
The built-in CC0 fixture catalog contains a chair, table, floor lamp and textured test cube.
Existing procedural templates and custom shaders remain available for novel art.
Use `search --compact` for small packets and `show ID` for complete metadata. The reviewed
CC0 chair faces +Z (back at -Z); rotate it toward the table without reading vertex JSON.
Generic imports accept `--forward` when the source direction has been reviewed; otherwise
it remains unknown. Use a preview to review an unknown facing direction.

Fetch a checksum-pinned small selection into a game/external directory:

```sh
python3 tools/assets.py fetch-models --output /path/to/game/art/source/furniture
python3 tools/assets.py import-model /path/to/game/art/source/furniture/chair.glb \
  --output /path/to/game/art/chair --id chair --pack-id games/my-art \
  --source-url https://kenney.nl/assets/furniture-kit \
  --attribution 'Kenney, Furniture Kit 2.0, CC0-1.0' \
  --tag furniture --tag chair --scale 2.5 --repair-degenerate
python3 tools/assets.py --include /path/to/game/art/chair/pack.json search chair
```

`import-model` builds only the optional authoring converter with Cargo's normal freshness
checks. CC0-1.0 is the default and only accepted import license. This is your reviewed
license assertion, not automatic license recognition. Keep the upstream license with
the art. Other existing catalog licenses are unchanged. Output must be a new directory;
failure preserves completed packs. `pack.json` records provenance, hashes, exact bounds,
box collider metadata, tags and triangle count. `provenance.json` retains repair warnings.
Never edit generated model data; reimport into a new directory and review the difference.

Use the same embedded data in native builds, with the existing portable
feature (or native presentation). No glTF importer, image codec or file access is needed:

```rust,ignore
use vesper3d::asset_model::draw::{Model, Mat4, Vec3};
// Upload once during graphics initialization, then retain Model for subsequent frames.
let chair = Model::from_json(include_bytes!("../art/chair/model.json"))?;
for mesh in chair.meshes(Mat4::from_translation(Vec3::new(2.0, 0.0, 1.0)))? {
    world.mesh(mesh);
}
```

`Model::meshes` also accepts rotation and nonuniform scale, transforms normals correctly,
and reverses winding for mirrored instances. Its ordinary macroquad meshes retain true
UVs for custom shaders. Do not send textured meshes through procedural kit `Template`:
that kit intentionally uses UV.x for glow. Default portable presentation uses the existing
unlit color × texture pipeline; full PBR lighting is outside this importer.

Headless authority reads **only** `ColliderMetadata::from_json` on `collider.json`.
It contains `center` and positive `half_extents` for a conservative box, without mesh
or texture decoding. Apply the same instance transform to visuals and collision; rotated
boxes use an oriented box in physics. Flat art gets 1 mm minimum collider half extent.
Import adds no gameplay/collision authority, network behavior or scripting language.

The converter flattens the default glTF scene (first scene if none is designated),
including node transforms. It imports static triangle lists, vertex colors, base-color
factor and opaque PNG/JPEG texture on TEXCOORD_0. KHR_materials_unlit is supported.
Unsupported skins/animation/morphs, sparse accessors, other extensions, alpha modes,
double-sided materials and non-triangle primitives fail with an export/custom-adapter
route. UVs must stay in one tile; bake repeating UVs or use a custom repeat shader.
A sampler border preserves clamp/repeat/mirror filtering within that tile. Mipmapping
and differing min/mag filtering require a custom material. Additional PBR maps produce
an explicit base-color-only warning rather than a claim of full glTF fidelity.

All imported geometry runs through existing `kit::lint`, including across materials.
By default any finding, including an exhausted lint budget, rejects the import.
`--repair-degenerate` explicitly removes only zero-area findings, records their original
triangle IDs, and reruns the full lint; other findings still fail. Kenney's selected chair
and table need this repair. Nothing is silently dropped to fit draw limits: validated
meshes split below 9,000 vertices and 5,000 indices, safe with both default portable
buffers and the native kit's larger configured buffers; over 65,535 vertices
in the complete lint input is rejected with instructions to split the source asset.

Budgets: 32 MiB aggregate input, 1024 px texture dimensions, 16 MiB processed pack.
The fetch selection is capped at 256 KiB; the full archive stays outside the repository.
Only small CC0 fixtures are committed. Fetch checks archive and selected member SHA-256,
never extracts arbitrary paths, and records a receipt. No timestamps certify freshness.

Verify edits with `python3 tools/be2.py check --changed --loop inner --plan` and the
selected checks; final engine changes require `python3 tools/be2.py check` and both
Linux/Windows CI. Games retain native/package checks and manual art review.
