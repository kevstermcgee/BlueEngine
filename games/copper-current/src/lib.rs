//! Ninety-second collection run; simulation owns rules and draw only reads state.
use serde::{Deserialize, Serialize};
use vesper3d::two_d::*;
pub const RUN_TICKS: u32 = 90 * 60;
pub const WALLS: [Rect; 5] = [
    Rect::new(20, 60, 760, 10),
    Rect::new(20, 360, 760, 10),
    Rect::new(20, 60, 10, 310),
    Rect::new(770, 60, 10, 310),
    Rect::new(350, 160, 100, 110),
];
fn bloom(n: u32) -> Rect {
    Rect::new(
        160 + (n % 4) as i32 * 140,
        if n < 4 { 95 } else { 310 },
        24,
        24,
    )
}
fn drums(tick: u32) -> [Rect; 2] {
    let phase = (tick * 2 % 840) as i32;
    let sweep = if phase < 420 { phase } else { 840 - phase };
    let phase = (tick * 2 % 520) as i32;
    let fall = if phase < 260 { phase } else { 520 - phase };
    [
        Rect::new(240 + sweep, 210, 34, 34),
        Rect::new(500, 80 + fall, 34, 34),
    ]
}
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
    const ID: &'static str = "copper-current";
    const TITLE: &'static str = "Copper Current";
    const CONTROLS: &'static str =
        "WASD/arrows/pad move - collect 8 coils, dodge drums for 90 seconds";
    const VERIFY_TICKS: u32 = RUN_TICKS;
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
        } else if self.state.hearts <= 0 || self.state.tick >= RUN_TICKS {
            "lost"
        } else {
            "playing"
        }
    }
    fn verification_input(tick: u32) -> Intent {
        let (x, y) = match tick {
            0..=219 => (1, 0),
            220..=289 => (0, 1),
            290..=489 => (-1, 0),
            490..=559 => (0, -1),
            _ => (0, 0),
        };
        Intent {
            x,
            y,
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
        for n in 0..8 {
            if self.state.collected & (1 << n) == 0 && bloom(n).overlaps(self.state.player) {
                self.state.collected |= 1 << n;
                self.cues.push(0);
            }
        }
        if self.state.cooldown == 0
            && drums(self.state.tick)
                .iter()
                .any(|r| r.overlaps(self.state.player))
        {
            self.state.hearts -= 1;
            self.state.cooldown = 90;
            self.cues.push(1);
        }
        if self.state.tick == RUN_TICKS && self.state.collected == 255 && self.state.hearts > 0 {
            self.state.won = true;
            self.cues.push(2);
        }
    }
    fn state_hash(&self) -> u64 {
        hash_json(&self.state)
    }
}
impl Snapshot for Garden {
    const KIND: &'static str = "copper-current";
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, state: State) -> Result<(), String> {
        if state.tick > RUN_TICKS
            || state.hearts < 0
            || state.hearts > 3
            || state.collected > 255
            || state.player.w != 24
            || state.player.h != 24
            || state.player.x < 30
            || state.player.x > 746
            || state.player.y < 70
            || state.player.y > 336
            || state.cooldown > 90
            || (state.won
                && (state.tick != RUN_TICKS || state.collected != 255 || state.hearts <= 0))
        {
            return Err(
                "Invalid Copper Current save: check timer, health, coils and arena bounds".into(),
            );
        }
        self.state = state;
        self.cues.clear();
        Ok(())
    }
}
#[cfg(feature = "client")]
mod art {
    use super::*;
    use draw::{Color, Scene};
    pub const COPPER: Color = Color::new(0.88, 0.46, 0.27, 1.);
    pub const TEAL: Color = Color::new(0.18, 0.77, 0.73, 1.);
    pub const PLUM: Color = Color::new(0.39, 0.18, 0.40, 1.);
    pub const CREAM: Color = Color::new(0.98, 0.91, 0.76, 1.);
    // Original prop 1: a copper spool with three offset teal coil apertures.
    pub fn coil_bloom(s: &mut Scene, r: Rect) {
        s.rect(2, Rect::new(r.x + 4, r.y + 18, 16, 6), COPPER);
        for (dx, dy) in [(6, 8), (18, 8), (12, 16)] {
            s.circle(3, Point::new(r.x + dx, r.y + dy), 7., COPPER);
            s.circle(4, Point::new(r.x + dx, r.y + dy), 3., TEAL);
        }
        s.rect(5, Rect::new(r.x + 10, r.y, 4, 18), CREAM);
    }
    // Original prop 2: a plum storm drum, copper bands and a teal charge core.
    pub fn storm_drum(s: &mut Scene, r: Rect) {
        s.circle(2, Point::new(r.x + 17, r.y + 17), 17., PLUM);
        s.rect(3, Rect::new(r.x + 3, r.y + 5, 28, 5), COPPER);
        s.rect(3, Rect::new(r.x + 3, r.y + 24, 28, 5), COPPER);
        s.circle(4, Point::new(r.x + 17, r.y + 17), 7., TEAL);
        s.rect(5, Rect::new(r.x + 15, r.y + 11, 4, 12), CREAM);
    }
}
#[cfg(feature = "client")]
impl draw::Game for Garden {
    fn draw(&self, s: &mut draw::Scene) {
        use art::*;
        s.rect(
            -10,
            Rect::new(0, 0, 800, 450),
            draw::Color::new(0.07, 0.12, 0.16, 1.),
        );
        for y in 0..6 {
            for x in 0..15 {
                s.rect(
                    -5,
                    Rect::new(32 + x * 49, 72 + y * 48, 46, 45),
                    draw::Color::new(0.11, 0.22, 0.25, 1.),
                );
            }
        }
        for wall in WALLS {
            s.rect(0, wall, PLUM);
        }
        for n in 0..8 {
            if self.state.collected & (1 << n) == 0 {
                coil_bloom(s, bloom(n));
            }
        }
        for r in drums(self.state.tick) {
            storm_drum(s, r);
        }
        let r = self.state.player;
        s.rect(6, r, CREAM);
        s.rect(7, Rect::new(r.x + 4, r.y + 5, 16, 7), TEAL);
        s.rect(7, Rect::new(r.x + 9, r.y + 16, 6, 8), COPPER);
        s.text(
            10,
            format!(
                "COILS {}/8   HULL {}   {}s",
                self.state.collected.count_ones(),
                self.state.hearts,
                (RUN_TICKS - self.state.tick).div_ceil(60)
            ),
            Point::new(340, 30),
            20.,
            COPPER,
        );
    }
    fn menu_status(&self) -> String {
        format!(
            "Collect 8 coil blooms. Dodge storm drums. Survive 90s. Hull: {}",
            self.state.hearts
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn route_collects_both_rows_and_survives_exactly_ninety_seconds() {
        let inputs: Vec<_> = (0..RUN_TICKS).map(Garden::verification_input).collect();
        vesper3d::runtime::assert_deterministic(|| Garden::new(7), &inputs);
        vesper3d::runtime::snapshot::assert_resumes_exactly(|| Garden::new(7), &inputs, 491);
        let mut g = Garden::new(7);
        for i in &inputs[..RUN_TICKS as usize - 1] {
            g.step(i);
        }
        assert_eq!(g.state.collected, 255);
        assert_eq!(g.outcome(), "playing");
        g.step(inputs.last().unwrap());
        assert_eq!(g.tick(), RUN_TICKS);
        assert_eq!(g.outcome(), "won");
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
    fn missing_coils_times_out_and_terminal_state_stops() {
        let mut g = Garden::new(7);
        for _ in 0..RUN_TICKS {
            g.step(&Intent::default());
        }
        assert_eq!(g.outcome(), "lost");
        let hash = g.state_hash();
        g.step(&Intent {
            x: 1,
            ..Default::default()
        });
        assert_eq!(hash, g.state_hash());
    }
    #[test]
    fn wall_blocks_and_drum_collision_has_cooldown() {
        let mut g = Garden::new(7);
        g.state.player = Rect::new(326, 180, 24, 24);
        g.step(&Intent {
            x: 1,
            ..Default::default()
        });
        assert_eq!(g.state.player.x, 326);
        let r = drums(2)[0];
        g.state.player = Rect::new(r.x, r.y, 24, 24);
        g.step(&Intent::default());
        assert_eq!(g.state.hearts, 2);
        g.step(&Intent::default());
        assert_eq!(g.state.hearts, 2);
        for _ in 0..2 {
            g.state.cooldown = 0;
            let r = drums(g.tick() + 1)[0];
            g.state.player.x = r.x;
            g.state.player.y = r.y;
            g.step(&Intent::default());
        }
        assert_eq!(g.outcome(), "lost");
        assert_eq!(Garden::new(7).state.hearts, 3);
    }
    #[test]
    fn invalid_timer_save_is_rejected() {
        let mut g = Garden::new(7);
        let mut state = g.capture();
        state.tick = RUN_TICKS + 1;
        assert!(g.restore(state).is_err());
    }
}
