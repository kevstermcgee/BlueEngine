# BlueEngine feedback from shipping Rift Delver

Engine baseline: `9189d68aabf098bd82a34265bbcf4395970a9da7` (latest main when development began). Consumer: `games/rift-delver`, a native spatial 3D expedition shooter with fixed-step authority, six enemy types, three weapons, twelve circuits, persistent workshop progression and exact expedition saves.

This report distinguishes observed authoring failures from proposed capabilities. It contains reproducible commands, game-side workarounds and acceptance criteria; no session logs or credentials are included.

## P1: custom-sim scaffolding does not produce the metadata its workflow requires

Reproduce in an empty directory:

```sh
python3 tools/be2.py map new-game repro /tmp/rift-repro "$PWD" custom-sim
```

The generated project has native check/ship scripts, but no `game.project.json` and no `scripts/project.py`. `docs/AI_SPRINGBOARD.md` directs authors to update that project manifest after scaffolding. Adding it with `runtime: custom-sim` fails with `runtime must be portable (native, any presentation) or legacy-native`; the valid value is `legacy-native`. Adding a valid manifest then makes `scripts/check.py --skip-ship` fail with `game.project.json needs scripts/project.py; refresh generated project tooling`.

Rift Delver explicitly declared `runtime: legacy-native`, `presentation: 3d`, native Linux/Windows targets and keyboard/mouse/controller input, then copied the canonical `templates/game_project.py` to its generated scripts directory. This preserves requirements rather than removing metadata to silence validation.

**Requested fix:** emit the canonical manifest and validator for *every* starter. Keep CLI starter names distinct from runtime vocabulary in generated documentation, and make a reported refresh command actually callable. Add a scaffold-matrix test that immediately validates and plans inner/integration/shipping checks for stock and custom-sim, including spatial 3D and two explicit native targets. It must require no manual file copying.

## P1: fresh authoring tooling is not discoverable by the generated game checker

The canonical `be2.py map new-game` builds fresh tooling at `target/itest/be2-tools`. Immediately running the generated custom-sim `scripts/check.py --skip-ship` reports `No be2-tools found` and recommends a separate `--profile fast` build. Its suggested discovery path and the coordinator's authoring profile disagree.

Observed workaround: `python3 games/rift-delver/scripts/check.py --skip-ship --tools "$PWD/target/itest/be2-tools"` succeeds, including the rule suite. The tool is present and usable; this is discovery friction, not a missing compiler.

**Requested fix:** generated scripts should share the coordinator's tool-resolution/freshness contract, including `itest` and a selected `CARGO_TARGET_DIR`. A clean scaffold test should invoke the checker without `--tools` or a second tooling compilation and assert the same binary/source identity. Preserve an explicit override for packaged tool users.

## P2: a 3D action brief can select planar collision without surfacing that distinction early enough

The initial objective, “Create Rift Delver, a polished 3D action roguelite with persistent progression, procedural expeditions, satisfying dash combat”, selected `three-d`/portable planar collision. Adding explicit “spatial 3D enemies projectiles” and `--template custom-sim` selected the required native route. The resulting packet correctly identifies `custom-sim`, but ranks `game_documents` as its context feature and points to the stock GameDocument guide instead of the custom simulation cheat sheet.

**Requested fix:** treat common phrases such as first-person combat, airborne enemies, dodge projectiles and action roguelite as a reason to inspect dimensionality. When uncertainty remains, report the planar/spatial choice prominently. An explicit custom-sim selection should make `custom_simulation` and its cheat sheet the first contract. Test both the original broad brief and the narrowed brief against starter selection and context ownership.

## P2: unattended performance reports mix logical time with actual rendering cost

A software-rendered capture printed `avg 16.67 ms (60 fps)` and all logical-frame percentiles at 16.67 ms, while the same report separately recorded `own work per frame ... max 162.01 ms`. Unattended fixed-dt runs intentionally advance one simulation tick per frame, but a player-facing “fps” label can turn that into an unsupported performance claim.

**Requested fix:** label these as `simulation_step_ms`/`logical_fps`, and report independently measured wall frame time, presentation work, capture overhead and missed real-time budgets. Include `timing_mode: fixed_capture` in machine-readable receipts. A test using an intentionally slow frame should retain deterministic simulation timing while reporting slow real-time presentation. This report does **not** claim 60 fps on a GPU or certify hardware performance.

## P2: promote an isolated viewmodel pass into the presentation kit

The first capture showed a disproportionately large gun; correcting its scale also revealed the known near-wall viewmodel intersection problem. Rift Delver renders the gun into a transparent, separately depth-buffered native render target and composites it before the HUD. The target resizes with the window. This is an application of the existing learning hint, rather than a new engine defect.

**Requested capability:** a kit-owned viewmodel pass that handles target lifetime, resize, alpha, lighting, projection and compositing. Acceptance captures should cover flush-to-wall, a narrow doorway, muzzle recoil, a resize and both native platforms. Keep simulation/raycast authority in the world; moving a viewmodel must never extend a shot through cover.

## P2: provide a small headless distance-query contract for game-owned combat

Rift Delver needed nearest ray-sphere and ray-AABB distances for cover-consistent hitscan, penetration and AI line of sight. The indexed FPS feature provides firearm/ammunition/team-deathmatch policy; `math::Bounds::hit` reports a boolean. This energy/heat roguelite owns different weapon rules and needs geometric distances, not the stock firearm policy. It therefore implements these two narrow queries in its library, with cover and spatial-target tests.

**Requested capability:** rendering-free, validated nearest-distance ray/sphere/box queries with explicit normalized direction, range, inside-origin, parallel-ray and stable tie-breaking contracts. Document them in the custom-sim cheat sheet. Test zero/parallel directions, an origin inside geometry, overlapping targets and a target behind cover. Keep heat, circuit effects, damage and enemy policy game-owned.

## P2: make actual device-path QA a first-class native check

Scripted intentions are valuable, but bypass `ClientInput` and cannot prove real keys reach authority. Rift Delver's `scripts/control_smoke.py` launches an isolated Xvfb instance, sends actual X11 E/W/Shift/F5 and mouse buttons, then reads the engine-created quick snapshot. It asserts phase transition, movement, dash cooldown, shot heat and pulse cooldown; it does not write its own save files. A null ALSA device is necessary on this headless host even with muted gameplay, because Macroquad initializes audio separately.

**Requested capability:** a documented native device-injection fixture on Linux and Windows with semantic action observations, rather than each game maintaining ctypes/native event plumbing. It should distinguish emulated device-path evidence from physical mouse/gamepad and audible speaker evidence, and keep player files isolated. A regression that mistakenly gates uncaptured menu/game input on `shell.playing()` must fail.

## What worked well

- `Controller` movement, collision and impulse channels made dash behavior consistent with cover. `Controller::interpolated` plus `Lifecycle.pending_look` preserved immediate aiming while smoothing translation.
- Seeded `Rng`, fixed 60 Hz authority, events and exact `Snapshot` continuation made the same game playable, testable and capturable. The game's replay and frequent-save tests pass without weakening the save policy.
- Atomic engine save slots support automatic workshop/expedition persistence, F5/F9 and damaged-file rejection without a custom persistence implementation.
- `Template`, materials, particles, HUD, synthesized sound and shadows enabled a coherent native low-poly presentation without importing a second engine.
- A player-like bot completed repeated depths over 180,000 ticks (50 minutes of simulation) while live enemy and projectile counts remained bounded. This is longevity/authority evidence; it is not a substitute for human enjoyment testing.
- Windows EXE-only distribution, identity resources, isolated package smoke and source-pinned release definitions provide a strong shipping contract. The report is linked from the game's source and accompanies its release work.
