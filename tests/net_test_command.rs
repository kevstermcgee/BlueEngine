//! `be2-tools net-test` must mean what `ok` says: its invariants held for its own workload, and a broken
//! delivery or acknowledgement path cannot still pass.
use serde_json::Value;
use std::process::Command;

fn run(args: &[&str]) -> (bool, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_be2-tools"))
        .arg("net-test")
        .args(args)
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "net-test printed no report ({e}): {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.success(), report)
}

fn failed(report: &Value) -> Vec<String> {
    report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["passed"] == false)
        .map(|c| c["name"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn the_default_workload_is_verified_and_says_what_it_covers() {
    let (success, report) = run(&[]);
    assert!(success, "{report}");
    assert_eq!(report["ok"], true);
    assert_eq!(report["completed"], true);
    assert_eq!(report["verified"], true);
    assert!(failed(&report).is_empty());
    let names: Vec<_> = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    for required in [
        "packets_conserved",
        "delivery_progress",
        "acknowledgement_progress",
        "server_applied_inputs",
        "prediction_divergence_bounded",
    ] {
        assert!(names.contains(&required), "missing check {required}");
    }
    let transport = report["workload"]["transport"].as_str().unwrap();
    assert!(transport.contains("NetworkSimulator") && transport.contains("no sockets"));
    let not_tested = report["not_tested"].to_string();
    assert!(not_tested.contains("QUIC") && not_tested.contains("any game other than"));
    // Loss is checked against a tolerance band, never an exact packet count.
    let dropped = report["measurements"]["packets_dropped"].as_u64().unwrap();
    assert!((3..=9).contains(&dropped), "dropped {dropped}");
}

#[test]
fn fields_existing_callers_read_are_still_there() {
    let (_, report) = run(&[]);
    for key in [
        "simulated_ticks",
        "simulated_latency_ms",
        "simulated_packet_loss_rate",
        "dropped_packets",
        "reconciled_corrections",
        "final_position",
    ] {
        assert!(report.get(key).is_some(), "missing legacy field {key}");
    }
}

#[test]
fn a_network_that_delivers_nothing_fails_even_though_the_run_completes() {
    let (success, report) = run(&["--loss-rate=1"]);
    assert!(!success, "total loss must exit nonzero");
    assert_eq!(
        report["completed"], true,
        "the simulation itself ran to the end"
    );
    assert_eq!(report["verified"], false);
    assert_eq!(report["ok"], false);
    let failures = failed(&report);
    assert!(
        failures.contains(&"delivery_progress".to_owned()),
        "{failures:?}"
    );
    assert!(
        failures.contains(&"acknowledgement_progress".to_owned()),
        "{failures:?}"
    );
    assert!(
        failures.contains(&"server_applied_inputs".to_owned()),
        "{failures:?}"
    );
}

#[test]
fn a_delay_longer_than_the_run_cannot_pass_either() {
    let (success, report) = run(&["--latency-ms=2000"]);
    assert!(!success);
    assert!(failed(&report).contains(&"delivery_progress".to_owned()));
}

#[test]
fn harsher_but_workable_conditions_still_verify() {
    for args in [
        &["--loss-rate=0.3", "--latency-ms=300"][..],
        &["--loss-rate=0", "--latency-ms=0"],
        &["--ticks=600"],
    ] {
        let (success, report) = run(args);
        assert!(success, "{args:?}: {report}");
    }
}

#[test]
fn bad_options_are_refused() {
    for args in [
        &["--loss-rate=2"][..],
        &["--ticks=3"],
        &["--bogus=1"],
        &["--ticks"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_be2-tools"))
            .arg("net-test")
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{args:?} should be refused");
    }
}

#[test]
fn performance_commands_say_they_measured_the_built_in_fixture() {
    for command in ["validate-budget", "inspect-performance"] {
        let output = Command::new(env!("CARGO_BIN_EXE_be2-tools"))
            .arg(command)
            .output()
            .unwrap();
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            report["measured"]["kind"], "built-in engine fixture",
            "{command}"
        );
        assert_eq!(report["measured"]["user_selected_content"], false);
    }
}
