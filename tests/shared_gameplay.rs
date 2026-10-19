//! External callers use only the public engine API; no executable-source imports.
use vesper3d::{
    prelude::*,
    viewer::{controller::CharacterKind, creative, game::GameDocument, newgame::scaffold_new_game},
};
fn base() -> MapDocument {
    SceneBuilder::new("Base")
        .spawn(V(0., 0., 8.), 0.)
        .structural_box("floor", V(0., -0.1, 0.), V(20., 0.1, 20.), V::ONE)
        .build()
        .unwrap()
}
#[test]
fn arbitrary_asset_roots_place_and_remove_without_copied_sandbox_code() {
    let source = SceneBuilder::new("Kit")
        .spawn(V(0., 0., 8.), 0.)
        .structural_box("floor", V(0., -0.1, 0.), V(20., 0.1, 20.), V::ONE)
        .box_body("crate", V(0., 0.5, 0.), V(0.5, 0.5, 0.5), V::ONE)
        .build()
        .unwrap();
    let asset = creative::extract(source, "crate").unwrap();
    let base = base();
    let player = Controller::for_character_at(CharacterKind::Scientist, V(0., 0., 8.), 0.).unwrap();
    let edited = creative::place(&base, &asset, V(5., 0., 0.), 1, &player).unwrap();
    assert!(edited.entities.iter().any(|e| e.id == "creative-1"));
    let restored = creative::remove(&edited, "creative-1").unwrap();
    assert_eq!(
        serde_json::to_value(restored).unwrap(),
        serde_json::to_value(base).unwrap()
    );
}
#[test]
fn history_and_save_paths_have_bounds() {
    let mut history = creative::History::default();
    for _ in 0..20 {
        history.remember(base());
    }
    let mut count = 0;
    while history.pop().is_some() {
        count += 1;
    }
    assert_eq!(count, 16);
    for id in ["", "../escape", "a/b", "a\\b", "C:drive"] {
        assert!(creative::save_path(std::path::Path::new("saves"), id).is_err());
    }
    assert_eq!(
        creative::save_path(std::path::Path::new("saves"), "world-1").unwrap(),
        std::path::Path::new("saves/world-1.json")
    );
}
#[test]
fn generated_project_has_a_valid_game_document_and_shared_playable_entry() {
    let dir = std::env::temp_dir().join(format!(
        "blueengine-shared-starter-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    scaffold_new_game("shared-starter", &dir, Some("../BlueEngine")).unwrap();
    GameDocument::load(&dir.join("game.json"))
        .unwrap()
        .world()
        .unwrap();
    let main = std::fs::read_to_string(dir.join("src/main.rs")).unwrap();
    assert!(main.contains("playable::run_game_with_options"));
    assert!(!main.contains("src/bin/sandbox"));
    let installer: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("scripts/installer.json")).unwrap())
            .unwrap();
    assert_eq!(installer["app_id"], "BlueEngineGames.shared-starter");
    assert!(std::fs::read_to_string(dir.join(".gitignore"))
        .unwrap()
        .contains("/installers/"));
    let guide = std::fs::read_to_string(dir.join("AGENTS.md")).unwrap();
    assert!(guide.len() < 3000);
    assert!(guide.contains("--content-only"));
    assert_eq!(
        std::fs::read_to_string(dir.join("CLAUDE.md")).unwrap(),
        "@AGENTS.md\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("scripts/check.py")).unwrap(),
        vesper3d::viewer::newgame::tooling_entry("check")
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("scripts/ship.py")).unwrap(),
        vesper3d::viewer::newgame::tooling_entry("ship")
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("build.rs")).unwrap(),
        include_str!("../templates/game_build.rs")
    );
    let windows = std::fs::read_to_string(dir.join("scripts/blue.ps1")).unwrap();
    assert!(windows.contains("exit $LASTEXITCODE"));
    assert!(windows.contains("python scripts/check.py @CheckArgs"));
    let unix = std::fs::read_to_string(dir.join("scripts/blue")).unwrap();
    assert!(
        unix.contains("command -v python3 || command -v python"),
        "python3 is preferred, python is the fallback"
    );
    assert!(unix.contains("\"$PY\" scripts/check.py \"$@\""));
    assert!(!unix.contains("cargo check"));
    assert!(unix.contains("\"$PY\" scripts/dev.py"));
    assert!(windows.contains("python scripts/dev.py"));
    assert!(
        !windows.contains("\"$PY\""),
        "PowerShell must use its own Python command"
    );
    #[cfg(unix)]
    {
        let launched = std::process::Command::new(dir.join("scripts/blue"))
            .arg("help")
            .output()
            .expect("the documented generated launcher must be directly executable");
        assert!(launched.status.success());
        assert!(String::from_utf8_lossy(&launched.stdout).contains("Usage: scripts/blue"));
    }
    assert_eq!(
        std::fs::read_to_string(dir.join("scripts/dev.py")).unwrap(),
        vesper3d::viewer::newgame::tooling_entry("dev")
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("scripts/project.py")).unwrap(),
        vesper3d::viewer::newgame::tooling_entry("project")
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("scripts/tooling.py")).unwrap(),
        include_str!("../templates/game_entry.py")
    );
    assert!(scaffold_new_game("shared-starter", &dir, None).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn compiled_bootstrap_runs_generated_launchers_and_matches_live_digest() {
    use vesper3d::viewer::newgame::{scaffold_new_game_with, Template};
    let engine = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = std::env::temp_dir().join(format!(
        "blueengine-bootstrap-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    scaffold_new_game_with(
        "bootstrap-probe",
        &dir,
        engine.to_str(),
        Template::CustomSim,
    )
    .unwrap();
    let python = if cfg!(windows) { "python" } else { "python3" };
    // Each emitted launcher loads the embedded bootstrap and checks the pin
    // produced by Rust against Python's current ordered digest inputs.
    for command in ["check", "ship", "dev", "project"] {
        let mut process = std::process::Command::new(python);
        if matches!(command, "check" | "ship") {
            process
                .arg(dir.join(format!("scripts/{command}.py")))
                .arg("--help");
        } else {
            // dev forwards arguments to Cargo rather than implementing --help.
            // Import its actual generated entry without starting a game/build.
            process
                .args(["-c", "import runpy,sys; runpy.run_path(sys.argv[1])"])
                .arg(dir.join(format!("scripts/{command}.py")));
        }
        let output = process
            .env_remove("BE2_TOOLING_ROOT")
            .current_dir(engine)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{command}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let pin_path = dir.join("scripts/tooling.json");
    let mut pin: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&pin_path).unwrap()).unwrap();
    assert!(pin["sha256"]
        .as_str()
        .is_some_and(|value| value.len() == 64));
    pin["sha256"] = "0".repeat(64).into();
    std::fs::write(&pin_path, serde_json::to_vec(&pin).unwrap()).unwrap();
    let output = std::process::Command::new(python)
        .arg(dir.join("scripts/check.py"))
        .arg("--help")
        .env_remove("BE2_TOOLING_ROOT")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("tooling content differs from the pin")
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn all_starters_emit_native_requirements_and_no_browser_execution_path() {
    use vesper3d::viewer::newgame::{scaffold_new_game_with, Template};
    for (template, presentation) in [
        (Template::Stock, "3d"),
        (Template::CustomSim, "3d"),
        (Template::TwoD, "2d"),
        (Template::ThreeD, "3d"),
        (Template::Hybrid, "hybrid"),
        (Template::Portable, "hybrid"),
    ] {
        let dir = std::env::temp_dir().join(format!(
            "blueengine-native-starter-{}-{}-{}",
            template.name(),
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        scaffold_new_game_with("native-starter", &dir, Some("../BlueEngine"), template).unwrap();
        let project: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("game.project.json")).unwrap()).unwrap();
        assert_eq!(project["presentation"], presentation);
        assert_eq!(
            project["runtime"],
            if template.is_portable() {
                "portable"
            } else {
                "legacy-native"
            }
        );
        assert_eq!(project["targets"], serde_json::json!(["windows"]));
        assert!(!dir.join("scripts/web.py").exists());
        assert!(dir.join("scripts/project.py").is_file());
        let guide = std::fs::read_to_string(dir.join("AGENTS.md")).unwrap();
        assert!(guide.contains("ship --no-install"));
        assert!(!guide.contains("scripts/blue web build"));
        if template.is_portable() {
            assert!(std::fs::read_to_string(dir.join("src/lib.rs"))
                .unwrap()
                .contains("impl GameLogic"));
        }
        #[cfg(unix)]
        {
            let rejected = std::process::Command::new(dir.join("scripts/blue"))
                .arg("web")
                .arg("prepare")
                .output()
                .unwrap();
            assert_eq!(rejected.status.code(), Some(2));
            assert!(String::from_utf8_lossy(&rejected.stderr).contains("retired"));
            assert!(!dir.join("dist").exists());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn requirements_ids_preserve_the_public_cargo_name_contract() {
    use vesper3d::viewer::newgame::{scaffold_new_game_with, Template};
    for (template, name, id) in [
        (Template::Stock, "My_Game", "my-game"),
        (Template::CustomSim, "_hidden", "game--hidden"),
        (
            Template::Portable,
            "abcdefghijklmnopqrstuvwxyz-abcdefghijklmnopqrstuvwxyz",
            "abcdefghijklmnopqrstuvwxyz-abcdefghijklmnopqrstu",
        ),
    ] {
        let dir = std::env::temp_dir().join(format!(
            "blueengine-cargo-name-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        scaffold_new_game_with(name, &dir, None, template).unwrap();
        let project: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("game.project.json")).unwrap()).unwrap();
        assert_eq!(project["id"], id);
        assert!(std::fs::read_to_string(dir.join("Cargo.toml"))
            .unwrap()
            .contains(&format!("name = \"{name}\"")));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
