//! Mouse-driven resource/tower strategy; presentation never decides hits or economy.
use serde::{Deserialize, Serialize};
use vesper3d::two_d::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct Enemy {
    pub x: i32,
    pub lane: usize,
    pub hp: i32,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    pub tick: u32,
    pub coins: i32,
    pub hearts: i32,
    pub towers: [bool; 3],
    pub cooldown: [u32; 3],
    pub spawned: u32,
    pub defeated: u32,
    pub enemies: Vec<Enemy>,
}
pub struct Garden {
    pub state: State,
    cues: Vec<usize>,
}
impl GameLogic for Garden {
    const ID: &'static str = "orchard-watch";
    const TITLE: &'static str = "Orchard Watch";
    const CONTROLS: &'static str =
        "Click a pad to plant a guard (2 seeds) · Protect three orchard lanes";
    const VERIFY_TICKS: u32 = 2200;
    fn new(_: u64) -> Self {
        Self {
            state: State {
                tick: 0,
                coins: 4,
                hearts: 3,
                towers: [false; 3],
                cooldown: [0; 3],
                spawned: 0,
                defeated: 0,
                enemies: vec![],
            },
            cues: vec![],
        }
    }
    fn probe_input() -> Intent {
        Intent {
            action: true,
            pointer: Some(Point::new(350, 110)),
            ..Default::default()
        }
    }
    fn probe_success(&self) -> bool {
        self.state.towers[0]
    }
    fn tick(&self) -> u32 {
        self.state.tick
    }
    fn outcome(&self) -> &'static str {
        if self.state.hearts <= 0 {
            "lost"
        } else if self.state.spawned == 9 && self.state.enemies.is_empty() {
            "won"
        } else {
            "playing"
        }
    }
    fn verification_input(tick: u32) -> Intent {
        let lane = match tick {
            0 => Some(0),
            1 => Some(1),
            650 => Some(2),
            _ => None,
        };
        Intent {
            action: lane.is_some(),
            pointer: lane.map(|n| Point::new(350, 110 + n * 100)),
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
        if i.action {
            if let Some(p) = i.pointer {
                for lane in 0..3 {
                    if Rect::new(325, 85 + lane as i32 * 100, 50, 50).contains(p)
                        && !self.state.towers[lane]
                        && self.state.coins >= 2
                    {
                        self.state.towers[lane] = true;
                        self.state.coins -= 2;
                        self.cues.push(0);
                    }
                }
            }
        }
        if self.state.spawned < 9 && self.state.tick == 90 + self.state.spawned * 180 {
            self.state.enemies.push(Enemy {
                x: 750,
                lane: self.state.spawned as usize % 3,
                hp: 3,
            });
            self.state.spawned += 1;
        }
        for cooldown in &mut self.state.cooldown {
            *cooldown = cooldown.saturating_sub(1);
        }
        for enemy in &mut self.state.enemies {
            enemy.x -= 2;
            if self.state.towers[enemy.lane]
                && self.state.cooldown[enemy.lane] == 0
                && (enemy.x - 350).abs() < 180
            {
                enemy.hp -= 1;
                self.state.cooldown[enemy.lane] = 20;
            }
        }
        self.state.enemies.retain(|e| {
            if e.hp <= 0 {
                self.state.coins += 2;
                self.state.defeated += 1;
                self.cues.push(0);
                false
            } else if e.x < 80 {
                self.state.hearts -= 1;
                self.cues.push(1);
                false
            } else {
                true
            }
        });
        if self.outcome() == "won" {
            self.cues.push(2);
        }
    }
    fn state_hash(&self) -> u64 {
        hash_json(&self.state)
    }
}
impl Snapshot for Garden {
    const KIND: &'static str = "orchard-watch";
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, s: State) -> Result<(), String> {
        if s.coins < 0
            || s.coins > 22
            || s.hearts < 0
            || s.hearts > 3
            || s.spawned > 9
            || s.defeated > s.spawned
            || s.enemies.len() > 9
            || s.enemies
                .iter()
                .any(|e| e.lane >= 3 || e.hp <= 0 || e.hp > 3 || e.x < 80 || e.x > 750)
        {
            return Err("Invalid orchard save bounds".into());
        }
        self.state = s;
        self.cues.clear();
        Ok(())
    }
}
#[cfg(feature = "client")]
impl draw::Game for Garden {
    fn draw(&self, s: &mut draw::Scene) {
        use draw::*;
        for lane in 0..3 {
            let y = 110 + lane as i32 * 100;
            s.rect(
                -5,
                Rect::new(55, y - 28, 710, 56),
                Color::new(0.15, 0.26, 0.18, 1.),
            );
            s.circle(1, Point::new(90, y), 24., GOLD);
            s.rect(
                1,
                Rect::new(325, y - 25, 50, 50),
                if self.state.towers[lane] {
                    TEAL
                } else {
                    Color::new(0.32, 0.35, 0.28, 1.)
                },
            );
            s.text(
                3,
                if self.state.towers[lane] {
                    "GUARD"
                } else {
                    "2 seeds"
                },
                Point::new(322, y + 48),
                16.,
                WHITE,
            );
        }
        for e in &self.state.enemies {
            s.circle(4, Point::new(e.x, 110 + e.lane as i32 * 100), 16., PINK);
            s.text(
                5,
                e.hp.to_string(),
                Point::new(e.x - 6, 116 + e.lane as i32 * 100),
                16.,
                INK,
            );
        }
        s.text(
            10,
            format!(
                "Seeds {} · Hearts {} · Saved {}/9",
                self.state.coins, self.state.hearts, self.state.defeated
            ),
            Point::new(290, 30),
            22.,
            GOLD,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use vesper3d::runtime::{assert_deterministic, snapshot};
    #[test]
    fn real_mouse_strategy_wins_and_resumes() {
        let inputs: Vec<_> = (0..Garden::VERIFY_TICKS)
            .map(Garden::verification_input)
            .collect();
        assert_deterministic(|| Garden::new(7), &inputs);
        snapshot::assert_resumes_exactly(|| Garden::new(7), &inputs, 531);
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
    fn no_guards_loses_and_building_twice_never_spends_again() {
        let mut g = Garden::new(7);
        let input = Garden::verification_input(0);
        g.step(&input);
        g.step(&input);
        assert_eq!(g.state.coins, 2);
        let mut g = Garden::new(7);
        for _ in 0..2200 {
            g.step(&Intent::default());
        }
        assert_eq!(g.outcome(), "lost");
    }
}
