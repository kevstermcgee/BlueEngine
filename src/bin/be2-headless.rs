//! Headless simulation and authoritative dedicated server for Blue Engine V2.
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;
use vesper3d::viewer::{
    controller::Movement,
    net::{DatagramTransport, Identity, SecureSocket, TransportProfile, UdpTransport},
    server::DedicatedServer,
    simulation::HeadlessWorld,
};

fn run_server<T: DatagramTransport>(
    transport: T,
    world: HeadlessWorld,
    auth_key: Option<&str>,
    ticks: Option<u64>,
) -> vesper3d::Result<()> {
    let stop_signal = Arc::new(AtomicBool::new(false));
    let mut server = DedicatedServer::with_transport(transport, world)?;
    if let Some(key) = auth_key {
        server = server.with_auth(key);
    }
    server.run_realtime(stop_signal, ticks)
}

fn main() -> vesper3d::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mut ticks = None;
    let mut realtime = false;
    let mut map_file = None;
    let mut game_file = None;
    let mut server_addr = None;
    let mut auth_key = None;
    let mut transport_profile = TransportProfile::Development;
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
            "--auth-key" | "--auth" => {
                index += 1;
                auth_key = Some(
                    args.get(index)
                        .ok_or("--auth-key needs a secret key")?
                        .clone(),
                );
            }
            "--transport" => {
                index += 1;
                transport_profile = args
                    .get(index)
                    .ok_or("--transport needs development or production")?
                    .parse()?;
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
            "--game" => {
                index += 1;
                game_file = Some(args.get(index).ok_or("--game needs a file")?.clone());
            }
            "--realtime" => realtime = true,
            "--help" => {
                println!(
                    "be2-headless [--server [ADDR]] [--listen ADDR] [--transport development|production] [--auth-key KEY] [--ticks N] [--realtime] [--map FILE | --game FILE]\n\
                     Modes:\n\
                       --server [ADDR]   Run authoritative dedicated multiplayer server (default 0.0.0.0:4000)\n\
                       --transport development  Raw UDP for local development (default)\n\
                       --transport production   QUIC/TLS 1.3; BLUE_TLS_CERT_FILE pin + required BLUE_TLS_KEY_FILE\n\
                       --auth-key KEY    Additionally require client challenge-response authentication\n\
                       (no --server)     Run local benchmark simulation"
                );
                return Ok(());
            }
            other => return Err(format!("Unknown argument: {other}").into()),
        }
        index += 1;
    }

    if map_file.is_some() && game_file.is_some() {
        return Err("Use --map or --game, not both".into());
    }
    let world = if let Some(ref path) = game_file {
        vesper3d::viewer::game::GameDocument::load(std::path::Path::new(path))?.world()?
    } else if let Some(ref path) = map_file {
        HeadlessWorld::try_with_room(
            vesper3d::viewer::authoring::MapDocument::load(std::path::Path::new(path))?.build()?,
        )?
    } else {
        HeadlessWorld::new()?
    };

    if let Some(addr) = server_addr {
        println!("[Server] Selected {transport_profile} transport");
        match transport_profile {
            TransportProfile::Development => {
                let transport = UdpTransport::bind(&addr)?;
                run_server(transport, world, auth_key.as_deref(), ticks)?;
            }
            TransportProfile::Production => {
                let address = addr.parse()?;
                let transport = SecureSocket::server(address, Identity::load()?)?;
                run_server(transport, world, auth_key.as_deref(), ticks)?;
            }
        }
        return Ok(());
    }

    if transport_profile != TransportProfile::Development {
        return Err("--transport only applies with --server".into());
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
    let mut runner = vesper3d::viewer::metrics::FixedTickRunner::new(60);
    for i in 0..ticks {
        let tick_start = Instant::now();
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
            let elapsed = tick_start.elapsed().as_micros();
            runner.sleep_until_next_tick(elapsed);
        }
    }
    println!(
        "BE2 headless: players=2 ticks={} elapsed_ms={:.3} mean_tick_us={:.3} realtime={}",
        world.tick,
        started.elapsed().as_secs_f64() * 1000.,
        started.elapsed().as_secs_f64() * 1_000_000. / ticks as f64,
        realtime
    );
    if let Some(game) = &world.game {
        println!(
            "{}",
            serde_json::json!({"game_state":game.state(), "checksum": world.checksum()})
        );
    }
    Ok(())
}
