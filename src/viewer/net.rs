//! Low-latency server-authoritative multiplayer networking foundation.
//!
//! Provides:
//! - UDP packet protocol with protocol versioning
//! - Fixed-tick input framing and sequence tracking
//! - Authoritative world snapshots and delta compression
//! - Client-side prediction and server reconciliation replay
//! - Snapshot interpolation buffer for remote entities
//! - Network condition harness (artificial latency, jitter, loss)
use super::{
    controller::{Collider, Controller, Movement},
    lifecycle::Generation,
    spatial::RoomId,
};
use crate::math::V;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_PACKET_BYTES: usize = 1400; // Safe MTU size

/// Replicated network state for one player.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerNetState {
    pub id: u64,
    pub tick: u64,
    pub position: V,
    pub yaw: f32,
    pub pitch: f32,
    pub vertical_vel: f32,
    pub grounded: bool,
    pub crouched: bool,
    #[serde(default)]
    pub character_kind: crate::viewer::controller::CharacterKind,
    pub room_id: Option<RoomId>,
}

impl PlayerNetState {
    pub fn from_controller(id: u64, tick: u64, c: &Controller, room_id: Option<RoomId>) -> Self {
        Self {
            id,
            tick,
            position: c.position,
            yaw: c.yaw,
            pitch: c.pitch,
            vertical_vel: c.vertical_velocity(),
            grounded: c.is_grounded(),
            crouched: c.is_crouched(),
            character_kind: c.character_kind(),
            room_id,
        }
    }

    pub fn apply_to_controller(&self, c: &mut Controller) {
        c.yaw = self.yaw;
        c.pitch = self.pitch;
        c.set_physics_state(self.position, self.vertical_vel, self.grounded);
    }
}

/// Replicated network state for one dynamic/promoted prop.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PropNetState {
    pub id: String,
    pub position: V,
    pub rotation: [f32; 4],
    pub linear_velocity: V,
    pub angular_velocity: V,
    pub sleeping: bool,
    pub held_by: Option<u64>,
    pub generation: Generation,
}

impl PropNetState {
    pub fn is_held(&self) -> bool {
        self.held_by.is_some()
    }
}

/// Complete authoritative state snapshot for a simulation tick.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub tick: u64,
    pub ack_client_tick: u64,
    pub players: Vec<PlayerNetState>,
    pub props: Vec<PropNetState>,
}

impl WorldSnapshot {
    /// Compute a delta snapshot relative to a baseline snapshot.
    /// Only entities whose state has changed since `base` are included.
    pub fn compute_delta(&self, base: &WorldSnapshot) -> DeltaSnapshot {
        let mut changed_players = Vec::new();
        for p in &self.players {
            if let Some(old) = base.players.iter().find(|o| o.id == p.id) {
                if old != p {
                    changed_players.push(p.clone());
                }
            } else {
                changed_players.push(p.clone());
            }
        }

        let mut changed_props = Vec::new();
        for pr in &self.props {
            if let Some(old) = base.props.iter().find(|o| o.id == pr.id) {
                if old != pr {
                    changed_props.push(pr.clone());
                }
            } else {
                changed_props.push(pr.clone());
            }
        }

        let mut removed_players = Vec::new();
        for old in &base.players {
            if !self.players.iter().any(|p| p.id == old.id) {
                removed_players.push(old.id);
            }
        }

        let mut removed_props = Vec::new();
        for old in &base.props {
            if !self.props.iter().any(|pr| pr.id == old.id) {
                removed_props.push(old.id.clone());
            }
        }

        DeltaSnapshot {
            base_tick: base.tick,
            target_tick: self.tick,
            ack_client_tick: self.ack_client_tick,
            changed_players,
            changed_props,
            removed_players,
            removed_props,
        }
    }
}

/// Compact delta snapshot transmitting only changes and removals between two ticks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeltaSnapshot {
    pub base_tick: u64,
    pub target_tick: u64,
    pub ack_client_tick: u64,
    pub changed_players: Vec<PlayerNetState>,
    pub changed_props: Vec<PropNetState>,
    pub removed_players: Vec<u64>,
    pub removed_props: Vec<String>,
}

impl DeltaSnapshot {
    /// Apply delta changes onto an existing base snapshot to reconstruct the full state.
    pub fn apply_to(&self, base: &WorldSnapshot) -> WorldSnapshot {
        let mut players = base.players.clone();
        players.retain(|p| !self.removed_players.contains(&p.id));
        for changed in &self.changed_players {
            if let Some(existing) = players.iter_mut().find(|p| p.id == changed.id) {
                *existing = changed.clone();
            } else {
                players.push(changed.clone());
            }
        }

        let mut props = base.props.clone();
        props.retain(|pr| !self.removed_props.contains(&pr.id));
        for changed in &self.changed_props {
            if let Some(existing) = props.iter_mut().find(|pr| pr.id == changed.id) {
                *existing = changed.clone();
            } else {
                props.push(changed.clone());
            }
        }

        WorldSnapshot {
            tick: self.target_tick,
            ack_client_tick: self.ack_client_tick,
            players,
            props,
        }
    }
}

/// Client input sent to the authoritative server each tick.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InputFrame {
    pub client_tick: u64,
    pub movement: Movement,
    pub yaw: f32,
    pub pitch: f32,
    pub fire_wrench: bool,
    pub fire_pistol: bool,
    pub interact: bool,
    #[serde(default)]
    pub ack_server_tick: u64,
}

/// Network packets exchanged between client and dedicated server.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Packet {
    Hello {
        protocol_version: u32,
        player_id: u64,
    },
    Welcome {
        player_id: u64,
        server_tick: u64,
        map_name: String,
    },
    Input(InputFrame),
    Snapshot(WorldSnapshot),
    Delta(DeltaSnapshot),
    RequestKeyframe,
    Ping {
        seq: u32,
        send_time_ms: u64,
    },
    Pong {
        seq: u32,
        send_time_ms: u64,
    },
    Disconnect {
        player_id: u64,
    },
}

impl Packet {
    pub fn encode(&self) -> crate::Result<Vec<u8>> {
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() > MAX_PACKET_BYTES {
            return Err("Packet exceeds MTU limit".into());
        }
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> crate::Result<Self> {
        if bytes.len() > MAX_PACKET_BYTES {
            return Err("Packet exceeds MTU limit".into());
        }
        let pkt: Self = serde_json::from_slice(bytes)?;
        Ok(pkt)
    }
}

/// Client prediction buffer storing historical inputs and predicted poses.
#[derive(Default)]
pub struct PredictionBuffer {
    pub history: VecDeque<(InputFrame, Controller)>,
    pub max_history: usize,
    pub last_correction_error: f32,
}

impl PredictionBuffer {
    pub fn new(max_history: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(max_history),
            max_history,
            last_correction_error: 0.0,
        }
    }

    /// Record an input frame and the resulting predicted controller state.
    pub fn push(&mut self, input: InputFrame, predicted: Controller) {
        if self.history.len() >= self.max_history {
            self.history.pop_front();
        }
        self.history.push_back((input, predicted));
    }

    /// Reconcile with incoming authoritative state.
    /// If server position differs from client prediction at acked tick by > error_threshold,
    /// snap to server state and replay all subsequent unacknowledged inputs!
    pub fn reconcile(
        &mut self,
        ack_tick: u64,
        server_state: &PlayerNetState,
        controller: &mut Controller,
        colliders: &[Collider],
        error_threshold: f32,
    ) -> bool {
        // Find the index in history matching ack_tick
        let mut match_idx = None;
        for (i, (inp, _)) in self.history.iter().enumerate() {
            if inp.client_tick == ack_tick {
                match_idx = Some(i);
                break;
            }
        }

        let Some(idx) = match_idx else {
            // Ack is too old or unknown, discard history up to ack_tick
            return false;
        };

        let (_, predicted_pose) = &self.history[idx];
        let error = (predicted_pose.position - server_state.position).length();
        self.last_correction_error = error;

        // Discard frames up to and including the acked tick
        self.history.drain(0..=idx);

        if error > error_threshold {
            // Discrepancy detected: Reconcile!
            // 1. Reset controller to authoritative server state
            server_state.apply_to_controller(controller);

            // 2. Replay all remaining unacknowledged inputs
            for (inp, recorded_pred) in self.history.iter_mut() {
                controller.yaw = inp.yaw;
                controller.pitch = inp.pitch;
                controller.update(inp.movement, super::simulation::TICK_SECONDS, colliders);
                *recorded_pred = controller.clone();
            }
            true
        } else {
            false
        }
    }
}

/// Snapshot interpolation buffer for smooth rendering of remote players and props.
pub struct InterpolationBuffer<T> {
    pub snapshots: VecDeque<(u64, T)>,
    pub max_size: usize,
}

impl<T: Clone> InterpolationBuffer<T> {
    pub fn new(max_size: usize) -> Self {
        Self {
            snapshots: VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    pub fn push(&mut self, tick: u64, item: T) {
        if self.snapshots.len() >= self.max_size {
            self.snapshots.pop_front();
        }
        self.snapshots.push_back((tick, item));
    }

    pub fn sample_latest(&self) -> Option<&T> {
        self.snapshots.back().map(|(_, item)| item)
    }
}

impl InterpolationBuffer<PlayerNetState> {
    /// Linearly interpolate remote player position at fractional render tick.
    pub fn interpolate_at(&self, render_tick: f32) -> Option<V> {
        if self.snapshots.is_empty() {
            return None;
        }
        if self.snapshots.len() == 1 {
            return Some(self.snapshots[0].1.position);
        }

        for i in 0..self.snapshots.len() - 1 {
            let (t0, s0) = &self.snapshots[i];
            let (t1, s1) = &self.snapshots[i + 1];
            let t0_f = *t0 as f32;
            let t1_f = *t1 as f32;
            if render_tick >= t0_f && render_tick <= t1_f {
                let alpha = if (t1_f - t0_f).abs() > 1e-4 {
                    (render_tick - t0_f) / (t1_f - t0_f)
                } else {
                    0.0
                };
                return Some(s0.position.lerp(s1.position, alpha.clamp(0.0, 1.0)));
            }
        }

        if render_tick < self.snapshots[0].0 as f32 {
            Some(self.snapshots[0].1.position)
        } else {
            Some(self.snapshots.back().unwrap().1.position)
        }
    }

    /// Interpolate full remote player state (position, yaw, pitch) at fractional render tick.
    pub fn interpolate_state_at(&self, render_tick: f32) -> Option<PlayerNetState> {
        if self.snapshots.is_empty() {
            return None;
        }
        if self.snapshots.len() == 1 {
            return Some(self.snapshots[0].1.clone());
        }

        for i in 0..self.snapshots.len() - 1 {
            let (t0, s0) = &self.snapshots[i];
            let (t1, s1) = &self.snapshots[i + 1];
            let t0_f = *t0 as f32;
            let t1_f = *t1 as f32;
            if render_tick >= t0_f && render_tick <= t1_f {
                let alpha = if (t1_f - t0_f).abs() > 1e-4 {
                    ((render_tick - t0_f) / (t1_f - t0_f)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let pos = s0.position.lerp(s1.position, alpha);
                let yaw = s0.yaw + (s1.yaw - s0.yaw) * alpha;
                let pitch = s0.pitch + (s1.pitch - s0.pitch) * alpha;
                let mut state = s1.clone();
                state.position = pos;
                state.yaw = yaw;
                state.pitch = pitch;
                return Some(state);
            }
        }

        if render_tick < self.snapshots[0].0 as f32 {
            Some(self.snapshots[0].1.clone())
        } else {
            Some(self.snapshots.back().unwrap().1.clone())
        }
    }
}

/// Normalized linear quaternion interpolation between two unit quaternions [x, y, z, w].
pub fn nlerp_quat(q0: [f32; 4], mut q1: [f32; 4], t: f32) -> [f32; 4] {
    let dot = q0[0] * q1[0] + q0[1] * q1[1] + q0[2] * q1[2] + q0[3] * q1[3];
    if dot < 0.0 {
        q1 = [-q1[0], -q1[1], -q1[2], -q1[3]];
    }
    let inv_t = 1.0 - t;
    let x = inv_t * q0[0] + t * q1[0];
    let y = inv_t * q0[1] + t * q1[1];
    let z = inv_t * q0[2] + t * q1[2];
    let w = inv_t * q0[3] + t * q1[3];
    let len = (x * x + y * y + z * z + w * w).sqrt();
    if len > 1e-6 {
        [x / len, y / len, z / len, w / len]
    } else {
        q0
    }
}

impl InterpolationBuffer<PropNetState> {
    /// Smoothly interpolate prop state (position, rotation, linear & angular velocity) at fractional render tick.
    pub fn interpolate_at(&self, render_tick: f32) -> Option<PropNetState> {
        if self.snapshots.is_empty() {
            return None;
        }
        if self.snapshots.len() == 1 {
            return Some(self.snapshots[0].1.clone());
        }

        for i in 0..self.snapshots.len() - 1 {
            let (t0, s0) = &self.snapshots[i];
            let (t1, s1) = &self.snapshots[i + 1];
            let t0_f = *t0 as f32;
            let t1_f = *t1 as f32;
            if render_tick >= t0_f && render_tick <= t1_f {
                let alpha = if (t1_f - t0_f).abs() > 1e-4 {
                    ((render_tick - t0_f) / (t1_f - t0_f)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let pos = s0.position.lerp(s1.position, alpha);
                let rot = nlerp_quat(s0.rotation, s1.rotation, alpha);
                let linvel = s0.linear_velocity.lerp(s1.linear_velocity, alpha);
                let angvel = s0.angular_velocity.lerp(s1.angular_velocity, alpha);
                let mut state = s1.clone();
                state.position = pos;
                state.rotation = rot;
                state.linear_velocity = linvel;
                state.angular_velocity = angvel;
                return Some(state);
            }
        }

        if render_tick < self.snapshots[0].0 as f32 {
            Some(self.snapshots[0].1.clone())
        } else {
            Some(self.snapshots.back().unwrap().1.clone())
        }
    }
}

/// Simulated network packet with delivery schedule.
#[derive(Clone, Debug)]
pub struct DelayedPacket<T> {
    pub delivery_tick: u64,
    pub packet: T,
}

/// Network condition simulator for test harnesses (latency, loss, jitter).
#[derive(Clone, Debug, Default)]
pub struct NetworkSimulator<T = Packet> {
    pub latency_ms: u64,
    pub packet_loss_rate: f32, // 0.0 to 1.0
    packet_counter: u64,
    pub queue: VecDeque<DelayedPacket<T>>,
}

impl<T> NetworkSimulator<T> {
    pub fn new(latency_ms: u64, packet_loss_rate: f32) -> Self {
        Self {
            latency_ms,
            packet_loss_rate,
            packet_counter: 0,
            queue: VecDeque::new(),
        }
    }

    /// Enqueue a packet for simulated delivery. Returns false if dropped by packet loss.
    pub fn send(&mut self, current_tick: u64, packet: T) -> bool {
        if self.packet_loss_rate > 0.0 {
            self.packet_counter += 1;
            let modulus = (1.0 / self.packet_loss_rate).round() as u64;
            if modulus > 0 && self.packet_counter.is_multiple_of(modulus) {
                return false; // Dropped!
            }
        }
        let delay_ticks = ((self.latency_ms as f64 / 1000.0)
            / crate::viewer::simulation::TICK_SECONDS as f64)
            .round() as u64;
        self.queue.push_back(DelayedPacket {
            delivery_tick: current_tick + delay_ticks,
            packet,
        });
        true
    }

    /// Deliver all packets whose delivery schedule has elapsed by current_tick.
    pub fn receive(&mut self, current_tick: u64) -> Vec<T> {
        let mut ready = Vec::new();
        let mut i = 0;
        while i < self.queue.len() {
            if self.queue[i].delivery_tick <= current_tick {
                ready.push(self.queue.remove(i).unwrap().packet);
            } else {
                i += 1;
            }
        }
        ready
    }

    /// Returns true if packet should be dropped to simulate packet loss.
    pub fn should_drop(&mut self) -> bool {
        if self.packet_loss_rate <= 0.0 {
            return false;
        }
        self.packet_counter += 1;
        let modulus = (1.0 / self.packet_loss_rate).round() as u64;
        modulus > 0 && self.packet_counter.is_multiple_of(modulus)
    }
}

use std::net::{SocketAddr, UdpSocket};

/// Non-blocking UDP transport layer for authoritative server and clients.
pub struct UdpTransport {
    pub socket: UdpSocket,
    recv_buf: [u8; MAX_PACKET_BYTES],
}

impl UdpTransport {
    /// Bind to a local address (e.g. `"127.0.0.1:0"`) and set non-blocking mode.
    pub fn bind(addr: &str) -> crate::Result<Self> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            recv_buf: [0u8; MAX_PACKET_BYTES],
        })
    }

    /// Local socket address.
    pub fn local_addr(&self) -> crate::Result<SocketAddr> {
        Ok(self.socket.local_addr()?)
    }

    /// Send a packet to destination address.
    pub fn send_packet(&self, packet: &Packet, dest: SocketAddr) -> crate::Result<usize> {
        let bytes = packet.encode()?;
        let sent = self.socket.send_to(&bytes, dest)?;
        Ok(sent)
    }

    /// Receive a packet from socket if available (non-blocking).
    pub fn recv_packet(&mut self) -> crate::Result<Option<(Packet, SocketAddr)>> {
        match self.socket.recv_from(&mut self.recv_buf) {
            Ok((len, src)) => {
                let packet = Packet::decode(&self.recv_buf[..len])?;
                Ok(Some((packet, src)))
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_encode_decode_roundtrip() {
        let pkt = Packet::Hello {
            protocol_version: PROTOCOL_VERSION,
            player_id: 42,
        };
        let bytes = pkt.encode().unwrap();
        let decoded = Packet::decode(&bytes).unwrap();
        match decoded {
            Packet::Hello {
                protocol_version,
                player_id,
            } => {
                assert_eq!(protocol_version, PROTOCOL_VERSION);
                assert_eq!(player_id, 42);
            }
            _ => panic!("Unexpected packet variant"),
        }
    }

    #[test]
    fn delta_compression_roundtrip() {
        let snap1 = WorldSnapshot {
            tick: 100,
            ack_client_tick: 50,
            players: vec![PlayerNetState {
                id: 1,
                tick: 100,
                position: V(1.0, 0.0, 1.0),
                yaw: 0.0,
                pitch: 0.0,
                vertical_vel: 0.0,
                grounded: true,
                crouched: false,
                character_kind: Default::default(),
                room_id: Some(RoomId(1)),
            }],
            props: vec![],
        };

        let mut snap2 = snap1.clone();
        snap2.tick = 101;
        snap2.players[0].position = V(1.1, 0.0, 1.0);

        let delta = snap2.compute_delta(&snap1);
        assert_eq!(delta.changed_players.len(), 1);
        assert_eq!(delta.changed_players[0].position, V(1.1, 0.0, 1.0));

        let reconstructed = delta.apply_to(&snap1);
        assert_eq!(reconstructed, snap2);
    }

    #[test]
    fn prediction_reconciliation_replays_inputs_on_error() {
        let mut pred = PredictionBuffer::new(64);
        let mut controller = Controller::default();
        let colliders = vec![];

        // Simulate 3 ticks ahead on client
        for tick in 1..=3 {
            let input = InputFrame {
                client_tick: tick,
                movement: Movement {
                    forward: 1.0,
                    ..Default::default()
                },
                yaw: 0.0,
                pitch: 0.0,
                fire_wrench: false,
                fire_pistol: false,
                interact: false,
                ack_server_tick: 0,
            };
            controller.update(
                input.movement,
                crate::viewer::simulation::TICK_SECONDS,
                &colliders,
            );
            pred.push(input, controller.clone());
        }

        // Server sends correction for tick 1 with slightly different position
        let mut server_state = PlayerNetState::from_controller(1, 1, &Controller::default(), None);
        server_state.position = V(0.0, 0.0, 0.05); // shifted by 0.05m

        let reconciled = pred.reconcile(1, &server_state, &mut controller, &colliders, 0.01);
        assert!(reconciled);
        assert!(pred.last_correction_error > 0.01);
        // Only ticks 2 and 3 remain in history after tick 1 was acked
        assert_eq!(pred.history.len(), 2);
    }

    #[test]
    fn udp_transport_localhost_roundtrip() {
        let mut server = UdpTransport::bind("127.0.0.1:0").unwrap();
        let server_addr = server.local_addr().unwrap();

        let client = UdpTransport::bind("127.0.0.1:0").unwrap();
        let client_addr = client.local_addr().unwrap();

        let ping = Packet::Ping {
            seq: 1,
            send_time_ms: 1234,
        };
        client.send_packet(&ping, server_addr).unwrap();

        // Non-blocking loop waiting for packet arrival
        let mut received = None;
        for _ in 0..100 {
            if let Ok(Some((pkt, src))) = server.recv_packet() {
                received = Some((pkt, src));
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        assert!(received.is_some());
        let (pkt, src) = received.unwrap();
        assert_eq!(src, client_addr);
        match pkt {
            Packet::Ping { seq, send_time_ms } => {
                assert_eq!(seq, 1);
                assert_eq!(send_time_ms, 1234);
            }
            _ => panic!("Expected Ping packet"),
        }
    }
}
