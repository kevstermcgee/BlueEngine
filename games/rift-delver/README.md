# Rift Delver

A native 3D expedition shooter built with BlueEngine. Dash through a lost machine city, assemble a powerful circuit build, then decide whether to extract your salvage or descend into a harder depth.

Three encounters make a depth. Each encounter ends with three seeded circuit choices; install one and recover some hull. Every third depth ends with a Warden and its escort. Clearing a depth opens extraction. Extracted salvage funds the permanent workshop; defeat keeps 25% of your unbanked haul, plus any Insurance upgrades. A lifetime contract pays another 50 salvage every 100 kills.

The workshop has four tracks with ten ranks each. Completing depth three unlocks the Shard Caster, and completing depth six unlocks the Arc Lance. Depths continue with increasing health, speed and enemy counts; enemy counts and live projectiles stay bounded. Three biome palettes and cover layouts rotate while enemy composition, spawn order and circuit drafts vary with the expedition's random stream.

Twelve stackable circuits modify damage, fire rate, cooling, penetration, chain lightning, kill explosions, lifesteal, hull, dash recharge, freezing, shockwave recharge and critical chance. Fast firing generates heat; burst fire and cooling circuits keep the weapon available. Dash grants a short invulnerability window. Shockwave damages and slows nearby machines while clearing their projectiles. Flying drones, charging melee machines, heavy brutes, ranged Sentinels and Wardens require different movement and target priorities.

## Controls

- WASD / left stick: move. Mouse / right stick: aim.
- Left mouse / right analog trigger: fire. Right mouse / right bumper: shockwave.
- Shift / left bumper: dash. Space / controller A: jump.
- Q / controller X: cycle unlocked weapons.
- E / Enter: begin an expedition, descend, or return from defeat.
- 1–3: choose a circuit. At an extraction gate, 1 extracts and 2 descends.
- 1–4: buy a workshop rank. Menus also support arrow keys, controller navigation and mouse clicks.
- F5 / F9: quick save / load. Esc: pause and settings. F: fullscreen.

Workshop and expedition state save automatically at transitions, after purchases, every 30 seconds of simulation and when quitting through the menu. Relaunch resumes the expedition. Saves use BlueEngine's checked, atomic snapshot files with backups. Installed updates retain saves and settings. Closing through the menu is the best way to preserve the latest moment.

## Development and evidence

The simulation is a rendering-free 60 Hz Rust library; `src/main.rs` handles devices, interpolated presentation, procedural meshes, effects, synthesized audio and engine-owned saves. No browser build is published. Windows x64 installers are delivered through BlueEngineGames.

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
python3 scripts/check.py --skip-ship
python3 scripts/ship.py ship --no-install
```

`--seed N`, `--capture DIR --frames 30,300`, `--script 'enter@2,fire:3-300'`, `--load SLOT_OR_FILE` and `--autopilot` provide repeatable QA. The autopilot uses ordinary intentions and has no extra health, damage or movement privileges. `examples/preview_states.rs` creates draft, extraction, loss and Warden saves by playing those same intentions. On Linux, `scripts/control_smoke.py` exercises actual X11 keyboard and mouse events and checks the resulting engine snapshot.

A 180,000-tick endurance test covers 50 minutes of simulation; this verifies bounded state and repeated progression, rather than certifying human enjoyment or physical audio output. Native package smoke, visual capture review and control checks provide separate evidence.
