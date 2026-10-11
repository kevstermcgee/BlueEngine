//! Rules and presentation; shared client owns devices, audio, storage and timing.
use serde::{Deserialize, Serialize};
#[cfg(test)]
use vesper3d::runtime::snapshot;
use vesper3d::two_d::*;
pub const TERRACE: Rect = Rect::new(605, 250, 160, 110);
pub const TERRACE_GATE: Rect = Rect::new(645, 240, 50, 10);
pub const SWITCH_ZONE: Rect = Rect::new(650, 170, 44, 68);
pub const WALLS: [Rect; 9] = [
    Rect::new(20, 60, 760, 10),
    Rect::new(20, 360, 760, 10),
    Rect::new(20, 60, 10, 310),
    Rect::new(770, 60, 10, 310),
    Rect::new(220, 140, 20, 220),
    Rect::new(450, 70, 20, 30),
    Rect::new(595, 250, 10, 110),
    Rect::new(605, 240, 40, 10),
    Rect::new(695, 240, 70, 10),
];
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    pub tick: u32,
    pub player: Rect,
    pub collected: u32,
    pub hearts: i32,
    pub cooldown: u32,
    pub won: bool,
    pub terrace_open: bool,
}
pub struct Garden {
    pub state: State,
    cues: Vec<usize>,
}
impl GameLogic for Garden {
    const ID: &'static str = "lantern-grove";
    const TITLE: &'static str = "Lantern Grove";
    const CONTROLS: &'static str = "WASD/arrows/pad move - Space near switch opens Echo Terrace";
    const VERIFY_TICKS: u32 = 400;
    fn new(_: u64) -> Self {
        Self {
            state: State {
                tick: 0,
                player: Rect::new(55, 100, 24, 24),
                collected: 0,
                hearts: 3,
                cooldown: 0,
                won: false,
                terrace_open: false,
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
    fn verification_input(tick: u32) -> Intent {
        let (x, y) = match tick {
            0..=204 => (1, 0),
            205..=234 => (0, 1),
            236..=274 => (0, 1),
            275..=343 => (0, -1),
            344..=399 => (1, 0),
            _ => (0, 0),
        };
        Intent {
            x,
            y,
            action: tick == 235,
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
        if i.action && !self.state.terrace_open && SWITCH_ZONE.overlaps(self.state.player) {
            self.state.terrace_open = true;
            self.cues.push(0);
        }
        let mut walls = WALLS.to_vec();
        if !self.state.terrace_open {
            walls.push(TERRACE_GATE);
        }
        self.state
            .player
            .slide(i.x.clamp(-1, 1) * 3, i.y.clamp(-1, 1) * 3, &walls);
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
    const KIND: &'static str = "lantern-grove";
    const VERSION: u32 = 2;
    const MIGRATIONS: &'static [vesper3d::runtime::snapshot::Migration] =
        &[vesper3d::runtime::snapshot::Migration {
            from: 1,
            step: migrate_v1,
        }];
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
fn migrate_v1(mut state: serde_json::Value) -> Result<serde_json::Value, String> {
    let fields = state
        .as_object_mut()
        .ok_or("Lantern Grove v1 state must be an object")?;
    let player: Rect = serde_json::from_value(
        fields
            .get("player")
            .cloned()
            .ok_or("Lantern Grove v1 state needs player")?,
    )
    .map_err(|e| format!("Lantern Grove v1 player is invalid: {e}"))?;
    // Old players already in this previously open ground must be able to leave it.
    fields.insert(
        "terrace_open".into(),
        serde_json::Value::Bool(TERRACE.overlaps(player)),
    );
    Ok(state)
}
#[cfg(feature = "client")]
impl draw::Game for Garden {
    fn draw(&self, s: &mut draw::Scene) {
        use draw::*;
        let mut world = World::new([4., 5., 5.5], [4., 0., 2.]);
        world.cube(
            [4., -0.2, 2.],
            [8., 0.2, 4.],
            Color::new(0.12, 0.22, 0.24, 1.),
        );
        world.cube(
            [6.825, -0.08, 3.05],
            [1.55, 0.08, 1.1],
            Color::new(0.22, 0.36, 0.29, 1.),
        );
        let mut walls = WALLS.to_vec();
        if !self.state.terrace_open {
            walls.push(TERRACE_GATE);
        }
        for r in walls {
            world.cube(
                [
                    (r.x + r.w / 2) as f32 / 100.,
                    0.2,
                    (r.y + r.h / 2) as f32 / 100.,
                ],
                [r.w as f32 / 100., 0.5, r.h as f32 / 100.],
                Color::new(0.3, 0.39, 0.42, 1.),
            );
        }
        world.cube(
            [6.72, 0.3, 2.0],
            [0.22, 0.6, 0.24],
            if self.state.terrace_open { TEAL } else { GOLD },
        );
        for n in 0..4 {
            if self.state.collected & (1 << n) == 0 {
                world.sphere([(172 + n * 140) as f32 / 100., 0.3, 1.1], 0.12, GOLD);
            }
        }
        world.cube([7.37, 0.1, 1.1], [0.55, 0.2, 0.7], TEAL);
        world.sphere(
            [
                (317 + self.state.tick as i32 % 200) as f32 / 100.,
                0.25,
                2.27,
            ],
            0.2,
            PINK,
        );
        let r = self.state.player;
        world.cube(
            [(r.x + 12) as f32 / 100., 0.22, (r.y + 12) as f32 / 100.],
            [0.24, 0.45, 0.24],
            WHITE,
        );
        s.world(0, Rect::new(20, 55, 760, 345), world);
        s.text(
            20,
            format!(
                "LANTERNS {}/4 HULL {} TERRACE {}",
                self.state.collected.count_ones(),
                self.state.hearts,
                if self.state.terrace_open {
                    "OPEN"
                } else {
                    "LOCKED"
                }
            ),
            Point::new(340, 30),
            16.,
            GOLD,
        );

        s.rect(30, Rect::new(552, 260, 216, 125), INK);
        s.text(31, "MAP", Point::new(565, 280), 16., TEAL);
        s.rect(
            31,
            Rect::new(
                558 + TERRACE.x / 4,
                283 + (TERRACE.y - 60) / 4,
                TERRACE.w / 4,
                TERRACE.h / 4,
            ),
            Color::new(0.22, 0.36, 0.29, 1.),
        );
        for r in WALLS {
            s.rect(
                32,
                Rect::new(
                    558 + r.x / 4,
                    283 + (r.y - 60) / 4,
                    (r.w / 4).max(2),
                    (r.h / 4).max(2),
                ),
                TEAL,
            );
        }
        if !self.state.terrace_open {
            s.rect(
                32,
                Rect::new(
                    558 + TERRACE_GATE.x / 4,
                    283 + (TERRACE_GATE.y - 60) / 4,
                    TERRACE_GATE.w / 4,
                    3,
                ),
                PINK,
            );
        }
        s.circle(
            33,
            Point::new(558 + 672 / 4, 283 + (200 - 60) / 4),
            3.,
            if self.state.terrace_open { TEAL } else { GOLD },
        );
        s.circle(
            33,
            Point::new(558 + (r.x + 12) / 4, 283 + (r.y - 48) / 4),
            4.,
            GOLD,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_action_unlocks_terrace_and_saved_route_resumes() {
        let inputs: Vec<_> = (0..Garden::VERIFY_TICKS)
            .map(Garden::verification_input)
            .collect();
        vesper3d::runtime::assert_deterministic(|| Garden::new(7), &inputs);
        snapshot::assert_resumes_exactly(|| Garden::new(7), &inputs, 260);
        let mut game = Garden::new(7);
        for input in &inputs[..275] {
            game.step(input);
        }
        assert!(game.state.terrace_open);
        assert!(TERRACE.overlaps(game.state.player));
        for input in &inputs[275..] {
            game.step(input);
        }
        assert_eq!(game.outcome(), "won");
    }
    #[test]
    fn gate_blocks_without_action_and_remote_action_cannot_open_it() {
        let mut game = Garden::new(7);
        game.step(&Intent {
            action: true,
            ..Default::default()
        });
        assert!(!game.state.terrace_open);
        game.state.player = Rect::new(670, 216, 24, 24);
        game.step(&Intent {
            y: 1,
            ..Default::default()
        });
        assert_eq!(game.state.player.y, 216);
        game.step(&Intent {
            action: true,
            y: 1,
            ..Default::default()
        });
        assert!(game.state.terrace_open);
        assert_eq!(game.state.player.y, 219);
        game.step(&Intent {
            action: true,
            ..Default::default()
        });
        assert!(game.state.terrace_open);
        assert_eq!(game.take_cues(), vec![0]);
    }
    #[test]
    fn actual_v1_save_inside_new_section_migrates_without_trapping_player() {
        let mut game = Garden::new(7);
        let header = snapshot::restore(
            &mut game,
            include_bytes!("../tests/fixtures/grove-v1.be2save"),
        )
        .unwrap();
        assert_eq!(header.version, 1);
        assert_eq!(game.state.tick, 123);
        assert_eq!(game.state.player, Rect::new(670, 270, 24, 24));
        assert!(game.state.terrace_open);
        for _ in 0..35 {
            game.step(&Intent {
                y: -1,
                ..Default::default()
            });
        }
        assert!(game.state.player.y < TERRACE_GATE.y - game.state.player.h);
    }
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
