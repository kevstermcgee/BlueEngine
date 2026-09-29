//! The lifecycle around a custom simulation's window: run flags in, whole ticks with exactly one input
//! each, quick save and load, the `--load` flag, and screenshot and frame-time evidence out.
//!
//! It composes the pieces this module already has ([`CapturePlan`], [`Timeline`], [`FixedStepper`],
//! [`InputAccumulator`], [`PerfReport`], [`SaveSlots`] and [`snapshot`]) so that a game's `main.rs` keeps
//! only the drawing and the mapping from devices and cues to its own held state. Every generated game
//! then behaves the same way under `--script`, `--capture`, `--load` and `--perf`. Graphics-free: the
//! caller writes the screenshot (`kit::capture::save_frame`) and reports it back through
//! [`Lifecycle::captured`].
//!
//! ```
//! use vesper3d::viewer::devkit::{Lifecycle, Snapshot, Simulation, StateHasher};
//! # #[derive(Default)] struct Sim { x: f32 }
//! # impl Simulation for Sim { type Input = f32; fn step(&mut self, i: &f32) { self.x += i; }
//! #   fn state_hash(&self) -> u64 { StateHasher::new().f32(self.x).finish() } }
//! # impl Snapshot for Sim { const KIND: &'static str = "doc"; type State = f32;
//! #   fn capture(&self) -> f32 { self.x }
//! #   fn restore(&mut self, s: f32) -> Result<(), String> { self.x = s; Ok(()) } }
//! const JUMP: u32 = 1;
//! let args: Vec<String> = ["game", "--script", "fwd:0-5,jump@2", "--seed", "3"].map(String::from).into();
//! let mut life = Lifecycle::<f32>::start(&args, &["fwd", "jump"]).unwrap();
//! let mut sim = Sim::default();
//! for _ in 0..6 {
//!     let dt = life.begin_frame(1. / 144.);            // unattended runs use one fixed tick per frame
//!     let script = life.script().unwrap();             // the cues of this frame
//!     life.feed(script.axis("fwd", "back"), if script.starts("jump") { JUMP } else { 0 }, [0.; 2]);
//!     for _ in 0..life.ticks(dt, 1., true) {
//!         let tick = life.take_tick();
//!         sim.step(&(tick.held + if tick.pressed(JUMP) { 10. } else { 0. }));
//!     }
//!     assert!(!life.end_frame(dt), "no capture plan, so the run never asks to exit");
//! }
//! assert_eq!((sim.x, life.frame()), (16., 6));
//! ```
use super::{
    clock::{FixedStepper, PerfReport, TICK},
    input::{Edges, InputAccumulator, Tick},
    playback::{flag_value, has_flag, parse_size, CapturePlan, Cue, Timeline},
    snapshot::{self, Snapshot},
};
use crate::viewer::savestate::{SaveHeader, SaveSlots, Source, QUICK_SLOT};
use std::{
    path::{Path, PathBuf},
    time::Instant,
};

/// The run flags every custom-simulation window understands (the starter's `main.rs` documents them).
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// `--seed N`; the clock when absent, so a plain run differs each time and a flagged one replays.
    pub seed: u64,
    /// `--script "fwd:0-200,jump@60"`: cues that drive the human input path.
    pub script: Option<Timeline>,
    /// `--capture DIR [--frames 30,90] [--exit-after N]`: screenshots, then exit.
    pub capture: Option<CapturePlan>,
    /// `--save-dir DIR`; absent means next to the executable.
    pub save_dir: Option<PathBuf>,
    /// `--load SLOT_OR_FILE`: resume before the first frame.
    pub load: Option<String>,
    /// `--perf`: print frame-time percentiles at exit.
    pub perf: bool,
    /// `--mute`.
    pub mute: bool,
    /// `--novsync`.
    pub novsync: bool,
    /// `--size WxH`, clamped to 320x240 .. 7680x4320.
    pub size: Option<(u32, u32)>,
}

impl Options {
    /// Parse the flags. `vocabulary` names the cues a `--script` may use. Errors are one sentence each.
    pub fn parse(args: &[String], vocabulary: &[&str]) -> Result<Self, String> {
        let capture = CapturePlan::from_args(args).map_err(|e| e.to_string())?;
        let script = match flag_value(args, "--script") {
            Some(text) => Some(Timeline::parse(text, vocabulary)?),
            None if has_flag(args, "--script") => {
                return Err("--script requires a cue script".into())
            }
            None => None,
        };
        let seed = match flag_value(args, "--seed") {
            Some(text) => text
                .parse()
                .map_err(|_| format!("--seed expects a whole number, got '{text}'"))?,
            None => std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(1, |d| d.as_nanos() as u64),
        };
        let size = match flag_value(args, "--size") {
            Some(text) => Some(
                parse_size(text)
                    .map(|(w, h)| (w.clamp(320, 7680), h.clamp(240, 4320)))
                    .ok_or_else(|| format!("--size expects WxH, got '{text}'"))?,
            ),
            None => None,
        };
        Ok(Self {
            seed,
            script,
            capture,
            save_dir: flag_value(args, "--save-dir").map(PathBuf::from),
            load: flag_value(args, "--load").map(str::to_owned),
            perf: has_flag(args, "--perf"),
            mute: has_flag(args, "--mute"),
            novsync: has_flag(args, "--novsync"),
            size,
        })
    }
    /// True for captures and scripted runs: fixed-step and silent, whatever the display does.
    pub fn unattended(&self) -> bool {
        self.capture.is_some() || self.script.is_some()
    }
    /// True when no sound should play (`--mute`, or an unattended run).
    pub fn silent(&self) -> bool {
        self.mute || self.unattended()
    }
}

/// The outcome of a quick save or load, ready for a banner.
#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    /// Short upper-case title: `GAME SAVED`, `LOAD FAILED`.
    pub title: &'static str,
    /// One line of detail: the save's label, or the error.
    pub detail: String,
    /// Linear RGB for the banner.
    pub color: [f32; 3],
    /// True when the save was written or the load applied.
    pub ok: bool,
}

impl Notice {
    /// A save the game declined (a finished run, a cutscene), with the reason.
    pub fn refused(why: impl Into<String>) -> Self {
        Self {
            title: "NOT SAVED",
            detail: why.into(),
            color: [1., 0.6, 0.3],
            ok: false,
        }
    }
}

/// The cues of a `--script` on one frame, so a game maps cue names to its held state in a few lines.
#[derive(Clone, Copy)]
pub struct ScriptFrame<'a> {
    script: &'a Timeline,
    frame: u32,
}

impl ScriptFrame<'_> {
    /// True while a cue named `name` is active (a held key).
    pub fn held(&self, name: &str) -> bool {
        self.script.active(self.frame).any(|c| c.name == name)
    }
    /// `1`, `-1` or `0`: a movement axis from two held cues (`axis("fwd", "back")`).
    pub fn axis(&self, positive: &str, negative: &str) -> f32 {
        f32::from(self.held(positive)) - f32::from(self.held(negative))
    }
    /// True on the first frame of a cue named `name` (a press edge).
    pub fn starts(&self, name: &str) -> bool {
        self.script.starting(self.frame).any(|c| c.name == name)
    }
    /// The sum of the `index`-th value of every active cue named `name` (`look:0.01/-0.02@0-60`).
    pub fn value(&self, name: &str, index: usize) -> f32 {
        self.script
            .active(self.frame)
            .filter(|c| c.name == name)
            .map(|c| c.value(index))
            .sum()
    }
    /// Every cue active on this frame.
    pub fn cues(&self) -> impl Iterator<Item = &Cue> {
        self.script.active(self.frame)
    }
}

/// See the module documentation. `H` is the game's own `Copy` held state (movement axes, trigger down).
pub struct Lifecycle<H: Copy + Default> {
    /// The parsed run flags.
    pub options: Options,
    /// Where F5 saves and F9 loads (`--save-dir`, or beside the executable).
    pub slots: SaveSlots,
    acc: InputAccumulator<H>,
    stepper: FixedStepper,
    perf: PerfReport,
    frame: u32,
    time: f32,
    began: Option<Instant>,
}

impl<H: Copy + Default> Lifecycle<H> {
    /// Parse the flags (`vocabulary` names the game's cues; `save` and `load` are always understood) and
    /// create the capture directory. An `Err` is a sentence for stderr; see [`Self::start_or_exit`].
    pub fn start(args: &[String], vocabulary: &[&str]) -> Result<Self, String> {
        let mut names: Vec<&str> = vocabulary.to_vec();
        names.extend(["save", "load"]);
        let options = Options::parse(args, &names)?;
        if let Some(plan) = &options.capture {
            plan.create_dir().map_err(|e| e.to_string())?;
        }
        let slots = options
            .save_dir
            .as_deref()
            .map_or_else(SaveSlots::beside_exe, SaveSlots::new);
        Ok(Self {
            options,
            slots,
            acc: InputAccumulator::new(),
            stepper: FixedStepper::new(),
            perf: PerfReport::new(30),
            frame: 0,
            time: 0.,
            began: None,
        })
    }
    /// [`Self::start`], printing a bad flag to stderr and exiting with status 2 (the convention every
    /// starter follows, so a caller that cannot watch the window still gets a plain reason).
    pub fn start_or_exit(args: &[String], vocabulary: &[&str]) -> Self {
        Self::start(args, vocabulary).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(2)
        })
    }
    /// Resume from `--load SLOT_OR_FILE` when it was given (`Ok(None)` when it was not). The error names
    /// the target; the simulation is untouched on failure.
    pub fn load_flag<S: Snapshot>(&self, sim: &mut S) -> Result<Option<SaveHeader>, String> {
        let Some(target) = &self.options.load else {
            return Ok(None);
        };
        snapshot::load_target(sim, &self.slots, target)
            .map(|(header, _)| Some(header))
            .map_err(|e| format!("--load {target}: {e}"))
    }
    /// [`Self::load_flag`], printing what was loaded, or the error followed by exit status 2.
    pub fn load_flag_or_exit<S: Snapshot>(&self, sim: &mut S) {
        match self.load_flag(sim) {
            Ok(Some(header)) => println!("loaded: {}", header.label),
            Ok(None) => {}
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(2)
            }
        }
    }
    /// The run's seed (`--seed`, or the clock).
    pub fn seed(&self) -> u64 {
        self.options.seed
    }
    /// A seed for a fresh run started from this frame (R to retry): different from the last one,
    /// reproducible under `--seed`.
    pub fn restart_seed(&self) -> u64 {
        self.options.seed.wrapping_add(u64::from(self.frame))
    }
    /// Frames begun so far (the number of the frame in progress, from 0).
    pub fn frame(&self) -> u32 {
        self.frame
    }
    /// Seconds of frame time so far, for animation.
    pub fn time(&self) -> f32 {
        self.time
    }
    /// Start a frame and return its length: `measured` (the wall-clock interval, `ClientInput::frame_seconds`)
    /// for a person, one fixed tick for an unattended run, so captures and scripts are reproducible.
    pub fn begin_frame(&mut self, measured: f32) -> f32 {
        self.began = Some(Instant::now());
        let dt = if self.options.unattended() || !measured.is_finite() || measured < 0. {
            TICK
        } else {
            measured
        };
        self.time += dt;
        dt
    }
    /// This frame's cues when the run is scripted; a game maps them to its held state instead of
    /// reading devices.
    pub fn script(&self) -> Option<ScriptFrame<'_>> {
        self.options.script.as_ref().map(|script| ScriptFrame {
            script,
            frame: self.frame,
        })
    }
    /// One frame of device state (see [`InputAccumulator::feed`]).
    pub fn feed(&mut self, held: H, edges: Edges, look: [f32; 2]) {
        self.acc.feed(held, edges, look);
    }
    /// Press edges from outside a frame (a UI button).
    pub fn press(&mut self, edges: Edges) {
        self.acc.press(edges);
    }
    /// Look motion no tick has taken yet, for a camera that turns immediately.
    pub fn pending_look(&self) -> [f32; 2] {
        self.acc.pending_look()
    }
    /// Whether this frame asks for a quick save or load: the `save@N` and `load@N` cues when scripted,
    /// otherwise the keys the caller read (F5 and F9 in the starter).
    pub fn save_load_requested(&self, save_key: bool, load_key: bool) -> (bool, bool) {
        match self.script() {
            Some(s) => (s.starts("save"), s.starts("load")),
            None => (save_key, load_key),
        }
    }
    /// F5: write the simulation to the quick slot with `label` as its menu text.
    pub fn quick_save<S: Snapshot>(&self, sim: &S, label: &str) -> Notice {
        match snapshot::save_to_slot(sim, &self.slots, QUICK_SLOT, label) {
            Ok(()) => Notice {
                title: "GAME SAVED",
                detail: "F9 loads it".into(),
                color: [0.4, 1., 0.6],
                ok: true,
            },
            Err(e) => Notice {
                title: "SAVE FAILED",
                detail: e.to_string(),
                color: [1., 0.4, 0.4],
                ok: false,
            },
        }
    }
    /// F9: resume the quick slot. A damaged save falls back to the previous good one and the notice says
    /// so. On success the saved moment replaces everything in flight here (pending input, leftover frame
    /// time); presentation state (particles, shake) is the caller's to reset.
    pub fn quick_load<S: Snapshot>(&mut self, sim: &mut S) -> Notice {
        let notice = match snapshot::load_from_slot(sim, &self.slots, QUICK_SLOT) {
            Ok((header, Source::Primary)) => Notice {
                title: "GAME LOADED",
                detail: header.label,
                color: [0.4, 0.9, 1.],
                ok: true,
            },
            Ok((header, Source::Backup(_))) => Notice {
                title: "LOADED THE PREVIOUS SAVE",
                detail: header.label,
                color: [1., 0.8, 0.4],
                ok: true,
            },
            Err(e) => Notice {
                title: "LOAD FAILED",
                detail: e.to_string(),
                color: [1., 0.4, 0.4],
                ok: false,
            },
        };
        if notice.ok {
            self.reset_input();
        }
        notice
    }
    /// Whole fixed ticks to run this frame (`dt` scaled by the game's time scale, hit-stop and slow
    /// motion). A paused game runs none and drops the input gathered while paused.
    pub fn ticks(&mut self, dt: f32, time_scale: f32, playing: bool) -> u32 {
        if !playing {
            self.acc.clear();
            return 0;
        }
        self.stepper.advance(dt * time_scale)
    }
    /// The next tick's input: held state, edges delivered once, look delivered once.
    pub fn take_tick(&mut self) -> Tick<H> {
        self.acc.take_tick()
    }
    /// Fraction of a tick left over, for interpolating presentation between the last two states.
    pub fn alpha(&self) -> f32 {
        self.stepper.alpha()
    }
    /// Forget pending input and leftover frame time (a new round began, a save was loaded).
    pub fn reset_input(&mut self) {
        self.acc.clear();
        self.stepper.reset();
    }
    /// The file this frame's screenshot goes to, when the capture plan wants one.
    pub fn capture_path(&self) -> Option<PathBuf> {
        self.options
            .capture
            .as_ref()
            .filter(|plan| plan.wants(self.frame))
            .map(|plan| plan.path_for(self.frame))
    }
    /// Report a screenshot the caller wrote: one JSON line per capture (`tools/contact_sheet.py` reads
    /// it), or the failure on stderr.
    pub fn captured(&self, path: &Path, result: Result<(u32, u32), String>) {
        match result {
            Ok((w, h)) => println!(
                "{{\"frame\":{},\"width\":{w},\"height\":{h},\"path\":{:?}}}",
                self.frame,
                path.display().to_string()
            ),
            Err(e) => eprintln!("capture failed: {e}"),
        }
    }
    /// End the frame: record frame-time evidence when `--perf` was given and count the frame. Returns
    /// true when the run should exit because the capture plan is finished.
    pub fn end_frame(&mut self, dt: f32) -> bool {
        if self.options.perf {
            self.perf.frame(dt);
            if let Some(began) = self.began {
                self.perf.work(began.elapsed().as_secs_f32());
            }
        }
        self.frame += 1;
        self.options
            .capture
            .as_ref()
            .is_some_and(|plan| plan.finished(self.frame))
    }
    /// The `--perf` summary to print at exit, when it was asked for.
    pub fn report(&self) -> Option<String> {
        self.options.perf.then(|| self.perf.text(25.))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewer::devkit::{Simulation, StateHasher};

    #[derive(Default)]
    struct Counter {
        total: f32,
    }
    impl Simulation for Counter {
        type Input = f32;
        fn step(&mut self, add: &f32) {
            self.total += add;
        }
        fn state_hash(&self) -> u64 {
            StateHasher::new().f32(self.total).finish()
        }
    }
    impl Snapshot for Counter {
        const KIND: &'static str = "counter";
        type State = f32;
        fn capture(&self) -> f32 {
            self.total
        }
        fn restore(&mut self, s: f32) -> Result<(), String> {
            self.total = s;
            Ok(())
        }
    }

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(String::from).collect()
    }
    struct Dir(PathBuf);
    impl Dir {
        fn new(name: &str) -> Self {
            Self(std::env::temp_dir().join(format!(
                "devkit-lifecycle-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )))
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn flags_parse_with_a_named_reason_for_each_mistake() {
        let o = Options::parse(
            &args("game --seed 7 --size 4000x100 --mute --perf --novsync"),
            &[],
        )
        .unwrap();
        assert_eq!((o.seed, o.size), (7, Some((4000, 240))));
        assert!(o.mute && o.perf && o.novsync && !o.unattended() && o.silent());
        assert!(o.script.is_none() && o.capture.is_none() && o.load.is_none());
        let o = Options::parse(
            &args("game --script fwd:0-3 --load quick --save-dir saves"),
            &["fwd"],
        )
        .unwrap();
        assert!(o.unattended() && o.silent() && o.script.is_some());
        assert_eq!(
            (o.load.as_deref(), o.save_dir.as_deref()),
            (Some("quick"), Some(Path::new("saves")))
        );
        assert!(
            Options::parse(&args("game"), &[]).unwrap().seed > 1,
            "the clock seeds a plain run"
        );
        for (bad, names, mentions) in [
            ("game --seed x", &[][..], "--seed"),
            ("game --size big", &[], "--size"),
            ("game --script", &[], "--script"),
            ("game --script dance:0-5", &["fwd"], "dance"),
            ("game --capture", &[], "--capture"),
        ] {
            let e = Options::parse(&args(bad), names).unwrap_err();
            assert!(e.contains(mentions), "{bad}: {e}");
        }
    }

    #[test]
    fn a_scripted_run_maps_cues_and_asks_for_saves_and_loads_from_the_script() {
        let a =
            args("game --script fwd:0-4,left:2,look:0.5/-0.25@1-2,look:0.5@2,jump@3,save@1,load@4");
        let mut life = Lifecycle::<f32>::start(&a, &["fwd", "left", "look", "jump"]).unwrap();
        let mut seen = Vec::new();
        for _ in 0..6 {
            let dt = life.begin_frame(0.5);
            assert_eq!(
                dt, TICK,
                "an unattended frame is one tick whatever the display took"
            );
            let s = life.script().unwrap();
            seen.push((
                s.axis("fwd", "back"),
                s.held("left"),
                s.starts("jump"),
                s.value("look", 0),
                s.value("look", 1),
                life.save_load_requested(true, true),
                s.cues().count(),
            ));
            let forward = s.axis("fwd", "back");
            life.feed(forward, 0, [0.; 2]);
            assert_eq!(life.ticks(dt, 1., true), 1);
            assert_eq!(life.take_tick().held, forward);
            assert!(!life.end_frame(dt));
        }
        assert_eq!(seen[0], (1., false, false, 0., 0., (false, false), 1));
        assert_eq!(seen[1], (1., false, false, 0.5, -0.25, (true, false), 3));
        assert_eq!(seen[2], (1., true, false, 1., -0.25, (false, false), 4));
        assert_eq!(seen[3], (1., false, true, 0., 0., (false, false), 2));
        assert_eq!(seen[4], (1., false, false, 0., 0., (false, true), 2));
        assert_eq!(seen[5], (0., false, false, 0., 0., (false, false), 0));
        assert!((life.time() - 6. * TICK).abs() < 1e-6);
        let plain = Lifecycle::<f32>::start(&args("game"), &[]).unwrap();
        assert!(plain.script().is_none());
        assert_eq!(
            plain.save_load_requested(true, false),
            (true, false),
            "keys decide without a script"
        );
    }

    #[test]
    fn a_frame_for_a_person_uses_the_measured_length_and_pausing_drops_input() {
        let mut life = Lifecycle::<f32>::start(&args("game --seed 5"), &[]).unwrap();
        assert_eq!(life.begin_frame(0.05), 0.05);
        assert_eq!(
            life.begin_frame(f32::NAN),
            TICK,
            "a broken clock is one tick"
        );
        life.feed(1., 1, [0.1, 0.]);
        assert_eq!(life.pending_look(), [0.1, 0.]);
        assert_eq!(life.ticks(0.05, 1., false), 0, "paused: no ticks");
        assert_eq!(
            life.take_tick(),
            Tick::default(),
            "and the input gathered meanwhile is gone"
        );
        life.feed(1., 1, [0.; 2]);
        assert_eq!(life.ticks(TICK * 2.5, 1., true), 2);
        assert!((life.alpha() - 0.5).abs() < 1e-3);
        assert!(life.take_tick().pressed(1));
        assert!(!life.take_tick().pressed(1));
        assert_eq!(
            life.ticks(TICK, 0., true),
            0,
            "a hit-stop scales time to zero"
        );
        assert_eq!(
            (life.seed(), life.restart_seed()),
            (5, 5),
            "no frame has ended yet"
        );
        assert!(!life.end_frame(TICK));
        assert_eq!((life.frame(), life.restart_seed()), (1, 6));
        life.reset_input();
        assert_eq!(life.alpha(), 0.);
    }

    #[test]
    fn quick_save_and_load_go_through_the_slots_and_reset_what_is_in_flight() {
        let dir = Dir::new("slots");
        let a = args(&format!("game --save-dir {}", dir.0.display()));
        let mut life = Lifecycle::<f32>::start(&a, &[]).unwrap();
        let mut sim = Counter { total: 3. };
        let notice = life.quick_load(&mut sim);
        assert_eq!((notice.title, notice.ok), ("LOAD FAILED", false));
        assert_eq!(sim.total, 3., "a failed load changes nothing");
        let notice = life.quick_save(&sim, "three");
        assert_eq!((notice.title, notice.ok), ("GAME SAVED", true));
        let info =
            crate::viewer::savestate::describe(&life.slots.path(QUICK_SLOT).unwrap()).unwrap();
        assert_eq!(
            (info["summary"]["policy"].as_str(), info["label"].as_str()),
            (Some("exact"), Some("three")),
            "save-info shows the policy the game declared: {info}"
        );
        sim.total = 9.;
        life.feed(1., 1, [0.2, 0.]);
        life.ticks(TICK * 1.5, 1., true);
        let notice = life.quick_load(&mut sim);
        assert_eq!(
            (notice.title, notice.detail.as_str(), notice.ok),
            ("GAME LOADED", "three", true)
        );
        assert_eq!(sim.total, 3.);
        assert_eq!(life.take_tick(), Tick::default(), "pending input is gone");
        assert_eq!(life.alpha(), 0., "and so is leftover time");
        let refused = Notice::refused("the run is over");
        assert_eq!((refused.title, refused.ok), ("NOT SAVED", false));
        // --load resumes before the first frame, and names its target when it cannot.
        let mut fresh = Counter::default();
        let a = args(&format!("game --save-dir {} --load quick", dir.0.display()));
        let life = Lifecycle::<f32>::start(&a, &[]).unwrap();
        assert_eq!(life.load_flag(&mut fresh).unwrap().unwrap().label, "three");
        assert_eq!(fresh.total, 3.);
        let a = args(&format!(
            "game --save-dir {} --load nothing",
            dir.0.display()
        ));
        let life = Lifecycle::<f32>::start(&a, &[]).unwrap();
        let e = life.load_flag(&mut fresh).unwrap_err();
        assert!(e.starts_with("--load nothing:"), "{e}");
        assert!(Lifecycle::<f32>::start(&args("game"), &[])
            .unwrap()
            .load_flag(&mut fresh)
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_capture_plan_names_the_frames_and_ends_the_run_and_perf_reports_only_when_asked() {
        let dir = Dir::new("capture");
        let shots = dir.0.join("shots");
        let a = args(&format!(
            "game --capture {} --frames 1,2 --perf",
            shots.display()
        ));
        let mut life = Lifecycle::<f32>::start(&a, &[]).unwrap();
        assert!(shots.is_dir(), "the capture directory is created at start");
        assert!(
            Lifecycle::<f32>::start(&a, &[]).is_err(),
            "and never reused"
        );
        let mut exits = Vec::new();
        for frame in 0..3u32 {
            let dt = life.begin_frame(TICK);
            let path = life.capture_path();
            assert_eq!(path.is_some(), frame >= 1, "frame {frame}");
            if let Some(path) = &path {
                assert_eq!(
                    path.file_name().unwrap().to_str().unwrap(),
                    format!("shot_{frame:05}.png")
                );
                life.captured(path, Ok((320, 240)));
                life.captured(path, Err("no window".into()));
            }
            exits.push(life.end_frame(dt));
        }
        assert_eq!(
            exits,
            [false, false, true],
            "one frame after the last capture"
        );
        assert_eq!(
            life.report().as_deref(),
            Some("no frames recorded"),
            "--perf reports, but three frames are all warm-up"
        );
        let mut timed = Lifecycle::<f32>::start(&args("game --perf"), &[]).unwrap();
        let mut quiet = Lifecycle::<f32>::start(&args("game"), &[]).unwrap();
        for _ in 0..40 {
            let dt = timed.begin_frame(TICK);
            assert!(!timed.end_frame(dt));
            let dt = quiet.begin_frame(TICK);
            assert!(!quiet.end_frame(dt));
        }
        assert!(timed.report().unwrap().starts_with("frames 10"));
        assert_eq!(quiet.report(), None);
    }
}
