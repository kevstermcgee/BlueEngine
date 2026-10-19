//! Exercise the real CLI/report boundary under the shared worker's Linux address-space budget.
#[cfg(all(target_os = "linux", not(feature = "client")))]
#[test]
fn maximum_length_session_reports_within_128_mib() {
    use std::{
        path::Path,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let project = Path::new(env!("CARGO_MANIFEST_DIR"));
    let runner = project.join("../../tools/playtest.py");
    let evidence = project.join(".blue-check").join(format!(
        "playtest-cli-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    ));
    let output = Command::new("python3")
        .arg(runner)
        .args([
            "--binary",
            env!("CARGO_BIN_EXE_workshop"),
            "--seeds",
            "42",
            "--ticks",
            "100000",
            "--concurrency",
            "1",
            "--memory-mib",
            "128",
            "--seconds",
            "300",
            "--total-seconds",
            "330",
            "--out",
        ])
        .arg(&evidence)
        .output()
        .expect("run shared bounded playtest worker");
    assert!(
        output.status.success(),
        "real CLI exceeded its budget or failed; evidence retained at {}: {} {}",
        evidence.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).expect("structured runner stdout");
    assert_eq!(stdout["ok"], true);
    assert!(output.stderr.is_empty(), "unexpected runner stderr; evidence at {}", evidence.display());
    assert!(
        std::fs::read(evidence.join("42/stderr.log")).unwrap().is_empty(),
        "unexpected game stderr; evidence retained at {}",
        evidence.display()
    );
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(evidence.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["ok"], true);
    assert_eq!(report["runs"][0]["ticks"], 100000);
    assert!(report["runs"][0]["coverage"]["reloads"].as_u64().unwrap() > 3000);
    assert!(report["runs"][0]["coverage"]["migrations"].as_u64().unwrap() >= 100);
    // Leave failures intact. Successful test-owned evidence has no player data or prior outputs.
    std::fs::remove_dir_all(evidence).unwrap();
}
