//! `be2-hub`: one hub for every BlueEngine online game (ADR 0037). Std only; no window, no graphics.
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};
use vesper3d::viewer::netplay::hub::deploy::{self, GameStatus, VerifyOptions};
use vesper3d::viewer::netplay::hub::legacy::Mode;
use vesper3d::viewer::netplay::hub::wire::{Control, ControlReply};
use vesper3d::viewer::netplay::hub::{
    self, log, parse_config, HubOptions, HubSection, Limits, ManagerConfig, ProcessInfo,
    ProcessSpawner, Registry,
};

const USAGE: &str = "be2-hub: one hub that lists and creates rooms for every BlueEngine online game

USAGE:
  be2-hub --config PATH [OPTIONS]        run the hub
  be2-hub reload GAME [--config PATH]    re-read GAME's registry entry and server, retire its rooms (hub must be running).
                                         \"reloaded\" means the hub accepted the request and started a replacement room: it is not
                                         proof the room works (use status)
  be2-hub status GAME [--config PATH] [--expect-build HEX8] [--wait SECS]
                                         ask the running hub what it holds for GAME: the build its registry loaded and whether the
                                         Public room's process is up and printing STATUS lines (with which build). Prints state=
                                         ready | registry-only | starting | missing | wrong-build | no-answer; exits 0 only for
                                         ready or registry-only (no Public room to observe). --wait polls until then or the deadline.
  be2-hub verify GAME --server PATH [--config PATH] [--installs-to PATH] [--start] [--info-timeout S] [--startup-timeout S]
                                         check a candidate server for GAME without touching the hub: its --info (bounded time and
                                         output) must pass every rule the hub applies when it loads a game against GAME's
                                         [game] section; --installs-to PATH requires the config to run that very file; --start
                                         also runs it on loopback with a scratch report dir until its first STATUS line, then stops it
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
    // verify / status
    server: Option<PathBuf>,
    installs_to: Option<PathBuf>,
    start: bool,
    info_timeout: Option<u64>,
    startup_timeout: Option<u64>,
    expect_build: Option<u32>,
    wait: Option<u64>,
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
            "--server" => f.server = Some(value(&mut i, a)?.into()),
            "--installs-to" => f.installs_to = Some(value(&mut i, a)?.into()),
            "--start" => f.start = true,
            "--info-timeout" => f.info_timeout = Some(seconds(&value(&mut i, a)?, a)?),
            "--startup-timeout" => f.startup_timeout = Some(seconds(&value(&mut i, a)?, a)?),
            "--wait" => f.wait = Some(seconds(&value(&mut i, a)?, a)?),
            "--expect-build" => {
                let v = value(&mut i, a)?;
                f.expect_build = Some(
                    Some(v.as_str())
                        .filter(|v| v.len() == 8)
                        .and_then(|v| u32::from_str_radix(v, 16).ok())
                        .ok_or_else(|| format!("--expect-build needs 8 hex digits (the build= line of --info), not {v}"))?,
                );
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown argument {other} (try --help)"))
            }
            other => f.positional.push(other.to_string()),
        }
        i += 1;
    }
    Ok(Some(f))
}

fn seconds(v: &str, flag: &str) -> Result<u64, String> {
    v.parse::<u64>()
        .ok()
        .filter(|s| (1..=600).contains(s))
        .ok_or_else(|| format!("{flag} needs a whole number of seconds, 1 to 600, not {v}"))
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
        Some(c @ ("reload" | "ports" | "status" | "verify")) => (Some(c.to_string()), &args[1..]),
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
        Some("status") => {
            let [game] = &flags.positional[..] else {
                return Err("usage: be2-hub status GAME [--config PATH] [--expect-build HEX8] [--wait SECS]".into());
            };
            status(game, &flags)
        }
        Some("verify") => {
            let [game] = &flags.positional[..] else {
                return Err(
                    "usage: be2-hub verify GAME --server PATH [--config PATH] [--start]".into(),
                );
            };
            verify(game, &flags)
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

/// Send one control datagram to the hub on this machine and wait for its answer (resending now and then).
/// `Err` is "no answer" only; the hub's own refusals come back as `Ok(reply)` with `ok == false`.
fn ask_hub(port: u16, control: &Control, within: Duration) -> Result<ControlReply, String> {
    let socket = UdpSocket::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_millis(200)))
        .map_err(|e| e.to_string())?;
    let nonce = std::process::id() ^ 0x5eed;
    let request = control.encode(nonce);
    let started = Instant::now();
    let mut buf = [0u8; 512];
    while started.elapsed() < within {
        let _ = socket.send_to(&request, ("127.0.0.1", port));
        for _ in 0..25 {
            if let Ok((n, _)) = socket.recv_from(&mut buf) {
                if let Some((got, reply)) = ControlReply::decode(&buf[..n]) {
                    if got == nonce {
                        return Ok(reply);
                    }
                }
            }
            if started.elapsed() >= within {
                break;
            }
        }
    }
    Err(format!(
        "no answer from a hub on 127.0.0.1:{port} (is it running? is --config the one it uses? a hub older than `status` never answers it)"
    ))
}

/// Tell the running hub on this machine to reload one game.
fn reload(game: &str, flags: &Flags) -> Result<(), String> {
    let port = settings(flags)?.listen.port();
    // The hub runs the game's --info before it answers (a few seconds at most): wait for it.
    let reply = ask_hub(
        port,
        &Control::Reload {
            game: game.to_string(),
        },
        Duration::from_secs(20),
    )?;
    if reply.ok {
        println!("{}", reply.text);
        Ok(())
    } else {
        Err(reply.text)
    }
}

/// What the running hub holds for a game, and whether that is the build the updater expects.
fn status(game: &str, flags: &Flags) -> Result<(), String> {
    let port = settings(flags)?.listen.port();
    let deadline = Instant::now() + Duration::from_secs(flags.wait.unwrap_or(0));
    loop {
        let reply = match ask_hub(
            port,
            &Control::Status {
                game: game.to_string(),
            },
            Duration::from_secs(3),
        ) {
            Ok(r) => r,
            Err(e) => {
                println!("state=no-answer");
                return Err(e);
            }
        };
        if !reply.ok {
            println!("state=unknown-game");
            return Err(reply.text);
        }
        let status = GameStatus::parse(&reply.text)
            .ok_or_else(|| format!("the hub's status reply is not understood: {}", reply.text))?;
        let readiness = status.readiness(flags.expect_build);
        if readiness.settled() || Instant::now() >= deadline {
            println!("state={}", readiness.as_str());
            println!("detail={}", reply.text);
            return if readiness.settled() {
                Ok(())
            } else {
                Err(format!(
                    "{game} is not ready: {} ({})",
                    readiness.as_str(),
                    reply.text
                ))
            };
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Check a candidate server file against the registry rules (and optionally start it), changing nothing.
fn verify(game: &str, flags: &Flags) -> Result<(), String> {
    let server = flags
        .server
        .as_deref()
        .ok_or("verify needs --server PATH (the candidate executable)")?;
    let config_path = flags
        .config
        .as_deref()
        .ok_or("verify needs --config PATH (the hub's registry file)")?;
    let config = read_config(config_path)?;
    let mut info = ProcessInfo::with_timeout(Duration::from_secs(flags.info_timeout.unwrap_or(5)));
    let opts = VerifyOptions {
        startup: flags.start.then(|| {
            flags
                .startup_timeout
                .map_or(deploy::STARTUP_TIMEOUT, Duration::from_secs)
        }),
        installs_to: flags.installs_to.clone(),
    };
    let report =
        deploy::verify_candidate(&config, game, server, &mut info, &mut ProcessSpawner, &opts)?;
    print!("{}", report.to_text());
    Ok(())
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
