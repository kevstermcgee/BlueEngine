//! Native save states: one framed, versioned, integrity-checked file format that every engine and game
//! payload shares, with atomic writes, a last-good backup, named slots and payload migration.
//!
//! A save state is a snapshot of a deterministic simulation that can be restored later: the engine's own
//! [`HeadlessWorld`](crate::viewer::simulation::HeadlessWorld) (see [`world`]) and any game's own
//! simulation (see `devkit::Snapshot`). This module is the part that must never fail the player:
//!
//! * **A save never replaces a good file with a partial one.** [`write_atomic`] writes a temporary file
//!   in the same directory, flushes it to disk, then renames it over the target (retrying the short
//!   sharing violations antivirus scanners cause on Windows). Before replacing a file it checks that the
//!   old file is itself valid and only then keeps it as `<file>.bak`, so a corrupt primary can never
//!   overwrite the last good backup. After the rename the new file is read back and verified; a failed
//!   verification restores the backup.
//! * **A corrupt file is never accepted.** Every byte of a save is covered by a SHA-256 trailer; magic,
//!   format version, lengths and header are validated before the payload is trusted, with hard size caps
//!   and no allocation driven by an unchecked length. Loading never panics on any input.
//! * **A failed load says why**, with a typed [`SaveError`] (not a save, newer than this build, truncated,
//!   corrupt, for different content, wrong kind), and falls back to the backup when the primary is bad.
//! * **Formats evolve safely**: the frame has a format version; each payload kind has its own version and
//!   a chain of [`Migration`] steps upgrades older payloads; newer payloads are refused, not guessed at.
//!
//! AI-INVARIANT SAVE-ATOMIC-001: a save never replaces a good file with a partial one, and a file whose
//! bytes do not verify is never loaded (`cargo test --lib savestate` proves both by corrupting every bit).
//!
//! # File layout (little-endian)
//!
//! | Bytes | Field |
//! |---|---|
//! | 0..8 | magic `BE2SAVE\x1a` |
//! | 8..10 | frame format version (`1`) |
//! | 10..12 | flags (must be `0`) |
//! | 12..16 | header length `H` |
//! | 16..24 | payload length `P` |
//! | 24..24+H | header: UTF-8 JSON ([`SaveHeader`]) |
//! | 24+H..24+H+P | payload: UTF-8 JSON of the state |
//! | last 32 | SHA-256 of every preceding byte |
//!
//! ```
//! use vesper3d::viewer::savestate::{decode, encode, SaveHeader};
//! let header = SaveHeader::new("demo", 1, "Quick save");
//! let bytes = encode(&header, br#"{"score":7}"#).unwrap();
//! let loaded = decode(&bytes).unwrap();
//! assert_eq!(loaded.header.label, "Quick save");
//! assert_eq!(loaded.payload, br#"{"score":7}"#);
//! let mut damaged = bytes.clone();
//! damaged[30] ^= 1;
//! assert!(decode(&damaged).is_err(), "one flipped bit is always caught");
//! ```
use crate::runtime::hash::sha256;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(not(target_arch = "wasm32"))]
pub mod world;

/// First eight bytes of every save file.
pub const MAGIC: [u8; 8] = *b"BE2SAVE\x1a";
/// Version of the frame layout in this build. Older frames stay readable; newer ones are refused.
pub const FRAME_VERSION: u16 = 1;
/// Fixed prefix length: magic, version, flags, header length, payload length.
const PREFIX: usize = 24;
const DIGEST: usize = 32;
/// Largest header accepted, in bytes.
pub const MAX_HEADER_BYTES: usize = 64 * 1024;
/// Largest payload accepted, in bytes. A world save is tens of kilobytes to a few megabytes.
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;
/// File extension of a save slot.
pub const EXTENSION: &str = "be2save";
/// Slot used by a quick-save key.
pub const QUICK_SLOT: &str = "quick";
/// Slot (or ring head) used by automatic saves.
pub const AUTO_SLOT: &str = "auto";

/// Everything that can go wrong saving or loading, in words a player or an agent can act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    /// The operating system refused a file operation.
    Io { path: String, message: String },
    /// There is no save at this path or in this slot.
    NotFound(String),
    /// The file does not start with the save magic.
    NotASave,
    /// The frame layout is newer than this build understands.
    UnsupportedFormat { found: u16, supported: u16 },
    /// The file is shorter than its own lengths say.
    Truncated { expected: u64, found: u64 },
    /// Bytes follow the checksum.
    TrailingBytes,
    /// The bytes do not verify (bad checksum, unreadable header, impossible field).
    Corrupt(String),
    /// A length exceeds the hard cap.
    TooLarge { what: &'static str, limit: u64 },
    /// The save holds a different kind of state than the caller asked for.
    WrongKind { expected: String, found: String },
    /// The save belongs to different content (another map, game or tuning).
    WrongContent { expected: String, found: String },
    /// The payload was written by a newer version of its kind than this build knows.
    NewerVersion {
        kind: String,
        found: u32,
        supported: u32,
    },
    /// The payload parsed but does not describe a state this build can restore.
    Invalid(String),
    /// A slot name is not `[a-z0-9_-]{1,48}` or is a reserved device name.
    BadSlot(String),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, message } => write!(f, "{path}: {message}"),
            Self::NotFound(what) => write!(f, "no save found: {what}"),
            Self::NotASave => write!(f, "not a BlueEngine save file"),
            Self::UnsupportedFormat { found, supported } => {
                write!(f, "save frame format {found} is newer than this build supports ({supported}); update the game")
            }
            Self::Truncated { expected, found } => {
                write!(f, "save file is truncated: {found} bytes, expected {expected}")
            }
            Self::TrailingBytes => write!(f, "save file has unexpected bytes after its checksum"),
            Self::Corrupt(why) => write!(f, "save file is corrupt: {why}"),
            Self::TooLarge { what, limit } => write!(f, "save {what} exceeds the {limit} byte limit"),
            Self::WrongKind { expected, found } => {
                write!(f, "this is a '{found}' save, expected '{expected}'")
            }
            Self::WrongContent { expected, found } => write!(
                f,
                "this save belongs to different content (map/game/tuning fingerprint {found}, this game is {expected})"
            ),
            Self::NewerVersion { kind, found, supported } => write!(
                f,
                "'{kind}' save version {found} is newer than this build supports ({supported}); update the game"
            ),
            Self::Invalid(why) => write!(f, "save cannot be restored: {why}"),
            Self::BadSlot(name) => write!(f, "invalid save slot name '{name}' (use a-z, 0-9, _ and -, at most 48)"),
        }
    }
}

impl std::error::Error for SaveError {}

fn io_error(path: &Path, error: std::io::Error) -> SaveError {
    if error.kind() == std::io::ErrorKind::NotFound {
        SaveError::NotFound(path.display().to_string())
    } else {
        SaveError::Io {
            path: path.display().to_string(),
            message: error.to_string(),
        }
    }
}

/// The self-describing part of a save: what it is, which content it belongs to, and how to show it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SaveHeader {
    /// What the payload is: `"world"` for engine world saves, or the game's own kind.
    pub kind: String,
    /// Payload schema version of `kind`.
    pub version: u32,
    /// Version of the engine that wrote the file (diagnostic only).
    pub engine: String,
    /// Fingerprint of the content the state belongs to, as 16 lowercase hex digits; empty when the
    /// state is not tied to particular content.
    pub content: String,
    /// Simulation tick at save time.
    pub tick: u64,
    /// Text for a load menu: "Quick save", "Level 3".
    pub label: String,
    /// The game or map name, for a load menu.
    pub game: String,
    /// Wall-clock time of the save in Unix milliseconds (0 when the clock is unavailable).
    pub saved_at_ms: u64,
}

/// Format a content fingerprint the way headers store it.
pub fn content_string(hash: u64) -> String {
    format!("{hash:016x}")
}

impl SaveHeader {
    /// A header for `kind` at payload `version`, stamped with the engine version and the current time.
    pub fn new(kind: &str, version: u32, label: &str) -> Self {
        Self {
            kind: kind.into(),
            version,
            engine: env!("CARGO_PKG_VERSION").into(),
            label: label.chars().take(MAX_LABEL).collect(),
            saved_at_ms: crate::runtime::storage::timestamp_ms(),
            ..Self::default()
        }
    }
    /// Tie the save to content (a map/game fingerprint).
    pub fn with_content(mut self, hash: u64) -> Self {
        self.content = content_string(hash);
        self
    }
    /// Record the simulation tick.
    pub fn with_tick(mut self, tick: u64) -> Self {
        self.tick = tick;
        self
    }
    /// Record the game or map name.
    pub fn with_game(mut self, game: &str) -> Self {
        self.game = game.chars().take(MAX_LABEL).collect();
        self
    }
    fn validate(&self) -> Result<(), SaveError> {
        let kind_ok = !self.kind.is_empty()
            && self.kind.len() <= 64
            && self
                .kind
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !kind_ok {
            return Err(SaveError::Corrupt("save kind is empty or malformed".into()));
        }
        if self.content.len() > 16 || !self.content.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(SaveError::Corrupt(
                "save content fingerprint is malformed".into(),
            ));
        }
        if self.label.chars().count() > MAX_LABEL || self.game.chars().count() > MAX_LABEL {
            return Err(SaveError::Corrupt("save label is too long".into()));
        }
        Ok(())
    }
}

/// Longest label or game name, in characters.
pub const MAX_LABEL: usize = 200;

/// A verified save frame: the header and a borrow of the payload bytes.
#[derive(Debug)]
pub struct Decoded<'a> {
    pub header: SaveHeader,
    pub payload: &'a [u8],
}

/// Frame a header and payload into save-file bytes.
pub fn encode(header: &SaveHeader, payload: &[u8]) -> Result<Vec<u8>, SaveError> {
    header.validate()?;
    let head = serde_json::to_vec(header).map_err(|e| SaveError::Corrupt(e.to_string()))?;
    if head.len() > MAX_HEADER_BYTES {
        return Err(SaveError::TooLarge {
            what: "header",
            limit: MAX_HEADER_BYTES as u64,
        });
    }
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(SaveError::TooLarge {
            what: "payload",
            limit: MAX_PAYLOAD_BYTES as u64,
        });
    }
    let mut out = Vec::with_capacity(PREFIX + head.len() + payload.len() + DIGEST);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&FRAME_VERSION.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(head.len() as u32).to_le_bytes());
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(&head);
    out.extend_from_slice(payload);
    let digest = sha256(&out);
    out.extend_from_slice(&digest);
    Ok(out)
}

/// The lengths a frame prefix declares, after every sanity check that needs no further bytes.
fn read_prefix(bytes: &[u8]) -> Result<(usize, usize), SaveError> {
    if bytes.len() < MAGIC.len() || bytes[..MAGIC.len()] != MAGIC {
        return Err(SaveError::NotASave);
    }
    if bytes.len() < PREFIX {
        return Err(SaveError::Truncated {
            expected: (PREFIX + DIGEST) as u64,
            found: bytes.len() as u64,
        });
    }
    let version = u16::from_le_bytes([bytes[8], bytes[9]]);
    if version == 0 {
        return Err(SaveError::Corrupt("frame format version 0".into()));
    }
    if version > FRAME_VERSION {
        return Err(SaveError::UnsupportedFormat {
            found: version,
            supported: FRAME_VERSION,
        });
    }
    if u16::from_le_bytes([bytes[10], bytes[11]]) != 0 {
        return Err(SaveError::Corrupt("unknown frame flags".into()));
    }
    let head = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as u64;
    let payload = u64::from_le_bytes(bytes[16..24].try_into().expect("eight bytes"));
    if head > MAX_HEADER_BYTES as u64 {
        return Err(SaveError::TooLarge {
            what: "header",
            limit: MAX_HEADER_BYTES as u64,
        });
    }
    if payload > MAX_PAYLOAD_BYTES as u64 {
        return Err(SaveError::TooLarge {
            what: "payload",
            limit: MAX_PAYLOAD_BYTES as u64,
        });
    }
    Ok((head as usize, payload as usize))
}

/// Verify and split a save file: prefix, lengths, checksum, then header. Never panics.
pub fn decode(bytes: &[u8]) -> Result<Decoded<'_>, SaveError> {
    let (head, payload) = read_prefix(bytes)?;
    let total = PREFIX + head + payload + DIGEST;
    if bytes.len() < total {
        return Err(SaveError::Truncated {
            expected: total as u64,
            found: bytes.len() as u64,
        });
    }
    if bytes.len() > total {
        return Err(SaveError::TrailingBytes);
    }
    let (body, trailer) = bytes.split_at(total - DIGEST);
    if sha256(body) != trailer {
        return Err(SaveError::Corrupt(
            "checksum mismatch (the file was damaged or edited)".into(),
        ));
    }
    let header: SaveHeader = serde_json::from_slice(&body[PREFIX..PREFIX + head])
        .map_err(|e| SaveError::Corrupt(format!("unreadable header: {e}")))?;
    header.validate()?;
    Ok(Decoded {
        header,
        payload: &body[PREFIX + head..],
    })
}

/// Read only the header of a save file without reading or verifying the payload: for menus that list
/// many saves. The result is *unverified*; [`decode`] or [`read_save`] verifies before any state is used.
pub fn peek_header(path: &Path) -> Result<SaveHeader, SaveError> {
    use std::io::Read;
    let mut file = fs::File::open(path).map_err(|e| io_error(path, e))?;
    let mut prefix = [0u8; PREFIX];
    file.read_exact(&mut prefix)
        .map_err(|_| SaveError::Truncated {
            expected: (PREFIX + DIGEST) as u64,
            found: 0,
        })?;
    let (head, _) = read_prefix(&prefix)?;
    let mut raw = vec![0u8; head];
    file.read_exact(&mut raw)
        .map_err(|_| SaveError::Truncated {
            expected: (PREFIX + head) as u64,
            found: 0,
        })?;
    let header: SaveHeader = serde_json::from_slice(&raw)
        .map_err(|e| SaveError::Corrupt(format!("unreadable header: {e}")))?;
    header.validate()?;
    Ok(header)
}

/// Where a loaded save came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// The named file itself.
    Primary,
    /// The file was missing or damaged; this is its last-good backup. The error says why the primary failed.
    Backup(SaveError),
}

/// A verified save read from disk.
#[derive(Clone, Debug)]
pub struct Loaded {
    pub header: SaveHeader,
    pub payload: Vec<u8>,
    pub source: Source,
}

/// The backup path for a save file (`quick.be2save` -> `quick.be2save.bak`).
pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".bak");
    path.with_file_name(name)
}

fn read_verified(path: &Path) -> Result<(SaveHeader, Vec<u8>), SaveError> {
    let bytes = fs::read(path).map_err(|e| io_error(path, e))?;
    let decoded = decode(&bytes)?;
    Ok((decoded.header, decoded.payload.to_vec()))
}

/// Read and verify a save; when the file is missing or damaged fall back to its backup. Never writes.
pub fn read_save(path: &Path) -> Result<Loaded, SaveError> {
    match read_verified(path) {
        Ok((header, payload)) => Ok(Loaded {
            header,
            payload,
            source: Source::Primary,
        }),
        Err(primary) => match read_verified(&backup_path(path)) {
            Ok((header, payload)) => Ok(Loaded {
                header,
                payload,
                source: Source::Backup(primary),
            }),
            Err(_) => Err(primary),
        },
    }
}

fn temp_path(target: &Path) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    target.with_file_name(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Rename with a few short retries: on Windows a virus scanner or indexer can briefly hold the target.
fn rename_retrying(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut last = None;
    for delay_ms in [0u64, 10, 30, 100, 300] {
        if delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => last = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last.expect("at least one attempt was made"))
}

/// Flush a directory's entries to disk after a rename (a no-op where the OS cannot).
fn sync_directory(dir: &Path) {
    #[cfg(unix)]
    if let Ok(handle) = fs::File::open(dir) {
        let _ = handle.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

/// Write `bytes` (a complete frame from [`encode`]) to `path` so that either the old file or the new
/// one survives a crash, never a mixture; keep the previous *valid* file as `<path>.bak`; read the result
/// back and verify it. On any failure the previous file is left in place and no temporary file remains.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), SaveError> {
    // A frame that does not verify must never be written over anything.
    let expected = decode(bytes)?.header;
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).map_err(|e| io_error(dir, e))?;
    let temp = temp_path(path);
    let cleanup = |error: SaveError| {
        let _ = fs::remove_file(&temp);
        error
    };
    {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| io_error(&temp, e))?;
        file.write_all(bytes)
            .map_err(|e| cleanup(io_error(&temp, e)))?;
        file.sync_all().map_err(|e| cleanup(io_error(&temp, e)))?;
    }
    // Keep the old file as the backup, but only if it is itself a valid save.
    let previous_valid = read_verified(path).is_ok();
    let backup = backup_path(path);
    if previous_valid {
        let staged = temp_path(&backup);
        if fs::copy(path, &staged).is_ok() {
            if rename_retrying(&staged, &backup).is_err() {
                let _ = fs::remove_file(&staged);
            }
        } else {
            let _ = fs::remove_file(&staged);
        }
    }
    rename_retrying(&temp, path).map_err(|e| cleanup(io_error(path, e)))?;
    sync_directory(dir);
    // Read it back: a disk that lies about a write is found now, not at load time.
    match fs::read(path)
        .map_err(|e| io_error(path, e))
        .and_then(|written| {
            let decoded = decode(&written)?;
            (decoded.header == expected && written == bytes)
                .then_some(())
                .ok_or_else(|| {
                    SaveError::Corrupt("the file read back differs from what was written".into())
                })
        }) {
        Ok(()) => Ok(()),
        Err(error) => {
            if previous_valid {
                let _ = fs::copy(&backup, path);
            }
            Err(error)
        }
    }
}

/// Serialise `state` as a save payload. Two things make this the only way a state should reach a file:
///
/// * floats are written widened to 64 bits (`serde_json`'s value tree does this), so every `f32` reads back
///   bit for bit; writing them as shortest 32-bit decimals and parsing through a 64-bit float can, very
///   rarely, land one ulp away;
/// * the state is read back and written again before anything is returned, so a state that cannot survive
///   its own serialisation (NaN or infinity, which JSON cannot hold, or a lossy custom `Serialize`) is an
///   error at save time, never a file that fails to load later.
pub fn payload_bytes<T: Serialize + DeserializeOwned>(state: &T) -> Result<Vec<u8>, SaveError> {
    let value = serde_json::to_value(state)
        .map_err(|e| SaveError::Invalid(format!("state is not serialisable: {e}")))?;
    let back: T = serde_json::from_value(value.clone()).map_err(|e| {
        SaveError::Invalid(format!(
            "state cannot be read back from what would be written (a NaN or infinite number?): {e}"
        ))
    })?;
    let again = serde_json::to_value(&back)
        .map_err(|e| SaveError::Invalid(format!("state is not serialisable: {e}")))?;
    if again != value {
        return Err(SaveError::Invalid(
            "state changes when it is written and read back".into(),
        ));
    }
    serde_json::to_vec(&value)
        .map_err(|e| SaveError::Invalid(format!("state is not serialisable: {e}")))
}

/// Serialise `state` as the payload of a `kind` save and frame it (see [`payload_bytes`]).
pub fn save_bytes<T: Serialize + DeserializeOwned>(
    header: &SaveHeader,
    state: &T,
) -> Result<Vec<u8>, SaveError> {
    encode(header, &payload_bytes(state)?)
}

/// One step of a payload migration chain: turns a payload of version `from` into version `from + 1`.
#[derive(Clone, Copy)]
pub struct Migration {
    /// The payload version this step upgrades.
    pub from: u32,
    /// The upgrade; an `Err` explains why this payload cannot be upgraded.
    pub step: fn(Value) -> Result<Value, String>,
}

/// Upgrade `payload` from version `from` to `to` through `steps` (which must contain every `from..to`
/// step). A missing step is an error, never a silent skip.
pub fn migrate(
    mut payload: Value,
    from: u32,
    to: u32,
    steps: &[Migration],
) -> Result<Value, SaveError> {
    for version in from..to {
        let step = steps.iter().find(|m| m.from == version).ok_or_else(|| {
            SaveError::Invalid(format!("no migration from save version {version}"))
        })?;
        payload = (step.step)(payload).map_err(|why| {
            SaveError::Invalid(format!("migrating save version {version}: {why}"))
        })?;
    }
    Ok(payload)
}

/// What a caller expects of a save it is about to load.
#[derive(Clone, Copy)]
pub struct Expect<'a> {
    /// The payload kind.
    pub kind: &'a str,
    /// The payload version this build writes and reads natively.
    pub version: u32,
    /// Migration steps for older versions.
    pub migrations: &'a [Migration],
    /// The content fingerprint the running game has. `Some(hash)` requires the save to carry exactly
    /// that fingerprint: a save for other content, or one with no fingerprint at all, is refused.
    /// `None` accepts any save of this kind (a save that is not tied to content).
    pub content: Option<u64>,
}

/// Verify a save file's bytes against `expect` and parse its payload as `T`. Refuses other kinds, newer
/// versions and other content; migrates older versions.
pub fn load_bytes<T: DeserializeOwned>(
    bytes: &[u8],
    expect: Expect<'_>,
) -> Result<(SaveHeader, T), SaveError> {
    let decoded = decode(bytes)?;
    let header = decoded.header;
    check_header(&header, expect)?;
    parse_payload(header, decoded.payload, expect)
}

/// The kind, version and content checks shared by every load path.
pub fn check_header(header: &SaveHeader, expect: Expect<'_>) -> Result<(), SaveError> {
    if header.kind != expect.kind {
        return Err(SaveError::WrongKind {
            expected: expect.kind.into(),
            found: header.kind.clone(),
        });
    }
    if header.version > expect.version {
        return Err(SaveError::NewerVersion {
            kind: header.kind.clone(),
            found: header.version,
            supported: expect.version,
        });
    }
    if let Some(hash) = expect.content {
        let want = content_string(hash);
        if header.content != want {
            let found = if header.content.is_empty() {
                "(none)".to_owned()
            } else {
                header.content.clone()
            };
            return Err(SaveError::WrongContent {
                expected: want,
                found,
            });
        }
    }
    Ok(())
}

/// Parse (and migrate) an already verified payload.
pub fn parse_payload<T: DeserializeOwned>(
    header: SaveHeader,
    payload: &[u8],
    expect: Expect<'_>,
) -> Result<(SaveHeader, T), SaveError> {
    let value: Value = serde_json::from_slice(payload)
        .map_err(|e| SaveError::Invalid(format!("payload is not valid JSON: {e}")))?;
    let value = migrate(value, header.version, expect.version, expect.migrations)?;
    let state = serde_json::from_value(value).map_err(|e| SaveError::Invalid(e.to_string()))?;
    Ok((header, state))
}

/// Describe a save file for a human or an agent without loading it into anything: the header, whether the bytes
/// verify (a damaged file falls back to its backup, and the result says so), and a summary of the state.
/// Engine world saves are summarised (tick, players, props, round); a game's own kinds list the top-level
/// fields of their state. `Err` when neither the file nor its backup verifies.
pub fn describe(path: &Path) -> Result<Value, SaveError> {
    let loaded = read_save(path)?;
    let header = &loaded.header;
    let payload: Result<Value, _> = serde_json::from_slice(&loaded.payload);
    let summary = match &payload {
        #[cfg(not(target_arch = "wasm32"))]
        Ok(_) if header.kind == world::KIND => {
            let expect = Expect {
                kind: world::KIND,
                version: world::VERSION,
                migrations: world::MIGRATIONS,
                content: None,
            };
            match parse_payload::<world::WorldState>(header.clone(), &loaded.payload, expect) {
                Ok((_, state)) => {
                    let game = state.game.as_ref().map(|g| &g.state);
                    serde_json::json!({
                        "tick": state.tick,
                        "players": state.players.iter().map(|p| p.id).collect::<Vec<_>>(),
                        "props": state.physics.as_ref().map_or(0, |p| p.props.len()),
                        "props_moving": state.physics.as_ref().map_or(0, |p| p.props.iter().filter(|q| !q.sleeping).count()),
                        "round": game.map(|g| g.round),
                        "completed": game.map(|g| g.completed),
                        "failed": game.map(|g| g.failed),
                        "valid": state.validate().is_ok(),
                    })
                }
                Err(error) => serde_json::json!({ "unreadable": error.to_string() }),
            }
        }
        // A game's own envelope is `{"hash": ..., "policy": ..., "state": {...}}`; a bare object lists
        // its own keys. The policy is what the game promised about resuming (`devkit::SavePolicy`).
        Ok(Value::Object(fields)) => {
            let state = fields
                .get("state")
                .and_then(Value::as_object)
                .unwrap_or(fields);
            serde_json::json!({
                "fields": state.keys().collect::<Vec<_>>(),
                "policy": fields.get("policy").cloned().unwrap_or(Value::Null),
            })
        }
        Ok(_) => serde_json::json!({ "fields": [] }),
        Err(error) => serde_json::json!({ "unreadable": error.to_string() }),
    };
    Ok(serde_json::json!({
        "file": path.display().to_string(),
        "source": match &loaded.source { Source::Primary => "primary".to_owned(), Source::Backup(why) => format!("backup ({why})") },
        "frame_format": FRAME_VERSION,
        "kind": header.kind,
        "version": header.version,
        "engine": header.engine,
        "label": header.label,
        "game": header.game,
        "tick": header.tick,
        "saved_at_ms": header.saved_at_ms,
        "content": header.content,
        "payload_bytes": loaded.payload.len(),
        "checksum": "verified",
        "summary": summary,
    }))
}

/// One slot's entry in a listing.
#[derive(Clone, Debug)]
pub struct SlotInfo {
    /// The slot name.
    pub slot: String,
    /// The header, or the reason the slot cannot be loaded.
    pub header: Result<SaveHeader, SaveError>,
    /// The file size in bytes.
    pub bytes: u64,
    /// Whether a last-good backup exists next to the slot.
    pub has_backup: bool,
}

/// Whether `name` is a usable slot name: `[a-z0-9_-]{1,48}`, not a Windows device name.
pub fn valid_slot(name: &str) -> bool {
    let plain = !name.is_empty()
        && name.len() <= 48
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-');
    let device = matches!(name, "con" | "prn" | "aux" | "nul")
        || (name.len() == 4
            && (name.starts_with("com") || name.starts_with("lpt"))
            && name.as_bytes()[3].is_ascii_digit());
    plain && !device
}

/// A directory of named save slots.
///
/// ```
/// use vesper3d::viewer::savestate::{SaveHeader, SaveSlots, QUICK_SLOT};
/// # let dir = std::env::temp_dir().join(format!("slots-doc-{}", std::process::id()));
/// let slots = SaveSlots::new(&dir);
/// let header = SaveHeader::new("demo", 1, "Quick save");
/// slots.save(QUICK_SLOT, &header, br#"{"level":3}"#).unwrap();
/// let loaded = slots.load(QUICK_SLOT).unwrap();
/// assert_eq!(loaded.payload, br#"{"level":3}"#);
/// assert!(slots.save("../escape", &header, b"{}").is_err(), "slot names cannot climb out of the directory");
/// # std::fs::remove_dir_all(dir).unwrap();
/// ```
#[derive(Clone, Debug)]
pub struct SaveSlots {
    dir: PathBuf,
}

/// Most slots a listing reports.
const MAX_LISTED: usize = 256;

impl SaveSlots {
    /// Slots stored in `dir` (created on the first save).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
    /// A `saves` directory next to the running executable (a packaged game keeps saves in `dist/saves`).
    pub fn beside_exe() -> Self {
        Self::new(crate::viewer::devkit::beside_exe("saves"))
    }
    /// The directory holding the slots.
    pub fn dir(&self) -> &Path {
        &self.dir
    }
    /// The file for `slot`.
    pub fn path(&self, slot: &str) -> Result<PathBuf, SaveError> {
        if !valid_slot(slot) {
            return Err(SaveError::BadSlot(slot.into()));
        }
        Ok(self.dir.join(format!("{slot}.{EXTENSION}")))
    }
    /// Save a framed state into `slot` atomically (see [`write_atomic`]).
    pub fn save(&self, slot: &str, header: &SaveHeader, payload: &[u8]) -> Result<(), SaveError> {
        write_atomic(&self.path(slot)?, &encode(header, payload)?)
    }
    /// Write already framed bytes into `slot`.
    pub fn save_framed(&self, slot: &str, bytes: &[u8]) -> Result<(), SaveError> {
        write_atomic(&self.path(slot)?, bytes)
    }
    /// Load `slot`, falling back to its backup when the file is damaged.
    pub fn load(&self, slot: &str) -> Result<Loaded, SaveError> {
        read_save(&self.path(slot)?)
    }
    /// Whether the slot has a file (valid or not).
    /// Open what a player named after `--load`: the path of a save file if such a file exists, otherwise a slot
    /// in this directory. Either way a damaged file falls back to its backup, and says so in the [`Source`].
    pub fn open(&self, target: &str) -> Result<Loaded, SaveError> {
        let path = Path::new(target);
        if valid_slot(target) && !path.is_file() {
            self.load(target)
        } else {
            read_save(path)
        }
    }
    pub fn exists(&self, slot: &str) -> bool {
        self.path(slot).is_ok_and(|p| p.is_file())
    }
    /// Delete the slot and its backup. Deleting a missing slot is not an error.
    pub fn delete(&self, slot: &str) -> Result<(), SaveError> {
        let path = self.path(slot)?;
        for file in [backup_path(&path), path] {
            match fs::remove_file(&file) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io_error(&file, e)),
            }
        }
        Ok(())
    }
    /// List the slots, newest first. Every slot's file is fully verified, so a damaged one is reported
    /// (with its reason) instead of being offered for loading. Stray and temporary files are ignored.
    pub fn list(&self) -> Result<Vec<SlotInfo>, SaveError> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io_error(&self.dir, e)),
        };
        let mut found = Vec::new();
        for entry in entries.flatten().take(MAX_LISTED * 4) {
            let path = entry.path();
            let Some(slot) = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_suffix(&format!(".{EXTENSION}")))
                .filter(|s| valid_slot(s))
            else {
                continue;
            };
            let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let header = read_verified(&path).map(|(h, _)| h);
            found.push(SlotInfo {
                slot: slot.to_owned(),
                header,
                bytes,
                has_backup: backup_path(&path).is_file(),
            });
            if found.len() >= MAX_LISTED {
                break;
            }
        }
        found.sort_by(|a, b| {
            let time = |i: &SlotInfo| i.header.as_ref().map_or(0, |h| h.saved_at_ms);
            time(b).cmp(&time(a)).then_with(|| a.slot.cmp(&b.slot))
        });
        Ok(found)
    }
    /// Rotating autosaves: shift `base`, `base-2`, ... `base-<keep>` up by one (dropping the oldest) and
    /// write the new save as `base`. A failure part-way leaves every remaining file valid.
    pub fn save_ring(
        &self,
        base: &str,
        keep: usize,
        header: &SaveHeader,
        payload: &[u8],
    ) -> Result<(), SaveError> {
        self.save_ring_framed(base, keep, &encode(header, payload)?)
    }
    /// [`Self::save_ring`] for a save that is already framed (what `HeadlessWorld::save_bytes` returns).
    pub fn save_ring_framed(
        &self,
        base: &str,
        keep: usize,
        framed: &[u8],
    ) -> Result<(), SaveError> {
        decode(framed)?;
        let keep = keep.clamp(1, 32);
        let name = |i: usize| {
            if i == 1 {
                base.to_owned()
            } else {
                format!("{base}-{i}")
            }
        };
        self.path(&name(1))?;
        self.path(&name(keep))?;
        for i in (1..keep).rev() {
            let (from, to) = (self.path(&name(i))?, self.path(&name(i + 1))?);
            if from.is_file() {
                // Copy, not rename: the source keeps its own backup chain and a crash loses nothing.
                let moved = fs::read(&from)
                    .map_err(|e| io_error(&from, e))
                    .and_then(|b| write_atomic(&to, &b));
                if let Err(e) = moved {
                    // An unreadable old autosave is dropped from the ring; it must not block a new save.
                    if matches!(e, SaveError::Io { .. }) {
                        return Err(e);
                    }
                }
            }
        }
        write_atomic(&self.path(&name(1))?, framed)
    }
}

#[cfg(test)]
mod tests;
