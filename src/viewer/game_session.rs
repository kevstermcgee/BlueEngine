//! Graphics-free adapter for local authority or a server-owned game mirror.
//! Only local mode steps HeadlessWorld. Online mode predicts movement and applies
//! snapshots; it never evaluates authored rules, triggers, timers or prop physics.
use super::{
    controller::{Controller, Movement},
    game::LoadedGame,
    net::*,
    savestate::{world::WorldState, Loaded, SaveError, SaveHeader, SaveSlots, Source},
    simulation::{HeadlessWorld, TICK_SECONDS},
};

type SaveResult<T> = std::result::Result<T, SaveError>;
use crate::Result;
use std::{
    collections::HashMap,
    net::SocketAddr,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GameInput {
    pub movement: Movement,
    /// Radian look deltas, applied immediately; edges are retained until a fixed tick.
    pub look: [f32; 2],
    /// Interact/carry, or restart after completion.
    pub interact: bool,
}
struct Online {
    transport: Box<dyn DatagramTransport>,
    address: SocketAddr,
    auth_key: Option<String>,
    player: Option<u64>,
    token: Option<SessionToken>,
    baseline: Option<WorldSnapshot>,
    prediction: PredictionBuffer,
    players: HashMap<u64, InterpolationBuffer<PlayerNetState>>,
    props: HashMap<String, InterpolationBuffer<PropNetState>>,
    counters: ActionCounters,
    tick: u64,
    last_tick: u64,
    round_tick: u64,
    reconciled_player_tick: u64,
    hello: Instant,
    received: Instant,
    display_tick: f32,
}
/// Small lifecycle adapter used by the stock authored-game client and generated games.
/// A local pause freezes authority. An online pause only neutralizes input.
pub struct GameSession {
    world: HeadlessWorld,
    controller: Controller,
    previous: Controller,
    remote_players: HashMap<u64, Controller>,
    online: Option<Online>,
    remainder: f64,
    jump: bool,
    interact: bool,
}
impl GameSession {
    pub fn local(game: LoadedGame) -> Result<Self> {
        let mut world = game.world()?;
        if !world.join(1) {
            return Err("Could not join local game".into());
        }
        let controller = world.player(1).unwrap().clone();
        Ok(Self {
            previous: controller.clone(),
            controller,
            world,
            remote_players: HashMap::new(),
            online: None,
            remainder: 0.,
            jump: false,
            interact: false,
        })
    }
    /// Connect using an existing UDP or QUIC transport. Failure never falls back to local play.
    pub fn connect(
        game: LoadedGame,
        transport: Box<dyn DatagramTransport>,
        address: SocketAddr,
        auth_key: Option<String>,
    ) -> Result<Self> {
        let mut session = Self::local(game)?;
        session.world.leave(1);
        transport.try_send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: 0,
                content_hash: session.world.content_hash,
            },
            address,
        )?;
        session.online = Some(Online {
            transport,
            address,
            auth_key,
            player: None,
            token: None,
            baseline: None,
            prediction: PredictionBuffer::new(128),
            players: HashMap::new(),
            props: HashMap::new(),
            counters: ActionCounters::default(),
            tick: 0,
            last_tick: 0,
            round_tick: 0,
            reconciled_player_tick: 0,
            hello: Instant::now(),
            received: Instant::now(),
            display_tick: 0.,
        });
        Ok(session)
    }
    pub fn world(&self) -> &HeadlessWorld {
        &self.world
    }
    pub fn controller(&self) -> &Controller {
        &self.controller
    }
    pub fn remote_players(&self) -> &HashMap<u64, Controller> {
        &self.remote_players
    }
    pub fn is_online(&self) -> bool {
        self.online.is_some()
    }
    pub fn connected(&self) -> bool {
        self.online.as_ref().is_none_or(|n| n.player.is_some())
    }
    pub fn player_id(&self) -> Option<u64> {
        self.online.as_ref().map_or(Some(1), |n| n.player)
    }
    pub fn pose(&self) -> Controller {
        self.controller
            .interpolated(&self.previous, (self.remainder * 60.) as f32)
    }
    /// True when this session owns its world and can be saved and loaded. An online session mirrors a
    /// server's world: save on the server (`be2-headless --save`), not on a client.
    pub fn can_save(&self) -> bool {
        self.online.is_none()
    }
    fn require_local(&self) -> SaveResult<()> {
        if self.can_save() {
            Ok(())
        } else {
            Err(SaveError::Invalid(
                "an online game is owned by the server, so it cannot be saved or loaded from a client".into(),
            ))
        }
    }
    /// Serialise the local game as a framed, checksummed save (see [`savestate`](super::savestate)).
    pub fn save_bytes(&self, label: &str) -> SaveResult<Vec<u8>> {
        self.require_local()?;
        self.world.save_bytes(label)
    }
    /// Verify a save and resume the local game from it. All-or-nothing: on any error the game continues
    /// exactly as it was. Input pending at the moment of the load is dropped.
    pub fn restore_bytes(&mut self, bytes: &[u8]) -> SaveResult<SaveHeader> {
        self.require_local()?;
        let (header, state) = self.world.parse_save(bytes)?;
        self.resume(state)?;
        Ok(header)
    }
    /// Save the local game into a named slot (atomic, with a last-good backup).
    pub fn save_to_slot(&self, slots: &SaveSlots, slot: &str, label: &str) -> SaveResult<()> {
        slots.save_framed(slot, &self.save_bytes(label)?)
    }
    /// Load a slot, falling back to its backup when the file is damaged; the returned [`Source`] says which
    /// one was used.
    pub fn load_from_slot(
        &mut self,
        slots: &SaveSlots,
        slot: &str,
    ) -> SaveResult<(SaveHeader, Source)> {
        self.require_local()?;
        let loaded = slots.load(slot)?;
        self.restore_loaded(&loaded)
    }
    /// Resume from a save that a [`SaveSlots`] or `savestate::read_save` already read and verified.
    pub fn restore_loaded(&mut self, loaded: &Loaded) -> SaveResult<(SaveHeader, Source)> {
        self.require_local()?;
        let (header, state) = self.world.parse_loaded(loaded)?;
        self.resume(state)?;
        Ok((header, loaded.source.clone()))
    }
    fn resume(&mut self, state: WorldState) -> SaveResult<()> {
        if !state.players.iter().any(|p| p.id == 1) {
            return Err(SaveError::Invalid("the save has no local player".into()));
        }
        self.world.restore_state(&state)?;
        self.controller = self
            .world
            .player(1)
            .expect("player 1 was just restored")
            .clone();
        self.previous = self.controller.clone();
        self.remainder = 0.;
        self.jump = false;
        self.interact = false;
        Ok(())
    }
    /// Poll networking even while paused. Bounded catch-up avoids simulating a stall.
    pub fn advance(&mut self, mut input: GameInput, seconds: f32, playing: bool) -> Result<usize> {
        self.poll()?;
        if !seconds.is_finite() || seconds <= 0. {
            return Ok(0);
        }
        if !playing || !self.connected() {
            input = GameInput::default();
            self.jump = false;
            self.interact = false;
            if self.online.is_none() || !self.connected() {
                self.remainder = 0.;
                self.previous = self.controller.clone();
                self.world.neutralize_input(1);
                return Ok(0);
            }
        }
        self.controller
            .look(input.look[0], input.look[1], 1., false);
        self.jump |= input.movement.jump;
        self.interact |= input.interact;
        const STEP: f64 = 1. / 60.;
        self.remainder = (self.remainder + f64::from(seconds)).min(STEP * 8.);
        let mut steps = 0;
        while self.remainder + 1e-9 >= STEP && steps < 8 {
            self.previous = self.controller.clone();
            input.movement.jump = std::mem::take(&mut self.jump);
            let action = std::mem::take(&mut self.interact);
            if let Some(net) = &mut self.online {
                net.tick += 1;
                if input.movement.jump {
                    net.counters.jump = net.counters.jump.saturating_add(1);
                }
                if action {
                    net.counters.interact = net.counters.interact.saturating_add(1);
                }
                self.controller
                    .update(input.movement, TICK_SECONDS, &self.world.room.colliders);
                let frame = InputFrame {
                    client_tick: net.tick,
                    movement: input.movement,
                    yaw: self.controller.yaw,
                    pitch: self.controller.pitch,
                    fire_wrench: false,
                    fire_pistol: false,
                    interact: action,
                    ack_server_tick: net.last_tick,
                    session_token: net.token,
                };
                net.transport.try_send_packet(
                    &Packet::SequencedInput(frame.to_sequenced(
                        self.world.game.as_ref().unwrap().state().round,
                        net.counters,
                        net.last_tick,
                    )),
                    net.address,
                )?;
                net.prediction.push(frame, self.controller.clone());
            } else {
                self.world.input(
                    1,
                    input.movement,
                    self.controller.yaw,
                    self.controller.pitch,
                );
                let round = self.world.game.as_ref().unwrap().state().round;
                if action {
                    self.world.game_action(1)?;
                }
                self.world.step();
                self.controller = self.world.player(1).unwrap().clone();
                if self.world.game.as_ref().unwrap().state().round != round {
                    self.previous = self.controller.clone();
                }
            }
            self.remainder = (self.remainder - STEP).max(0.);
            steps += 1;
        }
        self.interpolate(seconds);
        Ok(steps)
    }
    fn poll(&mut self) -> Result<()> {
        let Some(net) = &mut self.online else {
            return Ok(());
        };
        if net.received.elapsed() > Duration::from_secs(10) {
            return Err("Server connection timed out".into());
        }
        if net.player.is_none() && net.hello.elapsed() >= Duration::from_millis(500) {
            net.transport.try_send_packet(
                &Packet::Hello {
                    protocol_version: PROTOCOL_VERSION,
                    player_id: 0,
                    content_hash: self.world.content_hash,
                },
                net.address,
            )?;
            net.hello = Instant::now();
        }
        let incoming = match net.transport.receive_packets() {
            Ok(packets) => packets,
            Err(error)
                if net.player.is_none()
                    && error.downcast_ref::<std::io::Error>().is_some_and(|e| {
                        matches!(
                            e.kind(),
                            std::io::ErrorKind::ConnectionReset
                                | std::io::ErrorKind::ConnectionRefused
                        )
                    }) =>
            {
                Vec::new()
            }
            Err(error) => return Err(error),
        };
        for (packet, source) in incoming {
            if source != net.address {
                continue;
            }
            net.received = Instant::now();
            let snapshot = match packet {
                Packet::Rejected { reason } => return Err(reason.into()),
                Packet::AuthChallenge { nonce, salt } => {
                    let key = net.auth_key.as_ref().ok_or("Server requires --auth-key")?;
                    net.transport.try_send_packet(
                        &Packet::AuthResponse {
                            player_id: 0,
                            nonce,
                            proof: session::compute_auth_proof(key, nonce, 0, &salt),
                            content_hash: self.world.content_hash,
                        },
                        net.address,
                    )?;
                    None
                }
                Packet::Welcome {
                    player_id,
                    session_token,
                    ..
                } if net.player.is_none() => {
                    net.player = Some(player_id);
                    net.token = session_token;
                    self.controller = self.world.game.as_ref().unwrap().controller(player_id);
                    self.previous = self.controller.clone();
                    None
                }
                Packet::GameState {
                    tick,
                    state,
                    session,
                } if session == net.token => {
                    let game = self.world.game.as_mut().unwrap();
                    let round = game.state().round;
                    if game.accept_snapshot(tick, state) {
                        game.apply_mover_colliders(&mut self.world.room);
                        if game.state().round != round {
                            net.round_tick = tick;
                            net.baseline = None;
                            net.prediction.history.clear();
                            net.players.clear();
                            net.props.clear();
                            self.remote_players.clear();
                            self.controller = game.controller(net.player.unwrap_or(1));
                            self.previous = self.controller.clone();
                            self.jump = false;
                            self.interact = false;
                            net.transport.try_send_packet(
                                &Packet::Resynchronize {
                                    session: net.token.unwrap_or_default(),
                                    after_tick: net.last_tick.max(net.round_tick.saturating_sub(1)),
                                },
                                net.address,
                            )?;
                        }
                    }
                    None
                }
                packet @ (Packet::Snapshot(_) | Packet::Delta(_)) => {
                    let session = match &packet {
                        Packet::Snapshot(s) => s.session,
                        Packet::Delta(d) => d.session,
                        _ => unreachable!(),
                    };
                    if session != net.token {
                        continue;
                    }
                    let tick = match &packet {
                        Packet::Snapshot(s) => s.tick,
                        Packet::Delta(d) => d.target_tick,
                        _ => unreachable!(),
                    };
                    if tick < net.round_tick {
                        net.transport.try_send_packet(
                            &Packet::Resynchronize {
                                session: net.token.unwrap_or_default(),
                                after_tick: net.last_tick.max(net.round_tick.saturating_sub(1)),
                            },
                            net.address,
                        )?;
                        continue;
                    }
                    if tick <= net.last_tick {
                        continue;
                    }
                    match receive_update(&mut net.baseline, packet) {
                        Ok(snap) => snap,
                        Err(_) => {
                            net.transport.try_send_packet(
                                &Packet::Resynchronize {
                                    session: net.token.unwrap_or_default(),
                                    after_tick: net.last_tick.max(net.round_tick.saturating_sub(1)),
                                },
                                net.address,
                            )?;
                            None
                        }
                    }
                }
                _ => None,
            };
            let Some(snapshot) = snapshot else {
                continue;
            };
            if snapshot.tick <= net.last_tick || snapshot.tick < net.round_tick {
                continue;
            }
            if let Some(me) = snapshot
                .players
                .iter()
                .find(|p| Some(p.id) == net.player && p.tick > net.reconciled_player_tick)
            {
                net.reconciled_player_tick = me.tick;
                if net
                    .prediction
                    .history
                    .iter()
                    .any(|(f, _)| f.client_tick == snapshot.ack_client_tick)
                {
                    net.prediction.reconcile(
                        snapshot.ack_client_tick,
                        me,
                        &mut self.controller,
                        &self.world.room.colliders,
                        0.03,
                    );
                } else {
                    let look = (self.controller.yaw, self.controller.pitch);
                    me.apply_to_controller(&mut self.controller);
                    self.controller.yaw = look.0;
                    self.controller.pitch = look.1;
                    net.prediction.history.clear();
                }
                self.previous = self.controller.clone();
            }
            net.last_tick = snapshot.tick;
            net.display_tick = snapshot.tick.saturating_sub(6) as f32;
            net.players
                .retain(|id, _| snapshot.players.iter().any(|p| p.id == *id));
            self.remote_players
                .retain(|id, _| snapshot.players.iter().any(|p| p.id == *id));
            for player in &snapshot.players {
                if Some(player.id) != net.player {
                    net.players
                        .entry(player.id)
                        .or_insert_with(|| InterpolationBuffer::new(16))
                        .push(snapshot.tick, player.clone());
                }
            }
            net.props
                .retain(|id, _| snapshot.props.iter().any(|p| p.id == *id));
            for prop in &snapshot.props {
                net.props
                    .entry(prop.id.clone())
                    .or_insert_with(|| InterpolationBuffer::new(16))
                    .push(snapshot.tick, prop.clone());
            }
            net.baseline = Some(snapshot);
        }
        Ok(())
    }
    fn interpolate(&mut self, seconds: f32) {
        let Some(net) = &mut self.online else {
            return;
        };
        net.display_tick = (net.display_tick + seconds * 60.).min(net.last_tick as f32);
        for (id, buffer) in &net.players {
            if let Some(state) = buffer.interpolate_state_at(net.display_tick) {
                let controller = self
                    .remote_players
                    .entry(*id)
                    .or_insert_with(|| self.world.game.as_ref().unwrap().controller(*id));
                state.apply_to_controller(controller);
            }
        }
        if let Some(physics) = &mut self.world.prop_physics {
            // Snapshot transforms only: no step_simulation or local prop authority online.
            for buffer in net.props.values() {
                if let Some(state) = buffer.interpolate_at(net.display_tick) {
                    physics.set_prop_transform_and_vel(
                        &state.id,
                        state.position,
                        state.rotation,
                        state.linear_velocity,
                        state.angular_velocity,
                        state.sleeping,
                    );
                    if let Some(index) = physics.props.iter().position(|p| p.id == state.id) {
                        if let Some(holder) = state.held_by {
                            physics.set_held_for_player(holder, index);
                        } else if let Some(holder) = physics.holder_of(index) {
                            physics.drop_for_player(holder);
                        }
                    }
                }
            }
            physics.sync(&mut self.world.room);
            self.world
                .game
                .as_mut()
                .unwrap()
                .apply_mover_colliders(&mut self.world.room);
        }
    }
}
impl Drop for GameSession {
    fn drop(&mut self) {
        if let Some(net) = &self.online {
            if let Some(player_id) = net.player {
                let _ = net.transport.try_send_packet(
                    &Packet::Disconnect {
                        player_id,
                        session_token: net.token,
                    },
                    net.address,
                );
            }
        }
    }
}
