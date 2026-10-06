# Authoring schema maintenance

Acceptance criteria for this milestone:

- Existing valid GameDocument, patch and asset-pack fixtures remain valid;
  existing numeric, identity, collection and strict-object constraints remain.
- Nullable presentation and rule fields keep their v1 behavior.
- Published `$defs` fragments retain their v1 names and constraints. The asset
  `vec3` fragment keeps its older permissive cardinality; pack fields require
  the three components already required by runtime validation.
- A structurally valid objective that references an undeclared counter still
  fails through the production GameDocument validator with a repairable diagnostic.
- Existing stock-game interaction, completion, restart/save and multiplayer
  behavior pass their normal suites. Imported props retain model/collider checks.
- Every committed schema equals generation, including all thirteen MCP input
  shapes. Plans build nothing; failed/stale generation preserves prior outputs.
- Full engine verification and existing Linux/Windows/browser CI remain required.

Game authors use the committed schemas, `game-validate`, asset validation and
their scenario/project checks. They do not need the schema generator or Rust
implementation context.

Engine maintainers change the owning Rust Serde type and, for bounded fields,
its constraint hook in `src/authoring_schemas.rs` or
`src/authoring_schemas/assets.rs`. Then run:

```sh
python3 tools/be2.py schemas --write --plan
python3 tools/be2.py schemas --write
python3 tools/be2.py schemas --check
python3 tools/be2.py check
```

`--plan` runs no executable, probe, installation or build. `--write` builds
only the native, rendering-free `be2-tools` generator with `schema-generation`.
It obtains the executable from Cargo's artifact report, checks source identity
before replacing outputs, parses all four results before writing, replaces
each changed file atomically and leaves identical files untouched. Normal
Cargo freshness is the build cache. `--check` runs the focused Rust parity and
fixture suite with `schema-validation`. Both operations return one JSON packet
and retain command output in the reported ignored log file. Missing Cargo,
failed generation, changed inputs and cross-target execution failures remain
failures. Use a native host Cargo target for this maintenance operation.

The native backend is `be2-tools schema-generate KIND`, with `game`, `patch`,
`asset-pack` or `mcp`. Use the canonical front door above to select the fresh
feature-built executable; ordinary authoring binaries do not enable generation.

| Output | Shape owner | Bounds and other v1 constraints |
|---|---|---|
| `tools/game.schema.json` | `viewer::game::GameDocument` and its existing nested types | Rust hooks beside the generator, using existing gameplay limits |
| `tools/patch.schema.json` | `viewer::authoring::Edit`, `viewer::props::PropKind` | Patch hook: identities, coordinates, colors, transaction size |
| `assets/asset-pack.schema.json` | Typed asset-pack authoring contract in `src/authoring_schemas/assets.rs` | Local hooks: provenance, taxonomy, vector shape, license metadata |
| `tools/mcp.schemas.json` | Typed argument structs in `src/viewer/mcp.rs` | Existing optional argument nullability; native unsigned count/seed range |

Do not hand-edit generated JSON. MCP keeps its existing transport, tool names,
descriptions, dispatch and direct toolkit access; regular builds embed the
generated input schemas without compiling the generator or validator.
Asset adapters and Python validation still own catalog loading and semantic
checks. General packs retain their existing license flexibility; model import
continues to require CC0 by default.

Schema validity is only structural evidence. References, byte budgets, condition
depth, profile relationships, map paths, geometry lint, gameplay behavior,
visual/input checks and packaging require the existing validators and gates.
The full checker adds generation parity, maintained valid fixtures, invalid
authoring cases and strict linting; no existing check is removed.

Generation uses pinned `schemars 1.2.2`, Draft 2020-12, and Rust fixtures use
`jsonschema 0.34.0` without HTTP/file resolution features. The newer validator
did not compile with the engine's pinned `serde_json`; engine pins were preserved.
Both dependencies are native-only and opt-in. Native/browser games and headless
servers keep their existing default dependency graph and fixed-step authority.
