//! Managing running servers: where they register, how they are asked for status and told to stop (Unix only).
//!
//! A server started with `--name NAME` writes a small **record** (pid, address, arguments, who launched it) into the
//! state directory and listens on an owner-only Unix socket there. `be2-ctl` reads the records, talks to the sockets, and
//! keeps **definitions** (a name and the arguments to start it with) so a stopped server can be started again. There is no
//! daemon: everything lives in those files and in the servers themselves.
//!
//! ```text
//! $BLUEENGINE_STATE_DIR   (default $XDG_STATE_HOME/blueengine, then ~/.local/state/blueengine)
//!   servers/NAME.json     the record of a running (or crashed) server
//!   servers/NAME.sock     its control socket (mode 0600, directory 0700)
//!   logs/NAME.log         output of servers started by be2-ctl
//!   definitions.json      { "NAME": { "args": ["--server", ...] } }
//! ```
//!
//! **Protocol.** One request per connection, one line of JSON each way, at most [`MAX_LINE`] bytes, two seconds to send it:
//! `{"cmd":"status"}` and `{"cmd":"shutdown"}`. Anything else is an error reply, never a crash.
pub mod proc;
pub mod server;

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{self, BufRead, Read, Write},
    os::unix::{
        fs::{DirBuilderExt, PermissionsExt},
        net::UnixStream,
    },
    path::{Path, PathBuf},
    time::Duration,
};

/// Longest request or reply line, in bytes.
pub const MAX_LINE: usize = 16 * 1024;
/// How long a peer has to send its request, and how long a client waits for an answer.
pub const IO_TIMEOUT: Duration = Duration::from_secs(2);
/// The program that servers run as, for finding ones that never registered.
pub const SERVER_PROGRAM: &str = "be2-headless";

/// Server names are short, lowercase and safe to use as file names.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        && !name.starts_with('-')
}

/// Where the records, sockets, logs and definitions live.
pub fn state_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("BLUEENGINE_STATE_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    if let Some(dir) = std::env::var_os("XDG_STATE_HOME").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir).join("blueengine");
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".local/state/blueengine")
}

/// Create `dir` (and parents) readable only by the owner.
pub fn private_dir(dir: &Path) -> io::Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

pub fn servers_dir() -> PathBuf {
    state_dir().join("servers")
}
pub fn logs_dir() -> PathBuf {
    state_dir().join("logs")
}
pub fn record_path(name: &str) -> PathBuf {
    servers_dir().join(format!("{name}.json"))
}
pub fn socket_path(name: &str) -> PathBuf {
    servers_dir().join(format!("{name}.sock"))
}
pub fn log_path(name: &str) -> PathBuf {
    logs_dir().join(format!("{name}.log"))
}
pub fn definitions_path() -> PathBuf {
    state_dir().join("definitions.json")
}

/// Who started a server: `be2-ctl` can restart its own; it only observes and stops the rest (systemd, Docker, by hand).
pub const LAUNCHER_ENV: &str = "BE2_LAUNCHER";
pub const LAUNCHED_BY_CTL: &str = "be2-ctl";

/// What a running server writes about itself.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ServerRecord {
    pub name: String,
    pub pid: u32,
    /// Process start time in clock ticks; with the pid it tells a recycled pid from the server that owned it.
    #[serde(default)]
    pub start_ticks: Option<u64>,
    pub started_unix: u64,
    pub addr: String,
    /// The full command line after the program name, so a server can be restarted exactly as it was started.
    pub args: Vec<String>,
    /// `be2-ctl` if it launched this server, otherwise `external`.
    pub launcher: String,
    pub socket: String,
    #[serde(default)]
    pub log: Option<String>,
    pub version: String,
    pub protocol: u32,
}

/// Write `bytes` to `path` without ever leaving a half-written file behind.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

impl ServerRecord {
    pub fn write(&self) -> io::Result<()> {
        private_dir(&servers_dir())?;
        write_atomic(&record_path(&self.name), &serde_json::to_vec_pretty(self)?)
    }
    pub fn read(name: &str) -> io::Result<Self> {
        Ok(serde_json::from_slice(&std::fs::read(record_path(name))?)?)
    }
    /// Is the process this record describes still the one running? False if the pid is gone, a zombie, or was recycled.
    pub fn process_alive(&self) -> bool {
        if !proc::is_alive(self.pid) {
            return false;
        }
        match (self.start_ticks, proc::start_ticks(self.pid)) {
            (Some(recorded), Some(now)) => recorded == now,
            _ => true, // no start time to compare (not Linux): trust the pid
        }
    }
}

/// Remove a server's record and socket (both may already be gone).
pub fn remove_files(name: &str) {
    let _ = std::fs::remove_file(record_path(name));
    let _ = std::fs::remove_file(socket_path(name));
}

/// Every record in the state directory, sorted by name. A record that cannot be parsed is reported, not hidden.
pub fn read_records() -> Vec<(String, io::Result<ServerRecord>)> {
    let Ok(entries) = std::fs::read_dir(servers_dir()) else {
        return Vec::new();
    };
    let mut records: Vec<_> = entries
        .filter_map(|e| {
            let path = e.ok()?.path();
            let name = path
                .file_name()?
                .to_str()?
                .strip_suffix(".json")?
                .to_owned();
            Some((name.clone(), ServerRecord::read(&name)))
        })
        .collect();
    records.sort_by(|a, b| a.0.cmp(&b.0));
    records
}

/// A saved way to start a server.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Definition {
    pub args: Vec<String>,
}

pub fn read_definitions() -> io::Result<BTreeMap<String, Definition>> {
    match std::fs::read(definitions_path()) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e),
    }
}

pub fn write_definitions(definitions: &BTreeMap<String, Definition>) -> io::Result<()> {
    private_dir(&state_dir())?;
    write_atomic(
        &definitions_path(),
        &serde_json::to_vec_pretty(definitions)?,
    )
}

/// What a server reports about itself.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub name: String,
    pub pid: u32,
    pub uptime_s: f64,
    pub tick: u64,
    pub addr: String,
    pub transport: String,
    pub map: String,
    pub clients: usize,
    pub max_players: usize,
    pub network_threads: usize,
    /// Mean and worst tick time over the last status window, in microseconds.
    pub tick_mean_us: u64,
    pub tick_max_us: u64,
    pub autosave_written: u64,
    pub autosave_failed: u64,
    /// Mean broadcasts a changed record waited before it was sent (1.0 = sent at the first chance).
    pub replication_mean_wait: f64,
    pub version: String,
    pub protocol: u32,
    /// Set once the server has been told to shut down and is finishing its last tick and save.
    pub stopping: bool,
}

/// A request to a server.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Status,
    Shutdown,
}

/// A server's answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Reply {
    Status { ok: bool, status: Status },
    Done { ok: bool, message: String },
    Error { ok: bool, error: String },
}

impl Reply {
    pub fn error(message: impl Into<String>) -> Self {
        Reply::Error {
            ok: false,
            error: message.into(),
        }
    }
}

/// Read one line of at most [`MAX_LINE`] bytes; longer is an error rather than unbounded memory.
pub fn read_line(stream: &mut impl Read) -> io::Result<String> {
    let mut reader = io::BufReader::new(stream.take(MAX_LINE as u64 + 1));
    let mut line = String::new();
    reader.read_line(&mut line)?;
    if line.len() > MAX_LINE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "line too long"));
    }
    Ok(line)
}

/// Ask the server called `name` something and wait for its answer.
pub fn ask(name: &str, request: &Request) -> io::Result<Reply> {
    let mut stream = UnixStream::connect(socket_path(name))?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let mut line = serde_json::to_vec(request)?;
    line.push(b'\n');
    stream.write_all(&line)?;
    let reply = read_line(&mut stream)?;
    if reply.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the server closed the connection without answering",
        ));
    }
    serde_json::from_str(&reply).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_short_lowercase_and_file_safe() {
        for good in ["lobby", "test-1", "a", "arena_2", &"x".repeat(32)] {
            assert!(valid_name(good), "{good}");
        }
        for bad in [
            "",
            "Lobby",
            "a b",
            "../x",
            "a/b",
            "-lead",
            "x.y",
            &"x".repeat(33),
            "é",
        ] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn requests_and_replies_round_trip_as_single_lines() {
        for request in [Request::Status, Request::Shutdown] {
            let text = serde_json::to_string(&request).unwrap();
            assert!(!text.contains('\n'));
            assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
        }
        assert_eq!(
            serde_json::to_string(&Request::Shutdown).unwrap(),
            r#"{"cmd":"shutdown"}"#
        );
        assert!(serde_json::from_str::<Request>(r#"{"cmd":"format_disk"}"#).is_err());
        let done = Reply::Done {
            ok: true,
            message: "stopping".into(),
        };
        assert_eq!(
            serde_json::from_str::<Reply>(&serde_json::to_string(&done).unwrap()).unwrap(),
            done
        );
    }

    #[test]
    fn a_line_longer_than_the_limit_is_refused() {
        let long = vec![b'a'; MAX_LINE + 10];
        assert!(read_line(&mut &long[..]).is_err());
        let fine = b"{\"cmd\":\"status\"}\n";
        assert_eq!(read_line(&mut &fine[..]).unwrap(), "{\"cmd\":\"status\"}\n");
    }
}
