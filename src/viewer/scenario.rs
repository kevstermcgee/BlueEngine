//! Scripted scenario runner and deterministic replay divergence debugger for BlueEngine.
//!
//! Executes multi-agent gameplay scenarios headlessly against `HeadlessWorld` / `GameDocument`,
//! validates timed assertions, and when replaying traces, pinpoints the *exact first divergent tick*
//! with a field-level state diff.

use super::{controller::Movement, net::PlayerNetState, simulation::HeadlessWorld};
use crate::{math::V, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerConfig {
    pub id: u64,
    #[serde(default)]
    pub spawn: Option<[f32; 3]>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimedInput {
    pub tick: u64,
    pub player: u64,
    #[serde(default)]
    pub forward: f32,
    #[serde(default)]
    pub right: f32,
    #[serde(default)]
    pub yaw: f32,
    #[serde(default)]
    pub pitch: f32,
    #[serde(default)]
    pub sprint: bool,
    #[serde(default)]
    pub jump: bool,
    #[serde(default)]
    pub crouch: bool,
    #[serde(default)]
    pub interact: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assertion {
    pub tick: u64,
    #[serde(default)]
    pub player: Option<u64>,
    #[serde(default)]
    pub position_near: Option<[f32; 3]>,
    #[serde(default = "default_tolerance")]
    pub tolerance: f32,
    #[serde(default)]
    pub counter: Option<String>,
    #[serde(default)]
    pub counter_equals: Option<i32>,
    #[serde(default)]
    pub completed_equals: Option<bool>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub enabled_equals: Option<bool>,
    #[serde(default)]
    pub visible_equals: Option<bool>,
}

fn default_tolerance() -> f32 {
    0.35
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    #[serde(default)]
    pub game_path: Option<String>,
    #[serde(default = "default_scenario_ticks")]
    pub ticks: u64,
    pub players: Vec<PlayerConfig>,
    #[serde(default)]
    pub inputs: Vec<TimedInput>,
    #[serde(default)]
    pub assertions: Vec<Assertion>,
}

fn default_scenario_ticks() -> u64 {
    120
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checkpoint {
    pub tick: u64,
    pub checksum: u64,
    pub players: Vec<PlayerNetState>,
    #[serde(default)]
    pub counters: HashMap<String, i32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SimulationTrace {
    pub schema_version: u32,
    pub scenario_name: String,
    pub total_ticks: u64,
    pub initial_checksum: u64,
    #[serde(default)]
    pub game_path: Option<String>,
    pub checkpoints: Vec<Checkpoint>,
    pub inputs: Vec<TimedInput>,
    #[serde(default)]
    pub players: Vec<PlayerConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerDiff {
    pub id: u64,
    pub expected_pos: [f32; 3],
    pub actual_pos: [f32; 3],
    pub pos_delta_m: f32,
    pub yaw_delta: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DivergenceReport {
    pub deterministic: bool,
    pub total_ticks: u64,
    pub verified_checkpoints: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_divergent_tick: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_checksum: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_checksum: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_diffs: Option<Vec<PlayerDiff>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssertionOutcome {
    pub tick: u64,
    pub ok: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScenarioRunReport {
    pub ok: bool,
    pub trace: SimulationTrace,
    pub assertions: Vec<AssertionOutcome>,
}

/// Load a scenario and resolve its game path relative to the scenario file.
pub fn load_scenario(path: &Path) -> Result<Scenario> {
    let mut scenario: Scenario = serde_json::from_slice(&std::fs::read(path)?)?;
    if let Some(game_path) = &scenario.game_path {
        let game_path = Path::new(game_path);
        if game_path.is_relative() {
            let parent = path.parent().unwrap_or_else(|| Path::new("."));
            scenario.game_path = Some(parent.join(game_path).to_string_lossy().into_owned());
        }
    }
    Ok(scenario)
}

/// Run every tick and return structured outcomes for all declared assertions.
pub fn evaluate_scenario(scenario: &Scenario) -> Result<ScenarioRunReport> {
    let mut world = if let Some(p) = &scenario.game_path {
        let doc = super::game::GameDocument::load(Path::new(p))?;
        doc.world()?
    } else {
        HeadlessWorld::new()?
    };

    for p in &scenario.players {
        if let Some(pos) = p.spawn {
            world.join_at(p.id, V(pos[0], pos[1], pos[2]));
        } else {
            world.join(p.id);
        }
    }

    let initial_checksum = world.checksum();
    let mut checkpoints = Vec::new();
    let mut input_idx = 0;
    let mut inputs_sorted = scenario.inputs.clone();
    inputs_sorted.sort_by_key(|i| i.tick);
    let mut assertion_outcomes = scenario
        .assertions
        .iter()
        .filter(|assertion| assertion.tick == 0 || assertion.tick > scenario.ticks)
        .map(|assertion| AssertionOutcome {
            tick: assertion.tick,
            ok: false,
            detail: format!("assertion tick must be between 1 and {}", scenario.ticks),
        })
        .collect::<Vec<_>>();

    for tick in 1..=scenario.ticks {
        while input_idx < inputs_sorted.len() && inputs_sorted[input_idx].tick <= tick {
            let inp = &inputs_sorted[input_idx];
            let mv = Movement {
                forward: inp.forward,
                right: inp.right,
                sprint: inp.sprint,
                jump: inp.jump,
                crouch: inp.crouch,
            };
            world.input(inp.player, mv, inp.yaw, inp.pitch);
            if inp.interact {
                world.request_interaction(inp.player);
            }
            input_idx += 1;
        }

        world.step();

        // Evaluate every assertion at this tick without hiding later evidence.
        for assert in &scenario.assertions {
            if assert.tick == tick {
                let mut failures = Vec::new();
                if let Some(pid) = assert.player {
                    let snap = world.snapshot(tick);
                    if let Some(p_state) = snap.players.iter().find(|p| p.id == pid) {
                        if let Some(expected_pos) = assert.position_near {
                            let actual = p_state.position;
                            let dist = ((actual.0 - expected_pos[0]).powi(2)
                                + (actual.1 - expected_pos[1]).powi(2)
                                + (actual.2 - expected_pos[2]).powi(2))
                            .sqrt();
                            if dist > assert.tolerance {
                                failures.push(format!(
                                    "player {pid} position differs by {dist:.2}m (tolerance {:.2}m)",
                                    assert.tolerance
                                ));
                            }
                        }
                    } else {
                        failures.push(format!("player {pid} not found"));
                    }
                }

                if let Some(c_name) = &assert.counter {
                    if let Some(game) = &world.game {
                        let idx = game.document().counters.keys().position(|k| k == c_name);
                        if let Some(val) = idx.and_then(|i| game.state().counters.get(i).copied()) {
                            if let Some(expected) = assert.counter_equals {
                                if val != expected {
                                    failures.push(format!(
                                        "counter '{c_name}' is {val}, expected {expected}"
                                    ));
                                }
                            }
                        } else {
                            failures.push(format!("counter '{c_name}' not found"));
                        }
                    } else {
                        failures.push(format!("counter '{c_name}' requires a game"));
                    }
                }

                if let Some(expected) = assert.completed_equals {
                    match &world.game {
                        Some(game) if game.state().completed != expected => failures.push(format!(
                            "completed is {}, expected {expected}",
                            game.state().completed
                        )),
                        None => failures.push("completed assertion requires a game".into()),
                        _ => {}
                    }
                }

                if let Some(target) = &assert.target {
                    match &world.game {
                        Some(game) => {
                            if let Some(expected) = assert.enabled_equals {
                                match game.enabled_entity(target) {
                                    Some(actual) if actual != expected => failures.push(format!(
                                        "target '{target}' enabled is {actual}, expected {expected}"
                                    )),
                                    None => failures.push(format!("target '{target}' not found")),
                                    _ => {}
                                }
                            }
                            if let Some(expected) = assert.visible_equals {
                                match game.visible_entity(target) {
                                    Some(actual) if actual != expected => failures.push(format!(
                                        "target '{target}' visible is {actual}, expected {expected}"
                                    )),
                                    None => failures.push(format!("target '{target}' not found")),
                                    _ => {}
                                }
                            }
                        }
                        None => failures.push(format!("target '{target}' requires a game")),
                    }
                }

                assertion_outcomes.push(AssertionOutcome {
                    tick,
                    ok: failures.is_empty(),
                    detail: if failures.is_empty() {
                        "passed".into()
                    } else {
                        failures.join("; ")
                    },
                });
            }
        }

        if tick % 15 == 0 || tick == scenario.ticks {
            let snap = world.snapshot(tick);
            let mut counters = HashMap::new();
            if let Some(game) = &world.game {
                for (i, name) in game.document().counters.keys().enumerate() {
                    if let Some(&val) = game.state().counters.get(i) {
                        counters.insert(name.clone(), val);
                    }
                }
            }
            checkpoints.push(Checkpoint {
                tick,
                checksum: world.checksum(),
                players: snap.players,
                counters,
            });
        }
    }

    let trace = SimulationTrace {
        schema_version: 1,
        scenario_name: scenario.name.clone(),
        total_ticks: scenario.ticks,
        initial_checksum,
        game_path: scenario.game_path.clone(),
        checkpoints,
        inputs: scenario.inputs.clone(),
        players: scenario.players.clone(),
    };

    Ok(ScenarioRunReport {
        ok: assertion_outcomes.iter().all(|outcome| outcome.ok),
        trace,
        assertions: assertion_outcomes,
    })
}

/// Compatibility API: failed assertions remain errors for existing callers.
pub fn run_scenario(scenario: &Scenario) -> Result<(SimulationTrace, Option<String>)> {
    let report = evaluate_scenario(scenario)?;
    if let Some(failure) = report.assertions.iter().find(|outcome| !outcome.ok) {
        return Err(format!(
            "Assertion failed at tick {}: {}",
            failure.tick, failure.detail
        )
        .into());
    }
    Ok((report.trace, None))
}

pub fn verify_replay_trace(
    trace: &SimulationTrace,
    game_path: Option<&str>,
) -> Result<DivergenceReport> {
    let game_path = game_path.or(trace.game_path.as_deref());
    let mut replay_world = if let Some(p) = game_path {
        let doc = super::game::GameDocument::load(Path::new(p))?;
        doc.world()?
    } else {
        HeadlessWorld::new()?
    };

    // Join players
    if !trace.players.is_empty() {
        for p in &trace.players {
            if let Some(pos) = p.spawn {
                replay_world.join_at(p.id, V(pos[0], pos[1], pos[2]));
            } else {
                replay_world.join(p.id);
            }
        }
    } else if let Some(first_cp) = trace.checkpoints.first() {
        for p in &first_cp.players {
            replay_world.join(p.id);
        }
    }

    let mut input_idx = 0;
    let mut inputs_sorted = trace.inputs.clone();
    inputs_sorted.sort_by_key(|i| i.tick);
    let mut verified = 0;

    for tick in 1..=trace.total_ticks {
        while input_idx < inputs_sorted.len() && inputs_sorted[input_idx].tick <= tick {
            let inp = &inputs_sorted[input_idx];
            let mv = Movement {
                forward: inp.forward,
                right: inp.right,
                sprint: inp.sprint,
                jump: inp.jump,
                crouch: inp.crouch,
            };
            replay_world.input(inp.player, mv, inp.yaw, inp.pitch);
            if inp.interact {
                replay_world.request_interaction(inp.player);
            }
            input_idx += 1;
        }

        replay_world.step();

        if let Some(expected_cp) = trace.checkpoints.iter().find(|cp| cp.tick == tick) {
            let actual_checksum = replay_world.checksum();
            if actual_checksum != expected_cp.checksum {
                // Pinpoint divergence
                let actual_snap = replay_world.snapshot(tick);
                let mut diffs = Vec::new();

                for exp_p in &expected_cp.players {
                    if let Some(act_p) = actual_snap.players.iter().find(|p| p.id == exp_p.id) {
                        let dist = (exp_p.position - act_p.position).length();
                        let yaw_diff = (exp_p.yaw - act_p.yaw).abs();
                        diffs.push(PlayerDiff {
                            id: exp_p.id,
                            expected_pos: [exp_p.position.0, exp_p.position.1, exp_p.position.2],
                            actual_pos: [act_p.position.0, act_p.position.1, act_p.position.2],
                            pos_delta_m: dist,
                            yaw_delta: yaw_diff,
                        });
                    }
                }

                return Ok(DivergenceReport {
                    deterministic: false,
                    total_ticks: trace.total_ticks,
                    verified_checkpoints: verified,
                    first_divergent_tick: Some(tick),
                    expected_checksum: Some(format!("0x{:016x}", expected_cp.checksum)),
                    actual_checksum: Some(format!("0x{:016x}", actual_checksum)),
                    player_diffs: Some(diffs),
                });
            }
            verified += 1;
        }
    }

    Ok(DivergenceReport {
        deterministic: true,
        total_ticks: trace.total_ticks,
        verified_checkpoints: verified,
        first_divergent_tick: None,
        expected_checksum: None,
        actual_checksum: None,
        player_diffs: None,
    })
}
