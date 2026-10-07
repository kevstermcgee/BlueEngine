//! Prickle Putt: nine-hole hedgehog mini-golf.
//!
//! Prickle curls up and gets flung around hedge-walled greens. Aim with the stick, press once to start
//! the power meter, press again to fling. Beat par, earn a medal, and chase your best score on every hole.
//! Rules are integer-only and rendering-free; the shared client owns devices, audio, saves and timing.
use serde::{Deserialize, Serialize};
#[cfg(test)]
use vesper3d::runtime::snapshot;
use vesper3d::two_d::*;

pub const HOLES: usize = 9;
/// Sub-pixel units per logical pixel for ball position and velocity.
pub const SUB: i32 = 64;
/// Ball radius in logical pixels.
pub const BALL: i32 = 12;
/// The most strokes a hole can take; the next one is scored as a pick-up.
pub const STROKE_CAP: u8 = 7;

const QUARTER: [i32; 65] = [
    0, 101, 201, 301, 401, 501, 601, 700, 799, 897, 995, 1092, 1189, 1285, 1380, 1474, 1567, 1660,
    1751, 1842, 1931, 2019, 2106, 2191, 2276, 2359, 2440, 2520, 2598, 2675, 2751, 2824, 2896, 2967,
    3035, 3102, 3166, 3229, 3290, 3349, 3406, 3461, 3513, 3564, 3612, 3659, 3703, 3745, 3784, 3822,
    3857, 3889, 3920, 3948, 3973, 3996, 4017, 4036, 4052, 4065, 4076, 4085, 4091, 4095, 4096,
];
/// Sine of a direction in 1/256 turns, scaled by 4096. Pure table lookup, so native and web agree.
pub fn sin256(dir: i32) -> i32 {
    let d = dir.rem_euclid(256);
    match d {
        0..=64 => QUARTER[d as usize],
        65..=128 => QUARTER[(128 - d) as usize],
        129..=192 => -QUARTER[(d - 128) as usize],
        _ => -QUARTER[(256 - d) as usize],
    }
}
pub fn cos256(dir: i32) -> i32 {
    sin256(dir + 64)
}
/// Integer square root (floor), Newton iteration; deterministic everywhere.
pub fn isqrt(v: i64) -> i64 {
    if v <= 0 {
        return 0;
    }
    let mut x = v;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + v / x) / 2;
    }
    x
}
/// The direction (1/256 turns, +x is 0, +y is 64) that best matches a vector.
pub fn dir_to(dx: i32, dy: i32) -> i32 {
    let mut best = 0;
    let mut best_dot = i64::MIN;
    for d in 0..256 {
        let dot = dx as i64 * cos256(d) as i64 + dy as i64 * sin256(d) as i64;
        if dot > best_dot {
            best_dot = dot;
            best = d;
        }
    }
    best
}

#[derive(Clone, Copy, Debug)]
pub struct Bumper {
    pub x: i32,
    pub y: i32,
    pub r: i32,
}
pub struct Layout {
    pub tee: (i32, i32),
    pub cup: (i32, i32),
    pub par: u8,
    pub walls: &'static [Rect],
    pub water: &'static [Rect],
    pub sand: &'static [Rect],
    pub bumpers: &'static [Bumper],
}
/// The green: x 40..760, y 70..390, hedged on all sides.
pub const FIELD: Rect = Rect::new(40, 70, 720, 320);
const EDGES: [Rect; 4] = [
    Rect::new(30, 60, 740, 10),
    Rect::new(30, 390, 740, 10),
    Rect::new(30, 60, 10, 340),
    Rect::new(760, 60, 10, 340),
];
const fn bumper(x: i32, y: i32, r: i32) -> Bumper {
    Bumper { x, y, r }
}
const NONE_R: [Rect; 0] = [];
const NONE_B: [Bumper; 0] = [];
const W2: [Rect; 1] = [Rect::new(380, 150, 20, 250)];
const B3: [Bumper; 3] = [bumper(300, 170, 18), bumper(300, 290, 18), bumper(500, 230, 22)];
const WA4: [Rect; 1] = [Rect::new(330, 125, 140, 210)];
const S5: [Rect; 2] = [Rect::new(240, 70, 110, 320), Rect::new(480, 70, 110, 320)];
const W5: [Rect; 2] = [Rect::new(400, 60, 16, 150), Rect::new(400, 300, 16, 100)];
const B6: [Bumper; 5] = [
    bumper(260, 150, 16),
    bumper(260, 310, 16),
    bumper(400, 230, 20),
    bumper(540, 150, 16),
    bumper(540, 310, 16),
];
const W6: [Rect; 2] = [Rect::new(610, 170, 150, 12), Rect::new(610, 60, 12, 122)];
const WA7: [Rect; 4] = [
    Rect::new(470, 120, 230, 66),
    Rect::new(470, 276, 230, 66),
    Rect::new(470, 186, 40, 26),
    Rect::new(470, 250, 40, 26),
];
const W8: [Rect; 3] = [
    Rect::new(220, 60, 16, 220),
    Rect::new(400, 180, 16, 220),
    Rect::new(580, 60, 16, 220),
];
const WA9: [Rect; 2] = [Rect::new(300, 60, 60, 150), Rect::new(300, 290, 60, 110)];
const S9: [Rect; 1] = [Rect::new(520, 70, 90, 320)];
const B9: [Bumper; 3] = [bumper(450, 150, 16), bumper(450, 310, 16), bumper(660, 230, 18)];
pub fn layout(hole: usize) -> Layout {
    match hole {
        0 => Layout { tee: (120, 230), cup: (670, 230), par: 2, walls: &NONE_R, water: &NONE_R, sand: &NONE_R, bumpers: &NONE_B },
        1 => Layout { tee: (110, 340), cup: (680, 120), par: 3, walls: &W2, water: &NONE_R, sand: &NONE_R, bumpers: &NONE_B },
        2 => Layout { tee: (110, 230), cup: (690, 230), par: 3, walls: &NONE_R, water: &NONE_R, sand: &NONE_R, bumpers: &B3 },
        3 => Layout { tee: (110, 230), cup: (690, 230), par: 3, walls: &NONE_R, water: &WA4, sand: &NONE_R, bumpers: &NONE_B },
        4 => Layout { tee: (110, 230), cup: (690, 230), par: 4, walls: &W5, water: &NONE_R, sand: &S5, bumpers: &NONE_B },
        5 => Layout { tee: (110, 340), cup: (700, 110), par: 4, walls: &W6, water: &NONE_R, sand: &NONE_R, bumpers: &B6 },
        6 => Layout { tee: (110, 230), cup: (600, 231), par: 4, walls: &NONE_R, water: &WA7, sand: &NONE_R, bumpers: &NONE_B },
        7 => Layout { tee: (110, 340), cup: (690, 340), par: 4, walls: &W8, water: &NONE_R, sand: &NONE_R, bumpers: &NONE_B },
        _ => Layout { tee: (110, 230), cup: (700, 230), par: 5, walls: &NONE_R, water: &WA9, sand: &S9, bumpers: &B9 },
    }
}
pub fn par_total() -> u32 {
    (0..HOLES).map(|h| layout(h).par as u32).sum()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// Title card with the records.
    Intro,
    Aim,
    Power,
    Rolling,
    /// Sink animation.
    Holed,
    /// Hole result card.
    HoleDone,
    /// Round over: total, medal, records.
    Results,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Records {
    pub rounds: u32,
    /// Best completed round in strokes; 0 means none yet.
    pub best_round: u32,
    pub aces: u32,
    /// Fewest strokes ever taken on each hole; 0 means not holed yet.
    pub best_hole: [u8; HOLES],
    /// Gold, silver and bronze medals earned.
    pub medals: [u32; 3],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub tick: u32,
    pub phase: Phase,
    pub timer: u32,
    pub hole: u8,
    pub strokes: [u8; HOLES],
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    pub last: (i32, i32),
    pub aim: i32,
    pub hold: u32,
    pub power: i32,
    pub power_dir: i32,
    pub roll: i32,
    pub rest: u32,
    pub splash: u32,
    /// Ticks left on the "Hole N, par P" banner.
    pub banner: u32,
    /// Medal for the finished round: 0 gold, 1 silver, 2 bronze, 3 none.
    pub medal: u8,
    pub new_best: bool,
    pub records: Records,
}
pub struct Putt {
    pub state: State,
    cues: Vec<usize>,
}
impl Putt {
    pub fn layout(&self) -> Layout {
        layout(self.state.hole as usize)
    }
    pub fn strokes_now(&self) -> u8 {
        self.state.strokes[self.state.hole as usize]
    }
    pub fn total(&self) -> u32 {
        self.state.strokes.iter().map(|&s| s as u32).sum()
    }
    /// 0 gold (at or under par), 1 silver (within 6), 2 bronze (within 12), 3 none.
    pub fn medal_for(total: u32) -> u8 {
        let par = par_total();
        if total <= par {
            0
        } else if total <= par + 6 {
            1
        } else if total <= par + 12 {
            2
        } else {
            3
        }
    }
    pub fn speed(&self) -> i32 {
        isqrt(self.state.vx as i64 * self.state.vx as i64 + self.state.vy as i64 * self.state.vy as i64) as i32
    }
    fn place_at_tee(&mut self) {
        let l = self.layout();
        self.state.x = l.tee.0 * SUB;
        self.state.y = l.tee.1 * SUB;
        self.state.last = (self.state.x, self.state.y);
        self.state.vx = 0;
        self.state.vy = 0;
        self.state.aim = dir_to(l.cup.0 - l.tee.0, l.cup.1 - l.tee.1);
        self.state.roll = 0;
        self.state.rest = 0;
    }
    fn begin_hole(&mut self) {
        self.state.phase = Phase::Aim;
        self.state.hold = 0;
        self.state.banner = 150;
        self.place_at_tee();
    }
    fn ball_rect(x: i32, y: i32) -> Rect {
        Rect::new(x / SUB - BALL, y / SUB - BALL, BALL * 2, BALL * 2)
    }
    fn blocked_static(walls: &[Rect], x: i32, y: i32) -> bool {
        let r = Self::ball_rect(x, y);
        EDGES.iter().chain(walls).any(|w| w.overlaps(r))
    }
    /// Name of the result of `strokes` on a hole of `par`.
    pub fn verdict(strokes: u8, par: u8) -> &'static str {
        if strokes == 1 {
            "HOLE IN ONE!"
        } else if strokes > STROKE_CAP {
            "PICKED UP"
        } else {
            match strokes as i32 - par as i32 {
                i32::MIN..=-2 => "EAGLE!",
                -1 => "BIRDIE!",
                0 => "PAR",
                1 => "BOGEY",
                _ => "DOUBLE BOGEY",
            }
        }
    }
    fn finish_hole(&mut self, holed: bool) {
        let h = self.state.hole as usize;
        if !holed {
            self.state.strokes[h] = STROKE_CAP + 1;
        }
        let strokes = self.state.strokes[h];
        let r = &mut self.state.records;
        if holed && (r.best_hole[h] == 0 || strokes < r.best_hole[h]) {
            r.best_hole[h] = strokes;
        }
        if holed && strokes == 1 {
            r.aces += 1;
        }
        self.state.phase = Phase::HoleDone;
        self.state.timer = 0;
    }
    fn after_hole_done(&mut self) {
        if (self.state.hole as usize) + 1 < HOLES {
            self.state.hole += 1;
            self.begin_hole();
        } else {
            let total = self.total();
            let medal = Self::medal_for(total);
            let r = &mut self.state.records;
            r.rounds += 1;
            let improved = r.best_round == 0 || total < r.best_round;
            if improved {
                r.best_round = total;
            }
            if medal < 3 {
                r.medals[medal as usize] += 1;
            }
            self.state.medal = medal;
            self.state.new_best = improved;
            self.state.phase = Phase::Results;
            self.state.timer = 0;
        }
    }
    fn new_round(&mut self) {
        self.state.strokes = [0; HOLES];
        self.state.hole = 0;
        self.state.medal = 3;
        self.state.new_best = false;
        self.begin_hole();
    }
    fn step_rolling(&mut self) {
        let l = layout(self.state.hole as usize);
        let s = &mut self.state;
        let in_sand = l.sand.iter().any(|r| r.contains(Point::new(s.x / SUB, s.y / SUB)));
        let drag = if in_sand { 10 } else { 48 };
        s.vx -= s.vx / drag + s.vx.signum() * i32::from(s.vx != 0 && s.vx.abs() < drag);
        s.vy -= s.vy / drag + s.vy.signum() * i32::from(s.vy != 0 && s.vy.abs() < drag);
        let steps = 8;
        for _ in 0..steps {
            let nx = s.x + s.vx / steps;
            if Self::blocked_static(l.walls, nx, s.y) {
                s.vx = -s.vx * 7 / 8;
            } else {
                s.x = nx;
            }
            let ny = s.y + s.vy / steps;
            if Self::blocked_static(l.walls, s.x, ny) {
                s.vy = -s.vy * 7 / 8;
            } else {
                s.y = ny;
            }
            for b in l.bumpers {
                let dx = s.x - b.x * SUB;
                let dy = s.y - b.y * SUB;
                let dist = isqrt(dx as i64 * dx as i64 + dy as i64 * dy as i64);
                let min = ((b.r + BALL) * SUB) as i64;
                if dist < min && dist > 0 {
                    let nx = dx as i64 * 4096 / dist;
                    let ny = dy as i64 * 4096 / dist;
                    let dot = (s.vx as i64 * nx + s.vy as i64 * ny) / 4096;
                    if dot < 0 {
                        s.vx -= (2 * dot * nx / 4096) as i32;
                        s.vy -= (2 * dot * ny / 4096) as i32;
                        let sp = isqrt(s.vx as i64 * s.vx as i64 + s.vy as i64 * s.vy as i64).max(1);
                        let want = sp.max(9 * SUB as i64);
                        s.vx = (s.vx as i64 * want / sp) as i32;
                        s.vy = (s.vy as i64 * want / sp) as i32;
                        self.cues.push(1);
                    }
                    s.x = b.x * SUB + (nx * min / 4096) as i32;
                    s.y = b.y * SUB + (ny * min / 4096) as i32;
                }
            }
        }
        s.roll += s.vx / 16;
        let center = Point::new(s.x / SUB, s.y / SUB);
        if l.water.iter().any(|w| w.contains(center)) {
            s.x = s.last.0;
            s.y = s.last.1;
            s.vx = 0;
            s.vy = 0;
            s.splash = 40;
            let h = s.hole as usize;
            s.strokes[h] = s.strokes[h].saturating_add(1);
            let capped = s.strokes[h] >= STROKE_CAP;
            self.cues.push(1);
            if capped {
                self.finish_hole(false);
            } else {
                self.state.phase = Phase::Aim;
            }
            return;
        }
        let speed = isqrt(s.vx as i64 * s.vx as i64 + s.vy as i64 * s.vy as i64) as i32;
        let ddx = s.x / SUB - l.cup.0;
        let ddy = s.y / SUB - l.cup.1;
        if ddx * ddx + ddy * ddy <= 13 * 13 && speed < 7 * SUB {
            s.x = l.cup.0 * SUB;
            s.y = l.cup.1 * SUB;
            s.vx = 0;
            s.vy = 0;
            s.phase = Phase::Holed;
            s.timer = 0;
            self.cues.push(2);
            return;
        }
        if speed < 14 {
            s.rest += 1;
            if s.rest >= 12 {
                s.vx = 0;
                s.vy = 0;
                s.rest = 0;
                let capped = s.strokes[s.hole as usize] >= STROKE_CAP;
                if capped {
                    self.finish_hole(false);
                } else {
                    s.last = (s.x, s.y);
                    s.phase = Phase::Aim;
                }
            }
        } else {
            s.rest = 0;
        }
    }
}
impl GameLogic for Putt {
    const ID: &'static str = "prickle-putt";
    const TITLE: &'static str = "Prickle Putt";
    const CONTROLS: &'static str = "Stick/arrows aim · Space/A: start the meter, then fling";
    const VERIFY_TICKS: u32 = 900;
    fn new(_: u64) -> Self {
        let mut game = Self {
            state: State {
                tick: 0,
                phase: Phase::Intro,
                timer: 0,
                hole: 0,
                strokes: [0; HOLES],
                x: 0,
                y: 0,
                vx: 0,
                vy: 0,
                last: (0, 0),
                aim: 0,
                hold: 0,
                power: 0,
                power_dir: 1,
                roll: 0,
                rest: 0,
                splash: 0,
                banner: 0,
                medal: 3,
                new_best: false,
                records: Records::default(),
            },
            cues: vec![],
        };
        game.place_at_tee();
        game
    }
    fn probe_input() -> Intent {
        Intent { x: 1, action: true, ..Default::default() }
    }
    fn probe_success(&self) -> bool {
        self.state.phase != Phase::Intro
    }
    fn tick(&self) -> u32 {
        self.state.tick
    }
    fn outcome(&self) -> &'static str {
        "playing"
    }
    fn verification_input(tick: u32) -> Intent {
        // Start the round, open the power meter, fling, then keep pressing so the route crosses
        // rolls, hole cards and the next hole's banner.
        let press = matches!(tick, 5 | 30 | 60) || (tick > 120 && tick % 150 == 0);
        Intent {
            x: i32::from((70..90).contains(&tick)),
            action: press,
            ..Default::default()
        }
    }
    fn take_cues(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.cues)
    }
    fn audio_banks() -> &'static [AudioBankSpec] {
        &[AudioBankSpec { id: "music", root: "assets/audio/music", music: true }]
    }
    fn audio_level(&self, _bank: &str, layer: &str) -> f32 {
        let rolling = self.state.phase == Phase::Rolling;
        let between = matches!(self.state.phase, Phase::Intro | Phase::HoleDone | Phase::Results);
        match layer {
            "meadow" => 0.6,
            "stroll" => if between { 0.2 } else { 0.5 },
            "plink" => if rolling { 0.25 } else { 0.5 },
            "whistle" => if rolling { 0.6 } else if between { 0.45 } else { 0.15 },
            _ => 0.0,
        }
    }
    fn cue_point(&self, _cue: usize) -> Point {
        Point::new(self.state.x / SUB, self.state.y / SUB)
    }
    fn restart(&mut self) {
        let records = self.state.records.clone();
        *self = Self::new(7);
        self.state.records = records;
    }
}
impl Simulation for Putt {
    type Input = Intent;
    fn step(&mut self, i: &Intent) {
        self.state.tick += 1;
        self.state.splash = self.state.splash.saturating_sub(1);
        self.state.banner = self.state.banner.saturating_sub(1);
        match self.state.phase {
            Phase::Intro => {
                if i.action {
                    self.new_round();
                    self.cues.push(0);
                }
            }
            Phase::Aim => {
                let s = &mut self.state;
                if i.x != 0 {
                    s.hold += 1;
                    s.aim += i.x.signum() * if s.hold > 24 { 2 } else { 1 };
                } else {
                    s.hold = 0;
                }
                if i.y != 0 && s.tick % 4 == 0 {
                    s.aim += i.y.signum();
                }
                s.aim = s.aim.rem_euclid(256);
                if i.action {
                    s.phase = Phase::Power;
                    s.power = 0;
                    s.power_dir = 1;
                }
            }
            Phase::Power => {
                let s = &mut self.state;
                s.power += 3 * s.power_dir;
                if s.power >= 100 {
                    s.power = 100;
                    s.power_dir = -1;
                } else if s.power <= 0 {
                    s.power = 0;
                    s.power_dir = 1;
                }
                if i.action {
                    let speed = (2 * SUB + s.power * 12 * SUB / 100).max(SUB);
                    s.vx = (speed as i64 * cos256(s.aim) as i64 / 4096) as i32;
                    s.vy = (speed as i64 * sin256(s.aim) as i64 / 4096) as i32;
                    s.last = (s.x, s.y);
                    let h = s.hole as usize;
                    s.strokes[h] = s.strokes[h].saturating_add(1);
                    s.phase = Phase::Rolling;
                    s.rest = 0;
                    self.cues.push(0);
                }
            }
            Phase::Rolling => self.step_rolling(),
            Phase::Holed => {
                self.state.timer += 1;
                if self.state.timer >= 50 {
                    self.finish_hole(true);
                }
            }
            Phase::HoleDone => {
                self.state.timer += 1;
                if i.action && self.state.timer > 20 {
                    self.after_hole_done();
                }
            }
            Phase::Results => {
                self.state.timer += 1;
                if i.action && self.state.timer > 30 {
                    self.new_round();
                    self.cues.push(0);
                }
            }
        }
    }
    fn state_hash(&self) -> u64 {
        hash_json(&self.state)
    }
}
impl Snapshot for Putt {
    const KIND: &'static str = "prickle-putt";
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, state: State) -> Result<(), String> {
        if state.hole as usize >= HOLES
            || !(0..256).contains(&state.aim)
            || !(0..=100).contains(&state.power)
            || state.strokes.iter().any(|&s| s > STROKE_CAP + 1)
            || state.medal > 3
            || state.x < FIELD.x * SUB
            || state.x > (FIELD.x + FIELD.w) * SUB
            || state.y < FIELD.y * SUB
            || state.y > (FIELD.y + FIELD.h) * SUB
        {
            return Err("Invalid Prickle Putt save".into());
        }
        self.state = state;
        self.cues.clear();
        Ok(())
    }
}
#[cfg(feature = "client")]
const PRICKLE_PNG: &[u8] = include_bytes!("../assets/sprites/prickle.png");
#[cfg(feature = "client")]
impl draw::Game for Putt {
    fn show_hud() -> bool {
        false
    }
    fn menu_status(&self) -> String {
        format!("Hole {} of {}  ·  {} strokes", self.state.hole + 1, HOLES, self.total())
    }
    fn draw(&self, s: &mut draw::Scene) {
        use draw::*;
        let st = &self.state;
        let l = self.layout();
        let grass = Color::new(0.33, 0.62, 0.30, 1.);
        let grass2 = Color::new(0.30, 0.57, 0.27, 1.);
        let hedge = Color::new(0.20, 0.36, 0.17, 1.);
        let hedge_top = Color::new(0.30, 0.50, 0.24, 1.);
        let sand = Color::new(0.87, 0.78, 0.52, 1.);
        let water = Color::new(0.24, 0.52, 0.86, 1.);
        let water2 = Color::new(0.42, 0.68, 0.95, 1.);
        let medal_colors = [GOLD, Color::new(0.82, 0.85, 0.9, 1.), Color::new(0.8, 0.5, 0.3, 1.)];
        let medal_names = ["GOLD", "SILVER", "BRONZE"];
        for i in 0..20 {
            s.rect(-10, Rect::new(i * 40, 0, 40, 450), if i % 2 == 0 { grass } else { grass2 });
        }
        for r in l.sand {
            s.rect(-8, *r, sand);
        }
        for r in l.water {
            s.rect(-8, *r, water);
            for k in 0..(r.h / 24) {
                let wave = Rect::new(r.x + 6 + ((st.tick as i32 / 6 + k * 7) % (r.w - 20).max(1)), r.y + 10 + k * 24, 14, 3);
                if wave.x + wave.w < r.x + r.w {
                    s.rect(-7, wave, water2);
                }
            }
        }
        for w in EDGES.iter() {
            s.rect(1, Rect::new(w.x + w.w.min(w.h) / 2 - 2, w.y + w.w.min(w.h) / 2 - 2, if w.w > w.h { w.w - w.h + 4 } else { 4 }, if w.h > w.w { w.h - w.w + 4 } else { 4 }), hedge);
        }
        for w in l.walls {
            s.rect(1, *w, hedge);
            s.rect(2, Rect::new(w.x, w.y, w.w, 4), hedge_top);
        }
        for b in l.bumpers {
            s.rect(2, Rect::new(b.x - 4, b.y, 8, b.r + 2), WHITE);
            s.circle(3, Point::new(b.x, b.y - 2), b.r as f32, Color::new(0.85, 0.2, 0.25, 1.));
            s.circle(4, Point::new(b.x - b.r / 2, b.y - b.r / 2), 4., WHITE);
            s.circle(4, Point::new(b.x + b.r / 3, b.y - b.r / 4), 3., WHITE);
        }
        s.circle(3, Point::new(l.cup.0, l.cup.1), 15., Color::new(0.12, 0.2, 0.1, 1.));
        s.circle(4, Point::new(l.cup.0, l.cup.1), 11., Color::new(0.04, 0.08, 0.04, 1.));
        s.rect(5, Rect::new(l.cup.0 - 1, l.cup.1 - 48, 3, 46), WHITE);
        s.rect(5, Rect::new(l.cup.0 + 2, l.cup.1 - 48, 22, 14), Color::new(0.95, 0.3, 0.3, 1.));
        if st.splash > 0 {
            s.circle(6, Point::new(st.x / SUB, st.y / SUB), 20. + (40. - st.splash as f32), Color::new(1., 1., 1., st.splash as f32 / 60.));
        }
        let playing_ball = matches!(st.phase, Phase::Aim | Phase::Power | Phase::Rolling | Phase::Holed);
        if playing_ball {
            let sink = if st.phase == Phase::Holed { (50 - st.timer.min(50)) as f32 / 50. } else { 1. };
            let size = 30. * sink;
            s.sprite_png(
                8,
                "prickle",
                PRICKLE_PNG,
                Transform {
                    position: [st.x as f32 / SUB as f32 - size / 2., st.y as f32 / SUB as f32 - size / 2.],
                    rotation: st.roll as f32 / 180.,
                    ..Default::default()
                },
                [size, size],
                WHITE,
            );
        }
        if matches!(st.phase, Phase::Aim | Phase::Power) {
            let len = if st.phase == Phase::Power { 30 + st.power } else { 70 };
            let dots = (len / 10).max(1);
            for k in 1..=dots {
                let d = 18 + k * 10;
                let px = st.x / SUB + d * cos256(st.aim) / 4096;
                let py = st.y / SUB + d * sin256(st.aim) / 4096;
                s.circle(7, Point::new(px, py), if k == dots { 5. } else { 3. }, if st.phase == Phase::Power { GOLD } else { WHITE });
            }
            if st.phase == Phase::Power {
                s.rect(20, Rect::new(300, 404, 200, 14), Color::new(0., 0., 0., 0.5));
                s.rect(21, Rect::new(302, 406, st.power * 196 / 100, 10), if st.power > 85 { PINK } else { GOLD });
                s.text(22, "FLING!", Point::new(508, 416), 16., GOLD);
            } else {
                s.text(22, "Aim, then press to swing", Point::new(300, 416), 16., WHITE);
            }
        }
        if !matches!(st.phase, Phase::Intro | Phase::Results) {
            s.text(20, format!("Hole {}  ·  Par {}", st.hole + 1, l.par), Point::new(40, 30), 22., GOLD);
            let strokes = self.strokes_now() + u8::from(matches!(st.phase, Phase::Aim | Phase::Power));
            s.text(20, format!("Stroke {}", strokes), Point::new(40, 52), 16., WHITE);
            let done = st.hole as usize + usize::from(matches!(st.phase, Phase::Holed | Phase::HoleDone));
            let par_done: i32 = (0..done).map(|h| layout(h).par as i32).sum();
            let taken: i32 = st.strokes[..done].iter().map(|&v| v as i32).sum();
            let diff = taken - par_done;
            let label = if done == 0 { "no holes finished".to_string() } else if diff == 0 { "level par".to_string() } else if diff > 0 { format!("+{} over par", diff) } else { format!("{} under par", -diff) };
            s.text(20, format!("Total {}  ({})", self.total(), label), Point::new(560, 30), 20., WHITE);
            let best = st.records.best_hole[st.hole as usize];
            if best > 0 {
                s.text(20, format!("Your best on this hole: {}", best), Point::new(560, 52), 16., TEAL);
            }
        }
        if st.banner > 0 && st.phase == Phase::Aim {
            let a = (st.banner as f32 / 40.).min(1.);
            s.text(25, format!("HOLE {}", st.hole + 1), Point::new(330, 150), 40., Color::new(1., 1., 1., a));
            s.text(25, format!("Par {}", l.par), Point::new(370, 180), 24., Color::new(1., 0.72, 0.25, a));
        }
        let dim = Color::new(0.02, 0.05, 0.04, 0.72);
        match st.phase {
            Phase::Intro => {
                s.rect(30, Rect::new(60, 60, 680, 330), dim);
                s.text(31, "PRICKLE PUTT", Point::new(260, 110), 44., GOLD);
                s.text(31, "Fling the hedgehog. Sink the putt. Beat par on nine holes.", Point::new(150, 142), 20., WHITE);
                s.sprite_png(32, "prickle", PRICKLE_PNG, Transform { position: [352., 160.], ..Default::default() }, [96., 96.], WHITE);
                s.text(31, "Press to tee off", Point::new(330, 290), 20., TEAL);
                let r = &st.records;
                let best = if r.best_round == 0 { "-".to_string() } else { r.best_round.to_string() };
                s.text(31, format!("Rounds {}   Best round {} (par {})   Hole-in-ones {}", r.rounds, best, par_total(), r.aces), Point::new(150, 330), 17., TEAL);
                s.text(31, format!("Medals: gold {}  silver {}  bronze {}", r.medals[0], r.medals[1], r.medals[2]), Point::new(240, 358), 17., GOLD);
            }
            Phase::HoleDone => {
                s.rect(30, Rect::new(160, 120, 480, 200), dim);
                let strokes = st.strokes[st.hole as usize];
                s.text(31, Self::verdict(strokes, l.par), Point::new(200, 175), 34., GOLD);
                let shown = if strokes > STROKE_CAP { format!("{} (picked up)", strokes) } else { strokes.to_string() };
                s.text(31, format!("Hole {} in {}  ·  par {}", st.hole + 1, shown, l.par), Point::new(200, 215), 20., WHITE);
                let best = st.records.best_hole[st.hole as usize];
                if best > 0 && best == strokes && strokes <= STROKE_CAP {
                    s.text(31, "Your best on this hole!", Point::new(200, 250), 18., PINK);
                }
                s.text(31, "Press to continue", Point::new(200, 290), 16., TEAL);
            }
            Phase::Results => {
                s.rect(100, Rect::new(100, 80, 600, 300), dim);
                let t = self.total();
                s.text(101, "ROUND COMPLETE", Point::new(140, 125), 34., GOLD);
                s.text(101, format!("{} strokes  (par {})", t, par_total()), Point::new(140, 168), 24., WHITE);
                if st.medal < 3 {
                    s.text(101, format!("{} MEDAL!", medal_names[st.medal as usize]), Point::new(140, 205), 26., medal_colors[st.medal as usize]);
                } else {
                    s.text(101, "No medal yet. Within 12 of par earns bronze.", Point::new(140, 205), 18., WHITE);
                }
                if st.new_best {
                    s.text(101, "NEW BEST ROUND!", Point::new(140, 238), 22., PINK);
                }
                let row: Vec<String> = st.strokes.iter().map(|v| v.to_string()).collect();
                s.text(101, format!("Holes  {}", row.join("  ")), Point::new(140, 280), 18., TEAL);
                let pars: Vec<String> = (0..HOLES).map(|h| layout(h).par.to_string()).collect();
                s.text(101, format!("Par    {}", pars.join("  ")), Point::new(140, 304), 18., WHITE);
                s.text(101, "Press to play again", Point::new(140, 345), 16., WHITE);
            }
            _ => {}
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn press() -> Intent {
        Intent { action: true, ..Default::default() }
    }
    fn start_hole(g: &mut Putt) {
        g.step(&press());
        assert_eq!(g.state.phase, Phase::Aim);
    }
    fn fling(g: &mut Putt, aim: i32, power: i32) {
        g.state.aim = aim;
        g.step(&press());
        assert_eq!(g.state.phase, Phase::Power);
        g.state.power = power;
        g.step(&press());
        assert_eq!(g.state.phase, Phase::Rolling);
    }
    fn settle(g: &mut Putt) -> u32 {
        let mut n = 0;
        while g.state.phase == Phase::Rolling && n < 2000 {
            g.step(&Intent::default());
            n += 1;
        }
        n
    }
    #[test]
    fn verification_route_is_deterministic_and_resumes() {
        let inputs: Vec<Intent> = (0..Putt::VERIFY_TICKS).map(Putt::verification_input).collect();
        vesper3d::runtime::assert_deterministic(|| Putt::new(7), &inputs);
        snapshot::assert_resumes_exactly(|| Putt::new(7), &inputs, 97);
        let (hash, outcome) = verify::<Putt>();
        assert_eq!(outcome, "playing");
        let mut g = Putt::new(7);
        for i in &inputs {
            g.step(i);
        }
        assert_ne!(g.state.phase, Phase::Intro, "the route starts the round");
        assert!(g.total() >= 1, "the route flings at least once");
        if let Ok(path) = std::env::var("BE2_VERIFY_REPORT") {
            std::fs::write(
                path,
                serde_json::json!({"hash":format!("{hash:016x}"),"outcome":outcome,"ticks":Putt::VERIFY_TICKS,
                    "purpose":"Open-ended mini-golf: starts the round, opens the power meter, flings and continues through hole cards"}).to_string(),
            )
            .unwrap();
        }
    }
    #[test]
    fn a_putt_travels_and_a_straight_line_holes_out() {
        let mut g = Putt::new(7);
        start_hole(&mut g);
        let before = g.state.x;
        fling(&mut g, 0, 100);
        assert_eq!(g.strokes_now(), 1);
        settle(&mut g);
        assert!(g.state.x > before + 300 * SUB, "the ball travels down the green");
        let mut holed = false;
        for _ in 0..4 {
            if g.state.phase == Phase::Holed {
                holed = true;
                break;
            }
            let l = g.layout();
            let aim = dir_to(l.cup.0 - g.state.x / SUB, l.cup.1 - g.state.y / SUB);
            let dist = (l.cup.0 - g.state.x / SUB).abs().max(8);
            fling(&mut g, aim, (dist * 100 / 600).clamp(3, 100));
            settle(&mut g);
        }
        holed |= g.state.phase == Phase::Holed;
        assert!(holed, "a straight line to the cup holes out within a few flings");
        for _ in 0..60 {
            g.step(&Intent::default());
        }
        assert_eq!(g.state.phase, Phase::HoleDone);
        assert!(g.state.records.best_hole[0] >= 1, "the first hole's best is recorded");
    }
    #[test]
    fn walls_reflect_and_bumpers_speed_the_ball_up() {
        let mut g = Putt::new(7);
        start_hole(&mut g);
        fling(&mut g, 128, 60);
        for _ in 0..30 {
            g.step(&Intent::default());
        }
        assert!(g.state.vx > 0, "the ball bounced back off the hedge");
        assert!(g.state.x / SUB >= FIELD.x + BALL - 2, "never inside the hedge");
        let mut h = Putt::new(7);
        h.state.hole = 2;
        h.place_at_tee();
        h.state.phase = Phase::Aim;
        let b = h.layout().bumpers[0];
        h.state.x = (b.x - 60) * SUB;
        h.state.y = b.y * SUB;
        fling(&mut h, 0, 20);
        let launch = h.speed();
        let mut peak = 0;
        for _ in 0..40 {
            h.step(&Intent::default());
            peak = peak.max(h.speed());
        }
        assert!(peak >= launch && h.state.vx < 0, "a mushroom kicks the ball back at speed");
    }
    #[test]
    fn water_costs_a_stroke_and_sand_is_slow() {
        let mut g = Putt::new(7);
        g.state.hole = 3;
        g.place_at_tee();
        g.state.phase = Phase::Aim;
        let origin = (g.state.x, g.state.y);
        fling(&mut g, 0, 100);
        assert!(settle(&mut g) < 400);
        assert_eq!((g.state.x, g.state.y), origin, "the ball returns to where it was flung from");
        assert_eq!(g.strokes_now(), 2, "one stroke plus one penalty");
        assert_eq!(g.state.phase, Phase::Aim);
        let mut grass = Putt::new(7);
        grass.state.phase = Phase::Aim;
        fling(&mut grass, 0, 50);
        settle(&mut grass);
        let mut dune = Putt::new(7);
        dune.state.hole = 4;
        dune.place_at_tee();
        dune.state.phase = Phase::Aim;
        fling(&mut dune, 0, 50);
        settle(&mut dune);
        assert!(dune.state.x < grass.state.x, "sand stops the ball sooner");
    }
    /// Sink every remaining hole with a tap from beside the cup.
    fn tap_in_everything(g: &mut Putt) {
        let mut guard = 0;
        while g.state.phase != Phase::Results && guard < 60 {
            guard += 1;
            for _ in 0..25 {
                g.step(&Intent::default());
            }
            if g.state.phase == Phase::HoleDone {
                g.step(&press());
            }
            if g.state.phase != Phase::Aim {
                continue;
            }
            let l = g.layout();
            g.state.x = (l.cup.0 - 20) * SUB;
            g.state.y = l.cup.1 * SUB;
            fling(g, 0, 0);
            settle(g);
            for _ in 0..60 {
                g.step(&Intent::default());
            }
        }
    }
    #[test]
    fn cap_ends_a_hole_and_a_round_earns_a_medal_and_records() {
        let mut g = Putt::new(7);
        start_hole(&mut g);
        for _ in 0..STROKE_CAP {
            assert_eq!(g.state.phase, Phase::Aim);
            fling(&mut g, 64, 1);
            settle(&mut g);
        }
        assert_eq!(g.state.phase, Phase::HoleDone);
        assert_eq!(g.state.strokes[0], STROKE_CAP + 1, "seven strokes and you pick up");
        assert_eq!(g.state.records.best_hole[0], 0, "a pick-up is not a best");
        tap_in_everything(&mut g);
        assert_eq!(g.state.phase, Phase::Results, "a whole round plays out");
        assert_eq!(g.state.records.rounds, 1);
        assert!(g.state.records.best_round > 0 && g.state.new_best);
        assert!(g.state.records.best_hole[1..].iter().all(|&b| b >= 1));
        assert!(g.state.records.aces >= 1, "tapping in from the cup edge on a fresh hole can be an ace");
        let records = g.state.records.clone();
        g.restart();
        assert_eq!(g.state.phase, Phase::Intro);
        assert_eq!(g.state.records, records, "restart keeps the records");
        assert_eq!(g.total(), 0);
        g.step(&press());
        assert_eq!(g.state.hole, 0);
        assert!(g.state.banner > 0, "each hole opens with a banner");
    }
    #[test]
    fn medals_follow_par() {
        let par = par_total();
        assert_eq!(par, 32);
        assert_eq!(Putt::medal_for(par), 0);
        assert_eq!(Putt::medal_for(par + 6), 1);
        assert_eq!(Putt::medal_for(par + 12), 2);
        assert_eq!(Putt::medal_for(par + 13), 3);
    }
    #[test]
    fn saves_reject_nonsense_and_the_probe_is_meaningful() {
        let mut g = Putt::new(7);
        let mut bad = g.capture();
        bad.hole = 9;
        assert!(g.restore(bad).is_err());
        let mut far = g.capture();
        far.x = 0;
        assert!(g.restore(far).is_err());
        assert!(!g.probe_success());
        g.step(&Putt::probe_input());
        assert!(g.probe_success());
        assert_eq!(Putt::verdict(1, 3), "HOLE IN ONE!");
        assert_eq!(Putt::verdict(2, 3), "BIRDIE!");
        assert_eq!(Putt::verdict(5, 3), "DOUBLE BOGEY");
        assert_eq!(Putt::verdict(8, 3), "PICKED UP");
    }
}
