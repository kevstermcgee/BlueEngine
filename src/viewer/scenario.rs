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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimedInput {
    pub tick: u64,
    pub player: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub forward: f32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub right: f32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub yaw: f32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub pitch: f32,
    #[serde(default, skip_serializing_if = "is_false")]
    pub sprint: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub jump: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub crouch: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub interact: bool,
    /// Restart the match, as a player pressing the action button does once it is won or lost (the stock client's
    /// shared `game_action` policy). Everything resets and the players respawn. It is reported as a problem if
    /// the match is still running, so a scenario cannot pass by restarting at the wrong moment.
    #[serde(default, skip_serializing_if = "is_false")]
    pub restart: bool,
    /// Walk to this point (x, z, metres) at full speed and stop there. Later inputs for the same player
    /// wait until it arrives and settles, so a scenario reads as intent ("walk here, then press") instead of
    /// tick arithmetic. Unless `face` is given the player keeps the heading it has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub walk_to: Option<[f32; 2]>,
    /// Plan a walk around current colliders using the player's profile, rechecking every 30 executed ticks.
    /// If a gate blocks the route, wait and replan up to the walk timeout. False preserves direct steering.
    #[serde(default, skip_serializing_if = "is_false")]
    pub route: bool,
    /// Hold later inputs for this player for this many executed simulation steps after this input applies.
    /// Use 1 on repeated presses: overdue timestamps alone do not prevent interaction coalescing. Maximum 60,000.
    #[serde(default, skip_serializing_if = "is_u32_zero")]
    pub wait_ticks: u32,
    /// Look at the centre of this entity (yaw and pitch are set from where the player's eyes are now).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assertion {
    pub tick: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position_near: Option<[f32; 3]>,
    #[serde(default = "default_tolerance")]
    pub tolerance: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counter_equals: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_equals: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_equals: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled_equals: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_equals: Option<bool>,
}

fn is_zero(v: &f32) -> bool {
    *v == 0.0
}
fn is_false(v: &bool) -> bool {
    !*v
}
fn is_u32_zero(v: &u32) -> bool {
    *v == 0
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
    /// Where each player ended up (eye position), so a scenario's walk can be checked without guessing.
    pub players: Vec<FinalPlayer>,
    /// Enabled targets actually observed within authoritative interaction reach/line of sight.
    /// This selected run proves later reachability, not reachability in every possible state.
    #[serde(default)]
    pub reachable_targets: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FinalPlayer {
    pub id: u64,
    pub position: [f32; 3],
    pub yaw: f32,
}

/// How close to a `walk_to` point counts as arrived, and how long the player then takes to stop.
const ARRIVE_M: f32 = 0.12;
const SETTLE_TICKS: u32 = 8;
/// A walk that has not arrived after this many ticks is reported instead of hanging the scenario.
const WALK_TIMEOUT_TICKS: u32 = 1800;

/// Applies a scenario's timed inputs, including the `walk_to` and `face` intents. The assertion runner and
/// the replay verifier both use it, so they cannot disagree about what an input does.
pub struct InputDriver {
    pending: Vec<TimedInput>,
    walking: std::collections::BTreeMap<u64, Walk>,
    settling: std::collections::BTreeMap<u64, u32>,
    problems: Vec<String>,
}
type V2 = [f32; 2];
struct Walk {
    target: V2,
    ticks: u32,
    routed: bool,
    points: std::collections::VecDeque<V2>,
}

impl InputDriver {
    pub fn new(inputs: &[TimedInput]) -> Self {
        let mut pending = inputs.to_vec();
        pending.sort_by_key(|i| i.tick); // stable: same-tick inputs keep their order
        Self {
            pending,
            walking: Default::default(),
            settling: Default::default(),
            problems: Vec::new(),
        }
    }
    /// Queue another input while a run is in progress (used to record a scenario as it is played).
    pub fn push(&mut self, input: TimedInput) {
        self.pending.push(input);
        self.pending.sort_by_key(|i| i.tick);
    }
    /// True when this player is walking or still settling, so its next input would be held.
    pub fn busy(&self, player: u64) -> bool {
        self.held(player) || self.pending.iter().any(|i| i.player == player)
    }
    /// Things that went wrong that an assertion cannot see (a walk that never arrived, an unknown entity).
    pub fn problems(&self) -> &[String] {
        &self.problems
    }
    /// True when nothing is left to apply and nobody is walking or settling.
    pub fn finished(&self) -> bool {
        self.pending.is_empty() && self.walking.is_empty() && self.settling.is_empty()
    }
    fn held(&self, player: u64) -> bool {
        self.walking.contains_key(&player) || self.settling.contains_key(&player)
    }
    /// Apply what is due at `tick`, then steer walkers. Call once per tick, before `world.step()`.
    pub fn before_step(&mut self, world: &mut HeadlessWorld, tick: u64) {
        let mut i = 0;
        while i < self.pending.len() {
            if self.pending[i].tick <= tick && !self.held(self.pending[i].player) {
                let input = self.pending.remove(i);
                self.apply(world, &input);
            } else {
                i += 1;
            }
        }
        let walkers: Vec<u64> = self.walking.keys().copied().collect();
        for id in walkers {
            self.steer(world, id);
        }
        self.settling.retain(|_, left| {
            *left = left.saturating_sub(1);
            *left > 0
        });
    }
    fn apply(&mut self, world: &mut HeadlessWorld, input: &TimedInput) {
        let (mut yaw, mut pitch) = (input.yaw, input.pitch);
        if let Some(name) = &input.face {
            match Self::heading_to(world, input.player, name) {
                Some(heading) => (yaw, pitch) = heading,
                None => self.problems.push(format!(
                    "tick {}: player {} cannot face '{name}' (unknown entity or player)",
                    input.tick, input.player
                )),
            }
        } else if input.walk_to.is_some() {
            if let Some(player) = world.player(input.player) {
                (yaw, pitch) = (player.yaw, player.pitch);
            }
        }
        let movement = Movement {
            forward: if input.walk_to.is_some() {
                0.
            } else {
                input.forward
            },
            right: if input.walk_to.is_some() {
                0.
            } else {
                input.right
            },
            sprint: input.sprint,
            jump: input.jump,
            crouch: input.crouch,
        };
        world.input(input.player, movement, yaw, pitch);
        if let Some(target) = input.walk_to {
            self.walking.insert(
                input.player,
                Walk {
                    target,
                    ticks: 0,
                    routed: input.route,
                    points: Default::default(),
                },
            );
        }
        if input.wait_ticks > 0 {
            self.settling.insert(input.player, input.wait_ticks);
        }
        if input.interact {
            world.request_interaction(input.player);
        }
        if input.restart {
            if world.game.as_ref().is_some_and(|g| g.state().finished()) {
                if let Err(error) = world.restart_game() {
                    self.problems
                        .push(format!("tick {}: restart failed: {error}", input.tick));
                }
            } else {
                self.problems.push(format!(
                    "tick {}: restart requested but the match is not finished",
                    input.tick
                ));
            }
        }
    }
    /// Yaw and pitch that put the centre of entity `name` in the middle of the player's view.
    fn heading_to(world: &HeadlessWorld, player: u64, name: &str) -> Option<(f32, f32)> {
        let eye = world.player(player)?.position;
        let bounds = &world.room.entities.iter().find(|e| e.id == name)?.bounds;
        let d = (bounds.min + bounds.max) * 0.5 - eye;
        let level = d.0.hypot(d.2);
        Some((d.0.atan2(-d.2), d.1.atan2(level)))
    }
    fn steer(&mut self, world: &mut HeadlessWorld, id: u64) {
        let Some(mut walk) = self.walking.remove(&id) else {
            return;
        };
        let Some(player) = world.player(id) else {
            self.walking.remove(&id);
            return;
        };
        let (position, yaw, pitch) = (player.position, player.yaw, player.pitch);
        let (target, ticks) = (walk.target, walk.ticks);
        let (dx, dz) = (target[0] - position.0, target[1] - position.2);
        let distance = dx.hypot(dz);
        let stop = Movement {
            forward: 0.,
            right: 0.,
            sprint: false,
            jump: false,
            crouch: false,
        };
        if distance <= ARRIVE_M {
            world.input(id, stop, yaw, pitch);
            self.walking.remove(&id);
            self.settling.insert(id, SETTLE_TICKS);
        } else if ticks >= WALK_TIMEOUT_TICKS {
            world.input(id, stop, yaw, pitch);
            self.walking.remove(&id);
            self.problems.push(format!(
                "player {id} did not reach ({:.2}, {:.2}) within {WALK_TIMEOUT_TICKS} ticks; it is at ({:.2}, {:.2})",
                target[0], target[1], position.0, position.2
            ));
        } else {
            if walk.routed {
                if ticks.is_multiple_of(30) {
                    let feet = player.feet_height();
                    walk.points = super::pathing::plan_route_with_profile(
                        &world.room.colliders,
                        V(position.0, feet, position.2),
                        V(target[0], feet, target[1]),
                        player.profile(),
                    )
                    .unwrap_or_default()
                    .into_iter()
                    .skip(1)
                    .map(|p| [p.x, p.z])
                    .collect();
                }
                while walk.points.len() > 1
                    && walk
                        .points
                        .front()
                        .is_some_and(|p| (p[0] - position.0).hypot(p[1] - position.2) < 0.16)
                {
                    walk.points.pop_front();
                }
                if walk.points.is_empty() {
                    world.input(id, stop, yaw, pitch);
                    walk.ticks += 1;
                    self.walking.insert(id, walk);
                    return;
                }
            }
            let aim = walk.points.front().copied().unwrap_or(target);
            let (dx, dz) = (aim[0] - position.0, aim[1] - position.2);
            let distance = dx.hypot(dz).max(0.001);
            // World direction to the target, expressed as forward/right for the current heading, and
            // eased in over the last half metre so the player stops close to the point.
            let (ux, uz) = (dx / distance, dz / distance);
            let forward = ux * yaw.sin() - uz * yaw.cos();
            let right = ux * yaw.cos() + uz * yaw.sin();
            let ease = (distance / 0.5).clamp(0.2, 1.);
            let movement = Movement {
                forward: forward * ease,
                right: right * ease,
                ..stop
            };
            world.input(id, movement, yaw, pitch);
            walk.ticks += 1;
            self.walking.insert(id, walk);
        }
    }
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
    if scenario.inputs.iter().any(|i| i.wait_ticks > 60_000) {
        return Err("wait_ticks must be at most 60,000 executed steps".into());
    }
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
    let mut driver = InputDriver::new(&scenario.inputs);
    let mut reachable_targets = std::collections::BTreeSet::new();
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
        driver.before_step(&mut world, tick);

        if let Some(game) = &world.game {
            for player in &scenario.players {
                if let Some(index) = world
                    .player(player.id)
                    .and_then(|p| game.target(&world.room, p))
                {
                    if game.enabled(index) && !game.state().finished() {
                        reachable_targets
                            .insert(game.document().interactables[index].entity.clone());
                    }
                }
            }
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

                if let Some(expected) = assert.failed_equals {
                    match &world.game {
                        Some(game) if game.state().failed != expected => failures.push(format!(
                            "failed is {}, expected {expected}",
                            game.state().failed
                        )),
                        None => failures.push("failed assertion requires a game".into()),
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

    for problem in driver.problems() {
        assertion_outcomes.push(AssertionOutcome {
            tick: scenario.ticks,
            ok: false,
            detail: problem.clone(),
        });
    }
    let players = scenario
        .players
        .iter()
        .filter_map(|p| {
            let c = world.player(p.id)?;
            Some(FinalPlayer {
                id: p.id,
                position: [c.position.0, c.position.1, c.position.2],
                yaw: c.yaw,
            })
        })
        .collect();
    Ok(ScenarioRunReport {
        ok: assertion_outcomes.iter().all(|outcome| outcome.ok),
        trace,
        assertions: assertion_outcomes,
        players,
        reachable_targets: reachable_targets.into_iter().collect(),
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

    let mut driver = InputDriver::new(&trace.inputs);
    let mut verified = 0;

    for tick in 1..=trace.total_ticks {
        driver.before_step(&mut replay_world, tick);

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
