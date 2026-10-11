//! Tap decoded samples unchanged. Analysis and history stay bounded for long playlists.
use rodio::{Decoder, Source};
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use std::{
    collections::VecDeque,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
const N: usize = 2048;
const HOP: usize = 512;
#[derive(Clone, Copy, Debug)]
pub struct Features {
    pub bands: [f32; 32],
    pub wave: [f32; 64],
    pub energy: f32,
    pub bass: f32,
    pub mid: f32,
    pub high: f32,
    pub beat: f32,
}
impl Default for Features {
    fn default() -> Self {
        Self {
            bands: [0.; 32],
            wave: [0.; 64],
            energy: 0.,
            bass: 0.,
            mid: 0.,
            high: 0.,
            beat: 0.,
        }
    }
}
pub type Timeline = Arc<Mutex<VecDeque<(f64, Features)>>>;
pub fn timeline() -> Timeline {
    Arc::new(Mutex::new(VecDeque::with_capacity(256)))
}
pub fn at_time(timeline: &Timeline, seconds: f64) -> Features {
    timeline
        .try_lock()
        .ok()
        .and_then(|frames| {
            frames
                .iter()
                .rev()
                .find(|(t, _)| *t <= seconds + 0.025)
                .map(|(_, f)| *f)
        })
        .unwrap_or_default()
}
pub struct Analyzer {
    rate: u32,
    ring: [f32; N],
    offset: usize,
    count: usize,
    fft: Arc<dyn Fft<f32>>,
    buffer: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    previous: [f32; 32],
    flux_mean: f32,
}
impl Analyzer {
    pub fn new(rate: u32) -> Self {
        let fft = FftPlanner::new().plan_fft_forward(N);
        let scratch = vec![Complex::default(); fft.get_inplace_scratch_len()];
        Self {
            rate,
            ring: [0.; N],
            offset: 0,
            count: 0,
            fft,
            buffer: vec![Complex::default(); N],
            scratch,
            previous: [0.; 32],
            flux_mean: 0.,
        }
    }
    pub fn push(&mut self, sample: f32) -> Option<Features> {
        self.ring[self.offset] = if sample.is_finite() { sample } else { 0. };
        self.offset = (self.offset + 1) % N;
        self.count += 1;
        if self.count < N || !self.count.is_multiple_of(HOP) {
            return None;
        }
        let mut square = 0.;
        for i in 0..N {
            let sample = self.ring[(self.offset + i) % N];
            square += sample * sample;
            self.buffer[i] = Complex::new(
                sample * (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (N - 1) as f32).cos()),
                0.,
            );
        }
        self.fft
            .process_with_scratch(&mut self.buffer, &mut self.scratch);
        let mut f = Features {
            energy: (square / N as f32).sqrt().min(1.),
            ..Default::default()
        };
        for b in 0..32 {
            let lo = 30. * (16000_f32 / 30.).powf(b as f32 / 32.);
            let hi = 30. * (16000_f32 / 30.).powf((b + 1) as f32 / 32.);
            let start = ((lo * N as f32 / self.rate as f32) as usize).clamp(1, N / 2 - 1);
            let end = ((hi * N as f32 / self.rate as f32).ceil() as usize).clamp(start + 1, N / 2);
            let power = self.buffer[start..end]
                .iter()
                .map(|v| v.norm_sqr())
                .sum::<f32>();
            let amplitude = (power.sqrt() * 4. / N as f32).min(1.);
            f.bands[b] = (amplitude * 7.).sqrt().min(1.);
        }
        let flux = f
            .bands
            .iter()
            .zip(self.previous)
            .map(|(a, b)| (a - b).max(0.))
            .sum::<f32>()
            / 32.;
        self.flux_mean = self.flux_mean * 0.97 + flux * 0.03;
        f.beat = ((flux - self.flux_mean * 1.35) * 16.).clamp(0., 1.);
        self.previous = f.bands;
        f.bass = f.bands[..10].iter().copied().fold(0., f32::max);
        f.mid = f.bands[10..23].iter().copied().fold(0., f32::max);
        f.high = f.bands[23..].iter().copied().fold(0., f32::max);
        for i in 0..64 {
            f.wave[i] = self.ring[(self.offset + i * N / 64) % N].clamp(-1., 1.);
        }
        Some(f)
    }
}
pub struct Tap<S: Source> {
    inner: S,
    analyzer: Analyzer,
    timeline: Timeline,
    channels: u16,
    channel: u16,
    dominant: f32,
    frames: u64,
}
impl<S: Source> Tap<S> {
    pub fn new(inner: S, timeline: Timeline) -> Self {
        let channels = inner.channels();
        let rate = inner.sample_rate();
        Self {
            inner,
            analyzer: Analyzer::new(rate),
            timeline,
            channels,
            channel: 0,
            dominant: 0.,
            frames: 0,
        }
    }
}
impl<S: Source> Iterator for Tap<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next()?;
        // Preserve anti-phase stereo instead of cancelling it in a mono average.
        if sample.abs() > self.dominant.abs() {
            self.dominant = sample;
        }
        self.channel += 1;
        if self.channel == self.channels {
            self.channel = 0;
            self.frames += 1;
            if let Some(f) = self.analyzer.push(self.dominant) {
                if let Ok(mut frames) = self.timeline.try_lock() {
                    if frames.len() == 256 {
                        frames.pop_front();
                    }
                    frames.push_back((self.frames as f64 / self.sample_rate() as f64, f));
                }
            }
            self.dominant = 0.;
        }
        Some(sample)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}
impl<S: Source> Source for Tap<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> u16 {
        self.inner.channels()
    }
    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}
pub struct Opened {
    pub source: Tap<Decoder<std::io::BufReader<File>>>,
    pub timeline: Timeline,
    pub fingerprint: u64,
}
pub fn open(path: &Path) -> Result<Opened, String> {
    let mut file = File::open(path).map_err(|e| format!("Cannot open song: {e}"))?;
    let mut bytes = [0_u8; 65536];
    let count = file.read(&mut bytes).map_err(|e| e.to_string())?;
    let mut hash = 0xcbf29ce484222325_u64;
    for b in &bytes[..count] {
        hash = (hash ^ u64::from(*b)).wrapping_mul(0x100000001b3);
    }
    hash ^= file.metadata().map_err(|e| e.to_string())?.len();
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let source =
        Decoder::try_from(file).map_err(|e| format!("Unsupported or damaged audio: {e}"))?;
    if source.channels() == 0 || source.sample_rate() == 0 {
        return Err("Audio has no playable samples".into());
    }
    let timeline = timeline();
    Ok(Opened {
        source: Tap::new(source, timeline.clone()),
        timeline,
        fingerprint: hash,
    })
}
#[derive(Default)]
pub struct Playlist {
    pub files: Vec<PathBuf>,
    pub current: usize,
}
impl Playlist {
    pub fn import(&mut self, paths: impl IntoIterator<Item = PathBuf>) -> usize {
        let before = self.files.len();
        for path in paths {
            if self.files.len() < 4096 && path.is_file() {
                self.files.push(path);
            }
        }
        self.files.len() - before
    }
    pub fn advance(&mut self, direction: i32) {
        if !self.files.is_empty() {
            self.current = (self.current as i64 + direction as i64)
                .rem_euclid(self.files.len() as i64) as usize;
        }
    }
    pub fn title(&self) -> String {
        self.files
            .get(self.current)
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;
    #[test]
    fn real_spectrum_finds_a_tone_and_silence_stays_silent() {
        let mut analyzer = Analyzer::new(48000);
        let mut f = Features::default();
        for n in 0..8192 {
            if let Some(v) =
                analyzer.push((std::f32::consts::TAU * 440. * n as f32 / 48000.).sin() * 0.3)
            {
                f = v;
            }
        }
        let peak = f
            .bands
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;
        assert!((12..16).contains(&peak), "440Hz peak band {peak}");
        assert!(f.energy > 0.2);
        for _ in 0..8192 {
            if let Some(v) = analyzer.push(0.) {
                f = v;
            }
        }
        assert_eq!(f.energy, 0.);
        assert!(f.bands.iter().all(|v| *v == 0.));
    }
    #[test]
    fn tap_preserves_pcm_and_antiphase_energy_with_bounded_history() {
        let pcm: Vec<_> = (0..160000)
            .flat_map(|n| {
                let a = (std::f32::consts::TAU * 90. * n as f32 / 48000.).sin() * 0.2;
                [a, -a]
            })
            .collect();
        let timeline = timeline();
        let tap = Tap::new(SamplesBuffer::new(2, 48000, pcm.clone()), timeline.clone());
        assert_eq!(tap.collect::<Vec<_>>(), pcm);
        let frames = timeline.lock().unwrap();
        assert_eq!(frames.len(), 256);
        assert!(frames.back().unwrap().1.bass > 0.1);
    }
    #[test]
    fn import_supports_single_and_multiple_paths_and_wraps() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/check-tone.wav");
        let mut p = Playlist::default();
        assert_eq!(p.import([fixture.clone()]), 1);
        assert_eq!(p.import([fixture.clone(), fixture]), 2);
        assert_eq!(p.files.len(), 3);
        p.advance(-1);
        assert_eq!(p.current, 2);
        p.advance(1);
        assert_eq!(p.current, 0);
        let opened = open(&p.files[0]).unwrap();
        assert!(opened.source.total_duration().unwrap().as_secs_f32() > 0.);
    }
}
