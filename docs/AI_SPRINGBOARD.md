# Start and resume a bounded task

From a source checkout run `python3 tools/be2.py start "<objective>"`. Use `python`
on Windows. This is the shared startup entry; it reuses `context` and the existing
feature index. It needs Python 3.11+, but no compiled engine binary. It probes only
read-only toolchain/dependency information; it never installs, builds, launches or
executes checks. Metadata lives in ignored `.be2-work/tasks/`; `--no-save` on start/next/resume is entirely
read-only. `--json`/`--compact` emit schema-version-1 JSON, and `--detail` adds ownership,
full input identity, observed probes and report history. Essential constraints stay intact.

```sh
python3 tools/be2.py start "Create a small 2D collect-four game" \
  --kind new-game --project ../relic-room --template two-d --target windows --compact
python3 tools/be2.py start "Fix path sampling in the engine" \
  --kind engine --path src/viewer/devkit/path.rs --compact
python3 tools/be2.py next TASK_ID --compact
python3 tools/be2.py resume TASK_ID --note "Rules edited; inspect timeout boundary" --compact
```

The packet gives **one action**, its working directory, structured `argv`, expected
result, public references, explicit starter/runtime/targets, prerequisites, constraints
and evidence. Execute the chosen command separately. Scaffolding uses the existing
`map new-game NAME DIR ENGINE TEMPLATE` and builds fresh authoring tooling only when
that command executes. Never treat a discovered binary as fresh passing evidence.
After scaffolding, implement the objective, update `game.project.json` to the requested
targets and author behavior tests. A generated sample is not proof of the requested game.

Kinds are `new-game`, `change-game`, `engine`, `diagnose` and `upgrade`. Specify one
when inference is uncertain. Without a project, a new game uses a named ignored scratch
directory shown in the packet; publication still needs its own committed public source.
`--target` and `--constraint` repeat. Targets are never dropped. The starter catalog in
`templates/starters.json` is embedded in the native tool and read by this coordinator.
The CLI default is **portable**, the original library default remains **stock**;
always name the intended starter. Declarative native GameDocument, native custom
simulation/netplay and portable rules remain distinct. BlueEngineGames distributes native
Windows EXE installers; optional browser compatibility requires a separate destination.
Browser UDP/QUIC/native
Rapier/world APIs are unsupported combinations, with engineering routes shown. Headless
engine APIs exist even though this coordinator does not scaffold a headless application.

Tool presence is not target readiness. Rust version/toolchain, installed wasm standard
library, host C linker, Node/ws, Chromium and relevant presentation headers are checked
without compilation. Dependency-cache availability and real build success remain
unverified. Another OS's packaging/runtime gate cannot pass on this host. Setup commands
such as `web prepare` are recommendations to run explicitly, never automatic discovery.

## Observe checks, do not certify notes

Add `--task TASK_ID` to the canonical checker, for example:

```sh
python3 tools/be2.py check --game ../relic-room --loop inner --task TASK_ID
python3 tools/be2.py check --game ../relic-room --loop integration --task TASK_ID
python3 tools/be2.py check --game ../relic-room --loop shipping --task TASK_ID
# Engine task: use its packet's --path/--changed selection, then integration and final checks.
```

`next` and `resume` recompute progress from current files and observed reports, without
rerunning a command. Reports record current engine source content, game source/assets/
configuration using the existing browser hash, lock, environment/configuration identity,
tool/executable identity and package output identity. Before/after source changes during
a check invalidate evidence. The existing first-use metadata step may legitimately update
only the game lock before locked tests. An engine Git commit alone cannot preserve evidence.
Binary/package replacement, changed source/assets/configuration or missing logs invalidate
prior passes. Full logs/failure records remain in their original check directories.

Planned checks and passed, failed, skipped or unverified evidence are distinct. Schema
validity is not behavior. An inner pass is not shipping. Notes are context only; checks
without `--task`, legacy reports without input binding and delegated tool results are not
promoted to coordinated passing evidence. No independent verification cache exists.
The coordinator stages portable new games and focused engine changes. Existing native
games, diagnosis and upgrades delegate to their established tools. Existing portable
games reuse the same project-check stages. Advanced custom code,
clients, shaders, physics and networking remain available directly.

A current shipping report proves its recorded machine scope. Objective review, changed
visuals/controls, other declared platforms, Linux/Windows engine CI, physical-device/audio
limits and public-source/deployment obligations remain explicit. The coordinator does not
mark the overall creative task complete merely because tests passed.
