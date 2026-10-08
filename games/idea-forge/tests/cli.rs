use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_idea-forge-cli"))
        .args(args)
        .output()
        .expect("run headless utility")
}

#[test]
fn seeded_json_batch_is_filtered_distinct_and_story_is_opt_in() {
    let args = [
        "--seed", "42", "--count", "12", "--genre", "puzzle", "--format", "json",
    ];
    let first = cli(&args);
    assert!(first.status.success());
    assert_eq!(first.stdout, cli(&args).stdout);
    let batch: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    let ideas = batch["ideas"].as_array().unwrap();
    assert_eq!(ideas.len(), 12);
    let mut ids = std::collections::BTreeSet::new();
    for idea in ideas {
        assert_eq!(idea["genre"], "puzzle");
        assert!(idea.get("story").is_none());
        assert!(ids.insert(idea["id"].as_str().unwrap()));
        assert!(idea["loop"].as_str().unwrap().len() > 10);
    }
    let output = cli(&["--count", "1", "--story", "--format", "json"]);
    let with_story: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(with_story["ideas"][0]["story"].as_str().unwrap().len() > 10);
}

#[test]
fn invalid_requests_fail_without_partial_output() {
    for args in [
        vec!["--count", "37"],
        vec!["--genre", "strategy", "--count", "13"],
        vec!["--count", "0"],
        vec!["--seed", "-1"],
        vec!["--genre", "unknown"],
        vec!["--format", "html"],
        vec!["--count"],
        vec!["--unknown", "value"],
    ] {
        let output = cli(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}

#[test]
fn export_never_overwrites_an_existing_file() {
    let path = std::env::temp_dir().join(format!("idea-forge-cli-{}-keep.txt", std::process::id()));
    std::fs::write(&path, "keep my existing work").unwrap();
    let output = cli(&["--output", path.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "keep my existing work"
    );
    std::fs::remove_file(path).unwrap();
}
