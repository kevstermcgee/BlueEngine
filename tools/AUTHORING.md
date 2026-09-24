# Source-free BE2 authoring

Run `python tools/author.py describe` from the BE2 repository. This is the short entry point for content agents. Python 3.10+ and the packaged binaries in `bin/` are sufficient; no build, external service, embedding model or source inspection is needed. Commands emit one JSON envelope on stdout with `protocol_version`, `ok`, and results. Failures exit 1; argparse help is human-readable. Paths are relative to the caller's working directory. Use `--at=-2,0,-9` for negative coordinates.

## Example session

Create the `edits` directory first. Every destination must be new.

```sh
python tools/author.py describe
python tools/author.py query "chair furniture"
python tools/author.py new edits/house.json
python tools/author.py add edits/house.json garden_planter edits/garden.json --id garden-snack --at=-1.8,0,-11.2
python tools/author.py map select edits/garden.json garden-snack
python tools/author.py map diff edits/house.json edits/garden.json
python tools/author.py verify edits/garden.json edits/review --route tools/examples/upstairs.route.json --route tools/examples/garden.route.json --capture
```

Inspect the PNGs under `edits/review/captures`, including the menu. Launch `bin/BE2.exe --map edits/garden.json` to use the document. Without `--map`, the engine still loads its built-in house.

`assets` lists seventeen available props with stable catalogue IDs. `recipes` lists three composed patches and their dependencies. `add` accepts either ID, prefixes every child ID with the instance ID, translates the entire template and delegates the transaction to native `apply`. Objects are expanded into ordinary map components; there are no live prefab links or automatic updates. Rotation, scaling and custom behaviors are outside this interface.

`map select MAP INSTANCE` returns the actual component lists for a prefab instance, including slash-prefixed descendants. Use those lists with `map apply` and `schema patch` to translate or remove the entire object. Do not infer ownership from proximity. Native audit/apply remain the validation authority, including unknown fields, occupied IDs, bounds and schema version.

## Discovery and context budget

`describe` combines the native catalog with curated units, supported operations and explicit limitations. It includes binary and metadata hashes. This is capability discovery, not full Rust reflection. `query` searches a small curated index of assets, recipes and topics, returning five hits by default and at most twenty. It reads neither engine source nor repository-wide files. It uses local token matching, not embeddings or a semantic model. A missing result does not authorize inventing a capability.

`tools/authoring.json` is the author-facing knowledge/dependency index. `tools/FEATURES.json` remains a separate maintenance map to engine files and checks. A source-symbol database has been deferred: it would help engine maintenance, but is unnecessary for this bounded content interface and would need reliable parsing and invalidation. Never substitute a regex index for authoritative Rust dependency analysis.

## Executable feedback

`verify` writes a persistent `report.json` even when auditing, a route or capture fails. Pass repeatable `--route FILE` arguments; no routes are assumed when omitted. `--capture` runs the packaged graphical client with its existing twelve-camera house tour, including the menu, checks expected artifacts and records their SHA256 hashes. It requires a working graphical desktop and has a 60-second timeout. It does not rebuild the client. Custom maps outside the house camera tour need engine support for configurable cameras.

Use `--capture --baseline PREVIOUS/report.json` to compare with an explicitly selected, successful previous report. Exact image matches with the same client report `identical_to_baseline`; changed images or a different client report `review_required`. This detects byte changes, not perceptual similarity, and can flag nondeterministic rendering. `ok` means the requested executable checks completed; it does **not** mean visual approval. Even identical images do not establish that the baseline was good. Map, route and binary hashes provide provenance. Review images directly when visual quality matters.

## Boundaries and maintenance

Map v1 supports static primitive geometry, collision and inspect entities. It cannot express new gameplay, network transport, Prop Hunt rounds, disguise mechanics, imported meshes or animation. Building those features requires separate engine development. The source-free objective is met for the supported content workflow, not arbitrary game creation.

Keep the packaged binaries current using the existing build/package workflow after engine changes. `describe` checks catalogue compatibility but does not prove the binary matches the working source. For metadata/recipe changes run `python -m unittest discover -s tools -p test_author.py -v`. For engine changes run all existing Rust checks as directed in AGENTS.md. Add catalogue entries only for real instantiable assets; do not publish imagined variants. Update the index and add native integration coverage with each new recipe or command.

## Starter maps

Office, convenience store and school wing JSON maps are available in assets/maps/starters, with native validation reports and per-room/aisle controller routes. Load with `bin/BE2.exe --map assets/maps/starters/office.json`; use these documents directly as authoring inputs. See the directory README for scope and checks. The native `new` command still exports the house.

## Interior furnishing prefabs

Use `python tools/place_interior.py --list` for 53 additional data-only templates (furniture, clutter, written boards and closed doors). Place with `python tools/place_interior.py MAP ASSET OUTPUT --id ID --at=X,Y,Z --yaw 90`. The native audit validates the result before a new file is created. This separate helper supports quarter turns and conservative inspection/collision bounds; these are not extra native add_prop kinds. See assets/props/interiors/README.md.
