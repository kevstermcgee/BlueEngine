//! Driving a loop without a human: recorded inputs, a tiny cue script and a screenshot schedule.
//!
//! An AI agent cannot play the game it builds, so it verifies by running fixed-step scripts and looking
//! at a few captured frames. The stock runner has this for `GameInput` (`--playback INPUTS.json`,
//! `--capture NEW_DIR`); these pieces give a game with its own input type the same flags and file
//! formats without re-implementing them.
use crate::Result;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

/// Largest playback file accepted, in bytes.
const MAX_PLAYBACK_BYTES: usize = 32_000_000;

/// Recorded per-tick inputs: a JSON array with one element per 60 Hz tick (fields you omit take
/// their `Default`, if the input type says `#[serde(default)]`).
///
/// ```
/// use vesper3d::viewer::devkit::Playback;
/// let mut play = Playback::<f32>::from_json(b"[1.0, 0.5]").unwrap();
/// assert_eq!(play.next_input(), 1.0);
/// assert_eq!(play.next_input(), 0.5);
/// assert!(play.done());
/// assert_eq!(play.next_input(), 0.0, "past the end is the default input");
/// ```
#[derive(Clone, Debug)]
pub struct Playback<T> {
    frames: Vec<T>,
    cursor: usize,
}

impl<T: DeserializeOwned + Default + Clone> Playback<T> {
    /// Wrap already-built inputs.
    pub fn from_frames(frames: Vec<T>) -> Self {
        Self { frames, cursor: 0 }
    }
    /// Parse a JSON array of inputs.
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_PLAYBACK_BYTES {
            return Err(format!("Playback file exceeds {MAX_PLAYBACK_BYTES} bytes").into());
        }
        Ok(Self::from_frames(serde_json::from_slice(bytes)?))
    }
    /// Read and parse a JSON playback file.
    pub fn load(path: &Path) -> Result<Self> {
        Self::from_json(&std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?)
    }
    /// Number of recorded ticks.
    pub fn len(&self) -> usize {
        self.frames.len()
    }
    /// True for an empty recording.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
    /// The input recorded for `tick`, or the default past the end.
    pub fn get(&self, tick: usize) -> T {
        self.frames.get(tick).cloned().unwrap_or_default()
    }
    /// The next tick's input (default past the end); advances the cursor.
    pub fn next_input(&mut self) -> T {
        let input = self.get(self.cursor);
        self.cursor = (self.cursor + 1).min(self.frames.len().max(self.cursor));
        input
    }
    /// Ticks handed out so far.
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    /// True once every recorded tick has been handed out.
    pub fn done(&self) -> bool {
        self.cursor >= self.frames.len()
    }
}

/// One entry of a [`Timeline`]: something that is active over a range of frames.
#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    /// The game's own vocabulary: `fwd`, `jump`, `look`, `fire`...
    pub name: String,
    /// Numbers given after the name (`look:0.01/-0.02@0-60` has two).
    pub values: Vec<f32>,
    /// First and last frame (inclusive) the cue is active. An edge such as `jump@30` has `from == to`.
    pub from: u32,
    pub to: u32,
}

impl Cue {
    /// True while the cue is active (held keys, per-frame look).
    pub fn active(&self, frame: u32) -> bool {
        (self.from..=self.to).contains(&frame)
    }
    /// True on the cue's first frame (press edges).
    pub fn starts(&self, frame: u32) -> bool {
        self.from == frame
    }
    /// The `index`-th value, or 0.
    pub fn value(&self, index: usize) -> f32 {
        self.values.get(index).copied().unwrap_or(0.)
    }
}

/// A comma-separated cue script for driving the *human* input path from a command line:
/// `--script "fwd:0-120,look:0.01/-0.02@0-60,jump@30"`.
///
/// * `name:FROM-TO` or `name:FRAME`: held over those frames;
/// * `name@FRAME`: an event on one frame;
/// * `name:VALUE[/VALUE...]@FROM-TO` (or `@FRAME`): values applied over those frames.
///
/// The game maps cue names to its own device state; the engine only parses and schedules.
///
/// ```
/// use vesper3d::viewer::devkit::Timeline;
/// let script = Timeline::parse("fwd:0-10, jump@3, look:0.02/-0.01@2-4", &["fwd", "jump", "look"]).unwrap();
/// assert!(script.active(3).any(|c| c.name == "fwd"));
/// assert!(script.starting(3).any(|c| c.name == "jump"));
/// assert_eq!(script.active(4).find(|c| c.name == "look").unwrap().value(1), -0.01);
/// assert!(Timeline::parse("dance:0-5", &["fwd"]).is_err());
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Timeline {
    cues: Vec<Cue>,
}

fn frame_range(text: &str) -> Option<(u32, u32)> {
    match text.split_once('-') {
        Some((a, b)) => {
            let (a, b): (u32, u32) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
            (a <= b).then_some((a, b))
        }
        None => text.trim().parse().ok().map(|n| (n, n)),
    }
}

impl Timeline {
    /// Parse a script. `names` is the game's vocabulary: an unknown name is an error (an empty list
    /// accepts any name made of letters, digits, `_` and `-`).
    pub fn parse(text: &str, names: &[&str]) -> std::result::Result<Self, String> {
        let mut cues = Vec::new();
        for token in text.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            let bad = |why: &str| format!("cannot parse cue '{token}': {why}");
            let (head, range) = match token.split_once('@') {
                Some((head, range)) => (head, Some(range)),
                None => (token, None),
            };
            let (name, tail) = match head.split_once(':') {
                Some((name, tail)) => (name.trim(), Some(tail.trim())),
                None => (head.trim(), None),
            };
            let known = if names.is_empty() {
                !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            } else {
                names.contains(&name)
            };
            if !known {
                return Err(bad("unknown name"));
            }
            let (values, (from, to)) = match (tail, range) {
                (Some(range), None) => (
                    Vec::new(),
                    frame_range(range).ok_or_else(|| bad("expected FROM-TO frames"))?,
                ),
                (None, Some(range)) => (
                    Vec::new(),
                    frame_range(range).ok_or_else(|| bad("expected a frame or FROM-TO"))?,
                ),
                (Some(values), Some(range)) => {
                    let values = values
                        .split('/')
                        .map(|v| v.trim().parse::<f32>().ok().filter(|v| v.is_finite()))
                        .collect::<Option<Vec<f32>>>()
                        .ok_or_else(|| bad("values must be finite numbers separated by /"))?;
                    (
                        values,
                        frame_range(range).ok_or_else(|| bad("expected a frame or FROM-TO"))?,
                    )
                }
                (None, None) => {
                    return Err(bad(
                        "expected NAME:FROM-TO, NAME@FRAME or NAME:VALUE@FROM-TO",
                    ))
                }
            };
            cues.push(Cue {
                name: name.to_owned(),
                values,
                from,
                to,
            });
        }
        Ok(Self { cues })
    }
    /// Every cue, in script order.
    pub fn cues(&self) -> &[Cue] {
        &self.cues
    }
    /// Cues active on `frame`.
    pub fn active(&self, frame: u32) -> impl Iterator<Item = &Cue> {
        self.cues.iter().filter(move |c| c.active(frame))
    }
    /// Cues whose first frame is `frame`.
    pub fn starting(&self, frame: u32) -> impl Iterator<Item = &Cue> {
        self.cues.iter().filter(move |c| c.starts(frame))
    }
    /// The last frame any cue is active on (0 for an empty script).
    pub fn last_frame(&self) -> u32 {
        self.cues.iter().map(|c| c.to).max().unwrap_or(0)
    }
    /// True for an empty script.
    pub fn is_empty(&self) -> bool {
        self.cues.is_empty()
    }
}

/// Parse a frame list such as `30,60,100-102` into sorted unique frame numbers (at most 10 000).
pub fn parse_frame_list(text: &str) -> std::result::Result<Vec<u32>, String> {
    let mut frames = Vec::new();
    for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (from, to) =
            frame_range(part).ok_or_else(|| format!("cannot parse frame list entry '{part}'"))?;
        if frames.len() as u64 + u64::from(to - from) + 1 > 10_000 {
            return Err("frame list is longer than 10000 frames".into());
        }
        frames.extend(from..=to);
    }
    frames.sort_unstable();
    frames.dedup();
    Ok(frames)
}

/// The value after `flag` in a command line, if present and not itself a flag.
pub fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let at = args.iter().position(|a| a == flag)?;
    args.get(at + 1)
        .map(String::as_str)
        .filter(|v| !v.starts_with("--"))
}

/// True when the bare flag is present.
pub fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

/// Parse `1280x720` (also `1280X720`); both sides must be positive.
pub fn parse_size(text: &str) -> Option<(u32, u32)> {
    let (w, h) = text.split_once(['x', 'X'])?;
    let (w, h): (u32, u32) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

/// Which frames to save as screenshots, and when to stop: `--capture DIR --frames 30,120 --exit-after 130`.
///
/// Without `--frames` one screenshot is taken at frame 30; without `--exit-after` the run ends one
/// frame after the last screenshot, so `--capture DIR` alone produces evidence and exits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturePlan {
    /// New directory receiving `shot_00030.png`-style files.
    pub dir: PathBuf,
    /// Frame numbers to capture (sorted, unique).
    pub frames: Vec<u32>,
    /// Frame count after which the loop should exit.
    pub exit_after: u32,
}

impl CapturePlan {
    /// Read the capture flags; `Ok(None)` when `--capture` is absent.
    pub fn from_args(args: &[String]) -> Result<Option<Self>> {
        let Some(dir) = flag_value(args, "--capture") else {
            if has_flag(args, "--capture") {
                return Err("--capture requires a directory".into());
            }
            return Ok(None);
        };
        let frames = match flag_value(args, "--frames") {
            Some(list) => parse_frame_list(list)?,
            None => vec![30],
        };
        if frames.is_empty() {
            return Err("--frames needs at least one frame number".into());
        }
        let exit_after = match flag_value(args, "--exit-after") {
            Some(n) => n
                .parse()
                .map_err(|_| format!("--exit-after expects a frame count, got '{n}'"))?,
            None => frames[frames.len() - 1] + 1,
        };
        Ok(Some(Self {
            dir: PathBuf::from(dir),
            frames,
            exit_after,
        }))
    }
    /// Create the output directory. It must not exist yet: captures never overwrite earlier evidence.
    pub fn create_dir(&self) -> Result<()> {
        std::fs::create_dir_all(
            self.dir
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )?;
        std::fs::create_dir(&self.dir).map_err(|e| {
            format!(
                "cannot create capture directory {} (it must be new): {e}",
                self.dir.display()
            )
            .into()
        })
    }
    /// True when `frame` should be saved.
    pub fn wants(&self, frame: u32) -> bool {
        self.frames.binary_search(&frame).is_ok()
    }
    /// The file for `frame`.
    pub fn path_for(&self, frame: u32) -> PathBuf {
        self.dir.join(format!("shot_{frame:05}.png"))
    }
    /// True once the loop should exit (called with the number of frames run so far).
    pub fn finished(&self, frames_run: u32) -> bool {
        frames_run >= self.exit_after
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(String::from).collect()
    }

    #[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
    #[serde(default, deny_unknown_fields)]
    struct Pad {
        x: f32,
        fire: bool,
    }

    #[test]
    fn playback_reads_json_defaults_missing_fields_and_ends_cleanly() {
        let mut play = Playback::<Pad>::from_json(br#"[{"x":1.0},{"fire":true},{}]"#).unwrap();
        assert_eq!(play.len(), 3);
        assert_eq!(play.next_input(), Pad { x: 1., fire: false });
        assert_eq!(play.next_input(), Pad { x: 0., fire: true });
        assert!(!play.done());
        assert_eq!(play.next_input(), Pad::default());
        assert!(play.done());
        assert_eq!(play.next_input(), Pad::default());
        assert_eq!(play.cursor(), 3, "the cursor stops at the end");
        assert_eq!(play.get(1).fire, true);
        assert!(
            Playback::<Pad>::from_json(br#"[{"nope":1}]"#).is_err(),
            "unknown fields are rejected"
        );
        assert!(Playback::<Pad>::from_json(b"{").is_err());
        assert!(Playback::<Pad>::from_json(b"[]").unwrap().is_empty());
        assert!(Playback::<Pad>::load(Path::new("/definitely/not/here.json")).is_err());
    }

    #[test]
    fn timeline_maps_holds_events_and_values_to_the_right_frames() {
        let names = ["fwd", "left", "jump", "look"];
        let t = Timeline::parse("fwd:0-10, left:5, look:0.02/-0.01@2-4, jump@3", &names).unwrap();
        assert_eq!(t.cues().len(), 4);
        assert_eq!(
            t.active(0).map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["fwd"]
        );
        assert_eq!(
            t.active(5).map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["fwd", "left"]
        );
        let look = t.active(3).find(|c| c.name == "look").unwrap();
        assert_eq!(
            (look.value(0), look.value(1), look.value(2)),
            (0.02, -0.01, 0.)
        );
        assert!(
            t.starting(3).any(|c| c.name == "jump") && !t.starting(4).any(|c| c.name == "jump")
        );
        assert_eq!(t.last_frame(), 10);
        assert!(t.active(11).next().is_none());
    }

    #[test]
    fn timeline_rejects_nonsense_with_a_message() {
        for bad in [
            "dance:0-5",
            "fwd",
            "fwd:x-y",
            "fwd:9-2",
            "jump@abc",
            "look:0.1",
            "look:nan@1",
            "@3",
            "fwd:@2",
        ] {
            let e = Timeline::parse(bad, &["fwd", "jump", "look"]).unwrap_err();
            assert!(e.contains("cannot parse cue"), "{bad}: {e}");
        }
        assert!(Timeline::parse("", &[]).unwrap().is_empty());
        assert!(Timeline::parse(" , ", &[]).unwrap().is_empty());
        assert!(
            Timeline::parse("anything-goes:1-2", &[]).is_ok(),
            "an empty vocabulary accepts any identifier"
        );
        assert!(Timeline::parse("no spaces:1-2", &[]).is_err());
    }

    #[test]
    fn frame_lists_expand_ranges_sort_and_bound() {
        assert_eq!(
            parse_frame_list("30, 5,10-12,5").unwrap(),
            [5, 10, 11, 12, 30]
        );
        assert!(parse_frame_list("a").is_err());
        assert!(parse_frame_list("0-20000").is_err());
        assert!(parse_frame_list("").unwrap().is_empty());
    }

    #[test]
    fn command_line_helpers() {
        let a = args("game --seed 7 --mute --size 1280x720 --capture");
        assert_eq!(flag_value(&a, "--seed"), Some("7"));
        assert_eq!(
            flag_value(&a, "--mute"),
            None,
            "a flag followed by a flag has no value"
        );
        assert!(has_flag(&a, "--mute") && !has_flag(&a, "--perf"));
        assert_eq!(parse_size("1280x720"), Some((1280, 720)));
        assert_eq!(parse_size("640X480"), Some((640, 480)));
        for bad in ["0x5", "axb", "100", "100x", ""] {
            assert_eq!(parse_size(bad), None, "{bad}");
        }
    }

    #[test]
    fn capture_plan_defaults_and_paths() {
        assert_eq!(
            CapturePlan::from_args(&args("game --seed 1")).unwrap(),
            None
        );
        assert!(CapturePlan::from_args(&args("game --capture")).is_err());
        let plan = CapturePlan::from_args(&args("game --capture out"))
            .unwrap()
            .unwrap();
        assert_eq!((plan.frames.clone(), plan.exit_after), (vec![30], 31));
        let plan =
            CapturePlan::from_args(&args("game --capture out --frames 5,120 --exit-after 200"))
                .unwrap()
                .unwrap();
        assert!(plan.wants(120) && !plan.wants(6));
        assert!(!plan.finished(199) && plan.finished(200));
        assert_eq!(plan.path_for(5), Path::new("out").join("shot_00005.png"));
        assert!(CapturePlan::from_args(&args("game --capture out --frames x")).is_err());
        assert!(CapturePlan::from_args(&args("game --capture out --exit-after soon")).is_err());
    }

    #[test]
    fn capture_directory_must_be_new() {
        let base = std::env::temp_dir().join(format!("devkit-capture-{}", std::process::id()));
        let plan = CapturePlan {
            dir: base.join("shots"),
            frames: vec![1],
            exit_after: 2,
        };
        plan.create_dir().unwrap();
        assert!(plan.dir.is_dir());
        assert!(
            plan.create_dir().is_err(),
            "an existing capture directory is never reused"
        );
        std::fs::remove_dir_all(base).unwrap();
    }
}
