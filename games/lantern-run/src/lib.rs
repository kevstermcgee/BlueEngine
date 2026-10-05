//! Rules and presentation; shared client owns devices, audio, storage and timing.
use serde::{Deserialize, Serialize};
#[cfg(test)]
use vesper3d::runtime::snapshot;
use vesper3d::two_d::*;
pub const WALLS: [Rect; 6] = [
    Rect::new(20, 60, 760, 10),
    Rect::new(20, 360, 760, 10),
    Rect::new(20, 60, 10, 310),
    Rect::new(770, 60, 10, 310),
    Rect::new(220, 140, 20, 220),
    Rect::new(450, 70, 20, 30),
];
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    pub tick: u32,
    pub player: Rect,
    pub collected: u32,
    pub hearts: i32,
    pub cooldown: u32,
    pub won: bool,
}
pub struct Garden {
    pub state: State,
    cues: Vec<usize>,
}
impl GameLogic for Garden {
    const ID: &'static str = "lantern-run";
    const TITLE: &'static str = "Lantern Run";
    const CONTROLS: &'static str = "WASD/arrows/pad move · Collect four lanterns, reach the exit";
    const VERIFY_TICKS: u32 = 240;
    fn new(_: u64) -> Self {
        Self {
            state: State {
                tick: 0,
                player: Rect::new(55, 100, 24, 24),
                collected: 0,
                hearts: 3,
                cooldown: 0,
                won: false,
            },
            cues: vec![],
        }
    }
    fn probe_input() -> Intent {
        Intent {
            x: 1,
            ..Default::default()
        }
    }
    fn probe_success(&self) -> bool {
        self.state.player.x > 55
    }
    fn tick(&self) -> u32 {
        self.state.tick
    }
    fn outcome(&self) -> &'static str {
        if self.state.won {
            "won"
        } else if self.state.hearts <= 0 {
            "lost"
        } else {
            "playing"
        }
    }
    fn verification_input(_: u32) -> Intent {
        Intent {
            x: 1,
            ..Default::default()
        }
    }
    fn take_cues(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.cues)
    }
}
impl Simulation for Garden {
    type Input = Intent;
    fn step(&mut self, i: &Intent) {
        if self.outcome() != "playing" {
            return;
        }
        self.state.tick += 1;
        self.state.cooldown = self.state.cooldown.saturating_sub(1);
        self.state
            .player
            .slide(i.x.clamp(-1, 1) * 3, i.y.clamp(-1, 1) * 3, &WALLS);
        for n in 0..4 {
            let pickup = Rect::new(160 + n * 140, 95, 24, 30);
            if self.state.collected & (1 << n) == 0 && pickup.overlaps(self.state.player) {
                self.state.collected |= 1 << n;
                self.cues.push(0);
            }
        }
        let hazard = Rect::new(300 + (self.state.tick as i32 % 200), 210, 34, 34);
        if hazard.overlaps(self.state.player) && self.state.cooldown == 0 {
            self.state.hearts -= 1;
            self.state.cooldown = 90;
            self.cues.push(1);
        }
        if self.state.collected == 15 && self.state.player.x > 710 {
            self.state.won = true;
            self.cues.push(2);
        }
    }
    fn state_hash(&self) -> u64 {
        hash_json(&self.state)
    }
}
impl Snapshot for Garden {
    const KIND: &'static str = "lantern-run";
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, state: State) -> Result<(), String> {
        if state.hearts < 0
            || state.hearts > 3
            || state.collected > 15
            || state.player.w != 24
            || state.player.h != 24
            || state.player.x < 30
            || state.player.x > 746
            || state.player.y < 70
            || state.player.y > 336
        {
            return Err("Invalid garden save bounds".into());
        }
        self.state = state;
        self.cues.clear();
        Ok(())
    }
}
#[cfg(feature = "client")]
impl draw::Game for Garden {
    fn draw(&self, s: &mut draw::Scene) {
        use draw::*;
        for y in 0..6 {
            for x in 0..15 {
                s.rect(
                    -5,
                    Rect::new(32 + x * 49, 72 + y * 48, 46, 45),
                    Color::new(0.12, 0.22, 0.24, 1.),
                );
            }
        }
        for wall in WALLS {
            s.rect(0, wall, Color::new(0.30, 0.39, 0.42, 1.));
        }
        for n in 0..4 {
            if self.state.collected & (1 << n) == 0 {
                s.circle(2, Point::new(172 + n * 140, 110), 12., GOLD);
            }
        }
        s.rect(1, Rect::new(710, 75, 55, 70), TEAL);
        s.rect(
            3,
            Rect::new(300 + (self.state.tick as i32 % 200), 210, 34, 34),
            PINK,
        );
        let r = self.state.player;
        let frame = Animation::new(4, 8).unwrap().frame(self.state.tick);
        s.sprite_png(
            4,
            "keeper",
            include_bytes!("../assets/icon.png"),
            Transform {
                position: [r.x as f32 - 3., r.y as f32 - 3.],
                rotation: (frame as f32 - 1.5) * 0.05,
                ..Default::default()
            },
            [30., 30.],
            WHITE,
        );
        s.text(
            10,
            format!(
                "Lanterns {}/4   Hearts {}",
                self.state.collected.count_ones(),
                self.state.hearts
            ),
            Point::new(360, 30),
            22.,
            GOLD,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wins_deterministically_and_save_resumes() {
        let inputs = vec![
            Intent {
                x: 1,
                ..Default::default()
            };
            240
        ];
        vesper3d::runtime::assert_deterministic(|| Garden::new(7), &inputs);
        snapshot::assert_resumes_exactly(|| Garden::new(7), &inputs, 91);
        let (hash, outcome) = verify::<Garden>();
        assert_eq!(outcome, "won");
        if let Ok(path) = std::env::var("BE2_VERIFY_REPORT") {
            std::fs::write(
                path,
                serde_json::json!({"hash":format!("{hash:016x}"),"outcome":outcome}).to_string(),
            )
            .unwrap();
        }
    }
    #[test]
    fn wall_blocks_and_hazard_can_defeat_player() {
        let mut g = Garden::new(7);
        g.state.player = Rect::new(190, 180, 24, 24);
        for _ in 0..10 {
            g.step(&Intent {
                x: 1,
                ..Default::default()
            });
        }
        assert_eq!(g.state.player.x, 196);
        g.state.player = Rect::new(300, 210, 24, 24);
        for _ in 0..3 {
            g.state.cooldown = 0;
            g.state.tick = 0;
            g.step(&Intent::default());
        }
        assert_eq!(g.outcome(), "lost");
        let hash = g.state_hash();
        g.step(&Intent {
            x: 1,
            ..Default::default()
        });
        assert_eq!(hash, g.state_hash());
        assert_eq!(Garden::new(7).state.hearts, 3);
    }
}
