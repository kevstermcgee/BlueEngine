# Choose a native game path

Read the answer and runtime in `be2.py context TASK --level 1 --compact`, then only
the relevant contract below. Detailed guides are references for the current task,
not mandatory initial reading. `context FEATURE --level 2` supplies contracts,
examples and focused checks; level 3 supplies implementation ownership.

## Start a project

`python tools/be2.py map new-game NAME DIR ENGINE_PATH TEMPLATE`

- `two-d`: offline enemies, projectiles, scoring and AI in your own Rust rules.
  Implement `vesper3d::two_d::GameLogic: Snapshot<Input = Intent>`. Start with
  `templates/two-d/lib.rs`; details: [TWO_D.md](TWO_D.md).
- `three-d` / `hybrid` / default `portable`: draw 3D over 2D collision and rules.
  Spatial enemies/physics require `custom-sim`; details: [PORTABLE_GAMES.md](PORTABLE_GAMES.md).
- `stock`: declarative `GameDocument` counters, interactions, triggers, timers and
  movers. Edit validated game/map JSON; example `assets/games/minigames`.
  Enemy/projectile custom rules require Rust. Details: [GAME_QUICKSTART.md](GAME_QUICKSTART.md).
- `custom-sim`: native game-owned spatial rules, optional physics and netplay.
  `Simulation::step(&mut self, input: &Self::Input)` advances one fixed tick;
  `state_hash(&self) -> u64` covers authoritative state. Use `Lifecycle` for
  fixed ticks/input and your own presentation. Start with `templates/custom-sim/src/lib.rs`.
  Enable `physics` for spatial physics and `networking` for production QUIC/TLS;
  enable `native-client` for native devices/drawing. Task reference: [CUSTOM_SIM_CHEATSHEET.md](CUSTOM_SIM_CHEATSHEET.md).

Controls, camera, fonts, art, HUD and audio belong to your game. Custom clients
remain supported. Authority is fixed-step and rendering-free; presentation reads
it. For uncaptured mouse input use `shell.accepting_input()`, since `playing()`
requires capture. Verify real keys and changed visuals in a native window.

## Movement and saves

Stock `GameDocument.player_profile.jump_height: f32` is metres, finite `0..=3`,
default `0.35`. Raise it to jump higher. Custom controllers use
`Controller::for_profile(profile, feet, yaw)`. `set_gravity` changes the arc while
preserving this height. Check `context movement --level 2`.

Implement `devkit::Snapshot` on your existing simulation: `capture() -> State`,
`restore(State) -> Result<(), String>`. Use engine-owned saves through Lifecycle
or GameLogic. Keep `KIND` stable, version/migrate layout or hash changes, retain
RNG/tick state, and test `snapshot::assert_resumes_as_promised(make, inputs, every)`.
Details: [SAVE_STATE.md](SAVE_STATE.md); do not invent another save format.

## Presentation and original audio

Custom `kit::hud` and portable `Scene` accept your own drawing; stock
`presentation.hud` is a separate declarative path. Check `context client_kit`.
Use `python tools/be2.py map audio describe` to discover authored audio projects,
then `audio validate PROJECT`, `audio render PROJECT NEW_BUNDLE`, `audio check BUNDLE`.
Custom `AudioBank::load(root, muted, sfx_volume, music_volume).await?` loads checked
bundles; stock binds `presentation.audio`. JSON/PCM/music edits need rendering,
not a Rust rebuild. Assets/counters do not prove audible devices. Details: [AUDIO.md](AUDIO.md).

## Custom multiplayer limits

Implement `viewer::netplay::{NetGame, ClientView}`; start with
`src/viewer/netplay/toy.rs`. `netplay::cli::serve::<G>(&ServeSpec)` supplies the
server main and signal handling. Rules, prediction, bot handoff and presentation
remain game-owned. Read `context netplay --level 2` before designing the wire data.

Custom netplay does not inherit stock `be2-headless` partial replication,
interest scheduling, `--max-players` or `--network-threads`. Snapshots must fit
`min(active transport payload_limit, 1100)` including framing.
`snapshot_with_budget` can trim optional data per recipient; it does not fragment
large worlds. `MAX_SEATS` permits 1..=16; room settings are not demonstrated
capacity. `RELIABLE_EVENTS` is opt-in, bounded, acknowledged and reports gaps.
Local queue acceptance never proves remote delivery. Select QUIC/TLS for
encryption; hub rooms use development UDP. Details: [NETPLAY.md](NETPLAY.md).

## Iterate and deliver

`python tools/be2.py check --game DIR --loop inner` checks game behavior;
`--loop integration` checks integration. These do not certify shipping.
`python scripts/ship.py ship --no-install` on Windows builds and checks the
native package in `dist`: resources, identity/icon, asset closure and isolated
packaged-game launch. It does not build an installer. Use `python scripts/ship.py
installer prepare`, then `installer build --iscc PATH` and explicit `installer test
--stage PATH` on an isolated Windows account. New scaffolds keep a stable AppId in
`scripts/installer.json`; existing custom installer scripts remain supported.
Declare game-owned device criteria in `assets/native-review.json`, then use
`python scripts/ship.py review prepare`, `review record --observations FILE`, and
`review status`. Review receipts bind package/source/settings/artifacts and keep
software captures distinct from physical attestations. Commands and examples:
[NATIVE_DELIVERY.md](NATIVE_DELIVERY.md).
Linux development checks cannot certify Windows installers or physical devices. Full engine Linux/Windows gates
remain required after engine changes. Publish/deploy only when explicitly authorized.
New-game creation checks for rustfmt before writing, then formats the assembled
Rust sources once so the first integration format gate passes.
Browser gameplay/WASM is retired: [BROWSER_WORKFLOW.md](BROWSER_WORKFLOW.md).
