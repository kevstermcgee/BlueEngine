//! Named score with bounded course preparation; active play never decodes long WAVs.
use embertrail::cues::Cue;
use embertrail::Embertrail;
use macroquad::audio::{
    load_sound_from_bytes, play_sound, set_sound_volume, stop_sound, PlaySoundParams, Sound,
};
use serde::Deserialize;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};
use vesper3d::runtime::audio_project::AudioBundle;
use vesper3d::two_d::GameLogic;

#[derive(Clone, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub seconds: f32,
}
struct Playing {
    id: usize,
    sound: Sound,
    // Retain the exact checked PCM bytes for an instant, matching download.
    bytes: Arc<Vec<u8>>,
    volume: f32,
}
struct Prepared {
    id: usize,
    sound: Sound,
    bytes: Arc<Vec<u8>>,
}
type Loaded = Result<Vec<u8>, String>;
struct PendingTrack {
    id: usize,
    receiver: mpsc::Receiver<Loaded>,
}
struct InteractionSound {
    variants: Vec<Sound>,
    seconds: Vec<f32>,
    next: usize,
    last: Option<Instant>,
}
struct ActiveInteraction {
    cue: Cue,
    variant: usize,
    until: Instant,
}
pub struct Score {
    pub tracks: Vec<Track>,
    current: Option<Playing>,
    outgoing: Option<Playing>,
    prepared: Vec<Prepared>,
    preparation_context: Vec<usize>,
    pending: Option<PendingTrack>,
    download: Option<mpsc::Receiver<Result<String, String>>>,
    cues: Vec<InteractionSound>,
    active_cues: Vec<ActiveInteraction>,
    muted: bool,
    failed: Vec<usize>,
    pub submitted: u32,
    pub downloads: u32,
    /// Audio submission evidence, deliberately distinct from physical speaker playback.
    pub effects_submitted: u32,
    pub effect_counts: [u32; Cue::ALL.len()],
    pub last_effect: Option<&'static str>,
    pub last_decode_ms: f64,
    pub max_decode_ms: f64,
    pub decode_count: u32,
    pub last_decode_track: Option<usize>,
    pub decoded_tracks: usize,
    pub max_decoded_tracks: usize,
    pub decoded_pcm_bytes: u64,
    pub max_decoded_pcm_bytes: u64,
    pub boss_music: bool,
    /// Fraction of submitted music volume belonging to guardian compositions during a fade.
    pub boss_mix: f32,
    pub crossfade_seconds: f32,
}
impl Score {
    pub async fn new(muted: bool) -> Result<Self, String> {
        let tracks: Vec<Track> =
            serde_json::from_str(include_str!("../assets/audio/score/tracks.json"))
                .map_err(|e| format!("Soundtrack catalog: {e}"))?;
        let mut cues = Vec::new();
        if !muted {
            let root = std::path::Path::new("assets/audio/sfx");
            let bank =
                AudioBundle::load(root).map_err(|e| format!("Interaction sound bank: {e}"))?;
            for cue in Cue::ALL {
                let files = bank
                    .effects
                    .get(cue.id())
                    .ok_or_else(|| format!("Interaction sound {} is missing", cue.id()))?;
                let mut variants = Vec::new();
                let mut seconds = Vec::new();
                for info in files {
                    let wav = std::fs::read(root.join(&info.file))
                        .map_err(|e| format!("Interaction sound {}: {e}", cue.id()))?;
                    AudioBundle::verify_file(info, &wav)
                        .map_err(|e| format!("Interaction sound {}: {e}", cue.id()))?;
                    variants.push(
                        load_sound_from_bytes(&wav)
                            .await
                            .map_err(|e| format!("Interaction sound {} decode: {e}", cue.id()))?,
                    );
                    seconds.push(info.frames as f32 / 44100.);
                }
                cues.push(InteractionSound {
                    variants,
                    seconds,
                    next: 0,
                    last: None,
                });
            }
        }
        Ok(Self {
            tracks,
            current: None,
            outgoing: None,
            prepared: Vec::new(),
            preparation_context: Vec::new(),
            pending: None,
            download: None,
            cues,
            active_cues: Vec::new(),
            muted,
            failed: Vec::new(),
            submitted: 0,
            downloads: 0,
            effects_submitted: 0,
            effect_counts: [0; Cue::ALL.len()],
            last_effect: None,
            last_decode_ms: 0.,
            max_decode_ms: 0.,
            decode_count: 0,
            last_decode_track: None,
            decoded_tracks: 0,
            max_decoded_tracks: 0,
            decoded_pcm_bytes: 0,
            max_decoded_pcm_bytes: 0,
            boss_music: false,
            boss_mix: 0.,
            crossfade_seconds: 0.8,
        })
    }
    pub fn wanted(&self, game: &Embertrail, started: bool) -> usize {
        let id = if !started {
            "title"
        } else if game.outcome() == "won" {
            "victory"
        } else if game.state.secret_inside {
            "underroots"
        } else if game.boss_battle_active() {
            match game.state.course / 4 {
                0 | 6 => "warden-root",
                1 | 2 => "warden-tide",
                3 | 5 => "warden-gale",
                4 => "warden-iron",
                _ => "vesper",
            }
        } else {
            [
                "meadow", "moon", "marsh", "frost", "dunes", "sky", "forge", "citadel",
            ][game.state.course / 4]
        };
        self.tracks.iter().position(|t| t.id == id).unwrap_or(0)
    }
    fn context_tracks(&self, game: &Embertrail, started: bool) -> Vec<usize> {
        let wanted = self.wanted(game, started);
        if !started || game.outcome() == "won" {
            return vec![wanted];
        }
        let world = game.state.course / 4;
        let mut ids = vec![wanted];
        let mut names = vec![
            [
                "meadow", "moon", "marsh", "frost", "dunes", "sky", "forge", "citadel",
            ][world],
            "underroots",
        ];
        if game.state.course % 4 == 3 {
            names.push(match world {
                0 | 6 => "warden-root",
                1 | 2 => "warden-tide",
                3 | 5 => "warden-gale",
                4 => "warden-iron",
                _ => "vesper",
            });
        }
        for name in names {
            if let Some(id) = self.tracks.iter().position(|track| track.id == name) {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        ids
    }
    /// A context includes every theme reachable without a noninteractive course change.
    pub fn context_ready(&self, game: &Embertrail, started: bool) -> bool {
        if self.muted {
            return true;
        }
        let mut context = self.context_tracks(game, started);
        context.sort_unstable();
        self.pending.is_none()
            && context == self.preparation_context
            && context.iter().all(|id| {
                self.prepared.iter().any(|track| track.id == *id) || self.failed.contains(id)
            })
    }
    fn measure_resident_tracks(&mut self) {
        let mut ids: Vec<_> = self.prepared.iter().map(|track| track.id).collect();
        ids.extend(self.current.iter().map(|track| track.id));
        ids.extend(self.outgoing.iter().map(|track| track.id));
        ids.sort_unstable();
        ids.dedup();
        self.decoded_tracks = ids.len();
        self.decoded_pcm_bytes = ids
            .iter()
            .map(|id| (f64::from(self.tracks[*id].seconds) * 44100.).ceil() as u64 * 2 * 4)
            .sum();
        self.max_decoded_tracks = self.max_decoded_tracks.max(self.decoded_tracks);
        self.max_decoded_pcm_bytes = self.max_decoded_pcm_bytes.max(self.decoded_pcm_bytes);
        debug_assert!(
            self.decoded_tracks <= 4,
            "course audio cache exceeded four unique tracks"
        );
    }
    /// Call only after drawing/yielding a loading screen and suspending gameplay input.
    /// Each call handles at most one complete decode. Checked file I/O remains a worker job.
    pub async fn prepare_context(&mut self, game: &Embertrail, started: bool) -> Option<String> {
        if self.muted {
            return None;
        }
        let requested = self.context_tracks(game, started);
        let mut key = requested.clone();
        key.sort_unstable();
        if key != self.preparation_context {
            // Keep the audible current song during preparation; retire any obsolete fade
            // before warming at most three context tracks. Handles and PCM are shared.
            if let Some(previous) = self.outgoing.take() {
                stop_sound(&previous.sound);
            }
            self.prepared.retain(|track| requested.contains(&track.id));
            self.failed.retain(|id| requested.contains(id));
            self.preparation_context = key;
            self.measure_resident_tracks();
        }
        let mut notice = None;
        if let Some(pending) = &self.pending {
            let id = pending.id;
            match pending.receiver.try_recv() {
                Ok(result) => {
                    self.pending = None;
                    if requested.contains(&id) {
                        match result {
                            Ok(bytes) => {
                                let started = Instant::now();
                                let decoded = load_sound_from_bytes(&bytes).await;
                                self.last_decode_ms = started.elapsed().as_secs_f64() * 1000.;
                                self.max_decode_ms = self.max_decode_ms.max(self.last_decode_ms);
                                self.decode_count += 1;
                                self.last_decode_track = Some(id);
                                println!(
                                    "{}",
                                    serde_json::json!({"music_decode":self.tracks[id].id,"milliseconds":self.last_decode_ms,"bytes":bytes.len(),"phase":"course-preparation"})
                                );
                                match decoded {
                                    Ok(sound) => self.prepared.push(Prepared {
                                        id,
                                        sound,
                                        bytes: Arc::new(bytes),
                                    }),
                                    Err(error) => {
                                        self.failed.push(id);
                                        notice = Some(format!("Music decode failed: {error}"));
                                    }
                                }
                            }
                            Err(error) => {
                                self.failed.push(id);
                                notice = Some(error);
                            }
                        }
                    }
                    self.measure_resident_tracks();
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    if requested.contains(&id) {
                        self.failed.push(id);
                    }
                    notice = Some("Music loading worker ended unexpectedly".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.pending.is_none() {
            if let Some(id) = requested
                .iter()
                .find(|id| {
                    !self.prepared.iter().any(|track| track.id == **id) && !self.failed.contains(id)
                })
                .copied()
            {
                let (sender, receiver) = mpsc::channel();
                let name = self.tracks[id].id.clone();
                std::thread::spawn(move || {
                    let _ = sender.send(read_track(&name));
                });
                self.pending = Some(PendingTrack { id, receiver });
            }
        }
        notice
    }
    pub fn title(&self, wanted: usize) -> &str {
        &self.tracks[wanted].title
    }
    fn is_boss_track(&self, id: usize) -> bool {
        self.tracks[id].id.starts_with("warden-") || self.tracks[id].id == "vesper"
    }
    pub fn now_playing(&self, wanted: usize) -> usize {
        self.current.as_ref().map_or(wanted, |playing| playing.id)
    }
    pub fn loading(&self, wanted: usize) -> bool {
        !self.muted && self.current.as_ref().is_none_or(|p| p.id != wanted)
    }
    /// Release native audio handles while Macroquad's context is still alive.
    pub fn shutdown(&mut self) {
        for playing in self.current.iter().chain(self.outgoing.iter()) {
            stop_sound(&playing.sound);
        }
        for track in &self.prepared {
            stop_sound(&track.sound);
        }
        for cue in &self.cues {
            for sound in &cue.variants {
                stop_sound(sound);
            }
        }
        self.current = None;
        self.outgoing = None;
        self.prepared.clear();
        self.cues.clear();
        self.active_cues.clear();
        self.pending = None;
        self.boss_music = false;
        self.boss_mix = 0.;
        self.measure_resident_tracks();
    }
    pub fn cue(&mut self, cue: usize, enabled: bool) {
        if !enabled || self.muted {
            return;
        }
        let Some(kind) = Cue::from_index(cue) else {
            return;
        };
        let Some(sound) = self.cues.get(cue) else {
            return;
        };
        let now = Instant::now();
        if sound
            .last
            .is_some_and(|last| now.duration_since(last).as_secs_f32() < kind.retrigger_seconds())
        {
            return;
        }
        self.active_cues.retain(|active| active.until > now);
        let variant = (0..sound.variants.len())
            .map(|offset| (sound.next + offset) % sound.variants.len())
            .find(|variant| {
                !self
                    .active_cues
                    .iter()
                    .any(|active| active.cue == kind && active.variant == *variant)
            });
        let Some(variant) = variant else { return };
        // Four simultaneous world voices leave headroom for the score. Critical damage and
        // bell responses can replace ordinary movement; spark clusters cannot bury them.
        if self.active_cues.len() >= 4 {
            let Some((index, weakest)) = self
                .active_cues
                .iter()
                .enumerate()
                .min_by_key(|(_, active)| active.cue.priority())
            else {
                return;
            };
            if weakest.cue.priority() > kind.priority() {
                return;
            }
            let replaced = self.active_cues.swap_remove(index);
            stop_sound(&self.cues[replaced.cue as usize].variants[replaced.variant]);
        }
        let sound = &mut self.cues[cue];
        play_sound(
            &sound.variants[variant],
            PlaySoundParams {
                looped: false,
                volume: kind.gain(),
            },
        );
        sound.last = Some(now);
        sound.next = (variant + 1) % sound.variants.len();
        self.active_cues.push(ActiveInteraction {
            cue: kind,
            variant,
            until: now + Duration::from_secs_f32(sound.seconds[variant]),
        });
        self.effects_submitted += 1;
        self.effect_counts[cue] += 1;
        self.last_effect = Some(kind.id());
    }
    pub async fn update(
        &mut self,
        wanted: usize,
        enabled: bool,
        paused: bool,
        focused: bool,
        dt: f32,
    ) -> Option<String> {
        let mut notice = None;
        if let Some(receiver) = &self.download {
            match receiver.try_recv() {
                Ok(result) => {
                    self.download = None;
                    notice = Some(match result {
                        Ok(path) => {
                            self.downloads += 1;
                            println!("{}", serde_json::json!({"soundtrack_download":path}));
                            format!("Saved WAV: {path}")
                        }
                        Err(e) => e,
                    });
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.download = None;
                    notice = Some("Music download worker ended unexpectedly".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        // All long-form decoding is confined to prepare_context's explicit loading phase.
        // World/grotto/guardian switches only submit an already resident native handle.
        if !self.muted && self.current.as_ref().is_none_or(|track| track.id != wanted) {
            if let Some(prepared) = self.prepared.iter().find(|track| track.id == wanted) {
                let from = self
                    .current
                    .as_ref()
                    .map(|track| self.tracks[track.id].id.clone());
                let boss_transition = self.is_boss_track(wanted)
                    || self
                        .current
                        .as_ref()
                        .is_some_and(|track| self.is_boss_track(track.id));
                self.crossfade_seconds = if boss_transition { 1.6 } else { 0.8 };
                if let Some(previous) = self.outgoing.take() {
                    stop_sound(&previous.sound);
                }
                self.outgoing = self.current.take();
                let sound = prepared.sound.clone();
                play_sound(
                    &sound,
                    PlaySoundParams {
                        looped: true,
                        volume: 0.,
                    },
                );
                self.current = Some(Playing {
                    id: wanted,
                    sound,
                    bytes: prepared.bytes.clone(),
                    volume: 0.,
                });
                self.submitted += 1;
                println!(
                    "{}",
                    serde_json::json!({"music_transition":self.tracks[wanted].id,"from":from,"boss_music":self.is_boss_track(wanted),"crossfade_seconds":self.crossfade_seconds,"decode_count":self.decode_count})
                );
            }
        }
        // Pause leaves the exact song and playback position running for the music download menu.
        let target = if enabled && focused && !self.muted {
            if paused {
                0.23
            } else {
                0.31
            }
        } else {
            0.
        };
        let fade_rate = 0.31 / self.crossfade_seconds;
        if let Some(current) = &mut self.current {
            current.volume = if !enabled || !focused {
                0.
            } else {
                current.volume + (target - current.volume).clamp(-dt * fade_rate, dt * fade_rate)
            };
            set_sound_volume(&current.sound, current.volume);
        }
        if let Some(outgoing) = &mut self.outgoing {
            outgoing.volume = if !enabled || !focused {
                0.
            } else {
                (outgoing.volume - dt * fade_rate).max(0.)
            };
            set_sound_volume(&outgoing.sound, outgoing.volume);
            if outgoing.volume <= 0. {
                stop_sound(&outgoing.sound);
                self.outgoing = None;
            }
        }
        self.boss_music = self
            .current
            .as_ref()
            .is_some_and(|track| self.is_boss_track(track.id));
        let mut total = 0.;
        let mut battle = 0.;
        for playing in self.current.iter().chain(self.outgoing.iter()) {
            total += playing.volume;
            if self.is_boss_track(playing.id) {
                battle += playing.volume;
            }
        }
        self.boss_mix = if total > 0. { battle / total } else { 0. };
        self.measure_resident_tracks();
        notice
    }
    pub fn save_current(&mut self, wanted: usize) -> String {
        if self.download.is_some() {
            return "Saving music…".into();
        }
        let current = self
            .current
            .as_ref()
            .filter(|p| p.id == wanted)
            .map(|p| p.bytes.clone());
        let track = self.tracks[wanted].clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| {
                use vesper3d::viewer::devkit::{downloads_dir, sanitize_filename, unique_path};
                let directory = std::env::var_os("EMBERTRAIL_DOWNLOAD_DIR")
                    .map(std::path::PathBuf::from)
                    .or_else(|| {
                        downloads_dir().map(|directory| directory.join("Embertrail Soundtrack"))
                    })
                    .ok_or("Downloads folder unavailable")?;
                std::fs::create_dir_all(&directory).map_err(|e| format!("Music download: {e}"))?;
                let path = unique_path(
                    &directory,
                    &sanitize_filename(&format!("Embertrail - {}", track.title)),
                    "wav",
                );
                let bytes = match current {
                    Some(bytes) => bytes,
                    None => Arc::new(read_track(&track.id)?),
                };
                // create_new also protects a previous download if the name was taken meanwhile.
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|e| format!("Music download: {e}"))?;
                if let Err(error) = file.write_all(&bytes) {
                    drop(file);
                    // A full/unwritable Downloads folder must not leave a broken WAV behind.
                    let _ = std::fs::remove_file(&path);
                    return Err(format!("Music download: {error}"));
                }
                Ok(path.display().to_string())
            })();
            let _ = sender.send(result);
        });
        self.download = Some(receiver);
        format!("Saving {}…", self.tracks[wanted].title)
    }
}
fn read_track(id: &str) -> Result<Vec<u8>, String> {
    let root = std::path::PathBuf::from("assets/audio/score").join(id);
    let bank = AudioBundle::load(&root).map_err(|e| format!("Music {id}: {e}"))?;
    let info = bank
        .music
        .get("song")
        .ok_or_else(|| format!("Music {id}: missing song"))?;
    let wav = std::fs::read(root.join(&info.file)).map_err(|e| format!("Music {id}: {e}"))?;
    AudioBundle::verify_file(info, &wav).map_err(|e| format!("Music {id}: {e}"))?;
    Ok(wav)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vesper3d::runtime::Simulation;
    #[test]
    fn interaction_bank_has_distinct_checked_behavior_voices_with_short_clean_envelopes() {
        let root = std::path::Path::new("assets/audio/sfx");
        let bank = AudioBundle::load(root).unwrap();
        let brief: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("design.json")).unwrap()).unwrap();
        assert_eq!(bank.effects.len(), Cue::ALL.len());
        assert!(bank.music.is_empty());
        let mut recordings = std::collections::BTreeSet::new();
        for cue in Cue::ALL {
            let voice = &bank.effects[cue.id()];
            assert_eq!(brief["cues"][cue as usize]["id"], cue.id());
            assert!(!brief["cues"][cue as usize]["behavior"]
                .as_str()
                .unwrap()
                .is_empty());
            for info in voice {
                let bytes = std::fs::read(root.join(&info.file)).unwrap();
                AudioBundle::verify_file(info, &bytes).unwrap();
                // This original bank deliberately uses the simple PCM16 WAV layout.
                // Measure the payload too: a plausible manifest is not clipping evidence.
                assert_eq!(&bytes[36..40], b"data");
                let samples: Vec<f32> = bytes[44..]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|v| i16::from_le_bytes([v[0], v[1]]) as f32 / 32768.)
                    .collect();
                let peak = samples.iter().map(|v| v.abs()).fold(0., f32::max);
                assert!(peak < 0.64 && (peak - info.peak).abs() < 0.0001);
                assert!(
                    recordings.insert(info.checksum.clone()),
                    "reused effect {}",
                    cue.id()
                );
                let seconds = info.frames as f32 / 44100.;
                assert!(
                    (0.04..=1.8).contains(&seconds),
                    "{} duration {seconds}",
                    cue.id()
                );
                assert!(info.peak <= 0.64 && info.rms > 0.01 && info.dc < 0.002);
                assert!(info.seam_jump < 0.0001, "{} has a sharp boundary", cue.id());
                if cue.priority() == 0 {
                    assert!(seconds <= 0.17 && cue.gain() <= 0.13);
                }
            }
        }
        assert!(bank.effects["clay-break"].len() >= 2);
        assert!(bank.effects["spark-pickup"][0].frames < bank.effects["shard-pickup"][0].frames);
        assert!(bank.effects["hurt"][0].frames < bank.effects["death"][0].frames);
    }

    #[test]
    fn every_named_track_has_a_checked_long_form_original_composition() {
        let catalog: Vec<Track> =
            serde_json::from_str(include_str!("../assets/audio/score/tracks.json")).unwrap();
        assert_eq!(catalog.len(), 16);
        let mut hashes = std::collections::BTreeSet::new();
        let mut melodies = std::collections::BTreeSet::new();
        let mut meters = std::collections::BTreeSet::new();
        for track in catalog {
            assert!(
                (60. ..=120.).contains(&track.seconds),
                "{} duration {}",
                track.id,
                track.seconds
            );
            let root = std::path::PathBuf::from("assets/audio/score").join(&track.id);
            let bank = AudioBundle::load(&root).unwrap();
            let info = &bank.music["song"];
            let composition: serde_json::Value =
                serde_json::from_slice(&std::fs::read(root.join("composition.json")).unwrap())
                    .unwrap();
            assert_eq!(composition["sections"].as_array().unwrap().len(), 4);
            assert_ne!(composition["melody_a"], composition["melody_b"]);
            assert_ne!(composition["rhythm_a"], composition["rhythm_b"]);
            assert!(
                melodies.insert(composition["melody_a"].to_string()),
                "duplicate melody"
            );
            meters.insert(composition["meter"].as_u64().unwrap());
            if track.id.starts_with("warden-") || track.id == "vesper" {
                let arrangement = &composition["battle_arrangement"];
                assert_eq!(arrangement["heartbeats"].as_array().unwrap().len(), 2);
                assert!(arrangement["frame_drum_accents"].as_array().unwrap().len() >= 2);
                let strengths = arrangement["section_strength"].as_array().unwrap();
                assert_eq!(strengths.len(), 4);
                assert!(strengths[2].as_f64().unwrap() < strengths[3].as_f64().unwrap());
                assert!(!arrangement["extra_bass_answer"]
                    .as_str()
                    .unwrap()
                    .is_empty());
            }
            assert!(hashes.insert(info.checksum.clone()), "duplicate recording");
            assert!(info.rms > 0.03 && info.peak <= 0.77 && info.dc < 0.001);
            assert!(
                info.seam_jump < 0.04,
                "{} seam {}",
                track.title,
                info.seam_jump
            );
            assert!((info.frames as f32 / 44100. - track.seconds).abs() < 0.01);
            read_track(&track.id).unwrap();
        }
        assert_eq!(meters, [3, 4, 5].into_iter().collect());
    }
    #[test]
    fn musical_context_follows_world_secret_and_stable_guardian_encounter() {
        let score = Score {
            tracks: serde_json::from_str(include_str!("../assets/audio/score/tracks.json"))
                .unwrap(),
            current: None,
            outgoing: None,
            prepared: Vec::new(),
            preparation_context: Vec::new(),
            pending: None,
            download: None,
            cues: Vec::new(),
            active_cues: Vec::new(),
            muted: true,
            failed: Vec::new(),
            submitted: 0,
            downloads: 0,
            effects_submitted: 0,
            effect_counts: [0; Cue::ALL.len()],
            last_effect: None,
            last_decode_ms: 0.,
            max_decode_ms: 0.,
            decode_count: 0,
            last_decode_track: None,
            decoded_tracks: 0,
            max_decoded_tracks: 0,
            decoded_pcm_bytes: 0,
            max_decoded_pcm_bytes: 0,
            boss_music: false,
            boss_mix: 0.,
            crossfade_seconds: 0.8,
        };
        let mut game = Embertrail::new(7);
        assert_eq!(score.tracks[score.wanted(&game, false)].id, "title");
        assert_eq!(score.context_tracks(&game, false).len(), 1);
        assert!(
            score.context_ready(&game, true),
            "muted captures must never wait for audio"
        );
        let mut world_themes = std::collections::BTreeSet::new();
        for world in 0..8 {
            game.state.unlocked = 31;
            game.enter_course(world * 4);
            world_themes.insert(score.tracks[score.wanted(&game, true)].id.clone());
            let exploration = score.context_tracks(&game, true);
            assert_eq!(
                exploration.len(),
                2,
                "only this world and its grotto are warmed"
            );
            game.state.secret_inside = true;
            assert_eq!(score.tracks[score.wanted(&game, true)].id, "underroots");
            assert!(
                exploration.contains(&score.wanted(&game, true)),
                "grotto entry must use a prepared track"
            );
            game.enter_course(world * 4 + 3);
            let arena = score.context_tracks(&game, true);
            assert_eq!(
                arena.len(),
                3,
                "boss course adds exactly its guardian theme"
            );
            let boss = game
                .state
                .level
                .enemies
                .iter_mut()
                .find(|enemy| enemy.kind == 4)
                .unwrap();
            boss.phase = 1;
            let visual_left = boss.r.x - 12;
            game.state.px = (visual_left - 519) * embertrail::SUB;
            game.state.camera = visual_left - 800;
            assert!(!game.boss_on_screen());
            let normal = score.wanted(&game, true);
            assert!(
                !score.is_boss_track(normal),
                "an activated but unseen guardian must not change the song"
            );
            game.state.camera += 1;
            assert!(
                game.boss_on_screen(),
                "the first rendered guardian pixel is visible"
            );
            assert!(score.is_boss_track(score.wanted(&game, true)));
            game.step(&vesper3d::two_d::Intent::default());
            assert!(
                game.boss_on_screen(),
                "camera movement exposes the first guardian pixel"
            );
            assert!(game.state.boss_seen);
            let theme = score.wanted(&game, true);
            assert!(
                arena.contains(&theme),
                "guardian appearance must use a prepared track"
            );
            assert!(score.is_boss_track(theme));
            // A moving guardian can leave the picture without restarting either song.
            game.state.px = 0;
            game.state.camera = 0;
            assert!(!game.boss_on_screen());
            assert_eq!(theme, score.wanted(&game, true));
            game.state.dead = 90;
            assert_eq!(
                normal,
                score.wanted(&game, true),
                "death returns to exploration music"
            );
            game.state.dead = 0;
            let boss = game
                .state
                .level
                .enemies
                .iter_mut()
                .find(|enemy| enemy.kind == 4)
                .unwrap();
            boss.active = false;
            boss.hp = 0;
            assert_eq!(
                normal,
                score.wanted(&game, true),
                "freeing the guardian returns to the world song"
            );
        }
        assert_eq!(world_themes.len(), 8);
        game.state.won = true;
        assert_eq!(score.tracks[score.wanted(&game, true)].id, "victory");
        assert_eq!(score.context_tracks(&game, true).len(), 1);
    }
}
