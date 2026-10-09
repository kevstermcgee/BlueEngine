//! `be2-tools new-game NAME DIR [ENGINE_PATH] [TEMPLATE]`: Scaffolds a standalone, green game project.
//!
//! The generated project does not copy or fork the engine; it uses `vesper3d` as a
//! dependency. Portable 2D/3D/hybrid starters share native clients; the CLI defaults
//! to portable. The original native starters remain available explicitly:
//!
//! * `stock` (legacy library default): a declarative blueprint, pre-compiled map, `game.json` and the shared
//!   playable runner. For games whose rules fit counters, interactables, timers and triggers.
//! * `custom-sim`: a pure simulation library plus a window binary that uses the engine's devkit and
//!   kit. For games with enemies, projectiles, scoring, AI or per-frame physics.
//!
//! Both come with AI instructions (`AGENTS.md`), `STATUS.md`, shell runners (`scripts/blue` and
//! `scripts/blue.ps1`), a project check, and everything a game needs to *ship*: an identity file
//! (`assets/identity.json`), a generated icon set, a build script that embeds the icon in the exe, and
//! `scripts/ship.py`, which packages the game into `dist/` and creates and verifies a uniquely named,
//! uniquely iconned desktop shortcut.

use super::blueprint::{compile_blueprint, BlueprintSpec, DoorSpec, RoomSpec, SpawnSpec};
use super::icon::{write_icon_set, IconSpec};
use super::identity::Identity;
use crate::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Which starter to generate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Template {
    /// Authored rules run by the shared playable runner (`GameDocument`).
    #[default]
    Stock,
    /// A game that owns its simulation: pure library + window binary (devkit and kit).
    CustomSim,
    /// Offline 2D game with the shared native client.
    TwoD,
    /// Browser/native 3D presentation, with the same rules and services.
    ThreeD,
    /// Composable 2D + 3D native presentation.
    Hybrid,
    /// Recommended flexible starter; equivalent to hybrid.
    Portable,
}

impl Template {
    /// The command-line name from the starter catalog.
    pub fn name(self) -> &'static str {
        match self {
            Self::Stock => "stock",
            Self::CustomSim => "custom-sim",
            Self::TwoD => "two-d",
            Self::ThreeD => "three-d",
            Self::Hybrid => "hybrid",
            Self::Portable => "portable",
        }
    }
    pub fn is_portable(self) -> bool {
        matches!(
            self,
            Self::TwoD | Self::ThreeD | Self::Hybrid | Self::Portable
        )
    }
    /// CLI default from the same build-free catalog used by the Python springboard.
    /// The public library default remains stock for compatibility.
    pub fn cli_default() -> Self {
        Self::parse(
            starter_catalog()["cli_default"]
                .as_str()
                .expect("starter default"),
        )
        .expect("valid starter default")
    }
    /// Parse a command-line name.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "stock" => Some(Self::Stock),
            "custom-sim" => Some(Self::CustomSim),
            "two-d" => Some(Self::TwoD),
            "three-d" => Some(Self::ThreeD),
            "hybrid" => Some(Self::Hybrid),
            "portable" => Some(Self::Portable),
            _ => None,
        }
    }
}

/// Authoritative starter metadata for discovery without compiling a tool binary.
pub fn starter_catalog() -> serde_json::Value {
    serde_json::from_str(include_str!("../../templates/starters.json"))
        .expect("validated starter catalog")
}

/// Scaffold the default (`stock`) starter. See [`scaffold_new_game_with`].
pub fn scaffold_new_game(
    name: &str,
    target_dir: &Path,
    engine_rel_path: Option<&str>,
) -> Result<()> {
    scaffold_new_game_with(name, target_dir, engine_rel_path, Template::Stock)
}

/// Scaffold for the authoring command: an explicit engine path is resolved from the caller's
/// working directory and written relative to the generated manifest. The omitted path retains
/// the legacy project-relative `../BlueEngine` default. The library's project-relative entry
/// points ([`scaffold_new_game`] and [`scaffold_new_game_with`]) retain their existing contract.
pub fn scaffold_new_game_from_cwd(
    name: &str,
    target_dir: &Path,
    engine_path: Option<&str>,
    template: Template,
) -> Result<()> {
    let Some(engine_path) = engine_path else {
        return scaffold_new_game_with(name, target_dir, None, template);
    };
    let engine = fs::canonicalize(engine_path)
        .map_err(|e| format!("Cannot resolve ENGINE_PATH '{engine_path}': {e}"))?;
    if !engine.join("Cargo.toml").is_file() {
        return Err(format!("ENGINE_PATH '{}' has no Cargo.toml", engine.display()).into());
    }
    // Canonicalize existing ancestors, including symlinks, without creating the output. Resolve
    // '..' after those symlinks, as the filesystem does, even when the final directory is new.
    let mut target = PathBuf::new();
    for component in std::path::absolute(target_dir)?.components() {
        match component {
            std::path::Component::ParentDir => {
                target.pop();
            }
            _ => {
                target.push(component);
                if target.exists() {
                    target = fs::canonicalize(target)?;
                }
            }
        }
    }
    if target == engine {
        return Err("ENGINE_PATH resolves to the game directory (self dependency)".into());
    }
    let engine_parts: Vec<_> = engine.components().collect();
    let target_parts: Vec<_> = target.components().collect();
    let shared = engine_parts
        .iter()
        .zip(&target_parts)
        .take_while(|(a, b)| a == b)
        .count();
    let dependency = if shared == 0 {
        // Windows paths on different volumes cannot be expressed relatively.
        engine
    } else {
        let mut relative = PathBuf::new();
        for _ in shared..target_parts.len() {
            relative.push("..");
        }
        for part in &engine_parts[shared..] {
            relative.push(part);
        }
        relative
    };
    let dependency = dependency
        .to_str()
        .ok_or("ENGINE_PATH cannot be represented as a UTF-8 Cargo path")?;
    scaffold_new_game_with(name, target_dir, Some(dependency), template)
}

/// Scaffold a project. `engine_rel_path` is written into `Cargo.toml` as given (relative to the new
/// project, or absolute; default `../BlueEngine`).
pub fn scaffold_new_game_with(
    name: &str,
    target_dir: &Path,
    engine_rel_path: Option<&str>,
    template: Template,
) -> Result<()> {
    let mut chars = name.chars();
    let starts_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if !starts_ok
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("Game name must be a Cargo-compatible identifier (letters, digits, - and _, not starting with a digit)".into());
    }
    if target_dir.exists() && target_dir.read_dir()?.next().is_some() {
        return Err(format!(
            "Target directory '{}' already exists and is not empty",
            target_dir.display()
        )
        .into());
    }
    let engine_arg = engine_rel_path.unwrap_or("../BlueEngine");
    let project = Project {
        name,
        lib: name.replace('-', "_"),
        dir: target_dir,
        engine_toml: serde_json::to_string(engine_arg)?,
        engine_dir: target_dir.join(engine_arg),
    };
    fs::create_dir_all(target_dir)?;
    for dir in ["src", "scripts", "assets", "tests"] {
        fs::create_dir_all(target_dir.join(dir))?;
    }
    let identity = match template {
        Template::Stock => scaffold_stock(&project)?,
        Template::CustomSim => scaffold_custom_sim(&project)?,
        Template::TwoD | Template::ThreeD | Template::Hybrid | Template::Portable => {
            scaffold_two_d(&project, template)?
        }
    };
    write_shipping_files(&project, identity, template)?;
    write_scripts(&project)?;
    if template.is_portable() {
        project.write(
            "scripts/project.py",
            include_str!("../../templates/game_project.py"),
        )?;
    }
    fs::write(target_dir.join("CLAUDE.md"), "@AGENTS.md\n")?;
    Ok(())
}

/// What every generated file may refer to.
struct Project<'a> {
    name: &'a str,
    /// The library crate name (`name` with `-` turned into `_`).
    lib: String,
    dir: &'a Path,
    /// `engine_rel_path` as a quoted TOML/JSON string.
    engine_toml: String,
    /// Where the engine checkout is, if the path resolves from the new project.
    engine_dir: PathBuf,
}

impl Project<'_> {
    fn write(&self, relative: &str, content: impl AsRef<[u8]>) -> Result<()> {
        let path = self.dir.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        Ok(())
    }
}

/// Replace `{{key}}` placeholders in a template file.
fn fill(template: &str, values: &[(&str, &str)]) -> String {
    values
        .iter()
        .fold(template.to_owned(), |text, (key, value)| {
            text.replace(&format!("{{{{{key}}}}}"), value)
        })
}

/// The engine checkout's current commit (12 hex characters), read from `.git` without running git:
/// a detached HEAD, a loose or packed branch ref, or a `.git` file pointing at a worktree.
pub fn engine_revision(engine_dir: &Path) -> Option<String> {
    let mut git = engine_dir.join(".git");
    if git.is_file() {
        let pointer = fs::read_to_string(&git).ok()?;
        let target = pointer.trim().strip_prefix("gitdir:")?.trim();
        git = engine_dir.join(target);
    }
    let head = fs::read_to_string(git.join("HEAD")).ok()?;
    let head = head.trim();
    let full = match head.strip_prefix("ref: ") {
        None => head.to_owned(),
        Some(reference) => {
            // A worktree keeps its refs in the common directory named by `commondir`.
            let common = fs::read_to_string(git.join("commondir"))
                .ok()
                .map_or_else(|| git.clone(), |c| git.join(c.trim()));
            fs::read_to_string(common.join(reference))
                .ok()
                .map(|s| s.trim().to_owned())
                .or_else(|| {
                    fs::read_to_string(common.join("packed-refs"))
                        .ok()
                        .and_then(|packed| {
                            packed.lines().find_map(|line| {
                                let (sha, name) = line.split_once(' ')?;
                                (name == reference).then(|| sha.to_owned())
                            })
                        })
                })?
        }
    };
    (full.len() >= 12 && full.bytes().all(|b| b.is_ascii_hexdigit())).then(|| full[..12].to_owned())
}

fn stock_identity(project: &Project) -> Identity {
    Identity {
        package: vec!["game.json".into(), "maps".into()],
        smoke_args: Some(vec!["--capture".into(), "{dir}".into()]),
        ..Identity::starter(
            project.name,
            "Activate the blue terminal to win.",
            "WASD move, mouse look, Space jump, E interact, F5/F9 save/load, F fullscreen, Esc menu",
        )
    }
}

fn custom_sim_identity(project: &Project) -> Identity {
    Identity {
        smoke_args: Some(
            [
                "--capture",
                "{dir}",
                "--frames",
                "30,90",
                "--exit-after",
                "100",
            ]
            .map(String::from)
            .to_vec(),
        ),
        ..Identity::starter(
            project.name,
            "Collect the glowing orbs and stay off the void.",
            "WASD move, mouse look, Space jump, F5/F9 save/load, F fullscreen, Esc menu",
        )
    }
}

/// Identity, icon set, build script, ship/check tooling, `.gitignore` and the seeded lock file: what
/// every game needs to be delivered as a packaged program with its own name and icon.
fn write_shipping_files(
    project: &Project,
    mut identity: Identity,
    template: Template,
) -> Result<()> {
    identity.engine_revision = engine_revision(&project.engine_dir);
    project.write("assets/identity.json", identity.to_json())?;
    write_icon_set(
        &IconSpec::new(&identity.title),
        &project.dir.join("assets"),
        false,
    )?;
    project.write("build.rs", include_str!("../../templates/game_build.rs"))?;
    project.write(
        "scripts/check.py",
        include_str!("../../templates/game_check.py"),
    )?;
    project.write(
        "scripts/ship.py",
        include_str!("../../templates/game_ship.py"),
    )?;
    project.write(
        "scripts/dev.py",
        include_str!("../../templates/game_dev.py"),
    )?;
    if !template.is_portable() {
        project.write(
            "src/platform.rs",
            include_str!("../../templates/native_focus.rs"),
        )?;
    }
    project.write(
        "tests/identity.rs",
        include_str!("../../templates/game_identity_test.rs"),
    )?;
    project.write(".gitignore", "/target/\n/.blue-check/\n/dist/\n")?;
    // Seed the lock file from the engine's so the game builds against the versions the engine was
    // tested with (and offline builds work). The first Cargo command settles it: it adds this game's
    // entry and drops crates only the engine's own tests need. `scripts/check.py` does that before
    // its locked build; `cargo generate-lockfile` would discard the pins.
    if let Ok(lock) = fs::read(project.engine_dir.join("Cargo.lock")) {
        project.write("Cargo.lock", lock)?;
    }
    Ok(())
}

fn write_scripts(project: &Project) -> Result<()> {
    let blue_sh = r#"#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
PY="$(command -v python3 || command -v python || true)"
if [ -z "$PY" ]; then
  echo "scripts/blue needs Python 3 (python3 or python on PATH)" >&2
  exit 1
fi

cmd="${1:-help}"
case "$cmd" in
  web|publish) echo "Browser gameplay is retired; use scripts/blue ship on Windows. See engine docs/BROWSER_WORKFLOW.md." >&2; exit 2 ;;
  check)
    shift
    "$PY" scripts/check.py "$@"
    ;;
  build-all)
    cargo build --release
    ;;
  dev)
    shift
    "$PY" scripts/dev.py "$@"
    ;;
  play)
    "$PY" scripts/dev.py --release
    ;;
  package)
    shift
    "$PY" scripts/ship.py package "$@"
    ;;
  shortcut)
    shift
    "$PY" scripts/ship.py shortcut "$@"
    ;;
  ship)
    shift
    "$PY" scripts/ship.py ship "$@"
    ;;
  *)
    echo "Usage: scripts/blue {check|build-all|dev|play|package|shortcut|ship}"
    ;;
esac
"#;
    project.write("scripts/blue", blue_sh)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            project.dir.join("scripts/blue"),
            fs::Permissions::from_mode(0o755),
        )?;
    }

    let blue_ps1 = r#"param([string]$cmd = "help", [Parameter(ValueFromRemainingArguments=$true)][string[]]$CheckArgs)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

switch ($cmd) {
    { $_ -in "web", "publish" } { throw "Browser gameplay is retired; use scripts/blue ship on Windows. See engine docs/BROWSER_WORKFLOW.md." }
    "check" {
        python scripts/check.py @CheckArgs
        exit $LASTEXITCODE
    }
    "build-all" {
        cargo build --release
        exit $LASTEXITCODE
    }
    "dev" {
        python scripts/dev.py @CheckArgs
        exit $LASTEXITCODE
    }
    "play" {
        python scripts/dev.py --release
        exit $LASTEXITCODE
    }
    "package" {
        python scripts/ship.py package @CheckArgs
        exit $LASTEXITCODE
    }
    "shortcut" {
        python scripts/ship.py shortcut @CheckArgs
        exit $LASTEXITCODE
    }
    "ship" {
        python scripts/ship.py ship @CheckArgs
        exit $LASTEXITCODE
    }
    default {
        Write-Host "Usage: .\scripts\blue.ps1 {check|build-all|dev|play|package|shortcut|ship}"
    }
}
"#;
    project.write("scripts/blue.ps1", blue_ps1)
}

/// The custom-simulation starter: pure library, window binary, determinism tests.
fn scaffold_custom_sim(project: &Project) -> Result<Identity> {
    let identity = custom_sim_identity(project);
    let values = [
        ("name", project.name),
        ("lib", project.lib.as_str()),
        ("title", identity.title.as_str()),
        ("tagline", identity.tagline.as_str()),
        ("engine", project.engine_toml.as_str()),
    ];
    let files: [(&str, &str); 7] = [
        (
            "Cargo.toml",
            include_str!("../../templates/custom-sim/Cargo.toml.tmpl"),
        ),
        (
            "AGENTS.md",
            include_str!("../../templates/custom-sim/AGENTS.md"),
        ),
        (
            "STATUS.md",
            include_str!("../../templates/custom-sim/STATUS.md"),
        ),
        (
            "src/lib.rs",
            include_str!("../../templates/custom-sim/src/lib.rs"),
        ),
        (
            "src/main.rs",
            include_str!("../../templates/custom-sim/src/main.rs"),
        ),
        (
            "tests/determinism.rs",
            include_str!("../../templates/custom-sim/tests/determinism.rs"),
        ),
        (
            "rustfmt.toml",
            "max_width = 120\nuse_small_heuristics = \"Max\"\n",
        ),
    ];
    for (path, template) in files {
        project.write(path, fill(template, &values))?;
    }
    Ok(identity)
}

fn scaffold_two_d(project: &Project, template: Template) -> Result<Identity> {
    let identity = Identity::starter(
        project.name,
        "Collect four lanterns and reach the garden exit.",
        "WASD/arrows/pad move; K save, L load, M sound, R restart",
    );
    let values = [
        ("name", project.name),
        ("lib", project.lib.as_str()),
        ("title", identity.title.as_str()),
        ("engine", project.engine_toml.as_str()),
    ];
    for (path,template) in [
        ("Cargo.toml",include_str!("../../templates/two-d/Cargo.toml.tmpl")),
        ("src/main.rs",include_str!("../../templates/two-d/main.rs")),
        ("src/lib.rs",include_str!("../../templates/two-d/lib.rs")),
        ("AGENTS.md",include_str!("../../templates/two-d/AGENTS.md")),
        ("README.md","# {{title}}\nCollect the four lanterns, avoid the pink patrol and reach the teal exit.\nWASD/arrows move. Click/Enter starts. K saves, L resumes, M toggles sound, R restarts.\n"),
        ("STATUS.md","2D native starter. Verify and inspect real captures before shipping.\n"),
    ] { project.write(path,fill(template,&values))?; }
    let presentation = match template {
        Template::TwoD => "2d",
        Template::ThreeD => "3d",
        _ => "hybrid",
    };
    if template != Template::TwoD {
        let original = fs::read_to_string(project.dir.join("src/lib.rs"))?;
        let start = original
            .find("#[cfg(feature=\"client\")]\nimpl draw::Game")
            .ok_or("Portable starter draw implementation missing")?;
        let end = original[start..]
            .find("#[cfg(test)] mod tests")
            .ok_or("Portable starter tests missing")?
            + start;
        let minimap = if presentation == "hybrid" {
            r#"
        s.rect(30,Rect::new(552,260,216,125),INK);
        s.text(31,"MAP",Point::new(565,280),16.,TEAL);
        for r in WALLS {s.rect(32,Rect::new(558+r.x/4,283+(r.y-60)/4,(r.w/4).max(2),(r.h/4).max(2)),TEAL);}
        s.circle(33,Point::new(558+(r.x+12)/4,283+(r.y-48)/4),4.,GOLD);
        "#
        } else {
            ""
        };
        let draw =
            include_str!("../../templates/two-d/world.rs").replace("// {{minimap}}", minimap);
        project.write(
            "src/lib.rs",
            format!("{}{}{}", &original[..start], draw, &original[end..]),
        )?;
    }
    project.write("game.project.json",serde_json::to_string_pretty(&serde_json::json!({"schema_version":1,"id":project.name,"presentation":presentation,"runtime":"portable","mobile_controls":{"layout":"dpad","action_label":null},"targets":starter_catalog()["starters"][template.name()]["default_targets"],"networking":"offline","input":["keyboard","mouse","controller"],"description":identity.tagline,"session_minutes":1,"complexity":"low"}))?)?;
    Ok(identity)
}

/// The stock starter: blueprint, map, `game.json` and the shared playable runner.
fn scaffold_stock(project: &Project) -> Result<Identity> {
    let name = project.name;
    fs::create_dir_all(project.dir.join("blueprints"))?;
    fs::create_dir_all(project.dir.join("maps"))?;
    let identity = stock_identity(project);

    // 1. Cargo.toml
    let engine_path_str = &project.engine_toml;
    let cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"
description = "{title}: {tagline}"

[features]
default = ["client"]
client = ["vesper3d/client", "dep:macroquad", "dep:windows-sys"]

[[bin]]
name = "{name}"
path = "src/main.rs"
required-features = ["client"]

[dependencies]
vesper3d = {{ package = "be2", path = {engine_path_str}, default-features = false }}
macroquad = {{ optional = true, version = "=0.4.14", default-features = false }}
serde = {{ version = "1.0", features = ["derive"] }}
serde_json = "1.0"

[target.'cfg(windows)'.dependencies]
windows-sys = {{ optional = true, version = "=0.61.2", features = ["Win32_UI_WindowsAndMessaging", "Win32_System_Threading", "Win32_System_Console", "Win32_UI_Input_KeyboardAndMouse"] }}
"#,
        title = identity.title,
        tagline = identity.tagline
    );
    project.write("Cargo.toml", cargo_toml)?;

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
    project.write("blueprints/main.blueprint.json", bp_json)?;

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
    project.write("maps/main.json", map_json)?;

    // 4. game.json
    let spawn = map_doc
        .default_spawn
        .ok_or("Starter blueprint has no spawn")?;
    let game = super::game::GameDocument {
        schema_version: 1,
        name: identity.title.clone(),
        map: "maps/main.json".into(),
        player_profile: Default::default(),
        spawn_points: vec![super::game::SpawnPoint {
            id: "player1".into(),
            feet: spawn.feet,
            yaw: spawn.yaw,
        }],
        counters: std::collections::BTreeMap::from([("visits".into(), 0)]),
        presentation: None,
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
    project.write("game.json", serde_json::to_string_pretty(&game)?)?;

    // 5. src/main.rs: identity, icon and packaged content beside the exe.
    let main_rs = r#"#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod platform;
use vesper3d::viewer::{game::GameDocument, game_client, identity::Identity, playable};

/// Title, tagline and controls live in one file, shared with the build script and `scripts/ship.py`.
const IDENTITY: &str = include_str!("../assets/identity.json");

fn window() -> macroquad::conf::Conf {
    platform::attach_console();
    let identity = Identity::parse(IDENTITY).expect("assets/identity.json is invalid; see scripts/ship.py verify");
    game_client::window_config_with_icon(
        &identity.title,
        game_client::icon_from_rgba(
            include_bytes!("../assets/icon_16.rgba"),
            include_bytes!("../assets/icon_32.rgba"),
            include_bytes!("../assets/icon_64.rgba"),
        ),
    )
}

/// Content next to the executable (a packaged game in dist/), else the project directory (cargo run).
fn content(file: &str) -> std::path::PathBuf {
    let beside = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|dir| dir.join(file)));
    match beside {
        Some(path) if path.is_file() => path,
        _ => std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(file),
    }
}

#[macroquad::main(window)]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{}", serde_json::json!({"ok":false,"error":error.to_string()}));
        std::process::exit(1);
    }
}
async fn run() -> vesper3d::Result<()> {
    let game = GameDocument::load(&content("game.json"))?;
    let mut options = playable::GameOptions::from_args(&std::env::args().collect::<Vec<_>>())?;
    options.keyboard = platform::keyboard();
    playable::run_game_with_options(game, options, platform::focused).await
}
"#;
    project.write("src/main.rs", main_rs)?;
    project.write(
        "tests/gameplay.rs",
        include_str!("../../templates/game_runtime_test.rs"),
    )?;

    // 6. Compact local instructions; engine maintenance context is demand-loaded.
    let status_md = format!(
        r#"# {name} Status

## Completed
- Project scaffolding initialized with BlueEngine v0.2.0.
- Declarative blueprint created in `blueprints/main.blueprint.json`.
- Starter map compiled to `maps/main.json`.
- Authored game document with an interactive terminal and dynamic apple written to `game.json`.
- Shared authoritative gameplay runs locally; --connect presents server-owned state.
- E interacts/carries; after completion E restarts the world for all players.
- Identity (`assets/identity.json`), a generated icon set, packaging and desktop-shortcut tooling.

## Next Steps
- Add custom gameplay rules to `game.json`.
- Expand map rooms and layout via blueprint.
- Edit `assets/identity.json` (real title, tagline, controls) and regenerate the icon.
- Test changes locally and against the shared headless server.
- Finish with `scripts/blue ship`.
"#
    );
    project.write("STATUS.md", status_md)?;

    let agents_md = format!(
        r#"# {name} - AI Agent Guide

BlueEngine is a path dependency (`vesper3d`); `assets/identity.json` records its revision. Read
this game's files first; do not load engine source or run engine-wide checks for game-only edits.
Start in the engine checkout: `python3 tools/be2.py start "<task>" --project GAME_DIR`
(`python` on Windows); `next`/`resume TASK_ID` refreshes context. Missing capability needs engine work; never invent APIs.

Rules that do not fit counters/interactables/timers (enemies, projectiles, scoring, AI, per-frame
physics)? Wrong starter: `new-game NAME DIR ENGINE_PATH custom-sim` owns its simulation
(engine docs/SHARED_GAMEPLAY.md "Custom loops", docs/CUSTOM_CLIENT.md).

## Working with Maps
- Edit `blueprints/main.blueprint.json` (rooms, doors, props); compile into a NEW map file,
  preserve authored objective/prop additions, validate, then adopt it.
- Keep visual, collision and semantic IDs together; discover reusable assets before creating new ones.
- Add map `checks` and behavioral scenarios for the behavior being changed.
- `playable::run_game_with_options` runs authored rules, dynamic props and replay on shared
  authority; `--connect ADDR` uses server state. F5/F9 save and load (engine docs/SAVE_STATE.md;
  never hand-write save files). `run_map` is a static viewer only.
- Stock audio: `presentation.audio` binds checked bundles in `assets/audio`; see engine docs/AUDIO.md.

## Running Tests
- Record friction in the engine: `python tools/learn.py record --game {name} --area AREA --tokens N
  --note "..." --keywords "future,query,terms"`. Add `--trap` for a silent failure; verify a representative
  future query with `be2.py context`. Without keywords the record is archive-only. Never copy session logs.
- Cargo.lock is seeded from the engine's; any Cargo command settles it. Commit it. Never run
  `cargo generate-lockfile` (it drops the pins). Set BE2_TOOLS to a matching be2-tools binary
  (engine `python tools/be2.py build tools` prints its directory).
- `python scripts/check.py`: map audit/lint, declared checks, GameDocument validation, locked Cargo
  tests, and the ship gate. `--content-only`: fast content iteration, no Cargo (not certifying Rust).
  `--skip-ship`: everything except the ship gate. `--scenario PATH` adds a behavior scenario.
- Inspect world/menu captures (`--capture NEW_DIR`; `python <engine>/tools/contact_sheet.py`).

## Definition of done (every game made with BlueEngine)
1. `python scripts/check.py` passes on final files; it ends with the ship gate.
2. Set title/tagline/controls in `assets/identity.json`; regenerate changed title art:
   `be2-tools icon TITLE assets --replace`. `scripts/blue ship --no-install` verifies package/smoke
   without desktop access; plain `ship` also installs and verifies its own shortcut.
3. You exercised changed controls and looked at real frames; state what you did not verify.
"#
    );
    project.write("AGENTS.md", agents_md)?;
    Ok(identity)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "newgame-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn templates_have_names_and_parse_back() {
        let catalog = starter_catalog();
        assert_eq!(catalog["schema_version"], 1);
        assert_eq!(catalog["starters"].as_object().unwrap().len(), 6);
        for t in [
            Template::Stock,
            Template::CustomSim,
            Template::TwoD,
            Template::ThreeD,
            Template::Hybrid,
            Template::Portable,
        ] {
            assert_eq!(Template::parse(t.name()), Some(t));
            let metadata = &catalog["starters"][t.name()];
            assert_eq!(metadata["runtime"] == "portable", t.is_portable());
            assert!(Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(metadata["guide"].as_str().unwrap())
                .is_file());
            if t.is_portable() {
                assert_eq!(metadata["networking"], serde_json::json!(["offline"]));
                assert_eq!(metadata["default_targets"], serde_json::json!(["windows"]));
            }
        }
        assert_eq!(Template::parse("racing"), None);
        assert_eq!(Template::default(), Template::Stock);
        assert_eq!(Template::default().name(), catalog["library_default"]);
        assert_eq!(Template::cli_default(), Template::Portable);
    }

    #[test]
    fn placeholders_are_filled_everywhere_and_unknown_ones_survive() {
        assert_eq!(
            fill("{{a}} and {{a}} but {{b}}", &[("a", "x")]),
            "x and x but {{b}}"
        );
    }

    #[test]
    fn revision_is_read_from_a_detached_head_a_loose_ref_a_packed_ref_and_a_worktree_file() {
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let dir = temp("rev");
        let git = dir.join(".git");
        fs::create_dir_all(git.join("refs/heads")).unwrap();
        assert_eq!(engine_revision(&dir), None, "no HEAD yet");
        fs::write(git.join("HEAD"), format!("{sha}\n")).unwrap();
        assert_eq!(engine_revision(&dir).as_deref(), Some("0123456789ab"));
        fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(engine_revision(&dir), None, "dangling ref");
        fs::write(
            git.join("packed-refs"),
            format!("# pack-refs\n{sha} refs/heads/main\n"),
        )
        .unwrap();
        assert_eq!(engine_revision(&dir).as_deref(), Some("0123456789ab"));
        fs::write(
            git.join("refs/heads/main"),
            "fedcba9876543210fedcba9876543210fedcba98\n",
        )
        .unwrap();
        assert_eq!(
            engine_revision(&dir).as_deref(),
            Some("fedcba987654"),
            "a loose ref wins"
        );
        // A linked worktree: `.git` is a file pointing at the real git directory.
        let linked = temp("rev-linked");
        fs::create_dir_all(&linked).unwrap();
        fs::write(linked.join(".git"), format!("gitdir: {}\n", git.display())).unwrap();
        assert_eq!(engine_revision(&linked).as_deref(), Some("fedcba987654"));
        fs::write(git.join("HEAD"), "not a sha\n").unwrap();
        assert_eq!(engine_revision(&dir), None);
        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(linked).unwrap();
    }

    #[test]
    fn names_must_be_cargo_compatible_and_directories_empty() {
        let dir = temp("names");
        for bad in ["", "2048", "-x", "a b", "a/b", "é"] {
            assert!(scaffold_new_game(bad, &dir, None).is_err(), "{bad:?}");
        }
        assert!(!dir.exists(), "a rejected name creates nothing");
    }
}
