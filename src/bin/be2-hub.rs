//! `be2-hub`: one hub for every BlueEngine online game (ADR 0037). Std only; no window, no graphics.
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};
use vesper3d::viewer::netplay::hub::legacy::Mode;
use vesper3d::viewer::netplay::hub::wire::{Control, ControlReply};
use vesper3d::viewer::netplay::hub::{
    self, log, parse_config, HubOptions, HubSection, Limits, ManagerConfig, ProcessInfo,
    ProcessSpawner, Registry,
};

const USAGE: &str = "be2-hub: one hub that lists and creates rooms for every BlueEngine online game

USAGE:
  be2-hub --config PATH [OPTIONS]        run the hub
  be2-hub reload GAME [--config PATH]    re-read GAME's registry entry and server, retire its rooms (hub must be running)
  be2-hub ports [--config PATH]          print the UDP ports to open on the router, one per line (hub port, then the room pool)

OPTIONS (each overrides the [hub] section of the config file):
  --config PATH        the registry file (see src/viewer/netplay/hub/registry.rs, deploy/hub/hub.conf.example)
  --listen ADDR        the hub's address (default 0.0.0.0:4100)
  --pool-start PORT    first room port (default: the hub's port + 1)
  --pool-size N        how many room ports (default 16, at most 64)
  --report-dir DIR     match reports go to DIR/<game>/port-<n>/matches.jsonl
  --legacy MODE        serve (answer old Deadfall clients, default) or refuse (tell them to update)
  --help               this text

Rooms are processes of each game's server (built with netplay::cli::serve), started on the pool ports and closed when empty.
SIGINT or SIGTERM stops the hub and every room it started; `reload` retires one game's rooms without touching the others.
";

fn main() {
    if let Err(e) = run() {
        eprintln!("be2-hub: {e}");
        std::process::exit(1);
    }
}

#[derive(Default)]
struct Flags {
    config: Option<PathBuf>,
    over: HubSection,
    positional: Vec<String>,
}

fn parse_flags(args: &[String]) -> Result<Option<Flags>, String> {
    let mut f = Flags::default();
    let mut i = 0;
    let value = |i: &mut usize, flag: &str| -> Result<String, String> {
        *i += 1;
        args.get(*i)
            .cloned()
            .ok_or_else(|| format!("{flag} needs a value"))
    };
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "--help" | "-h" => {
                print!("{USAGE}");
                return Ok(None);
            }
            "--config" => f.config = Some(value(&mut i, a)?.into()),
            "--listen" => {
                let v = value(&mut i, a)?;
                f.over.listen = Some(
                    v.parse::<SocketAddr>()
                        .ok()
                        .filter(SocketAddr::is_ipv4)
                        .ok_or_else(|| {
                            format!("--listen must be an IPv4 address and port like 0.0.0.0:4100, not {v}")
                        })?,
                );
            }
            "--pool-start" => {
                f.over.pool_start = Some(
                    value(&mut i, a)?
                        .parse()
                        .map_err(|_| "--pool-start must be a port number")?,
                )
            }
            "--pool-size" => {
                f.over.pool_size = Some(
                    value(&mut i, a)?
                        .parse()
                        .map_err(|_| "--pool-size must be a number")?,
                )
            }
            "--report-dir" => f.over.report_dir = Some(value(&mut i, a)?.into()),
            "--legacy" => f.over.legacy = Some(value(&mut i, a)?.parse::<Mode>()?),
            other if other.starts_with("--") => {
                return Err(format!("unknown argument {other} (try --help)"))
            }
            other => f.positional.push(other.to_string()),
        }
        i += 1;
    }
    Ok(Some(f))
}

fn read_config(path: &Path) -> Result<vesper3d::viewer::netplay::hub::Config, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read the config {}: {e}", path.display()))?;
    let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
    parse_config(&text, &base).map_err(|e| format!("{}: {e}", path.display()))
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (command, rest) = match args.first().map(String::as_str) {
        Some(c @ ("reload" | "ports")) => (Some(c.to_string()), &args[1..]),
        _ => (None, &args[..]),
    };
    let Some(flags) = parse_flags(rest)? else {
        return Ok(());
    };
    match command.as_deref() {
        Some("reload") => {
            let [game] = &flags.positional[..] else {
                return Err("usage: be2-hub reload GAME [--config PATH]".into());
            };
            reload(game, &flags)
        }
        Some("ports") => {
            let settings = settings(&flags)?;
            for p in settings.ports() {
                println!("{p}");
            }
            Ok(())
        }
        _ => {
            if let Some(extra) = flags.positional.first() {
                return Err(format!("unexpected argument {extra} (try --help)"));
            }
            serve(&flags)
        }
    }
}

fn settings(flags: &Flags) -> Result<hub::HubSettings, String> {
    let file = match &flags.config {
        Some(path) => read_config(path)?.hub,
        None => HubSection::default(),
    };
    file.resolve(&flags.over)
}

/// Tell the running hub on this machine to reload one game.
fn reload(game: &str, flags: &Flags) -> Result<(), String> {
    let port = settings(flags)?.listen.port();
    let socket = UdpSocket::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_millis(200)))
        .map_err(|e| e.to_string())?;
    let nonce = std::process::id() ^ 0x5eed;
    let request = Control::Reload {
        game: game.to_string(),
    }
    .encode(nonce);
    let started = Instant::now();
    let mut buf = [0u8; 512];
    // The hub runs the game's --info before it answers (a few seconds at most): wait for it, resending now and then.
    while started.elapsed() < Duration::from_secs(20) {
        let _ = socket.send_to(&request, ("127.0.0.1", port));
        for _ in 0..25 {
            if let Ok((n, _)) = socket.recv_from(&mut buf) {
                if let Some((got, ControlReply { ok, text })) = ControlReply::decode(&buf[..n]) {
                    if got == nonce {
                        return if ok {
                            println!("{text}");
                            Ok(())
                        } else {
                            Err(text)
                        };
                    }
                }
            }
            if started.elapsed() >= Duration::from_secs(20) {
                break;
            }
        }
    }
    Err(format!(
        "no answer from a hub on 127.0.0.1:{port} (is it running? is --config the one it uses?)"
    ))
}

fn serve(flags: &Flags) -> Result<(), String> {
    let config_path = flags
        .config
        .clone()
        .ok_or("--config PATH is required (see --help)")?;
    let config = read_config(&config_path)?;
    let settings = config.hub.resolve(&flags.over)?;
    let socket = UdpSocket::bind(settings.listen).map_err(|e| {
        format!(
            "cannot listen on {}: {e} (is another hub, or a game server such as Deadfall or Spooky Kart, using that UDP port?)",
            settings.listen
        )
    })?;
    let mut info = ProcessInfo::new();
    let (registry, problems) = Registry::load(&config, &mut info);
    for p in &problems {
        log(p);
    }
    if registry.games().is_empty() {
        log("No game could be loaded: the hub will answer every request with \"unknown game\" until one is reloaded");
    }
    for g in registry.games() {
        log(&format!(
            "Game {}: {} (build {:08x}, up to {} players, {} setting(s), {} player room(s){})",
            g.id(),
            g.config.server.display(),
            g.info.build,
            g.info.max_seats,
            g.info.settings.len(),
            g.config.max_rooms,
            if g.config.public { ", Public room" } else { "" }
        ));
    }
    let opts = HubOptions {
        manager: ManagerConfig {
            pool_start: settings.pool_start,
            pool_size: settings.pool_size,
            bind_ip: settings.bind_ip.to_string(),
            report_dir: Some(settings.report_dir.clone()),
            max_processes: settings.max_processes,
            max_rooms_per_ip: settings.max_rooms_per_ip,
            ..Default::default()
        },
        limits: Limits {
            max_rooms_per_ip: settings.max_rooms_per_ip,
            max_processes: settings.max_processes,
            burst: settings.rate_burst,
            per_sec: settings.rate_per_sec,
            ..Default::default()
        },
        legacy: settings.legacy,
        config_path: Some(config_path),
    };
    let stop = Arc::new(AtomicBool::new(false));
    vesper3d::viewer::shutdown::install(stop.clone()).map_err(|e| e.to_string())?;
    log(&format!(
        "be2-hub on {}: rooms on UDP {}-{}, reports in {}, legacy Deadfall protocol: {}",
        settings.listen,
        settings.pool_start,
        settings.pool_start + settings.pool_size - 1,
        settings.report_dir.display(),
        match settings.legacy {
            Mode::Serve => "served",
            Mode::Refuse => "refused (old clients are told to update)",
        }
    ));
    let mut hub = hub::Hub::new(opts, registry, Box::new(ProcessSpawner), Box::new(info));
    hub::serve(&socket, &mut hub, &stop).map_err(|e| format!("socket error: {e}"))?;
    vesper3d::viewer::shutdown::finished();
    log("Stopped");
    Ok(())
}
