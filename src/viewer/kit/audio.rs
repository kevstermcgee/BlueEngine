//! Playing generated sound: a bank of effects with variants, and looping music stems that fade with
//! the action.
//!
//! Rendering audio is CPU work (a full effect set is a fraction of a second, a music loop more). The
//! bank therefore takes a `render` closure that runs on a worker thread and returns WAV files;
//! [`SoundBank::poll`] hands them to the audio backend a few per frame, so the window opens at once and
//! never hitches. The backend itself is initialised first, while the game is still loading, because its
//! first use costs tens of milliseconds. With no audio device everything degrades to silence.
//!
//! Sounds are addressed by index (`Preset as usize`, your own enum's discriminant) and variant; the
//! bank plays variants round-robin so repeated sounds do not fatigue.
use macroquad::audio::{
    load_sound_from_bytes, play_sound, set_sound_volume, PlaySoundParams, Sound,
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
    rx: Option<Receiver<Rendered>>,
    queue: VecDeque<Slot>,
    sounds: usize,
    rendered: bool,
    failed: bool,
}

impl Loader {
    fn new(rx: Option<Receiver<Rendered>>) -> Self {
        Self {
            rx,
            queue: VecDeque::new(),
            sounds: 0,
            rendered: false,
            failed: false,
        }
    }
    /// Take the worker's result if it has arrived, then release up to `budget` queued sounds.
    fn pump(&mut self, budget: usize) -> Vec<Slot> {
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(rendered) => {
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
                Err(TryRecvError::Disconnected) => {
                    self.rx = None;
                    self.failed = true;
                    eprintln!("audio render worker failed: inspect the render closure's panic; audio is unavailable");
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        (0..budget).map_while(|_| self.queue.pop_front()).collect()
    }
    fn finished(&self) -> bool {
        self.rx.is_none() && self.queue.is_empty()
    }
}

/// Observable audio evidence. Loaded/submitted means the backend accepted it, not that a listener
/// heard it: a disconnected or null output device still needs a listening check.
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
    music_playing: bool,
    stem_now: Vec<f32>,
    stem_target: Vec<f32>,
    load_failures: usize,
    effect_plays: u64,
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
            music_playing: false,
            stem_now: Vec::new(),
            stem_target: Vec::new(),
            load_failures: 0,
            effect_plays: 0,
        }
    }

    /// Finish loading once the worker is done: hands a few sounds to the backend per call, so it is
    /// cheap to call every frame and never causes a hitch.
    pub async fn poll(&mut self) {
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
                            self.load_failures += 1;
                            eprintln!("audio effect {i} failed to load: {error}; check the generated WAV bytes");
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
                        self.load_failures += 1;
                        eprintln!("audio music stem failed to load: {error}; check the generated WAV bytes");
                    }
                },
            }
        }
    }

    /// True once rendering/loading finished (immediately when muted). Check [`Self::status`] for
    /// success: readiness alone does not prove any sounds loaded. Music-only banks are supported.
    pub fn ready(&self) -> bool {
        self.muted || self.loader.finished()
    }

    /// Inspect loading and playback submissions, including failures that would otherwise sound silent.
    pub fn status(&self) -> AudioStatus {
        AudioStatus {
            muted: self.muted,
            rendered: self.loader.rendered,
            pending: !self.loader.finished(),
            worker_failed: self.loader.failed,
            loaded_effects: self.sounds.iter().map(Vec::len).sum(),
            loaded_stems: self.stems.len(),
            load_failures: self.load_failures,
            effect_plays: self.effect_plays,
            music_playing: self.music_playing,
        }
    }

    fn volume(&self, volume: f32) -> f32 {
        (volume * self.sfx_volume).clamp(0., 1.)
    }

    /// Play the next variant of `sound` at `volume` (0-1, scaled by [`SoundBank::sfx_volume`]).
    /// Unknown or not-yet-loaded sounds are ignored.
    pub fn play(&mut self, sound: usize, volume: f32) {
        if self.muted {
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
        if self.muted {
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
        if self.muted || self.music_playing || self.stems.is_empty() || !self.loader.finished() {
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
        tx.send(rendered(0, 0, 1)).unwrap();
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
        tx.send(rendered(3, 2, 3)).unwrap();
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
        let (tx, rx) = channel::<Rendered>();
        let mut loader = Loader::new(Some(rx));
        drop(tx);
        assert!(loader.pump(5).is_empty());
        assert!(loader.finished(), "no result will ever arrive");
        assert_eq!(loader.sounds, 0);
        assert!(Loader::new(None).finished(), "muted: nothing to load");
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
