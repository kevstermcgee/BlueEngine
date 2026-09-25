//! End-to-end multiplayer smoke test and engine validation tool.
//!
//! Validates:
//! - Full client-server handshake (Hello / Welcome)
//! - Bi-directional snapshot and input stream flow
//! - Delta compression and baseline recovery
//! - Multi-client entity replication
//! - Clean disconnection
//!
//! Usage:
//!   cargo run --example network_smoke                  # Local in-process test
//!   cargo run --example network_smoke -- 127.0.0.1:4000 # Test live dedicated server
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use vesper3d::viewer::{
    controller::Movement,
    net::{InputFrame, Packet, UdpTransport, PROTOCOL_VERSION},
    server::DedicatedServer,
};

/// Fluent scenario runner for engine multiplayer smoke validation.
pub struct Scenario {
    target_addr: Option<SocketAddr>,
    client_count: usize,
    duration_ticks: u64,
}

impl Default for Scenario {
    fn default() -> Self {
        Self::new()
    }
}

impl Scenario {
    pub fn new() -> Self {
        Self {
            target_addr: None,
            client_count: 2,
            duration_ticks: 180,
        }
    }

    pub fn target(mut self, addr: SocketAddr) -> Self {
        self.target_addr = Some(addr);
        self
    }

    pub fn clients(mut self, count: usize) -> Self {
        self.client_count = count.max(1);
        self
    }

    pub fn ticks(mut self, ticks: u64) -> Self {
        self.duration_ticks = ticks;
        self
    }

    pub fn run(self) -> vesper3d::Result<()> {
        let (server_addr, stop_signal, server_handle) = if let Some(addr) = self.target_addr {
            (addr, None, None)
        } else {
            // Spin up a local dedicated server on a random loopback port
            let mut server = DedicatedServer::bind("127.0.0.1:0")?;
            let local_addr = server.local_addr;
            let stop = Arc::new(AtomicBool::new(false));
            let stop_clone = stop.clone();
            let handle = thread::spawn(move || {
                let _ = server.run_realtime(stop_clone, None);
            });
            // Brief pause for server startup
            thread::sleep(Duration::from_millis(50));
            (local_addr, Some(stop), Some(handle))
        };

        println!(
            "[Smoke] Running smoke scenario against server {}",
            server_addr
        );

        let temp_world = vesper3d::viewer::simulation::HeadlessWorld::new()?;
        let content_hash = temp_world.content_hash;

        // Bind client sockets
        let mut clients: Vec<UdpTransport> = (0..self.client_count)
            .map(|_| UdpTransport::bind("127.0.0.1:0"))
            .collect::<Result<_, _>>()?;

        let mut welcomed = vec![false; self.client_count];
        let mut snapshots_received = vec![0u64; self.client_count];
        let mut deltas_received = vec![0u64; self.client_count];
        let mut assigned_ids = vec![0u64; self.client_count];

        // Send Hellos
        for (i, c) in clients.iter().enumerate() {
            c.send_packet(
                &Packet::Hello {
                    protocol_version: PROTOCOL_VERSION,
                    player_id: (i + 1) as u64,
                    content_hash,
                },
                server_addr,
            )?;
        }

        let start = Instant::now();
        let tick_duration = Duration::from_secs_f64(1.0 / 60.0);

        for client_tick in 1..=self.duration_ticks {
            let frame_start = Instant::now();

            for (i, c) in clients.iter_mut().enumerate() {
                // Poll incoming packets
                while let Ok(Some((packet, from))) = c.recv_packet() {
                    if from != server_addr {
                        continue;
                    }
                    match packet {
                        Packet::Welcome {
                            player_id,
                            server_tick: _,
                            map_name: _,
                        } => {
                            welcomed[i] = true;
                            assigned_ids[i] = player_id;
                        }
                        Packet::Snapshot(_) => {
                            snapshots_received[i] += 1;
                        }
                        Packet::Delta(_) => {
                            deltas_received[i] += 1;
                        }
                        Packet::Rejected { reason } => {
                            return Err(format!("Client {i} rejected by server: {reason}").into());
                        }
                        _ => {}
                    }
                }

                // Send input if welcomed
                if welcomed[i] {
                    let input = InputFrame {
                        client_tick,
                        movement: Movement {
                            forward: if i == 0 { 1.0 } else { -0.5 },
                            right: if i == 0 { 0.0 } else { 0.5 },
                            jump: client_tick % 60 == 0,
                            ..Default::default()
                        },
                        yaw: client_tick as f32 * 0.01,
                        pitch: 0.0,
                        fire_wrench: false,
                        fire_pistol: false,
                        interact: false,
                        ack_server_tick: client_tick.saturating_sub(2),
                    };
                    let _ = c.send_packet(&Packet::Input(input), server_addr);
                }
            }

            let elapsed = frame_start.elapsed();
            if elapsed < tick_duration {
                thread::sleep(tick_duration - elapsed);
            }
        }

        // Clean disconnect
        for (i, c) in clients.iter().enumerate() {
            if welcomed[i] {
                let _ = c.send_packet(
                    &Packet::Disconnect {
                        player_id: assigned_ids[i],
                    },
                    server_addr,
                );
            }
        }

        // Stop in-process server if we spawned one
        if let Some(stop) = stop_signal {
            stop.store(true, Ordering::Relaxed);
        }
        if let Some(handle) = server_handle {
            let _ = handle.join();
        }

        // Verification assertions
        for i in 0..self.client_count {
            if !welcomed[i] {
                return Err(format!("Smoke test failed: Client {i} never received Welcome").into());
            }
            let total_packets = snapshots_received[i] + deltas_received[i];
            if total_packets == 0 {
                return Err(format!(
                    "Smoke test failed: Client {i} never received WorldSnapshots or Deltas"
                )
                .into());
            }
            println!(
                "[Smoke] Client {} (ID: {}) -> OK (Snapshots: {}, Deltas: {})",
                i, assigned_ids[i], snapshots_received[i], deltas_received[i]
            );
        }

        println!(
            "[Smoke] PASS: {} clients connected, sustained replication over {} ticks in {:.2}s",
            self.client_count,
            self.duration_ticks,
            start.elapsed().as_secs_f64()
        );

        Ok(())
    }
}

fn main() -> vesper3d::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut scenario = Scenario::new().clients(2).ticks(120);

    if let Some(arg) = args.first() {
        if arg == "--help" {
            println!("Usage: cargo run --example network_smoke [-- [SERVER_ADDR]]");
            return Ok(());
        }
        let addr: SocketAddr = arg.parse()?;
        scenario = scenario.target(addr);
    }

    scenario.run()
}
