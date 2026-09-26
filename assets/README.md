# BlueEngine asset library

This directory is a discoverable asset API, not a mandate to use only built-in
content. The working order is **Reuse → Modify → Generate → Import**. Start with
`python tools/assets.py search TEXT`, use a game-local pack for specialized work,
and promote only assets that prove useful beyond one game.

`assets/catalog.json` registers the shared packs. `tools/assets.py` adapts the
existing native prop and interior-prefab sources into one normalized JSON contract,
so geometry remains in its established project location and metadata does not fork
into a manually synchronized mega-index.

```sh
python tools/assets.py describe
python tools/assets.py search "small desk lamp" --limit 5
python tools/assets.py show core/native/table_lamp_1
python tools/assets.py list --pack core/interiors --tag food
python tools/assets.py validate
python tools/assets.py init-pack games/my-game/assets.json --id games/my-game --name "My Game"
python tools/assets.py promote games/my-game/assets.json crate outputs/crate-promotion.json
```

Game-local manifests follow `asset-pack.schema.json` and can be included in any
read command with `--include PATH`. A promotion command creates a review proposal;
it deliberately does not copy or overwrite shared assets. This keeps promotion an
explicit curation decision while preserving provenance and reuse evidence.

Stable IDs are qualified as `PACK/LOCAL_ID`, for example
`core/interiors/student-desk`. Unqualified local IDs and declared aliases are
accepted only when they resolve to exactly one asset.
