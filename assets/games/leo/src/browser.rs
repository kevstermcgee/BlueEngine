//! Browser adapter around the existing authoritative Sim and exact SimState saves.
//! No alternate meadow rules; presentation caches are deliberately outside the save.
use crate::{Input, Sim, SimState};
use vesper3d::portable::{AudioBankSpec, GameLogic, Intent, Simulation, Snapshot};
use vesper3d::runtime::SavePolicy;

pub struct Walker {
    pub sim: Sim,
    cues: Vec<usize>,
    #[cfg(feature = "portable-client")]
    visual: std::cell::RefCell<Option<Visual>>,
}
#[cfg(feature = "portable-client")]
struct Visual {
    scene: crate::scene::Scene,
    materials: vesper3d::viewer::kit::Materials,
    shadows: vesper3d::viewer::kit::Shadows,
}
impl Simulation for Walker {
    type Input = Intent;
    fn step(&mut self, input: &Intent) {
        self.sim
            .step(&Input {
                forward: -input.y as f32,
                right: input.x as f32,
                sprint: input.sprint,
                jump: input.action,
                look: input.look,
            })
            .expect("valid Leo input coordinates");
        if input.action {
            self.cues.push(0);
        }
    }
    fn state_hash(&self) -> u64 {
        self.sim.state_hash()
    }
}
impl Snapshot for Walker {
    const KIND: &'static str = Sim::KIND;
    const POLICY: SavePolicy = SavePolicy::Exact;
    type State = SimState;
    fn capture(&self) -> SimState {
        self.sim.capture()
    }
    fn restore(&mut self, state: SimState) -> Result<(), String> {
        self.sim.restore(state)?;
        self.cues.clear();
        Ok(())
    }
    fn save_tick(&self) -> u64 {
        self.sim.tick
    }
}
impl GameLogic for Walker {
    const ID: &'static str = "leo";
    const TITLE: &'static str = "Leo";
    const CONTROLS: &'static str = "Wander, look around, hop. There is no deadline.";
    const VERIFY_TICKS: u32 = 1200;
    fn new(seed: u64) -> Self {
        Self {
            sim: Sim::new(seed),
            cues: vec![],
            #[cfg(feature = "portable-client")]
            visual: std::cell::RefCell::new(None),
        }
    }
    fn tick(&self) -> u32 {
        self.sim.tick.min(u64::from(u32::MAX)) as u32
    }
    fn outcome(&self) -> &'static str {
        "playing"
    }
    fn verification_input(tick: u32) -> Intent {
        Intent {
            y: -1,
            x: if tick % 400 < 200 { 1 } else { -1 },
            sprint: true,
            action: tick % 200 == 50,
            look: [if tick.is_multiple_of(400) { 0.5 } else { 0. }, 0.],
            ..Default::default()
        }
    }
    fn probe_input() -> Intent {
        Intent { y: -1, action: true, ..Default::default() }
    }
    fn probe_success(&self) -> bool {
        self.sim.tick > 2 && self.sim.player.position.2 < 15.9
    }
    fn streaming_marker(&self) -> (i64, i64) {
        (self.sim.origin.x, self.sim.origin.z)
    }
    fn take_cues(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.cues)
    }
    fn audio_banks() -> &'static [AudioBankSpec] {
        &[
            AudioBankSpec { id: "nature", root: "assets/audio/nature", music: false },
            AudioBankSpec { id: "score", root: "assets/audio/music", music: true },
        ]
    }
    fn audio_level(&self, bank: &str, layer: &str) -> f32 {
        let phase = self.sim.time().phase;
        let value = (-(phase * std::f32::consts::TAU).cos() + 0.22) / 0.44;
        let v = value.clamp(0., 1.);
        let day = v * v * (3. - 2. * v);
        match (bank, layer) {
            ("nature", "birds") => day * 0.6,
            ("nature", "wind") => 0.28 * 0.6,
            ("nature", "night") => (1. - day) * 0.6,
            ("nature", "leaves") => self.sim.forest() * 0.36,
            ("score", "home") => 0.65 * 0.55,
            ("score", "wander") => day * 0.7 * 0.55,
            ("score", "starlight") => (1. - day) * 0.8 * 0.55,
            _ => 0.,
        }
    }
}
#[cfg(feature = "portable-client")]
impl vesper3d::portable::draw::Game for Walker {
    fn draw<'a>(&'a self, scene: &mut vesper3d::portable::draw::Scene<'a>) {
        scene.render_view(0, vesper3d::portable::Rect::new(0, 0, 800, 450), move |viewport| {
            let mut cache = self.visual.borrow_mut();
            if cache.is_none() {
                *cache = Some(Visual {
                    scene: crate::scene::Scene::new(),
                    materials: vesper3d::viewer::kit::Materials::load()
                        .map_err(|e| format!("Leo materials failed: {e}"))?,
                    shadows: vesper3d::viewer::kit::Shadows::new(vesper3d::viewer::devkit::ShadowQuality::Simple),
                });
            }
            let visual = cache.as_mut().expect("initialized presentation");
            visual.scene.draw_view(&self.sim, 1., &visual.materials, &mut visual.shadows, false, Some(viewport))?;
            Ok(())
        });
    }
    fn drag_look() -> bool {
        true
    }
    fn show_hud() -> bool {
        false
    }
    fn menu_status(&self) -> String {
        format!("Day {} · Music N · Sound M", self.sim.time().days + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_adapter_preserves_rules_and_snapshot_continuation() {
        let inputs: Vec<_> = (0..Walker::VERIFY_TICKS).map(Walker::verification_input).collect();
        vesper3d::runtime::assert_deterministic(|| Walker::new(7), &inputs);
        vesper3d::runtime::snapshot::assert_resumes_as_promised(|| Walker::new(7), &inputs, 777);
        let mut browser = Walker::new(7);
        let mut original = Sim::new(7);
        for input in &inputs {
            browser.step(input);
            original
                .step(&Input {
                    forward: -input.y as f32,
                    right: input.x as f32,
                    sprint: input.sprint,
                    jump: input.action,
                    look: input.look,
                })
                .unwrap();
            assert_eq!(browser.state_hash(), original.state_hash());
        }
        assert_ne!(browser.sim.origin, vesper3d::runtime::procedural::ChunkId::default());
        assert_eq!(browser.sim.chunks.chunks().len(), 49);
        let (hash, outcome) = vesper3d::portable::verify::<Walker>();
        if let Ok(path) = std::env::var("BE2_VERIFY_REPORT") {
            std::fs::write(path,serde_json::to_vec(&serde_json::json!({"hash":format!("{hash:016x}"),"outcome":outcome,
                "ticks":Walker::VERIFY_TICKS,"purpose":"walk, hop and cross chunk boundaries in the unchanged open-ended Leo simulation"})).unwrap()).unwrap();
        }
    }
    #[test]
    fn real_input_hops_and_walks_and_restart_is_fresh() {
        let mut game = Walker::new(7);
        game.step(&Walker::probe_input());
        assert!(game.sim.player.position.1 > 1.23);
        for _ in 0..10 {
            game.step(&Intent { y: -1, ..Default::default() });
        }
        assert!(game.probe_success());
        assert_eq!(Walker::new(7).tick(), 0);
        assert_eq!(Walker::audio_banks().len(), 2);
    }
}
