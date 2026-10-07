//! Puff Pop: Pip the pufferfish dives through an endless reef.
//!
//! Hold up (stick, D-pad, W or the arrow key) to inflate and float; let go to sink; hold down to
//! squeeze small. Thread the coral gaps, grab pearls in a chain and pop the bubble walls while fully
//! puffed. Three hearts per dive; beat your best and earn hats. Integer rules, rendering-free.
use serde::{Deserialize, Serialize};
#[cfg(test)]
use vesper3d::runtime::snapshot;
use vesper3d::two_d::*;

pub const TOP: i32 = 52;
pub const FLOOR: i32 = 398;
pub const FISH_X: i32 = 200;
pub const HEARTS: i32 = 3;
/// Lifetime pearls needed for each hat tier.
pub const HAT_TIERS: [u32; 4] = [40, 150, 400, 900];
pub const HAT_NAMES: [&str; 5] = ["bare", "sailor cap", "party hat", "gold crown", "captain's hat"];

const QUARTER: [i32; 65] = [
    0, 101, 201, 301, 401, 501, 601, 700, 799, 897, 995, 1092, 1189, 1285, 1380, 1474, 1567, 1660,
    1751, 1842, 1931, 2019, 2106, 2191, 2276, 2359, 2440, 2520, 2598, 2675, 2751, 2824, 2896, 2967,
    3035, 3102, 3166, 3229, 3290, 3349, 3406, 3461, 3513, 3564, 3612, 3659, 3703, 3745, 3784, 3822,
    3857, 3889, 3920, 3948, 3973, 3996, 4017, 4036, 4052, 4065, 4076, 4085, 4091, 4095, 4096,
];
/// Sine in 1/256 turns scaled by 4096, from a table so native and browser agree exactly.
pub fn sin256(dir: i32) -> i32 {
    let d = dir.rem_euclid(256);
    match d {
        0..=64 => QUARTER[d as usize],
        65..=128 => QUARTER[(128 - d) as usize],
        129..=192 => -QUARTER[(d - 128) as usize],
        _ => -QUARTER[(256 - d) as usize],
    }
}
fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
fn rnd(state: &mut u64, lo: i32, hi: i32) -> i32 {
    lo + (splitmix(state) % (hi - lo + 1) as u64) as i32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Pearl,
    Coral,
    Urchin,
    Jelly,
    Wall,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Obj {
    pub kind: Kind,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub phase: i32,
    pub alive: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Intro,
    Dive,
    Sunk,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Records {
    pub dives: u32,
    pub best_dive: u32,
    pub best_chain: u32,
    /// Pearls collected over every dive; unlocks the hats.
    pub pearls: u32,
    pub walls_popped: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub tick: u32,
    pub phase: Phase,
    pub timer: u32,
    pub dist16: i32,
    pub ys: i32,
    pub vys: i32,
    pub puff: i32,
    pub hearts: i32,
    pub inv: u32,
    pub chain: u32,
    pub best_chain: u32,
    pub pearls: u32,
    pub points: u32,
    pub rng: u64,
    pub gen_x: i32,
    pub last_cy: i32,
    pub objs: Vec<Obj>,
    pub unlocked: bool,
    pub new_best: bool,
    pub popped: u32,
    pub records: Records,
}
pub struct Puff {
    pub state: State,
    cues: Vec<usize>,
    last_event: Point,
}
pub fn hat_tier(pearls: u32) -> usize {
    HAT_TIERS.iter().filter(|&&t| pearls >= t).count()
}
impl Puff {
    pub fn dist(&self) -> i32 {
        self.state.dist16 >> 4
    }
    pub fn y(&self) -> i32 {
        self.state.ys >> 8
    }
    pub fn radius(&self) -> i32 {
        13 + self.state.puff * 13 / 100
    }
    pub fn score(&self) -> u32 {
        (self.dist().max(0) as u32) / 20 + self.state.points
    }
    pub fn multiplier(&self) -> u32 {
        (1 + self.state.chain / 4).min(8)
    }
    pub fn gap_height(dist: i32) -> i32 {
        (132 - dist / 150).max(66)
    }
    pub fn jelly_y(&self, o: &Obj) -> i32 {
        o.y + sin256(self.state.tick as i32 * 3 + o.phase) * 44 / 4096
    }
    fn speed16(&self) -> i32 {
        (32 + self.dist() / 110).min(62)
    }
    fn start_dive(&mut self) {
        let s = &mut self.state;
        s.dist16 = 0;
        s.ys = 220 << 8;
        s.vys = 0;
        s.puff = 25;
        s.hearts = HEARTS;
        s.inv = 0;
        s.chain = 0;
        s.best_chain = 0;
        s.pearls = 0;
        s.points = 0;
        s.gen_x = 520;
        s.last_cy = 220;
        s.objs.clear();
        s.unlocked = false;
        s.popped = 0;
        s.rng = 0xC0FFEE ^ (u64::from(s.records.dives) * 7919) ^ u64::from(s.tick);
        s.phase = Phase::Dive;
        s.timer = 0;
        s.new_best = false;
        self.generate();
    }
    fn push(&mut self, kind: Kind, x: i32, y: i32, w: i32, h: i32, phase: i32) {
        self.state.objs.push(Obj { kind, x, y, w, h, phase, alive: true });
    }
    /// Lay out the next 420-pixel stretch of reef. Every pattern leaves a flyable path.
    fn generate(&mut self) {
        while self.state.gen_x < self.dist() + FISH_X + 1500 {
            let x0 = self.state.gen_x;
            let dist = x0;
            let mut rng = self.state.rng;
            let first = x0 < 700;
            let roll = if first { 1 } else { rnd(&mut rng, 0, 99) };
            // The fish climbs about 3 px per tick; the faster the reef scrolls, the less height
            // a pattern may ask for, so every layout stays flyable.
            let speed16 = (32 + x0 / 110).min(62);
            let reach = 3600 / speed16;
            let cy = (self.state.last_cy + rnd(&mut rng, -reach, reach)).clamp(140, 320);
            let wall_ok = dist > 1400;
            let mut end = cy;
            let low = cy >= 230;
            if roll < 30 {
                // Coral gap: tight squeeze with a pearl trail through it.
                let g = Self::gap_height(dist);
                let top = cy - g / 2;
                self.push(Kind::Coral, x0 + 170, TOP - 20, 70, top - (TOP - 20), 0);
                self.push(Kind::Coral, x0 + 170, cy + g / 2, 70, FLOOR + 30 - (cy + g / 2), 0);
                for k in 0..4 {
                    self.push(Kind::Pearl, x0 + 70 + k * 70, cy, 16, 16, 0);
                }
            } else if roll < 52 {
                // Pearl wave: climb and dive to keep the chain.
                let amp = rnd(&mut rng, 40, 70);
                for k in 0..8 {
                    let y = (cy + sin256(k * 20) * amp / 4096).clamp(TOP + 20, FLOOR - 20);
                    self.push(Kind::Pearl, x0 + 40 + k * 46, y, 16, 16, 0);
                    end = y;
                }
            } else if roll < 68 && wall_ok {
                // Bubble wall: inflate fully and pop it for a bonus.
                self.push(Kind::Wall, x0 + 190, TOP - 10, 36, FLOOR + 20 - (TOP - 10), 0);
                for k in 0..3 {
                    self.push(Kind::Pearl, x0 + 100 + k * 40, cy, 16, 16, 0);
                }
                for k in 0..4 {
                    self.push(Kind::Pearl, x0 + 270 + k * 40, cy, 16, 16, 0);
                }
            } else if roll < 84 {
                // Jellyfish pair bobbing together: the pearl line down the middle is always clear.
                let a = rnd(&mut rng, 0, 255);
                let mid = cy.clamp(190, 285);
                self.push(Kind::Jelly, x0 + 150, mid - 95, 34, 34, a);
                self.push(Kind::Jelly, x0 + 150, mid + 95, 34, 34, a);
                for k in 0..5 {
                    self.push(Kind::Pearl, x0 + 90 + k * 34, mid, 16, 16, 0);
                }
                end = mid;
            } else if low {
                // Urchin row on the seabed with an arc of pearls overhead.
                for k in 0..3 {
                    self.push(Kind::Urchin, x0 + 130 + k * 80, FLOOR - 16, 36, 36, 0);
                }
                for k in 0..7 {
                    let y = cy - sin256(k * 21) * 60 / 4096;
                    self.push(Kind::Pearl, x0 + 80 + k * 40, y, 16, 16, 0);
                    end = y;
                }
            } else {
                for k in 0..6 {
                    self.push(Kind::Pearl, x0 + 60 + k * 50, cy, 16, 16, 0);
                }
            }
            self.state.rng = rng;
            self.state.last_cy = end;
            self.state.gen_x += 420;
        }
        let d = self.dist();
        self.state.objs.retain(|o| o.x + o.w + 100 > d + FISH_X - 400);
    }
    fn hit(&mut self) {
        let s = &mut self.state;
        s.hearts -= 1;
        s.inv = 100;
        s.chain = 0;
        self.cues.push(1);
        if s.hearts <= 0 {
            s.phase = Phase::Sunk;
            s.timer = 0;
        }
    }
    fn finish_dive(&mut self) {
        let score = self.score();
        let before = hat_tier(self.state.records.pearls);
        let s = &mut self.state;
        let r = &mut s.records;
        r.dives += 1;
        s.new_best = score > r.best_dive;
        r.best_dive = r.best_dive.max(score);
        r.best_chain = r.best_chain.max(s.best_chain);
        r.pearls += s.pearls;
        r.walls_popped += s.popped;
        s.unlocked = hat_tier(r.pearls) > before;
    }
    fn step_dive(&mut self, i: &Intent) {
        let up = i.y < 0;
        let down = i.y > 0;
        {
            let s = &mut self.state;
            s.puff = if up {
                (s.puff + 3).min(100)
            } else if down {
                (s.puff - 5).max(0)
            } else if s.puff > 25 {
                (s.puff - 2).max(25)
            } else {
                (s.puff + 1).min(25)
            };
            // Buoyancy: puff above 45 floats up, below sinks (screen y grows downward). Units 1/256 px.
            s.vys = s.vys * 7 / 8 - (s.puff - 45) * 2;
            s.ys += s.vys;
        }
        let s = &mut self.state;
        let r = 13 + s.puff * 13 / 100;
        if s.ys < (TOP + r) << 8 {
            s.ys = (TOP + r) << 8;
            s.vys = s.vys.max(0);
        }
        if s.ys > (FLOOR - r) << 8 {
            s.ys = (FLOOR - r) << 8;
            s.vys = s.vys.min(0);
        }
        s.inv = s.inv.saturating_sub(1);
        let speed = self.speed16();
        self.state.dist16 += speed;
        self.generate();
        let d = self.dist();
        let px = d + FISH_X;
        let py = self.y();
        let rad = self.radius() * 85 / 100;
        let puff = self.state.puff;
        let tick = self.state.tick as i32;
        let mut hit = false;
        let mut events: Vec<(usize, Point)> = vec![];
        let mut missed = false;
        let mut gained = 0u32;
        let mut chain = self.state.chain;
        let mut pearls = 0;
        let mut popped = 0;
        let inv = self.state.inv > 0;
        for o in self.state.objs.iter_mut() {
            if !o.alive {
                continue;
            }
            match o.kind {
                Kind::Pearl => {
                    let dx = px - (o.x + 8);
                    let dy = py - (o.y);
                    let reach = rad + 10;
                    if dx * dx + dy * dy <= reach * reach {
                        o.alive = false;
                        chain += 1;
                        pearls += 1;
                        gained += 10 * (1 + chain / 4).min(8);
                        events.push((0, Point::new(o.x + 8 - d, o.y)));
                    } else if o.x + 16 < px - rad - 30 {
                        o.alive = false;
                        if chain > 0 {
                            missed = true;
                        }
                    }
                }
                Kind::Coral => {
                    let rect = Rect::new(o.x, o.y, o.w, o.h);
                    let me = Rect::new(px - rad, py - rad, rad * 2, rad * 2);
                    if !inv && rect.overlaps(me) {
                        hit = true;
                    }
                }
                Kind::Urchin => {
                    let (cx, cy) = (o.x + o.w / 2, o.y);
                    let reach = rad + o.w / 2 - 3;
                    if !inv && (px - cx) * (px - cx) + (py - cy) * (py - cy) <= reach * reach {
                        hit = true;
                    }
                }
                Kind::Jelly => {
                    let cy = o.y + sin256(tick * 3 + o.phase) * 44 / 4096;
                    let cx = o.x + o.w / 2;
                    let reach = rad + o.w / 2 - 2;
                    if !inv && (px - cx) * (px - cx) + (py - cy) * (py - cy) <= reach * reach {
                        hit = true;
                    }
                }
                Kind::Wall => {
                    let rect = Rect::new(o.x, o.y, o.w, o.h);
                    let me = Rect::new(px - rad, py - rad, rad * 2, rad * 2);
                    if rect.overlaps(me) {
                        if puff >= 80 {
                            o.alive = false;
                            popped += 1;
                            chain += 3;
                            gained += 100 * (1 + chain / 4).min(8);
                            events.push((2, Point::new(o.x + o.w / 2 - d, py)));
                        } else if !inv {
                            hit = true;
                        }
                    }
                }
            }
        }
        for (cue, at) in events {
            self.cues.push(cue);
            self.last_event = at;
        }
        let s = &mut self.state;
        s.chain = chain;
        s.best_chain = s.best_chain.max(chain);
        s.pearls += pearls;
        s.points += gained;
        s.popped += popped;
        if missed {
            s.chain = 0;
        }
        // A long chain heals: every 12 in a row restores a heart.
        if pearls > 0 && s.chain > 0 && s.chain % 12 == 0 && s.hearts < HEARTS {
            s.hearts += 1;
        }
        if hit {
            self.last_event = Point::new(FISH_X, py);
            self.hit();
        }
    }
}
impl GameLogic for Puff {
    const ID: &'static str = "puff-pop";
    const TITLE: &'static str = "Puff Pop";
    const CONTROLS: &'static str = "Hold UP to puff and float · release to sink · DOWN squeezes small";
    const VERIFY_TICKS: u32 = 1200;
    fn new(_: u64) -> Self {
        Self {
            state: State {
                tick: 0,
                phase: Phase::Intro,
                timer: 0,
                dist16: 0,
                ys: 220 << 8,
                vys: 0,
                puff: 25,
                hearts: HEARTS,
                inv: 0,
                chain: 0,
                best_chain: 0,
                pearls: 0,
                points: 0,
                rng: 1,
                gen_x: 520,
                last_cy: 220,
                objs: vec![],
                unlocked: false,
                new_best: false,
                popped: 0,
                records: Records::default(),
            },
            cues: vec![],
            last_event: Point::new(FISH_X, 220),
        }
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
        // Start the dive, then pulse "up" in a gentle rhythm so the dive crosses pearls and obstacles;
        // later presses move through the result card into the next dive.
        let press = tick == 6 || (tick > 600 && tick % 90 == 0);
        let pulse = tick > 20 && (tick / 38) % 2 == 0;
        Intent {
            y: if pulse { -1 } else { i32::from(tick > 20 && tick % 19 < 4) },
            action: press,
            ..Default::default()
        }
    }
    fn take_cues(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.cues)
    }
    fn cue_point(&self, _cue: usize) -> Point {
        self.last_event
    }
    fn audio_banks() -> &'static [AudioBankSpec] {
        &[AudioBankSpec { id: "music", root: "assets/audio/music", music: true }]
    }
    fn audio_level(&self, _bank: &str, layer: &str) -> f32 {
        let diving = matches!(self.state.phase, Phase::Dive);
        let chain = (self.state.chain as f32 / 12.).min(1.);
        let near = self.state.hearts == 1;
        match layer {
            "reef" => 0.55,
            "groove" => if diving { 0.55 } else { 0.25 },
            "bubbles" => if diving { 0.35 + 0.4 * chain } else { 0.3 },
            "steel" => if diving && self.state.chain >= 4 { 0.7 } else if diving { 0.0 } else { 0.35 },
            "thump" => if diving { 0.5 } else { 0.0 },
            "danger" => if diving && near { 0.7 } else { 0.0 },
            _ => 0.0,
        }
    }
    fn restart(&mut self) {
        let records = self.state.records.clone();
        *self = Self::new(7);
        self.state.records = records;
    }
}
impl Simulation for Puff {
    type Input = Intent;
    fn step(&mut self, i: &Intent) {
        self.state.tick += 1;
        match self.state.phase {
            Phase::Intro => {
                if i.action {
                    self.start_dive();
                    self.cues.push(0);
                }
            }
            Phase::Dive => self.step_dive(i),
            Phase::Sunk => {
                self.state.timer += 1;
                if self.state.timer == 1 {
                    self.finish_dive();
                }
                self.state.ys = (self.state.ys + 600).min((FLOOR - 14) << 8);
                if i.action && self.state.timer > 40 {
                    self.start_dive();
                    self.cues.push(0);
                }
            }
        }
    }
    fn state_hash(&self) -> u64 {
        hash_json(&self.state)
    }
}
impl Snapshot for Puff {
    const KIND: &'static str = "puff-pop";
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, state: State) -> Result<(), String> {
        if !(0..=100).contains(&state.puff)
            || !(0..=HEARTS).contains(&state.hearts)
            || state.objs.len() > 400
            || state.ys < 0
            || state.ys > 450 << 8
        {
            return Err("Invalid Puff Pop save".into());
        }
        self.state = state;
        self.cues.clear();
        Ok(())
    }
}
#[cfg(feature = "client")]
const PIP_PNG: &[u8] = include_bytes!("../assets/sprites/pip.png");
#[cfg(feature = "client")]
const PEARL_PNG: &[u8] = include_bytes!("../assets/sprites/pearl.png");
#[cfg(feature = "client")]
impl Puff {
    fn draw_fish(&self, s: &mut draw::Scene, cx: i32, cy: i32, puff: i32, tilt: f32, layer: i32) {
        use draw::*;
        let r = 13 + puff * 13 / 100;
        let size = (r * 2) as f32 * 1.3;
        if puff > 55 {
            // Spikes pop out as the fish swells.
            let len = ((puff - 55) / 6 + 2) as i32;
            for k in 0..12 {
                let a = k * 21;
                let sx = cx + (sin256(a + 64) * (r + 1)) / 4096;
                let sy = cy + (sin256(a) * (r + 1)) / 4096;
                let ex = cx + (sin256(a + 64) * (r + len)) / 4096;
                let ey = cy + (sin256(a) * (r + len)) / 4096;
                let (x0, x1, y0, y1) = (sx.min(ex), sx.max(ex), sy.min(ey), sy.max(ey));
                s.rect(layer - 1, Rect::new(x0 - 1, y0 - 1, x1 - x0 + 3, y1 - y0 + 3), Color::new(1., 0.86, 0.4, 1.));
            }
        }
        s.sprite_png(layer, "pip", PIP_PNG, Transform { position: [cx as f32 - size / 2., cy as f32 - size / 2.], rotation: tilt, ..Default::default() }, [size, size], WHITE);
        let top = cy - r - 2;
        match hat_tier(self.state.records.pearls) {
            1 => {
                s.rect(layer + 1, Rect::new(cx - 9, top - 6, 18, 7), WHITE);
                s.rect(layer + 1, Rect::new(cx - 9, top, 18, 3), Color::new(0.2, 0.35, 0.8, 1.));
            }
            2 => {
                for k in 0..4 {
                    s.rect(layer + 1, Rect::new(cx - 8 + k * 2, top - 3 - k * 4, 16 - k * 4, 4), if k % 2 == 0 { PINK } else { GOLD });
                }
            }
            3 => {
                s.rect(layer + 1, Rect::new(cx - 10, top - 8, 20, 9), GOLD);
                for k in 0..3 {
                    s.rect(layer + 1, Rect::new(cx - 10 + k * 8, top - 13, 4, 6), GOLD);
                }
            }
            4 => {
                s.rect(layer + 1, Rect::new(cx - 12, top - 3, 24, 4), Color::new(0.1, 0.1, 0.15, 1.));
                s.rect(layer + 1, Rect::new(cx - 8, top - 10, 16, 8), Color::new(0.1, 0.1, 0.15, 1.));
                s.rect(layer + 2, Rect::new(cx - 2, top - 8, 4, 4), WHITE);
            }
            _ => {}
        }
    }
}
#[cfg(feature = "client")]
impl draw::Game for Puff {
    fn show_hud() -> bool {
        false
    }
    fn menu_status(&self) -> String {
        format!("Dive {}  ·  best {}", self.state.records.dives + 1, self.state.records.best_dive)
    }
    fn draw(&self, s: &mut draw::Scene) {
        use draw::*;
        let st = &self.state;
        let d = self.dist();
        for k in 0..9 {
            let t = k as f32 / 8.;
            s.rect(-30, Rect::new(0, k * 51, 800, 52), Color::new(0.14 - 0.08 * t, 0.52 - 0.30 * t, 0.78 - 0.36 * t, 1.));
        }
        for k in 0..5 {
            let x = (k * 190 - (d / 6) % 190) as i32;
            s.rect(-29, Rect::new(x, 0, 34, 340), Color::new(1., 1., 1., 0.04));
        }
        for k in 0..14 {
            let x = k * 70 - (d / 3) % 70;
            let h = 40 + ((k * 37) % 5) * 14;
            s.rect(-20, Rect::new(x, FLOOR + 12 - h, 8, h), Color::new(0.1, 0.38, 0.30, 1.));
            s.rect(-20, Rect::new(x + 8, FLOOR + 12 - h + 12, 6, h - 12), Color::new(0.14, 0.46, 0.34, 1.));
        }
        s.rect(-10, Rect::new(0, FLOOR + 10, 800, 40), Color::new(0.72, 0.64, 0.44, 1.));
        s.rect(-9, Rect::new(0, FLOOR + 10, 800, 5), Color::new(0.86, 0.78, 0.55, 1.));
        for k in 0..8 {
            let t = st.tick as i32;
            let bx = (k * 113 + 40) % 800;
            let by = 420 - ((t * (1 + k % 3) + k * 90) % 400);
            s.circle(-15, Point::new(bx, by), 3. + (k % 3) as f32, Color::new(1., 1., 1., 0.22));
        }
        for o in st.objs.iter().filter(|o| o.alive) {
            let x = o.x - d;
            if x < -120 || x > 920 {
                continue;
            }
            match o.kind {
                Kind::Pearl => {
                    let bob = sin256(st.tick as i32 * 4 + o.x) * 3 / 4096;
                    s.sprite_png(5, "pearl", PEARL_PNG, Transform { position: [x as f32, (o.y - 8 + bob) as f32], ..Default::default() }, [16., 16.], WHITE);
                }
                Kind::Coral => {
                    s.rect(3, Rect::new(x, o.y, o.w, o.h), Color::new(0.93, 0.45, 0.45, 1.));
                    s.rect(4, Rect::new(x, o.y, 8, o.h), Color::new(1., 0.65, 0.6, 1.));
                    let lip = if o.y <= TOP { o.y + o.h - 10 } else { o.y };
                    s.rect(4, Rect::new(x - 8, lip, o.w + 16, 10), Color::new(0.82, 0.32, 0.38, 1.));
                }
                Kind::Urchin => {
                    let (cx, cy) = (x + o.w / 2, o.y);
                    for k in 0..12 {
                        let a = k * 21;
                        let ex = cx + sin256(a + 64) * (o.w / 2 + 8) / 4096;
                        let ey = cy + sin256(a) * (o.w / 2 + 8) / 4096;
                        s.rect(3, Rect::new(ex.min(cx) - 1, ey.min(cy) - 1, (ex - cx).abs() + 3, (ey - cy).abs() + 3), Color::new(0.35, 0.2, 0.5, 1.));
                    }
                    s.circle(4, Point::new(cx, cy), (o.w / 2) as f32, Color::new(0.22, 0.12, 0.35, 1.));
                    s.circle(5, Point::new(cx - 4, cy - 4), 3., Color::new(1., 0.4, 0.5, 1.));
                }
                Kind::Jelly => {
                    let cy = self.jelly_y(o);
                    let cx = x + o.w / 2;
                    for k in 0..4 {
                        let sway = sin256(st.tick as i32 * 6 + k * 40) * 4 / 4096;
                        s.rect(3, Rect::new(cx - 14 + k * 9 + sway, cy + 4, 3, 22), Color::new(0.85, 0.6, 0.95, 0.8));
                    }
                    s.circle(4, Point::new(cx, cy), (o.w / 2) as f32, Color::new(0.82, 0.55, 0.95, 0.92));
                    s.circle(5, Point::new(cx - 5, cy - 6), 4., Color::new(1., 1., 1., 0.6));
                }
                Kind::Wall => {
                    s.rect(3, Rect::new(x, o.y, o.w, o.h), Color::new(0.8, 0.95, 1., 0.28));
                    for k in 0..(o.h / 36) {
                        s.circle(4, Point::new(x + o.w / 2, o.y + 18 + k * 36), 17., Color::new(0.85, 0.97, 1., 0.55));
                        s.circle(5, Point::new(x + o.w / 2 - 6, o.y + 12 + k * 36), 4., WHITE);
                    }
                    if x < 520 && x > FISH_X {
                        s.text(20, "POP! (puff full)", Point::new(x - 40, 70), 16., Color::new(1., 1., 1., 0.9));
                    }
                }
            }
        }
        if st.phase != Phase::Intro {
            let blink = st.inv > 0 && (st.inv / 4) % 2 == 0;
            if !blink {
                let tilt = (st.vys as f32 / 256.) * 0.08;
                self.draw_fish(s, FISH_X, self.y(), st.puff, if st.phase == Phase::Sunk { 2.8 } else { tilt }, 10);
            }
            s.text(20, format!("score {}", self.score()), Point::new(330, 30), 24., Color::new(0.4, 0.9, 0.85, 1.));
            for h in 0..HEARTS {
                s.circle(20, Point::new(700 + h * 26, 24), 9., if h < st.hearts { PINK } else { Color::new(1., 1., 1., 0.18) });
            }
            s.text(20, format!("chain {}  x{}   best {}", st.chain, self.multiplier(), st.records.best_dive), Point::new(330, 52), 16., GOLD);
            s.rect(20, Rect::new(24, 20, 100, 8), Color::new(0., 0., 0., 0.4));
            s.rect(21, Rect::new(25, 21, st.puff.min(100) * 98 / 100, 6), if st.puff >= 80 { PINK } else { TEAL });
            s.text(20, "puff", Point::new(130, 29), 12., WHITE);
            if st.phase == Phase::Dive && st.chain >= 4 {
                s.text(20, format!("{} IN A ROW!", st.chain), Point::new(FISH_X - 40, self.y() - 48), 18., GOLD);
            }
            if st.phase == Phase::Dive && d < 180 {
                s.text(25, "Hold UP to puff and float", Point::new(300, 400), 18., Color::new(1., 1., 1., 0.85));
            }
        }
        let dim = Color::new(0.02, 0.07, 0.14, 0.78);
        match st.phase {
            Phase::Intro => {
                s.rect(30, Rect::new(70, 60, 660, 340), dim);
                s.text(31, "PUFF POP", Point::new(290, 112), 46., GOLD);
                s.text(31, "Hold UP to puff and float. Let go to sink. DOWN squeezes small.", Point::new(130, 146), 19., WHITE);
                s.text(31, "Grab pearls in a chain. Pop bubble walls while fully puffed.", Point::new(150, 172), 18., TEAL);
                self.draw_fish(s, 400, 240, 60, 0., 32);
                s.text(31, "Press to dive", Point::new(335, 300), 20., TEAL);
                let r = &st.records;
                s.text(31, format!("Dives {}   Best {}   Best chain {}   Walls popped {}", r.dives, r.best_dive, r.best_chain, r.walls_popped), Point::new(130, 340), 17., TEAL);
                let next = HAT_TIERS.iter().find(|&&t| r.pearls < t).map(|t| format!("  ·  next hat at {} pearls", t)).unwrap_or_default();
                s.text(31, format!("Pearls {}  ·  hat: {}{}", r.pearls, HAT_NAMES[hat_tier(r.pearls)], next), Point::new(130, 366), 17., GOLD);
            }
            Phase::Sunk if st.timer > 20 => {
                s.rect(30, Rect::new(150, 100, 500, 240), dim);
                s.text(31, "POPPED!", Point::new(190, 150), 34., GOLD);
                s.text(31, format!("Score {}", self.score()), Point::new(190, 190), 26., Color::new(0.4, 0.9, 0.85, 1.));
                s.text(31, format!("Pearls {}  ·  best chain {}  ·  walls popped {}", st.pearls, st.best_chain, st.popped), Point::new(190, 224), 17., WHITE);
                if st.new_best {
                    s.text(31, "NEW BEST DIVE!", Point::new(190, 258), 22., PINK);
                } else {
                    s.text(31, format!("Best {}", st.records.best_dive), Point::new(190, 258), 18., TEAL);
                }
                if st.unlocked {
                    s.text(31, format!("NEW HAT: {}!", HAT_NAMES[hat_tier(st.records.pearls)]), Point::new(190, 288), 20., PINK);
                }
                s.text(31, "Press to dive again", Point::new(190, 320), 16., TEAL);
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
    fn up() -> Intent {
        Intent { y: -1, ..Default::default() }
    }
    fn begin(g: &mut Puff) {
        g.step(&press());
        assert_eq!(g.state.phase, Phase::Dive);
    }
    /// Greedy pilot: chase the next pearl, puff up for walls. Proves every pattern is flyable.
    fn pilot(g: &Puff) -> Intent {
        let px = g.dist() + FISH_X;
        let wall = g.state.objs.iter().find(|o| o.alive && o.kind == Kind::Wall && o.x + o.w > px - 30 && o.x - px < 170);
        if wall.is_some() {
            return up();
        }
        let target = g
            .state
            .objs
            .iter()
            .filter(|o| o.alive && o.kind == Kind::Pearl && o.x + 8 > px - 10)
            .min_by_key(|o| o.x)
            .map_or(220, |o| o.y);
        let y = g.y();
        let desired = ((target - y) * 64).clamp(-800, 700);
        let wanted = 45 - (desired - g.state.vys * 7 / 8) / 2;
        if wanted > g.state.puff + 1 {
            up()
        } else if wanted < g.state.puff - 1 {
            Intent { y: 1, ..Default::default() }
        } else {
            Intent::default()
        }
    }
    #[test]
    fn verification_route_is_deterministic_and_resumes() {
        let inputs: Vec<Intent> = (0..Puff::VERIFY_TICKS).map(Puff::verification_input).collect();
        vesper3d::runtime::assert_deterministic(|| Puff::new(7), &inputs);
        snapshot::assert_resumes_exactly(|| Puff::new(7), &inputs, 131);
        let (hash, outcome) = verify::<Puff>();
        assert_eq!(outcome, "playing");
        let mut g = Puff::new(7);
        let mut flew = 0;
        for i in &inputs {
            g.step(i);
            flew = flew.max(g.dist());
        }
        assert!(flew > 600, "the route actually dives and scrolls the reef");
        if let Ok(path) = std::env::var("BE2_VERIFY_REPORT") {
            std::fs::write(
                path,
                serde_json::json!({"hash":format!("{hash:016x}"),"outcome":outcome,"ticks":Puff::VERIFY_TICKS,
                    "purpose":"Dives from the title card and pulses the inflate control through pearls and coral until the dive ends"}).to_string(),
            )
            .unwrap();
        }
    }
    #[test]
    fn inflating_floats_releasing_sinks_and_squeezing_shrinks() {
        let mut g = Puff::new(7);
        begin(&mut g);
        let y0 = g.y();
        for _ in 0..70 {
            g.step(&Intent::default());
        }
        assert!(g.y() > y0, "a relaxed fish sinks");
        for _ in 0..80 {
            g.step(&up());
        }
        assert_eq!(g.state.puff, 100);
        for _ in 0..40 {
            g.step(&up());
        }
        assert!(g.y() < TOP + 40, "a fully puffed fish floats to the surface");
        assert!(g.y() >= TOP + g.radius(), "never above the surface");
        for _ in 0..40 {
            g.step(&Intent { y: 1, ..Default::default() });
        }
        assert_eq!(g.state.puff, 0);
        assert!(g.radius() < 14);
    }
    #[test]
    fn pearls_chain_into_multipliers_and_walls_pop_only_when_puffed() {
        let mut g = Puff::new(7);
        begin(&mut g);
        g.state.objs.clear();
        let px = g.dist() + FISH_X;
        let y = g.y();
        for k in 0..5 {
            g.push(Kind::Pearl, px - 8 + k * 3, y, 16, 16, 0);
        }
        g.step(&Intent::default());
        assert_eq!(g.state.pearls, 5);
        assert_eq!(g.state.chain, 5);
        assert!(g.state.points >= 10 * 5, "points grow with the chain: {}", g.state.points);
        assert_eq!(g.multiplier(), 2);
        let mut small = Puff::new(7);
        begin(&mut small);
        small.state.objs.clear();
        let px = small.dist() + FISH_X;
        small.push(Kind::Wall, px + 10, TOP - 10, 36, 400, 0);
        for _ in 0..12 {
            small.step(&Intent::default());
        }
        assert_eq!(HEARTS - small.state.hearts, 1, "an unpuffed fish bumps the wall and loses a heart");
        let mut big = Puff::new(7);
        begin(&mut big);
        big.state.objs.clear();
        big.state.puff = 90;
        let px = big.dist() + FISH_X;
        big.push(Kind::Wall, px + 10, TOP - 10, 36, 400, 0);
        for _ in 0..12 {
            big.step(&up());
        }
        assert_eq!(big.state.hearts, HEARTS, "a puffed fish pops it without harm");
        assert_eq!(big.state.popped, 1);
        assert!(big.state.points >= 100);
    }
    #[test]
    fn missing_a_pearl_or_taking_a_hit_breaks_the_chain() {
        let mut g = Puff::new(7);
        begin(&mut g);
        g.state.objs.clear();
        g.state.chain = 6;
        let px = g.dist() + FISH_X;
        g.push(Kind::Pearl, px - 100, 100, 16, 16, 0);
        g.step(&Intent::default());
        assert_eq!(g.state.chain, 0, "a pearl left behind ends the chain");
        g.state.chain = 6;
        g.push(Kind::Urchin, g.dist() + FISH_X, g.y(), 36, 36, 0);
        g.step(&Intent::default());
        assert_eq!(g.state.hearts, HEARTS - 1);
        assert_eq!(g.state.chain, 0);
        assert!(g.state.inv > 0);
    }
    fn sink(g: &mut Puff) {
        for _ in 0..HEARTS {
            g.state.inv = 0;
            g.state.objs.clear();
            let (x, y) = (g.dist() + FISH_X, g.y());
            g.push(Kind::Urchin, x, y, 36, 36, 0);
            g.step(&Intent::default());
        }
        assert_eq!(g.state.phase, Phase::Sunk);
    }
    #[test]
    fn dives_record_bests_and_pearls_unlock_hats_that_survive_restart() {
        let mut g = Puff::new(7);
        begin(&mut g);
        sink(&mut g);
        g.state.pearls = 45;
        g.state.points = 500;
        g.step(&Intent::default());
        assert_eq!(g.state.records.dives, 1);
        assert_eq!(g.state.records.pearls, 45);
        assert!(g.state.unlocked, "45 lifetime pearls earn the first hat");
        assert!(g.state.new_best && g.state.records.best_dive >= 500);
        for _ in 0..50 {
            g.step(&Intent::default());
        }
        g.step(&press());
        assert_eq!(g.state.phase, Phase::Dive, "one press starts the next dive");
        assert_eq!(g.state.hearts, HEARTS);
        assert_eq!(g.state.points, 0);
        sink(&mut g);
        g.step(&Intent::default());
        assert!(!g.state.new_best, "a worse dive is not a new best");
        assert_eq!(g.state.records.dives, 2);
        let records = g.state.records.clone();
        g.restart();
        assert_eq!(g.state.records, records, "restart keeps records and hats");
        assert_eq!(g.state.phase, Phase::Intro);
        assert_eq!(hat_tier(45), 1);
        assert_eq!(hat_tier(900), 4);
    }
    #[test]
    fn every_generated_pattern_is_flyable_by_a_simple_pilot() {
        for seed in 0..6u32 {
            let mut g = Puff::new(7);
            for _ in 0..seed * 17 {
                g.step(&Intent::default());
            }
            begin(&mut g);
            let mut early_hits = 0;
            for _ in 0..4200 {
                let before = g.state.hearts;
                let i = pilot(&g);
                g.step(&i);
                if g.state.hearts < before {
                    early_hits += i32::from(g.dist() < 8000);
                    g.state.hearts = HEARTS;
                    g.state.phase = Phase::Dive;
                }
            }
            assert!(g.dist() > 8000, "seed {seed}: scrolled {}", g.dist());
            assert_eq!(early_hits, 0, "seed {seed}: the pilot was hit before the gaps tighten");
            assert!(g.state.pearls >= 40, "seed {seed}: only {} pearls", g.state.pearls);
        }
    }
    #[test]
    fn saves_reject_nonsense() {
        let mut g = Puff::new(7);
        let mut bad = g.capture();
        bad.puff = 400;
        assert!(g.restore(bad).is_err());
        assert!(!g.probe_success());
        g.step(&Puff::probe_input());
        assert!(g.probe_success());
    }
}
