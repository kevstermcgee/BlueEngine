//! Playing generated sound: a bank of effects with variants, and looping music stems that fade with
//! the action.
//!
//! Rendering audio is CPU work (a full effect set is a fraction of a second, a music loop more). The
//! bank therefore takes a `render` closure that runs on a worker thread and returns WAV files;
//! [`SoundBank::poll`] hands them to the audio backend a few per frame. Generation stays off the game
//! loop; decoder cost still depends on asset size and backend. The backend is initialised first because its
//! first use costs tens of milliseconds. With no audio device everything degrades to silence.
//!
//! Sounds are addressed by index (`Preset as usize`, your own enum's discriminant) and variant; the
//! bank plays variants round-robin so repeated sounds do not fatigue.
use crate::viewer::audio_backend::{
    load_sound_from_bytes, play_sound, set_sound_volume, stop_sound, PlaySoundParams, Sound,
};
use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

/// WAV files produced by a render closure.
#[derive(Default)]
pub struct Rendered {
    /// `sfx[sound][variant]` = WAV bytes.
    pub sfx: Vec<Vec<Vec<u8>>>,
    /// Looping music stems of equal length, started together.
    pub stems: Vec<Vec<u8>>,
}

enum Slot {
    Sfx(usize, Vec<u8>),
    Stem(Vec<u8>),
}

/// The pure half of loading: unpacks a [`Rendered`] into a queue and releases it in small batches.
struct Loader {
    rx: Option<Receiver<Result<Rendered, String>>>,
    queue: VecDeque<Slot>,
    sounds: usize,
    errors: Vec<String>,
    rendered: bool,
    failed: bool,
    resource_errors: usize,
}

impl Loader {
    fn new(rx: Option<Receiver<Result<Rendered, String>>>) -> Self {
        Self {
            rx,
            queue: VecDeque::new(),
            sounds: 0,
            errors: Vec::new(),
            rendered: false,
            failed: false,
            resource_errors: 0,
        }
    }
    /// Take the worker's result if it has arrived, then release up to `budget` queued sounds.
    fn pump(&mut self, budget: usize) -> Vec<Slot> {
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(Ok(rendered)) => {
                    self.rx = None;
                    self.rendered = true;
                    self.sounds = rendered.sfx.len();
                    for (i, variants) in rendered.sfx.into_iter().enumerate() {
                        self.queue
                            .extend(variants.into_iter().map(|bytes| Slot::Sfx(i, bytes)));
                    }
                    self.queue
                        .extend(rendered.stems.into_iter().map(Slot::Stem));
                }
                Ok(Err(error)) => {
                    self.rx = None;
                    self.failed = true;
                    self.resource_errors += 1;
                    self.errors.push(error);
                }
                Err(TryRecvError::Disconnected) => {
                    self.rx = None;
                    self.failed = true;
                    self.resource_errors += 1;
                    self.errors
                        .push("audio worker exited without a result".into());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        (0..budget).map_while(|_| self.queue.pop_front()).collect()
    }
    fn finished(&self) -> bool {
        self.rx.is_none() && self.queue.is_empty()
    }

    fn state(&self, muted: bool, has_assets: bool) -> AudioState {
        if !self.errors.is_empty() {
            AudioState::Failed
        } else if muted {
            AudioState::Muted
        } else if !self.finished() {
            AudioState::Loading
        } else if has_assets {
            AudioState::Ready
        } else {
            AudioState::Empty
        }
    }
}

/// Observable audio evidence. Loaded counts describe validated resources/handles; playback counters
/// record submissions while the backend was Ready. Worker failure rejects verification. None proves audibility.
#[derive(Clone, Debug, serde::Serialize)]
pub struct AudioStatus {
    pub muted: bool,
    pub rendered: bool,
    pub pending: bool,
    pub worker_failed: bool,
    pub loaded_effects: usize,
    pub loaded_stems: usize,
    pub load_failures: usize,
    pub effect_plays: u64,
    pub music_playing: bool,
}

impl AudioStatus {
    /// Require the intended bank AND playback submissions. Use after an `--audible` scripted run,
    /// passing the number of effect variants and music stems the render closure promised. Silent,
    /// incomplete and partly loaded banks fail; this still cannot verify a physical output device.
    pub fn verify_playback(&self, effects: usize, stems: usize) -> Result<(), String> {
        if self.muted
            || !self.rendered
            || self.pending
            || self.worker_failed
            || self.load_failures > 0
            || self.loaded_effects != effects
            || self.loaded_stems != stems
            || (effects > 0 && self.effect_plays == 0)
            || (stems > 0 && !self.music_playing)
        {
            return Err(format!("audio verification incomplete: {self:?}; expected {effects} effect variants and {stems} music stems; use --audible, exercise a cue, allow loading to finish, and inspect audio errors"));
        }
        Ok(())
    }
}

/// Next variant of a round-robin over `count` variants.
fn next_variant(counter: &mut usize, count: usize) -> usize {
    let v = *counter % count.max(1);
    *counter = counter.wrapping_add(1);
    v
}

/// A 2-sample silent 16-bit mono WAV, used to wake the audio backend.
fn silent_wav() -> Vec<u8> {
    let data = [0u8; 4];
    let mut out = Vec::with_capacity(44 + data.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&44_100u32.to_le_bytes());
    out.extend_from_slice(&88_200u32.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

/// Sound effects plus music stems. Create it once after the window exists.
pub struct SoundBank {
    loader: Loader,
    sounds: Vec<Vec<Sound>>,
    next: Vec<usize>,
    stems: Vec<Sound>,
    /// Effect volume multiplier, 0-1.
    pub sfx_volume: f32,
    /// Music volume multiplier, 0-1.
    pub music_volume: f32,
    /// Silences everything (a `--mute` flag, the M key).
    pub muted: bool,
    effect_plays: u64,
    music_playing: bool,
    stem_now: Vec<f32>,
    stem_target: Vec<f32>,
}

impl SoundBank {
    /// Start `render` on a worker thread (skipped when `muted`) and wake the audio backend. Call
    /// [`SoundBank::poll`] every frame afterwards.
    pub async fn start(
        muted: bool,
        sfx_volume: f32,
        music_volume: f32,
        render: impl FnOnce() -> Rendered + Send + 'static,
    ) -> Self {
        Self::start_checked(muted, sfx_volume, music_volume, move || Ok(render())).await
    }

    /// Fallible worker for imported or data-authored audio. Inspect `state()` and `errors()`;
    /// a failed load never becomes ready, and no partial bank can be played after failure.
    pub async fn start_checked(
        muted: bool,
        sfx_volume: f32,
        music_volume: f32,
        render: impl FnOnce() -> Result<Rendered, String> + Send + 'static,
    ) -> Self {
        let rx = if muted {
            None
        } else {
            // First use of the audio backend initialises a device; do it now, not mid-game.
            let _ = load_sound_from_bytes(&silent_wav()).await;
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let _ = tx.send(render());
            });
            Some(rx)
        };
        Self {
            loader: Loader::new(rx),
            sounds: Vec::new(),
            next: Vec::new(),
            stems: Vec::new(),
            sfx_volume: sfx_volume.clamp(0., 1.),
            music_volume: music_volume.clamp(0., 1.),
            muted,
            effect_plays: 0,
            music_playing: false,
            stem_now: Vec::new(),
            stem_target: Vec::new(),
        }
    }

    /// Finish loading once the worker is done: bounded decoder submissions per call. Call every
    /// frame; expensive generation/file verification remains on the worker, not in this method.
    pub async fn poll(&mut self) {
        if let crate::viewer::audio_backend::BackendState::Unavailable(error) = self.backend_state()
        {
            if !self.loader.errors.contains(&error) {
                self.loader.errors.push(error);
            }
        }
        for slot in self.loader.pump(5) {
            match slot {
                Slot::Sfx(i, bytes) => {
                    if self.sounds.len() <= i {
                        self.sounds.resize_with(i + 1, Vec::new);
                        self.next.resize(i + 1, 0);
                    }
                    match load_sound_from_bytes(&bytes).await {
                        Ok(sound) => self.sounds[i].push(sound),
                        Err(error) => {
                            self.loader.resource_errors += 1;
                            self.loader.errors.push(format!("effect {i}: {error}"));
                        }
                    }
                }
                Slot::Stem(bytes) => match load_sound_from_bytes(&bytes).await {
                    Ok(sound) => {
                        self.stems.push(sound);
                        self.stem_now.push(0.);
                        self.stem_target.push(0.);
                    }
                    Err(error) => {
                        self.loader.resource_errors += 1;
                        self.loader.errors.push(format!("music stem: {error}"));
                    }
                },
            }
        }
    }

    /// True once every sound has been handed to the backend (immediately when muted).
    pub fn ready(&self) -> bool {
        matches!(self.state(), AudioState::Ready | AudioState::Muted)
    }

    /// Loading/ready concerns assets and decoder submission; the backend cannot confirm audibility.
    /// Hardware-worker health; Ready is device initialization, not a listening test.
    pub fn backend_state(&self) -> crate::viewer::audio_backend::BackendState {
        crate::viewer::audio_backend::state()
    }
    /// Resource/render/decode failures, independent of unavailable playback hardware.
    pub fn resource_failed(&self) -> bool {
        self.loader.failed || self.loader.resource_errors > 0
    }
    pub fn status(&self) -> AudioStatus {
        AudioStatus {
            muted: self.muted,
            rendered: self.loader.rendered,
            pending: !self.loader.finished(),
            worker_failed: self.loader.failed
                || matches!(
                    self.backend_state(),
                    crate::viewer::audio_backend::BackendState::Unavailable(_)
                ),
            loaded_effects: self.sounds.iter().map(Vec::len).sum(),
            loaded_stems: self.stems.len(),
            load_failures: self.loader.resource_errors,
            effect_plays: self.effect_plays,
            music_playing: self.music_playing,
        }
    }

    pub fn state(&self) -> AudioState {
        self.loader.state(
            self.muted,
            self.sounds.iter().any(|v| !v.is_empty()) || !self.stems.is_empty(),
        )
    }

    pub fn errors(&self) -> &[String] {
        &self.loader.errors
    }

    /// Stop loops explicitly; restarting resets all layer levels to silence.
    pub fn stop_music(&mut self) {
        for stem in &self.stems {
            stop_sound(stem);
        }
        self.music_playing = false;
        self.stem_now.fill(0.);
        self.stem_target.fill(0.);
    }

    fn volume(&self, volume: f32) -> f32 {
        (volume * self.sfx_volume).clamp(0., 1.)
    }

    /// Play the next variant of `sound` at `volume` (0-1, scaled by [`SoundBank::sfx_volume`]).
    /// Unknown or not-yet-loaded sounds are ignored.
    pub fn play(&mut self, sound: usize, volume: f32) {
        if self.muted
            || !self.loader.errors.is_empty()
            || !matches!(
                self.backend_state(),
                crate::viewer::audio_backend::BackendState::Ready
            )
        {
            return;
        }
        let Some(variants) = self.sounds.get(sound).filter(|v| !v.is_empty()) else {
            return;
        };
        let v = next_variant(&mut self.next[sound], variants.len());
        let params = PlaySoundParams {
            looped: false,
            volume: self.volume(volume),
        };
        play_sound(&variants[v], params);
        self.effect_plays += 1;
    }

    /// Play a specific variant (a combo pitch ladder). An out-of-range variant plays the last one.
    pub fn play_variant(&mut self, sound: usize, variant: usize, volume: f32) {
        if self.muted
            || !self.loader.errors.is_empty()
            || !matches!(
                self.backend_state(),
                crate::viewer::audio_backend::BackendState::Ready
            )
        {
            return;
        }
        if let Some(s) = self
            .sounds
            .get(sound)
            .and_then(|v| v.get(variant.min(v.len().saturating_sub(1))))
        {
            play_sound(
                s,
                PlaySoundParams {
                    looped: false,
                    volume: self.volume(volume),
                },
            );
            self.effect_plays += 1;
        }
    }

    /// Start every music stem together, silent until [`SoundBank::update_music`] raises them.
    pub fn start_music(&mut self) {
        if self.muted
            || !matches!(
                self.backend_state(),
                crate::viewer::audio_backend::BackendState::Ready
            )
            || !self.loader.errors.is_empty()
            || self.music_playing
            || self.stems.is_empty()
            || !self.loader.finished()
        {
            return;
        }
        for stem in &self.stems {
            play_sound(
                stem,
                PlaySoundParams {
                    looped: true,
                    volume: 0.,
                },
            );
        }
        self.music_playing = true;
    }

    /// Set each stem's target level (0-1) and glide towards it; call every frame with real seconds.
    pub fn update_music(&mut self, dt: f32, targets: &[f32]) {
        for (target, wanted) in self.stem_target.iter_mut().zip(targets) {
            *target = wanted.clamp(0., 1.);
        }
        if !self.music_playing {
            return;
        }
        if self.muted
            || !self.loader.errors.is_empty()
            || !matches!(
                self.backend_state(),
                crate::viewer::audio_backend::BackendState::Ready
            )
        {
            for stem in &self.stems {
                set_sound_volume(stem, 0.);
            }
            return;
        }
        let k = 1. - (-dt.clamp(0., 0.1) * 2.5).exp();
        for i in 0..self.stems.len() {
            self.stem_now[i] += (self.stem_target[i] - self.stem_now[i]) * k;
            set_sound_volume(
                &self.stems[i],
                (self.stem_now[i] * self.music_volume).clamp(0., 1.),
            );
        }
    }
}

/// Asset-loading state. Ready is not confirmation from a physical audio device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioState {
    Loading,
    Ready,
    Muted,
    Empty,
    Failed,
}

/// Named, pre-rendered bank. Load after window creation; poll each frame. Editing JSON and rendering
/// a replacement bundle requires no Rust rebuild. Names are resolved once from checked metadata.
pub struct AudioBank {
    pub sounds: SoundBank,
    effects: std::collections::BTreeMap<String, usize>,
    layers: std::collections::BTreeMap<String, usize>,
    targets: Vec<f32>,
}
impl AudioBank {
    pub async fn load(
        root: impl Into<std::path::PathBuf>,
        muted: bool,
        sfx_volume: f32,
        music_volume: f32,
    ) -> Result<Self, String> {
        for volume in [sfx_volume, music_volume] {
            if !volume.is_finite() || !(0. ..=1.).contains(&volume) {
                return Err("audio volumes must be finite 0..1".into());
            }
        }
        let root = root.into();
        let bank = crate::viewer::devkit::audio_project::AudioBundle::load(&root)?;
        let effects = bank
            .effects
            .keys()
            .enumerate()
            .map(|(i, n)| (n.clone(), i))
            .collect();
        let layers = bank
            .music
            .keys()
            .enumerate()
            .map(|(i, n)| (n.clone(), i))
            .collect();
        let targets = vec![0.; bank.music.len()];
        let sounds = SoundBank::start_checked(muted, sfx_volume, music_volume, move || {
            let audio = bank.read_audio(&root)?;
            Ok(Rendered {
                sfx: audio.effects,
                stems: audio.music,
            })
        })
        .await;
        Ok(Self {
            sounds,
            effects,
            layers,
            targets,
        })
    }

    /// Unknown names and not-ready banks are errors; success means submitted, not audibly played.
    pub fn play(&mut self, name: &str, volume: f32) -> Result<(), String> {
        let index = *self
            .effects
            .get(name)
            .ok_or_else(|| format!("unknown audio cue {name:?}"))?;
        if !volume.is_finite() || !(0. ..=1.).contains(&volume) {
            return Err("cue volume must be finite 0..1".into());
        }
        if !self.sounds.ready() {
            return Err(format!(
                "audio bank {:?}: {:?}",
                self.sounds.state(),
                self.sounds.errors()
            ));
        }
        self.sounds.play(index, volume);
        Ok(())
    }

    /// Complete named target mix: omitted layers fade to zero. Validate before changing any targets.
    /// The legacy backend starts loops individually; this is not a sample-clock synchronisation API.
    pub fn music(&mut self, dt: f32, levels: &[(&str, f32)]) -> Result<(), String> {
        if !dt.is_finite() || dt < 0. {
            return Err("audio dt must be finite and nonnegative".into());
        }
        for (name, level) in levels {
            if !self.layers.contains_key(*name) {
                return Err(format!("unknown music layer {name:?}"));
            }
            if !level.is_finite() || !(0. ..=1.).contains(level) {
                return Err("music levels must be finite 0..1".into());
            }
        }
        if !self.sounds.ready() {
            return Err(format!(
                "audio bank {:?}: {:?}",
                self.sounds.state(),
                self.sounds.errors()
            ));
        }
        self.targets.fill(0.);
        for (name, level) in levels {
            self.targets[self.layers[*name]] = *level;
        }
        self.sounds.start_music();
        self.sounds.update_music(dt, &self.targets);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audio_evidence_requires_complete_loading_and_actual_playback_submissions() {
        let good = AudioStatus {
            muted: false,
            rendered: true,
            pending: false,
            worker_failed: false,
            loaded_effects: 3,
            loaded_stems: 1,
            load_failures: 0,
            effect_plays: 1,
            music_playing: true,
        };
        assert!(good.verify_playback(3, 1).is_ok());
        for bad in [
            AudioStatus {
                muted: true,
                ..good.clone()
            },
            AudioStatus {
                pending: true,
                ..good.clone()
            },
            AudioStatus {
                worker_failed: true,
                ..good.clone()
            },
            AudioStatus {
                load_failures: 1,
                ..good.clone()
            },
            AudioStatus {
                loaded_effects: 2,
                ..good.clone()
            },
            AudioStatus {
                loaded_stems: 0,
                ..good.clone()
            },
            AudioStatus {
                effect_plays: 0,
                ..good.clone()
            },
            AudioStatus {
                music_playing: false,
                ..good.clone()
            },
        ] {
            assert!(bad.verify_playback(3, 1).unwrap_err().contains("--audible"));
        }
        let music_only = AudioStatus {
            loaded_effects: 0,
            effect_plays: 0,
            ..good
        };
        assert!(music_only.verify_playback(0, 1).is_ok());
    }

    #[test]
    fn failed_worker_finishes_with_explicit_failure_evidence() {
        let (tx, rx) = channel();
        let mut loader = Loader::new(Some(rx));
        drop(tx);
        assert!(loader.pump(5).is_empty());
        assert!(loader.finished() && loader.failed && !loader.rendered);
    }

    #[test]
    fn music_only_render_is_finished_and_observable() {
        let (tx, rx) = channel();
        let mut loader = Loader::new(Some(rx));
        tx.send(Ok(rendered(0, 0, 1))).unwrap();
        assert!(matches!(loader.pump(5).as_slice(), [Slot::Stem(_)]));
        assert!(loader.finished() && loader.rendered && !loader.failed);
    }

    fn rendered(sfx: usize, variants: usize, stems: usize) -> Rendered {
        Rendered {
            sfx: (0..sfx)
                .map(|_| (0..variants).map(|_| vec![1, 2, 3]).collect())
                .collect(),
            stems: (0..stems).map(|_| vec![9]).collect(),
        }
    }

    #[test]
    fn the_loader_releases_a_few_sounds_per_call_in_order() {
        let (tx, rx) = channel();
        let mut loader = Loader::new(Some(rx));
        assert!(
            loader.pump(5).is_empty(),
            "nothing before the worker finishes"
        );
        assert!(!loader.finished());
        tx.send(Ok(rendered(3, 2, 3))).unwrap();
        let first = loader.pump(5);
        assert_eq!(first.len(), 5);
        assert!(matches!(first[0], Slot::Sfx(0, _)) && matches!(first[2], Slot::Sfx(1, _)));
        assert!(!loader.finished());
        let mut total = first.len();
        while !loader.finished() {
            total += loader.pump(5).len();
        }
        assert_eq!(
            total,
            3 * 2 + 3,
            "every variant and stem is delivered exactly once"
        );
        assert_eq!(loader.sounds, 3);
        assert!(loader.pump(5).is_empty());
    }

    #[test]
    fn a_dead_worker_leaves_a_silent_finished_loader() {
        let (tx, rx) = channel::<Result<Rendered, String>>();
        let mut loader = Loader::new(Some(rx));
        drop(tx);
        assert!(loader.pump(5).is_empty());
        assert!(loader.finished(), "no result will ever arrive");
        assert_eq!(loader.sounds, 0);
        assert_eq!(loader.errors, ["audio worker exited without a result"]);
        assert!(Loader::new(None).finished(), "muted: nothing to load");
    }

    #[test]
    fn failures_are_terminal_and_music_only_is_ready_after_loading() {
        let (tx, rx) = channel();
        let mut loader = Loader::new(Some(rx));
        assert_eq!(loader.state(false, false), AudioState::Loading);
        tx.send(Err("missing bank file".into())).unwrap();
        assert!(loader.pump(5).is_empty());
        assert_eq!(
            loader.state(false, true),
            AudioState::Failed,
            "partial assets never hide failure"
        );
        assert_eq!(loader.errors, ["missing bank file"]);
        assert_eq!(loader.state(true, true), AudioState::Failed);
        let (tx, rx) = channel();
        let mut music = Loader::new(Some(rx));
        tx.send(Ok(rendered(0, 0, 2))).unwrap();
        assert_eq!(music.pump(1).len(), 1);
        assert_eq!(music.state(false, true), AudioState::Loading);
        assert_eq!(music.pump(1).len(), 1);
        assert_eq!(music.state(false, true), AudioState::Ready);
        assert_eq!(Loader::new(None).state(false, false), AudioState::Empty);
        assert_eq!(Loader::new(None).state(true, false), AudioState::Muted);
    }

    #[test]
    fn variants_cycle_and_tolerate_empty_lists() {
        let mut counter = 0;
        let seen: Vec<usize> = (0..7).map(|_| next_variant(&mut counter, 3)).collect();
        assert_eq!(seen, [0, 1, 2, 0, 1, 2, 0]);
        assert_eq!(next_variant(&mut 5, 0), 0);
    }

    #[test]
    fn the_warm_up_sound_is_a_valid_wav() {
        let wav = silent_wav();
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(
            u32::from_le_bytes(wav[4..8].try_into().unwrap()) as usize,
            wav.len() - 8
        );
        assert_eq!(
            u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize,
            wav.len() - 44
        );
    }
}
