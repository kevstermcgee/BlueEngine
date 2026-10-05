//! Arcade ball-and-paddle rules, using the shared 2D client.
use serde::{Deserialize, Serialize};
use vesper3d::two_d::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    pub tick: u32,
    pub ball: Rect,
    pub velocity: Point,
    pub paddle: Rect,
    pub gates: u32,
    pub lost: bool,
}
pub struct Garden {
    pub state: State,
    cues: Vec<usize>,
}
impl GameLogic for Garden {
    const ID: &'static str = "pocket-breaker";
    const TITLE: &'static str = "Pocket Breaker";
    const CONTROLS: &'static str =
        "Mouse or arrows/pad steer the paddle · Break all six glass bands";
    const VERIFY_TICKS: u32 = 1800;
    fn new(_: u64) -> Self {
        Self {
            state: State {
                tick: 0,
                ball: Rect::new(400, 300, 12, 12),
                velocity: Point::new(3, -4),
                paddle: Rect::new(350, 365, 100, 14),
                gates: 0,
                lost: false,
            },
            cues: vec![],
        }
    }
    fn probe_input() -> Intent {
        Intent {
            pointer: Some(Point::new(650, 365)),
            ..Default::default()
        }
    }
    fn probe_success(&self) -> bool {
        self.state.paddle.x > 500
    }
    fn tick(&self) -> u32 {
        self.state.tick
    }
    fn outcome(&self) -> &'static str {
        if self.state.gates == 63 {
            "won"
        } else if self.state.lost {
            "lost"
        } else {
            "playing"
        }
    }
    fn verification_input(tick: u32) -> Intent {
        let phase = (360 + tick as i32 * 3) % 1420;
        let x = 40 + if phase <= 710 { phase } else { 1420 - phase };
        Intent {
            pointer: Some(Point::new(x + 6, 365)),
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
        self.state.paddle.x = if i.x != 0 {
            (self.state.paddle.x + i.x.clamp(-1, 1) * 7).clamp(30, 670)
        } else if let Some(p) = i.pointer {
            (p.x - 50).clamp(30, 670)
        } else {
            self.state.paddle.x
        };
        self.state.ball.x += self.state.velocity.x;
        if self.state.ball.x < 40 || self.state.ball.x > 750 {
            self.state.velocity.x = -self.state.velocity.x;
            self.state.ball.x += 2 * self.state.velocity.x;
        }
        self.state.ball.y += self.state.velocity.y;
        if self.state.ball.y < 60 {
            self.state.ball.y = 60;
            self.state.velocity.y = 4;
        }
        if self.state.velocity.y > 0 && self.state.ball.overlaps(self.state.paddle) {
            self.state.ball.y = self.state.paddle.y - 12;
            self.state.velocity.y = -4;
            self.cues.push(0);
        }
        for n in 0..6 {
            if self.state.gates & (1 << n) == 0
                && self
                    .state
                    .ball
                    .overlaps(Rect::new(40, 75 + n * 32, 722, 18))
            {
                self.state.gates |= 1 << n;
                self.state.velocity.y = -self.state.velocity.y;
                self.cues.push(0);
                break;
            }
        }
        if self.state.ball.y > 400 || self.state.tick > 5400 {
            self.state.lost = true;
            self.cues.push(1);
        }
        if self.state.gates == 63 {
            self.cues.push(2);
        }
    }
    fn state_hash(&self) -> u64 {
        hash_json(&self.state)
    }
}
impl Snapshot for Garden {
    const KIND: &'static str = "pocket-breaker";
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, s: State) -> Result<(), String> {
        if s.gates > 63
            || s.ball.w != 12
            || s.ball.h != 12
            || s.ball.x < 30
            || s.ball.x > 762
            || s.ball.y < 50
            || s.ball.y > 420
            || s.velocity.x.abs() != 3
            || s.velocity.y.abs() != 4
            || s.paddle.w != 100
            || s.paddle.h != 14
            || s.paddle.y != 365
            || !(30..=670).contains(&s.paddle.x)
        {
            return Err("Invalid breaker save".into());
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
        s.rect(
            -5,
            Rect::new(30, 50, 740, 350),
            Color::new(0.13, 0.15, 0.28, 1.),
        );
        for n in 0..6 {
            if self.state.gates & (1 << n) == 0 {
                s.rect(
                    1,
                    Rect::new(40, 75 + n * 32, 722, 18),
                    if n % 2 == 0 { TEAL } else { GOLD },
                );
            }
        }
        s.rect(3, self.state.paddle, WHITE);
        s.circle(
            4,
            Point::new(self.state.ball.x + 6, self.state.ball.y + 6),
            8.,
            PINK,
        );
        s.text(
            10,
            format!("Glass cleared {}/6", self.state.gates.count_ones()),
            Point::new(450, 30),
            22.,
            TEAL,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use vesper3d::runtime::{assert_deterministic, snapshot};
    #[test]
    fn public_paddle_route_wins_and_resumes() {
        let inputs: Vec<_> = (0..Garden::VERIFY_TICKS)
            .map(Garden::verification_input)
            .collect();
        assert_deterministic(|| Garden::new(7), &inputs);
        snapshot::assert_resumes_exactly(|| Garden::new(7), &inputs, 171);
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
    fn missing_paddle_loses_and_restart_is_fresh() {
        let mut game = Garden::new(7);
        game.state.paddle.x = 30;
        for _ in 0..1000 {
            game.step(&Intent::default());
        }
        assert_eq!(game.outcome(), "lost");
        assert_eq!(Garden::new(7).outcome(), "playing");
    }
}
