//! `be2-tools new-game NAME DIR [ENGINE_PATH]`: Scaffolds a standalone, green game project.
//!
//! The generated project does not copy or fork the engine; it pins `vesper3d` as a
//! dependency. It comes fully equipped with a declarative blueprint, pre-compiled map,
//! `game.json`, AI instructions (`CLAUDE.md`), `STATUS.md`, and shell runners
//! (`scripts/blue` and `scripts/blue.ps1`).

use super::blueprint::{compile_blueprint, BlueprintSpec, DoorSpec, FillSpec, RoomSpec, SpawnSpec};
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

[dependencies]
vesper3d = {{ package = "be2", path = {engine_path_str}, default-features = false, features = ["client"] }}
macroquad = {{ version = "=0.4.14", default-features = false, features = ["audio"] }}
serde = {{ version = "1.0", features = ["derive"] }}
serde_json = "1.0"

[target.'cfg(windows)'.dependencies]
windows-sys = {{ version = "=0.61.2", features = ["Win32_UI_WindowsAndMessaging", "Win32_System_Threading"] }}
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
        fill: vec![
            FillSpec {
                room: "lobby".into(),
                kind: "chair".into(),
                count: 2,
                seed: 42,
            },
            FillSpec {
                room: "courtyard".into(),
                kind: "potted-cactus".into(),
                count: 2,
                seed: 101,
            },
        ],
    };
    let bp_json = serde_json::to_string_pretty(&blueprint)?;
    fs::write(
        target_dir.join("blueprints").join("main.blueprint.json"),
        bp_json,
    )?;

    // 3. Compile map
    let map_doc = compile_blueprint(&blueprint)?;
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
        interactables: vec![],
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
        rules: vec![super::game::Rule {
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
        }],
    };
    game.validate(&map_doc)?;
    fs::write(
        target_dir.join("game.json"),
        serde_json::to_string_pretty(&game)?,
    )?;

    // 5. src/main.rs
    let main_rs = r#"mod platform;
use vesper3d::viewer::{authoring::MapDocument, game_client, local_client};
fn window() -> macroquad::conf::Conf { game_client::window_config("BlueEngine game") }
#[macroquad::main(window)]
async fn main() -> vesper3d::Result<()> {
    let map = MapDocument::load(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/maps/main.json")))?;
    local_client::run_map_with_focus(map, platform::focused).await
}
"#;
    fs::write(target_dir.join("src").join("main.rs"), main_rs)?;

    fs::write(
        target_dir.join("src/platform.rs"),
        include_str!("../../templates/native_focus.rs"),
    )?;

    // 6. scripts/blue and scripts/blue.ps1
    let blue_sh = r#"#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cmd="${1:-help}"
case "$cmd" in
  check)
    cargo check
    cargo test
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

    let blue_ps1 = r#"param([string]$cmd = "help")
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

switch ($cmd) {
    "check" {
        cargo check
        cargo test
    }
    "build-all" {
        cargo build --release
    }
    "play" {
        cargo run --release
    }
    default {
        Write-Host "Usage: .\scripts\blue.ps1 {check|build-all|play}"
    }
}
"#;
    fs::write(target_dir.join("scripts").join("blue.ps1"), blue_ps1)?;

    // 7. STATUS.md and CLAUDE.md
    let status_md = format!(
        r#"# {name} Status

## Completed
- Project scaffolding initialized with BlueEngine v0.2.0.
- Declarative blueprint created in `blueprints/main.blueprint.json`.
- Starter map compiled to `maps/main.json`.
- Valid stock-client game document written to `game.json`.
- Playable local static-map client uses shared input, camera, characters and pause menus.

## Next Steps
- Add custom gameplay rules to `game.json`.
- Expand map rooms and layout via blueprint.
- Implement multiplayer match flow.
"#
    );
    fs::write(target_dir.join("STATUS.md"), status_md)?;

    let claude_md = format!(
        r#"# {name} - AI Agent Guide

This is a standalone BlueEngine game project. **The engine is not duplicated in this repo**; it is pinned via `vesper3d` in `Cargo.toml`.

## Presentation baseline
Follow engine docs/SHARED_GAMEPLAY.md and docs/GAME_PRESENTATION.md.
The starter uses local_client::run_map: native controllers, WASD/arrows, collision-safe
camera, cached rendering, avatar selection and pause menus are inherited.
This is a static-map client. To execute GameDocument rules or dynamic physics,
launch the stock engine client with `be2 --game game.json`; do not silently ignore
those rules in a custom client. Shared creative editing is opt-in via viewer::creative.

## Working with Maps
- Edit `blueprints/main.blueprint.json` to alter room layouts, doors, and prop placements.
- Run `be2-tools build blueprints/main.blueprint.json maps/main.json` to compile into the map.
- Run `be2-tools lint maps/main.json` to verify map integrity before committing.

## Running Tests
- `scripts/blue check` (or `.\scripts\blue.ps1 check` on Windows)
"#
    );
    fs::write(target_dir.join("CLAUDE.md"), claude_md)?;

    Ok(())
}
