//! The server side of management: register, listen on the control socket, answer `status` and `shutdown`.
//!
//! The control thread never touches the simulation. The server loop publishes a small [`Status`] snapshot now and then
//! ([`StatusSink::update`]); the thread answers `status` from the latest one and answers `shutdown` by setting the same
//! stop flag a signal sets, so a graceful stop from `be2-ctl` and one from `kill -TERM` take exactly the same path.
use super::{
    proc, read_line, record_path, remove_files, servers_dir, socket_path, Reply, Request,
    ServerRecord, Status, IO_TIMEOUT,
};
use std::{
    io::{self, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

/// Where the server loop puts its latest numbers for the control thread to report.
#[derive(Clone, Default)]
pub struct StatusSink(Arc<Mutex<Status>>);

impl StatusSink {
    /// Change the published snapshot. Keep `f` short: the control thread waits on the same lock.
    pub fn update(&self, f: impl FnOnce(&mut Status)) {
        f(&mut self.0.lock().unwrap_or_else(|p| p.into_inner()));
    }
    fn get(&self) -> Status {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

/// A registered server. Dropping it (a normal exit) removes the record and the socket; a crash leaves them behind, and
/// `be2-ctl` recognises the record as stale because its process is gone.
pub struct Control {
    name: String,
    quit: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    sink: StatusSink,
}

impl Control {
    /// Register `record` (whose `pid`, `socket` and `start_ticks` are filled in here) and start answering requests.
    /// `stop` is set when a client asks for a shutdown.
    pub fn start(mut record: ServerRecord, stop: Arc<AtomicBool>) -> io::Result<Self> {
        super::private_dir(&servers_dir())?;
        let name = record.name.clone();
        if let Ok(existing) = ServerRecord::read(&name) {
            if existing.process_alive() && super::ask(&name, &Request::Status).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    format!(
                        "a server named '{name}' is already running (pid {})",
                        existing.pid
                    ),
                ));
            }
        }
        // Either nothing was registered, or what is there is stale (its process is gone or no longer answering).
        remove_files(&name);
        let socket = socket_path(&name);
        let listener = UnixListener::bind(&socket).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("cannot create the control socket {}: {e}", socket.display()),
            )
        })?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;

        record.pid = std::process::id();
        record.start_ticks = proc::start_ticks(record.pid);
        record.socket = socket.display().to_string();
        record.write()?;

        let sink = StatusSink::default();
        let quit = Arc::new(AtomicBool::new(false));
        let thread = {
            let (sink, quit, name, stop) = (sink.clone(), Arc::clone(&quit), name.clone(), stop);
            let started = Instant::now();
            std::thread::Builder::new()
                .name("control".into())
                .spawn(move || {
                    while !quit.load(Ordering::Relaxed) {
                        match listener.accept() {
                            Ok((stream, _)) => {
                                let _ = serve(stream, &sink, &stop, started, &name);
                            }
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                std::thread::sleep(Duration::from_millis(25))
                            }
                            Err(_) => std::thread::sleep(Duration::from_millis(100)),
                        }
                    }
                })?
        };
        Ok(Self {
            name,
            quit,
            thread: Some(thread),
            sink,
        })
    }

    pub fn sink(&self) -> StatusSink {
        self.sink.clone()
    }
}

impl Drop for Control {
    fn drop(&mut self) {
        self.quit.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        remove_files(&self.name);
    }
}

/// Answer one connection: read a line, parse it, reply with a line.
fn serve(
    mut stream: UnixStream,
    sink: &StatusSink,
    stop: &AtomicBool,
    started: Instant,
    name: &str,
) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let reply = match read_line(&mut stream) {
        Err(e) => Reply::error(format!("could not read the request: {e}")),
        Ok(line) if line.trim().is_empty() => Reply::error("empty request"),
        Ok(line) => match serde_json::from_str::<Request>(line.trim()) {
            Err(e) => Reply::error(format!("not a request this server understands: {e}")),
            Ok(Request::Status) => {
                let mut status = sink.get();
                status.name = name.to_owned();
                status.pid = std::process::id();
                status.uptime_s = started.elapsed().as_secs_f64();
                status.stopping = stop.load(Ordering::SeqCst);
                Reply::Status { ok: true, status }
            }
            Ok(Request::Shutdown) => {
                eprintln!("[Server] Shutdown requested through the control socket: finishing this tick, saving, then exiting");
                stop.store(true, Ordering::SeqCst);
                Reply::Done {
                    ok: true,
                    message: "shutting down".into(),
                }
            }
        },
    };
    let mut line = serde_json::to_vec(&reply)?;
    line.push(b'\n');
    stream.write_all(&line)
}

/// The path of the record for `name`, for tools that want to show it.
pub fn record_file(name: &str) -> std::path::PathBuf {
    record_path(name)
}
