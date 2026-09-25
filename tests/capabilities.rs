use serde_json::Value;
use std::process::Command;
use vesper3d::viewer::capabilities::{features, COMMANDS};

fn native(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_be2-tools"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn discovery_and_parser_share_command_signatures() {
    let output = native(&["describe"]);
    assert!(output.status.success());
    let doc: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(doc["commands"].as_array().unwrap().len(), COMMANDS.len());
    for (name, sig) in COMMANDS {
        let max_args = sig.split_whitespace().count();
        let extras: Vec<&str> = (0..=max_args).map(|_| "extra").collect();
        let mut cmd_args = vec![*name];
        cmd_args.extend(extras);
        let output = native(&cmd_args);
        assert!(!output.status.success());
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert!(
            error["error"].as_str().unwrap().contains("expects"),
            "{name}"
        );
    }
    assert!(output.stdout.len() < 6000);
}

#[test]
fn search_is_bounded_and_reports_unsupported_topics_honestly() {
    let output = native(&["search", "multiplayer"]);
    assert!(output.status.success());
    let doc: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!doc["matches"].as_array().unwrap().is_empty());
    assert!(doc["matches"].as_array().unwrap().len() <= 10);
    assert!(!native(&["search", ""]).status.success());
    assert!(!native(&["search", &"x".repeat(101)]).status.success());
    let output = native(&["search", "nonexistent-quantum-engine"]);
    let doc: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(doc["total"], 0);
}

#[test]
fn feature_paths_and_evidence_resolve_to_suite_tests() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let data = features().unwrap();
    for (name, feature) in data["features"].as_object().unwrap() {
        for path in feature["files"].as_array().unwrap() {
            assert!(root.join(path.as_str().unwrap()).exists(), "{name}: {path}");
        }
        if let Some(evidence) = feature["evidence"].as_array() {
            for test in evidence {
                let file = root.join(format!("tests/{}.rs", test["suite"].as_str().unwrap()));
                let source = std::fs::read_to_string(file).unwrap();
                let signature = format!("fn {}(", test["test"].as_str().unwrap());
                assert!(source.contains(&signature), "{name}: missing {signature}");
            }
        }
    }
}

#[test]
fn exported_lab_loads_and_preserves_existing_output() {
    let path = std::env::temp_dir().join(format!("blue-lab-{}.json", std::process::id()));
    assert!(!path.exists());
    let output = native(&["export-lab", path.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(&path).unwrap();
    assert!(vesper3d::prelude::MapDocument::load(&path)
        .unwrap()
        .build()
        .is_ok());
    assert!(!native(&["export-lab", path.to_str().unwrap()])
        .status
        .success());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::remove_file(path).unwrap();
}
