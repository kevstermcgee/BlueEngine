#![cfg(feature = "narrative")]
#[path = "../examples/narrative/shared.rs"]
mod shared;
use shared::{Fixture, Input, Sim};
use vesper3d::viewer::{
    devkit::{self, snapshot, Simulation, Snapshot},
    narrative::{Command, Commands, Value},
};

fn route_input(sim: &Sim, tick: usize, route: &str, quiet: bool) -> Input {
    let mut input = shared::walk_inputs(tick);
    if let Some(scene) = sim.scene().unwrap() {
        input = Input::default();
        if scene.choices.is_empty() {
            input.advance = Some(scene.token);
        } else {
            input.choice = Some((scene.token, route.into()));
        }
    }
    if let Some(line) = sim.response().unwrap() {
        if quiet {
            input.interrupt = Some(line.token);
        } else if sim
            .narrative
            .as_ref()
            .unwrap()
            .state()
            .active_response
            .as_ref()
            .unwrap()
            .presented
        {
            input.complete = Some(line.token);
        } else {
            input.presented = Some(line.token);
        }
    }
    input
}
fn run(fixture: Fixture, route: &str, quiet: bool) -> (Sim, Vec<Input>) {
    let mut sim = Sim::new(fixture, 7).unwrap();
    let mut inputs = Vec::new();
    for tick in 0..340 {
        let input = route_input(&sim, tick, route, quiet);
        sim.step(&input);
        assert!(sim.notice.is_none(), "tick {tick}: {:?}", sim.notice);
        inputs.push(input);
    }
    (sim, inputs)
}
fn offer() -> Sim {
    let mut sim = Sim::new(Fixture::Scene, 7).unwrap();
    for tick in 0..195 {
        sim.step(&shared::walk_inputs(tick));
        assert!(sim.notice.is_none(), "tick {tick}: {:?}", sim.notice);
    }
    assert_eq!(sim.world.cells, 1, "real movement must collect raised cell");
    assert!(sim.scene().unwrap().is_some());
    sim
}
fn temp_source(name: &str, text: &str) -> std::path::PathBuf {
    let path =
        std::env::temp_dir().join(format!("blue-narrative-{}-{name}.poe", std::process::id()));
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn spatial_walk_choice_changes_collision_and_later_gameplay_callback() {
    let (share, _) = run(Fixture::Scene, "share", false);
    assert!(share.world.finished);
    assert!(share.world.gate_open);
    assert_eq!(share.world.cells, 0);
    assert_eq!(share.world.power_commits, 1);
    let state = share.narrative.as_ref().unwrap().state();
    assert_eq!(state.decisions.get("light.offer/offer/share"), Some(&1));
    assert_eq!(state.history.get("light.callback").unwrap().completed, 1);
    let (keep, _) = run(Fixture::Scene, "keep", false);
    assert!(!keep.world.gate_open);
    assert!(!keep.world.finished);
    assert_eq!(keep.world.cells, 1);
    assert_eq!(keep.world.power_commits, 0);
    assert!(!keep
        .narrative
        .as_ref()
        .unwrap()
        .state()
        .history
        .contains_key("light.callback"));
}
#[test]
fn no_narrative_and_muted_cosmetic_responses_preserve_gameplay() {
    let (plain, _) = run(Fixture::Storyless, "share", false);
    let (spoken, _) = run(Fixture::Reactive, "share", false);
    let (quiet, _) = run(Fixture::Reactive, "share", true);
    assert!(plain.narrative.is_none());
    assert!(plain.world.finished);
    assert_eq!(plain.world.state_hash(), spoken.world.state_hash());
    assert_eq!(spoken.world.state_hash(), quiet.world.state_hash());
    assert!(spoken
        .narrative
        .as_ref()
        .unwrap()
        .state()
        .history
        .values()
        .any(|h| h.completed > 0));
    assert!(quiet
        .narrative
        .as_ref()
        .unwrap()
        .state()
        .history
        .values()
        .all(|h| h.completed == 0));
}
#[test]
fn real_binding_routes_are_deterministic_and_resume_from_engine_snapshots() {
    let (_, inputs) = run(Fixture::Scene, "share", false);
    devkit::assert_deterministic(|| Sim::new(Fixture::Scene, 7).unwrap(), &inputs);
    snapshot::assert_resumes_exactly(|| Sim::new(Fixture::Scene, 7).unwrap(), &inputs, 29);
    let (_, plain) = run(Fixture::Storyless, "share", false);
    snapshot::assert_resumes_exactly(|| Sim::new(Fixture::Storyless, 7).unwrap(), &plain, 31);
}
#[test]
fn stale_choice_retry_interruption_and_commands_cannot_duplicate_debits() {
    let mut stale = offer();
    let token = stale.scene().unwrap().unwrap().token;
    stale.world.cells = 0;
    stale.step(&Input {
        choice: Some((token, "share".into())),
        ..Default::default()
    });
    assert!(stale.notice.as_deref().unwrap().contains("eligible"));
    assert!(!stale.world.gate_open);
    assert_eq!(stale.world.power_commits, 0);
    assert!(stale
        .narrative
        .as_ref()
        .unwrap()
        .state()
        .decisions
        .is_empty());
    let mut sim = offer();
    let token = sim.scene().unwrap().unwrap().token;
    let command = Command {
        id: "bridge.power".into(),
        value: Value::Bool(true),
        source: Default::default(),
    };
    let world_hash = sim.world.state_hash();
    assert!(sim.world.apply_atomic(&[command.clone(), command]).is_err());
    assert_eq!(world_hash, sim.world.state_hash());
    let choose = Input {
        choice: Some((token, "share".into())),
        ..Default::default()
    };
    sim.step(&choose);
    assert_eq!(sim.world.power_commits, 1);
    sim.step(&choose);
    assert!(sim.notice.as_deref().unwrap().contains("stale"));
    assert_eq!(sim.world.power_commits, 1);
    let thanks = sim.scene().unwrap().unwrap();
    sim.step(&Input {
        scene_interrupt: Some(thanks.token),
        ..Default::default()
    });
    assert!(sim.scene().unwrap().is_none());
    let bytes = snapshot::save(&sim, "interrupted after committed consequence").unwrap();
    let mut resumed = Sim::new(Fixture::Scene, 7).unwrap();
    snapshot::restore(&mut resumed, &bytes).unwrap();
    resumed.step(&choose);
    assert_eq!(resumed.world.power_commits, 1);
    assert_eq!(resumed.world.cells, 0);
}
#[test]
fn active_scene_owns_input_even_if_its_speaker_disappears() {
    let mut sim = offer();
    let before = sim.world.player.network_state();
    let token = sim
        .narrative
        .as_ref()
        .unwrap()
        .state()
        .scene
        .as_ref()
        .unwrap()
        .token;
    sim.world.speaker_present = false;
    assert!(sim.scene().is_err());
    sim.step(&Input {
        forward: 1.,
        right: 1.,
        jump: true,
        interact: true,
        ..Default::default()
    });
    assert_eq!(before, sim.world.player.network_state());
    sim.step(&Input {
        scene_interrupt: Some(token),
        ..Default::default()
    });
    assert!(sim.narrative.as_ref().unwrap().state().scene.is_none());
    let mut bark = Sim::new(Fixture::Reactive, 7).unwrap();
    bark.world.speaker_present = false;
    bark.step(&Input::default());
    assert!(bark.response().unwrap().is_none());
}
#[test]
fn failed_reload_keeps_previous_content_and_compatible_reload_never_replays_effects() {
    let mut sim = offer();
    let token = sim.scene().unwrap().unwrap().token;
    sim.step(&Input {
        choice: Some((token, "share".into())),
        ..Default::default()
    });
    let state_hash = sim.state_hash();
    let invalid = temp_source("invalid", "poe 999\n");
    assert!(sim.reload(&invalid).is_err());
    assert_eq!(state_hash, sim.state_hash());
    let original = std::fs::read_to_string(Fixture::Scene.source().unwrap()).unwrap();
    let compatible = temp_source(
        "prose",
        &original.replace(
            "There. Follow the rail across.",
            "Good. Follow the rail across.",
        ),
    );
    let content = sim.content();
    sim.reload(&compatible).unwrap();
    assert_eq!(content, sim.content());
    assert_eq!(sim.world.power_commits, 1);
    assert!(sim.scene().unwrap().unwrap().text.starts_with("Good."));
    let incompatible = temp_source(
        "effects",
        &original.replace("do bridge.power true", "do bridge.power false"),
    );
    assert!(sim.reload(&incompatible).unwrap_err().contains("semantics"));
    assert_eq!(sim.world.power_commits, 1);
    for path in [invalid, compatible, incompatible] {
        std::fs::remove_file(path).unwrap();
    }
}
#[test]
fn save_compatibility_rejects_changed_semantics_and_capability_removal() {
    let sim = offer();
    let bytes = snapshot::save(&sim, "before choice").unwrap();
    let original = std::fs::read_to_string(Fixture::Scene.source().unwrap()).unwrap();
    let changed = temp_source(
        "save-semantics",
        &original.replace("do bridge.power true", "do bridge.power false"),
    );
    let mut changed_sim = Sim::with_source(Fixture::Scene, 7, Some(&changed)).unwrap();
    assert!(snapshot::restore(&mut changed_sim, &bytes).is_err());
    let mut plain = Sim::new(Fixture::Storyless, 7).unwrap();
    assert!(snapshot::restore(&mut plain, &bytes).is_err());
    std::fs::remove_file(changed).unwrap();
}
