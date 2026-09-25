//! Dedicated authoritative multiplayer game server runtime.
use super::{
    game::GameMatch,
    lobby::{WireMessage, PROTOCOL_VERSION},
};
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use vesper3d::viewer::{
    metrics::FixedTickRunner,
    net::{
        session::{HandshakeLimiter, SessionRegistry},
        transport::DatagramTransport,
        UdpTransport,
    },
};

pub struct DedicatedServer {
    game: GameMatch,
    transport: UdpTransport,
    registry: SessionRegistry<usize>, // maps session token to player slot
    limiter: HandshakeLimiter,
    join_key: String,
    local_addr: SocketAddr,
}

impl DedicatedServer {
    pub fn bind(addr: &str, join_key: String, max_players: usize) -> vesper3d::Result<Self> {
        let transport = UdpTransport::bind(addr)?;
        let local_addr = transport.local_addr()?;
        let game = GameMatch::new(max_players)?;
        let registry = SessionRegistry::new(max_players, Duration::from_secs(6));
        let limiter = HandshakeLimiter::new(16);

        Ok(Self {
            game,
            transport,
            registry,
            limiter,
            join_key,
            local_addr,
        })
    }

    #[allow(dead_code)]
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub fn run(&mut self, stop_signal: Arc<AtomicBool>, max_ticks: Option<u64>) -> vesper3d::Result<()> {
        let mut runner = FixedTickRunner::new(60);
        println!("[Server] Dedicated game server listening on {}", self.local_addr);

        while !stop_signal.load(Ordering::Relaxed) {
            let tick_start = Instant::now();
            let now = Instant::now();

            // Receive network packets
            let dgrams = self.transport.receive()?;
            for d in dgrams {
                if let Some(msg) = WireMessage::decode(&d.data) {
                    self.handle_message(d.peer, msg, now)?;
                }
            }

            // Check timeouts
            let expired = self.registry.evict_timeouts(now);
            for entry in expired {
                let slot = entry.data;
                println!("[Server] Client in slot {slot} timed out");
                self.game.leave(slot);
            }

            // Advance authoritative game simulation
            self.game.step();

            // Broadcast snapshots at 20 Hz (every 3 ticks)
            if self.game.tick.is_multiple_of(3) {
                let snapshot = self.game.snapshot();
                for session in self.registry.iter() {
                    let msg = WireMessage::Snapshot {
                        token: session.token,
                        command_ack: session.last_command_sequence,
                        state: snapshot.clone(),
                    };
                    if let Ok(bytes) = msg.encode() {
                        let _ = self.transport.send(session.peer, &bytes);
                    }
                }
            }

            if let Some(max) = max_ticks {
                if self.game.tick >= max {
                    break;
                }
            }

            let elapsed = tick_start.elapsed().as_micros();
            runner.sleep_until_next_tick(elapsed);
        }

        println!("[Server] Clean shutdown after {} ticks", self.game.tick);
        Ok(())
    }

    fn handle_message(&mut self, peer: SocketAddr, msg: WireMessage, now: Instant) -> vesper3d::Result<()> {
        match msg {
            WireMessage::Hello { version, nonce, key } => {
                if !self.limiter.allow(now) {
                    return Ok(());
                }
                if version != PROTOCOL_VERSION {
                    let reject = WireMessage::Reject {
                        nonce,
                        reason: "Protocol version mismatch".into(),
                    };
                    self.transport.send(peer, &reject.encode()?)?;
                    return Ok(());
                }
                if key != self.join_key {
                    let reject = WireMessage::Reject {
                        nonce,
                        reason: "Incorrect join key".into(),
                    };
                    self.transport.send(peer, &reject.encode()?)?;
                    return Ok(());
                }

                // Find free slot
                let free_slot = self.game.players.iter().position(Option::is_none);
                if let Some(slot) = free_slot {
                    let token = self.registry.register(peer, nonce, now, slot)?;
                    self.game.join(slot);
                    let welcome = WireMessage::Welcome { nonce, token, slot };
                    self.transport.send(peer, &welcome.encode()?)?;
                    println!("[Server] Welcomed peer {peer} into slot {slot}");
                } else {
                    let reject = WireMessage::Reject {
                        nonce,
                        reason: "Server is full".into(),
                    };
                    self.transport.send(peer, &reject.encode()?)?;
                }
            }
            WireMessage::Input { token, input } => {
                if self.registry.accept_input_seq(&token, input.sequence, now) {
                    if let Some(entry) = self.registry.get(&token) {
                        let slot = entry.data;
                        self.game.input(slot, input);
                    }
                }
            }
            WireMessage::Command { token, sequence, action } => {
                if self.registry.accept_command_seq(&token, sequence, now) {
                    if let Some(entry) = self.registry.get(&token) {
                        let slot = entry.data;
                        self.game.action(slot, action);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
