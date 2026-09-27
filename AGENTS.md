# Working on BlueEngine

Start with the smallest relevant contract; do not preload architecture documents or
engine source. BlueEngine is a dependency for game projects, not their working context.

| Task | Start here | Validation |
|---|---|---|
| Game project | Its local AGENTS.md and game files | Its project check; engine tests only if engine changes |
| Map/assets/GameDocument | `python tools/author.py describe`, tools/AUTHORING.md | Native audit/verify, relevant routes/scenarios and visual review |
| Find an engine capability | `python tools/be2.py context QUERY` | Returns at most 3 feature records without Cargo or source reads |
| Engine/tool maintenance | Matching context record, then docs/ENGINE_MAINTENANCE.md | `python tools/be2.py check --changed --plan`, then `check --changed` |

`context` searches the existing tools/FEATURES.json; use exact feature IDs to narrow
results. No match is not an API. Read only relevant contracts, implementation and tests.
For changed source, use BE2_ARCHITECTURE.md; historical Blue/Vesper architecture is
needed only for those subsystems. The maintenance guide retains all engine invariants.

`check` without flags always runs the full suite. `--changed` includes staged,
unstaged and untracked files against HEAD; use `--base REV` for committed changes.
Only reviewed independent Python edits get narrower checks. Rust, content, manifests,
docs, validation infrastructure and unknown paths keep full validation. Plans describe
scope, not proof of success; no-change plans do not certify the baseline. CI keeps the
full Linux/Windows gates. Run required checks once on final inputs; repeat for new changes
or failures, not because another entry point repeats the same instructions.

Preserve completed outputs on failure; never shell-interpolate scene values. Keep
semantic IDs stable and visual/collision/entity edits consistent. Discover reusable
assets with `python tools/assets.py search TEXT` before modifying/generating/importing.
Unsupported gameplay requirements are engine work; do not invent APIs.

Playable games follow docs/GAME_PRESENTATION.md (shared shell, fixed-step movement,
minimal HUD, cached rendering and release-build playtests). Read docs/SHARED_GAMEPLAY.md
when extending shared controls or custom loops. Preserve official branding in
assets/branding; never regenerate it as routine game/engine work.

Run `python scripts/publish_games.py check` after changing games-publish.json or a
published source. See docs/GAMES_PUBLISHING.md. Keep scratch files, build output,
logs and credentials out of publication. No plugin/service installation is needed.
