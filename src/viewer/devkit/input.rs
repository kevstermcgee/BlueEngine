//! Device frames in, exactly one input per fixed tick out.
//!
//! A display faster than 60 Hz renders frames that run no simulation tick; a slower one runs several
//! ticks per frame. Games that read the keyboard once per frame and hand the result straight to the
//! simulation lose fast clicks (a press and release between two ticks) or fire a jump twice. The
//! accumulator applies three rules, which are the rules the shared session applies to its own input:
//!
//! * **held state** (movement axes, "trigger down") is replaced every frame and persists for every
//!   tick that frame runs;
//! * **press edges** are delivered on exactly one tick and stay pending across frames that run no
//!   tick, so a fast click is never lost and a slow frame never repeats one;
//! * **look deltas** accumulate and are delivered once (the camera applies them immediately from
//!   [`InputAccumulator::pending_look`], so aiming never waits for a tick).
//!
//! The held part is the game's own `Copy` struct, so nothing here knows about genres or buttons.

/// A set of press edges as a bit mask; define one `const` per action (`const JUMP: Edges = 1 << 0;`).
pub type Edges = u32;

/// What one fixed tick receives.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tick<H> {
    /// Held state of the most recent frame.
    pub held: H,
    /// Press edges for this tick only.
    pub edges: Edges,
    /// Look deltas gathered since the previous tick (radians, `[yaw, pitch]`).
    pub look: [f32; 2],
}

impl<H> Tick<H> {
    /// True when every bit of `edge` was pressed since the previous tick.
    pub fn pressed(&self, edge: Edges) -> bool {
        self.edges & edge == edge
    }
}

/// Collects one frame of device state at a time; see the module documentation.
///
/// ```
/// use vesper3d::viewer::devkit::InputAccumulator;
/// const JUMP: u32 = 1;
/// let mut acc = InputAccumulator::<f32>::new(); // held state: forward axis
/// acc.feed(1.0, JUMP, [0.1, 0.0]);
/// let first = acc.take_tick();
/// let second = acc.take_tick(); // a slow frame runs two ticks
/// assert!(first.pressed(JUMP) && !second.pressed(JUMP), "an edge reaches exactly one tick");
/// assert_eq!((first.held, second.held), (1.0, 1.0), "held state persists");
/// assert_eq!((first.look[0], second.look[0]), (0.1, 0.0), "look is delivered once");
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InputAccumulator<H: Copy + Default> {
    held: H,
    edges: Edges,
    look: [f32; 2],
}

impl<H: Copy + Default> InputAccumulator<H> {
    /// An empty accumulator (nothing held, nothing pending).
    pub fn new() -> Self {
        Self::default()
    }
    /// Record one rendered frame: `held` replaces the previous frame's, `edges` and `look` accumulate
    /// until a tick takes them. Non-finite look deltas are ignored.
    pub fn feed(&mut self, held: H, edges: Edges, look: [f32; 2]) {
        self.held = held;
        self.edges |= edges;
        for (total, delta) in self.look.iter_mut().zip(look) {
            if delta.is_finite() {
                *total += delta;
            }
        }
    }
    /// Add press edges without a full frame (an event callback, a UI button).
    pub fn press(&mut self, edges: Edges) {
        self.edges |= edges;
    }
    /// Look motion no tick has taken yet, for a camera that turns immediately.
    pub fn pending_look(&self) -> [f32; 2] {
        self.look
    }
    /// The input for the next tick; consumes the edges and the look, keeps the held state.
    pub fn take_tick(&mut self) -> Tick<H> {
        let tick = Tick {
            held: self.held,
            edges: self.edges,
            look: self.look,
        };
        self.edges = 0;
        self.look = [0.; 2];
        tick
    }
    /// Forget everything (a menu opened, focus was lost, a new round began).
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// A finite value clamped to `-limit..=limit`; anything else (NaN, infinity from a broken device)
/// becomes zero. Apply it to every axis that comes from hardware before the simulation sees it.
pub fn clean_axis(value: f32, limit: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-limit, limit)
    } else {
        0.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JUMP: Edges = 1;
    const FIRE: Edges = 2;

    #[test]
    fn an_edge_reaches_exactly_one_tick_even_when_a_frame_runs_several() {
        let mut a = InputAccumulator::<f32>::new();
        a.feed(1., JUMP | FIRE, [0.1, -0.1]);
        let ticks: Vec<_> = (0..3).map(|_| a.take_tick()).collect();
        assert!(ticks[0].pressed(JUMP) && ticks[0].pressed(FIRE));
        assert!(ticks[1..].iter().all(|t| t.edges == 0));
        assert!(
            ticks.iter().all(|t| t.held == 1.),
            "held movement persists across ticks"
        );
        assert_eq!(ticks[0].look, [0.1, -0.1]);
        assert_eq!(ticks[1].look, [0., 0.]);
    }

    #[test]
    fn edges_and_look_wait_through_frames_that_run_no_tick() {
        // A 240 Hz display: several frames pass before the next 60 Hz tick.
        let mut a = InputAccumulator::<f32>::new();
        for _ in 0..3 {
            a.feed(0., 0, [0.02, 0.]);
        }
        a.feed(0., JUMP, [0., 0.]);
        assert!(
            (a.pending_look()[0] - 0.06).abs() < 1e-6,
            "look accumulates for an immediate camera"
        );
        let t = a.take_tick();
        assert!(t.pressed(JUMP), "the click was not lost");
        assert!((t.look[0] - 0.06).abs() < 1e-6);
        let next = a.take_tick();
        assert!(
            !next.pressed(JUMP) && next.look == [0., 0.],
            "consumed exactly once"
        );
    }

    #[test]
    fn releasing_a_key_stops_movement_on_the_next_frame() {
        let mut a = InputAccumulator::<f32>::new();
        a.feed(1., 0, [0.; 2]);
        assert_eq!(a.take_tick().held, 1.);
        a.feed(0., 0, [0.; 2]);
        assert_eq!(a.take_tick().held, 0.);
    }

    #[test]
    fn clearing_drops_pending_input_and_bad_look_is_ignored() {
        let mut a = InputAccumulator::<u8>::new();
        a.feed(7, FIRE, [f32::NAN, f32::INFINITY]);
        assert_eq!(a.pending_look(), [0., 0.]);
        a.press(JUMP);
        a.clear();
        assert_eq!(a.take_tick(), Tick::default());
    }

    #[test]
    fn hardware_axes_are_cleaned() {
        assert_eq!(clean_axis(f32::NAN, 1.), 0.);
        assert_eq!(clean_axis(5., 1.), 1.);
        assert_eq!(clean_axis(-5., 1.), -1.);
        assert_eq!(clean_axis(0.25, 1.), 0.25);
    }
}
