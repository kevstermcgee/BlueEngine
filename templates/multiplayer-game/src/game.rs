//! Authoritative match simulation and rules engine for multiplayer games.
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use vesper3d::{
    math::V,
    viewer::{
        controller::{CharacterKind, Controller, ControllerState, Movement},
        net::action_counters::{ActionCounters, ActionCountersTracker},
        room::Room,
    },
};

pub const HZ: u64 = 60;
pub const DT: f32 = 1.0 / HZ as f32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchSettings {
    pub round_seconds: u32,
    pub score_to_win: u32,
}

impl Default for MatchSettings {
    fn default() -> Self {
        Self {
            round_seconds: 180,
            score_to_win: 5,
        }
    }
}

impl MatchSettings {
    pub fn is_valid(&self) -> bool {
        (30..=1800).contains(&self.round_seconds) && (1..=50).contains(&self.score_to_win)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchPhase {
    Lobby,
    Warmup,
    Playing,
    Finished,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GameAction {
    SelectRole(CharacterKind),
    Configure(MatchSettings),
    Ready { revision: u64 },
    Leave,
}

/// Incoming client input frame with monotonic action counters and lag compensation aim tick.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GameInput {
    pub sequence: u64,
    pub round: u64,
    pub movement: Movement,
    pub yaw: f32,
    pub pitch: f32,
    pub aim_tick: u64,
    pub counters: ActionCounters,
}

impl GameInput {
    pub fn is_valid(&self) -> bool {
        self.yaw.is_finite()
            && self.pitch.is_finite()
            && self.yaw.abs() < 1.0e6
            && self.pitch.abs() <= 1.55
            && self.movement.forward.is_finite()
            && self.movement.right.is_finite()
            && self.movement.forward.abs() <= 1.0
            && self.movement.right.abs() <= 1.0
    }
}

/// Replicated snapshot view of one player.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerSnapshot {
    pub slot: usize,
    pub role: Option<CharacterKind>,
    pub ready: bool,
    pub pose: ControllerState,
    pub ack_sequence: u64,
    pub score: u32,
}

/// Replicated game state broadcast to all clients each snapshot tick.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchSnapshot {
    pub tick: u64,
    pub round: u64,
    pub revision: u64,
    pub phase: MatchPhase,
    pub remaining_ticks: u64,
    pub winner_slot: Option<usize>,
    pub settings: MatchSettings,
    pub players: Vec<PlayerSnapshot>,
}

pub struct ServerPlayer {
    pub role: Option<CharacterKind>,
    pub ready: bool,
    pub controller: Controller,
    pub input: GameInput,
    pub last_input_tick: u64,
    pub tracker: ActionCountersTracker,
    pub score: u32,
    pub cooldown_ticks: u64,
}

impl Default for ServerPlayer {
    fn default() -> Self {
        Self {
            role: None,
            ready: false,
            controller: Controller::for_character(CharacterKind::Scientist),
            input: GameInput::default(),
            last_input_tick: 0,
            tracker: ActionCountersTracker::new(),
            score: 0,
            cooldown_ticks: 0,
        }
    }
}

/// Authoritative match simulation managing players, physics, combat, and history.
pub struct GameMatch {
    pub room: Room,
    pub players: Vec<Option<ServerPlayer>>,
    pub phase: MatchPhase,
    pub tick: u64,
    pub round: u64,
    pub revision: u64,
    pub settings: MatchSettings,
    pub winner_slot: Option<usize>,
    deadline_tick: u64,
    pose_history: VecDeque<(u64, Vec<Option<ControllerState>>)>,
}

impl GameMatch {
    pub fn new(max_players: usize) -> vesper3d::Result<Self> {
        let room = vesper3d::viewer::test_lab::build()?;
        Ok(Self {
            room,
            players: (0..max_players).map(|_| None).collect(),
            phase: MatchPhase::Lobby,
            tick: 0,
            round: 0,
            revision: 1,
            settings: MatchSettings::default(),
            winner_slot: None,
            deadline_tick: 0,
            pose_history: VecDeque::with_capacity(16),
        })
    }

    pub fn join(&mut self, slot: usize) {
        if slot < self.players.len() {
            self.players[slot] = Some(ServerPlayer::default());
            self.invalidate_ready();
        }
    }

    pub fn leave(&mut self, slot: usize) {
        if slot < self.players.len() {
            self.players[slot] = None;
            if self.phase == MatchPhase::Playing {
                self.finish(None);
            }
            self.invalidate_ready();
        }
    }

    fn invalidate_ready(&mut self) {
        self.revision += 1;
        for p in self.players.iter_mut().flatten() {
            p.ready = false;
        }
    }

    pub fn action(&mut self, slot: usize, action: GameAction) {
        if slot >= self.players.len() || self.players[slot].is_none() {
            return;
        }
        match action {
            GameAction::Leave => self.leave(slot),
            GameAction::SelectRole(role) => {
                if self.phase == MatchPhase::Lobby {
                    let player = self.players[slot].as_mut().unwrap();
                    player.role = Some(role);
                    player.controller = Controller::for_character(role);
                    self.invalidate_ready();
                }
            }
            GameAction::Configure(settings) => {
                if self.phase == MatchPhase::Lobby && slot == 0 && settings.is_valid() {
                    self.settings = settings;
                    self.invalidate_ready();
                }
            }
            GameAction::Ready { revision } => {
                if self.phase == MatchPhase::Lobby && revision == self.revision {
                    let player = self.players[slot].as_mut().unwrap();
                    if player.role.is_some() {
                        player.ready = true;
                    }
                    // Start match if all connected players are ready
                    let connected_count = self.players.iter().flatten().count();
                    let ready_count = self.players.iter().flatten().filter(|p| p.ready).count();
                    if connected_count >= 2 && ready_count == connected_count {
                        self.start_match();
                    }
                }
            }
        }
    }

    fn start_match(&mut self) {
        self.round += 1;
        self.phase = MatchPhase::Playing;
        self.winner_slot = None;
        self.deadline_tick = self.tick + self.settings.round_seconds as u64 * HZ;
        self.pose_history.clear();

        for (i, p) in self.players.iter_mut().enumerate() {
            if let Some(player) = p {
                let role = player.role.unwrap_or(CharacterKind::Scientist);
                let mut c = Controller::for_character(role);
                c.set_physics_state(V(0.0, 1.68, if i == 0 { 4.0 } else { -4.0 }), 0.0, true);
                player.controller = c;
                player.input = GameInput::default();
                player.tracker = ActionCountersTracker::new();
                player.cooldown_ticks = 0;
            }
        }
    }

    pub fn input(&mut self, slot: usize, input: GameInput) {
        if slot >= self.players.len() || !input.is_valid() || input.round != self.round {
            return;
        }
        if let Some(player) = &mut self.players[slot] {
            if input.sequence <= player.input.sequence {
                return;
            }
            player.input = input;
            player.last_input_tick = self.tick;
        }
    }

    pub fn step(&mut self) {
        self.tick += 1;

        if self.phase == MatchPhase::Finished && self.tick >= self.deadline_tick {
            self.phase = MatchPhase::Lobby;
            self.invalidate_ready();
            for p in self.players.iter_mut().flatten() {
                p.controller.stop();
            }
        }

        if self.phase == MatchPhase::Playing && self.tick >= self.deadline_tick {
            // Find player with highest score
            let mut top_score = 0;
            let mut top_slot = None;
            for (slot, p) in self.players.iter().enumerate() {
                if let Some(player) = p {
                    if player.score > top_score {
                        top_score = player.score;
                        top_slot = Some(slot);
                    }
                }
            }
            self.finish(top_slot);
        }

        // Advance simulation for each connected player
        let mut attacks = Vec::new();

        for (slot, p) in self.players.iter_mut().enumerate() {
            let Some(player) = p else { continue; };

            let fresh = self.tick.saturating_sub(player.last_input_tick) <= 10;
            let edges = player.tracker.update(&player.input.counters);

            let mut movement = if fresh { player.input.movement } else { Movement::default() };
            movement.jump = fresh && edges.jump;

            if self.phase == MatchPhase::Playing {
                player.controller.yaw = player.input.yaw;
                player.controller.pitch = player.input.pitch;
                player.controller.update(movement, DT, &self.room.colliders);
            } else {
                player.controller.stop();
            }

            // Primary weapon fire with lag compensation aim tick
            if self.phase == MatchPhase::Playing && fresh && edges.primary && self.tick >= player.cooldown_ticks {
                player.cooldown_ticks = self.tick + 15; // 4 shots/sec cooldown
                attacks.push((slot, player.input.aim_tick));
            }
        }

        // Record pose history for lag compensation (up to 13 frames = ~216 ms)
        let current_poses = self
            .players
            .iter()
            .map(|p| p.as_ref().map(|pl| pl.controller.network_state()))
            .collect::<Vec<_>>();
        self.pose_history.push_back((self.tick, current_poses));
        while self.pose_history.len() > 14 {
            self.pose_history.pop_front();
        }

        // Resolve combat attacks against historical poses
        for (attacker_slot, aim_tick) in attacks {
            if self.phase != MatchPhase::Playing {
                break;
            }
            let attacker = &self.players[attacker_slot].as_ref().unwrap().controller;
            let ray = attacker.ray();

            // Rewind target positions using bounded history (clamp to max 12 ticks rewind)
            let clamped_aim = aim_tick.clamp(self.tick.saturating_sub(12), self.tick);
            let historical_poses = self
                .pose_history
                .iter()
                .rev()
                .find(|(t, _)| *t <= clamped_aim)
                .map(|(_, poses)| poses);

            for target_slot in 0..self.players.len() {
                if target_slot == attacker_slot || self.players[target_slot].is_none() {
                    continue;
                }
                let target_pos = historical_poses
                    .and_then(|p| p.get(target_slot))
                    .and_then(|p| p.as_ref().map(|s| s.position))
                    .unwrap_or(self.players[target_slot].as_ref().unwrap().controller.position);

                // Check distance to target ray
                let to_target = target_pos - ray.o;
                let proj = to_target.dot(ray.d);
                if proj > 0.0 && proj < 40.0 {
                    let closest = ray.o + ray.d * proj;
                    let diff = target_pos - closest;
                    let dist_sq = diff.dot(diff);
                    if dist_sq < 0.40 * 0.40 {
                        // Confirmed authoritative hit!
                        if let Some(attacker_player) = &mut self.players[attacker_slot] {
                            attacker_player.score += 1;
                            if attacker_player.score >= self.settings.score_to_win {
                                self.finish(Some(attacker_slot));
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    fn finish(&mut self, winner: Option<usize>) {
        self.phase = MatchPhase::Finished;
        self.winner_slot = winner;
        self.deadline_tick = self.tick + 5 * HZ;
    }

    pub fn snapshot(&self) -> MatchSnapshot {
        MatchSnapshot {
            tick: self.tick,
            round: self.round,
            revision: self.revision,
            phase: self.phase,
            remaining_ticks: self.deadline_tick.saturating_sub(self.tick),
            winner_slot: self.winner_slot,
            settings: self.settings,
            players: self
                .players
                .iter()
                .enumerate()
                .filter_map(|(slot, p)| {
                    p.as_ref().map(|pl| PlayerSnapshot {
                        slot,
                        role: pl.role,
                        ready: pl.ready,
                        pose: pl.controller.network_state(),
                        ack_sequence: pl.input.sequence,
                        score: pl.score,
                    })
                })
                .collect(),
        }
    }
}
