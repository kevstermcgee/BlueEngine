# Lantern Run

Read this file, src/lib.rs and game.project.json. For a rule change, update the tests in lib.rs.
main.rs is platform glue; do not explore the 3D renderer. Engine docs/TWO_D.md is the API reference.

- GameLogic + Simulation own fixed 60 Hz integer rules. Input is Intent. Draw only reads state.
- Snapshot uses the engine frame/hash/migration contract on native and browser. Bump VERSION and
  add a Migration for released state-layout changes; save every field that decides the future.
- The shared client owns viewport, devices, pause/restart, sound cues, save/load and settings.
- game.project.json deliberately selects presentation, targets and networking. Unsupported combinations fail.
- verification_input is a real public-input route, never mutate the game to pass verification.
  Add losing/collision/alternate-input tests too. Browser verification compares this route's native hash.

Commands from this game:

```
cargo test --no-default-features
scripts/blue web build                 # test, WASM, clean static package, real browser smoke
scripts/blue web verify                # package integrity and fresh browser smoke
scripts/blue publish --backend directory --destination /path/to/library
scripts/blue ship                     # native desktop distribution
```

Web builds require rustup target add wasm32-unknown-unknown, Node, ws and Chromium.
Publishing to a directory produces a deployment-ready library, not an external URL.
Never claim cargo build proves browser playback or that audio counters prove a listener heard sound.
