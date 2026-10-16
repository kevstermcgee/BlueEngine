//! Named, contextual score. Only the current and fading previous WAV are decoded.
use embertrail::Embertrail;
use macroquad::audio::{
    load_sound_from_bytes, play_sound, set_sound_volume, stop_sound, PlaySoundParams, Sound,
};
use serde::Deserialize;
use std::sync::{mpsc, Arc};
use vesper3d::runtime::{audio_project::AudioBundle, synth};
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
type Loaded = Result<(usize, Vec<u8>), String>;
pub struct Score {
    pub tracks: Vec<Track>,
    current: Option<Playing>,
    outgoing: Option<Playing>,
    pending: Option<mpsc::Receiver<Loaded>>,
    download: Option<mpsc::Receiver<Result<String, String>>>,
    cues: Vec<Sound>,
    muted: bool,
    failed: Option<usize>,
    pub submitted: u32,
    pub downloads: u32,
}
impl Score {
    pub async fn new(muted: bool) -> Result<Self, String> {
        let tracks: Vec<Track> =
            serde_json::from_str(include_str!("../assets/audio/score/tracks.json"))
                .map_err(|e| format!("Soundtrack catalog: {e}"))?;
        let mut cues = Vec::new();
        if !muted {
            for preset in [
                synth::Preset::Coin,
                synth::Preset::Hit,
                synth::Preset::Success,
            ] {
                let wav = synth::wav_bytes(&synth::render(preset, 0, 7), synth::RATE);
                cues.push(
                    load_sound_from_bytes(&wav)
                        .await
                        .map_err(|e| e.to_string())?,
                );
            }
        }
        Ok(Self {
            tracks,
            current: None,
            outgoing: None,
            pending: None,
            download: None,
            cues,
            muted,
            failed: None,
            submitted: 0,
            downloads: 0,
        })
    }
    pub fn wanted(&self, game: &Embertrail, started: bool) -> usize {
        let id = if !started {
            "title"
        } else if game.outcome() == "won" {
            "victory"
        } else if game.state.secret_inside {
            "underroots"
        } else if game.boss_active() {
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
    pub fn title(&self, wanted: usize) -> &str {
        &self.tracks[wanted].title
    }
    pub fn now_playing(&self, wanted: usize) -> usize {
        self.current.as_ref().map_or(wanted, |playing| playing.id)
    }
    pub fn loading(&self, wanted: usize) -> bool {
        !self.muted && self.current.as_ref().is_none_or(|p| p.id != wanted)
    }
    pub fn cue(&mut self, cue: usize, enabled: bool) {
        if enabled && !self.muted {
            if let Some(sound) = self.cues.get(cue) {
                play_sound(
                    sound,
                    PlaySoundParams {
                        looped: false,
                        volume: if cue == 1 { 0.25 } else { 0.20 },
                    },
                );
            }
        }
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
        if let Some(receiver) = &self.pending {
            match receiver.try_recv() {
                Ok(result) => {
                    self.pending = None;
                    match result {
                        Ok((id, bytes)) if id == wanted => {
                            match load_sound_from_bytes(&bytes).await {
                                Ok(sound) => {
                                    if let Some(previous) = self.outgoing.take() {
                                        stop_sound(&previous.sound);
                                    }
                                    self.outgoing = self.current.take();
                                    play_sound(
                                        &sound,
                                        PlaySoundParams {
                                            looped: true,
                                            volume: 0.,
                                        },
                                    );
                                    self.current = Some(Playing {
                                        id,
                                        sound,
                                        bytes: Arc::new(bytes),
                                        volume: 0.,
                                    });
                                    self.submitted += 1;
                                }
                                Err(error) => {
                                    self.failed = Some(id);
                                    notice = Some(format!("Music decode failed: {error}"));
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(error) => {
                            self.failed = Some(wanted);
                            notice = Some(error);
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.failed = Some(wanted);
                    notice = Some("Music loading worker ended unexpectedly".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if !self.muted
            && self.pending.is_none()
            && self.failed != Some(wanted)
            && self.current.as_ref().is_none_or(|p| p.id != wanted)
        {
            let (sender, receiver) = mpsc::channel();
            let id = self.tracks[wanted].id.clone();
            std::thread::spawn(move || {
                let _ = sender.send(read_track(&id).map(|wav| (wanted, wav)));
            });
            self.pending = Some(receiver);
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
        if let Some(current) = &mut self.current {
            current.volume = if !enabled || !focused {
                0.
            } else {
                current.volume + (target - current.volume).clamp(-dt * 0.42, dt * 0.42)
            };
            set_sound_volume(&current.sound, current.volume);
        }
        if let Some(outgoing) = &mut self.outgoing {
            outgoing.volume = if !enabled || !focused {
                0.
            } else {
                (outgoing.volume - dt * 0.38).max(0.)
            };
            set_sound_volume(&outgoing.sound, outgoing.volume);
            if outgoing.volume <= 0. {
                stop_sound(&outgoing.sound);
                self.outgoing = None;
            }
        }
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
            pending: None,
            download: None,
            cues: Vec::new(),
            muted: true,
            failed: None,
            submitted: 0,
            downloads: 0,
        };
        let mut game = Embertrail::new(7);
        assert_eq!(score.tracks[score.wanted(&game, false)].id, "title");
        let mut world_themes = std::collections::BTreeSet::new();
        for world in 0..8 {
            game.state.unlocked = 31;
            game.enter_course(world * 4);
            world_themes.insert(score.tracks[score.wanted(&game, true)].id.clone());
            game.state.secret_inside = true;
            assert_eq!(score.tracks[score.wanted(&game, true)].id, "underroots");
            game.enter_course(world * 4 + 3);
            let boss = game
                .state
                .level
                .enemies
                .iter_mut()
                .find(|enemy| enemy.kind == 4)
                .unwrap();
            boss.phase = 1;
            let theme = score.wanted(&game, true);
            // Retreating from a moving guardian must not restart the exploration song.
            game.state.px = 0;
            assert_eq!(theme, score.wanted(&game, true));
            assert!(
                score.tracks[theme].id.starts_with("warden") || score.tracks[theme].id == "vesper"
            );
        }
        assert_eq!(world_themes.len(), 8);
        game.state.won = true;
        assert_eq!(score.tracks[score.wanted(&game, true)].id, "victory");
    }
}
