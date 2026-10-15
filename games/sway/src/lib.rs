//! Rendering-free variation rules and bounded live audio analysis.
pub mod audio;
use serde::{Deserialize, Serialize};
use vesper3d::portable::*;
pub const STYLES: [&str; 7] = [
    "Automatic",
    "Orbit",
    "Silk",
    "Prism",
    "Bloom",
    "Stardust",
    "Tide",
];
pub const PALETTES: [&str; 7] = [
    "Automatic",
    "Aurora",
    "Ember",
    "Iris",
    "Lagoon",
    "Sunrise",
    "Moonlight",
];
pub const MOTIONS: [&str; 3] = ["Calm", "Flow", "Surge"];
pub fn mix(mut n: u64) -> u64 {
    n = (n ^ (n >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    n = (n ^ (n >> 27)).wrapping_mul(0x94d049bb133111eb);
    n ^ (n >> 31)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct State {
    pub ticks: u64,
    pub seed: u64,
    pub listens: u64,
    pub song: u64,
    pub style: u8,
    pub palette: u8,
    pub motion: u8,
}
pub struct Sway {
    pub state: State,
}
impl Sway {
    pub fn begin_song(&mut self, fingerprint: u64) {
        self.state.song = fingerprint;
        self.state.listens = self.state.listens.wrapping_add(1);
        self.reroll();
    }
    pub fn reroll(&mut self) {
        self.state.seed = mix(self.state.seed.wrapping_add(0x9e3779b97f4a7c15)
            ^ self.state.song
            ^ self.state.listens);
    }
    pub fn key(&self, chapter: u64) -> u64 {
        mix(self.state.seed ^ self.state.song ^ chapter.wrapping_mul(0x9e3779b97f4a7c15))
    }
    pub fn title_alpha(elapsed: f32) -> f32 {
        if !(0.0..10.0).contains(&elapsed) {
            0.0
        } else {
            (elapsed / 0.35).min(1.0) * ((10.0 - elapsed) / 1.0).min(1.0)
        }
    }
}
impl GameLogic for Sway {
    const ID: &'static str = "sway";
    const TITLE: &'static str = "Sway";
    const CONTROLS: &'static str = "Drop music / O import · Space pause · Esc options · Arrows track · R fresh look · F fullscreen";
    const VERIFY_TICKS: u32 = 900;
    fn new(seed: u64) -> Self {
        Self {
            state: State {
                ticks: 0,
                seed,
                listens: 0,
                song: 0,
                style: 0,
                palette: 0,
                motion: 1,
            },
        }
    }
    fn tick(&self) -> u32 {
        self.state.ticks as u32
    }
    fn outcome(&self) -> &'static str {
        "playing"
    }
    fn probe_input() -> Intent {
        Intent {
            action: true,
            ..Default::default()
        }
    }
    fn probe_success(&self) -> bool {
        self.state.ticks > 0
    }
    fn verification_input(t: u32) -> Intent {
        Intent {
            action: t == 60 || t == 600,
            x: i32::from(t == 300),
            y: i32::from(t == 450),
            ..Default::default()
        }
    }
}
impl Simulation for Sway {
    type Input = Intent;
    fn step(&mut self, input: &Intent) {
        self.state.ticks = self.state.ticks.wrapping_add(1);
        if input.action {
            self.reroll();
        }
        if input.x != 0 {
            self.state.style = (self.state.style as i32 + input.x.signum()).rem_euclid(7) as u8;
        }
        if input.y != 0 {
            self.state.palette = (self.state.palette as i32 + input.y.signum()).rem_euclid(7) as u8;
        }
    }
    fn state_hash(&self) -> u64 {
        hash_json(&self.state)
    }
}
impl Snapshot for Sway {
    const KIND: &'static str = "sway";
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, state: State) -> Result<(), String> {
        if state.style > 6 || state.palette > 6 || state.motion > 2 {
            return Err("Invalid Sway visual settings".into());
        }
        self.state = state;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deterministic_commands_and_snapshot_resume() {
        let inputs: Vec<_> = (0..900).map(Sway::verification_input).collect();
        vesper3d::runtime::assert_deterministic(|| Sway::new(7), &inputs);
        vesper3d::runtime::snapshot::assert_resumes_exactly(|| Sway::new(7), &inputs, 457);
        let (hash, outcome) = verify::<Sway>();
        assert_eq!(outcome, "playing");
        if let Ok(path) = std::env::var("BE2_VERIFY_REPORT") {
            std::fs::write(path, serde_json::json!({"hash":format!("{hash:016x}"),"outcome":outcome,"ticks":900,"purpose":"Reroll, select shape and palette, preserve exact visual future through a snapshot"}).to_string()).unwrap();
        }
    }
    #[test]
    fn repeat_listens_and_different_songs_have_fresh_identity() {
        let mut s = Sway::new(7);
        s.begin_song(101);
        let a = s.key(0);
        s.begin_song(101);
        assert_ne!(a, s.key(0));
        let mut other = Sway::new(7);
        other.begin_song(102);
        assert_ne!(a, other.key(0));
        assert_ne!(s.key(0), s.key(1));
    }
    #[test]
    fn title_expires_at_ten_seconds_and_invalid_restore_is_atomic() {
        assert_eq!(Sway::title_alpha(10.), 0.);
        assert_eq!(Sway::title_alpha(15.), 0.);
        assert_eq!(Sway::title_alpha(5.), 1.);
        let mut s = Sway::new(7);
        let before = s.capture();
        let mut bad = before.clone();
        bad.style = 7;
        assert!(s.restore(bad).is_err());
        assert_eq!(s.capture(), before);
    }
}
