//! Game-local workaround for the shared client's missing native focus signal.
//! The shared client still owns timing, Intent accumulation, storage and drawing.
use afterpull::{focus_gate::FocusGate, Afterpull, State};
use vesper3d::two_d::{draw::*, *};
mod window_focus;

#[cfg(target_os = "linux")]
pub fn window_title() -> String {
    format!("{} [{}]", Afterpull::TITLE, std::process::id())
}

pub struct NativeGame {
    game: Afterpull,
    gate: FocusGate,
    window: window_focus::WindowFocus,
    replay: bool,
    initialized: bool,
}
impl NativeGame {
    fn observe_focus(&mut self) -> bool {
        if !self.replay {
            let sample = self.window.sample();
            // A press/release pair in this frame also needs to be discarded.
            let neutral = sample.neutral && macroquad::input::get_keys_pressed().is_empty();
            self.gate.observe(sample.focused, sample.lost, neutral);
            sample.space
        } else {
            false
        }
    }
}
impl GameLogic for NativeGame {
    const ID: &'static str = Afterpull::ID;
    const TITLE: &'static str = Afterpull::TITLE;
    const CONTROLS: &'static str = Afterpull::CONTROLS;
    const VERIFY_TICKS: u32 = Afterpull::VERIFY_TICKS;
    fn new(seed: u64) -> Self {
        Self {
            game: Afterpull::new(seed),
            gate: FocusGate::default(),
            window: window_focus::WindowFocus::new(),
            replay: std::env::args().any(|a| a == "--verify"),
            initialized: false,
        }
    }
    fn tick(&self) -> u32 {
        self.game.tick()
    }
    fn restart(&mut self) {
        self.observe_focus();
        if self.gate.active() {
            self.game.restart();
        }
    }
    fn pointer_target_only_on_press() -> bool {
        true
    }
    fn outcome(&self) -> &'static str {
        self.game.outcome()
    }
    fn verification_input(t: u32) -> Intent {
        Afterpull::verification_input(t)
    }
    fn probe_input() -> Intent {
        Afterpull::probe_input()
    }
    fn probe_success(&self) -> bool {
        self.game.probe_success()
    }
    fn cue_point(&self, cue: usize) -> Point {
        self.game.cue_point(cue)
    }
    fn take_cues(&mut self) -> Vec<usize> {
        let cues = self.game.take_cues();
        if self.gate.active() {
            cues
        } else {
            vec![]
        }
    }
}
impl Simulation for NativeGame {
    type Input = Intent;
    fn step(&mut self, input: &Intent) {
        self.gate.step(&mut self.game, input);
    }
    fn state_hash(&self) -> u64 {
        self.game.state_hash()
    }
}
impl Snapshot for NativeGame {
    const KIND: &'static str = <Afterpull as Snapshot>::KIND;
    const VERSION: u32 = <Afterpull as Snapshot>::VERSION;
    type State = State;
    fn capture(&self) -> State {
        self.game.capture()
    }
    fn restore(&mut self, state: State) -> Result<(), String> {
        if self.initialized {
            self.observe_focus();
        }
        if self.initialized && !self.gate.active() {
            return Err("Return to Afterpull and release controls before loading.".into());
        }
        self.game.restore(state)
    }
}
impl Game for NativeGame {
    fn show_hud() -> bool {
        false
    }
    fn menu_status(&self) -> String {
        self.game.menu_status()
    }
    fn device_input(&mut self, input: Intent) -> Intent {
        self.initialized = true;
        let space = self.observe_focus();
        if self.gate.active() {
            self.game.map_device_input(input, space)
        } else {
            Intent::default()
        }
    }
    fn draw<'a>(&'a self, scene: &mut Scene<'a>) {
        self.game.draw(scene);
        if !self.gate.active() {
            scene.rect(
                150,
                Rect::new(334, 66, 316, 316),
                Color::new(0.025, 0.04, 0.06, 0.9),
            );
            scene.text(151, "FOCUS PAUSED", Point::new(360, 208), 24., WHITE);
            scene.text(
                151,
                "Return and release controls",
                Point::new(356, 239),
                14.,
                WHITE,
            );
            scene.text(
                151,
                "before making a fresh tap.",
                Point::new(366, 261),
                14.,
                WHITE,
            );
        }
    }
}
