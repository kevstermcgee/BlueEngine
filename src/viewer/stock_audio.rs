//! Stock GameDocument audio pipeline: checked bundles, confirmed-state cues, adaptive music and settings.
//! Configuration/observation remain headless; playback never evaluates gameplay rules.
use super::{
    devkit::audio_project::AudioBundle,
    game::{GameDocument, GameState, MAX_COUNTER},
};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn full() -> f32 {
    1.
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StockAudio {
    /// Child directory containing bank.json, relative to the game document.
    pub bundle: String,
    #[serde(default)]
    pub cues: Vec<Cue>,
    #[serde(default)]
    pub music: BTreeMap<String, MusicLayer>,
    #[serde(default)]
    pub play_when_finished: bool,
    /// Established by GameDocument::load; not serialized or part of content identity.
    #[serde(skip)]
    pub(crate) root: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cue {
    pub cue: String,
    pub on: CueEvent,
    #[serde(default = "full")]
    pub volume: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CueEvent {
    CounterChanged {
        counter: String,
    },
    /// Crossing from above to at/below; no repeating alarm while below the threshold.
    CounterAtOrBelow {
        counter: String,
        value: i32,
    },
    Completed,
    Failed,
    Restarted,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MusicLayer {
    #[serde(default = "full")]
    pub level: f32,
    /// Optional linear mapping: from -> zero, to -> level, clamped outside that range.
    #[serde(default)]
    pub counter: Option<CounterMix>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CounterMix {
    pub name: String,
    pub from: i32,
    pub to: i32,
}

fn relative(s: &str) -> bool {
    !s.is_empty()
        && !s.contains(['\\', ':'])
        && !s.starts_with('/')
        && s.split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}
fn name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 48
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn level(v: f32) -> bool {
    v.is_finite() && (0. ..=1.).contains(&v)
}

impl StockAudio {
    pub fn validate(&self, counters: &BTreeMap<String, i32>) -> Result<()> {
        if !relative(&self.bundle)
            || self.bundle.len() > 200
            || self.cues.len() > 64
            || self.music.len() > 16
        {
            return Err(
                "Stock audio: bundle must be a child path; at most 64 cues and 16 music layers"
                    .into(),
            );
        }
        for cue in &self.cues {
            if !name(&cue.cue) || !level(cue.volume) {
                return Err("Stock audio: invalid cue name or volume (0..1)".into());
            }
            let counter = match &cue.on {
                CueEvent::CounterChanged { counter } => Some(counter),
                CueEvent::CounterAtOrBelow { counter, value } => {
                    if !(-MAX_COUNTER..=MAX_COUNTER).contains(value) {
                        return Err("Stock audio threshold outside counter limits".into());
                    }
                    Some(counter)
                }
                _ => None,
            };
            if counter.is_some_and(|c| !counters.contains_key(c)) {
                return Err("Stock audio cue references an unknown counter".into());
            }
        }
        for (n, layer) in &self.music {
            if !name(n) || !level(layer.level) {
                return Err("Stock audio: invalid layer name or level (0..1)".into());
            }
            if layer.counter.as_ref().is_some_and(|c| {
                !counters.contains_key(&c.name)
                    || c.from == c.to
                    || [c.from, c.to]
                        .iter()
                        .any(|v| !(-MAX_COUNTER..=MAX_COUNTER).contains(v))
            }) {
                return Err("Stock music counter needs a declared name and distinct from/to within counter limits".into());
            }
        }
        Ok(())
    }

    /// Verify names before submitting any playback. Asset verification remains AudioBank's job.
    pub fn validate_bank(&self, bank: &AudioBundle) -> Result<()> {
        for cue in &self.cues {
            if !bank.effects.contains_key(&cue.cue) {
                return Err(format!("Stock audio: missing cue {}", cue.cue).into());
            }
        }
        for name in self.music.keys() {
            if !bank.music.contains_key(name) {
                return Err(format!("Stock audio: missing music layer {name}").into());
            }
        }
        Ok(())
    }

    /// Confine assets including symlinks. Constructed games supply their content root explicitly.
    pub fn bundle_path(&self, content_root: Option<&Path>) -> Result<PathBuf> {
        if !relative(&self.bundle) {
            return Err("Stock audio bundle must be a child path".into());
        }
        let root = content_root
            .or(self.root.as_deref())
            .ok_or("Constructed game audio requires GameOptions::audio_root")?
            .canonicalize()?;
        let bundle = root.join(&self.bundle).canonicalize()?;
        if !bundle.starts_with(&root) {
            return Err("Stock audio bundle escapes game directory".into());
        }
        Ok(bundle)
    }
}

/// Cursor over observed authoritative states. Baseline/load does not replay historical cues.
pub struct AudioCursor {
    config: StockAudio,
    counters: BTreeMap<String, usize>,
    previous: GameState,
}
impl AudioCursor {
    pub fn new(config: &StockAudio, document: &GameDocument, state: &GameState) -> Result<Self> {
        config.validate(&document.counters)?;
        if state.counters.len() != document.counters.len() {
            return Err("Stock audio baseline counter count differs from document".into());
        }
        Ok(Self {
            config: config.clone(),
            counters: document
                .counters
                .keys()
                .enumerate()
                .map(|(i, n)| (n.clone(), i))
                .collect(),
            previous: state.clone(),
        })
    }
    pub fn rebase(&mut self, state: &GameState) {
        self.previous = state.clone();
    }
    /// Invoke after each local fixed step, or each observed online state. A snapshot is not an event log.
    pub fn observe(&mut self, state: &GameState) -> Vec<Cue> {
        let old = &self.previous;
        let restart = old.round != state.round;
        let mut cues = Vec::new();
        for cue in &self.config.cues {
            let value = |s: &GameState, n: &String| s.counters.get(self.counters[n]).copied();
            let fired = match &cue.on {
                CueEvent::Restarted => restart,
                _ if restart => false,
                CueEvent::Completed => state.completed && !old.completed,
                CueEvent::Failed => state.failed && !old.failed,
                CueEvent::CounterChanged { counter } => {
                    value(old, counter) != value(state, counter)
                }
                CueEvent::CounterAtOrBelow {
                    counter,
                    value: threshold,
                } => {
                    value(old, counter).is_some_and(|v| v > *threshold)
                        && value(state, counter).is_some_and(|v| v <= *threshold)
                }
            };
            if fired {
                cues.push(cue.clone());
            }
        }
        self.rebase(state);
        cues
    }
    pub fn music(&self, state: &GameState, playing: bool) -> BTreeMap<String, f32> {
        self.config
            .music
            .iter()
            .map(|(name, layer)| {
                let factor = layer.counter.as_ref().map_or(1., |c| {
                    let value = state.counters[self.counters[&c.name]];
                    ((f64::from(value) - f64::from(c.from)) / (f64::from(c.to) - f64::from(c.from)))
                        .clamp(0., 1.) as f32
                });
                (
                    name.clone(),
                    if playing && (!state.finished() || self.config.play_when_finished) {
                        layer.level * factor
                    } else {
                        0.
                    },
                )
            })
            .collect()
    }
}

#[cfg(feature = "client")]
pub(crate) struct StockSound {
    pub cursor: AudioCursor,
    bank: Option<super::kit::AudioBank>,
    pub settings: super::devkit::Settings,
    settings_path: PathBuf,
    pub has_music: bool,
}

#[cfg(feature = "client")]
impl StockSound {
    pub async fn load(
        document: &GameDocument,
        state: &GameState,
        root: Option<&Path>,
        settings_path: Option<&Path>,
        muted: bool,
    ) -> Result<Option<Self>> {
        let Some(config) = document
            .presentation
            .as_ref()
            .and_then(|p| p.audio.as_ref())
        else {
            return Ok(None);
        };
        let settings_path = settings_path.map_or_else(
            || super::devkit::beside_exe("settings.json"),
            Path::to_path_buf,
        );
        let settings = super::devkit::Settings::load(&settings_path);
        let bank = if muted {
            None
        } else {
            let path = config.bundle_path(root)?;
            config.validate_bank(&AudioBundle::load(&path)?)?;
            let mut bank = super::kit::AudioBank::load(
                path,
                false,
                settings.sfx_level(),
                settings.music_level(),
            )
            .await?;
            let start = std::time::Instant::now();
            while !bank.sounds.ready() {
                bank.sounds.poll().await;
                if bank.sounds.state() == super::kit::AudioState::Failed {
                    return Err(format!("Stock audio failed: {:?}", bank.sounds.errors()).into());
                }
                if start.elapsed().as_secs() >= 30 {
                    return Err("Stock audio loading timed out".into());
                }
                macroquad::prelude::clear_background(macroquad::prelude::BLACK);
                super::game_text::draw_text(
                    "Loading checked audio...",
                    24.,
                    48.,
                    24.,
                    macroquad::prelude::WHITE,
                );
                macroquad::prelude::next_frame().await;
            }
            Some(bank)
        };
        Ok(Some(Self {
            cursor: AudioCursor::new(config, document, state)?,
            bank,
            settings,
            settings_path,
            has_music: !config.music.is_empty(),
        }))
    }
    pub fn update(
        &mut self,
        state: &GameState,
        playing: bool,
        seconds: f32,
        cues: &[Cue],
        capture: bool,
    ) -> Result<Option<serde_json::Value>> {
        let levels = self.cursor.music(state, playing);
        if let Some(bank) = &mut self.bank {
            bank.sounds.sfx_volume = self.settings.sfx_level();
            bank.sounds.music_volume = self.settings.music_level();
            for cue in cues {
                bank.play(&cue.cue, cue.volume)?;
            }
            let mix: Vec<_> = levels
                .iter()
                .map(|(name, level)| (name.as_str(), *level))
                .collect();
            bank.music(seconds, &mix)?;
        }
        Ok(capture.then(|| serde_json::json!({"state": if self.bank.is_some() { "ready" } else { "muted" }, "cues":cues.iter().map(|c| &c.cue).collect::<Vec<_>>(), "submitted":self.bank.is_some(), "sfx_level":self.settings.sfx_level(), "music_level":self.settings.music_level(), "layers":levels, "audibility_verified":false})))
    }
    pub fn menu(
        &mut self,
        shell: &mut super::game_client::GameShell,
        title: &str,
        controls: &[&str],
        online: bool,
    ) -> bool {
        if self.bank.is_none() {
            return if online {
                shell.menu(title, controls)
            } else {
                shell.local_menu(title, controls)
            };
        }
        let outcome = shell.menu_with_audio_settings(
            title,
            controls,
            super::game_client::AudioMenu {
                music_on: self.settings.music_on,
                sfx_on: self.settings.sfx_on,
                has_music: self.has_music,
            },
            online,
        );
        if outcome.toggle_music {
            self.settings.toggle_music();
        }
        if outcome.toggle_sfx {
            self.settings.toggle_sfx();
        }
        if (outcome.toggle_music || outcome.toggle_sfx) && !self.settings.store(&self.settings_path)
        {
            eprintln!(
                "Stock audio: could not save settings to {}",
                self.settings_path.display()
            );
        }
        outcome.quit
    }
}
#[cfg(feature = "client")]
impl Drop for StockSound {
    fn drop(&mut self) {
        if let Some(bank) = &mut self.bank {
            bank.sounds.stop_music();
        }
    }
}
