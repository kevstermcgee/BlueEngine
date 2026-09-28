//! Frame timing for real-time loops: one wall clock, a fixed-step accumulator and a perf report.
//!
//! macroquad's `get_frame_time()` is stamped at the *end* of its draw, after the GL flush. A one-frame
//! stall inside that flush therefore reads as ~33 ms and then ~1 ms although frames were presented
//! evenly (measured on Windows/OpenGL with vsync: 0.5-0.8% of frames differ from the wall clock by
//! more than 5 ms). Fed to a fixed-step accumulator that is a doubled step followed by a repeated
//! frame; fed to stick look it is a camera jerk. [`FrameClock`] measures the interval between frame
//! *starts* with [`std::time::Instant`] instead, and [`FixedStepper`] turns those intervals into whole
//! simulation ticks.
//!
//! Everything here is graphics-free and deterministic apart from the wall clock itself, which tests
//! replace with [`FrameClock::tick_after`].
use std::time::{Duration, Instant};

/// Fixed simulation rate shared by the engine (60 Hz).
pub const TICK_RATE: f32 = 60.;
/// Seconds per fixed tick.
pub const TICK: f32 = 1. / TICK_RATE;

/// Longest interval one frame may report, in seconds. A stall (window drag, debugger, long load) must
/// not be simulated as one huge step.
pub const MAX_FRAME: f32 = 0.1;

/// Wall-clock frame intervals. Call [`FrameClock::tick`] exactly once at the top of every rendered
/// frame. The shared `ClientInput` (feature `client`) does it inside `begin_frame`, so a game that owns
/// one reads `ClientInput::frame_seconds()` instead of keeping a second clock.
#[derive(Clone, Debug)]
pub struct FrameClock {
    last: Option<Instant>,
    dt: f32,
    frames: u64,
    hitches: u64,
    average: f32,
}

impl Default for FrameClock {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameClock {
    /// A clock whose first frame reports one fixed tick.
    pub fn new() -> Self {
        Self {
            last: None,
            dt: TICK,
            frames: 0,
            hitches: 0,
            average: TICK,
        }
    }

    /// Start a frame now and return its length in seconds (see [`FrameClock::tick_after`]).
    pub fn tick(&mut self) -> f32 {
        self.tick_at(Instant::now())
    }

    /// Start a frame at `now`. The first call reports [`TICK`]; later calls report the interval since
    /// the previous call, clamped to `0..=`[`MAX_FRAME`]. A frame counts as a *hitch* when it is more
    /// than twice as long as the running average of recent frames and longer than 25 ms, so a game can
    /// show how often it stalled without a separate profiler.
    pub fn tick_at(&mut self, now: Instant) -> f32 {
        let raw = self.last.map_or(TICK, |previous| {
            now.saturating_duration_since(previous).as_secs_f32()
        });
        self.last = Some(now);
        self.record(raw)
    }

    /// Advance a frame by an explicit duration: deterministic, for tests and scripted runs.
    pub fn tick_after(&mut self, elapsed: Duration) -> f32 {
        self.record(elapsed.as_secs_f32())
    }

    fn record(&mut self, raw: f32) -> f32 {
        let dt = if raw.is_finite() {
            raw.clamp(0., MAX_FRAME)
        } else {
            TICK
        };
        if self.frames > 0 && dt > 0.025 && dt > self.average * 2. {
            self.hitches += 1;
        }
        self.average = if self.frames == 0 {
            dt.max(1e-4)
        } else {
            self.average * 0.95 + dt * 0.05
        };
        self.dt = dt;
        self.frames += 1;
        dt
    }

    /// Length of the current frame in seconds (what the last `tick` returned).
    pub fn dt(&self) -> f32 {
        self.dt
    }
    /// Frames started so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }
    /// Frames that were unusually long (see [`FrameClock::tick_at`]).
    pub fn hitches(&self) -> u64 {
        self.hitches
    }
    /// Recent average frame length in seconds.
    pub fn average(&self) -> f32 {
        self.average
    }
}

/// Turns variable frame lengths into whole fixed ticks, with a bounded catch-up.
///
/// ```
/// use vesper3d::viewer::devkit::{FixedStepper, TICK};
/// let mut stepper = FixedStepper::new();
/// assert_eq!(stepper.advance(TICK * 2.5), 2); // two whole ticks, half a tick carried over
/// assert!((stepper.alpha() - 0.5).abs() < 1e-3); // render half-way between the last two states
/// ```
#[derive(Clone, Debug)]
pub struct FixedStepper {
    remainder: f64,
    max_steps: u32,
}

impl Default for FixedStepper {
    fn default() -> Self {
        Self::new()
    }
}

impl FixedStepper {
    /// Catch up by at most eight ticks per frame (the shared session's cap).
    pub fn new() -> Self {
        Self::with_max_steps(8)
    }
    /// Catch up by at most `max_steps` (at least one) ticks per frame; anything beyond is dropped so a
    /// stall never makes the simulation race to catch up.
    pub fn with_max_steps(max_steps: u32) -> Self {
        Self {
            remainder: 0.,
            max_steps: max_steps.max(1),
        }
    }
    /// Add `seconds` and return how many fixed ticks to run now. Non-finite or negative input adds nothing.
    pub fn advance(&mut self, seconds: f32) -> u32 {
        if seconds.is_finite() && seconds > 0. {
            self.remainder += f64::from(seconds);
        }
        let step = f64::from(TICK);
        let mut steps = 0;
        while self.remainder + 1e-9 >= step && steps < self.max_steps {
            self.remainder -= step;
            steps += 1;
        }
        if steps == self.max_steps {
            // Dropped time: keep only the sub-tick remainder.
            self.remainder = self.remainder.rem_euclid(step);
        }
        steps
    }
    /// Fraction of a tick left over, `0..1`, for interpolating presentation between the last two states.
    pub fn alpha(&self) -> f32 {
        ((self.remainder / f64::from(TICK)) as f32).clamp(0., 1.)
    }
    /// Forget accumulated time (a menu opened, a level loaded).
    pub fn reset(&mut self) {
        self.remainder = 0.;
    }
}

/// Collects frame-time samples and summarises them (percentiles, hitches). Feed it the clock's
/// frame length and, if you want to tell a slow frame from a slow present, the CPU time your own
/// frame work took.
#[derive(Clone, Debug, Default)]
pub struct PerfReport {
    frames: Vec<f32>,
    work: Vec<f32>,
    skipped: usize,
}

/// Summary statistics in milliseconds.
#[derive(Clone, Debug, PartialEq)]
pub struct PerfSummary {
    pub frames: usize,
    pub average_ms: f32,
    pub p50_ms: f32,
    pub p95_ms: f32,
    pub p99_ms: f32,
    pub max_ms: f32,
    /// Frames longer than the threshold given to [`PerfReport::summary`], with their frame numbers
    /// (at most the first six).
    pub slow: usize,
    pub slow_frames: Vec<usize>,
    /// Median and worst of the game's own per-frame work, when recorded.
    pub work_p50_ms: Option<f32>,
    pub work_max_ms: Option<f32>,
}

impl PerfReport {
    /// Ignore the first `warm_up` frames (shader compilation, first-use initialisation).
    pub fn new(warm_up: usize) -> Self {
        Self {
            skipped: warm_up,
            ..Self::default()
        }
    }
    /// Record one frame's length in seconds (after warm-up).
    pub fn frame(&mut self, seconds: f32) {
        if self.skipped > 0 {
            self.skipped -= 1;
        } else if seconds.is_finite() {
            self.frames.push(seconds);
        }
    }
    /// Record the CPU seconds the game itself spent inside a frame.
    pub fn work(&mut self, seconds: f32) {
        if self.skipped == 0 && seconds.is_finite() {
            self.work.push(seconds);
        }
    }
    /// Samples recorded so far.
    pub fn len(&self) -> usize {
        self.frames.len()
    }
    /// True when no frame has been recorded yet.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
    /// Percentiles and slow-frame list; `None` before any frame. `slow_ms` is the slow-frame threshold.
    pub fn summary(&self, slow_ms: f32) -> Option<PerfSummary> {
        if self.frames.is_empty() {
            return None;
        }
        let sorted = |samples: &[f32]| {
            let mut v: Vec<f32> = samples.iter().map(|s| s * 1000.).collect();
            v.sort_by(|a, b| a.total_cmp(b));
            v
        };
        let at = |v: &[f32], q: f32| v[((v.len() - 1) as f32 * q) as usize];
        let v = sorted(&self.frames);
        let slow_frames: Vec<usize> = self
            .frames
            .iter()
            .enumerate()
            .filter(|(_, s)| **s * 1000. > slow_ms)
            .map(|(i, _)| i)
            .collect();
        let work = sorted(&self.work);
        Some(PerfSummary {
            frames: v.len(),
            average_ms: v.iter().sum::<f32>() / v.len() as f32,
            p50_ms: at(&v, 0.5),
            p95_ms: at(&v, 0.95),
            p99_ms: at(&v, 0.99),
            max_ms: v[v.len() - 1],
            slow: slow_frames.len(),
            slow_frames: slow_frames.into_iter().take(6).collect(),
            work_p50_ms: (!work.is_empty()).then(|| at(&work, 0.5)),
            work_max_ms: work.last().copied(),
        })
    }
    /// One-paragraph text for a `--perf` flag.
    pub fn text(&self, slow_ms: f32) -> String {
        let Some(s) = self.summary(slow_ms) else {
            return "no frames recorded".into();
        };
        let mut out = format!(
            "frames {}  avg {:.2} ms ({:.0} fps)  p50 {:.2}  p95 {:.2}  p99 {:.2}  max {:.2}  slow(>{slow_ms}ms) {} at {:?}",
            s.frames,
            s.average_ms,
            1000. / s.average_ms.max(1e-3),
            s.p50_ms,
            s.p95_ms,
            s.p99_ms,
            s.max_ms,
            s.slow,
            s.slow_frames
        );
        if let (Some(p50), Some(max)) = (s.work_p50_ms, s.work_max_ms) {
            out += &format!("\n  own work per frame: p50 {p50:.2} ms  max {max:.2} ms");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_frame_is_one_tick_and_long_stalls_are_clamped() {
        let mut clock = FrameClock::new();
        assert_eq!(
            clock.tick_after(Duration::from_secs(5)),
            MAX_FRAME,
            "a stall is never simulated in full"
        );
        let mut clock = FrameClock::new();
        let start = Instant::now();
        assert_eq!(
            clock.tick_at(start),
            TICK,
            "first frame has no interval yet"
        );
        let dt = clock.tick_at(start + Duration::from_millis(16));
        assert!((dt - 0.016).abs() < 1e-4);
        assert_eq!(clock.dt(), dt);
        assert_eq!(clock.frames(), 2);
    }

    #[test]
    fn a_clock_that_goes_backwards_or_reports_nonsense_stays_sane() {
        let mut clock = FrameClock::new();
        let start = Instant::now();
        clock.tick_at(start + Duration::from_millis(50));
        assert_eq!(
            clock.tick_at(start),
            0.,
            "backwards time is a zero-length frame, never a panic"
        );
        clock.record(f32::NAN);
        assert_eq!(clock.dt(), TICK);
    }

    #[test]
    fn hitches_are_counted_against_the_recent_average() {
        let mut clock = FrameClock::new();
        for _ in 0..30 {
            clock.tick_after(Duration::from_millis(16));
        }
        assert_eq!(clock.hitches(), 0);
        clock.tick_after(Duration::from_millis(60));
        assert_eq!(clock.hitches(), 1);
        assert!(clock.average() > 0.015 && clock.average() < 0.03);
    }

    #[test]
    fn stepper_yields_whole_ticks_and_carries_the_remainder() {
        let mut s = FixedStepper::new();
        assert_eq!(s.advance(TICK * 0.4), 0);
        assert_eq!(s.advance(TICK * 0.4), 0);
        assert_eq!(
            s.advance(TICK * 0.4),
            1,
            "three 0.4-tick frames add up to one tick"
        );
        assert!((s.alpha() - 0.2).abs() < 0.01);
        assert_eq!(s.advance(f32::NAN), 0);
        assert_eq!(s.advance(-1.), 0);
    }

    #[test]
    fn stepper_total_matches_the_wall_time_at_any_display_rate() {
        for hz in [30., 60., 75., 144., 240.] {
            let mut s = FixedStepper::new();
            let ticks: u32 = (0..hz as u32 * 10).map(|_| s.advance(1. / hz)).sum();
            assert!(
                (ticks as i32 - 600).abs() <= 1,
                "{hz} Hz gave {ticks} ticks in 10 s"
            );
        }
    }

    #[test]
    fn stepper_drops_time_beyond_the_catch_up_cap() {
        let mut s = FixedStepper::with_max_steps(5);
        assert_eq!(s.advance(10.), 5);
        assert!(s.alpha() < 1.);
        assert_eq!(
            s.advance(0.),
            0,
            "the dropped backlog is gone, not queued for later"
        );
        assert_eq!(
            FixedStepper::with_max_steps(0).advance(1.),
            1,
            "at least one step is always allowed"
        );
    }

    #[test]
    fn perf_report_summarises_and_skips_warm_up() {
        let mut p = PerfReport::new(2);
        assert!(p.summary(25.).is_none());
        assert_eq!(p.text(25.), "no frames recorded");
        p.frame(0.5);
        p.frame(0.5);
        for i in 0..100 {
            p.frame(if i == 40 { 0.05 } else { 0.0166 });
            p.work(0.004);
        }
        let s = p.summary(25.).unwrap();
        assert_eq!(s.frames, 100);
        assert_eq!((s.slow, s.slow_frames.clone()), (1, vec![40]));
        assert!((s.p50_ms - 16.6).abs() < 0.2 && (s.max_ms - 50.).abs() < 0.2);
        assert_eq!(s.work_p50_ms.map(|w| (w * 10.).round()), Some(40.));
        assert!(p.text(25.).contains("slow(>25ms) 1"));
        assert_eq!(p.len(), 100);
        assert!(!p.is_empty());
    }
}
