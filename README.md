# Blue Engine 2 (BE2) — 0.2.0

A native Rust client and headless simulation foundation for our Prop Hunt project. Forked from the latest Blue Engine working code, preserving the wrench, Blue mechanic skin, first/third-person camera and Git history.

## Play

Open `bin/BE2.exe`, then click **Enter the room**. No installation or server is needed. Windows x64 is the locally tested build.

WASD/arrows move; mouse looks; Shift sprints; Space jumps; Ctrl/C crouches; Q switches camera; left-click swings the wrench; E interacts; Escape/Tab pauses. H enables optional help; F3 enables diagnostics. The default view retains only the crosshair and relevant action prompts. Right-click/Backspace dismisses an inspection card. F/F11 toggles fullscreen.

## Changes

- Softer baked shadows, roughness-aware highlights and restrained metal reflections.
- Fixed 60 Hz movement with interpolated display poses and bounded catch-up after stalls.
- Preserved between-frame action taps and reduced held-tool screen coverage.
- Removed persistent branding, control panels by default, and redundant hit text.
- Shared static vertices and reusable character/tool buffers reduce rendering work and allocations.
- Four new props: shipping crate, steel barrel, workshop stool and toolbox, with stable IDs and collision bounds.
- A true headless Cargo build with no graphics/window dependencies.

## Reusable props

The props are in the room and in `assets/props/*.json`. Reuse them through `vesper3d::viewer::props::scene(PropKind)`, or export them to a new folder:

```sh
cargo run --no-default-features --example export_props -- my-props
```

Exports refuse to replace existing files. JSON uses the inherited scene format documented in AI_REFERENCE.md. Props are static objects for now; disguises and possession belong to the upcoming Prop Hunt game layer.

## Build

```sh
cargo run --release --locked --bin be2
cargo build --release --locked --no-default-features --bin be2-headless
cargo run --release --locked --no-default-features --bin be2-headless -- --ticks 60000
cargo run --release --locked --no-default-features --bin be2-headless -- --ticks 600 --realtime
```

The second command is the intended build configuration on Debian/Ubuntu. Linux compilation and VPS performance have not been tested locally. CI includes Linux checks. The headless executable runs a bounded local two-player simulation; it does not open a network port.

`bin/be2-headless.exe` is the Windows headless build. `bin/vesper3d.exe` retains the original offline rendering CLI. The library remains named `vesper3d` for compatibility.

## Validation

```sh
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked --no-default-features
cargo clippy --all-targets --locked --no-default-features -- -D warnings
```

Visual smoke modes: `BE2.exe --capture DIR`, `--capture-props DIR`, `--capture-character DIR`, `--capture-interactions DIR`, `--capture-motion DIR`, and `--capture-wrench DIR`. Use a new directory; capture filenames are replaced inside the explicitly supplied directory.

See VALIDATION.md for measured results and limitations; BE2_ARCHITECTURE.md for the implementation and PulseNet plan. The original prototype documentation is retained in BLUE_V1_README.md and BLUE_V1_VALIDATION.md.

## Next: Prop Hunt and PulseNet

This release improves the engine and adds props. It does not yet implement online multiplayer, prop disguises, hunters/hiders, rounds or scoring. The next phase connects the discussed PulseNet utility to the headless world, adds authoritative game rules, and validates one server with two clients. Offline play remains available.
