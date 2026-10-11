//! Endless journey; BlueEngine owns timing, input, audio, saves and native lifecycle.
use serde::{Deserialize, Serialize};
use vesper3d::portable::*;
const REGION_MM: u64 = 160_000;
const TURN: i32 = 6_283_185;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State {
    pub ticks: u64,
    pub distance_mm: u64,
    pub seed: u64,
    pub yaw: i32,
    pub pitch: i32,
    pub pace: u8,
}
pub struct Wormhole {
    pub state: State,
    #[cfg(feature = "client")]
    material: std::cell::RefCell<Option<macroquad::material::Material>>,
}
/// Every axial region has an independent reproducible identity.
pub fn mix(mut n: u64) -> u64 {
    n = (n ^ (n >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    n = (n ^ (n >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    n ^ (n >> 31)
}
impl Wormhole {
    pub fn region_key(&self, offset: i64) -> u64 {
        let region = (self.state.distance_mm / REGION_MM).wrapping_add_signed(offset);
        mix(self.state.seed ^ region.wrapping_mul(0x9e37_79b9_7f4a_7c15))
    }
    pub fn pace_name(&self) -> &'static str {
        ["FLOAT", "FALL", "SURGE"][self.state.pace as usize]
    }
}
impl GameLogic for Wormhole {
    const ID: &'static str = "wormhole";
    const TITLE: &'static str = "Wormhole";
    const CONTROLS: &'static str = "Drag / arrows / right stick look · Space pace · F fullscreen";
    const VERIFY_TICKS: u32 = 900;
    fn new(seed: u64) -> Self {
        let game = Self {
            state: State {
                ticks: 0,
                distance_mm: 0,
                seed,
                yaw: 0,
                pitch: 0,
                pace: 1,
            },
            #[cfg(feature = "client")]
            material: Default::default(),
        };
        #[cfg(feature = "client")]
        let game = {
            let mut game = game;
            let args: Vec<_> = std::env::args().collect();
            if args.iter().any(|a| a == "--capture") && !args.iter().any(|a| a == "--verify") {
                if let Some(seconds) = args
                    .windows(2)
                    .find(|a| a[0] == "--preview-seconds")
                    .and_then(|a| a[1].parse::<u32>().ok())
                {
                    for _ in 0..seconds.min(3600) * 60 {
                        game.step(&Intent::default());
                    }
                }
            }
            game
        };
        game
    }
    fn restart(&mut self) {
        *self = Self::new(mix(self
            .state
            .seed
            .wrapping_add(self.state.ticks)
            .wrapping_add(1)));
    }
    fn tick(&self) -> u32 {
        self.state.ticks as u32
    }
    fn outcome(&self) -> &'static str {
        "playing"
    }
    fn probe_input() -> Intent {
        Intent {
            x: 1,
            ..Default::default()
        }
    }
    fn probe_success(&self) -> bool {
        self.state.yaw > 0
    }
    fn verification_input(tick: u32) -> Intent {
        Intent {
            x: if tick < 90 {
                1
            } else if tick < 180 {
                -1
            } else {
                0
            },
            y: if (200..230).contains(&tick) { 1 } else { 0 },
            action: tick == 300 || tick == 600,
            ..Default::default()
        }
    }
    fn audio_banks() -> &'static [AudioBankSpec] {
        &[
            AudioBankSpec {
                id: "home",
                root: "assets/audio/home",
                music: true,
            },
            AudioBankSpec {
                id: "light",
                root: "assets/audio/light",
                music: true,
            },
            AudioBankSpec {
                id: "memory",
                root: "assets/audio/memory",
                music: true,
            },
            AudioBankSpec {
                id: "beyond",
                root: "assets/audio/beyond",
                music: true,
            },
        ]
    }
    fn audio_level(&self, bank: &str, layer: &str) -> f32 {
        // Different phrase lengths and seeded orchestration continually recombine the score.
        let bank_n = match bank {
            "home" => 0,
            "light" => 1,
            "memory" => 2,
            _ => 3,
        };
        let t = self.state.ticks as f64 / 60.;
        let a = (self.region_key(0).rotate_right(bank_n * 11) & 1023) as f64 / 1023.;
        let b = (self.region_key(1).rotate_right(bank_n * 11) & 1023) as f64 / 1023.;
        let u = (self.state.distance_mm % REGION_MM) as f64 / REGION_MM as f64;
        let color = a + (b - a) * u * u * (3. - 2. * u);
        let breathe = 0.5 + 0.5 * (t * (0.023 + bank_n as f64 * 0.007) + bank_n as f64).sin();
        (if layer == "ground" {
            0.09 + 0.15 * color
        } else {
            0.04 + 0.22 * color * breathe
        }) as f32
    }
}
impl Simulation for Wormhole {
    type Input = Intent;
    fn step(&mut self, input: &Intent) {
        self.state.ticks = self.state.ticks.wrapping_add(1);
        if input.action {
            self.state.pace = (self.state.pace + 1) % 3;
        }
        self.state.distance_mm = self
            .state
            .distance_mm
            .wrapping_add([95, 220, 430][self.state.pace as usize]);
        let look = |x: f32| {
            if x.is_finite() {
                (x.clamp(-0.5, 0.5) * 1_000_000.) as i32
            } else {
                0
            }
        };
        self.state.yaw =
            (self.state.yaw + input.x.clamp(-1, 1) * 18_000 + look(input.look[0])).rem_euclid(TURN);
        self.state.pitch = (self.state.pitch - input.y.clamp(-1, 1) * 14_000 + look(input.look[1]))
            .clamp(-1_350_000, 1_350_000);
    }
    fn state_hash(&self) -> u64 {
        hash_json(&self.state)
    }
}
impl Snapshot for Wormhole {
    const KIND: &'static str = "wormhole";
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, state: State) -> Result<(), String> {
        if state.pace > 2
            || !(0..TURN).contains(&state.yaw)
            || !(-1_350_000..=1_350_000).contains(&state.pitch)
        {
            return Err("Invalid Wormhole camera or pace".into());
        }
        self.state = state;
        Ok(())
    }
}
#[cfg(feature = "client")]
mod presentation;
#[cfg(test)]
mod tests {
    use super::*;
    use vesper3d::runtime::{assert_deterministic, snapshot};
    #[test]
    fn public_journey_and_snapshot_resume_exactly() {
        let inputs: Vec<_> = (0..Wormhole::VERIFY_TICKS)
            .map(Wormhole::verification_input)
            .collect();
        assert_deterministic(|| Wormhole::new(7), &inputs);
        snapshot::assert_resumes_exactly(|| Wormhole::new(7), &inputs, 411);
        let mut journey = Wormhole::new(7);
        for input in &inputs {
            journey.step(input);
        }
        assert_eq!(journey.outcome(), "playing");
        assert!(journey.state.distance_mm > REGION_MM);
        assert_eq!(journey.state.pace, 0);
        assert_ne!(journey.state.pitch, 0);
        let (hash, outcome) = verify::<Wormhole>();
        if let Ok(path) = std::env::var("BE2_VERIFY_REPORT") {
            std::fs::write(path, serde_json::json!({"hash":format!("{hash:016x}"),"outcome":outcome,"ticks":Wormhole::VERIFY_TICKS,"purpose":"Continuous travel, look in both directions, cross a region and cycle all three paces"}).to_string()).unwrap();
        }
    }
    #[test]
    fn regions_do_not_repeat_and_coordinates_remain_local_after_years() {
        let mut journey = Wormhole::new(7);
        let mut keys = std::collections::HashSet::new();
        for region in 0..10_000 {
            journey.state.distance_mm = region * REGION_MM;
            assert!(keys.insert(journey.region_key(0)));
            let next = journey.region_key(1);
            journey.state.distance_mm += REGION_MM;
            assert_eq!(next, journey.region_key(0));
        }
        journey.state.ticks = u32::MAX as u64 + 99;
        journey.state.distance_mm = 1_000_000_000_000;
        journey.step(&Intent::default());
        assert!(journey.state.ticks > u32::MAX as u64);
        assert!(journey.state.distance_mm % REGION_MM < REGION_MM);
        assert_ne!(journey.region_key(0), Wormhole::new(71).region_key(0));
    }
    #[test]
    fn look_and_pace_are_bounded_and_bad_saves_preserve_journey() {
        let mut journey = Wormhole::new(7);
        for _ in 0..1000 {
            journey.step(&Intent {
                x: 1,
                y: -1,
                look: [f32::NAN, f32::INFINITY],
                ..Default::default()
            });
        }
        assert_eq!(journey.state.pitch, 1_350_000);
        assert!((0..TURN).contains(&journey.state.yaw));
        let hash = journey.state_hash();
        let mut invalid = journey.capture();
        invalid.pace = 4;
        assert!(journey.restore(invalid).is_err());
        assert_eq!(hash, journey.state_hash());
        journey.restart();
        assert_eq!(journey.state.distance_mm, 0);
        assert_ne!(journey.state.seed, 7);
    }
    #[test]
    fn music_mix_is_finite_and_changes_over_the_journey() {
        let mut journey = Wormhole::new(7);
        let before = journey.audio_level("memory", "stars");
        for region in 0..1000 {
            journey.state.distance_mm = region * REGION_MM + REGION_MM / 2;
            journey.state.ticks = region * 7109;
            for bank in Wormhole::audio_banks() {
                for layer in ["ground", "stars"] {
                    assert!((0.0..=0.3).contains(&journey.audio_level(bank.id, layer)));
                }
            }
        }
        assert_ne!(before, journey.audio_level("memory", "stars"));
    }
}
