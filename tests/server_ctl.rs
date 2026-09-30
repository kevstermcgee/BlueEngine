//! `be2-ctl` against real `be2-headless` servers: lifecycle, stuck and unregistered servers, stale records,
//! and a control socket that survives bad input.
#![cfg(unix)]
use std::{
    io::{Read, Write},
    os::unix::{fs::PermissionsExt, net::UnixStream},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant},
};

extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}
const SIGKILL: i32 = 9;
const SIGSTOP: i32 = 19;
const SIGCONT: i32 = 18;

static NEXT: AtomicU32 = AtomicU32::new(0);

/// An isolated state directory; every server it started is killed when it is dropped.
struct Env {
    state: PathBuf,
    pids: std::cell::RefCell<Vec<u32>>,
}

impl Env {
    fn new() -> Self {
        // Short: a Unix socket path is limited to about 100 bytes.
        let state = std::env::temp_dir().join(format!(
            "be2ctl-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&state);
        std::fs::create_dir_all(&state).unwrap();
        Env {
            state,
            pids: Default::default(),
        }
    }

    fn ctl(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_be2-ctl"))
            .args(args)
            .env("BLUEENGINE_STATE_DIR", &self.state)
            .env("BE2_HEADLESS", env!("CARGO_BIN_EXE_be2-headless"))
            .output()
            .unwrap()
    }

    /// Run be2-ctl, expecting success, and return its standard output.
    fn ok(&self, args: &[&str]) -> String {
        let out = self.ctl(args);
        assert!(
            out.status.success(),
            "be2-ctl {args:?} failed: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Run be2-ctl, expecting failure, and return its standard error.
    fn err(&self, args: &[&str]) -> String {
        let out = self.ctl(args);
        assert!(!out.status.success(), "be2-ctl {args:?} should have failed");
        String::from_utf8_lossy(&out.stderr).into_owned()
    }

    fn start(&self, name: &str, extra: &[&str]) -> u32 {
        let mut args = vec!["start", name, "--", "--server", "127.0.0.1:0"];
        args.extend_from_slice(extra);
        self.ok(&args);
        let pid = self.pid_of(name);
        self.pids.borrow_mut().push(pid);
        pid
    }

    fn list(&self) -> Vec<serde_json::Value> {
        let text = self.ok(&["list", "--json"]);
        serde_json::from_str(&text).unwrap()
    }

    fn entry(&self, name: &str) -> Option<serde_json::Value> {
        self.list().into_iter().find(|e| e["name"] == name)
    }

    fn pid_of(&self, name: &str) -> u32 {
        self.entry(name)
            .unwrap_or_else(|| panic!("{name} is not listed"))["pid"]
            .as_u64()
            .unwrap() as u32
    }

    fn socket(&self, name: &str) -> PathBuf {
        self.state.join("servers").join(format!("{name}.sock"))
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        for pid in self.pids.borrow().iter() {
            unsafe {
                kill(*pid as i32, SIGCONT);
                kill(*pid as i32, SIGKILL);
            }
        }
        let _ = std::fs::remove_dir_all(&self.state);
    }
}

/// Is this process running (a zombie nobody has reaped yet counts as gone)?
fn running(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|s| s.rsplit(')').next().map(|r| r.trim_start().chars().next()))
        .flatten()
        .is_some_and(|state| state != 'Z')
}

fn wait_until(what: &str, limit: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + limit;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn start_list_status_stop_leaves_nothing_behind() {
    let env = Env::new();
    let saves = env.state.join("saves");
    let pid = env.start(
        "life",
        &[
            "--save-dir",
            saves.to_str().unwrap(),
            "--autosave",
            "3600",
            "--max-players",
            "5",
        ],
    );
    let entry = env.entry("life").unwrap();
    assert_eq!(entry["state"], "running");
    assert_eq!(entry["launcher"], "be2-ctl");
    assert_eq!(entry["status"]["max_players"], 5);

    let status: serde_json::Value =
        serde_json::from_str(&env.ok(&["status", "life", "--json"])).unwrap();
    assert_eq!(status["pid"], pid);
    assert_eq!(status["transport"], "development/udp");
    let table = env.ok(&["list"]);
    assert!(
        table.contains("life") && table.contains("running"),
        "{table}"
    );

    let said = env.ok(&["stop", "life"]);
    assert!(said.contains("asked to shut down"), "{said}");
    wait_until("the server to exit", Duration::from_secs(5), || {
        !running(pid)
    });
    assert!(env.entry("life").is_none(), "the record should be removed");
    assert!(!env.socket("life").exists());
    assert!(
        std::fs::read_dir(&saves).is_ok_and(|mut d| d.next().is_some()),
        "a graceful stop writes the final autosave"
    );
    assert!(env.ok(&["logs", "life"]).contains("Shutdown complete"));
}

#[test]
fn restart_replaces_the_process_and_keeps_the_arguments() {
    let env = Env::new();
    let before = env.start("again", &["--max-players", "7"]);
    let said = env.ok(&["restart", "again"]);
    assert!(said.contains("started"), "{said}");
    let after = env.pid_of("again");
    env.pids.borrow_mut().push(after);
    assert_ne!(before, after);
    wait_until("the old process to exit", Duration::from_secs(5), || {
        !running(before)
    });
    assert_eq!(env.entry("again").unwrap()["status"]["max_players"], 7);
}

#[test]
fn saved_definition_starts_a_server() {
    let env = Env::new();
    env.ok(&[
        "define",
        "saved",
        "--",
        "--server",
        "127.0.0.1:0",
        "--max-players",
        "3",
    ]);
    env.ok(&["start", "saved"]);
    env.pids.borrow_mut().push(env.pid_of("saved"));
    assert_eq!(env.entry("saved").unwrap()["status"]["max_players"], 3);
    env.ok(&["stop", "saved"]);
    env.ok(&["undefine", "saved"]);
    assert!(env.err(&["start", "saved"]).contains("no saved definition"));
}

#[test]
fn a_second_server_with_the_same_name_is_refused() {
    let env = Env::new();
    let pid = env.start("dup", &[]);
    let message = env.err(&["start", "dup", "--", "--server", "127.0.0.1:0"]);
    assert!(message.contains("already running"), "{message}");
    assert!(running(pid), "the first server must be untouched");
    assert!(env
        .err(&["start", "bad name", "--", "--server"])
        .contains("characters"));
}

#[test]
fn a_server_that_cannot_start_reports_why() {
    let env = Env::new();
    let message = env.err(&[
        "start",
        "broken",
        "--",
        "--server",
        "127.0.0.1:0",
        "--map",
        "/definitely/not/here.json",
    ]);
    assert!(message.contains("exited during startup"), "{message}");
    assert!(env.entry("broken").is_none());
}

#[test]
fn an_unregistered_server_is_found_and_stopped() {
    let env = Env::new();
    let mut child = Command::new(env!("CARGO_BIN_EXE_be2-headless"))
        .args(["--server", "127.0.0.1:0"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pid = child.id();
    let name = format!("pid:{pid}");
    wait_until("be2-ctl to see it", Duration::from_secs(5), || {
        env.entry(&name).is_some()
    });
    assert_eq!(env.entry(&name).unwrap()["state"], "unregistered");
    assert!(env
        .err(&["restart", &name])
        .contains("not started by be2-ctl"));
    assert!(running(pid));
    env.ok(&["stop", &name]);
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(0), "SIGTERM is a graceful stop");
}

#[test]
fn a_stuck_server_needs_force_and_then_dies() {
    let env = Env::new();
    let pid = env.start("stuck", &[]);
    unsafe { kill(pid as i32, SIGSTOP) };
    let message = env.err(&["stop", "stuck", "--timeout", "1"]);
    assert!(message.contains("--force"), "{message}");
    assert!(
        running(pid),
        "without --force the stuck server is left alone"
    );
    let said = env.ok(&["stop", "stuck", "--timeout", "1", "--force"]);
    assert!(said.contains("SIGKILL"), "{said}");
    wait_until("the stuck server to die", Duration::from_secs(5), || {
        !running(pid)
    });
    assert!(env.entry("stuck").is_none());
}

#[test]
fn stale_records_are_shown_and_pruned() {
    let env = Env::new();
    let dir = env.state.join("servers");
    std::fs::create_dir_all(&dir).unwrap();
    let record = serde_json::json!({
        "name": "ghost", "pid": 4_000_000_000u32, "started_unix": 1, "addr": "0.0.0.0:1",
        "args": ["--server"], "launcher": "external", "socket": "", "version": "0", "protocol": 0
    });
    std::fs::write(dir.join("ghost.json"), record.to_string()).unwrap();
    std::fs::write(dir.join("broken.json"), "{not json").unwrap();
    assert_eq!(env.entry("ghost").unwrap()["state"], "stale");
    assert_eq!(env.entry("broken").unwrap()["state"], "unreadable");
    assert!(env.err(&["status", "ghost"]).contains("stale"));
    let pruned = env.ok(&["prune"]);
    assert!(pruned.contains("ghost"), "{pruned}");
    assert!(env.entry("ghost").is_none());
    assert!(
        env.entry("broken").is_some(),
        "an unreadable record is never deleted silently"
    );
}

fn converse(socket: &Path, bytes: &[u8]) -> String {
    let mut stream = UnixStream::connect(socket).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    // The server may answer and close before it has read everything we send.
    let _ = stream.write_all(bytes);
    let mut reply = String::new();
    let _ = stream.read_to_string(&mut reply);
    reply
}

#[test]
fn the_control_socket_is_private_and_survives_bad_input() {
    let env = Env::new();
    env.start("guard", &[]);
    let socket = env.socket("guard");
    let mode = std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "only the owner may control a server");
    let dir_mode = std::fs::metadata(env.state.join("servers"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(dir_mode, 0o700);

    for (input, expect) in [
        (&b"not json\n"[..], "\"ok\":false"),
        (&b"{\"cmd\":\"explode\"}\n"[..], "\"ok\":false"),
        (&b"\n"[..], "\"ok\":false"),
        (&vec![b'x'; 40_000][..], "\"ok\":false"),
    ] {
        let reply = converse(&socket, input);
        assert!(reply.contains(expect), "{input:?} -> {reply}");
    }
    // A client that connects and says nothing must not wedge the server.
    let _idle = UnixStream::connect(&socket).unwrap();
    let reply = converse(&socket, b"{\"cmd\":\"status\"}\n");
    assert!(
        reply.contains("\"ok\":true") && reply.contains("guard"),
        "{reply}"
    );
    assert!(env.ok(&["status", "guard"]).contains("guard"));
}
