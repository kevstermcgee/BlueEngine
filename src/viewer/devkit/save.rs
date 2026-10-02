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

/// The player's Downloads folder (`%USERPROFILE%\Downloads` on Windows, `$HOME/Downloads` elsewhere),
/// or `None` when the home/profile environment variable is unset. A custom XDG `user-dirs.dirs` target
/// on Linux is not consulted; this is the plain default location on every platform.
pub fn downloads_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    downloads_from(std::env::var_os(var))
}

fn downloads_from(home: Option<std::ffi::OsString>) -> Option<PathBuf> {
    home.map(|home| PathBuf::from(home).join("Downloads"))
}

/// `s` with every character that is not a letter, digit, space, hyphen or underscore removed, trimmed,
/// and `"download"` substituted for an empty result: a title is safe to use in a filename on every
/// platform (Windows additionally forbids `:` `/` `\` `*` `?` `"` `<` `>` `|`, all excluded here too).
pub fn sanitize_filename(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "download".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// `dir/stem.ext`, or `dir/stem (2).ext`, `dir/stem (3).ext`, ... for the first name that does not
/// already exist, so a repeated save never overwrites an earlier one (the same convention a browser's
/// downloads use). Does not create `dir` or the file; `n` is capped at 1000 to guarantee termination.
pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    (2..1000)
        .map(|n| dir.join(format!("{stem} ({n}).{ext}")))
        .find(|path| !path.exists())
        .unwrap_or(first)
}

/// How much shadow a game draws; the player picks it in Esc > Settings and it is remembered.
///
/// `Simple` is the default: soft contact blobs under moving things, no extra render pass. `Full` adds one
/// directional shadow map (see `kit::Shadows`). Variants compare in cost order, so `quality >= Simple`
/// means "blobs or better". Stored as the lowercase word (`"off"`, `"simple"`, `"full"`); an unknown word
/// reads as the default instead of discarding the rest of the settings file.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(from = "String", into = "String")]
pub enum ShadowQuality {
    /// No shadows at all (flat lighting, cheapest).
    Off,
    /// Contact blobs under actors; no extra pass.
    #[default]
    Simple,
    /// One directional shadow map plus the blobs' job done by real shadows.
    Full,
}

impl ShadowQuality {
    /// Every tier, cheapest first.
    pub const ALL: [ShadowQuality; 3] = [Self::Off, Self::Simple, Self::Full];
    /// The lowercase word used in the settings file and the `--shadows` flag.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Simple => "simple",
            Self::Full => "full",
        }
    }
    /// The player-facing label for the Settings screen.
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Simple => "Simple",
            Self::Full => "Full",
        }
    }
    /// Parse `off`, `simple` or `full` (any case, surrounding space ignored); `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "0" => Some(Self::Off),
            "simple" | "blob" | "blobs" | "1" => Some(Self::Simple),
            "full" | "on" | "2" => Some(Self::Full),
            _ => None,
        }
    }
    /// The next tier in Off, Simple, Full order, wrapping: what the Settings selector does on a press.
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Simple,
            Self::Simple => Self::Full,
            Self::Full => Self::Off,
        }
    }
    /// The `--shadows off|simple|full` flag from the command line (`None` when absent). A bad value is an
    /// error naming the choices, so a typo does not silently run at the default.
    pub fn from_flag(args: &[String]) -> Result<Option<Self>, String> {
        match super::flag_value(args, "--shadows") {
            None if super::has_flag(args, "--shadows") => {
                Err("--shadows needs a value: off, simple or full".into())
            }
            None => Ok(None),
            Some(word) => Self::parse(word)
                .map(Some)
                .ok_or_else(|| format!("--shadows takes off, simple or full, not `{word}`")),
        }
    }
}

impl From<String> for ShadowQuality {
    fn from(text: String) -> Self {
        Self::parse(&text).unwrap_or_default()
    }
}

impl From<ShadowQuality> for String {
    fn from(quality: ShadowQuality) -> Self {
        quality.as_str().to_owned()
    }
}

/// The settings almost every action game exposes. Extend it by wrapping it in your own save struct.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Mouse look multiplier, 0.2-4.
    pub sensitivity: f32,
    /// Music volume, 0-1, used when `music_on`.
    pub music: f32,
    /// Sound-effect volume, 0-1, used when `sfx_on`.
    pub sfx: f32,
    /// Whether music plays at all; toggling this remembers `music` rather than zeroing it.
    pub music_on: bool,
    /// Whether sound effects play at all; toggling this remembers `sfx` rather than zeroing it.
    pub sfx_on: bool,
    /// Start in fullscreen.
    pub fullscreen: bool,
    /// The server address the player last connected to (as typed), so the join screen can offer it again.
    /// Absent from settings files written before this field existed; it then reads as `None`.
    pub last_server: Option<String>,
    /// Shadow tier for games that opt in (`kit::Shadows`). Absent from older files: reads as `Simple`.
    pub shadow_quality: ShadowQuality,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sensitivity: 1.,
            music: 0.6,
            sfx: 0.9,
            music_on: true,
            sfx_on: true,
            fullscreen: false,
            last_server: None,
            shadow_quality: ShadowQuality::default(),
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
    /// The volume to actually play music at: `music` when `music_on`, else 0. Hand this straight to
    /// `SoundBank::start`/`music_volume`; a toggle survives a relaunch because `music` itself is untouched.
    pub fn music_level(&self) -> f32 {
        if self.music_on {
            self.music
        } else {
            0.
        }
    }
    /// The volume to actually play sound effects at: `sfx` when `sfx_on`, else 0.
    pub fn sfx_level(&self) -> f32 {
        if self.sfx_on {
            self.sfx
        } else {
            0.
        }
    }
    /// Flip whether music plays. Does not persist; call [`Settings::store`] afterwards.
    pub fn toggle_music(&mut self) -> bool {
        self.music_on = !self.music_on;
        self.music_on
    }
    /// Flip whether sound effects play. Does not persist; call [`Settings::store`] afterwards.
    pub fn toggle_sfx(&mut self) -> bool {
        self.sfx_on = !self.sfx_on;
        self.sfx_on
    }
    /// Move to the next shadow tier (Off, Simple, Full, wrapping). Does not persist; call
    /// [`Settings::store`] afterwards.
    pub fn cycle_shadow_quality(&mut self) -> ShadowQuality {
        self.shadow_quality = self.shadow_quality.next();
        self.shadow_quality
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
    fn toggles_flip_and_remember_the_underlying_volume() {
        let mut s = Settings::default();
        assert_eq!((s.music_level(), s.sfx_level()), (s.music, s.sfx));
        assert!(!s.toggle_music());
        assert_eq!(s.music_level(), 0., "off plays silent");
        assert_eq!(
            s.music, 0.6,
            "the remembered volume is untouched by toggling"
        );
        assert!(s.toggle_music());
        assert_eq!(
            s.music_level(),
            0.6,
            "toggling back on restores the remembered volume"
        );
        assert!(!s.toggle_sfx());
        assert_eq!(s.sfx_level(), 0.);
        assert_eq!(s.music_level(), 0.6, "the two toggles are independent");
    }

    #[test]
    fn last_server_is_optional_in_old_files_and_survives_a_round_trip() {
        let path = temp("last-server.json");
        std::fs::write(
            &path,
            r#"{"sensitivity":1.5,"music":0.3,"sfx":0.9,"music_on":false,"sfx_on":true,"fullscreen":true}"#,
        )
        .unwrap();
        let old = Settings::load(&path);
        assert_eq!(old.last_server, None, "a file from before the field loads");
        assert_eq!(
            (old.sensitivity, old.music_on, old.fullscreen),
            (1.5, false, true)
        );
        let mut s = old;
        s.last_server = Some("play.example.com:27015".into());
        assert!(s.store(&path));
        assert_eq!(
            Settings::load(&path).last_server.as_deref(),
            Some("play.example.com:27015")
        );
        std::fs::write(&path, r#"{"last_server":null}"#).unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_settings_file_from_before_toggles_existed_still_loads_as_on() {
        let path = temp("pre-toggle.json");
        std::fs::write(
            &path,
            r#"{"sensitivity":1.0,"music":0.6,"sfx":0.9,"fullscreen":false}"#,
        )
        .unwrap();
        let s = Settings::load(&path);
        assert!(
            s.music_on && s.sfx_on,
            "missing keys default to on, not off"
        );
        assert_eq!((s.music_level(), s.sfx_level()), (0.6, 0.9));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn shadow_quality_defaults_to_simple_and_old_files_still_load() {
        let path = temp("pre-shadows.json");
        std::fs::write(
            &path,
            r#"{"sensitivity":1.5,"music":0.3,"sfx":0.9,"music_on":false,"sfx_on":true,"fullscreen":true}"#,
        )
        .unwrap();
        let old = Settings::load(&path);
        assert_eq!(old.shadow_quality, ShadowQuality::Simple);
        assert_eq!(
            (old.sensitivity, old.music_on),
            (1.5, false),
            "the other values survive"
        );
        let mut s = old;
        assert_eq!(s.cycle_shadow_quality(), ShadowQuality::Full);
        assert!(s.store(&path));
        assert!(std::fs::read_to_string(&path).unwrap().contains("\"full\""));
        assert_eq!(Settings::load(&path).shadow_quality, ShadowQuality::Full);
        // A hand-edited or future value must not throw away the rest of the file.
        std::fs::write(&path, r#"{"sensitivity":2.0,"shadow_quality":"ultra"}"#).unwrap();
        let odd = Settings::load(&path);
        assert_eq!(
            (odd.shadow_quality, odd.sensitivity),
            (ShadowQuality::Simple, 2.0)
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn shadow_quality_cycles_parses_and_orders_by_cost() {
        let mut q = ShadowQuality::Off;
        let seen: Vec<_> = (0..4)
            .map(|_| {
                q = q.next();
                q
            })
            .collect();
        assert_eq!(
            seen,
            [
                ShadowQuality::Simple,
                ShadowQuality::Full,
                ShadowQuality::Off,
                ShadowQuality::Simple
            ]
        );
        assert!(
            ShadowQuality::Off < ShadowQuality::Simple
                && ShadowQuality::Simple < ShadowQuality::Full
        );
        for tier in ShadowQuality::ALL {
            assert_eq!(ShadowQuality::parse(tier.as_str()), Some(tier));
            assert_eq!(
                ShadowQuality::parse(&tier.label().to_uppercase()),
                Some(tier)
            );
        }
        assert_eq!(ShadowQuality::parse("ultra"), None);
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(ShadowQuality::from_flag(&args(&["game"])), Ok(None));
        assert_eq!(
            ShadowQuality::from_flag(&args(&["game", "--shadows", "full"])),
            Ok(Some(ShadowQuality::Full))
        );
        assert!(ShadowQuality::from_flag(&args(&["game", "--shadows", "lots"])).is_err());
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
    fn sanitize_filename_keeps_safe_characters_and_never_returns_empty() {
        assert_eq!(sanitize_filename("Spooky Kart"), "Spooky Kart");
        assert_eq!(sanitize_filename("Foo/Bar:Baz*?\"<>|"), "FooBarBaz");
        assert_eq!(sanitize_filename("  padded  "), "padded");
        assert_eq!(sanitize_filename(":::"), "download");
        assert_eq!(sanitize_filename(""), "download");
    }

    #[test]
    fn unique_path_avoids_existing_files_and_numbers_from_two() {
        let dir = temp("unique").with_extension("");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(unique_path(&dir, "track", "wav"), dir.join("track.wav"));
        std::fs::write(dir.join("track.wav"), b"a").unwrap();
        assert_eq!(unique_path(&dir, "track", "wav"), dir.join("track (2).wav"));
        std::fs::write(dir.join("track (2).wav"), b"b").unwrap();
        assert_eq!(unique_path(&dir, "track", "wav"), dir.join("track (3).wav"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn downloads_dir_joins_home_and_is_none_without_it() {
        assert_eq!(
            downloads_from(Some("/home/alice".into())),
            Some(PathBuf::from("/home/alice/Downloads"))
        );
        assert_eq!(downloads_from(None), None);
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
