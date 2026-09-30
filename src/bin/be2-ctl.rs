//! `be2-ctl`: list, inspect, start, stop and restart BlueEngine game servers on this machine (Unix).
#[cfg(not(unix))]
fn main() {
    eprintln!("be2-ctl manages servers on Unix systems only");
    std::process::exit(2);
}

#[cfg(unix)]
fn main() {
    std::process::exit(ctl::run(std::env::args().skip(1).collect()));
}

#[cfg(unix)]
mod ctl {
    use std::fmt::Write as _;
    use std::io::{Read, Seek, SeekFrom};
    use std::os::unix::process::CommandExt;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    use vesper3d::viewer::control::{
        self, proc, Definition, Reply, Request, ServerRecord, Status, LAUNCHED_BY_CTL, LAUNCHER_ENV,
    };

    const USAGE: &str = "be2-ctl: manage BlueEngine game servers on this machine

USAGE:
  be2-ctl list [--json]
  be2-ctl status NAME [--json]
  be2-ctl start NAME [--save] [-- SERVER_ARGS...]   (no SERVER_ARGS: use the saved definition)
  be2-ctl stop NAME [--timeout SECONDS] [--force]
  be2-ctl restart NAME [--timeout SECONDS] [--force]
  be2-ctl logs NAME [-n LINES]
  be2-ctl define NAME -- SERVER_ARGS...
  be2-ctl undefine NAME
  be2-ctl prune

NAME is a server's registered name, or pid:N for a be2-headless server that never registered.
SERVER_ARGS are be2-headless arguments (--server ADDR --game FILE ...); --name is added for you.
stop asks the server to shut down, then sends SIGTERM; --force escalates to a second SIGTERM and then SIGKILL.
State lives in $BLUEENGINE_STATE_DIR, else $XDG_STATE_HOME/blueengine, else ~/.local/state/blueengine.
";

    const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
    const START_TIMEOUT: Duration = Duration::from_secs(10);

    type Res<T> = Result<T, String>;

    pub fn run(args: Vec<String>) -> i32 {
        match dispatch(&args) {
            Ok(code) => code,
            Err(message) => {
                eprintln!("be2-ctl: {message}");
                1
            }
        }
    }

    /// Options after the subcommand and name; everything after `--` is kept apart for the server.
    #[derive(Default)]
    struct Opts {
        json: bool,
        save: bool,
        force: bool,
        timeout: Option<Duration>,
        lines: Option<usize>,
        positional: Vec<String>,
        server_args: Vec<String>,
    }

    fn parse(args: &[String]) -> Res<Opts> {
        let mut opts = Opts::default();
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--" => {
                    opts.server_args = args[index + 1..].to_vec();
                    break;
                }
                "--json" => opts.json = true,
                "--save" => opts.save = true,
                "--force" => opts.force = true,
                "--timeout" | "-n" => {
                    let flag = args[index].clone();
                    index += 1;
                    let value = args.get(index).ok_or(format!("{flag} needs a number"))?;
                    if flag == "-n" {
                        opts.lines = Some(value.parse().map_err(|_| "-n needs a whole number")?);
                    } else {
                        let seconds: f64 = value
                            .parse()
                            .ok()
                            .filter(|s: &f64| s.is_finite() && *s >= 0.0 && *s <= 3600.0)
                            .ok_or("--timeout needs seconds between 0 and 3600")?;
                        opts.timeout = Some(Duration::from_secs_f64(seconds));
                    }
                }
                flag if flag.starts_with('-') => return Err(format!("unknown option {flag}")),
                _ => opts.positional.push(args[index].clone()),
            }
            index += 1;
        }
        Ok(opts)
    }

    fn dispatch(args: &[String]) -> Res<i32> {
        let Some((command, rest)) = args.split_first() else {
            print!("{USAGE}");
            return Ok(2);
        };
        if matches!(command.as_str(), "help" | "--help" | "-h") {
            print!("{USAGE}");
            return Ok(0);
        }
        let opts = parse(rest)?;
        let name = || -> Res<&str> {
            match opts.positional.as_slice() {
                [name] => Ok(name),
                [] => Err(format!("{command} needs a server NAME")),
                _ => Err(format!("{command} takes one NAME")),
            }
        };
        let timeout = opts.timeout.unwrap_or(DEFAULT_TIMEOUT);
        match command.as_str() {
            "list" | "ls" => list(opts.json),
            "status" => status(name()?, opts.json),
            "start" => start(name()?, &opts),
            "stop" => stop(name()?, timeout, opts.force),
            "restart" => restart(name()?, timeout, opts.force),
            "logs" => logs(name()?, opts.lines.unwrap_or(40)),
            "define" => define(name()?, &opts.server_args),
            "undefine" => undefine(name()?),
            "prune" => prune(),
            other => Err(format!("unknown command {other} (try `be2-ctl help`)")),
        }
    }

    // ---- what is running -------------------------------------------------------------------------

    /// One server as `be2-ctl` sees it.
    struct Entry {
        name: String,
        pid: u32,
        start_ticks: Option<u64>,
        /// `running`, `stale` (record of a process that is gone), `unregistered`, or `unreadable`.
        state: &'static str,
        record: Option<ServerRecord>,
        args: Vec<String>,
        problem: Option<String>,
    }

    impl Entry {
        fn running(&self) -> bool {
            matches!(self.state, "running" | "unregistered")
        }
        /// The process is still the one we mean (not gone, not a recycled pid).
        fn alive(&self) -> bool {
            proc::is_alive(self.pid)
                && match (self.start_ticks, proc::start_ticks(self.pid)) {
                    (Some(then), Some(now)) => then == now,
                    _ => true,
                }
        }
    }

    fn entries() -> Vec<Entry> {
        let mut out = Vec::new();
        let mut known = std::collections::HashSet::new();
        for (name, record) in control::read_records() {
            match record {
                Ok(record) => {
                    let alive = record.process_alive();
                    if alive {
                        known.insert(record.pid);
                    }
                    out.push(Entry {
                        name,
                        pid: record.pid,
                        start_ticks: record.start_ticks,
                        state: if alive { "running" } else { "stale" },
                        args: record.args.clone(),
                        record: Some(record),
                        problem: None,
                    });
                }
                Err(e) => out.push(Entry {
                    name,
                    pid: 0,
                    start_ticks: None,
                    state: "unreadable",
                    record: None,
                    args: Vec::new(),
                    problem: Some(e.to_string()),
                }),
            }
        }
        // Servers started by hand or by another supervisor without `--name`.
        for process in proc::find_by_program(control::SERVER_PROGRAM) {
            if known.contains(&process.pid) || !process.args.iter().any(|a| a == "--server") {
                continue;
            }
            out.push(Entry {
                name: format!("pid:{}", process.pid),
                pid: process.pid,
                start_ticks: process.start_ticks,
                state: "unregistered",
                record: None,
                args: process.args.iter().skip(1).cloned().collect(),
                problem: None,
            });
        }
        out
    }

    fn find(name: &str) -> Res<Entry> {
        entries()
            .into_iter()
            .find(|e| e.name == name)
            .ok_or_else(|| format!("no server named {name} (see `be2-ctl list`)"))
    }

    fn query(name: &str) -> Result<Status, String> {
        match control::ask(name, &Request::Status) {
            Ok(Reply::Status { status, .. }) => Ok(status),
            Ok(Reply::Error { error, .. }) => Err(error),
            Ok(Reply::Done { message, .. }) => Err(message),
            Err(e) => Err(e.to_string()),
        }
    }

    fn duration(seconds: f64) -> String {
        let s = seconds as u64;
        match s {
            0..=59 => format!("{s}s"),
            60..=3599 => format!("{}m{:02}s", s / 60, s % 60),
            _ => format!("{}h{:02}m", s / 3600, s % 3600 / 60),
        }
    }

    fn list(json: bool) -> Res<i32> {
        let all = entries();
        let rows: Vec<(Entry, Option<Status>)> = all
            .into_iter()
            .map(|e| {
                let status = (e.state == "running")
                    .then(|| query(&e.name).ok())
                    .flatten();
                (e, status)
            })
            .collect();
        if json {
            let value: Vec<_> = rows
                .iter()
                .map(|(e, s)| {
                    serde_json::json!({
                        "name": e.name, "state": e.state, "pid": e.pid,
                        "launcher": e.record.as_ref().map(|r| r.launcher.clone()),
                        "args": e.args, "problem": e.problem, "status": s,
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?
            );
            return Ok(0);
        }
        if rows.is_empty() {
            println!("no servers");
            return Ok(0);
        }
        let mut table = format!(
            "{:<16} {:<12} {:>7} {:<22} {:>7} {:>8} {:>8}  {}\n",
            "NAME", "STATE", "PID", "ADDRESS", "CLIENTS", "TICK ms", "UPTIME", "LAUNCHER"
        );
        for (e, status) in &rows {
            let address = status
                .as_ref()
                .map(|s| s.addr.clone())
                .or_else(|| e.record.as_ref().map(|r| r.addr.clone()))
                .unwrap_or_else(|| "-".into());
            let (clients, tick, uptime) = match status {
                Some(s) => (
                    format!("{}/{}", s.clients, s.max_players),
                    format!("{:.2}", s.tick_mean_us as f64 / 1000.0),
                    duration(s.uptime_s),
                ),
                None => ("-".into(), "-".into(), "-".into()),
            };
            let launcher = e
                .record
                .as_ref()
                .map_or("external", |r| r.launcher.as_str());
            let state = if e.state == "running" && status.is_none() {
                "no-response"
            } else {
                e.state
            };
            let _ = writeln!(
                table,
                "{:<16} {:<12} {:>7} {:<22} {:>7} {:>8} {:>8}  {}",
                e.name,
                state,
                if e.pid == 0 {
                    "-".into()
                } else {
                    e.pid.to_string()
                },
                address,
                clients,
                tick,
                uptime,
                launcher
            );
        }
        print!("{table}");
        if rows.iter().any(|(e, _)| e.state == "stale") {
            println!(
                "stale records belong to processes that are gone: `be2-ctl prune` removes them"
            );
        }
        Ok(0)
    }

    fn status(name: &str, json: bool) -> Res<i32> {
        let entry = find(name)?;
        if entry.state == "stale" {
            return Err(format!(
                "{name} is not running (stale record; `be2-ctl prune` removes it)"
            ));
        }
        if entry.state == "unreadable" {
            return Err(format!(
                "{name}: unreadable record ({})",
                entry.problem.unwrap_or_default()
            ));
        }
        if entry.state == "unregistered" {
            return Err(format!(
                "{name} never registered, so it cannot report status; pid {} runs: {}",
                entry.pid,
                entry.args.join(" ")
            ));
        }
        let status =
            query(name).map_err(|e| format!("{name} (pid {}) did not answer: {e}", entry.pid))?;
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&status).map_err(|e| e.to_string())?
            );
            return Ok(0);
        }
        println!(
            "{} (pid {}){}",
            status.name,
            status.pid,
            if status.stopping { "  STOPPING" } else { "" }
        );
        println!("  address      {} ({})", status.addr, status.transport);
        println!("  map          {}", status.map);
        println!("  clients      {}/{}", status.clients, status.max_players);
        println!(
            "  uptime       {}   tick {}",
            duration(status.uptime_s),
            status.tick
        );
        println!(
            "  tick time    mean {:.2} ms, worst {:.2} ms",
            status.tick_mean_us as f64 / 1000.0,
            status.tick_max_us as f64 / 1000.0
        );
        println!("  threads      {} network", status.network_threads);
        println!(
            "  autosave     {} written, {} failed",
            status.autosave_written, status.autosave_failed
        );
        println!(
            "  replication  mean wait {:.2} broadcasts",
            status.replication_mean_wait
        );
        println!(
            "  version      {} (protocol {})",
            status.version, status.protocol
        );
        Ok(0)
    }

    // ---- stopping --------------------------------------------------------------------------------

    fn wait_gone(entry: &Entry, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        loop {
            if !entry.alive() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn signal(entry: &Entry, sig: i32) -> Res<()> {
        // A recycled pid is somebody else's process: never signal it.
        if !entry.alive() {
            return Ok(());
        }
        proc::send_signal(entry.pid, sig)
            .map(|_| ())
            .map_err(|e| format!("cannot signal pid {}: {e}", entry.pid))
    }

    /// Stop a running server as gently as it allows; returns how it ended.
    fn stop_entry(entry: &Entry, timeout: Duration, force: bool) -> Res<&'static str> {
        if !entry.alive() {
            return Ok("already stopped");
        }
        if entry.state == "running" {
            // Ask first: the server saves and says goodbye to its clients.
            if control::ask(&entry.name, &Request::Shutdown).is_ok() && wait_gone(entry, timeout) {
                return Ok("stopped (asked to shut down)");
            }
        }
        signal(entry, proc::SIGTERM)?;
        if wait_gone(entry, timeout) {
            return Ok("stopped (SIGTERM)");
        }
        if !force {
            return Err(format!(
                "pid {} did not stop within {:.0}s; rerun with --force to escalate",
                entry.pid,
                timeout.as_secs_f64()
            ));
        }
        // A second SIGTERM makes the server exit at once without finishing its save.
        signal(entry, proc::SIGTERM)?;
        if wait_gone(
            entry,
            Duration::from_secs(2).min(timeout.max(Duration::from_secs(1))),
        ) {
            return Ok("stopped (forced exit)");
        }
        signal(entry, proc::SIGKILL)?;
        if wait_gone(entry, Duration::from_secs(5)) {
            Ok("killed (SIGKILL)")
        } else {
            Err(format!(
                "pid {} survived SIGKILL (uninterruptible?)",
                entry.pid
            ))
        }
    }

    fn stop(name: &str, timeout: Duration, force: bool) -> Res<i32> {
        let entry = find(name)?;
        if entry.state == "unreadable" {
            return Err(format!(
                "{name}: unreadable record ({})",
                entry.problem.unwrap_or_default()
            ));
        }
        if entry.state == "stale" {
            control::remove_files(name);
            println!("{name} was not running; removed its stale record");
            return Ok(0);
        }
        let how = stop_entry(&entry, timeout, force)?;
        if !entry.name.starts_with("pid:") {
            control::remove_files(&entry.name);
        }
        println!("{name}: {how}");
        Ok(0)
    }

    // ---- starting --------------------------------------------------------------------------------

    fn server_binary() -> PathBuf {
        if let Some(path) = std::env::var_os("BE2_HEADLESS") {
            return path.into();
        }
        if let Some(sibling) = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join(control::SERVER_PROGRAM)))
            .filter(|path| path.exists())
        {
            return sibling;
        }
        control::SERVER_PROGRAM.into() // found on PATH
    }

    /// `args` with any `--name` replaced by `--name NAME`.
    fn with_name(args: &[String], name: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut skip = false;
        for arg in args {
            if skip {
                skip = false;
            } else if arg == "--name" {
                skip = true;
            } else {
                out.push(arg.clone());
            }
        }
        out.push("--name".into());
        out.push(name.into());
        out
    }

    fn tail(path: &std::path::Path, lines: usize) -> String {
        let Ok(mut file) = std::fs::File::open(path) else {
            return String::new();
        };
        let len = file.metadata().map_or(0, |m| m.len());
        let from = len.saturating_sub(64 * 1024);
        let mut text = String::new();
        if file.seek(SeekFrom::Start(from)).is_ok() {
            let mut bytes = Vec::new();
            let _ = file.read_to_end(&mut bytes);
            text = String::from_utf8_lossy(&bytes).into_owned();
        }
        let all: Vec<&str> = text.lines().collect();
        all[all.len().saturating_sub(lines)..].join("\n")
    }

    fn launch(name: &str, args: &[String]) -> Res<u32> {
        if !control::valid_name(name) {
            return Err(
                "a name is 1 to 32 characters of a-z, 0-9, - or _ (not starting with -)".into(),
            );
        }
        if let Ok(existing) = find(name) {
            if existing.running() {
                return Err(format!("{name} is already running (pid {})", existing.pid));
            }
        }
        control::private_dir(&control::logs_dir())
            .map_err(|e| format!("cannot create the log directory: {e}"))?;
        // A leftover record from a dead server would make the wait below see a false success.
        control::remove_files(name);
        let log_path = control::log_path(name);
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| format!("cannot open {}: {e}", log_path.display()))?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let _ = std::io::Write::write_all(
            &mut &log,
            format!("--- be2-ctl starting {name} at unix time {stamp} ---\n").as_bytes(),
        );
        let program = server_binary();
        let mut child = Command::new(&program)
            .args(with_name(args, name))
            .env(LAUNCHER_ENV, LAUNCHED_BY_CTL)
            .stdin(Stdio::null())
            .stdout(log.try_clone().map_err(|e| e.to_string())?)
            .stderr(log)
            // Its own process group: Ctrl-C in this terminal must not reach a server that should outlive it.
            .process_group(0)
            .spawn()
            .map_err(|e| format!("cannot run {}: {e}", program.display()))?;
        let deadline = Instant::now() + START_TIMEOUT;
        loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                let mut message = format!("{name} exited during startup ({status})");
                let output = tail(&log_path, 12);
                let output = output
                    .split("--- be2-ctl starting")
                    .last()
                    .unwrap_or("")
                    .trim()
                    .to_owned();
                if !output.is_empty() {
                    message.push_str(&format!("; its log says:\n{output}"));
                }
                return Err(message);
            }
            if ServerRecord::read(name).is_ok_and(|r| r.pid == child.id()) && query(name).is_ok() {
                return Ok(child.id());
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "{name} (pid {}) did not come up within {}s; see `be2-ctl logs {name}`",
                    child.id(),
                    START_TIMEOUT.as_secs()
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn start(name: &str, opts: &Opts) -> Res<i32> {
        let mut definitions =
            control::read_definitions().map_err(|e| format!("definitions.json: {e}"))?;
        let args = if opts.server_args.is_empty() {
            definitions
                .get(name)
                .map(|d| d.args.clone())
                .ok_or_else(|| {
                    format!("no saved definition for {name}: give its arguments after `--`")
                })?
        } else {
            opts.server_args.clone()
        };
        if !args.iter().any(|a| a == "--server") {
            return Err("server arguments must include --server [ADDR]".into());
        }
        let pid = launch(name, &args)?;
        if opts.save {
            definitions.insert(name.to_owned(), Definition { args });
            control::write_definitions(&definitions).map_err(|e| e.to_string())?;
        }
        println!(
            "{name} started (pid {pid}); log: {}",
            control::log_path(name).display()
        );
        Ok(0)
    }

    fn restart(name: &str, timeout: Duration, force: bool) -> Res<i32> {
        let entry = find(name).ok();
        let definitions =
            control::read_definitions().map_err(|e| format!("definitions.json: {e}"))?;
        let args = match (&entry, definitions.get(name)) {
            (_, Some(definition)) => definition.args.clone(),
            (Some(e), None) if e.record.as_ref().is_some_and(|r| r.launcher == LAUNCHED_BY_CTL) => e.args.clone(),
            (Some(e), None) => {
                return Err(format!(
                    "{name} was not started by be2-ctl, so be2-ctl will not restart it; \
                     restart it with whatever runs it{}, or save how to start it: be2-ctl define {name} -- ARGS",
                    if e.state == "unregistered" { " (it never registered either)" } else { "" }
                ))
            }
            (None, None) => return Err(format!("no server or definition named {name}")),
        };
        if let Some(entry) = entry.filter(Entry::running) {
            println!("{name}: {}", stop_entry(&entry, timeout, force)?);
            if !entry.name.starts_with("pid:") {
                control::remove_files(&entry.name);
            }
        }
        if name.starts_with("pid:") {
            return Err(
                "an unregistered server has no name to restart under: `be2-ctl start NAME -- ARGS`"
                    .into(),
            );
        }
        let pid = launch(name, &args)?;
        println!("{name} started (pid {pid})");
        Ok(0)
    }

    // ---- the rest --------------------------------------------------------------------------------

    fn logs(name: &str, lines: usize) -> Res<i32> {
        if !control::valid_name(name) {
            return Err("not a valid server name".into());
        }
        let path = control::log_path(name);
        if !path.exists() {
            return Err(format!(
                "no log for {name}: only servers started by be2-ctl have one"
            ));
        }
        println!("{}", tail(&path, lines));
        Ok(0)
    }

    fn define(name: &str, args: &[String]) -> Res<i32> {
        if !control::valid_name(name) {
            return Err(
                "a name is 1 to 32 characters of a-z, 0-9, - or _ (not starting with -)".into(),
            );
        }
        if !args.iter().any(|a| a == "--server") {
            return Err("server arguments must include --server [ADDR]".into());
        }
        let mut definitions =
            control::read_definitions().map_err(|e| format!("definitions.json: {e}"))?;
        definitions.insert(
            name.to_owned(),
            Definition {
                args: args.to_vec(),
            },
        );
        control::write_definitions(&definitions).map_err(|e| e.to_string())?;
        println!("saved {name}: be2-ctl start {name}");
        Ok(0)
    }

    fn undefine(name: &str) -> Res<i32> {
        let mut definitions =
            control::read_definitions().map_err(|e| format!("definitions.json: {e}"))?;
        if definitions.remove(name).is_none() {
            return Err(format!("no saved definition for {name}"));
        }
        control::write_definitions(&definitions).map_err(|e| e.to_string())?;
        println!("removed the definition of {name}");
        Ok(0)
    }

    fn prune() -> Res<i32> {
        let mut removed = 0;
        for entry in entries() {
            if entry.state == "stale" {
                control::remove_files(&entry.name);
                println!("removed stale record {}", entry.name);
                removed += 1;
            }
        }
        if removed == 0 {
            println!("nothing to prune");
        }
        Ok(0)
    }
}
