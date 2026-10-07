//! Prickle Putt: nine-hole hedgehog mini-golf for two people passing one controller.
//!
//! Prickle and Bramble curl up and get flung around hedge-walled greens. Aim with the stick, press
//! once to start the power meter, press again to fling. Every hole, the controller changes hands.
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
/// A hole is scored at this many strokes if the ball is still out.
pub const STROKE_CAP: u8 = 7;
pub const NAMES: [&str; 2] = ["Prickle", "Bramble"];

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
    /// Title card: pick who tees off, read the records.
    Intro,
    /// "Pass the controller": waits for the next player's press.
    Handoff,
    Aim,
    Power,
    Rolling,
    /// Sink animation.
    Holed,
    /// Hole result card for the player who just finished.
    HoleDone,
    /// Round over: totals, winner, records.
    Results,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Records {
    pub rounds: u32,
    pub wins: [u32; 2],
    /// Best completed round per hedgehog; 0 means none yet.
    pub best_round: [u32; 2],
    pub aces: [u32; 2],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub tick: u32,
    pub phase: Phase,
    pub timer: u32,
    pub first: u8,
    pub player: u8,
    pub hole: u8,
    pub strokes: [[u8; HOLES]; 2],
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
        self.state.strokes[self.state.player as usize][self.state.hole as usize]
    }
    pub fn total(&self, player: usize) -> u32 {
        self.state.strokes[player].iter().map(|&s| s as u32).sum()
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
    fn begin_hole_for(&mut self, player: u8) {
        self.state.player = player;
        self.state.phase = Phase::Handoff;
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
        let p = self.state.player as usize;
        let h = self.state.hole as usize;
        if !holed {
            self.state.strokes[p][h] = STROKE_CAP + 1;
        }
        if self.state.strokes[p][h] == 1 {
            self.state.records.aces[p] += 1;
        }
        self.state.phase = Phase::HoleDone;
        self.state.timer = 0;
    }
    fn after_hole_done(&mut self) {
        if self.state.player == self.state.first {
            self.begin_hole_for(1 - self.state.first);
        } else if (self.state.hole as usize) + 1 < HOLES {
            self.state.hole += 1;
            self.begin_hole_for(self.state.first);
        } else {
            let totals = [self.total(0), self.total(1)];
            let r = &mut self.state.records;
            r.rounds += 1;
            for p in 0..2 {
                if r.best_round[p] == 0 || totals[p] < r.best_round[p] {
                    r.best_round[p] = totals[p];
                }
            }
            if totals[0] != totals[1] {
                r.wins[usize::from(totals[1] < totals[0])] += 1;
            }
            self.state.phase = Phase::Results;
            self.state.timer = 0;
        }
    }
    fn new_round(&mut self) {
        self.state.strokes = [[0; HOLES]; 2];
        self.state.hole = 0;
        self.begin_hole_for(self.state.first);
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
            let p = s.player as usize;
            let h = s.hole as usize;
            s.strokes[p][h] = s.strokes[p][h].saturating_add(1);
            let capped = s.strokes[p][h] >= STROKE_CAP;
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
                let capped = s.strokes[s.player as usize][s.hole as usize] >= STROKE_CAP;
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
    const CONTROLS: &'static str = "Stick/arrows aim · Space/A: start meter, then fling · Pass after each hole";
    const VERIFY_TICKS: u32 = 900;
    fn new(_: u64) -> Self {
        let mut game = Self {
            state: State {
                tick: 0,
                phase: Phase::Intro,
                timer: 0,
                first: 0,
                player: 0,
                hole: 0,
                strokes: [[0; HOLES]; 2],
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
        self.state.first != 0 || self.state.phase != Phase::Intro
    }
    fn tick(&self) -> u32 {
        self.state.tick
    }
    fn outcome(&self) -> &'static str {
        "playing"
    }
    fn verification_input(tick: u32) -> Intent {
        // Pick Bramble to tee off, confirm the handoff, start the meter, fling, then keep
        // pressing so the route crosses handoffs, rolls and hole cards.
        let press = matches!(tick, 5 | 30 | 60 | 85) || (tick > 120 && tick % 150 == 0);
        Intent {
            x: i32::from((5..20).contains(&tick)),
            y: i32::from((300..330).contains(&tick)),
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
        let between = matches!(
            self.state.phase,
            Phase::Intro | Phase::Handoff | Phase::HoleDone | Phase::Results
        );
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
        match self.state.phase {
            Phase::Intro => {
                if i.x != 0 {
                    self.state.first = u8::from(i.x > 0);
                }
                if i.action {
                    self.new_round();
                    self.cues.push(0);
                }
            }
            Phase::Handoff => {
                if i.action {
                    self.state.phase = Phase::Aim;
                    self.state.hold = 0;
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
                    let p = s.player as usize;
                    let h = s.hole as usize;
                    s.strokes[p][h] = s.strokes[p][h].saturating_add(1);
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
                    self.state.first = 1 - self.state.first;
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
            || state.player > 1
            || state.first > 1
            || !(0..256).contains(&state.aim)
            || !(0..=100).contains(&state.power)
            || state.strokes.iter().flatten().any(|&s| s > STROKE_CAP + 1)
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
const BRAMBLE_PNG: &[u8] = include_bytes!("../assets/sprites/bramble.png");
#[cfg(feature = "client")]
impl draw::Game for Putt {
    fn show_hud() -> bool {
        false
    }
    fn menu_status(&self) -> String {
        format!("Hole {} of {}  ·  Prickle {}  Bramble {}", self.state.hole + 1, HOLES, self.total(0), self.total(1))
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
        let tints = [Color::new(0.45, 0.65, 1., 1.), Color::new(1., 0.6, 0.3, 1.)];
        let sprite = |p: usize| -> (&'static str, &'static [u8]) {
            if p == 0 { ("prickle", PRICKLE_PNG) } else { ("bramble", BRAMBLE_PNG) }
        };
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
            let (id, png) = sprite(st.player as usize);
            s.sprite_png(
                8,
                id,
                png,
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
        s.text(20, format!("Hole {}  ·  Par {}", st.hole + 1, l.par), Point::new(330, 30), 22., GOLD);
        for p in 0..2 {
            let x = 520 + p as i32 * 130;
            let active = playing_ball && st.player as usize == p;
            s.text(20, format!("{} {}", NAMES[p], self.total(p)), Point::new(x, 30), 20., if active { tints[p] } else { WHITE });
            if active {
                s.rect(19, Rect::new(x - 4, 36, 100, 3), tints[p]);
            }
        }
        let dim = Color::new(0.02, 0.05, 0.04, 0.72);
        match st.phase {
            Phase::Intro => {
                s.rect(30, Rect::new(60, 60, 680, 330), dim);
                s.text(31, "PRICKLE PUTT", Point::new(260, 110), 44., GOLD);
                s.text(31, "Nine holes. One controller. Pass it after every hole.", Point::new(170, 140), 20., WHITE);
                s.text(31, "Who tees off first?  (left / right, then press)", Point::new(220, 175), 18., TEAL);
                for p in 0..2usize {
                    let x = 230 + p as i32 * 250;
                    let chosen = st.first as usize == p;
                    if chosen {
                        s.rect(31, Rect::new(x - 20, 190, 140, 150), Color::new(1., 1., 1., 0.12));
                    }
                    let (id, png) = sprite(p);
                    s.sprite_png(32, id, png, Transform { position: [x as f32 + 2., 198.], ..Default::default() }, [96., 96.], WHITE);
                    s.text(32, NAMES[p], Point::new(x + 10, 322), 22., if chosen { tints[p] } else { WHITE });
                }
                let r = &st.records;
                s.text(31, format!("Rounds {}   Wins {}-{}   Best {} / {}   Aces {} / {}", r.rounds, r.wins[0], r.wins[1], r.best_round[0], r.best_round[1], r.aces[0], r.aces[1]), Point::new(150, 370), 17., TEAL);
            }
            Phase::Handoff => {
                s.rect(30, Rect::new(160, 120, 480, 200), dim);
                let p = st.player as usize;
                s.text(31, format!("Pass the controller to {}", NAMES[p]), Point::new(200, 170), 26., tints[p]);
                s.text(31, format!("Hole {}, par {}. Press when ready.", st.hole + 1, l.par), Point::new(200, 210), 20., WHITE);
                if st.hole > 0 || st.player != st.first {
                    s.text(31, format!("Prickle {}  ·  Bramble {}", self.total(0), self.total(1)), Point::new(200, 250), 20., GOLD);
                }
                s.text(31, "Stick aims · press once for the meter, again to fling", Point::new(200, 290), 16., TEAL);
            }
            Phase::HoleDone => {
                s.rect(30, Rect::new(160, 120, 480, 200), dim);
                let p = st.player as usize;
                let strokes = st.strokes[p][st.hole as usize];
                s.text(31, Self::verdict(strokes, l.par), Point::new(200, 175), 34., GOLD);
                let shown = if strokes > STROKE_CAP { format!("{} (cap)", strokes) } else { strokes.to_string() };
                s.text(31, format!("{} finished hole {} in {}", NAMES[p], st.hole + 1, shown), Point::new(200, 215), 20., WHITE);
                s.text(31, format!("Prickle {}  ·  Bramble {}", self.total(0), self.total(1)), Point::new(200, 250), 20., tints[p]);
                s.text(31, "Press to continue", Point::new(200, 290), 16., TEAL);
            }
            Phase::Results => {
                s.rect(30, Rect::new(100, 80, 600, 300), dim);
                let t = [self.total(0), self.total(1)];
                let title = if t[0] == t[1] { "A TIE!".to_string() } else { format!("{} WINS!", NAMES[usize::from(t[1] < t[0])]) };
                s.text(31, title, Point::new(140, 130), 36., GOLD);
                s.text(31, format!("Prickle {}   Bramble {}   (par {})", t[0], t[1], par_total()), Point::new(140, 170), 22., WHITE);
                for p in 0..2 {
                    let row: Vec<String> = st.strokes[p].iter().map(|v| v.to_string()).collect();
                    s.text(31, format!("{:<8} {}", NAMES[p], row.join("  ")), Point::new(140, 210 + p as i32 * 28), 18., tints[p]);
                }
                let r = &st.records;
                s.text(31, format!("Wins {}-{}   Best rounds {} / {}", r.wins[0], r.wins[1], r.best_round[0], r.best_round[1]), Point::new(140, 290), 18., TEAL);
                s.text(31, "Press for a new round (the other hedgehog tees off first)", Point::new(140, 340), 16., WHITE);
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
        assert_eq!(g.state.phase, Phase::Handoff);
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
        assert_eq!(g.state.first, 1, "the route picks Bramble to tee off");
        assert!(g.total(1) >= 1, "the route flings at least once");
        if let Ok(path) = std::env::var("BE2_VERIFY_REPORT") {
            std::fs::write(
                path,
                serde_json::json!({"hash":format!("{hash:016x}"),"outcome":outcome,"ticks":Putt::VERIFY_TICKS,
                    "purpose":"Open-ended hot-seat golf: picks the first player, confirms the handoff, flings with the meter and continues through hole cards"}).to_string(),
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
    #[test]
    fn cap_ends_a_hole_and_records_survive_rounds_and_restart() {
        let mut g = Putt::new(7);
        start_hole(&mut g);
        for _ in 0..STROKE_CAP {
            assert_eq!(g.state.phase, Phase::Aim);
            fling(&mut g, 64, 1);
            settle(&mut g);
        }
        assert_eq!(g.state.phase, Phase::HoleDone);
        assert_eq!(g.state.strokes[0][0], STROKE_CAP + 1);
        // Bramble now plays; sink every hole for both by teleporting next to the cup and tapping it in.
        let mut guard = 0;
        while g.state.phase != Phase::Results && guard < 60 {
            guard += 1;
            for _ in 0..25 {
                g.step(&Intent::default());
            }
            g.step(&press());
            if g.state.phase == Phase::Handoff {
                g.step(&press());
            }
            if g.state.phase != Phase::Aim {
                continue;
            }
            let l = g.layout();
            g.state.x = (l.cup.0 - 20) * SUB;
            g.state.y = l.cup.1 * SUB;
            fling(&mut g, 0, 0);
            settle(&mut g);
            for _ in 0..60 {
                g.step(&Intent::default());
            }
        }
        assert_eq!(g.state.phase, Phase::Results, "a whole round plays out");
        assert_eq!(g.state.records.rounds, 1);
        assert!(g.state.records.best_round[0] > 0 && g.state.records.best_round[1] > 0);
        let records = g.state.records.clone();
        g.restart();
        assert_eq!(g.state.phase, Phase::Intro);
        assert_eq!(g.state.records, records, "restart keeps the scoreboard");
        assert_eq!(g.total(0) + g.total(1), 0);
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
        g.step(&Intent { x: 1, ..Default::default() });
        assert!(g.probe_success());
        assert_eq!(Putt::verdict(1, 3), "HOLE IN ONE!");
        assert_eq!(Putt::verdict(2, 3), "BIRDIE!");
        assert_eq!(Putt::verdict(5, 3), "DOUBLE BOGEY");
        assert_eq!(Putt::verdict(8, 3), "PICKED UP");
    }
}
