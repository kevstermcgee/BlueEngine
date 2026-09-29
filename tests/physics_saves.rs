//! Saves of a game simulation built on the engine's rigid-body world: the contract a physics game can meet
//! (docs/SAVE_STATE.md), proved with the devkit helpers on twenty kicked props, and the exact contract it
//! cannot, shown rather than assumed.
use vesper3d::{
    prelude::*,
    viewer::{
        devkit::{snapshot, SavePolicy, Simulation, Snapshot},
        savestate::world::WorldState,
    },
};

const PROPS: usize = 20;

/// A custom simulation that embeds `HeadlessWorld` as its physics authority (docs/CUSTOM_CLIENT.md): twenty
/// catalog props on a green, kicked at 4 to 6 m/s.
struct Kicked {
    world: HeadlessWorld,
}

impl Kicked {
    fn new(kind: &str, spacing: f32, lean: bool) -> Self {
        let mut course = SceneBuilder::new("Kicked")
            .without_spawn()
            .structural_box("green", V(0., -0.1, 0.), V(14., 0.1, 14.), V::ONE)
            .structural_box("wall", V(0., 0.5, -8.), V(14., 0.5, 0.1), V::ONE);
        for i in 0..PROPS {
            let x = (i as f32 - (PROPS - 1) as f32 / 2.) * spacing;
            course = course.prop(format!("p-{i}"), kind, V(x, 0., 4.));
        }
        let mut world = course.world().unwrap();
        for i in 0..PROPS {
            let id = format!("p-{i}");
            let mass = world.prop_mass(&id).unwrap();
            let sideways = if lean { 0.4 * (i % 5) as f32 - 0.8 } else { 0. };
            let speed = 4.2 + 0.5 * (i % 4) as f32;
            assert!(world.impulse(&id, V(sideways, 0., -speed) * mass));
        }
        Self { world }
    }
    /// Apples half a metre apart, leaning into each other: they roll, collide and bounce off the wall.
    fn rolling_apples() -> Self {
        Self::new("apple", 0.5, true)
    }
    /// Cereal boxes in lanes 1.2 m apart: they slide, tumble and come to rest without touching each other.
    fn sliding_boxes() -> Self {
        Self::new("cereal", 1.2, false)
    }
    fn moving(&self) -> usize {
        (0..PROPS)
            .filter(|i| self.world.is_prop_sleeping(&format!("p-{i}")) == Some(false))
            .count()
    }
}

impl Simulation for Kicked {
    type Input = ();
    fn step(&mut self, _: &()) {
        self.world.step();
    }
    fn state_hash(&self) -> u64 {
        self.world.checksum()
    }
    fn hash_parts(&self) -> Vec<(&'static str, u64)> {
        vec![("tick", self.world.tick), ("world", self.world.checksum())]
    }
}

impl Snapshot for Kicked {
    const KIND: &'static str = "kicked";
    /// The policy a game with rigid bodies declares: a load is a pure function of the file, a resumed run
    /// a fair continuation. `assert_resumes_as_promised` then demands exactly that.
    const POLICY: SavePolicy = SavePolicy::PhysicsContinuation;
    type State = WorldState;
    fn capture(&self) -> WorldState {
        self.world.save_state().expect("a finite world")
    }
    fn restore(&mut self, state: WorldState) -> std::result::Result<(), String> {
        self.world.restore_state(&state).map_err(|e| e.to_string())
    }
    fn save_tick(&self) -> u64 {
        self.world.tick
    }
}

/// How far the farthest prop is from where the other run has it, in metres.
fn farthest(a: &Kicked, b: &Kicked) -> f32 {
    (0..PROPS)
        .map(|i| {
            let id = format!("p-{i}");
            (a.world.prop_position(&id).unwrap() - b.world.prop_position(&id).unwrap()).length()
        })
        .fold(0., f32::max)
}

/// Three seconds, saved every second (splits at ticks 0, 60, 120 and 180). Debug-profile physics is slow,
/// so the scenario is kept short.
const TICKS: usize = 180;
const EVERY: usize = 60;
/// The farthest any kicked box ends up from where the uninterrupted run has it, over every split, was
/// 0.99 m on the machine this was measured on: a box that tips over in one run and slides on in the other
/// is a metre apart within a second, and boxes kicked at 4 to 6 m/s do tip. The bound leaves headroom for
/// other platforms. A kicked catalog *apple* is worse: its hull rolls like a die, so a resume that lands a
/// different facet sends it metres elsewhere within three seconds (measured 5 to 6 m, in lanes or
/// colliding). Drift is small only while bodies slide or rest; the pure-function proof above is the
/// promise a physics game makes, and a bound is the game's to choose from its own scene.
const DRIFT_BOUND: f32 = 2.;

#[test]
fn twenty_rolling_colliding_apples_load_as_a_pure_function_of_the_file() {
    let mut sim = Kicked::rolling_apples();
    for _ in 0..TICKS {
        sim.step(&());
    }
    let moving = sim.moving();
    assert!(
        moving >= 10,
        "only {moving} apples still move at the end: the scene is too tame"
    );
    // The declared policy picks the proof: for a physics continuation this is
    // `assert_loads_replay_identically`.
    snapshot::assert_resumes_as_promised(Kicked::rolling_apples, &[(); TICKS], EVERY);
}

#[test]
fn a_resumed_run_of_sliding_boxes_stays_within_a_measured_distance_and_never_resumes_exactly() {
    let mut sim = Kicked::sliding_boxes();
    for _ in 0..EVERY {
        sim.step(&());
    }
    assert_eq!(
        sim.moving(),
        PROPS,
        "every box is still sliding at the first mid-run split"
    );
    let worst = snapshot::assert_resumes_within(
        Kicked::sliding_boxes,
        &[(); TICKS],
        EVERY,
        DRIFT_BOUND,
        farthest,
    );
    println!("worst drift of a resumed run from the uninterrupted one: {worst:.4} m");
    assert!(
        worst > 0.,
        "a sliding box resumed bit-exactly; if rapier's caches are now saved, tighten docs/SAVE_STATE.md"
    );
    let message = *std::panic::catch_unwind(|| {
        snapshot::assert_resumes_exactly(Kicked::sliding_boxes, &[(); TICKS], EVERY);
    })
    .expect_err("the exact contract is out of reach while bodies touch")
    .downcast::<String>()
    .unwrap();
    assert!(message.contains("diverged at tick"), "{message}");
}

/// The kicked props plus a score that `capture` forgets: the kind of bug a physics game makes while
/// everything physical restores fine.
struct Scored {
    props: Kicked,
    score: u32,
}
impl Simulation for Scored {
    type Input = ();
    fn step(&mut self, _: &()) {
        self.props.step(&());
        self.score += 1;
    }
    fn state_hash(&self) -> u64 {
        self.props.state_hash() ^ u64::from(self.score).rotate_left(17)
    }
    fn hash_parts(&self) -> Vec<(&'static str, u64)> {
        vec![
            ("world", self.props.world.checksum()),
            ("score", u64::from(self.score)),
        ]
    }
}
impl Snapshot for Scored {
    const KIND: &'static str = "scored";
    type State = WorldState;
    fn capture(&self) -> WorldState {
        self.props.capture()
    }
    fn restore(&mut self, state: WorldState) -> std::result::Result<(), String> {
        self.props.restore(state)
    }
}

#[test]
fn a_physics_save_that_does_not_restore_names_the_forgotten_part_not_the_world() {
    let mut sim = Scored {
        props: Kicked::sliding_boxes(),
        score: 0,
    };
    for _ in 0..EVERY {
        sim.step(&());
    }
    let bytes = snapshot::save(&sim, "sixty").unwrap();
    let mut fresh = Scored {
        props: Kicked::sliding_boxes(),
        score: 0,
    };
    let error = snapshot::restore(&mut fresh, &bytes)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("part 'score' differs") && !error.contains("'world'"),
        "{error}"
    );
    assert_eq!(
        fresh.props.world.tick, 0,
        "a refused load put the world back"
    );
    // The props alone (nothing forgotten) load and hash as saved.
    let mut sim = Kicked::sliding_boxes();
    for _ in 0..EVERY {
        sim.step(&());
    }
    let bytes = snapshot::save(&sim, "sixty").unwrap();
    let mut fresh = Kicked::sliding_boxes();
    snapshot::restore(&mut fresh, &bytes).unwrap();
    assert_eq!(
        (fresh.world.tick, fresh.state_hash()),
        (EVERY as u64, sim.state_hash())
    );
}
