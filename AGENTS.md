# Working on BlueEngine

First run `python tools/be2.py context "<task>" --compact` in this checkout.
It needs only Python and the feature index, no build. Read the packet's selected
paths, not the whole repository. Exact feature IDs or diagnostic IDs narrow lookup;
low confidence means inspect/narrow before editing, never invent an API.
Use `be2-tools src find/outline/show` for deeper symbol navigation when needed.

For an iteration check, use the packet's `iterate` plan; select an indexed suite,
`--test SUITE::exact_test`, or `--typecheck` (library only). Choose one useful check,
not all three. Iteration success is not final verification.

After editing: `python tools/be2.py check --changed --plan` reports affected
features, uncertainty and the verification commands. Then run
`python tools/be2.py check --changed`. It includes staged/unstaged/untracked files;
use `--base REV` for committed work. Only reviewed independent Python scopes narrow
validation. Rust, manifests, content, docs, validation infrastructure and unknown
paths automatically require full checks. `check` always runs the full suite.
Feature test suggestions are for iteration; a plan is not passing evidence.
Keep full Linux/Windows CI. Inspect visuals/controls manually when they change.

Never violate:
- Authoritative simulation is shared, fixed-step and rendering-free. Clients send
  intentions; presentation must not duplicate gameplay authority.
- Respect active transport limits; queue acceptance is not acknowledgement.
- Preserve public vesper3d compatibility, semantic IDs and visual/collision agreement.
- Preserve completed outputs on failure; never shell-interpolate scene values.
- Preserve official assets/branding artwork. No plugin/service installation needed.

Game projects start with their own AGENTS.md and project check. Pick the starter by the rules
(docs/GAME_QUICKSTART.md): `GameDocument` counters/interactables/timers use the stock starter;
enemies, projectiles, scoring, AI or per-frame physics use `new-game ... custom-sim`. A game is
done only when it ships with its own icon and desktop shortcut (`scripts/blue ship`; its
`scripts/check.py` fails until it does). Saving and loading state is engine-owned (docs/SAVE_STATE.md):
F5/F9 in the stock client, `devkit::Snapshot` for a custom simulation; never hand-write save files. Authoring starts
with `python tools/author.py describe`; discover assets with
`python tools/assets.py search TEXT`. Retrieve detailed contracts through context;
see docs/ENGINE_MAINTENANCE.md only for relevant maintenance obligations.
Published-source changes also require `python scripts/publish_games.py check`.

Expect small tasks to touch 1-4 source files, subsystem work 3-8. Above 10, recheck
impact; simulation + networking + presentation may belong at a shared lower layer.
These are prompts to reconsider scope, not limits on necessary work or reading.

DONE WHEN requested behavior works, focused behavioral evidence and affected checks
pass, and public docs reflect changed public contracts. STOP. Do not refactor nearby
code, add speculative abstractions or expand scope. Tasks may override this default.
