//! The engine world's save state: what `HeadlessWorld::save_state` captures and `restore_state` accepts.
//!
//! The state is **portable**: it holds only values the engine itself owns and can validate (players,
//! rule state, prop poses and velocities, lifecycle tiers), never the internals of the physics library,
//! so a save written by one engine build restores in a later one. It is bound to the map/game content it
//! was written for (the header carries the content fingerprint) and carries the quantised world
//! checksum, which `restore_state` recomputes as an end-to-end proof that the restore reproduced the
//! saved world.
//!
//! What resumes bit-identically and what does not:
//!
//! * players, rules, timers, movers, counters, zones, lifecycle, prop poses, velocities, sleep state
//!   and carrying resume exactly (the restored world has the saved world's checksum);
//! * rigid-body *contact caches* (the solver's warm-start impulses between touching props) are not saved.
//!   A restore rebuilds the physics scene as it was first built and places every prop from the save, so a
//!   prop pile that was still settling can differ from an uninterrupted run by solver noise. Props that
//!   are asleep or in free fall resume exactly. Either way what happens after a load is a pure function of
//!   the save file: it never depends on what the world did before it loaded. Which piece of the physics
//!   library's state is responsible was isolated, not assumed (ADR 0018, `prop_physics.rs` test
//!   `what_a_restore_forgets_isolated_piece_by_piece`).
//!
//! See `docs/SAVE_STATE.md` for the contract, the failure modes and the versioning policy.
use super::SaveError;
use crate::viewer::{
    controller::{ControllerState, Movement},
    game::GameState,
    lifecycle::LifecycleRegistry,
};
use serde::{Deserialize, Serialize};

/// Header `kind` of engine world saves.
pub const KIND: &str = "world";
/// Payload schema version this build writes and reads natively.
pub const VERSION: u32 = 1;
/// Players a world can hold.
pub const MAX_PLAYERS: usize = 8;
/// Dynamic props a save may describe.
pub const MAX_PROPS: usize = 4096;
/// Lifecycle objects a save may describe.
pub const MAX_LIFECYCLE: usize = 65_536;
/// Coordinates beyond this many metres are not a world this engine simulates.
const MAX_COORD: f32 = 100_000.;
/// Velocities beyond this many metres per second are a blown-up simulation, not a game state.
const MAX_SPEED: f32 = 10_000.;

/// Migration steps for older `world` payloads (none yet: version 1 is the first).
pub const MIGRATIONS: &[super::Migration] = &[];

/// One player's authoritative state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerSave {
    pub id: u64,
    pub controller: ControllerState,
    /// Held movement intent, persisting across ticks; the jump field is a pending edge.
    pub input: Movement,
    /// A pending interaction edge.
    pub interact: bool,
}

/// Declarative game runtime state beyond [`GameState`]: the timers' countdowns and who stands in which zone.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameSave {
    pub state: GameState,
    /// Remaining ticks of every timer, in document order.
    pub timers: Vec<u32>,
    /// `(player id, bit mask of trigger zones the player is inside)`.
    pub zones: Vec<(u64, u64)>,
}

/// A prop's pose, velocity and sleep state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropSave {
    pub id: String,
    pub position: [f32; 3],
    /// Unit quaternion `[x, y, z, w]`.
    pub rotation: [f32; 4],
    pub linvel: [f32; 3],
    pub angvel: [f32; 3],
    pub sleeping: bool,
    /// Seconds the body has been slow enough to sleep (so it falls asleep on schedule after a load).
    pub sleep_timer: f32,
}

/// A player carrying a prop.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hold {
    pub player: u64,
    pub prop: String,
}

/// Prop physics state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicsSave {
    pub props: Vec<PropSave>,
    pub holds: Vec<Hold>,
    /// Physics time not yet consumed by a fixed physics step, in seconds.
    pub debt: f32,
}

/// The whole authoritative world.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldState {
    /// Completed simulation ticks.
    pub tick: u64,
    /// Players in ascending id order.
    pub players: Vec<PlayerSave>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<GameSave>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physics: Option<PhysicsSave>,
    pub lifecycle: LifecycleRegistry,
    /// `HeadlessWorld::checksum()` at save time.
    pub checksum: u64,
}

/// What a restore is allowed to replace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestoreOptions {
    /// Replace the players with the saved ones. A dedicated server resuming a world sets this to false:
    /// its connected sessions are new, and saved players would be ghosts. The end-to-end checksum check
    /// covers players, so it only runs when they are restored.
    pub players: bool,
}

impl Default for RestoreOptions {
    fn default() -> Self {
        Self { players: true }
    }
}

fn invalid(why: impl Into<String>) -> SaveError {
    SaveError::Invalid(why.into())
}

fn all_finite(values: &[f32]) -> bool {
    values.iter().all(|v| v.is_finite())
}

fn within(values: &[f32], limit: f32) -> bool {
    values.iter().all(|v| v.is_finite() && v.abs() <= limit)
}

impl WorldState {
    /// Structural validation: finite, bounded numbers, unique ids and size caps. Checks that need the
    /// world (do these props exist, do these timers match the rules) happen in `restore_state`.
    pub fn validate(&self) -> Result<(), SaveError> {
        if self.players.len() > MAX_PLAYERS {
            return Err(invalid(format!(
                "{} players (at most {MAX_PLAYERS})",
                self.players.len()
            )));
        }
        let mut seen = std::collections::BTreeSet::new();
        for player in &self.players {
            if !seen.insert(player.id) {
                return Err(invalid(format!("player {} appears twice", player.id)));
            }
            let c = &player.controller;
            let p = |what: &str| invalid(format!("player {}: {what}", player.id));
            if !within(
                &[c.position.0, c.position.1, c.position.2, c.feet],
                MAX_COORD,
            ) {
                return Err(p("position is not a finite, bounded coordinate"));
            }
            if !within(
                &[
                    c.velocity.0,
                    c.velocity.1,
                    c.velocity.2,
                    c.vertical_velocity,
                ],
                MAX_SPEED,
            ) || !within(&[c.push.0, c.push.1, c.push.2], MAX_SPEED)
            {
                return Err(p("velocity is not finite and bounded"));
            }
            if !c.yaw.is_finite()
                || c.yaw.abs() > 1e4
                || !c.pitch.is_finite()
                || c.pitch.abs() > 1.6
            {
                return Err(p("look angles are out of range"));
            }
            if !c.body_height.is_finite() || !(0.05..=4.).contains(&c.body_height) {
                return Err(p("body height is out of range"));
            }
            let m = &player.input;
            if !m.forward.is_finite()
                || !m.right.is_finite()
                || m.forward.abs() > 1.
                || m.right.abs() > 1.
            {
                return Err(p("movement intent is out of range"));
            }
        }
        if let Some(game) = &self.game {
            let mut zone_players = std::collections::BTreeSet::new();
            if !game
                .zones
                .iter()
                .all(|(player, _)| zone_players.insert(*player))
            {
                return Err(invalid("a player appears twice in the zone table"));
            }
        }
        if let Some(physics) = &self.physics {
            if physics.props.len() > MAX_PROPS {
                return Err(invalid(format!(
                    "{} props (at most {MAX_PROPS})",
                    physics.props.len()
                )));
            }
            if !physics.debt.is_finite() || physics.debt < 0. {
                return Err(invalid(
                    "physics time debt is not a finite, non-negative number",
                ));
            }
            let mut ids = std::collections::BTreeSet::new();
            for prop in &physics.props {
                if !ids.insert(prop.id.as_str()) {
                    return Err(invalid(format!("prop '{}' appears twice", prop.id)));
                }
                let bad = |what: &str| invalid(format!("prop '{}': {what}", prop.id));
                if !within(&prop.position, MAX_COORD) {
                    return Err(bad("position is not a finite, bounded coordinate"));
                }
                let norm = prop.rotation.iter().map(|v| v * v).sum::<f32>().sqrt();
                if !all_finite(&prop.rotation) || (norm - 1.).abs() > 1e-3 {
                    return Err(bad("rotation is not a unit quaternion"));
                }
                if !within(&prop.linvel, MAX_SPEED) || !within(&prop.angvel, MAX_SPEED) {
                    return Err(bad("velocity is not finite and bounded"));
                }
                if !prop.sleep_timer.is_finite() || prop.sleep_timer < 0. {
                    return Err(bad("sleep timer is not a finite, non-negative number"));
                }
            }
            let (mut holders, mut held) = (
                std::collections::BTreeSet::new(),
                std::collections::BTreeSet::new(),
            );
            for hold in &physics.holds {
                if !holders.insert(hold.player) || !held.insert(hold.prop.as_str()) {
                    return Err(invalid("a player or a prop appears in two holds"));
                }
                if !ids.contains(hold.prop.as_str()) {
                    return Err(invalid(format!(
                        "a hold names unknown prop '{}'",
                        hold.prop
                    )));
                }
            }
        }
        if self.lifecycle.objects.len() > MAX_LIFECYCLE {
            return Err(invalid("too many lifecycle objects"));
        }
        for object in &self.lifecycle.objects {
            if !within(
                &[
                    object.position.0,
                    object.position.1,
                    object.position.2,
                    object.rest_duration,
                ],
                MAX_COORD,
            ) {
                return Err(invalid(format!(
                    "lifecycle object '{}' has a non-finite value",
                    object.id
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::V;

    fn player(id: u64) -> PlayerSave {
        PlayerSave {
            id,
            controller: crate::viewer::controller::Controller::default().network_state(),
            input: Movement::default(),
            interact: false,
        }
    }

    fn prop(id: &str) -> PropSave {
        PropSave {
            id: id.into(),
            position: [1., 2., 3.],
            rotation: [0., 0., 0., 1.],
            linvel: [0.; 3],
            angvel: [0.; 3],
            sleeping: false,
            sleep_timer: 0.,
        }
    }

    fn state() -> WorldState {
        WorldState {
            tick: 5,
            players: vec![player(1), player(2)],
            game: None,
            physics: Some(PhysicsSave {
                props: vec![prop("a"), prop("b")],
                holds: vec![],
                debt: 0.004,
            }),
            lifecycle: LifecycleRegistry::new(),
            checksum: 0,
        }
    }

    #[test]
    fn a_reasonable_state_validates_and_round_trips_through_json_exactly() {
        let s = state();
        s.validate().unwrap();
        let back: WorldState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s, "f32 values survive a JSON round trip bit for bit");
    }

    #[test]
    fn every_kind_of_nonsense_is_rejected_with_a_reason() {
        type Break = fn(&mut WorldState);
        let cases: [(&str, Break); 14] = [
            ("nan position", |s| {
                s.players[0].controller.position = V(f32::NAN, 0., 0.)
            }),
            ("huge position", |s| {
                s.players[0].controller.position = V(1e9, 0., 0.)
            }),
            ("infinite velocity", |s| {
                s.players[0].controller.velocity = V(f32::INFINITY, 0., 0.)
            }),
            ("pitch", |s| s.players[0].controller.pitch = 3.),
            ("body height", |s| s.players[0].controller.body_height = 0.),
            ("intent", |s| s.players[0].input.forward = 5.),
            ("duplicate player", |s| s.players[1].id = 1),
            ("nine players", |s| {
                s.players = (1..=9).map(player).collect()
            }),
            ("prop nan", |s| {
                s.physics.as_mut().unwrap().props[0].position[1] = f32::NAN
            }),
            ("prop rotation", |s| {
                s.physics.as_mut().unwrap().props[0].rotation = [0.; 4]
            }),
            ("prop velocity", |s| {
                s.physics.as_mut().unwrap().props[0].linvel[0] = 1e9
            }),
            ("duplicate prop", |s| {
                s.physics.as_mut().unwrap().props[1].id = "a".into()
            }),
            ("hold of nothing", |s| {
                s.physics.as_mut().unwrap().holds.push(Hold {
                    player: 1,
                    prop: "ghost".into(),
                })
            }),
            ("negative debt", |s| s.physics.as_mut().unwrap().debt = -1.),
        ];
        for (name, damage) in cases {
            let mut s = state();
            damage(&mut s);
            let error = s.validate().expect_err(name);
            assert!(matches!(error, SaveError::Invalid(_)), "{name}: {error}");
        }
    }

    #[test]
    fn unknown_fields_are_refused_not_ignored() {
        let mut value = serde_json::to_value(state()).unwrap();
        value["surprise"] = 1.into();
        assert!(serde_json::from_value::<WorldState>(value).is_err());
    }

    #[test]
    fn options_default_to_restoring_players() {
        assert!(RestoreOptions::default().players);
    }
}
