//! Files Cargo will find inside this repository must be valid manifests.
//!
//! Cargo reads every file named `Cargo.toml` when it looks for a package inside a Git dependency, so a
//! template manifest with a `{{placeholder}}` in it makes a pinned Git dependency on the engine print a parse
//! error (and fail on older toolchains). Templates therefore keep their manifest under another name.
use std::path::{Path, PathBuf};

fn manifests(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            manifests(&path, found);
        } else if path.file_name().is_some_and(|n| n == "Cargo.toml") {
            found.push(path);
        }
    }
}

#[test]
fn no_manifest_cargo_can_see_in_templates_contains_a_placeholder() {
    let mut found = Vec::new();
    manifests(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"),
        &mut found,
    );
    // The retired example remains a Cargo consumer, but is no longer a starter.
    let archived =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/archive/multiplayer-game/Cargo.toml");
    assert!(archived.is_file(), "preserve the archived example manifest");
    found.push(archived);
    for path in found {
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            !text.contains("{{"),
            "{} has a placeholder; keep templated manifests under another name (Cargo.toml.tmpl)",
            path.display()
        );
    }
}

#[test]
fn the_custom_sim_template_is_still_scaffolded_with_its_engine_path_filled_in() {
    let dir = std::env::temp_dir().join(format!("be2-template-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_be2-tools"))
        .args([
            "new-game",
            "tmpl-check",
            dir.to_str().unwrap(),
            "../engine",
            "custom-sim",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    assert!(!manifest.contains("{{"), "{manifest}");
    assert!(manifest.contains("../engine"), "{manifest}");
}
