# Native installer and device review

Project entry points use their pinned engine tooling. Upgrade reviewed older
launchers with `python ENGINE/tools/game_tooling.py migrate GAME`; do not copy
the implementations into the game. These steps prepare and verify local artifacts;
publishing remains a separate, explicitly authorized action.

## Windows x64 installer

First run `python scripts/ship.py ship --no-install` on Windows. This verifies
the native package, executable resources and isolated packaged-game smoke.
Then use the same project command:

```text
python scripts/ship.py installer prepare
python scripts/ship.py installer build --iscc "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"
python scripts/ship.py installer test --stage "ABSOLUTE PREPARED STAGE PATH"
```

`prepare` also works on Linux with an existing Windows package. It validates
the AMD64 PE header and manifest hashes, copies only declared files into a new
`.blue-check/installers/prepared-*` directory, and emits a Unicode Inno script and `prepared.json`.
Generated stages stay outside source exports. Authored installer scripts and existing
game-owned delivery directories keep their current paths and publication policy.
It does not certify execution. `build` requires Windows and an explicit Inno Setup
6 compiler; it records compiler, input and output hashes in `built.json`. Failed
preparation/builds preserve prior generations and completed outputs, including logs.

Fresh scaffolds record a stable `app_id` in `scripts/installer.json`. Keep it across
Cargo package/title changes so upgrades find the same installation. Older projects
default to `BlueEngineGames.<Cargo package.name>`; set that original ID explicitly
before a rename. An optional `publisher` uses your own studio name. Existing custom
Inno scripts, including Reverie's, are preserved and retain their own delivery path.

The shared installer follows the existing per-user x64 path, offers an optional
desktop shortcut, and preserves saves/settings. It never includes undeclared
player files or uses wildcard `dist` inventory. `test` is a separate explicit
install/upgrade/uninstall action for an isolated Windows account: it refuses a
registered AppId in either registry hive/view or an existing Start Menu shortcut,
checks installed hashes and shortcut targets, and checks save/settings retention.
It does not establish installed-game launch or audible/physical device behavior.
Windows package and installer gates remain required before distribution.

## Game-owned native review

Declare the controls, camera, interface and audio behavior you actually intend in
`assets/native-review.json`. For example (replace these criteria for your game):

```json
{"schema_version":1,"checks":[
  {"id":"controls","area":"controls","instructions":"Exercise the identity's controls, menu and focus changes; describe actual results."},
  {"id":"camera","area":"camera","instructions":"Review the chosen camera and original interface during play at the intended window sizes."},
  {"id":"audio","area":"audio","instructions":"Listen to the authored cues/music and exercise mute and volume settings."}
]}
```

Areas are `controls`, `camera`, `ui`, `audio`, `gameplay` and `other`. IDs are stable
within this review. Omit inapplicable checks deliberately; a receipt covers only
declared checks and imposes no camera, HUD, artwork or sound style.
Package after declaring the spec, then prepare observations:

```text
python scripts/ship.py review prepare --settings SETTINGS_FILE --artifact CAPTURE_FILE
python scripts/ship.py review record --observations .blue-check/observations.json
python scripts/ship.py review status
```

Settings/artifacts are optional when inapplicable; repeat `--artifact` for more
files. Relative paths resolve from the game directory, independent of the caller.
Preparation creates `.blue-check/native-review.json` with an `observation_template`.
Save that object as `.blue-check/observations.json`, enter reviewer/session details,
the actual OS/display/renderer/input/audio devices and observed outcomes, then
record it. Preparation and recording do not launch the game or operate devices.
Existing receipts are preserved; use `--receipt .blue-check/NEW.json` for a new review.

The binding covers current package hashes, game/engine source inputs, the authored
spec, selected settings and artifacts. A normal package build captures source
provenance before and after compilation. `--no-build` cannot invent that provenance;
it retains existing proof only for unchanged source and executable bytes.
Changed or unverifiable inputs make the review stale/pending and retain its notes.

`manual: true` means all declared checks have explicit physical reviewer attestations
for that exact binding. It is not independent measurement or a signed certificate.
The recording host must match the package OS. Hardware rendering, physical inputs,
and physical audio output for audio checks must be stated. Known software rendering,
scripted input, silent/decoded audio counters and incomplete observations remain
pending. Software captures are useful artifacts, but cannot become physical passes.
Simulation, saves, networking, package smoke and installer gates remain separate.
