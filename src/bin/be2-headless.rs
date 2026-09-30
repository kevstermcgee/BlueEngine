//! Headless simulation and authoritative dedicated server for Blue Engine V2.
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;
use vesper3d::viewer::{
    controller::Movement,
    net::{DatagramTransport, Identity, SecureSocket, TransportProfile, UdpTransport},
    savestate::{world::RestoreOptions, SaveSlots},
    server::DedicatedServer,
    simulation::HeadlessWorld,
};

/// Server capacity options from the command line.
#[derive(Clone, Copy, Default)]
struct Tuning {
    max_players: Option<usize>,
    network_threads: Option<usize>,
}

fn run_server<T: DatagramTransport>(
    transport: T,
    world: HeadlessWorld,
    auth_key: Option<&str>,
    ticks: Option<u64>,
    autosave: Option<(SaveSlots, f32)>,
    tuning: Tuning,
) -> vesper3d::Result<()> {
    let stop_signal = Arc::new(AtomicBool::new(false));
    let mut server = DedicatedServer::with_transport(transport, world)?;
    if let Some(max) = tuning.max_players {
        server = server.with_max_players(max);
        println!("[Server] Admitting up to {} players", server.max_players());
    }
    if let Some(threads) = tuning.network_threads {
        server = server.with_network_threads(threads);
        println!(
            "[Server] Preparing peer updates on {} thread(s)",
            server.network_threads()
        );
    }
    if let Some(key) = auth_key {
        server = server.with_auth(key);
    }
    if let Some((slots, seconds)) = autosave {
        println!(
            "[Server] Autosaving to {} every {seconds} s",
            slots.dir().display()
        );
        server = server.with_autosave(slots, seconds, 3);
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
    let mut save_dir = None;
    let mut load = None;
    let mut autosave = None;
    let mut tuning = Tuning::default();
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
            "--save-dir" => {
                index += 1;
                save_dir = Some(args.get(index).ok_or("--save-dir needs a folder")?.clone());
            }
            "--load" => {
                index += 1;
                load = Some(
                    args.get(index)
                        .ok_or("--load needs a slot name or save file")?
                        .clone(),
                );
            }
            "--autosave" => {
                index += 1;
                let seconds: f32 = args
                    .get(index)
                    .ok_or("--autosave needs a number of seconds")?
                    .parse()?;
                if !(0.1..=86_400.).contains(&seconds) {
                    return Err("--autosave must be between 0.1 and 86400 seconds".into());
                }
                autosave = Some(seconds);
            }
            "--max-players" => {
                index += 1;
                let n: usize = args
                    .get(index)
                    .ok_or("--max-players needs a number")?
                    .parse()?;
                if !(1..=1024).contains(&n) {
                    return Err("--max-players must be between 1 and 1024".into());
                }
                tuning.max_players = Some(n);
            }
            "--network-threads" => {
                index += 1;
                let n: usize = args
                    .get(index)
                    .ok_or("--network-threads needs a number (0 = one per core)")?
                    .parse()?;
                tuning.network_threads = Some(n);
            }
            "--realtime" => realtime = true,
            "--help" => {
                println!(
                    "be2-headless [--server [ADDR]] [--listen ADDR] [--transport development|production] [--auth-key KEY] [--ticks N] [--realtime] [--map FILE | --game FILE] [--load SLOT_OR_FILE] [--save-dir DIR] [--autosave SECONDS] [--max-players N] [--network-threads N]\n\
                     Modes:\n\
                       --server [ADDR]   Run authoritative dedicated multiplayer server (default 0.0.0.0:4000)\n\
                       --transport development  Raw UDP for local development (default)\n\
                       --transport production   QUIC/TLS 1.3; BLUE_TLS_CERT_FILE pin + required BLUE_TLS_KEY_FILE\n\
                       --auth-key KEY    Additionally require client challenge-response authentication\n\
                       --load X          Resume the world from a save (a slot in --save-dir, or a file); players are new\n\
                       --save-dir DIR    Folder of save slots (default: `saves` next to the executable)\n\
                       --autosave S      Server: write a rotating autosave every S seconds and at shutdown\n\
                       --max-players N   Server: admit up to N players (default 8, at most 1024)\n\
                       --network-threads N  Server: prepare peer updates on N threads (0 = one per core; default 1)\n\
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
    let mut world = if let Some(ref path) = game_file {
        vesper3d::viewer::game::GameDocument::load(std::path::Path::new(path))?.world()?
    } else if let Some(ref path) = map_file {
        HeadlessWorld::try_with_room(
            vesper3d::viewer::authoring::MapDocument::load(std::path::Path::new(path))?
                .build_standalone()?,
        )?
    } else {
        HeadlessWorld::new()?
    };

    let slots = save_dir.map_or_else(SaveSlots::beside_exe, SaveSlots::new);
    if let Some(target) = &load {
        // Connected sessions are new, so the saved players are not restored (they would be ghosts).
        let loaded = slots
            .open(target)
            .map_err(|e| format!("--load {target}: {e}"))?;
        let (header, state) = world
            .parse_loaded(&loaded)
            .map_err(|e| format!("--load {target}: {e}"))?;
        world
            .restore_state_with(&state, RestoreOptions { players: false })
            .map_err(|e| format!("--load {target}: {e}"))?;
        println!(
            "[Server] Resumed \"{}\" at tick {}{}",
            header.label,
            world.tick,
            match &loaded.source {
                vesper3d::viewer::savestate::Source::Primary => String::new(),
                vesper3d::viewer::savestate::Source::Backup(why) =>
                    format!(" (from the previous good save: {why})"),
            }
        );
    }
    if autosave.is_some() && server_addr.is_none() {
        return Err("--autosave only applies with --server".into());
    }
    if (tuning.max_players.is_some() || tuning.network_threads.is_some()) && server_addr.is_none() {
        return Err("--max-players and --network-threads only apply with --server".into());
    }
    if let Some(addr) = server_addr {
        println!("[Server] Selected {transport_profile} transport");
        match transport_profile {
            TransportProfile::Development => {
                let transport = UdpTransport::bind(&addr)?;
                run_server(
                    transport,
                    world,
                    auth_key.as_deref(),
                    ticks,
                    autosave.map(|s| (slots, s)),
                    tuning,
                )?;
            }
            TransportProfile::Production => {
                let address = addr.parse()?;
                let transport = SecureSocket::server(address, Identity::load()?)?;
                run_server(
                    transport,
                    world,
                    auth_key.as_deref(),
                    ticks,
                    autosave.map(|s| (slots, s)),
                    tuning,
                )?;
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
