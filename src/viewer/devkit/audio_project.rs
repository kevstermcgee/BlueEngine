//! Named audio: checked stereo scores and crossfaded imported ambience loops, rendered before runtime.
//! Effects, stereo note scores and adaptive music share the existing synthesis toolkit. No device,
//! window or gameplay rules live here. Discover the contract with `be2-tools audio describe`.
use super::synth::{self, AmbientSpec, MusicSpec, Preset, RATE};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

const MAX_SECONDS: f32 = 180.;
const MAX_BYTES: u64 = 64 * 1024 * 1024;
fn one() -> f32 {
    1.
}
fn headroom() -> f32 {
    0.85
}
fn seed() -> u64 {
    1
}
fn bpm() -> f32 {
    120.
}
fn attack() -> f32 {
    0.008
}
fn decay() -> f32 {
    0.08
}
fn sustain() -> f32 {
    0.6
}
fn release() -> f32 {
    0.12
}

/// Versioned project. Paths in WAV sources are relative to this project's directory.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioProject {
    pub version: u32,
    #[serde(default = "seed")]
    pub seed: u64,
    /// Ceiling for each effect and *any subset* of music layers at levels 0..1.
    #[serde(default = "headroom")]
    pub headroom: f32,
    #[serde(default)]
    pub effects: BTreeMap<String, Effect>,
    #[serde(default)]
    pub music: Option<Music>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Effect {
    #[serde(default = "one")]
    pub gain: f32,
    pub source: EffectSource,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EffectSource {
    /// Uses all meaningful variants of an existing synth preset.
    Preset {
        preset: String,
    },
    Score {
        score: Score,
    },
    /// Strict 44.1 kHz mono/stereo PCM16; never resampled or silently downmixed.
    Wav {
        file: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Music {
    /// Imported ambience/stems, cropped to exact equal loop length with a bounded wrap crossfade.
    Clips {
        seconds: f32,
        crossfade_seconds: f32,
        layers: BTreeMap<String, Clip>,
    },
    Score {
        score: Score,
    },
    Generated {
        bpm: f32,
        bars: usize,
        root_midi: u8,
        minor: bool,
    },
    Ambient {
        seconds: f32,
        root_midi: u8,
        minor: bool,
        chords: usize,
        brightness: f32,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clip {
    pub file: String,
    #[serde(default = "one")]
    pub gain: f32,
}

/// Beat-based polyphonic score. Music wraps released tails; one-shots retain the full release.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Score {
    #[serde(default = "bpm")]
    pub bpm: f32,
    pub beats: f32,
    pub layers: Vec<Layer>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub name: String,
    #[serde(default)]
    pub instrument: Instrument,
    #[serde(default = "one")]
    pub gain: f32,
    pub notes: Vec<Note>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Wave {
    #[default]
    Sine,
    Triangle,
    Saw,
    Square,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Instrument {
    pub wave: Wave,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub lowpass_hz: Option<f32>,
}
impl Default for Instrument {
    fn default() -> Self {
        Self {
            wave: Wave::Sine,
            attack: attack(),
            decay: decay(),
            sustain: sustain(),
            release: release(),
            lowpass_hz: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub at: f32,
    pub beats: f32,
    pub midi: u8,
    #[serde(default = "one")]
    pub velocity: f32,
    /// Equal-power authoring pan: -1 left, 0 centre, +1 right.
    #[serde(default)]
    pub pan: f32,
}

fn range(label: &str, value: f32, min: f32, max: f32) -> Result<(), String> {
    if value.is_finite() && (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(format!(
            "{label}: expected finite {min}..{max}, got {value}"
        ))
    }
}
fn name(value: &str) -> Result<(), String> {
    if !value.is_empty()
        && value.len() <= 48
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        Ok(())
    } else {
        Err(format!(
            "invalid audio name {value:?}: use 1..48 ASCII letters, digits, _ or -"
        ))
    }
}

impl Score {
    fn validate(&self, looping: bool) -> Result<f32, String> {
        range("bpm", self.bpm, 40., 240.)?;
        range("beats", self.beats, 0.05, 720.)?;
        if self.layers.is_empty() || self.layers.len() > 16 {
            return Err("score needs 1..16 layers".into());
        }
        let mut names = std::collections::BTreeSet::new();
        let seconds = self.beats * 60. / self.bpm;
        let mut tail = 0f32;
        let mut work = 0f32;
        let mut notes = 0;
        for layer in &self.layers {
            name(&layer.name)?;
            if !names.insert(&layer.name) {
                return Err(format!("duplicate layer {}", layer.name));
            }
            range("layer gain", layer.gain, 0.001, 1.)?;
            let i = &layer.instrument;
            range("attack", i.attack, 0.001, 10.)?;
            range("decay", i.decay, 0.001, 10.)?;
            range("sustain", i.sustain, 0., 1.)?;
            range("release", i.release, 0.005, 10.)?;
            if let Some(hz) = i.lowpass_hz {
                range("lowpass_hz", hz, 40., 20_000.)?;
            }
            if layer.notes.is_empty() {
                return Err(format!("layer {} has no notes", layer.name));
            }
            for n in &layer.notes {
                range("note at", n.at, 0., self.beats)?;
                range("note beats", n.beats, 0.001, self.beats)?;
                range("velocity", n.velocity, 0.001, 1.)?;
                range("pan", n.pan, -1., 1.)?;
                if !(12..=108).contains(&n.midi) {
                    return Err("MIDI pitch must be 12..108".into());
                }
                if n.at >= self.beats || n.at + n.beats > self.beats + 0.0001 {
                    return Err(
                        "note extends beyond score beats (release tails wrap for music)".into(),
                    );
                }
                tail = tail.max((n.at + n.beats) * 60. / self.bpm + i.release);
                work += n.beats * 60. / self.bpm + i.release;
                notes += 1;
            }
        }
        if notes > 4096 || work > 600. {
            return Err("score exceeds 4096 notes or 600 seconds of synthesis work".into());
        }
        let duration = if looping { seconds } else { seconds.max(tail) };
        range("score duration", duration, 0.01, MAX_SECONDS)?;
        Ok(duration * self.layers.len() as f32)
    }
}

impl AudioProject {
    /// Strict schema/range validation before any expensive rendering. No clamped typo becomes music.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("unsupported audio project version (expected 1)".into());
        }
        range("headroom", self.headroom, 0.1, 0.95)?;
        if self.effects.len() > 64 || (self.effects.is_empty() && self.music.is_none()) {
            return Err("project needs audio, with at most 64 effects".into());
        }
        let mut budget = 0.;
        for (id, effect) in &self.effects {
            name(id)?;
            range("effect gain", effect.gain, 0.001, 1.)?;
            match &effect.source {
                EffectSource::Preset { preset } => {
                    Preset::from_name(preset)
                        .ok_or_else(|| format!("unknown preset {preset:?}; see audio describe"))?;
                    budget += 8.;
                }
                EffectSource::Score { score } => budget += score.validate(false)?,
                EffectSource::Wav { file } => {
                    safe_relative(file)?;
                }
            }
        }
        if let Some(music) = &self.music {
            budget += match music {
                Music::Clips {
                    seconds,
                    crossfade_seconds,
                    layers,
                } => {
                    range("clip loop seconds", *seconds, 0.1, MAX_SECONDS)?;
                    range(
                        "clip crossfade seconds",
                        *crossfade_seconds,
                        0.005,
                        seconds.min(4.) / 2.,
                    )?;
                    if layers.is_empty() || layers.len() > 16 {
                        return Err("clips need 1..16 named layers".into());
                    }
                    for (id, clip) in layers {
                        name(id)?;
                        safe_relative(&clip.file)?;
                        range("clip gain", clip.gain, 0.001, 1.)?;
                    }
                    *seconds * layers.len() as f32
                }
                Music::Score { score } => score.validate(true)?,
                Music::Generated {
                    bpm,
                    bars,
                    root_midi,
                    ..
                } => {
                    range("bpm", *bpm, 40., 240.)?;
                    if !(1..=64).contains(bars) || *root_midi > 127 {
                        return Err("generated music needs bars 1..64 and MIDI root 0..127".into());
                    }
                    *bars as f32 * 4. * 60. / bpm * 3.
                }
                Music::Ambient {
                    seconds,
                    root_midi,
                    chords,
                    brightness,
                    ..
                } => {
                    range("ambient seconds", *seconds, 30., MAX_SECONDS)?;
                    range("brightness", *brightness, 0., 1.)?;
                    if !(2..=8).contains(chords) || *root_midi > 127 {
                        return Err("ambient needs chords 2..8 and MIDI root 0..127".into());
                    }
                    *seconds
                }
            };
        }
        if budget > MAX_SECONDS {
            return Err("project exceeds 180 stereo-seconds; shorten music or split banks".into());
        }
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = read_bounded(path, 1024 * 1024)?;
        let project: Self =
            serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        project.validate()?;
        Ok(project)
    }
}

fn safe_relative(file: &str) -> Result<(), String> {
    // Identical interpretation on Windows and Unix; no absolute paths or alternate data streams.
    if file.is_empty()
        || file.contains(['\\', ':'])
        || file
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
        || Path::new(file).is_absolute()
    {
        return Err(format!("unsafe relative audio path {file:?}"));
    }
    Ok(())
}
fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > max {
        return Err(format!("{} exceeds {max} bytes", path.display()));
    }
    Ok(bytes)
}

/// Strict ingestion atop the legacy tolerant parser: truncated chunks and partial frames fail.
pub fn checked_wav(bytes: &[u8]) -> Result<(u16, Vec<f32>), String> {
    let (rate, channels, samples) = synth::parse_wav(bytes)?;
    if rate != RATE || !(1..=2).contains(&channels) || samples.is_empty() {
        return Err("audio requires nonempty 44100 Hz mono/stereo PCM16 WAV".into());
    }
    let declared = u64::from(u32::from_le_bytes(bytes[4..8].try_into().unwrap())) + 8;
    if declared != bytes.len() as u64 {
        return Err("WAV RIFF length does not match file".into());
    }
    let mut pos = 12;
    let mut saw_fmt = false;
    let mut saw_data = false;
    while pos < bytes.len() {
        if bytes.len() - pos < 8 {
            return Err("truncated WAV chunk header".into());
        }
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let end = pos
            .checked_add(8)
            .and_then(|s| s.checked_add(size))
            .ok_or("WAV chunk overflow")?;
        if end > bytes.len()
            || (!size.is_multiple_of(usize::from(channels) * 2) && &bytes[pos..pos + 4] == b"data")
        {
            return Err("truncated WAV chunk or partial PCM frame".into());
        }
        match &bytes[pos..pos + 4] {
            b"fmt " => {
                if saw_fmt || size < 16 {
                    return Err("audio requires one plain PCM16 fmt chunk".into());
                }
                saw_fmt = true;
                let body = &bytes[pos + 8..end];
                let align = u16::from_le_bytes(body[12..14].try_into().unwrap());
                let byte_rate = u32::from_le_bytes(body[8..12].try_into().unwrap());
                if align != channels * 2 || byte_rate != RATE * u32::from(align) {
                    return Err("inconsistent WAV frame alignment or byte rate".into());
                }
            }
            b"data" => {
                if saw_data {
                    return Err("audio requires one data chunk".into());
                }
                saw_data = true;
            }
            _ => {}
        }
        pos = end + (size & 1);
    }
    if pos != bytes.len() {
        return Err("missing WAV chunk padding".into());
    }
    Ok((channels, samples))
}

struct Stereo {
    left: Vec<f32>,
    right: Vec<f32>,
}
impl Stereo {
    fn mono(v: Vec<f32>) -> Self {
        Self {
            right: v.clone(),
            left: v,
        }
    }
    fn peak(&self) -> f32 {
        synth::peak(&self.left).max(synth::peak(&self.right))
    }
    fn gain(&mut self, gain: f32) {
        for v in self.left.iter_mut().chain(&mut self.right) {
            *v *= gain;
        }
    }
    fn bytes(&self) -> Vec<u8> {
        synth::wav_bytes_stereo(&self.left, &self.right, RATE)
    }
}

fn score_render(score: &Score, looping: bool) -> Vec<(String, Stereo)> {
    let seconds = score.beats * 60. / score.bpm;
    let length = if looping {
        seconds
    } else {
        score
            .layers
            .iter()
            .flat_map(|l| {
                l.notes
                    .iter()
                    .map(move |n| (n.at + n.beats) * 60. / score.bpm + l.instrument.release)
            })
            .fold(seconds, f32::max)
    };
    let frames = (length * RATE as f32).round() as usize;
    score
        .layers
        .iter()
        .map(|layer| {
            let mut left = synth::Mix::with_len(frames);
            let mut right = synth::Mix::with_len(frames);
            let i = &layer.instrument;
            for n in &layer.notes {
                let gate = n.beats * 60. / score.bpm;
                let hz = 440. * 2f32.powf((f32::from(n.midi) - 69.) / 12.);
                let duration = gate + i.release;
                let wave = match i.wave {
                    Wave::Sine => synth::osc(duration, |_| hz, synth::sine),
                    Wave::Triangle => synth::osc(duration, |_| hz, synth::triangle),
                    Wave::Saw => synth::osc_bl(duration, |_| hz, synth::Wave::Saw),
                    Wave::Square => synth::osc_bl(duration, |_| hz, synth::Wave::Square),
                };
                let mut note = synth::shape(wave, |t| {
                    synth::adsr(t, gate, i.attack, i.decay, i.sustain, i.release)
                });
                if let Some(hz) = i.lowpass_hz {
                    note = synth::lowpass(&note, hz, 0.707);
                }
                // Filtering can leave a residual tail: taper the final 5 ms, never normalize velocities.
                let fade = (RATE as f32 * 0.005) as usize;
                let len = note.len();
                for (idx, sample) in note.iter_mut().enumerate().skip(len.saturating_sub(fade)) {
                    *sample *= (len - 1 - idx) as f32 / fade as f32;
                }
                let angle = (n.pan + 1.) * std::f32::consts::FRAC_PI_4;
                let gain = n.velocity * layer.gain;
                let at = (n.at * 60. / score.bpm * RATE as f32).round() as usize;
                if looping {
                    left.add_wrapped_at(at, &note, gain * angle.cos());
                    right.add_wrapped_at(at, &note, gain * angle.sin());
                } else {
                    left.add_at(at, &note, gain * angle.cos());
                    right.add_at(at, &note, gain * angle.sin());
                }
            }
            (
                layer.name.clone(),
                Stereo {
                    left: left.into_vec(),
                    right: right.into_vec(),
                },
            )
        })
        .collect()
}

fn hash(bytes: &[u8]) -> String {
    format!(
        "{:016x}",
        bytes
            .iter()
            .fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(*b))
                .wrapping_mul(0x100000001b3))
    )
}

/// Per-file measurements are numerical evidence, not a judgment of musical quality.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioFile {
    pub file: String,
    pub checksum: String,
    pub frames: usize,
    pub peak: f32,
    pub rms: f32,
    pub dc: f32,
    pub seam_jump: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioBundle {
    pub version: u32,
    pub sample_rate: u32,
    pub effects: BTreeMap<String, Vec<AudioFile>>,
    pub music: BTreeMap<String, AudioFile>,
    pub music_scale: f32,
}

fn measured(file: String, s: &Stereo) -> AudioFile {
    let n = s.left.len();
    let dc = [s.left.as_slice(), s.right.as_slice()]
        .into_iter()
        .map(|v| (v.iter().map(|x| f64::from(*x)).sum::<f64>() / n as f64).abs() as f32)
        .fold(0., f32::max);
    AudioFile {
        file,
        checksum: hash(&s.bytes()),
        frames: n,
        peak: s.peak(),
        rms: synth::rms(&s.left).max(synth::rms(&s.right)),
        dc,
        seam_jump: (s.left[0] - s.left[n - 1])
            .abs()
            .max((s.right[0] - s.right[n - 1]).abs()),
    }
}

/// In-memory rendered project. Rendering never writes files; a failure preserves existing outputs.
pub struct RenderedProject {
    pub manifest: AudioBundle,
    pub files: BTreeMap<String, Vec<u8>>,
}

impl AudioProject {
    pub fn render(&self, root: &Path) -> Result<RenderedProject, String> {
        self.validate()?;
        let mut manifest = AudioBundle {
            version: 1,
            sample_rate: RATE,
            effects: BTreeMap::new(),
            music: BTreeMap::new(),
            music_scale: 1.,
        };
        let mut files = BTreeMap::new();
        let mut total = 0u64;
        let mut store = |file: String, s: &Stereo| -> Result<AudioFile, String> {
            if s.left.is_empty()
                || s.left.len() != s.right.len()
                || s.left.iter().chain(&s.right).any(|x| !x.is_finite())
            {
                return Err(format!("{file}: empty or non-finite audio"));
            }
            let info = measured(file.clone(), s);
            if info.rms < 0.00001 || info.dc > 0.02 {
                return Err(format!("{file}: silent audio or DC offset >0.02"));
            }
            let bytes = s.bytes();
            total += bytes.len() as u64;
            if total > MAX_BYTES {
                return Err("rendered bank exceeds 64 MiB; split banks".into());
            }
            files.insert(file, bytes);
            Ok(info)
        };
        for (id, effect) in &self.effects {
            let mut variants = match &effect.source {
                EffectSource::Preset { preset } => {
                    let p = Preset::from_name(preset).unwrap();
                    (0..p.variants())
                        .map(|v| Stereo::mono(synth::render(p, v, self.seed)))
                        .collect::<Vec<_>>()
                }
                EffectSource::Score { score } => vec![sum(score_render(score, false)
                    .into_iter()
                    .map(|(_, s)| s)
                    .collect())],
                EffectSource::Wav { file } => {
                    let bytes = read_bounded(&root.join(file), MAX_BYTES)?;
                    let (channels, samples) =
                        checked_wav(&bytes).map_err(|e| format!("{file}: {e}"))?;
                    let s = if channels == 1 {
                        Stereo::mono(samples)
                    } else {
                        Stereo {
                            left: samples.iter().step_by(2).copied().collect(),
                            right: samples.iter().skip(1).step_by(2).copied().collect(),
                        }
                    };
                    if s.left.len() > (MAX_SECONDS * RATE as f32) as usize {
                        return Err("import exceeds 180 seconds".into());
                    }
                    vec![s]
                }
            };
            let peak = variants.iter().map(Stereo::peak).fold(0., f32::max);
            let scale = effect.gain * (self.headroom / peak.max(0.00001)).min(1.);
            let mut entries = Vec::new();
            for (v, s) in variants.iter_mut().enumerate() {
                s.gain(scale);
                entries.push(store(format!("sfx-{id}-{v}.wav"), s)?);
            }
            manifest.effects.insert(id.clone(), entries);
        }
        if let Some(music) = &self.music {
            let mut layers = match music {
                Music::Clips {
                    seconds,
                    crossfade_seconds,
                    layers,
                } => {
                    let frames = (*seconds * RATE as f32).round() as usize;
                    let fade = (*crossfade_seconds * RATE as f32).round() as usize;
                    let mut out = Vec::new();
                    for (id, clip) in layers {
                        let bytes = read_bounded(&root.join(&clip.file), MAX_BYTES)?;
                        let (channels, samples) =
                            checked_wav(&bytes).map_err(|e| format!("{}: {e}", clip.file))?;
                        let length = samples.len() / usize::from(channels);
                        if length < frames + fade || length > (MAX_SECONDS * RATE as f32) as usize {
                            return Err(format!(
                                "{}: needs loop + crossfade samples, at most 180 seconds",
                                clip.file
                            ));
                        }
                        let sample = |frame: usize, channel: usize| {
                            samples[frame * usize::from(channels)
                                + channel.min(usize::from(channels) - 1)]
                        };
                        let channel = |c| {
                            (0..frames)
                                .map(|i| {
                                    let value = if i < fade {
                                        let t = i as f32 / (fade - 1) as f32;
                                        sample(frames + i, c) * (1. - t) + sample(i, c) * t
                                    } else {
                                        sample(i, c)
                                    };
                                    value * clip.gain
                                })
                                .collect()
                        };
                        out.push((
                            id.clone(),
                            Stereo {
                                left: channel(0),
                                right: channel(1),
                            },
                        ));
                    }
                    out
                }
                Music::Score { score } => score_render(score, true),
                Music::Generated {
                    bpm,
                    bars,
                    root_midi,
                    minor,
                } => {
                    let stems = synth::music_loop(&MusicSpec {
                        bpm: *bpm,
                        bars: *bars,
                        root_midi: *root_midi,
                        minor: *minor,
                        seed: self.seed,
                    });
                    vec![
                        ("base".into(), Stereo::mono(stems.base)),
                        ("melodic".into(), Stereo::mono(stems.melodic)),
                        ("lead".into(), Stereo::mono(stems.lead)),
                    ]
                }
                Music::Ambient {
                    seconds,
                    root_midi,
                    minor,
                    chords,
                    brightness,
                } => vec![(
                    "ambient".into(),
                    Stereo::mono(synth::ambient_loop(&AmbientSpec {
                        minutes: seconds / 60.,
                        root_midi: *root_midi,
                        minor: *minor,
                        chords: *chords,
                        brightness: *brightness,
                        seed: self.seed,
                    })),
                )],
            };
            // Sum absolute amplitudes, not the signed mix: every subset of adaptive layers is safe.
            let mut worst = 0f32;
            for i in 0..layers[0].1.left.len() {
                worst = worst
                    .max(layers.iter().map(|(_, s)| s.left[i].abs()).sum())
                    .max(layers.iter().map(|(_, s)| s.right[i].abs()).sum());
            }
            manifest.music_scale = (self.headroom / worst.max(0.00001)).min(1.);
            for (id, s) in &mut layers {
                s.gain(manifest.music_scale);
                manifest
                    .music
                    .insert(id.clone(), store(format!("music-{id}.wav"), s)?);
            }
            let preview = sum(layers.into_iter().map(|(_, s)| s).collect());
            store("preview-mix.wav".into(), &preview)?;
        }
        Ok(RenderedProject { manifest, files })
    }
}
fn sum(layers: Vec<Stereo>) -> Stereo {
    let n = layers[0].left.len();
    let mut out = Stereo {
        left: vec![0.; n],
        right: vec![0.; n],
    };
    for s in layers {
        for i in 0..n {
            out.left[i] += s.left[i];
            out.right[i] += s.right[i];
        }
    }
    out
}

impl AudioBundle {
    /// Validate metadata loaded through any platform, including browser asset fetches.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 1024 * 1024 {
            return Err("audio metadata exceeds 1 MiB".into());
        }
        let bank: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        bank.validate()?;
        Ok(bank)
    }
    /// Check one fetched payload against the same native bundle contract before decoding.
    pub fn verify_file(info: &AudioFile, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() as u64 > MAX_BYTES {
            return Err("audio payload exceeds 64 MiB".into());
        }
        let (channels, samples) = checked_wav(bytes).map_err(|e| format!("{}: {e}", info.file))?;
        if hash(bytes) != info.checksum || samples.len() / usize::from(channels) != info.frames {
            return Err(format!(
                "{}: checksum or sample-count mismatch; render the project again",
                info.file
            ));
        }
        Ok(())
    }
    /// Small metadata load; PCM reads/checks belong on the audio worker (`AudioBank::load`).
    pub fn load(root: &Path) -> Result<Self, String> {
        let bytes = read_bounded(&root.join("bank.json"), 1024 * 1024)?;
        Self::parse(&bytes)
    }
    fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || self.sample_rate != RATE
            || self.effects.len() > 64
            || self.music.len() > 16
            || (self.effects.is_empty() && self.music.is_empty())
        {
            return Err("invalid audio bundle version/rate/count".into());
        }
        let mut frames = None;
        for (id, variants) in &self.effects {
            name(id)?;
            if variants.is_empty() || variants.len() > 4 {
                return Err(format!("{id}: bundle needs 1..4 variants"));
            }
        }
        for (id, info) in &self.music {
            name(id)?;
            if frames
                .replace(info.frames)
                .is_some_and(|n| n != info.frames)
            {
                return Err("music layers have unequal sample lengths".into());
            }
        }
        range("music scale", self.music_scale, 0., 1.)?;
        let mut files = std::collections::BTreeSet::new();
        for info in self.effects.values().flatten().chain(self.music.values()) {
            safe_relative(&info.file)?;
            if !files.insert(&info.file) {
                return Err("duplicate audio bundle file".into());
            }
            if info.checksum.len() != 16 || !info.checksum.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("invalid audio checksum".into());
            }
            range("file peak", info.peak, 0., 0.95)?;
            range("file rms", info.rms, 0., 0.95)?;
            range("file dc", info.dc, 0., 0.02)?;
            range("file seam", info.seam_jump, 0., 1.9)?;
            if info.frames == 0 || info.frames > (MAX_SECONDS * RATE as f32) as usize {
                return Err("invalid audio bundle sample count".into());
            }
        }
        Ok(())
    }
    /// Verify every checksum, PCM format and sample count before anything reaches a device decoder.
    pub fn read_audio(&self, root: &Path) -> Result<BundleAudio, String> {
        self.validate()?;
        let mut total = 0;
        let mut read = |info: &AudioFile| -> Result<Vec<u8>, String> {
            let bytes = read_bounded(&root.join(&info.file), MAX_BYTES)?;
            total += bytes.len() as u64;
            if total > MAX_BYTES {
                return Err("audio bundle exceeds 64 MiB".into());
            }
            let (channels, samples) =
                checked_wav(&bytes).map_err(|e| format!("{}: {e}", info.file))?;
            if hash(&bytes) != info.checksum || samples.len() / usize::from(channels) != info.frames
            {
                return Err(format!(
                    "{}: checksum or sample-count mismatch; render the project again",
                    info.file
                ));
            }
            Ok(bytes)
        };
        let effects = self
            .effects
            .values()
            .map(|v| v.iter().map(&mut read).collect())
            .collect::<Result<Vec<_>, String>>()?;
        let music = self
            .music
            .values()
            .map(&mut read)
            .collect::<Result<Vec<_>, String>>()?;
        Ok(BundleAudio { effects, music })
    }
}
/// Byte payload sorted in exactly the same name order as the bundle's BTreeMaps.
pub struct BundleAudio {
    pub effects: Vec<Vec<Vec<u8>>>,
    pub music: Vec<Vec<u8>>,
}

impl RenderedProject {
    /// Reserve a new directory, then publish its manifest last. A reader never accepts an unfinished
    /// bundle. Never overwrite an existing directory; failure removes only our reserved directory.
    pub fn write_new(&self, out: &Path) -> Result<(), String> {
        self.manifest.validate()?;
        for file in self.files.keys() {
            safe_relative(file)?;
            if file.contains('/') || file == "bank.json" {
                return Err(
                    "rendered bundle filenames must be plain files, excluding bank.json".into(),
                );
            }
        }
        for info in self
            .manifest
            .effects
            .values()
            .flatten()
            .chain(self.manifest.music.values())
        {
            let bytes = self
                .files
                .get(&info.file)
                .ok_or_else(|| format!("missing rendered file {}", info.file))?;
            if hash(bytes) != info.checksum {
                return Err(format!("changed rendered file {}", info.file));
            }
        }
        if out.exists() {
            return Err(format!(
                "{} exists; choose a new bundle directory",
                out.display()
            ));
        }
        let parent = out
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        if !parent.is_dir() {
            return Err("bundle parent directory must exist".into());
        }
        std::fs::create_dir(out).map_err(|e| e.to_string())?;
        let result = (|| {
            for (file, bytes) in &self.files {
                std::fs::write(out.join(file), bytes).map_err(|e| e.to_string())?;
            }
            std::fs::write(
                out.join("bank.json"),
                serde_json::to_vec_pretty(&self.manifest).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(out);
        }
        result
    }
}

/// Compact discoverable contract; detailed DSP stays opt-in in `devkit::synth`.
pub fn describe() -> serde_json::Value {
    serde_json::json!({"version":1,"commands":["audio validate PROJECT.json","audio render PROJECT.json NEW_DIRECTORY","audio check BUNDLE_DIRECTORY"],"presets":Preset::ALL.map(|p|p.name()),"effects":["preset: preset name, all variants","score: beat-based polyphony","wav: relative 44100 Hz PCM16 mono/stereo"],"music":["generated: bpm,bars,root_midi,minor (base/melodic/lead)","ambient: seconds,root_midi,minor,chords,brightness","score: bpm,beats,layers; tails wrap","clips: seconds,crossfade_seconds,layers {name:{file,gain}}; checked PCM16 loops with wrap crossfade"],"instrument":{"wave":["sine","triangle","saw","square"],"envelope_seconds":["attack","decay","release"],"sustain":"0..1","lowpass_hz":"optional 40..20000"},"notes":"at/beats in beats; midi 12..108; velocity 0..1; pan -1..1","bounds":"64 effects, 16 layers, 4096 notes/score, 180 stereo-seconds synthesis budget, 64 MiB bundle","example":"assets/audio/observatory/project.json","runtime":"kit::audio::AudioBank::load; poll; play(name,volume); music(dt,[(layer,level)])","quality":"per-file peak/rms/DC/seam measurements; adaptive subset headroom; measurements do not establish subjective quality","limitations":"no live spatial/pitch voice control; backend does not report device audibility or sample-clock alignment"})
}
