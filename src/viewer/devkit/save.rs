//! Player settings and records that survive restarts: one small JSON file, atomic, never fatal.
//!
//! A game must never refuse to start because its save file is missing, half-written or hand-edited,
//! and must never lose a save because the process died mid-write. [`load_or_default`] therefore
//! swallows every read/parse failure into the default value, and [`store_atomic`] writes a temporary
//! file and renames it over the old one (a crash leaves either the old or the new file, never a torn one).
//! Both report failure by value; nothing panics.
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Largest save file read, in bytes; anything bigger is treated as corrupt.
const MAX_SAVE_BYTES: u64 = 4_000_000;

/// Read `path` as JSON, or return `T::default()` when it is missing, unreadable, too large or invalid.
/// Give `T` `#[serde(default)]` so a file written by an older version still loads.
pub fn load_or_default<T: DeserializeOwned + Default>(path: &Path) -> T {
    let Ok(meta) = std::fs::metadata(path) else {
        return T::default();
    };
    if meta.len() > MAX_SAVE_BYTES {
        return T::default();
    }
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Write `value` as pretty JSON atomically. Returns false (and leaves any old file untouched) when it
/// cannot be written.
pub fn store_atomic<T: Serialize>(path: &Path, value: &T) -> bool {
    let Ok(text) = serde_json::to_string_pretty(value) else {
        return false;
    };
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, text).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    if std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    true
}

/// `file` next to the running executable (so a packaged game keeps its save beside `dist/game.exe`),
/// or in the working directory when the executable path is unknown.
pub fn beside_exe(file: &str) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(file)))
        .unwrap_or_else(|| PathBuf::from(file))
}

/// The settings almost every action game exposes. Extend it by wrapping it in your own save struct.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Mouse look multiplier, 0.2-4.
    pub sensitivity: f32,
    /// Music volume, 0-1.
    pub music: f32,
    /// Sound-effect volume, 0-1.
    pub sfx: f32,
    /// Start in fullscreen.
    pub fullscreen: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sensitivity: 1.,
            music: 0.6,
            sfx: 0.9,
            fullscreen: false,
        }
    }
}

impl Settings {
    /// Clamp every value into its valid range; NaN and infinity fall back to the default.
    pub fn sanitized(mut self) -> Self {
        let d = Self::default();
        let fix = |v: f32, lo: f32, hi: f32, fallback: f32| {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                fallback
            }
        };
        self.sensitivity = fix(self.sensitivity, 0.2, 4., d.sensitivity);
        self.music = fix(self.music, 0., 1., d.music);
        self.sfx = fix(self.sfx, 0., 1., d.sfx);
        self
    }
    /// Load and sanitise (missing or corrupt file: defaults).
    pub fn load(path: &Path) -> Self {
        load_or_default::<Self>(path).sanitized()
    }
    /// Store atomically; false when it could not be written.
    pub fn store(&self, path: &Path) -> bool {
        store_atomic(path, self)
    }
}

/// Best scores per game mode (difficulty, level set, ...), keyed by any name the game likes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Records {
    /// Highest score per mode.
    pub best: BTreeMap<String, u64>,
    /// Finished runs in total.
    pub runs: u32,
}

impl Records {
    /// Fold a finished run in. Returns true when `score` beats the mode's previous best (a tie does not).
    pub fn record(&mut self, mode: &str, score: u64) -> bool {
        self.runs = self.runs.saturating_add(1);
        let best = self.best.entry(mode.to_owned()).or_insert(0);
        if score > *best {
            *best = score;
            true
        } else {
            false
        }
    }
    /// Best score for `mode` (0 when never played).
    pub fn best(&self, mode: &str) -> u64 {
        self.best.get(mode).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("devkit_save_{}_{name}", std::process::id()))
    }

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    struct Wrapper {
        settings: Settings,
        records: Records,
        note: String,
    }

    #[test]
    fn round_trips_through_a_file_and_replaces_the_old_one() {
        let path = temp("rt.json");
        let mut save = Wrapper::default();
        assert!(save.records.record("hard", 12_345));
        save.settings.sensitivity = 1.7;
        save.note = "hello".into();
        assert!(store_atomic(&path, &save));
        assert_eq!(load_or_default::<Wrapper>(&path), save);
        save.note = "again".into();
        assert!(store_atomic(&path, &save), "an existing save is replaced");
        assert_eq!(load_or_default::<Wrapper>(&path).note, "again");
        assert!(
            !path.with_extension("tmp").exists(),
            "no temporary file is left behind"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_corrupt_or_oversized_files_give_defaults() {
        assert_eq!(
            load_or_default::<Wrapper>(&temp("nope.json")),
            Wrapper::default()
        );
        let path = temp("bad.json");
        for junk in ["{ not json", "", "[1,2,3]", "null"] {
            std::fs::write(&path, junk).unwrap();
            assert_eq!(
                load_or_default::<Wrapper>(&path),
                Wrapper::default(),
                "{junk:?}"
            );
        }
        std::fs::write(&path, vec![b' '; MAX_SAVE_BYTES as usize + 1]).unwrap();
        assert_eq!(load_or_default::<Wrapper>(&path), Wrapper::default());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn partial_files_fill_in_defaults_and_bad_settings_are_clamped() {
        let path = temp("partial.json");
        std::fs::write(&path, r#"{"records":{"best":{"normal":500}}}"#).unwrap();
        let save = load_or_default::<Wrapper>(&path);
        assert_eq!(save.records.best("normal"), 500);
        assert_eq!(save.settings, Settings::default());
        std::fs::write(
            &path,
            r#"{"sensitivity": 900.0, "music": -3.0, "sfx": 0.5}"#,
        )
        .unwrap();
        let s = Settings::load(&path);
        assert_eq!((s.sensitivity, s.music, s.sfx), (4., 0., 0.5));
        let bad = Settings {
            sensitivity: f32::NAN,
            music: f32::INFINITY,
            ..Settings::default()
        }
        .sanitized();
        assert_eq!((bad.sensitivity, bad.music), (1., 0.6));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn records_only_beat_when_higher_and_are_kept_per_mode() {
        let mut r = Records::default();
        assert!(r.record("normal", 100));
        assert!(!r.record("normal", 100), "a tie is not a new best");
        assert!(!r.record("normal", 50));
        assert!(r.record("easy", 10), "each mode has its own best");
        assert_eq!(
            (r.best("normal"), r.best("easy"), r.best("hard"), r.runs),
            (100, 10, 0, 4)
        );
    }

    #[test]
    fn an_unwritable_path_does_not_panic_and_is_reported() {
        assert!(!store_atomic(
            Path::new("/definitely/not/a/real/dir/save.json"),
            &Settings::default()
        ));
        assert!(beside_exe("x.json").ends_with("x.json"));
    }
}
