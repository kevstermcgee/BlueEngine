//! Load generator for a running dedicated server (development transport).
//!
//! Joins N synthetic clients, moves them at 60 Hz for a while and prints ONE JSON line: how many
//! joined or were refused, packets and bytes received, the largest packet, gaps between world
//! updates and whether this generator itself kept pace. `tools/perf.py record --suite server`
//! drives it and adds the server's CPU, memory and tick times.
//!
//! Usage: cargo run --release --example server_load -- ADDR [--clients N] [--seconds S]
//! The server must run the same map (default house); use `--map`/`--game` on neither side.
use std::{
    net::SocketAddr,
    thread,
    time::{Duration, Instant},
};
use vesper3d::viewer::{
    controller::Movement,
    net::{receive_update, InputFrame, Packet, UdpTransport, PROTOCOL_VERSION},
    simulation::HeadlessWorld,
};

#[derive(Default)]
struct Stats {
    welcomed: bool,
    rejected: Option<String>,
    welcome_ms: Option<f64>,
    id: u64,
    token: Option<[u64; 2]>,
    baseline: Option<vesper3d::viewer::net::WorldSnapshot>,
    snapshots: u64,
    deltas: u64,
    resyncs: u64,
    bytes: u64,
    max_packet: usize,
    last_update: Option<Instant>,
    gap_sum_ms: f64,
    gaps: u64,
    max_gap_ms: f64,
    gaps_over_100ms: u64,
}

fn main() -> vesper3d::Result<()> {
    let mut args = std::env::args().skip(1);
    let addr: SocketAddr = args
        .next()
        .ok_or("usage: server_load ADDR [--clients N] [--seconds S]")?
        .parse()?;
    let (mut clients_n, mut seconds) = (2usize, 20.0f64);
    while let Some(flag) = args.next() {
        let value = args.next().ok_or("flag needs a value")?;
        match flag.as_str() {
            "--clients" => clients_n = value.parse()?,
            "--seconds" => seconds = value.parse()?,
            _ => return Err(format!("unknown flag {flag}").into()),
        }
    }

    let content_hash = HeadlessWorld::new()?.content_hash;
    let mut sockets: Vec<UdpTransport> = (0..clients_n)
        .map(|_| UdpTransport::bind("127.0.0.1:0"))
        .collect::<Result<_, _>>()?;
    let mut stats: Vec<Stats> = (0..clients_n).map(|_| Stats::default()).collect();
    let hello_at = Instant::now();
    for (i, socket) in sockets.iter().enumerate() {
        socket.send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: (i + 1) as u64,
                content_hash,
            },
            addr,
        )?;
    }

    let frame = Duration::from_secs_f64(1.0 / 60.0);
    let (started, mut late_frames, mut worst_late_ms) = (Instant::now(), 0u64, 0.0f64);
    let mut tick = 0u64;
    while started.elapsed().as_secs_f64() < seconds {
        tick += 1;
        let frame_start = Instant::now();
        for (i, socket) in sockets.iter_mut().enumerate() {
            let s = &mut stats[i];
            while let Ok(Some((packet, from))) = socket.recv_packet() {
                if from != addr {
                    continue;
                }
                s.bytes += packet.encode().map_or(0, |b| b.len() as u64);
                s.max_packet = s.max_packet.max(packet.encode().map_or(0, |b| b.len()));
                match packet {
                    Packet::Welcome {
                        player_id,
                        session_token,
                        ..
                    } => {
                        s.welcomed = true;
                        s.id = player_id;
                        s.token = session_token;
                        s.welcome_ms = Some(hello_at.elapsed().as_secs_f64() * 1000.0);
                    }
                    Packet::Rejected { reason } => s.rejected = Some(reason),
                    update @ (Packet::Snapshot(_) | Packet::Delta(_)) => {
                        if matches!(update, Packet::Snapshot(_)) {
                            s.snapshots += 1;
                        } else {
                            s.deltas += 1;
                        }
                        let now = Instant::now();
                        if let Some(previous) = s.last_update.replace(now) {
                            let gap = (now - previous).as_secs_f64() * 1000.0;
                            s.gap_sum_ms += gap;
                            s.gaps += 1;
                            s.max_gap_ms = s.max_gap_ms.max(gap);
                            s.gaps_over_100ms += (gap > 100.0) as u64;
                        }
                        if receive_update(&mut s.baseline, update).is_err() {
                            if let Some(session) = s.token {
                                s.resyncs += 1;
                                let after_tick = s.baseline.as_ref().map_or(0, |b| b.tick);
                                let _ = socket.send_packet(
                                    &Packet::Resynchronize {
                                        session,
                                        after_tick,
                                    },
                                    addr,
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
            if s.welcomed {
                let input = InputFrame {
                    client_tick: tick,
                    movement: Movement {
                        forward: if i % 2 == 0 { 1.0 } else { -0.5 },
                        right: if i % 3 == 0 { 0.5 } else { 0.0 },
                        jump: (tick + i as u64 * 7).is_multiple_of(90),
                        ..Default::default()
                    },
                    yaw: tick as f32 * 0.01 + i as f32,
                    pitch: 0.0,
                    fire_wrench: false,
                    fire_pistol: false,
                    interact: false,
                    ack_server_tick: s.baseline.as_ref().map_or(0, |b| b.tick),
                    session_token: s.token,
                };
                let _ = socket.send_packet(&Packet::Input(input), addr);
            }
        }
        let spent = frame_start.elapsed();
        if spent < frame {
            thread::sleep(frame - spent);
        } else {
            late_frames += 1;
            worst_late_ms = worst_late_ms.max((spent - frame).as_secs_f64() * 1000.0);
        }
    }
    for (socket, s) in sockets.iter().zip(&stats) {
        if s.welcomed {
            let _ = socket.send_packet(
                &Packet::Disconnect {
                    player_id: s.id,
                    session_token: s.token,
                },
                addr,
            );
        }
    }

    let elapsed = started.elapsed().as_secs_f64();
    let joined = stats.iter().filter(|s| s.welcomed).count();
    let refused: Vec<&str> = stats.iter().filter_map(|s| s.rejected.as_deref()).collect();
    let sum = |f: fn(&Stats) -> u64| stats.iter().map(f).sum::<u64>();
    let gaps = sum(|s| s.gaps).max(1) as f64;
    let welcome_max = stats
        .iter()
        .filter_map(|s| s.welcome_ms)
        .fold(0.0, f64::max);
    println!(
        "{}",
        serde_json::json!({
            "clients_requested": clients_n,
            "clients_joined": joined,
            "clients_refused": refused.len(),
            "refusal_reason": refused.first(),
            "seconds": (elapsed * 100.).round() / 100.,
            "snapshots": sum(|s| s.snapshots),
            "deltas": sum(|s| s.deltas),
            "resyncs": sum(|s| s.resyncs),
            "bytes_per_client_per_s": (sum(|s| s.bytes) as f64 / joined.max(1) as f64 / elapsed).round(),
            "max_packet_bytes": stats.iter().map(|s| s.max_packet).max().unwrap_or(0),
            "welcome_ms_max": (welcome_max * 10.).round() / 10.,
            "update_gap_ms_mean": (stats.iter().map(|s| s.gap_sum_ms).sum::<f64>() / gaps * 10.).round() / 10.,
            "update_gap_ms_max": stats.iter().map(|s| s.max_gap_ms).fold(0.0, f64::max).round(),
            "update_gaps_over_100ms": sum(|s| s.gaps_over_100ms),
            "generator_late_frames": late_frames,
            "generator_worst_late_ms": (worst_late_ms * 10.).round() / 10.,
        })
    );
    Ok(())
}
