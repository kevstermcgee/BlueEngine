//! Rendering-free simulation shared by the client and dedicated UDP server.
//!
//! Callers schedule ticks; this module does not authenticate players or expire inputs.
//! Held movement persists until replaced or the player leaves. The dedicated server
//! handles disconnects, stale input and packet ordering separately.
//!
//! ```
//! use vesper3d::viewer::{controller::Movement, simulation::HeadlessWorld};
//! let mut world = HeadlessWorld::new()?;
//! assert!(world.join(42));
//! assert!(world.input(42, Movement { right: 1.0, ..Default::default() }, 0.0, 0.0));
//! world.step();
//! assert_eq!(world.tick, 1);
//! assert!(world.player(42).is_some());
//! world.leave(42);
//! assert!(world.player(42).is_none());
//! # Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
//! ```
use super::{
    controller::{Collider, Controller, Movement},
    room::Room,
};
use std::collections::BTreeMap;

/// Duration in seconds of one shared simulation tick (60 Hz).
pub const TICK_SECONDS: f32 = 1. / 60.;
const MAX_STEPS: usize = 8;

/// Client frame accumulator with at most eight catch-up ticks per advance.
/// Call [`Self::reset`] before using a newly positioned controller.
#[derive(Default)]
pub struct PlayerStepper {
    remainder: f64,
    previous: Controller,
    jump: bool,
}
impl PlayerStepper {
    /// Discard accumulated time and pending jump; synchronize the previous pose.
    pub fn reset(&mut self, current: &Controller) {
        self.remainder = 0.;
        self.previous = current.clone();
        self.jump = false;
    }
    /// Advance fixed ticks and return their count. Nonpositive/nonfinite time is ignored.
    /// Retain a jump edge until a tick consumes it; discard stall time beyond eight ticks.
    pub fn advance(
        &mut self,
        current: &mut Controller,
        mut input: Movement,
        dt: f32,
        colliders: &[Collider],
    ) -> usize {
        if !dt.is_finite() || dt <= 0. {
            return 0;
        }
        self.jump |= input.jump;
        const STEP: f64 = 1. / 60.;
        self.remainder = (self.remainder + f64::from(dt)).min(STEP * MAX_STEPS as f64);
        let mut steps = 0;
        while self.remainder + 1e-9 >= STEP && steps < MAX_STEPS {
            self.previous = current.clone();
            input.jump = std::mem::take(&mut self.jump);
            current.update(input, TICK_SECONDS, colliders);
            self.remainder = (self.remainder - STEP).max(0.);
            steps += 1;
        }
        steps
    }
    /// Interpolate position/stance for display while keeping current look angles.
    pub fn pose(&self, current: &Controller) -> Controller {
        current.interpolated(&self.previous, (self.remainder * 60.) as f32)
    }
}

/// Stored controller and private pending intent for one match-local player.
pub struct Player {
    /// Simulation state; world callers inspect it through [`HeadlessWorld::player`].
    pub controller: Controller,
    input: Movement,
    interact: bool,
}
/// Match-local world, bounded to eight players. No socket, renderer or window.
pub struct HeadlessWorld {
    /// Optional authoritative declarative game rules/state.
    pub game: Option<super::game::GameRuntime>,
    /// Initial content fingerprint, captured before physics extracts/moves geometry.
    pub content_hash: u64,
    /// Shared geometry and collision data; mutation is the host caller's responsibility.
    pub room: Room,
    /// Spatial room graph for interest management and culling.
    pub room_graph: super::spatial::RoomGraph,
    /// Object lifecycle registry ("static until proven otherwise").
    pub lifecycle: super::lifecycle::LifecycleRegistry,
    /// Authoritative prop physics simulation.
    pub prop_physics: Option<super::prop_physics::PropPhysics>,
    players: BTreeMap<u64, Player>,
    /// Number of completed calls to [`Self::step`], initially zero.
    pub tick: u64,
    /// Execution time in microseconds spent in physics on the last tick.
    pub last_physics_time_us: f64,
}
impl HeadlessWorld {
    /// Build the default Blue Test Lab. Returns an error if map construction fails.
    pub fn new() -> crate::Result<Self> {
        Self::try_with_room(super::maps::build(super::maps::MapId::TestLab)?)
    }
    /// Start an empty world with an already constructed room and tick zero.
    pub fn with_room(mut room: Room) -> Self {
        let content_hash = super::content::fingerprint(&room);
        let prop_physics = super::prop_physics::PropPhysics::new(&mut room).ok();
        Self::from_initialized_room(room, prop_physics, content_hash)
    }
    /// Fallible initialization for new callers: never silently discard physics errors.
    pub fn try_with_room(mut room: Room) -> crate::Result<Self> {
        let content_hash = super::content::fingerprint(&room);
        let physics = super::prop_physics::PropPhysics::new(&mut room)?;
        Ok(Self::from_initialized_room(
            room,
            Some(physics),
            content_hash,
        ))
    }
    fn from_initialized_room(
        room: Room,
        prop_physics: Option<super::prop_physics::PropPhysics>,
        content_hash: u64,
    ) -> Self {
        let mut lifecycle = super::lifecycle::LifecycleRegistry::new();
        for e in &room.entities {
            let center = (e.bounds.min + e.bounds.max) * 0.5;
            lifecycle.register(e.id.clone(), e.label.clone(), center);
        }
        let room_graph = super::spatial::RoomGraph::for_room(&room);
        Self {
            game: None,
            content_hash,
            room,
            room_graph,
            lifecycle,
            prop_physics,
            players: BTreeMap::new(),
            tick: 0,
            last_physics_time_us: 0.0,
        }
    }
    /// Join at the shared default spawn; reject duplicate IDs or a full eight-player world.
    /// Players do not collide with each other. IDs are supplied by the caller.
    pub fn join(&mut self, id: u64) -> bool {
        if self.players.len() >= 8 || self.players.contains_key(&id) {
            return false;
        }
        self.players.insert(
            id,
            Player {
                controller: self
                    .game
                    .as_ref()
                    .map_or_else(Controller::default, |g| g.controller(id)),
                input: Movement::default(),
                interact: false,
            },
        );
        true
    }
    /// Remove state and pending input. Release any held prop owned by this player.
    pub fn leave(&mut self, id: u64) {
        if let Some(ref mut physics) = self.prop_physics {
            physics.drop_for_player(id);
        }
        self.players.remove(&id);
        if let Some(game) = &mut self.game {
            game.forget_player(id);
        }
    }
    /// Neutralize active movement intent for a player (used on input sequence timeout/stale frames).
    pub fn neutralize_input(&mut self, id: u64) {
        if let Some(player) = self.players.get_mut(&id) {
            player.input = Movement::default();
            player.interact = false;
        }
    }
    /// Authoritatively fire a pistol shot for a player.
    /// Resolves closest hit among static room geometry and dynamic props to prevent firing through walls.
    pub fn fire_pistol(&mut self, id: u64) -> Option<crate::math::V> {
        let player = self.players.get(&id)?;
        let ray = player.controller.ray();
        if !ray.o.finite() || !ray.d.finite() {
            return None;
        }

        let max_range = crate::viewer::weapons::PISTOL_RANGE;
        let static_hit = self.room.world.hit(ray, max_range, false);
        let max_prop_dist = static_hit.as_ref().map(|h| h.t).unwrap_or(max_range);

        let mut hit_prop = None;
        if let Some(ref mut physics) = self.prop_physics {
            if let Some((i, d)) = physics.hit_prop(ray, max_prop_dist) {
                hit_prop = Some((i, d));
            }
            if let Some((i, d)) = hit_prop {
                physics.apply_impulse(i, ray.d.norm() * 6.0);
                return Some(ray.o + ray.d.norm() * d);
            }
        }

        static_hit.map(|h| h.p)
    }

    /// Authoritatively swing a wrench for a player.
    /// Resolves closest hit among static room geometry and dynamic props within reach.
    pub fn fire_wrench(&mut self, id: u64) -> Option<crate::math::V> {
        let player = self.players.get(&id)?;
        let ray = player.controller.ray();
        let reach = crate::viewer::wrench::REACH;

        let static_hit = self.room.world.hit(ray, reach, false);
        let max_prop_dist = static_hit.as_ref().map(|h| h.t).unwrap_or(reach);

        let mut hit_prop = None;
        if let Some(ref mut physics) = self.prop_physics {
            if let Some((i, d)) = physics.hit_prop(ray, max_prop_dist) {
                hit_prop = Some((i, d));
            }
            if let Some((i, d)) = hit_prop {
                if let Some(holder) = physics.holder_of(i) {
                    physics.drop_for_player(holder);
                }
                physics.apply_impulse(i, ray.d.norm() * 12.0);
                return Some(ray.o + ray.d.norm() * d);
            }
        }

        static_hit.map(|h| h.p)
    }
    /// Borrow current authoritative controller state, or return `None` for an unknown ID.
    pub fn player(&self, id: u64) -> Option<&Controller> {
        self.players.get(&id).map(|p| &p.controller)
    }
    /// Borrow current authoritative controller state mutably, or return `None` for an unknown ID.
    pub fn player_mut(&mut self, id: u64) -> Option<&mut Controller> {
        self.players.get_mut(&id).map(|p| &mut p.controller)
    }
    /// Join at a specific initial spawn position.
    pub fn join_at(&mut self, id: u64, position: crate::math::V) -> bool {
        if !self.join(id) {
            return false;
        }
        if let Some(player) = self.players.get_mut(&id) {
            player.controller.position = position;
        }
        true
    }
    /// Apply an external linear impulse to an authoritative prop body by index.
    pub fn apply_prop_impulse(&mut self, i: usize, impulse: crate::math::V) {
        if let Some(ref mut physics) = self.prop_physics {
            physics.apply_impulse(i, impulse);
        }
    }
    /// Apply an impulse by stable semantic ID; false for static/unknown IDs or nonfinite input.
    /// Resolves the index internally so callers need no long-lived room/physics borrow.
    pub fn impulse(&mut self, id: &str, impulse: crate::math::V) -> bool {
        if !impulse.finite() {
            return false;
        }
        let Some(physics) = self.prop_physics.as_mut() else {
            return false;
        };
        let Some(index) = physics.props.iter().position(|p| p.id == id) else {
            return false;
        };
        physics.apply_impulse(index, impulse);
        true
    }
    /// Copy a dynamic prop's current center by semantic ID; static/unknown IDs return None.
    pub fn prop_position(&self, id: &str) -> Option<crate::math::V> {
        let physics = self.prop_physics.as_ref()?;
        let index = physics.props.iter().position(|p| p.id == id)?;
        physics.prop_position(index)
    }
    /// Queue one interaction edge for the next fixed tick; repeated intents coalesce.
    pub fn request_interaction(&mut self, id: u64) -> bool {
        let Some(player) = self.players.get_mut(&id) else {
            return false;
        };
        player.interact = true;
        true
    }
    /// Accept movement intent without accepting a client position.
    /// Return false without mutation for unknown IDs or nonfinite axes/look angles.
    /// Clamp axes to [-1, 1], wrap yaw to [0, TAU), clamp pitch to [-1.5, 1.5].
    /// Jump edges accumulate until the next tick; all other input replaces prior intent.
    /// Held input persists across ticks; there is no timeout in this API.
    pub fn input(&mut self, id: u64, mut input: Movement, yaw: f32, pitch: f32) -> bool {
        if !input.forward.is_finite()
            || !input.right.is_finite()
            || !yaw.is_finite()
            || !pitch.is_finite()
        {
            return false;
        }
        let Some(player) = self.players.get_mut(&id) else {
            return false;
        };
        input.forward = input.forward.clamp(-1., 1.);
        input.right = input.right.clamp(-1., 1.);
        input.jump |= player.input.jump;
        player.input = input;
        player.controller.yaw = yaw.rem_euclid(std::f32::consts::TAU);
        player.controller.pitch = pitch.clamp(-1.5, 1.5);
        true
    }
    /// Advance every player one tick and increment the world tick, even when empty.
    /// Consume pending jump edges once; retain all other movement intent.
    pub fn step(&mut self) {
        if let Some(game) = &mut self.game {
            game.step_movers(&mut self.room);
            game.step_timers();
        }
        for (&id, player) in &mut self.players {
            player
                .controller
                .update(player.input, TICK_SECONDS, &self.room.colliders);
            player.input.jump = false;
            if std::mem::take(&mut player.interact) {
                if let Some(game) = &mut self.game {
                    game.interact(&self.room, &player.controller, id);
                }
            }
            if let Some(game) = &mut self.game {
                game.step_triggers(&player.controller, id);
            }
        }

        let t_phys = std::time::Instant::now();
        if let Some(ref mut physics) = self.prop_physics {
            let mut player_controllers = std::collections::HashMap::new();
            for (&id, p) in &self.players {
                player_controllers.insert(id, &p.controller);
            }
            physics.step_simulation_with_players(TICK_SECONDS, &player_controllers, &mut self.room);

            // Synchronize prop positions & velocities into lifecycle registry
            for (i, p) in physics.props.iter().enumerate() {
                if let Some(pos) = physics.prop_position(i) {
                    self.lifecycle.update_position(&p.id, pos);
                    let is_held = physics.is_prop_held(i);
                    let speed = physics
                        .prop_linear_velocity(i)
                        .map(|v| v.length())
                        .unwrap_or(0.0);

                    if is_held || speed > 0.05 {
                        self.lifecycle
                            .promote_by_id(&p.id, super::lifecycle::LifecycleState::DynamicEntity);
                    } else {
                        self.lifecycle.update_prop_rest(&p.id, TICK_SECONDS, speed);
                    }
                }
            }
        }
        if let Some(game) = &mut self.game {
            game.apply_mover_colliders(&mut self.room);
        }
        self.last_physics_time_us = t_phys.elapsed().as_secs_f64() * 1_000_000.0;
        self.tick += 1;
    }

    /// Calculate a deterministic 64-bit checksum of the world state at the current tick.
    /// Uses quantized coordinates (to millimeter precision) and quantized prop transforms and
    /// velocities for repeat-run comparison and desync diagnostics. Cross-platform
    /// bitwise equivalence is not guaranteed by quantization alone.
    pub fn checksum(&self) -> u64 {
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;

        let mut hash = FNV_OFFSET;
        let mut mix_u64 = |mut val: u64| {
            for _ in 0..8 {
                let byte = (val & 0xff) as u8;
                hash ^= byte as u64;
                hash = hash.wrapping_mul(FNV_PRIME);
                val >>= 8;
            }
        };

        mix_u64(self.tick);

        // Players sorted by ID
        for (&id, player) in &self.players {
            mix_u64(id);
            let px = (player.controller.position.0 * 1000.0).round() as i64 as u64;
            let py = (player.controller.position.1 * 1000.0).round() as i64 as u64;
            let pz = (player.controller.position.2 * 1000.0).round() as i64 as u64;
            let vel = player.controller.velocity();
            let vx = (vel.0 * 1000.0).round() as i64 as u64;
            let vy = (vel.1 * 1000.0).round() as i64 as u64;
            let vz = (vel.2 * 1000.0).round() as i64 as u64;
            mix_u64(px);
            mix_u64(py);
            mix_u64(pz);
            mix_u64(vx);
            mix_u64(vy);
            mix_u64(vz);
        }

        // Lifecycle tier distribution
        let (static_inst, interactive, dynamic, replicated) = self.lifecycle.counts();
        mix_u64(static_inst as u64);
        mix_u64(interactive as u64);
        mix_u64(dynamic as u64);
        mix_u64(replicated as u64);

        // Full prop state: quantized position, rotation quaternion, linear & angular velocity, sleeping, holder
        if let Some(ref physics) = self.prop_physics {
            let (active, sleeping) = physics.active_and_sleeping_counts();
            mix_u64(active as u64);
            mix_u64(sleeping as u64);

            for (i, p) in physics.props.iter().enumerate() {
                for b in p.id.as_bytes() {
                    mix_u64(*b as u64);
                }
                if let Some(pos) = physics.prop_position(i) {
                    mix_u64((pos.0 * 1000.0).round() as i64 as u64);
                    mix_u64((pos.1 * 1000.0).round() as i64 as u64);
                    mix_u64((pos.2 * 1000.0).round() as i64 as u64);
                }
                if let Some(rot) = physics.prop_rotation(i) {
                    mix_u64((rot[0] * 10000.0).round() as i64 as u64);
                    mix_u64((rot[1] * 10000.0).round() as i64 as u64);
                    mix_u64((rot[2] * 10000.0).round() as i64 as u64);
                    mix_u64((rot[3] * 10000.0).round() as i64 as u64);
                }
                if let Some(lv) = physics.prop_linear_velocity(i) {
                    mix_u64((lv.0 * 1000.0).round() as i64 as u64);
                    mix_u64((lv.1 * 1000.0).round() as i64 as u64);
                    mix_u64((lv.2 * 1000.0).round() as i64 as u64);
                }
                if let Some(av) = physics.prop_angular_velocity(i) {
                    mix_u64((av.0 * 1000.0).round() as i64 as u64);
                    mix_u64((av.1 * 1000.0).round() as i64 as u64);
                    mix_u64((av.2 * 1000.0).round() as i64 as u64);
                }
                mix_u64(if physics.is_prop_sleeping(i) { 1 } else { 0 });
                mix_u64(physics.holder_of(i).unwrap_or(0));
            }
        }

        if let Some(game) = &self.game {
            for value in &game.state().counters {
                mix_u64(*value as u64);
            }
            mix_u64(u64::from(game.state().enabled));
            mix_u64(u64::from(game.state().fired));
            mix_u64(u64::from(game.state().completed));
        }
        hash
    }

    /// Generate an authoritative world snapshot for multiplayer replication.
    pub fn snapshot(&self, ack_client_tick: u64) -> super::net::WorldSnapshot {
        let players = self
            .players
            .iter()
            .map(|(&id, p)| {
                let room_id = self.room_graph.find_room_at(p.controller.position);
                super::net::PlayerNetState::from_controller(id, self.tick, &p.controller, room_id)
            })
            .collect();

        let props = self
            .lifecycle
            .objects
            .iter()
            .filter(|o| o.state.requires_rigid_body() || o.state.requires_networking())
            .map(|o| {
                let idx = self
                    .prop_physics
                    .as_ref()
                    .and_then(|phys| phys.props.iter().position(|p| p.id == o.id));
                let (rot, linvel, angvel, sleeping, held_by) =
                    if let (Some(ref phys), Some(i)) = (&self.prop_physics, idx) {
                        (
                            phys.prop_rotation(i).unwrap_or([0., 0., 0., 1.]),
                            phys.prop_linear_velocity(i).unwrap_or(crate::math::V::ZERO),
                            phys.prop_angular_velocity(i)
                                .unwrap_or(crate::math::V::ZERO),
                            phys.is_prop_sleeping(i),
                            phys.holder_of(i),
                        )
                    } else {
                        (
                            [0., 0., 0., 1.],
                            crate::math::V::ZERO,
                            crate::math::V::ZERO,
                            true,
                            None,
                        )
                    };

                super::net::PropNetState {
                    id: o.id.clone(),
                    position: o.position,
                    rotation: rot,
                    linear_velocity: linvel,
                    angular_velocity: angvel,
                    sleeping,
                    held_by,
                    generation: o.generation,
                }
            })
            .collect();

        super::net::WorldSnapshot {
            tick: self.tick,
            ack_client_tick,
            players,
            props,
        }
    }

    /// Generate an authoritative world snapshot filtered by spatial interest for a specific observer player.
    /// The observer always receives their own authoritative state (for client-side prediction reconciliation).
    /// Remote entities (players and props) are included only if they are spatially relevant (in the same room,
    /// an adjacent room via open portals, or global/outdoor).
    pub fn snapshot_for_player(
        &self,
        observer_id: u64,
        ack_client_tick: u64,
    ) -> super::net::WorldSnapshot {
        let obs_pos = self
            .players
            .get(&observer_id)
            .map(|p| p.controller.position);
        let obs_room = obs_pos.and_then(|p| self.room_graph.find_room_at(p));

        let is_relevant = |pos: crate::math::V| -> bool {
            match obs_room {
                None => true, // Observer outside/global: full visibility
                Some(r_obs) => match self.room_graph.find_room_at(pos) {
                    None => true, // Target is outdoor/global
                    Some(r_tgt) => self.room_graph.is_relevant_for_interest(r_obs, r_tgt),
                },
            }
        };

        let players = self
            .players
            .iter()
            .filter(|(&id, p)| id == observer_id || is_relevant(p.controller.position))
            .map(|(&id, p)| {
                let room_id = self.room_graph.find_room_at(p.controller.position);
                super::net::PlayerNetState::from_controller(id, self.tick, &p.controller, room_id)
            })
            .collect();

        let props = self
            .lifecycle
            .objects
            .iter()
            .filter(|o| o.state.requires_rigid_body() || o.state.requires_networking())
            .filter(|o| is_relevant(o.position))
            .map(|o| {
                let idx = self
                    .prop_physics
                    .as_ref()
                    .and_then(|phys| phys.props.iter().position(|p| p.id == o.id));
                let (rot, linvel, angvel, sleeping, held_by) =
                    if let (Some(ref phys), Some(i)) = (&self.prop_physics, idx) {
                        (
                            phys.prop_rotation(i).unwrap_or([0., 0., 0., 1.]),
                            phys.prop_linear_velocity(i).unwrap_or(crate::math::V::ZERO),
                            phys.prop_angular_velocity(i)
                                .unwrap_or(crate::math::V::ZERO),
                            phys.is_prop_sleeping(i),
                            phys.holder_of(i),
                        )
                    } else {
                        (
                            [0., 0., 0., 1.],
                            crate::math::V::ZERO,
                            crate::math::V::ZERO,
                            true,
                            None,
                        )
                    };

                super::net::PropNetState {
                    id: o.id.clone(),
                    position: o.position,
                    rotation: rot,
                    linear_velocity: linvel,
                    angular_velocity: angvel,
                    sleeping,
                    held_by,
                    generation: o.generation,
                }
            })
            .collect();

        super::net::WorldSnapshot {
            tick: self.tick,
            ack_client_tick,
            players,
            props,
        }
    }

    /// Create a performance snapshot for observability and profiling.
    pub fn performance_snapshot(
        &self,
        sim_cpu_time_us: f64,
    ) -> super::metrics::PerformanceSnapshot {
        let (static_inst, _, dynamic, replicated) = self.lifecycle.counts();
        let (active_dyn, sleeping) = if let Some(ref phys) = self.prop_physics {
            phys.active_and_sleeping_counts()
        } else {
            (dynamic, 0)
        };

        let snap = self.snapshot(0);
        let encoded_bytes = serde_json::to_vec(&snap).map(|b| b.len()).unwrap_or(0);
        let delta = snap.compute_delta(&super::net::WorldSnapshot::default());
        let delta_bytes = serde_json::to_vec(&delta).map(|b| b.len()).unwrap_or(0);

        super::metrics::PerformanceSnapshot {
            tick: self.tick,
            sim_cpu_time_us,
            physics_time_us: self.last_physics_time_us,
            active_dynamic_bodies: active_dyn,
            sleeping_bodies: sleeping,
            static_instances: static_inst,
            replicated_entities: replicated + self.players.len(),
            snapshot_bytes: encoded_bytes,
            delta_bytes,
            bandwidth_kbps: (encoded_bytes as f64 * 8.0 * 20.0) / 1000.0, // at 20 Hz snapshot rate
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn render_rates_produce_same_simulation() {
        let mut end = Vec::new();
        for hz in [30, 60, 144, 240] {
            let mut c = Controller::default();
            let mut stepper = PlayerStepper::default();
            let mut count = 0;
            for i in 0..hz {
                count += stepper.advance(
                    &mut c,
                    Movement {
                        forward: 1.,
                        jump: i == 0,
                        ..Default::default()
                    },
                    1. / hz as f32,
                    &[],
                );
            }
            assert_eq!(count, 60);
            end.push(c.position);
        }
        for p in &end {
            assert!((*p - end[0]).length() < 0.00001);
        }
    }
    #[test]
    fn jump_survives_subtick_and_stalls_are_bounded() {
        let mut c = Controller::default();
        let mut s = PlayerStepper::default();
        assert_eq!(
            s.advance(
                &mut c,
                Movement {
                    jump: true,
                    ..Default::default()
                },
                0.001,
                &[]
            ),
            0
        );
        s.advance(&mut c, Movement::default(), 0.02, &[]);
        assert!(!c.is_grounded());
        assert_eq!(s.advance(&mut c, Movement::default(), 10., &[]), MAX_STEPS);
        s.reset(&c);
        assert_eq!(s.pose(&c).position, c.position);
    }
    #[test]
    fn headless_bounds_input_and_matches_client() {
        let mut world = HeadlessWorld::new().unwrap();
        for id in 0..8 {
            assert!(world.join(id));
        }
        assert!(!world.join(8));
        assert!(!world.join(0));
        assert!(!world.input(0, Movement::default(), f32::NAN, 0.));
        let mut client = Controller::default();
        let input = Movement {
            right: 1.,
            ..Default::default()
        };
        world.input(0, input, client.yaw, client.pitch);
        for _ in 0..60 {
            world.step();
            client.update(input, TICK_SECONDS, &world.room.colliders);
        }
        assert!((world.player(0).unwrap().position - client.position).length() < 0.00001);
        world.leave(0);
        assert!(world.join(8));
    }

    #[test]
    fn headless_deterministic_checksum_and_prop_physics() {
        let mut world1 = HeadlessWorld::new().unwrap();
        let mut world2 = HeadlessWorld::new().unwrap();
        assert!(world1.join(1));
        assert!(world2.join(1));

        assert!(world1.prop_physics.is_some());
        assert!(world2.prop_physics.is_some());

        // Identical input across 60 steps must produce identical state and checksum
        let input = Movement {
            forward: 1.0,
            ..Default::default()
        };
        world1.input(1, input, 0.0, 0.0);
        world2.input(1, input, 0.0, 0.0);

        for _ in 0..60 {
            world1.step();
            world2.step();
        }

        assert_eq!(world1.tick, 60);
        assert_eq!(world2.tick, 60);
        assert_eq!(
            world1.player(1).unwrap().position,
            world2.player(1).unwrap().position
        );
        assert_eq!(
            world1.player(1).unwrap().velocity(),
            world2.player(1).unwrap().velocity()
        );
        assert_eq!(world1.lifecycle.counts(), world2.lifecycle.counts());
        assert_eq!(world1.checksum(), world2.checksum());

        let perf = world1.performance_snapshot(500.0);
        assert_eq!(perf.tick, 60);
        assert!(perf.snapshot_bytes > 0);
    }
}
