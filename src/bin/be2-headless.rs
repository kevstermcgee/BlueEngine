//! Headless simulation and authoritative dedicated server for Blue Engine V2.
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};
use vesper3d::viewer::{controller::Movement, server::DedicatedServer, simulation::HeadlessWorld};

fn main() -> vesper3d::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mut ticks = None;
    let mut realtime = false;
    let mut map_file = None;
    let mut server_addr = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--server" => {
                let next = args.get(index + 1);
                if let Some(val) = next {
                    if !val.starts_with("--") {
                        server_addr = Some(val.clone());
                        index += 1;
                    } else {
                        server_addr = Some("0.0.0.0:4000".to_string());
                    }
                } else {
                    server_addr = Some("0.0.0.0:4000".to_string());
                }
            }
            "--listen" => {
                index += 1;
                server_addr = Some(args.get(index).ok_or("--listen needs an address")?.clone());
            }
            "--ticks" => {
                index += 1;
                let t: u64 = args.get(index).ok_or("--ticks needs a number")?.parse()?;
                ticks = Some(t);
            }
            "--map" => {
                index += 1;
                map_file = Some(args.get(index).ok_or("--map needs a file")?.clone());
            }
            "--realtime" => realtime = true,
            "--help" => {
                println!(
                    "be2-headless [--server [ADDR]] [--listen ADDR] [--ticks N] [--realtime] [--map FILE]\n\
                     Modes:\n\
                       --server [ADDR]   Run authoritative dedicated multiplayer server (default 0.0.0.0:4000)\n\
                       (no --server)     Run local benchmark simulation"
                );
                return Ok(());
            }
            other => return Err(format!("Unknown argument: {other}").into()),
        }
        index += 1;
    }

    let world = if let Some(ref path) = map_file {
        HeadlessWorld::with_room(
            vesper3d::viewer::authoring::MapDocument::load(std::path::Path::new(path))?.build()?,
        )
    } else {
        HeadlessWorld::new()?
    };

    if let Some(addr) = server_addr {
        let stop_signal = Arc::new(AtomicBool::new(false));
        let mut server = DedicatedServer::with_world(&addr, world)?;
        server.run_realtime(stop_signal, ticks)?;
        return Ok(());
    }

    // Benchmark mode:
    let ticks = ticks.unwrap_or(600);
    if ticks == 0 || ticks > 10_000_000 {
        return Err("ticks must be 1..10000000".into());
    }
    let mut world = world;
    world.join(1);
    world.join(2);
    let started = Instant::now();
    for i in 0..ticks {
        for id in [1, 2] {
            world.input(
                id,
                Movement {
                    forward: 1.,
                    jump: i % 120 == 0,
                    crouch: i % 240 > 180,
                    ..Default::default()
                },
                i as f32 * 0.013 + id as f32,
                0.,
            );
        }
        world.step();
        if realtime {
            let deadline = started + Duration::from_secs_f64((i + 1) as f64 / 60.);
            std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        }
    }
    println!(
        "BE2 headless: players=2 ticks={} elapsed_ms={:.3} mean_tick_us={:.3} realtime={}",
        world.tick,
        started.elapsed().as_secs_f64() * 1000.,
        started.elapsed().as_secs_f64() * 1_000_000. / ticks as f64,
        realtime
    );
    Ok(())
}
