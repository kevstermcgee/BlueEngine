use std::path::Path;
use vesper3d::math::V;
use vesper3d::viewer::{
    authoring::MapDocument,
    blueprint::{compile_blueprint, BlueprintSpec, FillSpec, RoomSpec, SpawnSpec},
    gen::{line, scatter},
    lint::lint_map,
    maps::MapId,
    pathing::execute_walk,
    reach::analyze_reach,
    scenario::{run_scenario, verify_replay_trace, PlayerConfig, Scenario, TimedInput},
    symbols::SourceIndex,
    ui_check::audit_all_screens,
    verify::{verify_map, ChecksBlock, LintCheck},
};

#[test]
fn test_blueprint_compiler_and_spatial_graph() {
    let spec = BlueprintSpec {
        name: "Test Blueprint".into(),
        height: 3.2,
        wall_thickness: 0.2,
        rooms: vec![RoomSpec {
            id: "main_hall".into(),
            rect: [-5.0, -5.0, 5.0, 5.0],
            floor_color: Some([0.3, 0.4, 0.5]),
            wall_color: None,
            lamp: true,
        }],
        doors: vec![],
        spawns: vec![SpawnSpec {
            id: "spawn_p1".into(),
            room: "main_hall".into(),
            offset: Some([0.0, 0.0]),
        }],
        fill: vec![FillSpec {
            room: "main_hall".into(),
            kind: "chair".into(),
            count: 3,
            seed: 42,
        }],
    };

    let doc = compile_blueprint(&spec).expect("Blueprint should compile");
    assert_eq!(doc.name, "Test Blueprint");
    assert!(
        doc.spatial.is_some(),
        "Blueprint must generate spatial RoomGraph"
    );
    assert!(!doc.colliders.is_empty(), "Must have room colliders");
    assert_eq!(doc.entities.len(), 4, "1 spawn + 3 chairs");

    // Lint the generated map
    let lint_res = lint_map(&doc, false, &[]);
    assert_eq!(
        lint_res.errors, 0,
        "Blueprint maps should have 0 lint errors"
    );
}

#[test]
fn test_static_lint_and_reachability() {
    let lab = MapDocument::from_map(MapId::TestLab).expect("TestLab must load");

    // Lint
    let lint_res = lint_map(&lab, false, &[]);
    assert_eq!(lint_res.errors, 0, "TestLab should have 0 lint errors");

    // Reachability
    let reach_res = analyze_reach(&lab, None);
    assert!(reach_res.ok, "Reachability analysis should succeed");
    assert!(
        reach_res.reachable_cells > 0,
        "Should have reachable navigation cells"
    );
    assert!(
        !reach_res.drop_hazards.is_empty(),
        "TestLab contains elevated platforms with drop edges"
    );
    assert!(
        reach_res.perimeter_leaks.is_empty(),
        "TestLab should have no perimeter leaks"
    );
}

#[test]
fn test_pathing_and_physical_walk() {
    let lab = MapDocument::from_map(MapId::TestLab).expect("TestLab must load");

    // Walk a short clear path on floor
    let walk_res = execute_walk(&lab, V(0.0, 0.0, 0.0), V(1.5, 0.0, 1.5), None);
    assert!(walk_res.ok, "Clear walk in TestLab should succeed");
    assert!(walk_res.distance_m > 1.0, "Should record traveled distance");
    assert!(walk_res.ticks > 0, "Should record simulation ticks");

    // Walk into a solid boundary wall to test blocker diagnostics
    let blocked_res = execute_walk(&lab, V(0.0, 0.0, 0.0), V(50.0, 0.0, 0.0), None);
    assert!(!blocked_res.ok, "Walking outside boundary should fail");
}

#[test]
fn test_procedural_scatter_and_line() {
    let lab = MapDocument::from_map(MapId::TestLab).expect("TestLab must load");
    let initial_colliders = lab.colliders.len();

    // Scatter 4 props
    let (scattered, placed) = scatter(lab, "chair", 4, [-3.0, -3.0, 3.0, 3.0], 12345, None)
        .expect("Scatter should succeed");
    assert_eq!(placed, 4, "Should place all 4 chairs");
    assert_eq!(scattered.colliders.len(), initial_colliders + 4);

    // Line 3 props
    let lined = line(scattered, "cereal", 3, [-2.0, -2.0], [2.0, 2.0], None)
        .expect("Line placement should succeed");
    assert_eq!(lined.colliders.len(), initial_colliders + 7);
}

#[test]
fn test_deterministic_simulation_and_replay_verification() {
    let scenario = Scenario {
        name: "Test Determinism".into(),
        game_path: None,
        ticks: 60,
        players: vec![PlayerConfig {
            id: 1,
            spawn: Some([0.0, 0.0, 0.0]),
        }],
        inputs: vec![
            TimedInput {
                tick: 0,
                player: 1,
                forward: 1.0,
                right: 0.0,
                yaw: 0.0,
                pitch: 0.0,
                sprint: false,
                jump: false,
                crouch: false,
                interact: false,
            },
            TimedInput {
                tick: 30,
                player: 1,
                forward: 0.0,
                right: 0.0,
                yaw: 0.0,
                pitch: 0.0,
                sprint: false,
                jump: false,
                crouch: false,
                interact: false,
            },
        ],
        assertions: vec![],
    };

    let (trace, error) = run_scenario(&scenario).expect("Scenario should run");
    assert!(error.is_none(), "Should have no scenario assertion error");
    assert_eq!(trace.total_ticks, 60);
    assert!(!trace.checkpoints.is_empty());

    let divergence = verify_replay_trace(&trace, None).expect("Replay trace check should run");
    assert!(
        divergence.deterministic,
        "Simulation replay must be 100% bit-for-bit deterministic"
    );
    assert_eq!(divergence.verified_checkpoints, trace.checkpoints.len());
}

#[test]
fn test_verification_framework() {
    let lab = MapDocument::from_map(MapId::TestLab).expect("TestLab must load");
    let mut checks = ChecksBlock::default();
    checks.lint = Some(LintCheck {
        max_errors: 0,
        max_warnings: 10,
        forbid: vec!["overlap_critical".into()],
    });

    let report = verify_map(&lab, &checks, "TestLab");
    assert!(report.ok, "Verification of TestLab should pass");
    assert_eq!(report.failed, 0);
}

#[test]
fn test_source_code_navigator() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let index = SourceIndex::scan(root).expect("Source index scan should succeed");

    let outline = index.outline("src/viewer/capabilities.rs");
    assert!(
        !outline.is_empty(),
        "Outline should find functions and constants in capabilities.rs"
    );

    let find_matches = index.find("COMMANDS");
    assert!(!find_matches.is_empty(), "Find should locate COMMANDS");

    let refs = index.refs("COMMANDS");
    assert!(
        !refs.is_empty(),
        "Refs should locate references to COMMANDS"
    );
}

#[test]
fn test_headless_ui_layout_auditor() {
    let report = audit_all_screens();
    assert!(
        report.ok,
        "All UI screens across 9 standard resolutions should pass layout audit"
    );
    assert!(
        report.violations.is_empty(),
        "Violations: {:?}",
        report.violations
    );
    assert_eq!(
        report.total_tests, 45,
        "5 screens * 9 resolutions = 45 permutations"
    );
}
