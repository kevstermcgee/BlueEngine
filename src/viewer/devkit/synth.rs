//! Procedural audio for games that ship no recordings: a small DSP toolkit, a WAV writer and parser,
//! 23 generic sound-effect presets and a modest music-loop generator, in plain std Rust.
//!
//! Nobody has to record or hear anything. Every sound is computed from oscillators, filtered noise and
//! envelopes, so an AI agent (or anyone without a microphone) can give a new game a usable soundscape in
//! a few calls, and can *measure* what came out ([`peak`], [`rms`], [`tone_level`], [`dominant_hz`],
//! [`spectral_centroid`]) instead of listening to it. The module is pure: it never opens a sound device
//! or a file. It returns mono `f32` samples in `[-1, 1]` at [`RATE`], or WAV bytes ([`wav_bytes`]) for
//! any audio backend.
//!
//! * [`render`] computes a ready-made [`Preset`] (click, coin, hit, explosion, ...), a few variants each.
//! * The toolkit builds your own: [`osc`] and the alias-free [`osc_bl`], [`noise`], [`filter`], [`Mix`],
//!   envelopes ([`decay`], [`ad`], [`adsr`]), [`shape`], [`soft_clip`] and [`finish`], which makes the
//!   ends click-free and sets the peak.
//! * [`music_loop`] renders a seamless looping backing track as three stems. It is a starting point that
//!   was checked by measurement (see the tests), not by ear: keep it or replace it with real music.
//! * [`ambient_loop`] renders a seamless, beat-free pad-and-air loop for background listening (sleep,
//!   focus, a menu) instead of a song; `be2-tools ambient-music OUT.wav` writes one straight to a file.
//! * [`wav_bytes`], [`wav_bytes_stereo`] and [`parse_wav`] convert between samples and 16-bit PCM WAV.
//!
//! Everything is deterministic: the same arguments give the same samples on the same platform. (The
//! standard library's `sin`/`exp` may differ in the last bit between platforms, so do not compare
//! rendered audio across machines bit for bit.)
//!
//! ```
//! use vesper3d::viewer::devkit::synth::{decay, finish, note, render, sine, wav_bytes, Preset, RATE};
//!
//! // A ready-made effect: `variant` picks a designed variation, the seed adds a little jitter.
//! let coin = render(Preset::Coin, 0, 7);
//! assert!(coin.len() > 1_000);
//!
//! // Or build one: a 660 Hz sine ping with a 60 ms decay, faded and normalised to a 0.5 peak.
//! let ping = finish(note(0.3, 660.0, sine, |t| decay(t, 0.06)), 0.5, 20.0);
//!
//! // Hand the bytes to any audio backend, or write them to a .wav file.
//! let wav = wav_bytes(&ping, RATE);
//! assert_eq!(&wav[..4], b"RIFF");
//! ```
use super::rng::Rng;
use std::f32::consts::{PI, TAU};

/// Sample rate of everything this module renders, in Hz. Example: `n_samples(1.0)` is `RATE as usize`.
pub const RATE: u32 = 44_100;
const SR: f32 = RATE as f32;
/// Longest buffer any constructor allocates (ten minutes), so a typo such as `Mix::new(1e9)` cannot
/// exhaust memory.
const MAX_SECONDS: f32 = 600.;

// ------------------------------------------------------------------------------------ basics

/// Number of samples in `seconds` of audio at [`RATE`]. Rounded; negative and NaN input give 0 and the
/// result is capped at ten minutes. Example: `n_samples(0.5)` is `22_050`.
pub fn n_samples(seconds: f32) -> usize {
    (seconds.clamp(0., MAX_SECONDS) * SR).round() as usize
}

/// Frequency in Hz of a MIDI note number; fractions detune. Example: `midi(69.0)` is `440.0` (A4) and
/// `midi(60.0)` is about `261.6` (middle C).
pub fn midi(note: f32) -> f32 {
    440. * ((note - 69.) / 12.).exp2()
}

// -------------------------------------------------------------------------------- waveforms

/// Sine wave at `phase` in `[0, 1)`, starting at 0 and rising. Example: `sine(0.25)` is `1.0`.
pub fn sine(phase: f32) -> f32 {
    (phase * TAU).sin()
}

/// Rising sawtooth at `phase` in `[0, 1)`: -1 at the start, +1 at the end. Example: `saw(0.75)` is `0.5`.
pub fn saw(phase: f32) -> f32 {
    2. * phase - 1.
}

/// Square wave (50% duty): +1 for the first half of the cycle, -1 for the second. Example:
/// `square(0.1)` is `1.0` and `square(0.6)` is `-1.0`.
pub fn square(phase: f32) -> f32 {
    pulse(phase, 0.5)
}

/// Pulse wave: +1 while `phase < width`, else -1. Thin widths (0.1 to 0.25) sound nasal. Example:
/// `pulse(0.2, 0.25)` is `1.0` and `pulse(0.3, 0.25)` is `-1.0`.
pub fn pulse(phase: f32, width: f32) -> f32 {
    if phase < width {
        1.
    } else {
        -1.
    }
}

/// Triangle wave at `phase` in `[0, 1)`, starting at 0 and rising like [`sine`]. Example:
/// `triangle(0.25)` is `1.0` and `triangle(0.75)` is `-1.0`.
pub fn triangle(phase: f32) -> f32 {
    if phase < 0.25 {
        4. * phase
    } else if phase < 0.75 {
        2. - 4. * phase
    } else {
        4. * phase - 4.
    }
}

// ---------------------------------------------------------------------- oscillators and noise

/// Oscillator whose frequency in Hz follows `freq(t)` (`t` in seconds). The phase is accumulated, so
/// glides and vibrato are click-free. `wave` maps a phase in `[0, 1)` to a sample, for example [`sine`]
/// or `|p| pulse(p, 0.25)`. Non-finite frequencies hold the phase, and negative ones run it backwards.
/// Example: `osc(0.5, |_| 440.0, sine)` is half a second of A4.
pub fn osc(dur: f32, freq: impl Fn(f32) -> f32, wave: impl Fn(f32) -> f32) -> Vec<f32> {
    osc_dyn(dur, &freq, &wave)
}

// The public combinators are thin generic shims over non-generic bodies taking `&dyn Fn`: a game builds
// hundreds of voices from unique closures, and instantiating every loop per closure would multiply the
// compile time of the whole crate for no audible gain (rendering is far faster than real time either way).
fn osc_dyn(dur: f32, freq: &dyn Fn(f32) -> f32, wave: &dyn Fn(f32) -> f32) -> Vec<f32> {
    let n = n_samples(dur);
    let mut phase = 0f32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        out.push(wave(phase));
        let step = freq((i + 1) as f32 / SR) / SR;
        if step.is_finite() {
            phase += step;
            phase -= phase.floor();
        }
    }
    out
}

/// A band-limited waveform for [`osc_bl`]. Example: `Wave::Pulse(0.25)` is a thin, nasal pulse.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wave {
    /// Rising sawtooth, like [`saw`].
    Saw,
    /// Square wave, like [`square`].
    Square,
    /// Pulse wave of the given width (clamped to `0.02..=0.98`), like [`pulse`] but with its DC offset
    /// removed (a plain pulse of width `w` has mean `2w - 1`), so a thin pulse does not thump. Its range
    /// is `-2w ..= 2 - 2w`.
    Pulse(f32),
}

/// Like [`osc`] for the classic hard-edged waveforms, but band-limited with PolyBLEP: the edges are
/// smoothed just enough that overtones above half the sample rate no longer fold back down as
/// inharmonic whistles. A naive [`saw`] or [`square`] at 2 kHz folds back overtones that add up to only
/// about 12 to 15 dB below the note; this puts them about 15 dB further down, so use it for anything
/// high or sweeping. `freq(t)` is in Hz as for [`osc`]; a negative frequency counts as its absolute
/// value, a zero or non-finite one as silence, and anything at or above 0.49 x [`RATE`] is silent (it
/// would only alias). `Saw` and `Square` start at exactly 0 (a `Pulse` starts part-way up its edge), so
/// shape the result with an envelope that has an attack, as [`note`] does. Example:
/// `osc_bl(0.5, |_| 2500.0, Wave::Saw)` is half a second of a clean 2.5 kHz sawtooth.
pub fn osc_bl(dur: f32, freq: impl Fn(f32) -> f32, wave: Wave) -> Vec<f32> {
    osc_bl_dyn(dur, &freq, wave)
}

fn osc_bl_dyn(dur: f32, freq: &dyn Fn(f32) -> f32, wave: Wave) -> Vec<f32> {
    let n = n_samples(dur);
    let width = match wave {
        Wave::Pulse(w) if w.is_nan() => 0.5,
        Wave::Pulse(w) => w.clamp(0.02, 0.98),
        _ => 0.5,
    };
    let mut phase = 0f32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let hz = freq(i as f32 / SR);
        let hz = if hz.is_finite() { hz.abs() } else { 0. };
        // Silent at 0 Hz (a frozen oscillator would be a DC step) and at or above 0.49 x rate (it would
        // only alias); the phase holds meanwhile.
        if hz <= 0. || hz >= 0.49 * SR {
            out.push(0.);
            continue;
        }
        let dt = hz / SR;
        let sample = match wave {
            Wave::Saw => 2. * phase - 1. - poly_blep(phase, dt),
            Wave::Square | Wave::Pulse(_) => {
                let edge = phase + 1. - width;
                let level = if phase < width { 1. } else { -1. };
                let mut v = level + poly_blep(phase, dt) - poly_blep(edge - edge.floor(), dt);
                if matches!(wave, Wave::Pulse(_)) {
                    v -= 2. * width - 1.;
                }
                v
            }
        };
        out.push(sample);
        phase += dt;
        phase -= phase.floor();
    }
    out
}

/// The PolyBLEP correction for a step at phase 0 (`t` in `[0, 1)`, `dt` the phase advance per sample).
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        2. * x - x * x - 1.
    } else if t > 1. - dt {
        let x = (t - 1.) / dt;
        x * x + 2. * x + 1.
    } else {
        0.
    }
}

/// Exponential glide from `f0` to `f1` Hz over `over` seconds, then held: a frequency function for
/// [`osc`]. Pitch glides are exponential because that is how pitch is heard. Example:
/// `glide(100.0, 400.0, 1.0)(0.5)` is `200.0` (two octaves in a second, half-way is one octave).
pub fn glide(f0: f32, f1: f32, over: f32) -> impl Fn(f32) -> f32 {
    let (f0, f1, over) = (f0.max(1e-3), f1.max(1e-3), over.max(1e-6));
    let ratio = f1 / f0;
    move |t| f0 * ratio.powf((t / over).clamp(0., 1.))
}

/// White noise in `[-1, 1)` from the seeded generator, so the same [`Rng`] state gives the same noise.
/// Example: `noise(0.1, &mut Rng::new(1))` is 4410 samples.
pub fn noise(dur: f32, rng: &mut Rng) -> Vec<f32> {
    (0..n_samples(dur)).map(|_| rng.f32() * 2. - 1.).collect()
}

// ------------------------------------------------------------------------------------ filters

/// Which response a [`Biquad`] has. Example: `filter(&x, FilterKind::Low, |_| 800.0, 0.7)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FilterKind {
    /// Passes below the cutoff.
    Low,
    /// Passes above the cutoff.
    High,
    /// Passes a band around the cutoff (constant 0 dB peak gain, so `q` sets only the width).
    Band,
}

/// A second-order (12 dB per octave) filter from the RBJ "Audio EQ Cookbook", computed in `f64` so that
/// low cutoffs and high Q stay accurate. Cutoffs are clamped to 20 Hz .. 0.45 x [`RATE`] and Q to
/// 0.1 .. 1000; NaN parameters fall back to 1 kHz and 0.707, and non-finite input samples count as
/// silence, so the output is always finite. Example: `Biquad::new(FilterKind::Low, 1000.0, 0.707)`.
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    /// A filter of `kind` with cutoff (or centre) `fc` in Hz and quality `q` (0.707 is flat and
    /// natural; higher rings or narrows). Example: `Biquad::new(FilterKind::High, 200.0, 0.7)`.
    pub fn new(kind: FilterKind, fc: f32, q: f32) -> Self {
        let mut f = Self {
            b0: 0.,
            b1: 0.,
            b2: 0.,
            a1: 0.,
            a2: 0.,
            x1: 0.,
            x2: 0.,
            y1: 0.,
            y2: 0.,
        };
        f.set(kind, fc, q);
        f
    }

    /// Retune without clearing the state, so a sweeping cutoff stays continuous. Example:
    /// `filter.set(FilterKind::Low, 800.0, 1.0)`.
    pub fn set(&mut self, kind: FilterKind, fc: f32, q: f32) {
        let fc = if fc.is_nan() {
            1000.
        } else {
            fc.clamp(20., SR * 0.45)
        };
        let q = if q.is_nan() {
            0.707
        } else {
            q.clamp(0.1, 1000.)
        };
        let w0 = std::f64::consts::TAU * f64::from(fc) / f64::from(SR);
        let (s, c) = w0.sin_cos();
        let alpha = s / (2. * f64::from(q));
        let (b0, b1, b2) = match kind {
            FilterKind::Low => ((1. - c) * 0.5, 1. - c, (1. - c) * 0.5),
            FilterKind::High => ((1. + c) * 0.5, -(1. + c), (1. + c) * 0.5),
            FilterKind::Band => (alpha, 0., -alpha),
        };
        let a0 = 1. + alpha;
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = -2. * c / a0;
        self.a2 = (1. - alpha) / a0;
    }

    /// Clear the memory (the next output ignores everything fed so far). Example: `filter.reset()`.
    pub fn reset(&mut self) {
        self.x1 = 0.;
        self.x2 = 0.;
        self.y1 = 0.;
        self.y2 = 0.;
    }

    /// Filter one sample. Example: `let y = filter.process(x);`.
    pub fn process(&mut self, x: f32) -> f32 {
        let x = if x.is_finite() { f64::from(x) } else { 0. };
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        // Flush denormals and never let a non-finite value into the feedback path.
        let y = if y.is_finite() && y.abs() >= 1e-20 {
            y
        } else {
            0.
        };
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        let out = y as f32;
        if out.is_finite() {
            out
        } else {
            0.
        }
    }
}

/// Filter `input` with a cutoff that follows `fc(t)` (Hz, `t` in seconds); the coefficients are refreshed
/// every 16 samples. Example: `filter(&x, FilterKind::Low, |t| 4000.0 - 3000.0 * t, 0.9)` closes a
/// lowpass from 4 kHz downwards, one kHz per third of a second.
pub fn filter(input: &[f32], kind: FilterKind, fc: impl Fn(f32) -> f32, q: f32) -> Vec<f32> {
    filter_dyn(input, kind, &fc, q)
}

fn filter_dyn(input: &[f32], kind: FilterKind, fc: &dyn Fn(f32) -> f32, q: f32) -> Vec<f32> {
    let mut f = Biquad::new(kind, fc(0.), q);
    input
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            if i % 16 == 0 {
                f.set(kind, fc(i as f32 / SR), q);
            }
            f.process(x)
        })
        .collect()
}

fn fixed_filter(input: &[f32], kind: FilterKind, fc: f32, q: f32) -> Vec<f32> {
    let mut f = Biquad::new(kind, fc, q);
    input.iter().map(|&x| f.process(x)).collect()
}

/// Lowpass at `fc` Hz: keeps rumble and body, removes hiss. Example: `lowpass(&noise, 400.0, 0.7)`.
pub fn lowpass(input: &[f32], fc: f32, q: f32) -> Vec<f32> {
    fixed_filter(input, FilterKind::Low, fc, q)
}

/// Highpass at `fc` Hz: keeps clicks and air, removes rumble. Example: `highpass(&noise, 3000.0, 0.7)`.
pub fn highpass(input: &[f32], fc: f32, q: f32) -> Vec<f32> {
    fixed_filter(input, FilterKind::High, fc, q)
}

/// Bandpass centred on `fc` Hz; a higher `q` is a narrower, more tonal band. Example:
/// `bandpass(&noise, 1500.0, 2.0)`.
pub fn bandpass(input: &[f32], fc: f32, q: f32) -> Vec<f32> {
    fixed_filter(input, FilterKind::Band, fc, q)
}

// ------------------------------------------------------------------------------ mixing buffer

/// A fixed-length mixing buffer: sources are added at a time offset with a gain, and anything past the
/// end is dropped ([`Mix::add`]) or wrapped to the start ([`Mix::add_wrapped`], for seamless loops).
/// Example: `let mut m = Mix::new(1.0); m.add(0.25, &ping, 0.8); let out = m.into_vec();`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mix {
    s: Vec<f32>,
}

impl Mix {
    /// A silent buffer of `seconds` (capped at ten minutes). Example: `Mix::new(0.5).len()` is `22_050`.
    pub fn new(seconds: f32) -> Self {
        Self::with_len(n_samples(seconds))
    }

    /// A silent buffer of exactly `samples`. Example: `Mix::with_len(100).len()` is `100`.
    pub fn with_len(samples: usize) -> Self {
        Self {
            s: vec![0.; samples.min(n_samples(MAX_SECONDS))],
        }
    }

    /// Length in samples. Example: `Mix::new(1.0).len()` is `44_100`.
    pub fn len(&self) -> usize {
        self.s.len()
    }

    /// True for a zero-length buffer. Example: `Mix::new(0.0).is_empty()` is `true`.
    pub fn is_empty(&self) -> bool {
        self.s.is_empty()
    }

    /// The samples mixed so far. Example: `m.as_slice()[0]`.
    pub fn as_slice(&self) -> &[f32] {
        &self.s
    }

    /// Add `src * gain` starting `at` seconds in (negative times start at 0). The part that would run
    /// past the end is dropped. Example: `m.add(0.1, &ping, 0.5)`.
    pub fn add(&mut self, at: f32, src: &[f32], gain: f32) {
        self.add_at(n_samples(at), src, gain);
    }

    /// Like [`Mix::add`] but at an exact sample index. Example: `m.add_at(4410, &ping, 0.5)`.
    pub fn add_at(&mut self, start: usize, src: &[f32], gain: f32) {
        if start >= self.s.len() {
            return;
        }
        for (d, v) in self.s[start..].iter_mut().zip(src) {
            *d += v * gain;
        }
    }

    /// Add with wrap-around, so a tail that runs past the end lands at the start: how loops are built
    /// seamlessly. Example: in a 1 s buffer, `m.add_wrapped(0.9, &two_second_tail, 1.0)` wraps twice.
    pub fn add_wrapped(&mut self, at: f32, src: &[f32], gain: f32) {
        self.add_wrapped_at(n_samples(at), src, gain);
    }

    /// Like [`Mix::add_wrapped`] but at an exact sample index (taken modulo the length). Example:
    /// `m.add_wrapped_at(44_000, &tail, 1.0)`.
    pub fn add_wrapped_at(&mut self, start: usize, src: &[f32], gain: f32) {
        let n = self.s.len();
        if n == 0 {
            return;
        }
        let mut idx = start % n;
        for v in src {
            self.s[idx] += v * gain;
            idx += 1;
            if idx == n {
                idx = 0;
            }
        }
    }

    /// Take the mixed samples. Example: `let samples = m.into_vec();`.
    pub fn into_vec(self) -> Vec<f32> {
        self.s
    }
}

// ------------------------------------------------------------------------------- envelopes

/// Exponential decay with time constant `tau` seconds: 1 at `t = 0`, `1/e` (0.37) after `tau`, and 1
/// before 0. Example: `decay(0.1, 0.1)` is about `0.368`.
pub fn decay(t: f32, tau: f32) -> f32 {
    (-t.max(0.) / tau.max(1e-6)).exp()
}

/// Linear attack to 1 over `attack` seconds, then an exponential decay with time constant `tau`: the
/// workhorse percussive envelope. Example: `ad(0.005, 0.01, 0.1)` is `0.5` and `ad(0.01, 0.01, 0.1)` is
/// `1.0`.
pub fn ad(t: f32, attack: f32, tau: f32) -> f32 {
    let t = t.max(0.);
    if t < attack {
        t / attack.max(1e-6)
    } else {
        decay(t - attack, tau)
    }
}

/// Linear attack-decay-sustain-release envelope for a note held until `gate` seconds: rises to 1 over
/// `attack`, falls to `sustain` (`0..=1`) over `decay_time`, holds, and after `gate` falls to 0 over
/// `release` (from wherever it was, so a short note releases cleanly). Example:
/// `adsr(0.5, 1.0, 0.01, 0.1, 0.6, 0.2)` is `0.6` (sustaining) and `adsr(1.1, 1.0, 0.01, 0.1, 0.6, 0.2)`
/// is `0.3` (half way through the release).
pub fn adsr(t: f32, gate: f32, attack: f32, decay_time: f32, sustain: f32, release: f32) -> f32 {
    let (attack, decay_time, release) = (attack.max(1e-6), decay_time.max(1e-6), release.max(1e-6));
    let sustain = sustain.clamp(0., 1.);
    let held = |x: f32| -> f32 {
        if x < attack {
            x / attack
        } else if x < attack + decay_time {
            1. - (1. - sustain) * (x - attack) / decay_time
        } else {
            sustain
        }
    };
    let (t, gate) = (t.max(0.), gate.max(0.));
    if t <= gate {
        held(t)
    } else {
        held(gate) * (1. - (t - gate) / release).max(0.)
    }
}

/// Multiply `v` by an envelope of time in seconds and return it. Example:
/// `shape(osc(0.3, |_| 440.0, sine), |t| decay(t, 0.05))` is a plucked A4.
pub fn shape(v: Vec<f32>, env: impl Fn(f32) -> f32) -> Vec<f32> {
    shape_dyn(v, &env)
}

fn shape_dyn(mut v: Vec<f32>, env: &dyn Fn(f32) -> f32) -> Vec<f32> {
    for (i, x) in v.iter_mut().enumerate() {
        *x *= env(i as f32 / SR);
    }
    v
}

/// Tanh saturation in place: adds harmonics and squashes peaks, and is scaled so that an input of 1.0
/// still comes out as 1.0. `drive` around 1.2 is gentle glue, 2 or more is a crunchy limiter. Example:
/// `soft_clip(&mut samples, 1.8)`.
pub fn soft_clip(v: &mut [f32], drive: f32) {
    let drive = if drive.is_nan() { 1. } else { drive.max(0.05) };
    let norm = drive.tanh();
    for x in v {
        *x = (*x * drive).tanh() / norm;
    }
}

/// Make a sound ready to play: scrub non-finite samples, fade the very start (1.5 ms) and the last
/// `fade_out_ms` with a half-cosine so nothing clicks, then scale so the loudest sample is exactly
/// `peak` (clamped to `0..=1`). A silent input stays silent. Example: `finish(samples, 0.8, 30.0)`.
pub fn finish(mut v: Vec<f32>, peak: f32, fade_out_ms: f32) -> Vec<f32> {
    for x in &mut v {
        if !x.is_finite() {
            *x = 0.;
        }
    }
    let n = v.len();
    let fade_in = n_samples(0.0015).min(n);
    for (i, x) in v.iter_mut().take(fade_in).enumerate() {
        *x *= half_cosine(i as f32 / fade_in as f32);
    }
    let fade_out = n_samples(fade_out_ms / 1000.).min(n);
    for (i, x) in v.iter_mut().rev().take(fade_out).enumerate() {
        *x *= half_cosine(i as f32 / fade_out as f32);
    }
    let loudest = self::peak(&v);
    if loudest > 1e-6 {
        let g = peak.clamp(0., 1.) / loudest;
        for x in &mut v {
            *x *= g;
        }
    }
    v
}

/// 0 at `k = 0` rising smoothly to 1 at `k = 1`.
fn half_cosine(k: f32) -> f32 {
    0.5 - 0.5 * (PI * k).cos()
}

// -------------------------------------------------------------------------------- instruments

/// One-shot sum of decaying sine partials, for bells, chimes and metal: each `(ratio, tau, amp)` adds
/// a sine at `base * ratio` Hz with decay time `tau` seconds and gain `amp`. The first 1 ms fades in and
/// the last 30 ms fade out, so a strike that starts mid-mix, or a bell still ringing when `dur` ends,
/// does not click. Example: `bell(1.0, 880.0, &[(1.0, 0.4, 1.0), (2.76, 0.2, 0.4)])`.
pub fn bell(dur: f32, base: f32, partials: &[(f32, f32, f32)]) -> Vec<f32> {
    let mut m = Mix::new(dur);
    for &(ratio, tau, amp) in partials {
        let hz = base * ratio;
        m.add(0., &shape(osc(dur, |_| hz, sine), |t| decay(t, tau)), amp);
    }
    let mut v = m.into_vec();
    fade_edges(&mut v, 0.001, 0.03);
    v
}

/// A single note: `wave` at `hz` for `dur` seconds shaped by the envelope `env(t)`. The first 1 ms fades
/// in and the last 5 ms fade out, so a note that starts abruptly, or is cut off while still sounding,
/// does not click. Example: `note(0.2, 440.0, triangle, |t| ad(t, 0.005, 0.08))`.
pub fn note(dur: f32, hz: f32, wave: impl Fn(f32) -> f32, env: impl Fn(f32) -> f32) -> Vec<f32> {
    let mut v = shape(osc(dur, |_| hz, wave), env);
    fade_edges(&mut v, 0.001, 0.005);
    v
}

/// Fade the first `fade_in` and the last `fade_out` seconds of `v` with a half-cosine (0 at the very
/// first and last sample).
fn fade_edges(v: &mut [f32], fade_in: f32, fade_out: f32) {
    let head = n_samples(fade_in).min(v.len());
    for (i, x) in v.iter_mut().take(head).enumerate() {
        *x *= half_cosine(i as f32 / head as f32);
    }
    let tail = n_samples(fade_out).min(v.len());
    for (i, x) in v.iter_mut().rev().take(tail).enumerate() {
        *x *= half_cosine(i as f32 / tail as f32);
    }
}

/// Average interleaved multi-channel samples into mono, for example the output of [`parse_wav`]. A
/// trailing partial frame is dropped. Example: `to_mono(&[1.0, 0.0, 0.5, 0.5], 2)` is `[0.5, 0.5]`.
pub fn to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    let c = channels.max(1);
    interleaved
        .chunks_exact(c)
        .map(|frame| frame.iter().sum::<f32>() / c as f32)
        .collect()
}

// ------------------------------------------------------------------------------- measuring

/// Largest absolute sample (0 for an empty slice). Example: `peak(&[0.1, -0.7, 0.3])` is `0.7`.
pub fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0f32, |m, x| m.max(x.abs()))
}

/// Root-mean-square level, the usual measure of loudness (0 for an empty slice). Example:
/// `rms(&[0.5, -0.5])` is `0.5`.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.;
    }
    let sum: f64 = samples.iter().map(|&x| f64::from(x) * f64::from(x)).sum();
    (sum / samples.len() as f64).sqrt() as f32
}

/// Amplitude of the sinusoid at `hz` inside `samples`, by the Goertzel algorithm: a full-scale sine at
/// exactly `hz` reads about 1.0, silence and other frequencies read near 0. Use it to check pitch and
/// tone colour without listening. Example: `tone_level(&osc(1.0, |_| 440.0, sine), 440.0)` is about `1.0`.
pub fn tone_level(samples: &[f32], hz: f32) -> f32 {
    if samples.is_empty() || !hz.is_finite() {
        return 0.;
    }
    let w = std::f64::consts::TAU * f64::from(hz) / f64::from(RATE);
    let coeff = 2. * w.cos();
    let (mut s1, mut s2) = (0f64, 0f64);
    for &x in samples {
        let s = f64::from(x) + coeff * s1 - s2;
        s2 = s1;
        s1 = s;
    }
    let power = s1 * s1 + s2 * s2 - coeff * s1 * s2;
    (2. * power.max(0.).sqrt() / samples.len() as f64) as f32
}

/// The strongest frequency between `lo` and `hi` Hz: the peak of a Hann-windowed FFT of the first 3 s
/// (131072 samples), refined between bins, so a clean tone is found to within about 1% however long
/// the clip is. Noise and chords return whichever partial is strongest. 0 for an empty slice, a
/// silent one or an empty range. Example: `dominant_hz(&osc(0.5, |_| 440.0, sine), 50.0, 5000.0)` is
/// about `440.0`.
pub fn dominant_hz(samples: &[f32], lo: f32, hi: f32) -> f32 {
    if samples.is_empty() || !(lo.is_finite() && hi.is_finite()) || hi < lo || hi <= 0. {
        return 0.;
    }
    let (mags, bin_hz) = magnitude_spectrum(samples);
    let first = ((f64::from(lo.max(0.)) / bin_hz).ceil() as usize).max(1);
    let last = ((f64::from(hi) / bin_hz).floor() as usize).min(mags.len() - 1);
    let (mut k, mut best) = (first, 0f64);
    for (i, &m) in mags.iter().enumerate().take(last + 1).skip(first) {
        if m > best {
            best = m;
            k = i;
        }
    }
    if best <= 0. {
        return 0.;
    }
    // Parabolic interpolation on the log magnitude (a Hann main lobe is close to a Gaussian).
    let log = |i: usize| mags[i].max(1e-30).ln();
    let mut offset = 0.;
    if k > 0 && k + 1 < mags.len() {
        let (a, b, c) = (log(k - 1), log(k), log(k + 1));
        let curve = a - 2. * b + c;
        if curve.abs() > 1e-12 {
            offset = (0.5 * (a - c) / curve).clamp(-0.5, 0.5);
        }
    }
    ((k as f64 + offset) * bin_hz) as f32
}

/// The power-weighted mean frequency in Hz of the first 3 s (the "centre of mass" of the spectrum): a
/// single number for how bright a sound is. A pure tone reads its own frequency, a thud reads under
/// 100 Hz, a bright ping a few kHz and white noise about 11 kHz. 0 for an empty or silent slice.
/// Example: `spectral_centroid(&osc(0.5, |_| 1000.0, sine))` is about `1000.0`.
pub fn spectral_centroid(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.;
    }
    let (mags, bin_hz) = magnitude_spectrum(samples);
    let (mut weighted, mut total) = (0f64, 0f64);
    for (k, m) in mags.iter().enumerate().skip(1) {
        weighted += m * m * k as f64 * bin_hz;
        total += m * m;
    }
    if total > 0. {
        (weighted / total) as f32
    } else {
        0.
    }
}

/// Hann-windowed magnitude spectrum (bins 0 to Nyquist) of the first 131072 samples, zero-padded to at
/// least twice their length, and the width of one bin in Hz.
fn magnitude_spectrum(samples: &[f32]) -> (Vec<f64>, f64) {
    let n = samples.len().min(1 << 17);
    let size = (n * 2).next_power_of_two().max(1 << 12);
    let mut re = vec![0f64; size];
    let mut im = vec![0f64; size];
    for (i, (slot, &x)) in re.iter_mut().zip(&samples[..n]).enumerate() {
        let window = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos();
        *slot = f64::from(x) * window;
    }
    fft(&mut re, &mut im);
    let mags = re
        .iter()
        .zip(&im)
        .take(size / 2 + 1)
        .map(|(r, i)| r.hypot(*i))
        .collect();
    (mags, f64::from(RATE) / size as f64)
}

/// In-place radix-2 FFT; both slices must have the same power-of-two length.
fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -std::f64::consts::TAU / len as f64;
        let (step_re, step_im) = (angle.cos(), angle.sin());
        for start in (0..n).step_by(len) {
            let (mut w_re, mut w_im) = (1f64, 0f64);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let t_re = re[b] * w_re - im[b] * w_im;
                let t_im = re[b] * w_im + im[b] * w_re;
                re[b] = re[a] - t_re;
                im[b] = im[a] - t_im;
                re[a] += t_re;
                im[a] += t_im;
                let next = w_re * step_re - w_im * step_im;
                w_im = w_re * step_im + w_im * step_re;
                w_re = next;
            }
        }
        len <<= 1;
    }
}

/// Zero crossings per second divided by two: equals the frequency of a pure tone and rises with the
/// brightness of anything else, so comparing the start and end of a sound shows whether it sweeps up or
/// down. Example: `zero_crossing_hz(&osc(1.0, |_| 1000.0, sine))` is about `1000.0`.
pub fn zero_crossing_hz(samples: &[f32]) -> f32 {
    if samples.len() < 2 {
        return 0.;
    }
    let crossings = samples
        .windows(2)
        .filter(|w| (w[0] >= 0.) != (w[1] >= 0.))
        .count();
    crossings as f32 * SR / (2. * samples.len() as f32)
}

// ----------------------------------------------------------------------------------- WAV files

/// Wrap mono samples as a 16-bit PCM WAV file at `rate` Hz: the bytes of a complete `.wav` that any
/// audio backend can decode. Samples are clamped to `[-1, 1]` (NaN becomes silence) and `1.0` maps to
/// 32767. Example: `wav_bytes(&samples, RATE)`.
pub fn wav_bytes(samples: &[f32], rate: u32) -> Vec<u8> {
    pcm16_wav(samples, 1, rate)
}

/// Like [`wav_bytes`] for two channels. The shorter channel is padded with silence. Example:
/// `wav_bytes_stereo(&left, &right, RATE)`.
pub fn wav_bytes_stereo(left: &[f32], right: &[f32], rate: u32) -> Vec<u8> {
    let n = left.len().max(right.len());
    let mut interleaved = Vec::with_capacity(n * 2);
    for i in 0..n {
        interleaved.push(left.get(i).copied().unwrap_or(0.));
        interleaved.push(right.get(i).copied().unwrap_or(0.));
    }
    pcm16_wav(&interleaved, 2, rate)
}

fn pcm16_wav(interleaved: &[f32], channels: u16, rate: u32) -> Vec<u8> {
    let c = usize::from(channels);
    // Whole frames only, and never more than a RIFF size field can describe.
    let max_samples = (u32::MAX as usize - 36) / 2;
    let len = interleaved.len().min(max_samples) / c * c;
    let data = &interleaved[..len];
    let data_len = (len * 2) as u32;
    let block = channels * 2;
    let mut out = Vec::with_capacity(44 + len * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&rate.saturating_mul(u32::from(block)).to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in data {
        let v = (s.clamp(-1., 1.) * 32767.).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Read a 16-bit PCM WAV file into `(sample rate, channels, samples)` with the samples interleaved and
/// scaled to `[-1, 1]` (32767 is 1.0). Unknown chunks are skipped, a data chunk that claims more than the
/// file holds is read as far as it goes, and malformed input gives an `Err` naming the problem (never a
/// panic). Anything but 16-bit integer PCM is refused. Example:
/// `let (rate, channels, samples) = parse_wav(&wav_bytes(&x, RATE))?;`.
pub fn parse_wav(bytes: &[u8]) -> Result<(u32, u16, Vec<f32>), String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE file".to_string());
    }
    let mut format = None;
    let mut data = None;
    let mut pos = 12usize;
    while bytes.len().saturating_sub(pos) >= 8 && (format.is_none() || data.is_none()) {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let start = pos + 8;
        let end = start.saturating_add(size).min(bytes.len());
        let body = &bytes[start..end];
        if id == b"fmt " {
            if body.len() < 16 {
                return Err("fmt chunk is too short".to_string());
            }
            let tag = u16::from_le_bytes([body[0], body[1]]);
            let channels = u16::from_le_bytes([body[2], body[3]]);
            let rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
            let bits = u16::from_le_bytes([body[14], body[15]]);
            format = Some((tag, channels, rate, bits));
        } else if id == b"data" {
            data = Some(body);
        }
        // Chunks are padded to an even size.
        pos = start.saturating_add(size).saturating_add(size & 1);
    }
    let (tag, channels, rate, bits) = format.ok_or_else(|| "missing fmt chunk".to_string())?;
    let data = data.ok_or_else(|| "missing data chunk".to_string())?;
    if tag != 1 || bits != 16 {
        return Err(format!(
            "unsupported WAV encoding (format tag {tag}, {bits} bits): only 16-bit PCM is read"
        ));
    }
    if channels == 0 || rate == 0 {
        return Err("fmt chunk has zero channels or a zero sample rate".to_string());
    }
    let whole_frames = data.len() / 2 / usize::from(channels) * usize::from(channels);
    let samples = data
        .chunks_exact(2)
        .take(whole_frames)
        .map(|c| (f32::from(i16::from_le_bytes([c[0], c[1]])) / 32767.).max(-1.))
        .collect();
    Ok((rate, channels, samples))
}

// ----------------------------------------------------------------------------------- presets

/// A generic game sound effect that [`render`] can compute with no setup: interface sounds, pickups,
/// movement and impacts. Each variant below states its recipe in one line and how its variants differ.
/// The set carries no genre logic; treat it like an sfxr-style starter kit and swap in your own recipes
/// (built from the toolkit above) where a game needs a signature sound. Example:
/// `render(Preset::Coin, 0, 1)`; [`Preset::ALL`] lists every preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Preset {
    /// Button press: a 1.5 kHz sine tick over a 3.5 kHz-highpassed noise transient and a short low body,
    /// 70 ms. Variants (3) shift the pitch by a few percent.
    Click,
    /// Confirm: two rising chip notes a fifth apart (E5 then B5), 260 ms. Variants (2): 0 and +2
    /// semitones.
    Select,
    /// Cancel or back: the same pair falling by a fifth (B5 then E5), softer, 240 ms. Variants (2): 0
    /// and +2 semitones.
    Back,
    /// Tiny neutral beep for hover, typing or ticks: one 40%-pulse note at A5 with a 20 ms decay, 70 ms.
    /// Variants (4) climb a pentatonic ladder (+0, +2, +4, +7 semitones).
    Blip,
    /// The classic two-note pickup: a square B5 then a longer E6 with an octave shimmer, 0.5 s.
    /// Variants (4) are a rising ladder (+0, +2, +4, +7 semitones) for chains of coins.
    Coin,
    /// Item grab: a quick upward major arpeggio (C5 E5 G5 C6) of chip notes with octave sparkle, 0.42 s.
    /// Variants (4) climb the same ladder as [`Preset::Coin`].
    Pickup,
    /// Boost: a saw sweep rising from 220 to 1320 Hz through an opening lowpass with vibrato, ending in
    /// a bell sparkle, 0.7 s. Variants (2): 0 and +2 semitones.
    PowerUp,
    /// Hop: a sine plus triangle chirp rising from 250 to 540 Hz in 90 ms, 200 ms. Variants (3) shift the
    /// pitch by about a semitone up or down.
    Jump,
    /// Touchdown: a 115 to 52 Hz sine drop and a 700 Hz-lowpassed noise puff, 0.3 s. Variants (3) shift
    /// the pitch.
    Land,
    /// Punchy impact: a 230 to 75 Hz body drop, a mid slap, a highpassed noise crack and a triangle
    /// smack, 0.32 s. Variants (3) shift the pitch and change the noise texture.
    Hit,
    /// Heavy low impact (a door, a boulder): a 120 to 48 Hz sub drop, a 300 to 110 Hz knock and a
    /// 380 Hz-lowpassed noise thud, soft-clipped so small speakers hear its harmonics, 0.55 s. Variants
    /// (3) shift the pitch.
    Thump,
    /// Big boom: a 100 to 32 Hz sine, a noise blast swept from 5.2 kHz down, a low rumble, a crack and
    /// thinning crackle, soft-clipped, 1.4 s. Variants (3) shift the pitch and change the noise.
    Explosion,
    /// Electric zap: a 2.8 kHz to 160 Hz saw dive with an FM wobble, a 90 Hz amplitude buzz and crackle,
    /// 0.4 s. Variants (3) shift the pitch.
    Zap,
    /// Laser or gun "pew": a pulse dive from 1.9 kHz to 200 Hz, a noise snap and a body thump, 0.24 s.
    /// Variants (3) shift the pitch.
    Shoot,
    /// Swish or dash: bandpassed noise whose centre rises from 300 Hz to 3.9 kHz under a swell-then-fade
    /// envelope, 0.55 s. Variants (3) shift the band.
    Whoosh,
    /// Denied buzzer: two low, detuned square-and-saw buzzes falling from 196 to 156 Hz, 0.27 s. Variants
    /// (2) shift the pitch.
    Error,
    /// Alarm: four alternating high-low square beeps (B5, F#5, B5, F#5), 0.5 s. One variant.
    Warning,
    /// Fanfare: three rising chip notes (C5 E5 G5) into a held C-major chord with a sparkle, 1.1 s.
    /// Variants (2): 0 and +2 semitones.
    Success,
    /// Defeat: a slow descending minor phrase (G4, E-flat4, C4, G3) on vibrato saws over a sub tone,
    /// 1.6 s. One variant.
    GameOver,
    /// Soft step: a lowpassed noise thud on a 160 to 95 Hz body with a faint scuff, 0.16 s. Variants (4)
    /// alternate pitch and colour so a walk never repeats one sample.
    Footstep,
    /// Power loss: a saw sweep falling from 800 to 90 Hz through a closing lowpass, 0.65 s. Variants (2)
    /// shift the pitch.
    PowerDown,
    /// Damage taken: a buzzy saw-and-square fall from 520 to 150 Hz with a noise crack, 0.32 s. Variants
    /// (3) shift the pitch.
    Hurt,
    /// Notification: two struck-bell notes (G5 then C6) built from decaying inharmonic partials, 1.2 s.
    /// Variants (2): 0 and +2 semitones.
    Chime,
}

impl Preset {
    /// Every preset in declaration order, so `Preset::ALL[p.index()] == p`. Example:
    /// `Preset::ALL.iter().map(|p| p.name())` lists the names.
    pub const ALL: [Preset; 23] = [
        Preset::Click,
        Preset::Select,
        Preset::Back,
        Preset::Blip,
        Preset::Coin,
        Preset::Pickup,
        Preset::PowerUp,
        Preset::Jump,
        Preset::Land,
        Preset::Hit,
        Preset::Thump,
        Preset::Explosion,
        Preset::Zap,
        Preset::Shoot,
        Preset::Whoosh,
        Preset::Error,
        Preset::Warning,
        Preset::Success,
        Preset::GameOver,
        Preset::Footstep,
        Preset::PowerDown,
        Preset::Hurt,
        Preset::Chime,
    ];

    /// Position in [`Preset::ALL`], stable within a version. Example: `Preset::Click.index()` is `0`.
    pub fn index(self) -> usize {
        self as usize
    }

    /// Lower-case snake_case name, unique per preset, handy for file names and data files. Example:
    /// `Preset::PowerUp.name()` is `"power_up"`.
    pub fn name(self) -> &'static str {
        match self {
            Preset::Click => "click",
            Preset::Select => "select",
            Preset::Back => "back",
            Preset::Blip => "blip",
            Preset::Coin => "coin",
            Preset::Pickup => "pickup",
            Preset::PowerUp => "power_up",
            Preset::Jump => "jump",
            Preset::Land => "land",
            Preset::Hit => "hit",
            Preset::Thump => "thump",
            Preset::Explosion => "explosion",
            Preset::Zap => "zap",
            Preset::Shoot => "shoot",
            Preset::Whoosh => "whoosh",
            Preset::Error => "error",
            Preset::Warning => "warning",
            Preset::Success => "success",
            Preset::GameOver => "game_over",
            Preset::Footstep => "footstep",
            Preset::PowerDown => "power_down",
            Preset::Hurt => "hurt",
            Preset::Chime => "chime",
        }
    }

    /// The preset called `name` (see [`Preset::name`]), or `None`. Example:
    /// `Preset::from_name("game_over")` is `Some(Preset::GameOver)`.
    pub fn from_name(name: &str) -> Option<Preset> {
        Preset::ALL.into_iter().find(|p| p.name() == name)
    }

    /// How many variants make sense (1 to 4): play them round-robin so repeated sounds do not fatigue,
    /// or, for ladder presets such as [`Preset::Coin`], step up with a combo counter. Example:
    /// `Preset::Footstep.variants()` is `4`.
    pub fn variants(self) -> usize {
        match self {
            Preset::Warning | Preset::GameOver => 1,
            Preset::Select
            | Preset::Back
            | Preset::PowerUp
            | Preset::PowerDown
            | Preset::Error
            | Preset::Success
            | Preset::Chime => 2,
            Preset::Click
            | Preset::Jump
            | Preset::Land
            | Preset::Hit
            | Preset::Thump
            | Preset::Explosion
            | Preset::Zap
            | Preset::Shoot
            | Preset::Whoosh
            | Preset::Hurt => 3,
            Preset::Blip | Preset::Coin | Preset::Pickup | Preset::Footstep => 4,
        }
    }

    /// Method form of [`render`]: `preset.render(variant, seed)` is `render(preset, variant, seed)`.
    /// Example: `Preset::Coin.render(0, 7)`.
    pub fn render(self, variant: usize, seed: u64) -> Vec<f32> {
        render(self, variant, seed)
    }
}

/// Semitone steps of the rising ladder used by tonal presets (a major-pentatonic climb).
const LADDER: [f32; 4] = [0., 2., 4., 7.];
/// Pitch multipliers of the round-robin variants of percussive presets.
const SPREAD: [f32; 4] = [1., 1.07, 0.94, 1.12];

/// Everything a recipe needs to vary itself: the variant, two pitch factors and the noise generator.
struct Take {
    /// Variant index, already reduced modulo the preset's variant count.
    k: usize,
    /// Pitch multiplier for percussive and gliding sounds: the variant's spread times a few percent of
    /// seed jitter.
    p: f32,
    /// Pitch shift in semitones for tonal sounds: the variant's ladder step plus a fifth of a semitone
    /// of seed jitter.
    semis: f32,
    rng: Rng,
}

/// Render one variant of a preset as mono samples in `[-1, 1]` at [`RATE`].
///
/// `variant` selects a designed variation and is taken modulo [`Preset::variants`], so a running counter
/// gives round-robin playback. `seed` adds a little pitch jitter (a few percent) and a fresh noise
/// texture, so repeated hits with different seeds do not sound like the same recording. The result is
/// deterministic: the same arguments give the same samples. Every render is finite, peaks between 0.25
/// and 0.98, starts and ends at (almost) zero and lasts between 40 ms and 2.5 s, whatever the variant
/// number or seed (it never panics). Also available as [`Preset::render`]. Example:
/// `render(Preset::Coin, 2, 99)` is the third rung of the coin ladder.
pub fn render(preset: Preset, variant: usize, seed: u64) -> Vec<f32> {
    let k = variant % preset.variants();
    let salt = (preset.index() as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (k as u64 + 1).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let mut rng = Rng::new(seed ^ salt);
    let p = SPREAD[k % 4] * (1. + 0.03 * rng.range(-1., 1.));
    let semis = LADDER[k % 4] + 0.2 * rng.range(-1., 1.);
    let t = &mut Take { k, p, semis, rng };
    match preset {
        Preset::Click => click(t),
        Preset::Select => select(t),
        Preset::Back => back(t),
        Preset::Blip => blip(t),
        Preset::Coin => coin(t),
        Preset::Pickup => pickup(t),
        Preset::PowerUp => power_up(t),
        Preset::Jump => jump(t),
        Preset::Land => land(t),
        Preset::Hit => hit(t),
        Preset::Thump => thump(t),
        Preset::Explosion => explosion(t),
        Preset::Zap => zap(t),
        Preset::Shoot => shoot(t),
        Preset::Whoosh => whoosh(t),
        Preset::Error => error(t),
        Preset::Warning => warning(t),
        Preset::Success => success(t),
        Preset::GameOver => game_over(t),
        Preset::Footstep => footstep(t),
        Preset::PowerDown => power_down(t),
        Preset::Hurt => hurt(t),
        Preset::Chime => chime(t),
    }
}

/// Scale to a peak of 1, then soft-clip with `drive`: evens out sounds whose loudest moments are short
/// spikes (noise cracks, crackle), so that normalising to a peak level does not leave their body quiet.
fn squash(mut v: Vec<f32>, drive: f32) -> Vec<f32> {
    let loudest = peak(&v);
    if loudest > 1e-6 {
        for x in &mut v {
            *x /= loudest;
        }
    }
    soft_clip(&mut v, drive);
    v
}

/// Soft chiptune timbre: mostly triangle with a little square for bite.
fn chip_wave(p: f32) -> f32 {
    0.7 * triangle(p) + 0.3 * square(p)
}

/// Like [`note`] with a band-limited [`Wave`] instead of a phase function (same edge fades).
fn note_bl(dur: f32, hz: f32, wave: Wave, env: impl Fn(f32) -> f32) -> Vec<f32> {
    let mut v = shape(osc_bl(dur, |_| hz, wave), env);
    fade_edges(&mut v, 0.001, 0.005);
    v
}

/// Linear fade to zero over the `len` seconds before `end`, 1 earlier: an explicit tail for envelopes of
/// voices that are still sounding when their buffer ends.
fn tail(t: f32, end: f32, len: f32) -> f32 {
    1. - ((t - (end - len)) / len).clamp(0., 1.)
}

/// Sparse random clicks whose density per sample starts at `density` and decays with time constant `tau`.
fn crackle(dur: f32, density: f32, tau: f32, rng: &mut Rng) -> Vec<f32> {
    (0..n_samples(dur))
        .map(|i| {
            if rng.f32() < density * decay(i as f32 / SR, tau) {
                rng.f32() * 2. - 1.
            } else {
                0.
            }
        })
        .collect()
}

/// A gliding frequency with vibrato: `depth` is a fraction (0.01 is plus or minus 1%), `rate` in Hz.
fn glide_vibrato(f0: f32, f1: f32, over: f32, depth: f32, rate: f32) -> impl Fn(f32) -> f32 {
    let g = glide(f0, f1, over);
    move |t| g(t) * (1. + depth * (TAU * rate * t).sin())
}

fn click(t: &mut Take) -> Vec<f32> {
    let p = t.p;
    let mut m = Mix::new(0.07);
    m.add(0., &note(0.06, 1500. * p, sine, |s| decay(s, 0.008)), 0.55);
    m.add(0., &note(0.03, 220. * p, sine, |s| decay(s, 0.005)), 0.25);
    let tick = highpass(&noise(0.02, &mut t.rng), 3500., 0.7);
    m.add(0., &shape(tick, |s| decay(s, 0.003)), 0.45);
    finish(m.into_vec(), 0.5, 10.)
}

fn select(t: &mut Take) -> Vec<f32> {
    let root = midi(76. + t.semis);
    let mut m = Mix::new(0.26);
    m.add(
        0.,
        &note(0.09, root, chip_wave, |s| ad(s, 0.002, 0.03)),
        0.5,
    );
    m.add(
        0.07,
        &note(0.19, root * 1.5, chip_wave, |s| ad(s, 0.002, 0.08)),
        0.55,
    );
    finish(lowpass(&m.into_vec(), 6000., 0.7), 0.5, 25.)
}

fn back(t: &mut Take) -> Vec<f32> {
    let top = midi(83. + t.semis);
    let mut m = Mix::new(0.24);
    m.add(0., &note(0.08, top, chip_wave, |s| ad(s, 0.002, 0.03)), 0.5);
    m.add(
        0.065,
        &note(0.175, top / 1.5, chip_wave, |s| ad(s, 0.002, 0.07)),
        0.45,
    );
    finish(lowpass(&m.into_vec(), 4500., 0.7), 0.45, 25.)
}

fn blip(t: &mut Take) -> Vec<f32> {
    let hz = midi(81. + t.semis);
    let voice = note_bl(0.07, hz, Wave::Pulse(0.4), |s| ad(s, 0.001, 0.02));
    finish(lowpass(&voice, 6500., 0.7), 0.4, 15.)
}

fn coin(t: &mut Take) -> Vec<f32> {
    let (first, second) = (midi(83. + t.semis), midi(88. + t.semis));
    let mut m = Mix::new(0.5);
    m.add(
        0.,
        &note_bl(0.08, first, Wave::Square, |s| ad(s, 0.001, 0.03)),
        0.5,
    );
    m.add(
        0.07,
        &note_bl(0.43, second, Wave::Square, |s| ad(s, 0.001, 0.14)),
        0.5,
    );
    m.add(0.07, &note(0.3, second * 2., sine, |s| decay(s, 0.1)), 0.25);
    finish(lowpass(&m.into_vec(), 8000., 0.7), 0.55, 40.)
}

fn pickup(t: &mut Take) -> Vec<f32> {
    let root = 72. + t.semis;
    let mut m = Mix::new(0.42);
    for (i, step) in [0., 4., 7., 12.].into_iter().enumerate() {
        let (at, hz) = (i as f32 * 0.055, midi(root + step));
        m.add(at, &note(0.22, hz, chip_wave, |s| ad(s, 0.002, 0.07)), 0.5);
        m.add(at, &note(0.16, hz * 2., sine, |s| decay(s, 0.05)), 0.15);
    }
    finish(m.into_vec(), 0.5, 40.)
}

fn power_up(t: &mut Take) -> Vec<f32> {
    let dur = 0.7;
    let ratio = 2f32.powf(t.semis / 12.);
    let pitch = glide_vibrato(220. * ratio, 1320. * ratio, 0.45, 0.012, 11.);
    let (saws, squares) = (
        osc_bl(dur, &pitch, Wave::Saw),
        osc_bl(dur, &pitch, Wave::Square),
    );
    let sweep: Vec<f32> = saws
        .iter()
        .zip(&squares)
        .map(|(a, b)| 0.6 * a + 0.4 * b)
        .collect();
    let sweep = filter(
        &sweep,
        FilterKind::Low,
        |s| 700. + 5300. * (s / 0.45).min(1.),
        1.1,
    );
    let sweep = shape(sweep, |s| {
        ad(s, 0.03, 0.6) * (1. - ((s - 0.4) / 0.15).clamp(0., 1.))
    });
    let mut m = Mix::new(dur);
    m.add(0., &sweep, 0.7);
    m.add(
        0.38,
        &bell(
            0.32,
            midi(96. + t.semis),
            &[(1., 0.22, 0.5), (2., 0.14, 0.2), (3., 0.1, 0.1)],
        ),
        0.6,
    );
    finish(m.into_vec(), 0.6, 60.)
}

fn jump(t: &mut Take) -> Vec<f32> {
    let p = t.p;
    let mut m = Mix::new(0.2);
    m.add(
        0.,
        &shape(osc(0.2, glide(250. * p, 540. * p, 0.09), sine), |s| {
            ad(s, 0.003, 0.08)
        }),
        0.7,
    );
    m.add(
        0.,
        &shape(osc(0.2, glide(500. * p, 1080. * p, 0.09), triangle), |s| {
            ad(s, 0.003, 0.05)
        }),
        0.25,
    );
    finish(m.into_vec(), 0.55, 25.)
}

fn land(t: &mut Take) -> Vec<f32> {
    let p = t.p;
    let dur = 0.3;
    let mut m = Mix::new(dur);
    m.add(
        0.,
        &shape(osc(dur, glide(115. * p, 52. * p, 0.12), sine), |s| {
            ad(s, 0.002, 0.075)
        }),
        0.95,
    );
    let puff = lowpass(&noise(dur, &mut t.rng), 700., 0.7);
    m.add(0., &shape(puff, |s| ad(s, 0.001, 0.045)), 0.5);
    finish(m.into_vec(), 0.7, 30.)
}

fn hit(t: &mut Take) -> Vec<f32> {
    let p = t.p;
    let mut m = Mix::new(0.32);
    m.add(
        0.,
        &shape(osc(0.32, glide(230. * p, 75. * p, 0.09), sine), |s| {
            ad(s, 0.001, 0.07)
        }),
        0.9,
    );
    m.add(
        0.,
        &shape(osc(0.1, glide(420. * p, 190. * p, 0.05), triangle), |s| {
            decay(s, 0.03)
        }),
        0.3,
    );
    let slap = bandpass(&noise(0.08, &mut t.rng), 1400. * p, 1.2);
    m.add(0., &shape(slap, |s| decay(s, 0.02)), 0.5);
    let crack = highpass(&noise(0.03, &mut t.rng), 2500., 0.7);
    m.add(0., &shape(crack, |s| decay(s, 0.004)), 0.45);
    let mut v = m.into_vec();
    soft_clip(&mut v, 1.3);
    finish(v, 0.85, 40.)
}

fn thump(t: &mut Take) -> Vec<f32> {
    let p = t.p;
    let dur = 0.55;
    let mut m = Mix::new(dur);
    m.add(
        0.,
        &shape(osc(dur, glide(120. * p, 48. * p, 0.2), sine), |s| {
            ad(s, 0.004, 0.16)
        }),
        1.0,
    );
    m.add(
        0.,
        &shape(osc(dur, glide(300. * p, 110. * p, 0.06), sine), |s| {
            ad(s, 0.001, 0.04)
        }),
        0.6,
    );
    let thud = lowpass(&noise(dur, &mut t.rng), 380., 0.7);
    m.add(0., &shape(thud, |s| ad(s, 0.003, 0.07)), 0.6);
    let mut v = m.into_vec();
    soft_clip(&mut v, 2.);
    finish(v, 0.92, 60.)
}

fn explosion(t: &mut Take) -> Vec<f32> {
    let p = t.p;
    let dur = 1.4;
    let mut m = Mix::new(dur);
    m.add(
        0.,
        &shape(osc(dur, glide(100. * p, 32. * p, 0.75), sine), |s| {
            ad(s, 0.003, 0.45)
        }),
        1.0,
    );
    let blast = filter(
        &noise(dur, &mut t.rng),
        FilterKind::Low,
        |s| 5200. * 0.025f32.powf((s / 0.9).min(1.)),
        0.7,
    );
    m.add(0., &shape(blast, |s| ad(s, 0.002, 0.3)), 0.9);
    let rumble = lowpass(&noise(dur, &mut t.rng), 220., 0.7);
    m.add(0., &shape(rumble, |s| ad(s, 0.01, 0.5)), 0.6);
    let crack = highpass(&noise(0.06, &mut t.rng), 2500., 0.7);
    m.add(0., &shape(crack, |s| decay(s, 0.012)), 0.8);
    m.add(
        0.05,
        &highpass(&crackle(dur, 0.0016, 0.4, &mut t.rng), 1500., 0.7),
        0.7,
    );
    let mut v = m.into_vec();
    soft_clip(&mut v, 1.8);
    finish(v, 0.97, 80.)
}

fn zap(t: &mut Take) -> Vec<f32> {
    let p = t.p;
    let dur = 0.4;
    let dive = glide(2800. * p, 160. * p, 0.25);
    let wobble = |s: f32| dive(s) * (1. + 0.05 * (TAU * 40. * s).sin());
    let saw_dive = osc_bl(dur, wobble, Wave::Saw);
    let buzz = shape(saw_dive, |s| {
        ad(s, 0.001, 0.09) * (0.6 + 0.4 * (TAU * 90. * s).sin().signum())
    });
    let mut m = Mix::new(dur);
    m.add(0., &highpass(&buzz, 250., 0.7), 0.6);
    m.add(
        0.,
        &highpass(&crackle(dur, 0.03, 0.08, &mut t.rng), 1800., 0.7),
        0.5,
    );
    finish(squash(m.into_vec(), 2.5), 0.7, 50.)
}

fn shoot(t: &mut Take) -> Vec<f32> {
    let p = t.p;
    let dur = 0.24;
    let mut m = Mix::new(dur);
    let pew = osc_bl(dur, glide(1900. * p, 200. * p, 0.15), Wave::Pulse(0.3));
    m.add(
        0.,
        &shape(lowpass(&pew, 5500., 0.7), |s| {
            ad(s, 0.001, 0.05) * tail(s, dur, 0.1)
        }),
        0.55,
    );
    m.add(
        0.,
        &shape(osc(dur, glide(260. * p, 90. * p, 0.08), sine), |s| {
            ad(s, 0.002, 0.05)
        }),
        0.25,
    );
    let snap = highpass(&noise(0.03, &mut t.rng), 2000., 0.7);
    m.add(0., &shape(snap, |s| decay(s, 0.004)), 0.5);
    finish(squash(m.into_vec(), 2.), 0.65, 40.)
}

fn whoosh(t: &mut Take) -> Vec<f32> {
    let p = t.p;
    let dur = 0.55;
    let sweep = |s: f32| (300. + 3600. * (s / 0.38).min(1.).powf(1.4)) * p;
    // Two cascaded bands (24 dB per octave skirts) make the moving centre easy to hear.
    let air = filter(&noise(dur, &mut t.rng), FilterKind::Band, sweep, 1.4);
    let air = filter(&air, FilterKind::Band, sweep, 1.4);
    let swell = |s: f32| (s / 0.13).min(1.).powi(2) * decay((s - 0.13).max(0.), 0.14);
    let mut m = Mix::new(dur);
    m.add(0., &shape(air, swell), 0.9);
    let body = lowpass(&noise(dur, &mut t.rng), 500. * p, 0.7);
    m.add(0., &shape(body, |s| swell(s) * 0.5), 0.2);
    finish(squash(m.into_vec(), 1.8), 0.6, 80.)
}

fn error(t: &mut Take) -> Vec<f32> {
    let p = t.p;
    let mut m = Mix::new(0.27);
    for (at, hz) in [(0., 196. * p), (0.14, 156. * p)] {
        let a = osc(0.12, |_| hz, square);
        let b = osc(0.12, |_| hz * 1.03, saw);
        let mixed: Vec<f32> = a.iter().zip(&b).map(|(x, y)| 0.5 * x + 0.5 * y).collect();
        let buzz = shape(lowpass(&mixed, 1800., 0.8), |s| {
            (s / 0.006).min(1.) * tail(s, 0.12, 0.03)
        });
        m.add(at, &buzz, 0.6);
    }
    finish(m.into_vec(), 0.5, 30.)
}

fn warning(t: &mut Take) -> Vec<f32> {
    let p = t.p;
    let mut m = Mix::new(0.5);
    for (i, hz) in [988., 740., 988., 740.].into_iter().enumerate() {
        let beep = note_bl(0.1, hz * p, Wave::Square, |s| {
            (s / 0.004).min(1.) * tail(s, 0.1, 0.03)
        });
        m.add(i as f32 * 0.13, &lowpass(&beep, 3500., 0.7), 0.5);
    }
    finish(m.into_vec(), 0.42, 30.)
}

fn success(t: &mut Take) -> Vec<f32> {
    let root = 72. + t.semis;
    let mut m = Mix::new(1.1);
    for (i, step) in [0., 4., 7.].into_iter().enumerate() {
        m.add(
            i as f32 * 0.09,
            &note(0.3, midi(root + step), chip_wave, |s| ad(s, 0.003, 0.09)),
            0.45,
        );
    }
    for step in [0., 4., 7., 12.] {
        m.add(
            0.27,
            &note(0.83, midi(root + step), triangle, |s| {
                ad(s, 0.01, 0.3) * tail(s, 0.83, 0.2)
            }),
            0.28,
        );
    }
    m.add(
        0.27,
        &note(0.83, midi(root + 24.), sine, |s| {
            decay(s, 0.25) * tail(s, 0.83, 0.2)
        }),
        0.15,
    );
    let air = highpass(&noise(0.6, &mut t.rng), 7000., 0.7);
    m.add(
        0.3,
        &shape(air, |s| ad(s, 0.01, 0.2) * tail(s, 0.6, 0.2)),
        0.06,
    );
    finish(m.into_vec(), 0.65, 100.)
}

fn game_over(t: &mut Take) -> Vec<f32> {
    let p = t.p;
    let mut m = Mix::new(1.6);
    for (i, n) in [67., 64., 60., 55.].into_iter().enumerate() {
        let hz = midi(n) * p;
        let voice = osc(0.5, |s| hz * (1. + 0.006 * (TAU * 5.5 * s).sin()), saw);
        m.add(
            i as f32 * 0.24,
            &shape(lowpass(&voice, 1500., 0.8), |s| {
                ad(s, 0.01, 0.28) * tail(s, 0.5, 0.12)
            }),
            0.32,
        );
    }
    m.add(
        0.5,
        &note(1.1, 55. * p, sine, |s| ad(s, 0.05, 0.6) * tail(s, 1.1, 0.3)),
        0.4,
    );
    finish(m.into_vec(), 0.75, 150.)
}

fn footstep(t: &mut Take) -> Vec<f32> {
    let p = t.p;
    let cutoff = [900., 1200., 750., 1050.][t.k % 4];
    let mut m = Mix::new(0.16);
    m.add(
        0.,
        &shape(osc(0.16, glide(160. * p, 95. * p, 0.05), sine), |s| {
            ad(s, 0.002, 0.03)
        }),
        0.8,
    );
    let body = lowpass(&noise(0.16, &mut t.rng), cutoff, 0.7);
    m.add(0., &shape(body, |s| ad(s, 0.002, 0.03)), 0.7);
    let scuff = bandpass(&noise(0.05, &mut t.rng), 2500. * p, 0.8);
    m.add(0., &shape(scuff, |s| decay(s, 0.012)), 0.15);
    finish(m.into_vec(), 0.5, 30.)
}

fn power_down(t: &mut Take) -> Vec<f32> {
    let p = t.p;
    let dur = 0.65;
    let fall = osc(
        dur,
        glide_vibrato(800. * p, 90. * p, 0.55, 0.02, 9.),
        |ph| 0.6 * saw(ph) + 0.4 * square(ph),
    );
    let fall = filter(
        &fall,
        FilterKind::Low,
        |s| 300. + 3200. * (1. - (s / 0.55).min(1.)),
        1.1,
    );
    let fall = shape(fall, |s| ad(s, 0.01, 0.35) * tail(s, dur, 0.2));
    finish(fall, 0.6, 60.)
}

fn hurt(t: &mut Take) -> Vec<f32> {
    let p = t.p;
    let dur = 0.32;
    let mut m = Mix::new(dur);
    let fall = osc(dur, glide(520. * p, 150. * p, 0.22), |ph| {
        0.5 * saw(ph) + 0.5 * square(ph)
    });
    let fall = filter(
        &fall,
        FilterKind::Low,
        |s| 3000. - 2500. * (s / 0.25).min(1.),
        0.9,
    );
    m.add(0., &shape(fall, |s| ad(s, 0.003, 0.12)), 0.7);
    let crack = highpass(&noise(0.03, &mut t.rng), 1500., 0.7);
    m.add(0., &shape(crack, |s| decay(s, 0.006)), 0.5);
    finish(squash(m.into_vec(), 2.), 0.8, 45.)
}

fn chime(t: &mut Take) -> Vec<f32> {
    let partials = [
        (1., 0.4, 1.0),
        (2., 0.25, 0.3),
        (2.76, 0.16, 0.18),
        (4.07, 0.09, 0.08),
    ];
    let mut m = Mix::new(1.2);
    m.add(0., &bell(1., midi(79. + t.semis), &partials), 0.7);
    m.add(0.16, &bell(1.04, midi(84. + t.semis), &partials), 0.8);
    finish(m.into_vec(), 0.5, 150.)
}

// ------------------------------------------------------------------------------------- music

/// What [`music_loop`] should render. `MusicSpec::default()` is a 16-bar loop in A minor at 124 bpm.
/// Example: `MusicSpec { bars: 8, minor: false, ..MusicSpec::default() }` is eight bars in A major.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MusicSpec {
    /// Tempo in beats per minute, clamped to 40 to 240 (NaN counts as 120).
    pub bpm: f32,
    /// Length in bars of four beats, clamped to 1 to 64.
    pub bars: usize,
    /// The tonic as a MIDI note number. Only its pitch class matters (57 and 45 are both A): the bass,
    /// chords and lead always sit in the same registers.
    pub root_midi: u8,
    /// Natural minor when true, major when false.
    pub minor: bool,
    /// Picks the chord progressions, the arpeggio pattern, the melody and the drum noise.
    pub seed: u64,
}

impl Default for MusicSpec {
    fn default() -> Self {
        Self {
            bpm: 124.,
            bars: 16,
            root_midi: 57,
            minor: true,
            seed: 1,
        }
    }
}

impl MusicSpec {
    fn clamped(&self) -> (f64, usize) {
        let bpm = if self.bpm.is_finite() {
            self.bpm.clamp(40., 240.)
        } else {
            120.
        };
        (f64::from(bpm), self.bars.clamp(1, 64))
    }

    /// Length of one beat in seconds (after clamping the tempo). Example: at 120 bpm it is `0.5`.
    pub fn beat_seconds(&self) -> f32 {
        (60. / self.clamped().0) as f32
    }

    /// Length of the whole loop in samples (after clamping tempo and bars); every stem of
    /// [`music_loop`] has exactly this many. Example: 16 bars at 120 bpm is `1_411_200`.
    pub fn loop_samples(&self) -> usize {
        let (bpm, bars) = self.clamped();
        (bars as f64 * 4. * 60. / bpm * f64::from(RATE)).round() as usize
    }

    /// Length of the whole loop in seconds. Example: 16 bars at 120 bpm is `32.0`.
    pub fn loop_seconds(&self) -> f32 {
        self.loop_samples() as f32 / SR
    }
}

/// Three synchronised, equally long stems of one loop, so a game can fade layers in and out with the
/// intensity of the action. Each is normalised to its own peak (0.8, 0.6 and 0.6), leaving headroom to
/// sum them. Example: `let Stems { base, melodic, lead } = music_loop(&MusicSpec::default());`.
#[derive(Clone, Debug, PartialEq)]
pub struct Stems {
    /// Kick, clap, off-beat hats and a bass that follows the chord roots: the always-on layer.
    pub base: Vec<f32>,
    /// A pad of detuned saws and a plucked sixteenth-note arpeggio, both on the chords.
    pub melodic: Vec<f32>,
    /// A seeded lead melody through the scale, landing on chord tones.
    pub lead: Vec<f32>,
}

/// Semitone offsets of the natural minor and major scales.
const MINOR_SCALE: [i32; 7] = [0, 2, 3, 5, 7, 8, 10];
const MAJOR_SCALE: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
/// Four-bar chord progressions as scale degrees (0 is the tonic chord): minor i-VI-III-VII and friends,
/// major I-V-vi-IV and friends. Two of them, picked by the seed, alternate every four bars.
const MINOR_PROGRESSIONS: [[usize; 4]; 4] =
    [[0, 5, 2, 6], [0, 6, 5, 6], [0, 3, 6, 2], [0, 2, 6, 5]];
const MAJOR_PROGRESSIONS: [[usize; 4]; 4] =
    [[0, 4, 5, 3], [0, 5, 3, 4], [0, 3, 5, 4], [5, 3, 0, 4]];
/// Which chord tone each sixteenth of the arpeggio plays.
const ARP_PATTERNS: [[usize; 16]; 4] = [
    [0, 1, 2, 3, 2, 1, 2, 3, 0, 1, 2, 3, 2, 1, 3, 2],
    [0, 2, 1, 3, 0, 2, 1, 3, 0, 2, 1, 3, 1, 2, 3, 2],
    [3, 2, 1, 0, 1, 2, 3, 2, 3, 2, 1, 0, 1, 2, 1, 3],
    [0, 1, 0, 2, 1, 3, 2, 3, 0, 1, 0, 2, 1, 3, 3, 2],
];
/// Lead rhythms over two bars as (start beat, length in beats).
type Hook = [(f64, f64)];
const HOOKS: [&Hook; 4] = [
    &[
        (0., 1.5),
        (1.5, 0.5),
        (2., 1.),
        (3., 0.5),
        (3.5, 0.5),
        (4., 2.),
    ],
    &[
        (0., 1.),
        (1., 1.),
        (2., 2.),
        (4., 1.),
        (5., 0.5),
        (5.5, 0.5),
        (6., 1.5),
    ],
    &[
        (0., 0.5),
        (0.5, 0.5),
        (1., 1.),
        (2., 1.5),
        (3.5, 0.5),
        (4., 1.),
        (5., 1.),
        (6., 2.),
    ],
    &[
        (0., 2.),
        (2., 1.),
        (3., 1.),
        (4., 1.5),
        (5.5, 0.5),
        (6., 2.),
    ],
];

/// One chord of the progression: its bass note and four voiced tones (MIDI numbers).
struct Chord {
    bass: f32,
    tones: [f32; 4],
}

impl Chord {
    /// The diatonic seventh chord on `degree` of `scale` (thirds stacked within the scale), voiced with
    /// the root in the octave from F3 (53) up and the bass root in the octave from E1 (28) up.
    fn new(scale: &[i32; 7], tonic: i32, degree: usize) -> Self {
        let stacked: Vec<i32> = (0..4)
            .map(|k| scale[(degree + 2 * k) % 7] + 12 * ((degree + 2 * k) / 7) as i32)
            .collect();
        let root_pc = tonic + scale[degree];
        let root = 53 + (root_pc - 53).rem_euclid(12);
        let bass = 28 + (root_pc - 28).rem_euclid(12);
        let tones = [0, 1, 2, 3].map(|k| (root + stacked[k] - stacked[0]) as f32);
        Self {
            bass: bass as f32,
            tones,
        }
    }
}

/// One note of the lead melody.
struct LeadNote {
    /// Start in beats from the beginning of the loop.
    start: f64,
    /// Length in beats.
    len: f64,
    /// MIDI note number.
    note: i32,
}

/// Everything the seed decides, before any audio is rendered.
struct Plan {
    chords: Vec<Chord>,
    arp: [usize; 16],
    lead: Vec<LeadNote>,
}

fn scale_of(minor: bool) -> &'static [i32; 7] {
    if minor {
        &MINOR_SCALE
    } else {
        &MAJOR_SCALE
    }
}

/// Choose the chords (two progressions alternating every four bars), the arpeggio pattern and the lead
/// melody: every two bars the same rhythm with a stepwise walk through the scale whose strong beats and
/// last note land on chord tones.
fn plan(spec: &MusicSpec, rng: &mut Rng) -> Plan {
    let (_, bars) = spec.clamped();
    let scale = scale_of(spec.minor);
    let progressions = if spec.minor {
        &MINOR_PROGRESSIONS
    } else {
        &MAJOR_PROGRESSIONS
    };
    let tonic = i32::from(spec.root_midi % 12);
    let first = rng.below(4);
    let second = (first + 1 + rng.below(3)) % 4;
    let chords: Vec<Chord> = (0..bars)
        .map(|bar| {
            let progression = progressions[if (bar / 4) % 2 == 0 { first } else { second }];
            Chord::new(scale, tonic, progression[bar % 4])
        })
        .collect();
    let arp = ARP_PATTERNS[rng.below(4)];
    let hook = HOOKS[rng.below(4)];

    let in_range: Vec<i32> = (72..=90)
        .filter(|n| scale.contains(&(n - tonic).rem_euclid(12)))
        .collect();
    let mut position = in_range.len() / 2;
    let mut lead = Vec::new();
    for bar in (0..bars).step_by(2) {
        for (k, &(start, len)) in hook.iter().enumerate() {
            let start_beat = bar as f64 * 4. + start;
            if start_beat >= bars as f64 * 4. {
                continue;
            }
            let chord = &chords[(start_beat / 4.) as usize];
            let is_chord_tone = |note: i32| {
                chord
                    .tones
                    .iter()
                    .any(|t| (note - *t as i32).rem_euclid(12) == 0)
            };
            let step = [-2, -1, -1, 0, 1, 1, 2][rng.below(7)];
            position = (position as i32 + step).clamp(0, in_range.len() as i32 - 1) as usize;
            // Strong beats and the last note of a phrase land on a chord tone (nearest first).
            if start % 2. == 0. || k + 1 == hook.len() {
                let nearest = (0..in_range.len())
                    .min_by_key(|&j| (!is_chord_tone(in_range[j]), j.abs_diff(position)));
                position = nearest.unwrap_or(position);
            }
            lead.push(LeadNote {
                start: start_beat,
                len,
                note: in_range[position],
            });
        }
    }
    Plan { chords, arp, lead }
}

fn kick_hit(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.32);
    m.add(
        0.,
        &shape(osc(0.32, glide(170., 50., 0.075), sine), |t| {
            ad(t, 0.001, 0.15) * tail(t, 0.32, 0.08)
        }),
        1.0,
    );
    // A 0.5 ms attack ramp keeps the click from starting with a jump (the loop begins on a kick).
    m.add(
        0.,
        &shape(highpass(&noise(0.02, rng), 1500., 0.7), |t| {
            ad(t, 0.0005, 0.004)
        }),
        0.3,
    );
    m.into_vec()
}

fn clap_hit(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.3);
    for at in [0., 0.011, 0.023] {
        let burst = bandpass(&noise(0.2, rng), 1900., 0.9);
        m.add(at, &shape(burst, |t| decay(t, 0.02)), 0.5);
    }
    let wash = bandpass(&noise(0.27, rng), 1700., 0.8);
    m.add(0.03, &shape(wash, |t| decay(t, 0.09)), 0.5);
    let mut v = m.into_vec();
    fade_edges(&mut v, 0., 0.05);
    v
}

fn hat_hit(rng: &mut Rng, tau: f32) -> Vec<f32> {
    let mut v = shape(highpass(&noise(0.3, rng), 7000., 0.8), |t| decay(t, tau));
    fade_edges(&mut v, 0., 0.06);
    v
}

/// A short plucked chord tone: detuned pulse and saw through a lowpass that closes quickly.
fn pluck(hz: f32) -> Vec<f32> {
    let dur = 0.3;
    let a = osc_bl(dur, |_| hz, Wave::Pulse(0.35));
    let b = osc_bl(dur, |_| hz * 1.004, Wave::Saw);
    let mixed: Vec<f32> = a.iter().zip(&b).map(|(x, y)| 0.6 * x + 0.3 * y).collect();
    let voice = filter(
        &mixed,
        FilterKind::Low,
        |t| 900. + 3200. * decay(t, 0.08),
        1.2,
    );
    let mut v = shape(voice, |t| ad(t, 0.002, 0.11));
    fade_edges(&mut v, 0., 0.01);
    v
}

/// Bass note: a saw through a closing lowpass plus a sine sub, `dur` seconds long with a short release.
fn bass_voice(hz: f32, dur: f32) -> Vec<f32> {
    let len = dur + 0.05;
    let saw_part = filter(
        &osc(len, |_| hz, saw),
        FilterKind::Low,
        |t| 350. + 900. * decay(t, 0.09),
        1.1,
    );
    let sub = osc(len, |_| hz, sine);
    let mixed: Vec<f32> = saw_part
        .iter()
        .zip(&sub)
        .map(|(a, s)| 0.55 * a + 0.6 * s)
        .collect();
    shape(mixed, |t| ad(t, 0.004, 0.14) * tail(t, len, 0.05))
}

/// Add delayed, quieter copies of a loop to itself, wrapping around so the loop stays seamless.
fn echo(v: &[f32], taps: &[(usize, f32)]) -> Vec<f32> {
    let n = v.len();
    let mut out = v.to_vec();
    for &(delay, gain) in taps {
        let d = delay % n;
        for (i, o) in out.iter_mut().enumerate() {
            *o += v[(i + n - d) % n] * gain;
        }
    }
    out
}

/// Render a seamless backing-track loop as three stems (see [`Stems`]). Deterministic in `spec`.
///
/// The recipe: chord progressions built from the mode, a four-on-the-floor kick with a clap on 2 and 4,
/// off-beat hats, a bass that follows the chord roots, a detuned-saw pad and a plucked arpeggio ducked by
/// the kick, and a seeded lead that walks the scale and lands on chord tones. Every voice is added with
/// wrap-around, so tails that run past the end land at the start and the loop has no seam. It is a
/// starting point that was checked by measurement (tempo, seam, levels), not judged by ear: ship it if it
/// fits, or replace it with recorded music. Example: `music_loop(&MusicSpec::default())`.
pub fn music_loop(spec: &MusicSpec) -> Stems {
    let (bpm, _) = spec.clamped();
    let total = spec.loop_samples();
    let beat_samples = 60. / bpm * f64::from(RATE);
    let beat = beat_samples as f32 / SR;
    let bar_len = 4. * beat;
    // Sample index of a time given in beats.
    let at = |beats: f64| (beats * beat_samples).round() as usize;
    let mut rng = Rng::new(spec.seed);
    let Plan {
        chords,
        arp,
        lead: lead_notes,
    } = plan(spec, &mut rng);

    let kick = kick_hit(&mut rng);
    let clap = clap_hit(&mut rng);
    let hat_closed = hat_hit(&mut rng, 0.035);
    let hat_open = hat_hit(&mut rng, 0.12);

    let mut base = Mix::with_len(total);
    let mut melodic = Mix::with_len(total);
    let mut lead = Mix::with_len(total);
    // The kick "pumps" the pad and arpeggio: a dip after every beat that recovers before the next.
    let pump = |sample: usize| -> f32 {
        1. - 0.85 * decay((sample as f64 % beat_samples) as f32 / SR, 0.11)
    };

    for (bar, chord) in chords.iter().enumerate() {
        let bar_beats = bar as f64 * 4.;
        // Drums and bass.
        let (low, high) = (
            bass_voice(midi(chord.bass), beat * 0.45),
            bass_voice(midi(chord.bass + 12.), beat * 0.45),
        );
        let stab = shape(osc(beat * 0.3, |_| midi(chord.bass), sine), |t| {
            ad(t, 0.004, 0.1) * tail(t, beat * 0.3, 0.04)
        });
        for b in 0..4 {
            let on_beat = at(bar_beats + f64::from(b));
            let off_beat = at(bar_beats + f64::from(b) + 0.5);
            base.add_wrapped_at(on_beat, &kick, 0.95);
            if b == 1 || b == 3 {
                base.add_wrapped_at(on_beat, &clap, 0.6);
            }
            let open = bar % 4 == 3 && b == 3;
            base.add_wrapped_at(
                off_beat,
                if open { &hat_open } else { &hat_closed },
                if open { 0.3 } else { 0.24 },
            );
            base.add_wrapped_at(off_beat, if b == 3 { &high } else { &low }, 0.62);
            base.add_wrapped_at(on_beat, &stab, 0.35);
        }
        // Pad: detuned saws held for the bar, lowpassed, pumped by the kick.
        let pad_len = bar_len + 0.4;
        let mut pad = vec![0f32; n_samples(pad_len)];
        for tone in chord.tones {
            for detune in [-0.006, 0.006] {
                let hz = midi(tone) * (1. + detune);
                for (d, v) in pad.iter_mut().zip(osc(pad_len, |_| hz, saw)) {
                    *d += v;
                }
            }
        }
        let bar_start = at(bar_beats);
        let pad: Vec<f32> = lowpass(&pad, 1500., 0.8)
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let t = i as f32 / SR;
                v * (t / 0.25).min(1.) * tail(t, pad_len, 0.4) * pump(bar_start + i)
            })
            .collect();
        melodic.add_wrapped_at(bar_start, &pad, 0.075);
        // Arpeggio: sixteenths climbing and falling through the chord, accented on the beats.
        let plucks = chord.tones.map(|tone| pluck(midi(tone + 12.)));
        for (i, &tone_index) in arp.iter().enumerate() {
            let start = at(bar_beats + i as f64 * 0.25);
            let accent = if i % 4 == 0 { 1. } else { 0.72 };
            melodic.add_wrapped_at(start, &plucks[tone_index], 0.15 * accent * pump(start));
        }
    }

    for note in &lead_notes {
        let hz = midi(note.note as f32);
        let dur = note.len as f32 * beat + 0.1;
        let vibrato = |t: f32| hz * (1. + 0.004 * (TAU * 5.5 * t).sin() * (t / 0.3).min(1.));
        let (saw_part, square_part) = (
            osc_bl(dur, vibrato, Wave::Saw),
            osc_bl(dur, |_| hz, Wave::Square),
        );
        let mixed: Vec<f32> = saw_part
            .iter()
            .zip(&square_part)
            .map(|(a, b)| 0.6 * a + 0.25 * b)
            .collect();
        let voice = filter(
            &mixed,
            FilterKind::Low,
            |t| 1400. + 1800. * decay(t, 0.2),
            1.0,
        );
        let voice = shape(voice, |t| ad(t, 0.01, 0.6) * tail(t, dur, 0.1));
        lead.add_wrapped_at(at(note.start), &voice, 0.13);
    }

    let dotted = at(0.75);
    let melodic = echo(melodic.as_slice(), &[(dotted, 0.34), (2 * dotted, 0.16)]);
    let lead = echo(lead.as_slice(), &[(dotted, 0.3), (2 * dotted, 0.14)]);
    let mut base = base.into_vec();
    let (mut melodic, mut lead) = (melodic, lead);
    soft_clip(&mut base, 1.25);
    soft_clip(&mut melodic, 1.2);
    soft_clip(&mut lead, 1.2);
    let normalise = |mut v: Vec<f32>, target: f32| {
        let loudest = peak(&v).max(1e-6);
        for x in &mut v {
            *x *= target / loudest;
        }
        v
    };
    Stems {
        base: normalise(base, 0.8),
        melodic: normalise(melodic, 0.6),
        lead: normalise(lead, 0.6),
    }
}

// ------------------------------------------------------------------------------------- ambient

/// Parameters for a seamless, calm pad-and-air loop: no beat, no lead, meant for background listening
/// (sleep, focus, a menu) rather than a song. See [`ambient_loop`].
#[derive(Clone, Debug, PartialEq)]
pub struct AmbientSpec {
    /// Length of the loop in minutes, clamped to 0.5 to 10. `SoundBank` plays a stem on an endless
    /// loop, so a short, well-looped track reads the same as a long one.
    pub minutes: f32,
    /// The tonic as a MIDI note number; only its pitch class matters.
    pub root_midi: u8,
    /// Natural minor when true, major when false. Minor tends to read as more melancholy/ambient.
    pub minor: bool,
    /// How many chords the loop holds, clamped 2 to 8. Fewer means longer, stiller chords.
    pub chords: usize,
    /// 0 (dark and close) to 1 (airy and open): the pad's filter cutoff and the loudness of the soft
    /// air/chime layer.
    pub brightness: f32,
    /// Picks the chord progression, the sparse high chimes and nothing else: two specs that differ
    /// only here still share the same chord progression's shape.
    pub seed: u64,
}

impl Default for AmbientSpec {
    fn default() -> Self {
        Self {
            minutes: 2.,
            root_midi: 57,
            minor: true,
            chords: 4,
            brightness: 0.4,
            seed: 1,
        }
    }
}

impl AmbientSpec {
    fn clamped(&self) -> (f32, usize, f32) {
        let minutes = if self.minutes.is_finite() {
            self.minutes.clamp(0.5, 10.)
        } else {
            2.
        };
        let brightness = if self.brightness.is_finite() {
            self.brightness.clamp(0., 1.)
        } else {
            0.4
        };
        (minutes, self.chords.clamp(2, 8), brightness)
    }

    /// Length of the whole loop in seconds (after clamping). Example: 2 minutes is `120.0`.
    pub fn loop_seconds(&self) -> f32 {
        self.clamped().0 * 60.
    }
}

/// A deterministic 64-bit number from `text` (FNV-1a), for seeding a generator from a game's name so
/// two games get different, reproducible results without either storing or hand-picking a seed. The
/// same text always gives the same number, on any platform. Not for anything security-sensitive.
pub fn seed_from(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Dark/moody words: present, they nudge [`ambient_spec_for`] towards minor, a lower register and a
/// darker timbre. A short, curated, intentionally small list, not a sentiment model.
const DARK_WORDS: [&str; 14] = [
    "haunted",
    "spooky",
    "dark",
    "night",
    "ghost",
    "shadow",
    "grim",
    "horror",
    "creepy",
    "void",
    "doom",
    "nightmare",
    "crypt",
    "eerie",
];
/// Light/cheerful words: present, they nudge [`ambient_spec_for`] towards major, a higher register and
/// a brighter timbre.
const BRIGHT_WORDS: [&str; 14] = [
    "sun", "bright", "happy", "garden", "candy", "sunny", "cheerful", "festival", "carnival",
    "rainbow", "spring", "sparkle", "sweet", "meadow",
];

/// -1 (dark words outweigh bright ones) to 1 (the reverse), 0 for neither or a tie: how many words from
/// each small list in [`DARK_WORDS`]/[`BRIGHT_WORDS`] appear in `text` (lower-cased, substring match).
/// A light touch for an obviously-themed title or tagline ("haunted", "sunny meadow"), not a
/// text-understanding model; most titles will read as neutral, which is fine, not a bug.
fn mood_bias(text: &str) -> f32 {
    let lower = text.to_lowercase();
    let count = |words: &[&str]| words.iter().filter(|w| lower.contains(*w)).count() as f32;
    (count(&BRIGHT_WORDS) - count(&DARK_WORDS)).clamp(-2., 2.) / 2.
}

/// An [`AmbientSpec`] that fits the game: unique to `title` (key, mode, chord count and timbre all vary
/// with it, not just the seed, so two games with different titles read as different pieces of music,
/// not the same one with different notes), and nudged towards a mood `mood_bias` reads from `title`
/// and `tagline` together (an obviously spooky or cheerful title leans the generated mode, register and
/// brightness that way). Deterministic: the same title and tagline always give the same spec. A game
/// whose theme the words do not capture is free to build its own `AmbientSpec` by hand instead; this is
/// a reasonable default, not the only way to use [`ambient_loop`]. Example:
/// `ambient_loop(&ambient_spec_for("Spooky Kart", "Eight haunted karts, one hollow to win."))`.
pub fn ambient_spec_for(title: &str, tagline: &str) -> AmbientSpec {
    let mut rng = Rng::new(seed_from(title));
    let mood = mood_bias(&format!("{title} {tagline}"));
    let root_center = 51. + 8. * mood;
    // Any detected mood word decides the mode outright (a title that reads as spooky should not have a
    // 1-in-20 chance of coming out major); only a genuinely neutral title leaves it to the seed.
    let minor = if mood.abs() >= 0.5 {
        mood < 0.
    } else {
        rng.f32() < 0.5
    };
    AmbientSpec {
        minutes: 2.,
        root_midi: (root_center + rng.range(-3., 3.)).clamp(36., 72.) as u8,
        minor,
        chords: 3 + rng.below(4),
        brightness: (rng.range(0.25, 0.55) + 0.25 * mood).clamp(0., 1.),
        seed: rng.next_u64(),
    }
}

/// A warm, slow pad voice: a tone and its lower octave, each as two slightly detuned triangles,
/// through a lowpass that breathes with a slow LFO. `env(t)` shapes the whole voice (a long attack and
/// a long release, so successive chords crossfade rather than cut).
fn pad_voice(dur: f32, tone_hz: f32, cutoff_hz: f32, env: impl Fn(f32) -> f32) -> Vec<f32> {
    let mut mixed = vec![0f32; n_samples(dur)];
    for (octave, gain) in [(0.5, 0.5), (1.0, 0.35)] {
        for detune in [-0.0035, 0.0035] {
            let hz = tone_hz * octave * (1. + detune);
            for (d, v) in mixed.iter_mut().zip(osc(dur, |_| hz, triangle)) {
                *d += v * gain;
            }
        }
    }
    let breathing = filter(
        &mixed,
        FilterKind::Low,
        |t| cutoff_hz * (1. + 0.18 * (TAU * t / 23.).sin()),
        0.8,
    );
    shape(breathing, env)
}

/// Render a seamless ambient background loop: held pads over [`AmbientSpec::chords`] chords with slow
/// attacks and long overlapping releases (each crossfades into the next, wrapping), a lowpass that
/// breathes slowly, a soft filtered-noise air bed, and a few sparse high chimes. No beat, no lead.
/// Deterministic in `spec`. It is a starting point checked by measurement (see the tests), not by ear:
/// keep it, parameterise it per game, or replace it with recorded music.
/// Example: `ambient_loop(&AmbientSpec::default())`.
pub fn ambient_loop(spec: &AmbientSpec) -> Vec<f32> {
    let (minutes, chord_count, brightness) = spec.clamped();
    let total_seconds = minutes * 60.;
    let total = n_samples(total_seconds);
    let mut rng = Rng::new(spec.seed);
    let scale = scale_of(spec.minor);
    let progressions = if spec.minor {
        &MINOR_PROGRESSIONS
    } else {
        &MAJOR_PROGRESSIONS
    };
    let progression = progressions[rng.below(4)];
    let tonic = i32::from(spec.root_midi % 12);
    let chord_len = total_seconds / chord_count as f32;
    // Each voice runs past its slot and wraps, so one chord's release crossfades into the next.
    let overlap = (chord_len * 0.6).min(6.);
    let cutoff = 350. + 2600. * brightness;

    let mut mix = Mix::with_len(total);
    for i in 0..chord_count {
        let chord = Chord::new(scale, tonic, progression[i % 4]);
        let voice_len = chord_len + overlap;
        let attack = (chord_len * 0.4).min(5.);
        let env = move |t: f32| {
            (t / attack).min(1.)
                * if t > chord_len {
                    decay(t - chord_len, overlap * 0.5)
                } else {
                    1.
                }
        };
        for &tone in &chord.tones {
            let voice = pad_voice(voice_len, midi(tone), cutoff, env);
            mix.add_wrapped(i as f32 * chord_len, &voice, 0.11);
        }
        mix.add_wrapped(
            i as f32 * chord_len,
            &pad_voice(voice_len, midi(chord.bass), cutoff * 0.6, env),
            0.16,
        );
        // A sparse, quiet high chime: at most one per chord, more likely as the loop gets brighter.
        if rng.chance(0.35 + 0.3 * brightness) {
            let tone = *rng.pick(&chord.tones).unwrap_or(&chord.tones[0]);
            let at = i as f32 * chord_len + rng.range(chord_len * 0.15, chord_len * 0.85);
            let chime = bell(
                2.5,
                midi(tone + 24.),
                &[(1., 1.6, 1.), (2.0, 1.0, 0.4), (3.0, 0.7, 0.2)],
            );
            mix.add_wrapped(at, &chime, 0.04 + 0.05 * brightness);
        }
    }
    // A continuous soft air bed under everything, slowly amplitude-modulated so it never reads as a
    // static hiss.
    let air = bandpass(
        &noise(total_seconds, &mut rng),
        500. + 3000. * brightness,
        0.9,
    );
    let air_period = (total_seconds / 3.).max(4.);
    let air: Vec<f32> = air
        .iter()
        .enumerate()
        .map(|(i, v)| v * (0.6 + 0.4 * (TAU * i as f32 / SR / air_period).sin()))
        .collect();
    mix.add(0., &air, 0.015 + 0.02 * brightness);

    let mut v = mix.into_vec();
    soft_clip(&mut v, 1.1);
    finish(v, 0.45, 60.)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit sine, the test signal for filters and measurements.
    fn sine_wave(hz: f32, dur: f32) -> Vec<f32> {
        osc(dur, |_| hz, sine)
    }

    /// Largest jump between neighbouring samples.
    fn max_step(v: &[f32]) -> f32 {
        v.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0., f32::max)
    }

    #[test]
    fn documented_examples_hold() {
        assert_eq!(n_samples(0.5), 22_050);
        assert_eq!(n_samples(-1.), 0);
        assert_eq!(n_samples(f32::NAN), 0);
        assert_eq!(n_samples(1e9), n_samples(600.));
        assert!((midi(69.) - 440.).abs() < 1e-3 && (midi(60.) - 261.63).abs() < 0.01);
        assert!((sine(0.25) - 1.).abs() < 1e-6 && (saw(0.75) - 0.5).abs() < 1e-6);
        assert_eq!((square(0.1), square(0.6)), (1., -1.));
        assert_eq!((pulse(0.2, 0.25), pulse(0.3, 0.25)), (1., -1.));
        assert_eq!((triangle(0.25), triangle(0.75)), (1., -1.));
        assert!((glide(100., 400., 1.)(0.5) - 200.).abs() < 0.01);
        assert_eq!(noise(0.1, &mut Rng::new(1)).len(), 4410);
        assert!((decay(0.1, 0.1) - (-1f32).exp()).abs() < 1e-6);
        assert!(
            (ad(0.005, 0.01, 0.1) - 0.5).abs() < 1e-6 && (ad(0.01, 0.01, 0.1) - 1.).abs() < 1e-6
        );
        assert!((adsr(0.5, 1., 0.01, 0.1, 0.6, 0.2) - 0.6).abs() < 1e-6);
        assert!((adsr(1.1, 1., 0.01, 0.1, 0.6, 0.2) - 0.3).abs() < 1e-5);
        assert_eq!(Mix::new(0.5).len(), 22_050);
        assert_eq!(Mix::with_len(100).len(), 100);
        assert!(Mix::new(0.).is_empty());
        assert_eq!(to_mono(&[1., 0., 0.5, 0.5], 2), vec![0.5, 0.5]);
        assert!(
            (peak(&[0.1, -0.7, 0.3]) - 0.7).abs() < 1e-6 && (rms(&[0.5, -0.5]) - 0.5).abs() < 1e-6
        );
        assert!((tone_level(&sine_wave(440., 1.), 440.) - 1.).abs() < 0.01);
        let a4 = dominant_hz(&sine_wave(440., 0.5), 50., 5000.);
        assert!((a4 / 440. - 1.).abs() < 0.01, "{a4}");
        assert!((spectral_centroid(&sine_wave(1000., 0.5)) / 1000. - 1.).abs() < 0.02);
        assert!((zero_crossing_hz(&sine_wave(1000., 1.)) - 1000.).abs() < 2.);
        assert_eq!(
            bell(1., 880., &[(1., 0.4, 1.), (2.76, 0.2, 0.4)]).len(),
            44_100
        );
        assert_eq!(
            note(0.2, 440., triangle, |t| ad(t, 0.005, 0.08)).len(),
            8820
        );
    }

    #[test]
    fn waveforms_have_the_documented_shape() {
        // Every wave stays within [-1, 1] over the whole phase (and a hair beyond 1.0) and never NaN.
        for i in 0..=1000 {
            let p = i as f32 / 1000.;
            for v in [sine(p), saw(p), square(p), pulse(p, 0.1), triangle(p)] {
                assert!(v.is_finite() && v.abs() <= 1.0001, "{p}: {v}");
            }
        }
        // Sine and triangle start at 0 and rise; saw starts low; the triangle is continuous.
        assert!(sine(0.).abs() < 1e-6 && triangle(0.).abs() < 1e-6 && triangle(1.).abs() < 1e-6);
        assert_eq!(saw(0.), -1.);
        let tri: Vec<f32> = (0..=4000).map(|i| triangle(i as f32 / 4000.)).collect();
        assert!(
            max_step(&tri) < 0.0011,
            "triangle jumps by {}",
            max_step(&tri)
        );
        // Pulse duty cycle follows the width.
        let duty = (0..1000)
            .filter(|i| pulse(*i as f32 / 1000., 0.25) > 0.)
            .count();
        assert_eq!(duty, 250);
    }

    #[test]
    fn osc_accumulates_phase_and_survives_bad_frequencies() {
        // A hard frequency jump still produces a continuous waveform because the phase is accumulated.
        let jumpy = osc(0.2, |t| if t < 0.1 { 200. } else { 1000. }, sine);
        assert!(max_step(&jumpy) < 2. * std::f32::consts::PI * 1000. / SR + 0.01);
        // A glide keeps the sine's step bounded by its instantaneous frequency.
        let sweep = osc(0.5, glide(100., 4000., 0.4), sine);
        assert!(max_step(&sweep) <= TAU * 4000. / SR + 0.01);
        // NaN, infinite and negative frequencies never poison the output.
        for hz in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -440., 0., 1e12] {
            let v = osc(0.05, |_| hz, saw);
            assert_eq!(v.len(), n_samples(0.05));
            assert!(v.iter().all(|x| x.is_finite() && x.abs() <= 1.0001), "{hz}");
        }
        // The first sample of a sine is exactly zero (click-free starts).
        assert_eq!(sine_wave(440., 0.01)[0], 0.);
        assert!(osc(0., |_| 440., sine).is_empty());
    }

    #[test]
    fn glide_is_exponential_holds_and_survives_nonsense() {
        let g = glide(200., 800., 0.5);
        assert!(
            (g(0.) - 200.).abs() < 1e-3
                && (g(0.25) - 400.).abs() < 0.1
                && (g(0.5) - 800.).abs() < 0.1
        );
        assert_eq!((g(5.), g(-1.)), (g(0.5), g(0.)));
        for f in [
            glide(f32::NAN, 100., 1.),
            glide(100., f32::NAN, 1.),
            glide(0., -5., 0.),
        ] {
            assert!([-1., 0., 0.3, 10.]
                .iter()
                .all(|t| f(*t).is_finite() && f(*t) > 0.));
        }
    }

    #[test]
    fn envelopes_stay_in_range_and_have_the_documented_shape() {
        for i in -10..400 {
            let t = i as f32 * 0.01;
            for v in [
                decay(t, 0.3),
                ad(t, 0.05, 0.4),
                adsr(t, 1.5, 0.05, 0.2, 0.4, 0.5),
                adsr(t, 0.01, 0.05, 0.2, 0.4, 0.5),
            ] {
                assert!((0. ..=1.).contains(&v), "{t}: {v}");
            }
        }
        assert_eq!(decay(-1., 0.1), 1.);
        assert!(decay(10., 0.) < 1e-6 && decay(1., f32::NAN).is_finite());
        // ad: linear rise, exact peak, then monotonic decay.
        assert_eq!(ad(0., 0.02, 0.1), 0.);
        assert!((ad(0.02, 0.02, 0.1) - 1.).abs() < 1e-6);
        assert!(ad(0.1, 0.02, 0.1) < ad(0.05, 0.02, 0.1));
        assert_eq!(ad(0.5, 0., 0.1), decay(0.5, 0.1));
        // adsr: peak 1 at the end of the attack, sustain level, release from the sustain level.
        assert!((adsr(0.05, 2., 0.05, 0.1, 0.5, 0.3) - 1.).abs() < 1e-6);
        assert!((adsr(1., 2., 0.05, 0.1, 0.5, 0.3) - 0.5).abs() < 1e-6);
        assert!((adsr(2.15, 2., 0.05, 0.1, 0.5, 0.3) - 0.25).abs() < 1e-5);
        assert_eq!(adsr(3., 2., 0.05, 0.1, 0.5, 0.3), 0.);
        // A note released during the attack releases from the level it reached (0.5), not from 1.
        assert!((adsr(0.025, 0.025, 0.05, 0.1, 0.5, 0.1) - 0.5).abs() < 1e-5);
        assert!((adsr(0.075, 0.025, 0.05, 0.1, 0.5, 0.1) - 0.25).abs() < 1e-5);
        assert!(adsr(0.125, 0.025, 0.05, 0.1, 0.5, 0.1).abs() < 1e-5);
    }

    #[test]
    fn filters_pass_and_reject_the_right_bands() {
        let (lo, mid, hi) = (
            sine_wave(100., 0.5),
            sine_wave(1000., 0.5),
            sine_wave(8000., 0.5),
        );
        let settle = 4000;
        let level = |v: &[f32]| rms(&v[settle..]) * std::f32::consts::SQRT_2;
        // Lowpass at 1 kHz: passes 100 Hz, about -3 dB at the cutoff, rejects 8 kHz.
        assert!(level(&lowpass(&lo, 1000., 0.707)) > 0.95);
        assert!((level(&lowpass(&mid, 1000., 0.707)) - 0.707).abs() < 0.05);
        assert!(level(&lowpass(&hi, 1000., 0.707)) < 0.03);
        // Highpass at 2 kHz: the mirror image.
        assert!(level(&highpass(&lo, 2000., 0.707)) < 0.01);
        assert!(level(&highpass(&hi, 2000., 0.707)) > 0.9);
        // Bandpass at 1 kHz: unity at the centre, both sides down.
        assert!((level(&bandpass(&mid, 1000., 2.)) - 1.).abs() < 0.05);
        assert!(level(&bandpass(&lo, 1000., 2.)) < 0.2 && level(&bandpass(&hi, 1000., 2.)) < 0.2);
        // A higher Q narrows the band around the centre.
        let off_centre = sine_wave(1300., 0.5);
        assert!(
            level(&bandpass(&off_centre, 1000., 8.)) < level(&bandpass(&off_centre, 1000., 1.))
        );
    }

    #[test]
    fn filters_are_stable_for_extreme_cutoff_and_q() {
        let mut rng = Rng::new(9);
        let mut input = noise(0.2, &mut rng);
        input[100] = 1.; // an impulse
        input[200] = f32::NAN; // and bad samples
        input[300] = f32::INFINITY;
        let mut huge = input.clone();
        huge[100] = 1.0e6;
        let cutoffs = [
            f32::NEG_INFINITY,
            -50.,
            0.,
            1.,
            20.,
            100.,
            5000.,
            20_000.,
            22_050.,
            1e9,
            f32::INFINITY,
            f32::NAN,
        ];
        let qs = [
            f32::NEG_INFINITY,
            -1.,
            0.,
            1e-9,
            0.1,
            0.707,
            10.,
            1000.,
            1e9,
            f32::INFINITY,
            f32::NAN,
        ];
        for kind in [FilterKind::Low, FilterKind::High, FilterKind::Band] {
            for &fc in &cutoffs {
                for &q in &qs {
                    let out = filter(&input, kind, |_| fc, q);
                    assert!(out.iter().all(|x| x.is_finite()), "{kind:?} fc={fc} q={q}");
                    // Unit noise through a sane Q can gain at most a few times: instability would show up
                    // as huge or ever-growing values (the finiteness guard would only hide the overflow).
                    let bound = if q <= 10. || q.is_nan() { 50. } else { 1.0e5 };
                    assert!(
                        peak(&out) < bound,
                        "{kind:?} fc={fc} q={q} peaks at {}",
                        peak(&out)
                    );
                    assert!(
                        filter(&huge, kind, |_| fc, q).iter().all(|x| x.is_finite()),
                        "{kind:?} fc={fc} q={q}"
                    );
                }
            }
        }
        // Time-varying cutoffs that misbehave are handled too.
        let wild = filter(
            &input,
            FilterKind::Low,
            |t| {
                if (t * 1000.) as i32 % 2 == 0 {
                    1e9
                } else {
                    f32::NAN
                }
            },
            50.,
        );
        assert!(wild.iter().all(|x| x.is_finite()));
        // A retuned filter keeps its memory; reset clears it.
        let mut f = Biquad::new(FilterKind::Low, 500., 0.7);
        for _ in 0..200 {
            f.process(1.);
        }
        assert!(f.process(0.) > 0.5);
        f.set(FilterKind::Low, 600., 0.7);
        assert!(f.process(0.) > 0.3);
        f.reset();
        assert_eq!(f.process(0.), 0.);
    }

    #[test]
    fn a_sweeping_cutoff_moves_the_filtered_sound() {
        let mut rng = Rng::new(3);
        let white = noise(1.0, &mut rng);
        let closing = filter(&white, FilterKind::Low, |t| 9000. * (0.03f32).powf(t), 0.8);
        let (early, late) = (
            zero_crossing_hz(&closing[..4000]),
            zero_crossing_hz(&closing[40_000..]),
        );
        assert!(early > late * 3., "{early} vs {late}");
        let opening = filter(&white, FilterKind::Band, |t| 300. + 4000. * t, 2.);
        assert!(zero_crossing_hz(&opening[40_000..]) > zero_crossing_hz(&opening[..4000]) * 2.);
    }

    #[test]
    fn mix_places_clips_and_wraps() {
        let mut m = Mix::with_len(10);
        m.add_at(8, &[1., 1., 1., 1.], 0.5); // the last two are dropped
        assert_eq!(&m.as_slice()[6..], &[0., 0., 0.5, 0.5]);
        m.add_at(10, &[9.], 1.); // entirely past the end
        m.add(-1., &[2.], 1.); // negative times start at zero
        assert_eq!((m.as_slice()[0], m.len()), (2., 10));
        // Wrapping: a tail that runs past the end lands at the start.
        let mut w = Mix::with_len(10);
        w.add_wrapped_at(8, &[1., 2., 3., 4.], 1.);
        assert_eq!(w.as_slice(), &[3., 4., 0., 0., 0., 0., 0., 0., 1., 2.]);
        // Start indices wrap too, sources longer than the buffer wrap repeatedly, and it accumulates.
        let mut long = Mix::with_len(4);
        long.add_wrapped_at(6, &[1., 1., 1., 1., 1., 1., 1., 1., 1.], 1.);
        assert_eq!(long.as_slice(), &[2., 2., 3., 2.]);
        // The seconds versions match the sample versions.
        let mut a = Mix::new(1.);
        let mut b = Mix::with_len(44_100);
        a.add_wrapped(0.9, &vec![1.; 10_000], 0.5);
        b.add_wrapped_at(n_samples(0.9), &vec![1.; 10_000], 0.5);
        assert_eq!(a, b);
        assert_eq!(a.as_slice()[0], 0.5);
        // An empty mix never panics.
        let mut e = Mix::new(0.);
        e.add_wrapped(0.5, &[1., 2.], 1.);
        e.add(0.5, &[1., 2.], 1.);
        assert!(e.into_vec().is_empty());
    }

    #[test]
    fn shape_soft_clip_and_finish_behave() {
        let shaped = shape(vec![1.; 44_100], |t| 1. - t);
        assert!(
            (shaped[0] - 1.).abs() < 1e-6
                && (shaped[22_050] - 0.5).abs() < 1e-3
                && shaped[44_099] < 1e-3
        );
        // soft_clip: 1.0 stays 1.0, stays bounded, is odd and monotonic, and squashes the middle up.
        let mut v: Vec<f32> = (-20..=20).map(|i| i as f32 / 10.).collect();
        soft_clip(&mut v, 2.);
        assert!((v[30] - 1.).abs() < 1e-6 && (v[10] + 1.).abs() < 1e-6);
        // Inputs up to +-2 come out within 1 / tanh(2) = 1.0373.
        assert!(v.iter().all(|x| x.is_finite() && x.abs() <= 1.0374));
        assert!(v.windows(2).all(|w| w[1] >= w[0]));
        let mut half = [0.5];
        soft_clip(&mut half, 2.);
        assert!(half[0] > 0.5);
        let mut bad = [0.3];
        soft_clip(&mut bad, f32::NAN);
        assert!(bad[0].is_finite());
        soft_clip(&mut [], 1.);
    }

    #[test]
    fn finish_removes_clicks_and_sets_the_peak() {
        // A constant (the worst click: DC at full scale) becomes a faded, normalised blob.
        let out = finish(vec![1.; 4410], 0.6, 20.);
        assert_eq!(out.len(), 4410);
        assert_eq!(out[0], 0.);
        assert_eq!(out[4409], 0.);
        assert!((peak(&out) - 0.6).abs() < 1e-6);
        assert!(
            max_step(&out) < 0.02,
            "steps stay small: {}",
            max_step(&out)
        );
        // A loud, raw, hard-edged tone: the ends are silent and smooth after finish.
        let raw = osc(0.1, |_| 300., square);
        assert!(raw[0].abs() == 1. && max_step(&raw) == 2.);
        let fin = finish(raw, 0.9, 15.);
        assert!(fin[0].abs() < 1e-6 && fin[fin.len() - 1].abs() < 1e-6);
        assert!((peak(&fin) - 0.9).abs() < 1e-5);
        // Non-finite samples are scrubbed, silence stays silent, peak is clamped, empty is fine.
        let scrub = finish(
            vec![
                f32::NAN,
                0.5,
                f32::INFINITY,
                -0.5,
                0.25,
                0.5,
                0.5,
                0.5,
                0.5,
                0.5,
            ],
            0.5,
            0.,
        );
        assert!(scrub.iter().all(|x| x.is_finite()));
        assert_eq!(finish(vec![0.; 100], 0.9, 10.), vec![0.; 100]);
        assert!(peak(&finish(vec![0.2; 100], 5., 1.)) <= 1.0);
        assert!(finish(vec![], 0.5, 10.).is_empty());
        assert_eq!(finish(vec![1.], 0.5, 10.), vec![0.]);
        // The fade lengths are what the docs say: 1.5 ms in, fade_out_ms out.
        let tone = finish(vec![1.; 44_100], 1., 100.);
        assert!(tone[30] < 0.6 && tone[70] == 1.);
        assert!(tone[44_099 - 4000] > 0.95 && tone[44_099 - 2000] < 0.6);
    }

    #[test]
    fn bell_note_and_to_mono_build_what_they_say() {
        let b = bell(1., 440., &[(1., 0.3, 1.)]);
        assert!(tone_level(&b[..4410], 440.) > 0.5 && tone_level(&b[..4410], 880.) < 0.05);
        assert!(
            rms(&b[..4410]) > rms(&b[30_000..]) * 3.,
            "a bell rings down"
        );
        let n = note(0.5, 440., saw, |t| decay(t, 0.05));
        assert_eq!(n.len(), 22_050);
        assert!(peak(&n[..100]) > peak(&n[20_000..]) * 20.);
        assert_eq!(to_mono(&[0.2, 0.4, 0.6], 1), vec![0.2, 0.4, 0.6]);
        assert_eq!(to_mono(&[1., 3., 5.], 2), vec![2.]); // partial trailing frame dropped
        assert_eq!(to_mono(&[1., 2.], 0), vec![1., 2.]); // zero channels count as mono
        assert!(to_mono(&[], 2).is_empty());
    }

    #[test]
    fn measuring_helpers_read_known_signals() {
        let a4 = sine_wave(440., 1.);
        assert!((peak(&a4) - 1.).abs() < 1e-3);
        assert!((rms(&a4) - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3);
        assert!((tone_level(&a4, 440.) - 1.).abs() < 0.01);
        assert!(tone_level(&a4, 880.) < 0.01 && tone_level(&a4, 300.) < 0.05);
        let half = osc(1., |_| 440., |p| 0.5 * sine(p));
        assert!((tone_level(&half, 440.) - 0.5).abs() < 0.01);
        assert_eq!(
            (
                peak(&[]),
                rms(&[]),
                tone_level(&[], 440.),
                zero_crossing_hz(&[1.])
            ),
            (0., 0., 0., 0.)
        );
        assert_eq!(tone_level(&a4, f32::NAN), 0.);
        // Tones anywhere in the range, of any length, are found to within 1%; so is a tone that is much
        // quieter than a louder low-frequency neighbour (the window keeps the leakage down).
        for hz in [55., 110., 333., 1319., 1760., 4321., 7040., 15_000.] {
            // At least ten cycles: a shorter clip has no well-defined pitch for any spectrum estimate.
            for dur in [0.05f32, 0.4, 2.5] {
                if dur * hz < 10. {
                    continue;
                }
                let found = dominant_hz(&sine_wave(hz, dur), 30., 20_000.);
                assert!(
                    (found / hz - 1.).abs() < 0.01,
                    "{hz} Hz for {dur} s -> {found}"
                );
            }
        }
        let two: Vec<f32> = sine_wave(100., 0.5)
            .iter()
            .zip(sine_wave(2500., 0.5))
            .map(|(a, b)| a + 0.01 * b)
            .collect();
        assert!((dominant_hz(&two, 1000., 5000.) / 2500. - 1.).abs() < 0.01);
        assert!((dominant_hz(&two, 30., 5000.) / 100. - 1.).abs() < 0.02);
        assert_eq!(dominant_hz(&a4, 100., 50.), 0.);
        assert!(
            (dominant_hz(&a4, 0., 5000.) / 440. - 1.).abs() < 0.01,
            "a range starting at 0 ignores DC"
        );
        assert_eq!(dominant_hz(&[], 10., 50.), 0.);
        assert_eq!(dominant_hz(&a4, f32::NAN, 50.), 0.);
        assert_eq!(dominant_hz(&vec![0.; 1000], 10., 5000.), 0.);
        assert_eq!(dominant_hz(&a4, 30_000., 40_000.), 0.);
        // Brightness: a low tone is dull, white noise averages half of Nyquist, filtering darkens it.
        let white = noise(0.5, &mut Rng::new(4));
        let (dull, bright) = (
            spectral_centroid(&sine_wave(200., 0.3)),
            spectral_centroid(&white),
        );
        assert!(
            dull < 300. && (bright / 11_025. - 1.).abs() < 0.05,
            "{dull} {bright}"
        );
        assert!(spectral_centroid(&lowpass(&white, 500., 0.7)) < bright * 0.2);
        assert_eq!(
            (spectral_centroid(&[]), spectral_centroid(&[0.; 100])),
            (0., 0.)
        );
        let up = osc(1., glide(200., 3200., 1.), sine);
        assert!(zero_crossing_hz(&up[40_000..]) > zero_crossing_hz(&up[..4000]) * 5.);
    }

    #[test]
    fn wav_writer_and_parser_round_trip_and_headers_are_right() {
        let w = wav_bytes(&[0., 0.5, -0.5, 1.5, -2., f32::NAN], RATE);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..12], b"WAVE");
        assert_eq!(&w[12..16], b"fmt ");
        assert_eq!(u32::from_le_bytes([w[16], w[17], w[18], w[19]]), 16);
        assert_eq!(u16::from_le_bytes([w[20], w[21]]), 1, "PCM");
        assert_eq!(u16::from_le_bytes([w[22], w[23]]), 1, "mono");
        assert_eq!(u32::from_le_bytes([w[24], w[25], w[26], w[27]]), RATE);
        assert_eq!(
            u32::from_le_bytes([w[28], w[29], w[30], w[31]]),
            RATE * 2,
            "byte rate"
        );
        assert_eq!(u16::from_le_bytes([w[32], w[33]]), 2, "block align");
        assert_eq!(u16::from_le_bytes([w[34], w[35]]), 16, "bits");
        assert_eq!(&w[36..40], b"data");
        let data_len = u32::from_le_bytes([w[40], w[41], w[42], w[43]]) as usize;
        assert_eq!(data_len, 12);
        assert_eq!(w.len(), 44 + data_len);
        assert_eq!(
            u32::from_le_bytes([w[4], w[5], w[6], w[7]]) as usize,
            w.len() - 8,
            "RIFF size"
        );
        let pcm = |i: usize| i16::from_le_bytes([w[44 + 2 * i], w[45 + 2 * i]]);
        assert_eq!((pcm(0), pcm(1), pcm(2)), (0, 16384, -16384));
        assert_eq!(
            (pcm(3), pcm(4), pcm(5)),
            (32767, -32767, 0),
            "clamped, not wrapped; NaN is silence"
        );
        // Round trip within one 16-bit step, for a real tone.
        let tone = finish(osc(0.25, glide(200., 900., 0.2), triangle), 0.9, 10.);
        let (rate, channels, back) = parse_wav(&wav_bytes(&tone, 22_050)).expect("parses");
        assert_eq!((rate, channels, back.len()), (22_050, 1, tone.len()));
        let worst = tone
            .iter()
            .zip(&back)
            .map(|(a, b)| (a - b).abs())
            .fold(0., f32::max);
        assert!(worst <= 0.5 / 32767. + 1e-6, "round trip error {worst}");
        // Full scale survives exactly.
        let (_, _, full) = parse_wav(&wav_bytes(&[1., -1., 0.], RATE)).expect("parses");
        assert_eq!(full, vec![1., -1., 0.]);
    }

    #[test]
    fn stereo_wav_interleaves_pads_and_parses() {
        let w = wav_bytes_stereo(&[1., 0.5, 0.25], &[-1.], RATE);
        assert_eq!(u16::from_le_bytes([w[22], w[23]]), 2);
        assert_eq!(u32::from_le_bytes([w[28], w[29], w[30], w[31]]), RATE * 4);
        assert_eq!(u16::from_le_bytes([w[32], w[33]]), 4);
        let (rate, channels, s) = parse_wav(&w).expect("parses");
        assert_eq!((rate, channels), (RATE, 2));
        assert_eq!(s.len(), 6);
        assert_eq!(
            (s[0], s[1], s[3], s[5]),
            (1., -1., 0., 0.),
            "left, right, then the padded silence"
        );
        assert!((s[2] - 0.5).abs() < 1e-4 && (s[4] - 0.25).abs() < 1e-4);
        assert_eq!(to_mono(&s, 2).len(), 3);
        assert_eq!(
            parse_wav(&wav_bytes_stereo(&[], &[], RATE))
                .expect("parses")
                .2
                .len(),
            0
        );
    }

    #[test]
    fn parse_wav_reads_odd_but_legal_files_and_rejects_bad_ones_without_panicking() {
        let good = wav_bytes(&sine_wave(440., 0.05), RATE);
        // Extra chunks (with odd sizes and padding) before and after are skipped.
        let mut padded = good[..12].to_vec();
        padded.extend_from_slice(b"junk");
        padded.extend_from_slice(&3u32.to_le_bytes());
        padded.extend_from_slice(&[1, 2, 3, 0]); // 3 bytes + 1 pad
        padded.extend_from_slice(&good[12..]);
        padded.extend_from_slice(b"LIST");
        padded.extend_from_slice(&4u32.to_le_bytes());
        padded.extend_from_slice(&[9, 9, 9, 9]);
        let (_, _, a) = parse_wav(&good).expect("good");
        assert_eq!(parse_wav(&padded).expect("padded").2, a);
        // A data chunk that claims more than the file holds reads what is there.
        let mut lying = good.clone();
        lying[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(parse_wav(&lying).expect("lying size").2, a);
        // Named errors.
        assert!(parse_wav(&[]).unwrap_err().contains("RIFF"));
        assert!(parse_wav(b"RIFF\0\0\0\0WAVX").unwrap_err().contains("RIFF"));
        assert!(parse_wav(&good[..12]).unwrap_err().contains("fmt"));
        assert!(parse_wav(&good[..36]).unwrap_err().contains("data"));
        let mut float = good.clone();
        float[20] = 3; // IEEE float tag
        assert!(parse_wav(&float).unwrap_err().contains("16-bit PCM"));
        let mut eight = good.clone();
        eight[34] = 8;
        assert!(parse_wav(&eight).unwrap_err().contains("16-bit PCM"));
        let mut zero_channels = good.clone();
        zero_channels[22] = 0;
        assert!(parse_wav(&zero_channels).is_err());
        let mut short_fmt = good.clone();
        short_fmt[16] = 8;
        assert!(parse_wav(&short_fmt[..30]).is_err());
        // Every truncation and a sweep of single-byte corruptions must return Ok or Err, never panic.
        for len in 0..good.len() {
            let _ = parse_wav(&good[..len]);
        }
        for i in 0..good.len().min(80) {
            for v in [0u8, 1, 0x7F, 0x80, 0xFF] {
                let mut bad = good.clone();
                bad[i] = v;
                let _ = parse_wav(&bad);
            }
        }
    }

    #[test]
    fn band_limited_oscillators_alias_far_less_than_the_naive_waves() {
        let folded = |hz: f32| {
            let m = hz % SR;
            if m > SR / 2. {
                SR - m
            } else {
                m
            }
        };
        for f0 in [1000f32, 2000., 3000.] {
            for (wave, naive, odd_only) in [
                (Wave::Saw, saw as fn(f32) -> f32, false),
                (Wave::Square, square as fn(f32) -> f32, true),
            ] {
                let (plain, clean) = (osc(0.5, |_| f0, naive), osc_bl(0.5, |_| f0, wave));
                // Overtones above Nyquist fold back down to these inharmonic frequencies.
                let alias = |v: &[f32]| -> f32 {
                    (2..=60usize)
                        .filter(|k| *k as f32 * f0 > SR / 2. && (!odd_only || k % 2 == 1))
                        .map(|k| tone_level(v, folded(k as f32 * f0)).powi(2))
                        .sum::<f32>()
                        .sqrt()
                };
                let (bad, good) = (alias(&plain), alias(&clean));
                assert!(
                    good < 0.3 * bad,
                    "{wave:?} at {f0} Hz: band-limited alias {good} vs naive {bad}"
                );
                // The naive alias is loud (a fifth of the note or more at 2 kHz and up); the note itself is intact.
                if f0 >= 2000. {
                    assert!(
                        bad > 0.15 * tone_level(&plain, f0),
                        "{wave:?} at {f0} Hz: naive alias is only {bad}"
                    );
                }
                assert!(
                    (tone_level(&clean, f0) / tone_level(&plain, f0) - 1.).abs() < 0.03,
                    "{wave:?} at {f0} Hz loses the fundamental"
                );
            }
        }
    }

    #[test]
    fn osc_bl_makes_clean_waves_with_the_documented_range() {
        for (wave, lo, hi) in [
            (Wave::Saw, -1.05, 1.05),
            (Wave::Square, -1.15, 1.15),
            (Wave::Pulse(0.3), -0.7, 1.5),
        ] {
            let v = osc_bl(0.3, |_| 440., wave);
            assert_eq!(v.len(), n_samples(0.3));
            assert!(
                v.iter().all(|x| x.is_finite() && (lo..=hi).contains(x)),
                "{wave:?} range"
            );
            if !matches!(wave, Wave::Pulse(_)) {
                assert!(v[0].abs() < 1e-6, "{wave:?} starts at exactly zero");
            }
            assert_near(dominant_hz(&v, 100., 2000.), 440., 0.01, "pitch");
        }
        // The pulse has no DC offset and its duty cycle follows the width; a NaN width counts as a square.
        let pulse_wave = osc_bl(1., |_| 441., Wave::Pulse(0.3));
        assert!((pulse_wave.iter().sum::<f32>() / pulse_wave.len() as f32).abs() < 0.01);
        let positive =
            pulse_wave.iter().filter(|x| **x > 0.).count() as f32 / pulse_wave.len() as f32;
        assert!((positive - 0.3).abs() < 0.02, "duty cycle {positive}");
        assert_eq!(
            osc_bl(0.05, |_| 300., Wave::Pulse(f32::NAN)),
            osc_bl(0.05, |_| 300., Wave::Square)
        );
        assert_eq!(
            osc_bl(0.05, |_| 300., Wave::Pulse(0.5)),
            osc_bl(0.05, |_| 300., Wave::Square),
            "a 50% pulse is a square"
        );
        // Width is clamped rather than producing silence or garbage.
        for width in [-1., 0., 1., 5.] {
            let v = osc_bl(0.05, |_| 300., Wave::Pulse(width));
            assert!(v.iter().all(|x| x.is_finite() && x.abs() < 2.1) && peak(&v) > 0.5);
        }
        // Bad frequencies never poison the output; negative counts as positive; at or above 0.49 x rate is silent.
        for hz in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            0.,
            -500.,
            1e9,
            30_000.,
        ] {
            for wave in [Wave::Saw, Wave::Square, Wave::Pulse(0.2)] {
                let v = osc_bl(0.05, |_| hz, wave);
                assert_eq!(v.len(), n_samples(0.05));
                assert!(
                    v.iter().all(|x| x.is_finite() && x.abs() < 2.1),
                    "{hz} {wave:?}"
                );
                if hz == 0. || hz.abs() >= 0.49 * SR || !hz.is_finite() {
                    assert!(v.iter().all(|x| *x == 0.), "{hz} Hz must be silent");
                }
            }
        }
        assert_eq!(
            osc_bl(0.05, |_| -500., Wave::Saw),
            osc_bl(0.05, |_| 500., Wave::Saw)
        );
        assert!(osc_bl(0., |_| 440., Wave::Saw).is_empty());
        // A sweep stays continuous: steps are bounded by the wave's own edges, never by aliasing glitches.
        let sweep = osc_bl(0.5, glide(200., 8000., 0.4), Wave::Saw);
        assert!(sweep.iter().all(|x| x.is_finite() && x.abs() < 1.1));
        assert!(
            zero_crossing_hz(&sweep[SR as usize / 4..]) > 3. * zero_crossing_hz(&sweep[..2000])
        );
    }

    // ------------------------------------------------------------------------------ presets

    use std::sync::OnceLock;
    use std::time::Instant;

    type Bank = Vec<(Preset, usize, Vec<f32>)>;

    /// Every preset and variant rendered once with seed 1; the tests that only read audio share it.
    fn bank() -> &'static Bank {
        static BANK: OnceLock<Bank> = OnceLock::new();
        BANK.get_or_init(|| {
            Preset::ALL
                .into_iter()
                .flat_map(|p| (0..p.variants()).map(move |v| (p, v, render(p, v, 1))))
                .collect()
        })
    }

    fn take(p: Preset, variant: usize) -> &'static [f32] {
        &bank()
            .iter()
            .find(|(q, v, _)| *q == p && *v == variant)
            .expect("rendered")
            .2
    }

    fn of(p: Preset) -> &'static [f32] {
        take(p, 0)
    }

    fn millis(v: &[f32]) -> f32 {
        v.len() as f32 * 1000. / SR
    }

    /// Low-end against high-end level: rms below 250 Hz over rms above 2.5 kHz.
    fn low_to_high(v: &[f32]) -> f32 {
        rms(&lowpass(v, 250., 0.707)) / rms(&highpass(v, 2500., 0.707)).max(1e-6)
    }

    fn assert_near(actual: f32, expected: f32, tolerance: f32, what: &str) {
        assert!(
            (actual / expected - 1.).abs() < tolerance,
            "{what}: {actual} Hz, expected {expected} Hz"
        );
    }

    #[test]
    fn every_preset_variant_is_finite_bounded_clean_and_sane() {
        assert_eq!(
            bank().len(),
            Preset::ALL.iter().map(|p| p.variants()).sum::<usize>()
        );
        for (p, v, s) in bank() {
            let tag = format!("{}/{v}", p.name());
            assert!(!s.is_empty(), "{tag} is empty");
            assert!(
                s.iter().all(|x| x.is_finite() && x.abs() <= 0.98 + 1e-6),
                "{tag} has bad samples"
            );
            let loudest = peak(s);
            assert!((0.25..=0.98).contains(&loudest), "{tag} peak {loudest}");
            let (first, last) = (s[0], s[s.len() - 1]);
            assert!(
                first.abs() < 0.005 && last.abs() < 0.005,
                "{tag} does not start and end at silence: {first} {last}"
            );
            let secs = s.len() as f32 / SR;
            assert!((0.04..=2.5).contains(&secs), "{tag} lasts {secs} s");
            assert!(rms(s) > 0.03, "{tag} is nearly silent: rms {}", rms(s));
            let mean = s.iter().sum::<f32>() / s.len() as f32;
            assert!(mean.abs() < 0.02, "{tag} has a DC offset of {mean}");
            // No long stretch of silence at the end: the last 100 ms (minus the final 20 ms fade) has signal.
            let n = s.len();
            let tail = &s[n.saturating_sub(n_samples(0.1))..n.saturating_sub(n_samples(0.02))];
            assert!(peak(tail) > 0.005, "{tag} ends in silence");
        }
    }

    #[test]
    fn rendering_is_deterministic_and_seed_and_variant_change_the_sound() {
        for (p, v, s) in bank() {
            assert_eq!(
                &render(*p, *v, 1),
                s,
                "{}/{v} is not deterministic",
                p.name()
            );
        }
        for p in Preset::ALL {
            let (a, b) = (render(p, 0, 1), render(p, 0, 2));
            assert_ne!(a, b, "{}: the seed changes nothing", p.name());
            assert_eq!(
                a.len(),
                b.len(),
                "{}: the seed must not change the length",
                p.name()
            );
            for v in 0..p.variants() {
                for w in v + 1..p.variants() {
                    assert_ne!(
                        take(p, v),
                        take(p, w),
                        "{}: variants {v} and {w} are identical",
                        p.name()
                    );
                }
                // Variants wrap modulo the count, so a running counter gives round-robin playback.
                assert_eq!(
                    &render(p, v + 3 * p.variants(), 1),
                    take(p, v),
                    "{}: variant does not wrap",
                    p.name()
                );
            }
        }
        // Extreme arguments still give a finite, audible sound.
        for p in Preset::ALL {
            for (variant, seed) in [(usize::MAX, 0), (0, u64::MAX), (12_345, 987_654_321)] {
                let s = render(p, variant, seed);
                assert!(
                    s.iter().all(|x| x.is_finite()) && peak(&s) > 0.25,
                    "{}",
                    p.name()
                );
            }
        }
    }

    #[test]
    fn presets_are_distinct_from_each_other() {
        let features: Vec<(Preset, f32, f32, f32)> = Preset::ALL
            .iter()
            .map(|&p| {
                (
                    p,
                    millis(of(p)),
                    spectral_centroid(of(p)),
                    dominant_hz(of(p), 40., 16_000.),
                )
            })
            .collect();
        let apart = |a: f32, b: f32, ratio: f32| a.max(b) / a.min(b).max(1e-3) > ratio;
        for (i, x) in features.iter().enumerate() {
            for y in &features[i + 1..] {
                assert!(
                    apart(x.1, y.1, 1.1) || apart(x.2, y.2, 1.15) || apart(x.3, y.3, 1.1),
                    "{} and {} are too alike: (ms, centroid, dominant) {:?} vs {:?}",
                    x.0.name(),
                    y.0.name(),
                    (x.1, x.2, x.3),
                    (y.1, y.2, y.3)
                );
            }
        }
    }

    #[test]
    fn low_impacts_are_bass_heavy_and_bright_sounds_are_bright() {
        let level_below =
            |s: &[f32], hz: &[f32]| hz.iter().map(|&f| tone_level(s, f)).fold(0., f32::max);
        for p in [Preset::Thump, Preset::Explosion, Preset::Land, Preset::Hit] {
            let s = of(p);
            assert!(
                low_to_high(s) > 5.,
                "{} should be dominated by low end: {}",
                p.name(),
                low_to_high(s)
            );
            assert!(
                spectral_centroid(s) < 300.,
                "{} centroid {}",
                p.name(),
                spectral_centroid(s)
            );
        }
        // In Goertzel terms: the deepest sounds have far more tone near 40-100 Hz than near 4-8 kHz.
        for p in [Preset::Thump, Preset::Explosion] {
            let s = of(p);
            let low = level_below(s, &[40., 50., 60., 70., 80., 90., 100.]);
            let high = level_below(s, &[4000., 6000., 8000.]);
            assert!(
                low > 10. * high.max(1e-6),
                "{}: low {low} vs high {high}",
                p.name()
            );
        }
        for p in [
            Preset::Coin,
            Preset::Blip,
            Preset::Click,
            Preset::Pickup,
            Preset::PowerUp,
            Preset::Whoosh,
            Preset::Zap,
        ] {
            assert!(
                spectral_centroid(of(p)) > 900.,
                "{} should be bright: {}",
                p.name(),
                spectral_centroid(of(p))
            );
        }
        for p in [Preset::Coin, Preset::Blip, Preset::Zap, Preset::Whoosh] {
            assert!(
                low_to_high(of(p)) < 0.3,
                "{} low/high {}",
                p.name(),
                low_to_high(of(p))
            );
        }
        // Documented pitches: the coin's long note is E6, the blip is A5, the chime rings at C6.
        assert_near(
            dominant_hz(of(Preset::Coin), 500., 4000.),
            midi(88.),
            0.05,
            "coin",
        );
        assert_near(
            dominant_hz(of(Preset::Blip), 300., 3000.),
            midi(81.),
            0.05,
            "blip",
        );
        assert_near(
            dominant_hz(of(Preset::Chime), 300., 3000.),
            midi(84.),
            0.05,
            "chime",
        );
        // The warning alternates B5 and F#5.
        let w = of(Preset::Warning);
        assert_near(
            dominant_hz(&w[n_samples(0.01)..n_samples(0.09)], 300., 3000.),
            988.,
            0.04,
            "warning high",
        );
        assert_near(
            dominant_hz(&w[n_samples(0.14)..n_samples(0.22)], 300., 3000.),
            740.,
            0.04,
            "warning low",
        );
    }

    #[test]
    fn sweeps_go_the_documented_direction() {
        let ends = |s: &[f32]| {
            let fifth = s.len() / 5;
            (
                zero_crossing_hz(&s[..fifth]),
                zero_crossing_hz(&s[s.len() - fifth..]),
            )
        };
        for p in [
            Preset::Zap,
            Preset::Shoot,
            Preset::PowerDown,
            Preset::Hurt,
            Preset::Explosion,
        ] {
            let (early, late) = ends(of(p));
            assert!(
                early > 1.5 * late,
                "{} should fall: {early} -> {late}",
                p.name()
            );
        }
        for p in [Preset::Whoosh, Preset::PowerUp, Preset::Jump] {
            let (early, late) = ends(of(p));
            assert!(
                late > 1.5 * early,
                "{} should rise: {early} -> {late}",
                p.name()
            );
        }
        // Select rises by a fifth, Back falls by a fifth.
        let notes = |s: &[f32]| {
            let first = dominant_hz(&s[n_samples(0.01)..n_samples(0.06)], 300., 2000.);
            (
                first,
                dominant_hz(&s[n_samples(0.15)..n_samples(0.23)], 300., 2000.),
            )
        };
        let (first, second) = notes(of(Preset::Select));
        assert!(
            (second / first - 1.5).abs() < 0.06,
            "select {first} -> {second}"
        );
        let (first, second) = notes(of(Preset::Back));
        assert!(
            (first / second - 1.5).abs() < 0.06,
            "back {first} -> {second}"
        );
    }

    #[test]
    fn ladders_climb_with_the_variant_and_track_the_documented_pitches() {
        // The coin's long E6 and the blip's A5 climb the documented 0, 2, 4, 7 semitones.
        for (p, lo, hi, base) in [
            (Preset::Coin, 500., 4000., 88.),
            (Preset::Blip, 300., 3000., 81.),
        ] {
            let mut previous = 0.;
            for (k, step) in LADDER.iter().enumerate() {
                let f = dominant_hz(take(p, k), lo, hi);
                assert_near(f, midi(base + step), 0.04, &format!("{}/{k}", p.name()));
                assert!(
                    f > previous * 1.05,
                    "{}/{k} does not climb: {previous} -> {f}",
                    p.name()
                );
                previous = f;
            }
        }
        // Pickup and the two-note presets shift as a whole: brightness follows the ladder.
        let climb = |p: Preset, from: usize, to: usize, semis: f32| {
            let ratio = spectral_centroid(take(p, to)) / spectral_centroid(take(p, from));
            assert!(
                (ratio / 2f32.powf(semis / 12.) - 1.).abs() < 0.08,
                "{}: {from} -> {to} ratio {ratio}",
                p.name()
            );
        };
        climb(Preset::Pickup, 0, 3, 7.);
        climb(Preset::Pickup, 0, 1, 2.);
        climb(Preset::Select, 0, 1, 2.);
        climb(Preset::Chime, 0, 1, 2.);
    }

    #[test]
    fn documented_durations_and_names_are_true() {
        use Preset::*;
        let documented = [
            (Click, 70.),
            (Select, 260.),
            (Back, 240.),
            (Blip, 70.),
            (Coin, 500.),
            (Pickup, 420.),
            (PowerUp, 700.),
            (Jump, 200.),
            (Land, 300.),
            (Hit, 320.),
            (Thump, 550.),
            (Explosion, 1400.),
            (Zap, 400.),
            (Shoot, 240.),
            (Whoosh, 550.),
            (Error, 270.),
            (Warning, 500.),
            (Success, 1100.),
            (GameOver, 1600.),
            (Footstep, 160.),
            (PowerDown, 650.),
            (Hurt, 320.),
            (Chime, 1200.),
        ];
        assert_eq!(documented.len(), Preset::ALL.len());
        for (p, ms) in documented {
            for v in 0..p.variants() {
                assert!(
                    (millis(take(p, v)) - ms).abs() < 1.,
                    "{}/{v}: the docs say {ms} ms, the code renders {} ms",
                    p.name(),
                    millis(take(p, v))
                );
            }
        }
    }

    #[test]
    fn heavy_sounds_are_louder_than_interface_sounds() {
        assert!(rms(of(Preset::Explosion)) > rms(of(Preset::Click)) * 3.);
        assert!(rms(of(Preset::Thump)) > rms(of(Preset::Blip)) * 1.5);
        assert!(rms(of(Preset::Hit)) > rms(of(Preset::Click)) * 1.5);
        assert!(
            rms(of(Preset::Warning)) < rms(of(Preset::Explosion)),
            "the alarm must not out-shout the biggest sound"
        );
        assert!(peak(of(Preset::Explosion)) > peak(of(Preset::Blip)));
    }

    #[test]
    fn indices_names_and_variant_counts_are_consistent() {
        let mut names = std::collections::HashSet::new();
        for (i, p) in Preset::ALL.iter().enumerate() {
            assert_eq!(p.index(), i);
            assert!(names.insert(p.name()), "duplicate name {}", p.name());
            assert!(
                p.name().chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{}",
                p.name()
            );
            assert_eq!(Preset::from_name(p.name()), Some(*p));
            assert!((1..=4).contains(&p.variants()), "{}", p.name());
        }
        assert_eq!(Preset::from_name("nope"), None);
        assert_eq!(
            Preset::ALL.len(),
            Preset::Chime.index() + 1,
            "ALL must list every variant of the enum"
        );
        assert_eq!(Preset::ALL.len(), 23);
        for ladder in [Preset::Coin, Preset::Pickup, Preset::Blip, Preset::Footstep] {
            assert_eq!(ladder.variants(), 4);
        }
    }

    #[test]
    fn render_is_valid_deterministic_and_never_panics_for_any_variant_and_seed() {
        // Edge variants and seeds (zero, one, the maxima, the top bit, a large odd constant), plus random pairs.
        let edges: [(usize, u64); 6] = [
            (0, 0),
            (1, 1),
            (usize::MAX, u64::MAX),
            (usize::MAX - 1, u64::MAX - 1),
            (64, 1 << 63),
            (1000, 0x9E37_79B9_7F4A_7C15),
        ];
        let random_cases = if cfg!(debug_assertions) { 1 } else { 12 };
        let mut rng = Rng::new(0xC0FFEE);
        for p in Preset::ALL {
            let mut probes = edges.to_vec();
            for _ in 0..random_cases {
                probes.push((rng.next_u64() as usize, rng.next_u64()));
            }
            for (variant, seed) in probes {
                let tag = format!("{} variant {variant} seed {seed}", p.name());
                let s = p.render(variant, seed);
                assert!(
                    !s.is_empty() && s.iter().all(|x| x.is_finite()),
                    "{tag}: bad samples"
                );
                let loudest = peak(&s);
                assert!((0.25..=0.98).contains(&loudest), "{tag}: peak {loudest}");
                assert!(
                    s[0].abs() < 0.005 && s[s.len() - 1].abs() < 0.005,
                    "{tag}: edges"
                );
                assert!(
                    (0.04..=2.5).contains(&(s.len() as f32 / SR)),
                    "{tag}: duration"
                );
                assert_eq!(
                    s,
                    render(p, variant, seed),
                    "{tag}: the method and the function disagree or are not deterministic"
                );
                assert_eq!(
                    s.len(),
                    of(p).len(),
                    "{tag}: the length must not depend on variant or seed"
                );
            }
        }
    }

    // --------------------------------------------------------------------------------- music

    fn tiny_spec() -> MusicSpec {
        MusicSpec {
            bars: 2,
            ..MusicSpec::default()
        }
    }

    fn short_spec() -> MusicSpec {
        MusicSpec {
            bars: 4,
            ..MusicSpec::default()
        }
    }

    fn short_loop() -> &'static Stems {
        static LOOP: OnceLock<Stems> = OnceLock::new();
        LOOP.get_or_init(|| music_loop(&short_spec()))
    }

    fn full_loop() -> &'static Stems {
        static LOOP: OnceLock<Stems> = OnceLock::new();
        LOOP.get_or_init(|| music_loop(&MusicSpec::default()))
    }

    #[test]
    fn music_spec_lengths_and_clamps_are_documented_behaviour() {
        let s = MusicSpec {
            bpm: 120.,
            bars: 16,
            ..MusicSpec::default()
        };
        assert_eq!(s.loop_samples(), 1_411_200);
        assert!((s.loop_seconds() - 32.).abs() < 1e-3 && (s.beat_seconds() - 0.5).abs() < 1e-6);
        assert_eq!(
            MusicSpec::default().loop_samples(),
            1_365_677,
            "16 bars at 124 bpm"
        );
        assert_eq!(
            (
                MusicSpec::default().bpm,
                MusicSpec::default().bars,
                MusicSpec::default().root_midi
            ),
            (124., 16, 57)
        );
        assert_eq!(
            MusicSpec { bars: 0, ..s }.loop_samples(),
            MusicSpec { bars: 1, ..s }.loop_samples()
        );
        assert_eq!(
            MusicSpec { bars: 1000, ..s }.loop_samples(),
            MusicSpec { bars: 64, ..s }.loop_samples()
        );
        assert_eq!(
            MusicSpec { bpm: f32::NAN, ..s }.loop_samples(),
            s.loop_samples(),
            "NaN counts as 120 bpm"
        );
        assert_eq!(
            MusicSpec { bpm: 1., ..s }.loop_samples(),
            MusicSpec { bpm: 40., ..s }.loop_samples()
        );
        assert_eq!(
            MusicSpec { bpm: 1e9, ..s }.loop_samples(),
            MusicSpec { bpm: 240., ..s }.loop_samples()
        );
        // Odd corners still render finite, correctly sized stems.
        for spec in [
            MusicSpec {
                bpm: f32::NAN,
                bars: 1,
                ..s
            },
            MusicSpec {
                bpm: 240.,
                bars: 1,
                root_midi: 0,
                minor: false,
                seed: 5,
            },
            MusicSpec {
                bpm: 40.,
                bars: 3,
                root_midi: 255,
                minor: true,
                seed: u64::MAX,
            },
        ] {
            let m = music_loop(&spec);
            for stem in [&m.base, &m.melodic, &m.lead] {
                assert_eq!(stem.len(), spec.loop_samples());
                assert!(
                    stem.iter().all(|x| x.is_finite() && x.abs() <= 1.0),
                    "{spec:?}"
                );
                assert!(peak(stem) > 0.3, "{spec:?} is silent");
            }
        }
    }

    #[test]
    fn music_stems_are_synchronised_bounded_non_silent_and_normalised() {
        let m = full_loop();
        let n = MusicSpec::default().loop_samples();
        for (name, s, want) in [
            ("base", &m.base, 0.8),
            ("melodic", &m.melodic, 0.6),
            ("lead", &m.lead, 0.6),
        ] {
            assert_eq!(s.len(), n, "{name} length");
            assert!(
                s.iter().all(|x| x.is_finite() && x.abs() <= 1.0),
                "{name} bad samples"
            );
            assert!((peak(s) - want).abs() < 1e-3, "{name} peak {}", peak(s));
            assert!(rms(s) > 0.05, "{name} is too quiet: rms {}", rms(s));
            // No second of the loop is silent.
            for (i, second) in s.chunks(RATE as usize).enumerate() {
                assert!(rms(second) > 0.002, "{name} is silent during second {i}");
            }
            let mean = s.iter().sum::<f32>() / s.len() as f32;
            assert!(mean.abs() < 0.01, "{name} has a DC offset of {mean}");
        }
        // Each stem has its own character: the base lives in the lows, the lead in the mids.
        assert!(
            spectral_centroid(&m.base) < 400.,
            "base centroid {}",
            spectral_centroid(&m.base)
        );
        assert!(
            spectral_centroid(&m.lead) > 700.,
            "lead centroid {}",
            spectral_centroid(&m.lead)
        );
        assert!(spectral_centroid(&m.lead) > 2. * spectral_centroid(&m.base));
    }

    #[test]
    fn music_loops_without_a_seam() {
        let m = full_loop();
        for (name, s) in [
            ("base", &m.base),
            ("melodic", &m.melodic),
            ("lead", &m.lead),
        ] {
            let n = s.len();
            let seam = (s[n - 1] - s[0]).abs();
            let step = |a: &[f32]| {
                a.windows(2)
                    .map(|w| (w[1] - w[0]).abs())
                    .fold(0f32, f32::max)
            };
            let typical = s.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f32>() / (n - 1) as f32;
            let local = step(&s[n - 2000..]).max(step(&s[..2000]));
            assert!(
                seam <= 3. * typical.max(1e-4),
                "{name}: seam step {seam} vs typical step {typical}"
            );
            assert!(
                seam <= 3. * local + 0.002,
                "{name}: seam step {seam} vs local steps {local}"
            );
        }
    }

    fn short_ambient() -> AmbientSpec {
        AmbientSpec {
            minutes: 0.5,
            chords: 2,
            ..AmbientSpec::default()
        }
    }

    #[test]
    fn ambient_is_deterministic_and_loop_seconds_matches_render_length() {
        let spec = short_ambient();
        let a = ambient_loop(&spec);
        let b = ambient_loop(&spec);
        assert_eq!(a, b, "same spec, same samples");
        assert_eq!(a.len(), n_samples(spec.loop_seconds()));
        let mut different = spec.clone();
        different.seed = 2;
        assert_ne!(
            a,
            ambient_loop(&different),
            "a different seed changes the render"
        );
    }

    #[test]
    fn ambient_clamps_degenerate_input() {
        let wild = AmbientSpec {
            minutes: f32::NAN,
            chords: 0,
            brightness: f32::INFINITY,
            ..AmbientSpec::default()
        };
        assert_eq!(wild.loop_seconds(), 120.);
        let rendered = ambient_loop(&wild);
        assert!(rendered.iter().all(|x| x.is_finite()));
        assert!(peak(&rendered) <= 1.0001);
        let long = AmbientSpec {
            minutes: 999.,
            ..AmbientSpec::default()
        };
        assert_eq!(
            long.loop_seconds(),
            600.,
            "clamped to the ten-minute buffer cap"
        );
    }

    #[test]
    fn ambient_loops_without_a_seam() {
        let s = ambient_loop(&short_ambient());
        let n = s.len();
        let seam = (s[n - 1] - s[0]).abs();
        let step = |a: &[f32]| {
            a.windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .fold(0f32, f32::max)
        };
        let typical = s.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f32>() / (n - 1) as f32;
        let local = step(&s[n - 2000..]).max(step(&s[..2000]));
        assert!(
            seam <= 3. * typical.max(1e-4),
            "seam step {seam} vs typical step {typical}"
        );
        assert!(
            seam <= 3. * local + 0.002,
            "seam step {seam} vs local steps {local}"
        );
    }

    #[test]
    fn ambient_is_soft_and_has_no_sharp_beat_like_the_dance_loop() {
        let ambient = ambient_loop(&short_ambient());
        assert!(
            peak(&ambient) <= 0.46,
            "a soft loop should not approach full scale: peak {}",
            peak(&ambient)
        );
        assert!(
            rms(&ambient) > 0.03,
            "a soft loop should still be audible, not near silence: {}",
            rms(&ambient)
        );
        // No beat means no strong periodicity at a plausible tempo in the low end, unlike the dance
        // loop's four-on-the-floor kick, which the same measure finds strongly periodic.
        let (_, ambient_strength) = envelope_period(&ambient, n_samples(0.2), n_samples(2.0));
        let (_, beat_strength) = envelope_period(&full_loop().base, n_samples(0.2), n_samples(2.0));
        assert!(
            ambient_strength < 0.5,
            "ambient low end should not pulse at a steady beat: {ambient_strength}"
        );
        assert!(
            beat_strength > 0.5,
            "sanity: the dance loop's own kick should register as periodic: {beat_strength}"
        );
    }

    #[test]
    fn ambient_brightness_moves_the_spectrum_without_changing_the_chords() {
        let dark = AmbientSpec {
            brightness: 0.,
            ..short_ambient()
        };
        let bright = AmbientSpec {
            brightness: 1.,
            ..short_ambient()
        };
        assert!(
            spectral_centroid(&ambient_loop(&dark)) < spectral_centroid(&ambient_loop(&bright)),
            "brightness should raise the spectral centroid"
        );
    }

    #[test]
    fn mood_bias_reads_dark_and_bright_words_and_is_neutral_otherwise() {
        assert_eq!(mood_bias("Spooky Kart"), mood_bias("haunted"));
        assert!(mood_bias("Eight haunted karts, one hollow to win.") < 0.);
        assert!(mood_bias("Sunny Meadow Festival") > 0.);
        assert_eq!(mood_bias("Block Puzzle Adventure"), 0.);
        assert_eq!(mood_bias(""), 0.);
        assert_eq!(
            mood_bias("HAUNTED"),
            mood_bias("haunted"),
            "case-insensitive"
        );
    }

    #[test]
    fn ambient_spec_for_is_deterministic_and_varies_with_the_title() {
        let a = ambient_spec_for("Spooky Kart", "Eight haunted karts, one hollow to win.");
        let b = ambient_spec_for("Spooky Kart", "Eight haunted karts, one hollow to win.");
        assert_eq!(a, b, "same title and tagline, same spec");
        let different_title =
            ambient_spec_for("Garden Golf", "Eight haunted karts, one hollow to win.");
        assert_ne!(
            a.seed, different_title.seed,
            "a different title changes the seed"
        );
        let different_tagline = ambient_spec_for("Spooky Kart", "A sunny afternoon on the green.");
        assert_ne!(
            (a.minor, a.root_midi),
            (different_tagline.minor, different_tagline.root_midi),
            "a different tagline's mood should move mode/register"
        );
    }

    #[test]
    fn a_clearly_themed_title_always_lands_on_the_matching_mode_for_every_seed() {
        // The real case that motivated this: a game whose own title and tagline are unambiguously
        // spooky must not have a one-in-twenty chance of coming out in a major key.
        for i in 0..20 {
            let title = format!("Spooky Kart {i}");
            let dark = ambient_spec_for(&title, "Eight haunted karts, one hollow to win.");
            assert!(dark.minor, "{title} reads as spooky and must be minor");
            let bright = ambient_spec_for(&title, "A sunny, cheerful festival in a bright garden.");
            assert!(!bright.minor, "{title} reads as cheerful and must be major");
        }
    }

    #[test]
    fn ambient_spec_for_leans_the_generated_mood_towards_dark_or_bright_words() {
        // Many different titles sharing one dark or bright tagline, so any single title's own random
        // jitter averages out and only the shared mood word's effect on the population remains.
        let dark_tagline = "A haunted, spooky night in a dark crypt.";
        let bright_tagline = "A sunny, cheerful festival in a bright garden.";
        let (mut dark_minor, mut bright_minor) = (0u32, 0u32);
        let (mut dark_root_sum, mut bright_root_sum) = (0u32, 0u32);
        const N: u32 = 40;
        for i in 0..N {
            let title = format!("Game {i}");
            let dark = ambient_spec_for(&title, dark_tagline);
            let bright = ambient_spec_for(&title, bright_tagline);
            dark_minor += u32::from(dark.minor);
            bright_minor += u32::from(bright.minor);
            dark_root_sum += u32::from(dark.root_midi);
            bright_root_sum += u32::from(bright.root_midi);
        }
        assert_eq!(
            dark_minor, N,
            "an unambiguously dark theme should always land on minor"
        );
        assert_eq!(
            bright_minor, 0,
            "an unambiguously bright theme should always land on major"
        );
        assert!(
            dark_root_sum < bright_root_sum,
            "dark-tagline games should average a lower register: {dark_root_sum} vs {bright_root_sum}"
        );
    }

    /// Lag in samples and value of the strongest circular autocorrelation peak of the low-end envelope
    /// within `lo..=hi` samples.
    fn envelope_period(v: &[f32], lo: usize, hi: usize) -> (usize, f32) {
        const STEP: usize = 32;
        let low = lowpass(v, 120., 0.707);
        let envelope: Vec<f32> = low
            .chunks(STEP)
            .map(|c| c.iter().map(|x| x.abs()).sum::<f32>() / c.len() as f32)
            .collect();
        let mean = envelope.iter().sum::<f32>() / envelope.len() as f32;
        let centred: Vec<f32> = envelope.iter().map(|x| x - mean).collect();
        let energy: f32 = centred.iter().map(|x| x * x).sum();
        let n = centred.len();
        let correlation = |lag: usize| {
            (0..n)
                .map(|i| centred[i] * centred[(i + lag) % n])
                .sum::<f32>()
                / energy
        };
        let mut best = (0, f32::MIN);
        for lag in lo / STEP..=hi / STEP {
            let value = correlation(lag);
            if value > best.1 {
                best = (lag, value);
            }
        }
        (best.0 * STEP, best.1)
    }

    #[test]
    fn the_kick_period_equals_the_beat() {
        for spec in [
            short_spec(),
            MusicSpec {
                bpm: 90.,
                bars: 2,
                seed: 7,
                ..MusicSpec::default()
            },
        ] {
            let stems = if spec == short_spec() {
                short_loop().clone()
            } else {
                music_loop(&spec)
            };
            let beat = f64::from(spec.beat_seconds()) * f64::from(RATE);
            let (lag, value) =
                envelope_period(&stems.base, (0.4 * beat) as usize, (1.6 * beat) as usize);
            assert!(
                (lag as f64 - beat).abs() < 0.01 * beat,
                "envelope repeats every {lag} samples, the beat is {beat}"
            );
            assert!(value > 0.3, "the beat is weak: autocorrelation {value}");
            // The kick sits on every beat: energy below 120 Hz right after each beat beats the middle of the gap.
            let b = beat as usize;
            let lows = lowpass(&stems.base, 120., 0.707);
            let bars = spec.bars.clamp(1, 64);
            let (mut on, mut off) = (0f32, 0f32);
            for k in 0..bars * 4 {
                on += rms(&lows[k * b + n_samples(0.005)..k * b + n_samples(0.05)]);
                off += rms(&lows[k * b + b / 2 - n_samples(0.02)..k * b + b / 2]);
            }
            assert!(
                on > off * 1.2,
                "kick energy on the beat {on} vs just before the off-beat {off}"
            );
        }
    }

    #[test]
    fn music_is_deterministic_seeded_and_follows_its_spec() {
        let base = music_loop(&tiny_spec());
        assert_eq!(base, music_loop(&tiny_spec()), "same spec, same audio");
        let seeded = music_loop(&MusicSpec {
            seed: 2,
            ..tiny_spec()
        });
        assert_ne!(base.base, seeded.base);
        assert_ne!(base.lead, seeded.lead);
        // Only the pitch class of the root matters (57 and 45 are both A); a different pitch class differs.
        assert_eq!(
            base,
            music_loop(&MusicSpec {
                root_midi: 45,
                ..tiny_spec()
            })
        );
        assert_ne!(
            base.melodic,
            music_loop(&MusicSpec {
                root_midi: 58,
                ..tiny_spec()
            })
            .melodic
        );
        assert_ne!(
            base.melodic,
            music_loop(&MusicSpec {
                minor: false,
                ..tiny_spec()
            })
            .melodic
        );
        // Tempo changes the length in proportion.
        let slow = MusicSpec {
            bpm: 62.,
            ..tiny_spec()
        };
        assert!(
            (slow.loop_samples() as f64 / tiny_spec().loop_samples() as f64 - 2.).abs() < 0.001
        );
    }

    #[test]
    fn the_music_plan_is_diatonic_and_the_bass_follows_the_roots() {
        for minor in [true, false] {
            for root in [0u8, 9, 57, 60, 127] {
                for seed in 0..12u64 {
                    let spec = MusicSpec {
                        minor,
                        root_midi: root,
                        seed,
                        bars: 16,
                        ..MusicSpec::default()
                    };
                    let p = plan(&spec, &mut Rng::new(seed));
                    let scale = scale_of(minor);
                    let tonic = i32::from(root % 12);
                    let in_scale = |n: i32| scale.contains(&(n - tonic).rem_euclid(12));
                    assert_eq!(p.chords.len(), 16);
                    for c in &p.chords {
                        assert!(
                            c.tones.iter().all(|t| in_scale(*t as i32)),
                            "chord {:?} is not diatonic",
                            c.tones
                        );
                        assert!(
                            c.tones.windows(2).all(|w| w[1] > w[0]),
                            "voicing {:?} must ascend",
                            c.tones
                        );
                        assert_eq!(
                            (c.bass as i32 - c.tones[0] as i32).rem_euclid(12),
                            0,
                            "the bass plays the chord root"
                        );
                        assert!(
                            (53. ..65.).contains(&c.tones[0]) && (28. ..40.).contains(&c.bass),
                            "registers {:?}",
                            (c.tones[0], c.bass)
                        );
                    }
                    assert!(!p.lead.is_empty());
                    for n in &p.lead {
                        assert!(
                            (72..=90).contains(&n.note) && in_scale(n.note),
                            "lead note {} is off the scale",
                            n.note
                        );
                        assert!(n.start >= 0. && n.start < 64. && n.len > 0.);
                        let chord = &p.chords[(n.start / 4.) as usize];
                        if n.start % 2. == 0. {
                            assert!(
                                chord
                                    .tones
                                    .iter()
                                    .any(|t| (n.note - *t as i32).rem_euclid(12) == 0),
                                "strong beat off the chord"
                            );
                        }
                    }
                    assert!(p.arp.iter().all(|i| *i < 4));
                }
            }
        }
        // The tonic chord of the minor mode is a minor seventh (A C E G), of the major mode a major seventh.
        let minor = plan(&MusicSpec::default(), &mut Rng::new(1));
        let tones = minor.chords[0].tones;
        assert_eq!(
            (tones[0], tones[1], tones[2], tones[3]),
            (57., 60., 64., 67.)
        );
        assert_eq!(minor.chords[0].bass, 33.);
        let major = plan(
            &MusicSpec {
                minor: false,
                ..MusicSpec::default()
            },
            &mut Rng::new(1),
        );
        assert!(
            major.chords.iter().any(|c| c.tones[1] - c.tones[0] == 4.),
            "major chords have a major third"
        );
    }

    #[test]
    fn the_pad_plays_the_chord_and_the_bass_plays_the_root() {
        let spec = short_spec();
        let stems = short_loop();
        let p = plan(&spec, &mut Rng::new(spec.seed));
        let bar = n_samples(4. * spec.beat_seconds());
        // Pad and arpeggio: the strongest pitch in the first bar belongs to one of the first chord's tones
        // (any octave: the arpeggio plays them an octave up).
        let strongest = dominant_hz(&stems.melodic[..bar], 150., 700.);
        let semitones = 69. + 12. * (strongest / 440.).log2();
        let off_chord = p.chords[0]
            .tones
            .iter()
            .map(|t| {
                let d = (semitones - t).rem_euclid(12.);
                d.min(12. - d)
            })
            .fold(f32::MAX, f32::min);
        assert!(
            off_chord < 0.5,
            "the strongest pad pitch {strongest} Hz is {off_chord} semitones from every chord tone"
        );
        // The bass voice sounds its root.
        for chord in &p.chords {
            assert_near(
                dominant_hz(&bass_voice(midi(chord.bass), 0.2), 30., 400.),
                midi(chord.bass),
                0.03,
                "bass voice",
            );
        }
    }

    #[test]
    fn rendering_everything_is_fast_enough() {
        let start = Instant::now();
        let mut samples = 0;
        for p in Preset::ALL {
            for v in 0..p.variants() {
                samples += std::hint::black_box(render(p, v, 9)).len();
            }
        }
        let presets = start.elapsed();
        let start = Instant::now();
        let music = std::hint::black_box(music_loop(&MusicSpec {
            bpm: 124.,
            bars: 16,
            ..MusicSpec::default()
        }));
        let loop_time = start.elapsed();
        assert_eq!(music.base.len(), 1_365_677);
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        println!(
            "synth timing [{profile}]: {} sounds ({:.1} s of audio) in {:.0} ms, 16-bar loop at 124 bpm in {:.0} ms",
            bank().len(),
            samples as f32 / SR,
            presets.as_secs_f64() * 1000.,
            loop_time.as_secs_f64() * 1000.
        );
        // The real limits apply to optimised builds; an unoptimised build only has to finish sanely.
        let (preset_limit, music_limit) = if cfg!(debug_assertions) {
            (30., 60.)
        } else {
            (2., 3.)
        };
        assert!(
            presets.as_secs_f64() < preset_limit,
            "all presets took {presets:?}"
        );
        assert!(
            loop_time.as_secs_f64() < music_limit,
            "the 16-bar loop took {loop_time:?}"
        );
    }
}
