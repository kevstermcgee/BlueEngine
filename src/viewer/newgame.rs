//! `be2-tools new-game <NAME> [DIR]`: Scaffolds a standalone, green game project.
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

    let engine_path_str = engine_rel_path.unwrap_or("../BlueEngine");

    // 1. Cargo.toml
    let cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
vesper3d = {{ path = "{engine_path_str}", features = ["client", "offline"] }}
serde = {{ version = "1.0", features = ["derive"] }}
serde_json = "1.0"
tokio = {{ version = "1", features = ["full"] }}
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
                floor_color: Some([0.35, 0.40, 0.45]),
                wall_color: None,
                lamp: true,
            },
            RoomSpec {
                id: "courtyard".into(),
                rect: [0.0, -4.0, 8.0, 4.0],
                floor_color: Some([0.45, 0.50, 0.40]),
                wall_color: None,
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
            offset: Some([-3.0, 0.0]),
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
    let game_json = serde_json::json!({
        "schema_version": 1,
        "name": name,
        "map": "maps/main.json",
        "tick_rate": 60,
        "counters": {
            "score": 0
        },
        "rules": [
            {
                "event": "on_enter",
                "target": "courtyard",
                "condition": null,
                "action": "increment",
                "counter": "score",
                "amount": 1,
                "once": true
            }
        ]
    });
    fs::write(
        target_dir.join("game.json"),
        serde_json::to_string_pretty(&game_json)?,
    )?;

    // 5. src/main.rs
    let main_rs = r#"use std::path::Path;
use vesper3d::viewer::game::GameDocument;

#[tokio::main]
async fn main() -> vesper3d::Result<()> {
    println!("Launching BlueEngine Game...");
    let game_doc = GameDocument::load(Path::new("game.json"))?;
    let world = game_doc.world()?;
    println!("Game '{}' loaded successfully with content hash: {:016x}", game_doc.name, world.content_hash);
    Ok(())
}
"#;
    fs::write(target_dir.join("src").join("main.rs"), main_rs)?;

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
    cargo run
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
        cargo run
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
- Authoritative game logic configured in `game.json`.

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
