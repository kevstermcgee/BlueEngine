//! A real `be2-headless` stops cleanly on a shutdown signal, and a second signal stops it at once.
//!
//! Before this, SIGINT and SIGTERM killed the server mid-tick (exit status 143) with no final autosave.
#![cfg(unix)]
use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}
const SIGHUP: i32 = 1;
const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;
const SIGSTOP: i32 = 19;
const SIGCONT: i32 = 18;

struct Server {
    child: Child,
    log: Arc<Mutex<Vec<String>>>,
    saves: PathBuf,
}

impl Server {
    /// Start a server on an ephemeral port and wait until it says it is listening.
    fn start(name: &str, autosave: bool) -> Self {
        let saves =
            std::env::temp_dir().join(format!("be2-shutdown-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&saves);
        std::fs::create_dir_all(&saves).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_be2-headless"));
        command.args([
            "--server",
            "127.0.0.1:0",
            "--save-dir",
            saves.to_str().unwrap(),
        ]);
        if autosave {
            command.args(["--autosave", "3600"]);
        }
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        for stream in [
            Box::new(child.stdout.take().unwrap()) as Box<dyn std::io::Read + Send>,
            Box::new(child.stderr.take().unwrap()),
        ] {
            let log = Arc::clone(&log);
            std::thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    log.lock().unwrap().push(line);
                }
            });
        }
        let server = Self { child, log, saves };
        server.wait_for("listening on", Duration::from_secs(30));
        server
    }
    fn pid(&self) -> i32 {
        self.child.id() as i32
    }
    fn signal(&self, signal: i32) {
        assert_eq!(unsafe { kill(self.pid(), signal) }, 0, "signal {signal}");
    }
    fn text(&self) -> String {
        self.log.lock().unwrap().join("\n")
    }
    fn wait_for(&self, needle: &str, within: Duration) {
        let deadline = Instant::now() + within;
        while !self.text().contains(needle) {
            assert!(
                Instant::now() < deadline,
                "never saw {needle:?}; log so far:\n{}",
                self.text()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    /// Wait for the process to end on its own and return its exit code (`None` if a signal killed it).
    fn wait_exit(&mut self, within: Duration) -> Option<i32> {
        let deadline = Instant::now() + within;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                std::thread::sleep(Duration::from_millis(100)); // let the reader threads drain the pipes
                return status.code();
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                panic!(
                    "the server did not exit within {within:?}; log:\n{}",
                    self.text()
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    fn saved_files(&self) -> usize {
        std::fs::read_dir(&self.saves)
            .map(|d| d.count())
            .unwrap_or(0)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = std::fs::remove_dir_all(&self.saves);
    }
}

#[test]
fn sigterm_sigint_and_sighup_each_stop_the_server_cleanly_with_a_final_save() {
    for (name, signal) in [("term", SIGTERM), ("int", SIGINT), ("hup", SIGHUP)] {
        let mut server = Server::start(name, true);
        assert_eq!(server.saved_files(), 0, "nothing is saved before shutdown");
        server.signal(signal);
        assert_eq!(
            server.wait_exit(Duration::from_secs(20)),
            Some(0),
            "{name}: a clean exit, not death by signal"
        );
        let log = server.text();
        assert!(log.contains("Shutdown requested"), "{name}: {log}");
        assert!(log.contains("Final save written"), "{name}: {log}");
        assert!(log.contains("Shutdown complete"), "{name}: {log}");
        assert!(
            server.saved_files() >= 1,
            "{name}: the final autosave is on disk"
        );
    }
}

#[test]
fn a_server_with_no_autosave_still_exits_cleanly_on_sigterm() {
    let mut server = Server::start("plain", false);
    server.signal(SIGTERM);
    assert_eq!(server.wait_exit(Duration::from_secs(20)), Some(0));
    let log = server.text();
    assert!(
        log.contains("Shutdown complete") && !log.contains("Final save"),
        "{log}"
    );
    assert_eq!(server.saved_files(), 0, "nothing to save, nothing written");
}

/// Freeze the server, queue two different shutdown signals, then let it run: both arrive before any code does, so the
/// second request must force the exit instead of waiting for the save.
#[test]
fn a_second_shutdown_request_forces_an_immediate_exit() {
    let mut server = Server::start("force", true);
    server.signal(SIGSTOP);
    server.signal(SIGTERM);
    server.signal(SIGINT);
    server.signal(SIGCONT);
    assert_eq!(
        server.wait_exit(Duration::from_secs(20)),
        Some(130),
        "forced exit status"
    );
    let log = server.text();
    assert!(log.contains("Second shutdown request"), "{log}");
    assert!(
        !log.contains("Final save written"),
        "a forced exit does not wait for the save: {log}"
    );
}
