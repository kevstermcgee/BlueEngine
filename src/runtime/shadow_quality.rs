//! Shared shadow quality; no filesystem or device dependencies.
use serde::{Deserialize, Serialize};
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
        match crate::runtime::playback::flag_value(args, "--shadows") {
            None if crate::runtime::playback::has_flag(args, "--shadows") => {
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
