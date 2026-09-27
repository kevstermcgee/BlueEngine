//! `be2-tools new-game NAME DIR [ENGINE_PATH]`: Scaffolds a standalone, green game project.
//!
//! The generated project does not copy or fork the engine; it uses `vesper3d` as a
//! dependency. It comes fully equipped with a declarative blueprint, pre-compiled map,
//! `game.json`, AI instructions (`AGENTS.md`), `STATUS.md`, and shell runners
//! (`scripts/blue` and `scripts/blue.ps1`).

use super::blueprint::{compile_blueprint, BlueprintSpec, DoorSpec, RoomSpec, SpawnSpec};
use crate::Result;
use std::fs;
use std::path::Path;

pub fn scaffold_new_game(
    name: &str,
    target_dir: &Path,
    engine_rel_path: Option<&str>,
) -> Result<()> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("Game name must be a Cargo-compatible identifier".into());
    }
    if target_dir.exists() && target_dir.read_dir()?.next().is_some() {
        return Err(format!(
            "Target directory '{}' already exists and is not empty",
            target_dir.display()
        )
        .into());
    }

    fs::create_dir_all(target_dir)?;
    fs::create_dir_all(target_dir.join("src"))?;
    fs::create_dir_all(target_dir.join("blueprints"))?;
    fs::create_dir_all(target_dir.join("maps"))?;
    fs::create_dir_all(target_dir.join("scripts"))?;
    fs::create_dir_all(target_dir.join(".github").join("workflows"))?;

    let engine_path_str = serde_json::to_string(engine_rel_path.unwrap_or("../BlueEngine"))?;

    // 1. Cargo.toml
    let cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[features]
default = ["client"]
client = ["vesper3d/client", "dep:macroquad", "dep:windows-sys"]

[[bin]]
name = "{name}"
path = "src/main.rs"
required-features = ["client"]

[dependencies]
vesper3d = {{ package = "be2", path = {engine_path_str}, default-features = false }}
macroquad = {{ optional = true, version = "=0.4.14", default-features = false, features = ["audio"] }}
serde = {{ version = "1.0", features = ["derive"] }}
serde_json = "1.0"

[target.'cfg(windows)'.dependencies]
windows-sys = {{ optional = true, version = "=0.61.2", features = ["Win32_UI_WindowsAndMessaging", "Win32_System_Threading", "Win32_UI_Input_KeyboardAndMouse"] }}
"#
    );
    fs::write(target_dir.join("Cargo.toml"), cargo_toml)?;

    // 2. Blueprint
    let blueprint = BlueprintSpec {
        name: format!("{name} Starter Map"),
        height: 3.2,
        wall_thickness: 0.20,
        rooms: vec![
            RoomSpec {
                id: "lobby".into(),
                rect: [-6.0, -4.0, 0.0, 4.0],
                floor_color: Some([0.18, 0.30, 0.31]),
                wall_color: Some([0.72, 0.64, 0.46]),
                lamp: true,
            },
            RoomSpec {
                id: "courtyard".into(),
                rect: [0.0, -4.0, 8.0, 4.0],
                floor_color: Some([0.42, 0.31, 0.18]),
                wall_color: Some([0.72, 0.64, 0.46]),
                lamp: true,
            },
        ],
        doors: vec![DoorSpec {
            between: ["lobby".into(), "courtyard".into()],
            width: 1.2,
            at: Some(0.0),
        }],
        spawns: vec![SpawnSpec {
            id: "player1".into(),
            room: "lobby".into(),
            offset: Some([0.0, 0.0]),
        }],
        fill: vec![],
    };
    let bp_json = serde_json::to_string_pretty(&blueprint)?;
    fs::write(
        target_dir.join("blueprints").join("main.blueprint.json"),
        bp_json,
    )?;

    // 3. Compile map
    let map_doc = compile_blueprint(&blueprint)?.apply(&[
        super::authoring::Edit::AddBox {
            id: "objective".into(),
            label: "Activate the blue terminal".into(),
            center: crate::math::V(-3., 1.4, -1.8),
            half_extents: crate::math::V(0.35, 0.35, 0.2),
            structural: false,
            color: crate::math::V(0.15, 0.5, 0.9),
        },
        super::authoring::Edit::AddProp {
            id: "apple".into(),
            label: "Carryable apple".into(),
            kind: "apple".into(),
            origin: crate::math::V(-2., 0.5, -0.5),
        },
    ])?;
    let map_json = serde_json::to_string_pretty(&map_doc)?;
    fs::write(target_dir.join("maps").join("main.json"), map_json)?;

    // 4. game.json
    let spawn = map_doc
        .default_spawn
        .ok_or("Starter blueprint has no spawn")?;
    let game = super::game::GameDocument {
        schema_version: 1,
        name: name.into(),
        map: "maps/main.json".into(),
        player_profile: Default::default(),
        spawn_points: vec![super::game::SpawnPoint {
            id: "player1".into(),
            feet: spawn.feet,
            yaw: spawn.yaw,
        }],
        counters: std::collections::BTreeMap::from([("visits".into(), 0)]),
        interactables: vec![super::game::Interactable {
            entity: "objective".into(),
            enabled: true,
            visible: true,
        }],
        trigger_zones: vec![super::game::TriggerZone {
            id: "courtyard".into(),
            bounds: super::controller::Collider {
                min: crate::math::V(1., 0., -3.),
                max: crate::math::V(7., 2., 3.),
            },
            enabled: true,
        }],
        movers: vec![],
        timers: vec![],
        rules: vec![
            super::game::Rule {
                id: "activate-terminal".into(),
                on_interact: Some("objective".into()),
                on_enter: None,
                on_exit: None,
                on_timer: None,
                condition: None,
                once: true,
                actions: vec![super::game::GameAction::Complete],
            },
            super::game::Rule {
                id: "visit-courtyard".into(),
                on_interact: None,
                on_enter: Some("courtyard".into()),
                on_exit: None,
                on_timer: None,
                condition: None,
                once: true,
                actions: vec![super::game::GameAction::Increment {
                    counter: "visits".into(),
                    amount: 1,
                }],
            },
        ],
    };
    game.validate(&map_doc)?;
    fs::write(
        target_dir.join("game.json"),
        serde_json::to_string_pretty(&game)?,
    )?;

    // 5. src/main.rs
    let main_rs = r#"mod platform;
use vesper3d::viewer::{game::GameDocument, game_client, playable};
fn window() -> macroquad::conf::Conf { game_client::window_config("BlueEngine game") }
#[macroquad::main(window)]
async fn main() -> vesper3d::Result<()> {
    let game = GameDocument::load(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/game.json")))?;
    let mut options = playable::GameOptions::from_args(&std::env::args().collect::<Vec<_>>())?;
    options.keyboard = platform::keyboard();
    playable::run_game_with_options(game, options, platform::focused).await
}
"#;
    fs::write(target_dir.join("src").join("main.rs"), main_rs)?;

    fs::write(
        target_dir.join("src/platform.rs"),
        include_str!("../../templates/native_focus.rs"),
    )?;

    fs::create_dir_all(target_dir.join("tests"))?;
    fs::write(
        target_dir.join("tests/gameplay.rs"),
        include_str!("../../templates/game_runtime_test.rs"),
    )?;

    // 6. One validation implementation for both shells; logs stay out of AI context.
    fs::write(
        target_dir.join("scripts/check.py"),
        include_str!("../../templates/game_check.py"),
    )?;
    fs::write(target_dir.join(".gitignore"), "/target/\n/.blue-check/\n")?;
    let blue_sh = r#"#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cmd="${1:-help}"
case "$cmd" in
  check)
    shift
    python scripts/check.py "$@"
    ;;
  build-all)
    cargo build --release
    ;;
  play)
    cargo run --release
    ;;
  *)
    echo "Usage: scripts/blue {check|build-all|play}"
    ;;
esac
"#;
    fs::write(target_dir.join("scripts").join("blue"), blue_sh)?;

    let blue_ps1 = r#"param([string]$cmd = "help", [Parameter(ValueFromRemainingArguments=$true)][string[]]$CheckArgs)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

switch ($cmd) {
    "check" {
        python scripts/check.py @CheckArgs
        exit $LASTEXITCODE
    }
    "build-all" {
        cargo build --release
        exit $LASTEXITCODE
    }
    "play" {
        cargo run --release
        exit $LASTEXITCODE
    }
    default {
        Write-Host "Usage: .\scripts\blue.ps1 {check|build-all|play}"
    }
}
"#;
    fs::write(target_dir.join("scripts").join("blue.ps1"), blue_ps1)?;

    // 7. Compact local instructions; engine maintenance context is demand-loaded.
    let status_md = format!(
        r#"# {name} Status

## Completed
- Project scaffolding initialized with BlueEngine v0.2.0.
- Declarative blueprint created in `blueprints/main.blueprint.json`.
- Starter map compiled to `maps/main.json`.
- Authored game document with an interactive terminal and dynamic apple written to `game.json`.
- Shared authoritative gameplay runs locally; --connect presents server-owned state.
- E interacts/carries; after completion E restarts the world for all players.

## Next Steps
- Add custom gameplay rules to `game.json`.
- Expand map rooms and layout via blueprint.
- Test changes locally and against the shared headless server.
"#
    );
    fs::write(target_dir.join("STATUS.md"), status_md)?;

    let agents_md = format!(
        r#"# {name} - AI Agent Guide

This is a standalone game. BlueEngine is a path dependency (`vesper3d` in Cargo.toml),
not a pinned engine revision. Read this game's files first. Do not load engine source
or run engine-wide checks for game-only edits. Record the engine revision when shipping.

For an unfamiliar API, run `python tools/be2.py context QUERY` in the engine checkout;
read only the returned relevant contracts. Missing capability means engine work,
not permission to invent an API. Engine changes follow the engine's AGENTS.md.

## Presentation baseline
For presentation changes read engine docs/GAME_PRESENTATION.md; for custom loops or
shared controls read docs/SHARED_GAMEPLAY.md. Keep the official engine branding.
The starter uses playable::run_game_with_options: authored rules, dynamic props,
objectives and replay use shared HeadlessWorld authority. Input, camera, cached
rendering and menus are inherited. --connect ADDR uses server state and movement
prediction; local play opens no socket. local_client::run_map is a static viewer only.

## Working with Maps
- Edit `blueprints/main.blueprint.json` to alter room layouts, doors, and prop placements.
- Compile into a NEW map file, preserve authored objective/prop additions, then validate and adopt it.
- Keep visual, collision and semantic IDs together. Discover reusable assets before creating new ones.
- Add map `checks` and behavioral scenarios for the behavior being changed.

## Running Tests
- Setup once: `cargo generate-lockfile`; commit Cargo.lock. Set BE2_TOOLS to a matching
  be2-tools binary (engine `python tools/be2.py build tools` prints its directory).
- `python scripts/check.py`: map audit/lint, declared map checks, GameDocument validation,
  and this game's locked Cargo tests (compilation included). JSON summary points to logs.
- `python scripts/check.py --content-only`: fast content iteration, no Cargo;
  does not certify Rust edits. Add `--scenario PATH` for each relevant behavior scenario.
- Before delivery run the full project check once on final files. Inspect world/menu
  captures, exercise changed inputs/fullscreen, and measure movement/ticks in release builds.
- Shell wrappers delegate to the same runner and propagate failures.
"#
    );
    fs::write(target_dir.join("AGENTS.md"), agents_md)?;
    fs::write(target_dir.join("CLAUDE.md"), "@AGENTS.md\n")?;

    Ok(())
}
