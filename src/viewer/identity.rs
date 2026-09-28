//! A shipped game's identity: what it is called, what it says about itself and what it looks like.
//!
//! One small file, `assets/identity.json`, is the single source of truth for four surfaces of a game:
//! the window title, the desktop shortcut's name and tooltip, the executable's version resource
//! (Task Manager shows its `FileDescription`) and the packaging manifest. `be2-tools new-game` writes
//! it, the generated `build.rs` embeds it, `scripts/ship.py` reads it, and the game itself can read it
//! at runtime with [`Identity::parse`], so the window title can never drift from the shortcut's name.
//!
//! ```
//! use vesper3d::viewer::identity::Identity;
//! let id = Identity::parse(r#"{"title":"Bouncer","tagline":"Nobody gets in.","controls":"WASD move"}"#).unwrap();
//! assert_eq!(id.title, "Bouncer");
//! assert!(Identity::parse(r#"{"title":"Game","tagline":"x","controls":"y"}"#).is_err(), "placeholder names are rejected");
//! ```
use crate::Result;
use serde::{Deserialize, Serialize};

/// Titles a shipped game may not use: they say nothing about the game, so two games would collide.
pub const PLACEHOLDER_TITLES: [&str; 7] = [
    "play",
    "game",
    "blueengine game",
    "blueengine",
    "untitled",
    "my game",
    "new game",
];

/// Longest title, in characters.
pub const MAX_TITLE: usize = 60;
/// Longest controls line, in characters.
pub const MAX_CONTROLS: usize = 200;

/// The contents of `assets/identity.json`. Unknown keys are ignored so newer tools can add fields.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Identity {
    /// Display name: window title, desktop shortcut name, executable `FileDescription`/`ProductName`.
    pub title: String,
    /// One sentence about the game, shown in the shortcut's tooltip.
    pub tagline: String,
    /// The controls in one line, shown in the tooltip after the tagline.
    pub controls: String,
    /// Executable file stem when it differs from the Cargo package name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exe: Option<String>,
    /// Extra files or directories copied next to the executable when packaging, keeping their paths.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub package: Vec<String>,
    /// Arguments for the packaged smoke run; `{dir}` is replaced by a new empty directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smoke_args: Option<Vec<String>>,
    /// Engine commit the game was built against (12 hex characters).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine_revision: Option<String>,
}

impl Identity {
    /// Parse and validate an `identity.json` document.
    pub fn parse(json: &str) -> Result<Self> {
        let identity: Self =
            serde_json::from_str(json).map_err(|e| format!("identity.json: {e}"))?;
        identity.validate()?;
        Ok(identity)
    }

    /// Check the rules `scripts/ship.py verify` enforces: a real title, a tagline and controls.
    pub fn validate(&self) -> Result<()> {
        let title = self.title.trim();
        if title.is_empty() || title.chars().count() > MAX_TITLE {
            return Err(format!("identity title must be 1-{MAX_TITLE} characters").into());
        }
        if title
            .chars()
            .any(|c| c.is_control() || r#"\/:*?"<>|"#.contains(c))
        {
            return Err(
                "identity title may not contain control characters or any of \\ / : * ? \" < > |"
                    .into(),
            );
        }
        if PLACEHOLDER_TITLES.contains(&title.to_lowercase().as_str()) {
            return Err(format!("identity title '{title}' is a placeholder: name the game").into());
        }
        if self.tagline.trim().is_empty() {
            return Err("identity tagline is empty: say what the game is in one sentence".into());
        }
        if self.controls.trim().is_empty() || self.controls.chars().count() > MAX_CONTROLS {
            return Err(format!("identity controls must be 1-{MAX_CONTROLS} characters").into());
        }
        Ok(())
    }

    /// A starter identity for a Cargo-style name (`my-game` becomes "My Game"), with a tagline and
    /// controls line to edit. The result is deliberately generic prose: a shipped game must replace it.
    pub fn starter(name: &str, tagline: &str, controls: &str) -> Self {
        Self {
            title: title_from_name(name),
            tagline: tagline.into(),
            controls: controls.into(),
            ..Self::default()
        }
    }

    /// Pretty JSON, ending in a newline.
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("identity is always serialisable");
        text.push('\n');
        text
    }

    /// The shortcut tooltip: tagline, a space, controls; at most 255 characters (the `.lnk` limit),
    /// cut with `...` when longer.
    pub fn tooltip(&self) -> String {
        let text = format!("{} {}", self.tagline.trim(), self.controls.trim());
        if text.chars().count() <= 255 {
            text
        } else {
            let cut: String = text.chars().take(252).collect();
            format!("{}...", cut.trim_end())
        }
    }
}

/// `my-game` / `my_game` become "My Game": separators turn into spaces and every word is capitalised.
/// Words that are already capitalised or contain capitals inside (`BlueDM`) are left alone.
pub fn title_from_name(name: &str) -> String {
    name.split(['-', '_', ' '])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) if word.chars().skip(1).all(|c| !c.is_uppercase()) => {
                    first.to_uppercase().chain(chars).collect::<String>()
                }
                _ => word.to_owned(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> Identity {
        Identity::starter(
            "bouncer",
            "Nobody gets in, everybody gets thrown out.",
            "WASD move, mouse look",
        )
    }

    #[test]
    fn titles_are_derived_from_cargo_names() {
        assert_eq!(title_from_name("my-game"), "My Game");
        assert_eq!(title_from_name("neon_relay"), "Neon Relay");
        assert_eq!(title_from_name("BlueDM"), "BlueDM");
        assert_eq!(title_from_name("bouncer"), "Bouncer");
        assert_eq!(title_from_name("--"), "");
        assert_eq!(title_from_name("2048-clone"), "2048 Clone");
    }

    #[test]
    fn a_real_identity_round_trips_and_validates() {
        let id = valid();
        assert_eq!(id.title, "Bouncer");
        id.validate().unwrap();
        assert_eq!(Identity::parse(&id.to_json()).unwrap(), id);
        assert!(id.to_json().ends_with("}\n"));
        assert!(
            !id.to_json().contains("smoke_args"),
            "unset options are not written"
        );
    }

    #[test]
    fn placeholder_and_malformed_identities_are_rejected_with_a_reason() {
        for title in [
            "",
            "  ",
            "Game",
            "PLAY",
            "My Game",
            "BlueEngine",
            "a/b",
            "a:b",
            "x\ty",
            &"x".repeat(61),
        ] {
            let id = Identity {
                title: title.into(),
                ..valid()
            };
            assert!(id.validate().is_err(), "{title:?} must be rejected");
        }
        assert!(Identity {
            tagline: " ".into(),
            ..valid()
        }
        .validate()
        .unwrap_err()
        .to_string()
        .contains("tagline"));
        assert!(Identity {
            controls: String::new(),
            ..valid()
        }
        .validate()
        .is_err());
        assert!(Identity {
            controls: "x".repeat(201),
            ..valid()
        }
        .validate()
        .is_err());
        assert!(Identity::parse("{ not json").is_err());
        assert!(Identity::parse("{}").is_err());
    }

    #[test]
    fn unknown_keys_are_ignored_and_options_survive() {
        let id = Identity::parse(
            r#"{"title":"Ω Quest","tagline":"t","controls":"c","package":["game.json","maps"],
                "smoke_args":["--capture","{dir}"],"engine_revision":"5978a214d99f","future":1}"#,
        )
        .unwrap();
        assert_eq!(id.package, ["game.json", "maps"]);
        assert_eq!(id.smoke_args.as_deref().map(<[String]>::len), Some(2));
        assert_eq!(id.engine_revision.as_deref(), Some("5978a214d99f"));
        assert_eq!(Identity::parse(&id.to_json()).unwrap(), id);
    }

    #[test]
    fn tooltips_join_tagline_and_controls_and_respect_the_shortcut_limit() {
        assert_eq!(
            valid().tooltip(),
            "Nobody gets in, everybody gets thrown out. WASD move, mouse look"
        );
        let long = Identity {
            tagline: "t".repeat(300),
            ..valid()
        };
        let tip = long.tooltip();
        assert_eq!(tip.chars().count(), 255);
        assert!(tip.ends_with("..."));
    }
}
