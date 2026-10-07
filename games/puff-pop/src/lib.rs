//! Puff Pop: two pufferfish take turns diving through a reef.
//!
//! Hold up (stick, D-pad, W or the arrow key) to inflate and float; let go to sink; hold down to
//! squeeze small. Thread the coral gaps, grab pearls in a chain and pop the bubble walls while fully
//! puffed. Three dives each, then the controller passes. Integer rules, rendering-free.
use serde::{Deserialize, Serialize};
#[cfg(test)]
use vesper3d::runtime::snapshot;
use vesper3d::two_d::*;

pub const DIVES: usize = 3;
pub const NAMES: [&str; 2] = ["Pip", "Poppy"];
pub const TOP: i32 = 52;
pub const FLOOR: i32 = 398;
pub const PLAYER_X: i32 = 200;
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
    Handoff,
    Dive,
    Sunk,
    Results,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Records {
    pub matches: u32,
    pub wins: [u32; 2],
    pub best_dive: [u32; 2],
    pub best_chain: [u32; 2],
    /// Pearls collected over every dive; unlocks the hats.
    pub pearls: [u32; 2],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub tick: u32,
    pub phase: Phase,
    pub timer: u32,
    pub first: u8,
    pub dive: u8,
    pub scores: [[u32; DIVES]; 2],
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
    pub fn player(&self) -> usize {
        (self.state.first as usize + self.state.dive as usize) % 2
    }
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
    pub fn total(&self, p: usize) -> u32 {
        self.state.scores[p].iter().sum()
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
        s.rng = 0xC0FFEE ^ (u64::from(s.dive) * 7919) ^ (u64::from(s.records.matches) << 20) ^ u64::from(s.tick);
        s.phase = Phase::Handoff;
        self.generate();
    }
    fn push(&mut self, kind: Kind, x: i32, y: i32, w: i32, h: i32, phase: i32) {
        self.state.objs.push(Obj { kind, x, y, w, h, phase, alive: true });
    }
    /// Lay out the next 420-pixel stretch of reef. Every pattern leaves a flyable path.
    fn generate(&mut self) {
        while self.state.gen_x < self.dist() + PLAYER_X + 1500 {
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
        self.state.objs.retain(|o| o.x + o.w + 100 > d + PLAYER_X - 400);
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
        let p = self.player();
        let d = self.state.dive as usize;
        let score = self.score();
        let before = hat_tier(self.state.records.pearls[p]);
        let s = &mut self.state;
        s.scores[p][d / 2] = score;
        let r = &mut s.records;
        r.best_dive[p] = r.best_dive[p].max(score);
        r.best_chain[p] = r.best_chain[p].max(s.best_chain);
        r.pearls[p] += s.pearls;
        s.unlocked = hat_tier(r.pearls[p]) > before;
    }
    fn after_result(&mut self) {
        if (self.state.dive as usize) + 1 < DIVES * 2 {
            self.state.dive += 1;
            self.start_dive();
        } else {
            let t = [self.total(0), self.total(1)];
            let r = &mut self.state.records;
            r.matches += 1;
            if t[0] != t[1] {
                r.wins[usize::from(t[1] > t[0])] += 1;
            }
            self.state.phase = Phase::Results;
            self.state.timer = 0;
        }
    }
    fn new_match(&mut self) {
        self.state.scores = [[0; DIVES]; 2];
        self.state.dive = 0;
        self.start_dive();
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
        let px = d + PLAYER_X;
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
            self.last_event = Point::new(PLAYER_X, py);
            self.hit();
        }
    }
}
impl GameLogic for Puff {
    const ID: &'static str = "puff-pop";
    const TITLE: &'static str = "Puff Pop";
    const CONTROLS: &'static str = "Hold UP to puff and float · release to sink · DOWN squeezes small · A/Space confirms";
    const VERIFY_TICKS: u32 = 1200;
    fn new(_: u64) -> Self {
        Self {
            state: State {
                tick: 0,
                phase: Phase::Intro,
                timer: 0,
                first: 0,
                dive: 0,
                scores: [[0; DIVES]; 2],
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
                popped: 0,
                records: Records::default(),
            },
            cues: vec![],
            last_event: Point::new(PLAYER_X, 220),
        }
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
        // Pick Poppy to dive first, confirm, then pulse "up" in a gentle rhythm so the dive crosses
        // pearls and obstacles; later presses move through the result cards.
        let press = matches!(tick, 6 | 30 | 60);
        let pulse = tick > 70 && (tick / 38) % 2 == 0;
        Intent {
            x: i32::from((3..12).contains(&tick)),
            y: if pulse { -1 } else { i32::from(tick > 70 && tick % 19 < 4) },
            action: press || (tick > 600 && tick % 90 == 0),
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
                if i.x != 0 {
                    self.state.first = u8::from(i.x > 0);
                }
                if i.action {
                    self.new_match();
                    self.cues.push(0);
                }
            }
            Phase::Handoff => {
                self.state.timer += 1;
                if i.action {
                    self.state.phase = Phase::Dive;
                    self.state.timer = 0;
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
                    self.after_result();
                }
            }
            Phase::Results => {
                self.state.timer += 1;
                if i.action && self.state.timer > 30 {
                    self.state.first = 1 - self.state.first;
                    self.new_match();
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
        if state.first > 1
            || state.dive as usize >= DIVES * 2
            || !(0..=100).contains(&state.puff)
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
const POPPY_PNG: &[u8] = include_bytes!("../assets/sprites/poppy.png");
#[cfg(feature = "client")]
const PEARL_PNG: &[u8] = include_bytes!("../assets/sprites/pearl.png");
#[cfg(feature = "client")]
impl Puff {
    fn draw_fish(&self, s: &mut draw::Scene, who: usize, cx: i32, cy: i32, puff: i32, tilt: f32, layer: i32) {
        use draw::*;
        let r = 13 + puff * 13 / 100;
        let size = (r * 2) as f32 * 1.3;
        let (id, png) = if who == 0 { ("pip", PIP_PNG) } else { ("poppy", POPPY_PNG) };
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
        s.sprite_png(layer, id, png, Transform { position: [cx as f32 - size / 2., cy as f32 - size / 2.], rotation: tilt, ..Default::default() }, [size, size], WHITE);
        let tier = hat_tier(self.state.records.pearls[who]);
        let top = cy - r - 2;
        match tier {
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
        format!("Dive {} of {}  ·  Pip {}  Poppy {}", self.state.dive + 1, DIVES * 2, self.total(0), self.total(1))
    }
    fn draw(&self, s: &mut draw::Scene) {
        use draw::*;
        let st = &self.state;
        let d = self.dist();
        let tints = [Color::new(0.4, 0.9, 0.85, 1.), Color::new(1., 0.6, 0.78, 1.)];
        // Water: depth bands, drifting light shafts, parallax seaweed and bubbles.
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
                    let c = Color::new(0.93, 0.45, 0.45, 1.);
                    s.rect(3, Rect::new(x, o.y, o.w, o.h), c);
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
                    if x < 520 && x > PLAYER_X {
                        s.text(20, "POP! (puff full)", Point::new(x - 40, 70), 16., Color::new(1., 1., 1., 0.9));
                    }
                }
            }
        }
        // The fish.
        let showing = matches!(st.phase, Phase::Handoff | Phase::Dive | Phase::Sunk);
        if showing {
            let blink = st.inv > 0 && (st.inv / 4) % 2 == 0;
            if !blink {
                let tilt = (st.vys as f32 / 256.) * 0.08;
                let who = self.player();
                let flip = st.phase == Phase::Sunk;
                self.draw_fish(s, who, PLAYER_X, self.y(), st.puff, if flip { 2.8 } else { tilt }, 10);
            }
            // HUD.
            let who = self.player();
            s.text(20, format!("{}  score {}", NAMES[who], self.score()), Point::new(300, 30), 22., tints[who]);
            for h in 0..HEARTS {
                s.circle(20, Point::new(620 + h * 26, 24), 9., if h < st.hearts { PINK } else { Color::new(1., 1., 1., 0.18) });
            }
            let mult = self.multiplier();
            s.text(20, format!("chain {}  x{}", st.chain, mult), Point::new(300, 52), 16., GOLD);
            s.rect(20, Rect::new(24, 52, 100, 8), Color::new(0., 0., 0., 0.4));
            s.rect(21, Rect::new(25, 53, st.puff.min(100) * 98 / 100, 6), if st.puff >= 80 { PINK } else { TEAL });
            s.text(20, "puff", Point::new(130, 61), 12., WHITE);
            if st.phase == Phase::Dive && st.chain >= 4 {
                s.text(20, format!("{} IN A ROW!", st.chain), Point::new(PLAYER_X - 40, self.y() - 48), 18., GOLD);
            }
        }
        let dim = Color::new(0.02, 0.07, 0.14, 0.78);
        match st.phase {
            Phase::Intro => {
                s.rect(30, Rect::new(70, 60, 660, 340), dim);
                s.text(31, "PUFF POP", Point::new(290, 112), 46., GOLD);
                s.text(31, "Hold UP to puff and float. Let go to sink.", Point::new(210, 146), 20., WHITE);
                s.text(31, "Pop bubble walls while fully puffed. Keep the chain alive!", Point::new(160, 172), 18., TEAL);
                s.text(31, "Who dives first?  (left / right, then press)", Point::new(220, 204), 18., TEAL);
                for p in 0..2usize {
                    let x = 260 + p as i32 * 260;
                    if st.first as usize == p {
                        s.rect(31, Rect::new(x - 50, 215, 120, 120), Color::new(1., 1., 1., 0.12));
                    }
                    self.draw_fish(s, p, x + 10, 270, if st.first as usize == p { 70 } else { 20 }, 0., 32);
                    s.text(32, NAMES[p], Point::new(x - 20, 330), 22., if st.first as usize == p { tints[p] } else { WHITE });
                }
                let r = &st.records;
                s.text(31, format!("Matches {}  Wins {}-{}  Best dive {} / {}  Best chain {} / {}", r.matches, r.wins[0], r.wins[1], r.best_dive[0], r.best_dive[1], r.best_chain[0], r.best_chain[1]), Point::new(100, 360), 16., TEAL);
                s.text(31, format!("Pearls {} / {}  ·  hats: {} / {}", r.pearls[0], r.pearls[1], HAT_NAMES[hat_tier(r.pearls[0])], HAT_NAMES[hat_tier(r.pearls[1])]), Point::new(100, 384), 16., GOLD);
            }
            Phase::Handoff => {
                s.rect(30, Rect::new(150, 110, 500, 210), dim);
                let p = self.player();
                s.text(31, format!("Pass the controller to {}", NAMES[p]), Point::new(190, 160), 26., tints[p]);
                s.text(31, format!("Dive {} of {}.  Press when ready.", st.dive + 1, DIVES * 2), Point::new(190, 200), 20., WHITE);
                s.text(31, format!("Pip {}  ·  Poppy {}", self.total(0), self.total(1)), Point::new(190, 240), 20., GOLD);
                s.text(31, "Three hearts. Hold UP to puff, DOWN to squeeze.", Point::new(190, 290), 16., TEAL);
            }
            Phase::Sunk if st.timer > 20 => {
                s.rect(30, Rect::new(150, 100, 500, 240), dim);
                let p = self.player();
                s.text(31, "POPPED!", Point::new(190, 150), 34., GOLD);
                s.text(31, format!("{} scored {}", NAMES[p], self.score()), Point::new(190, 190), 24., tints[p]);
                s.text(31, format!("Pearls {}  ·  best chain {}  ·  walls popped {}", st.pearls, st.best_chain, st.popped), Point::new(190, 224), 17., WHITE);
                if st.unlocked {
                    s.text(31, format!("NEW HAT: {}!", HAT_NAMES[hat_tier(st.records.pearls[p])]), Point::new(190, 260), 22., PINK);
                }
                s.text(31, "Press to continue", Point::new(190, 310), 16., TEAL);
            }
            Phase::Results => {
                s.rect(30, Rect::new(90, 70, 620, 320), dim);
                let t = [self.total(0), self.total(1)];
                let title = if t[0] == t[1] { "A TIE!".to_string() } else { format!("{} WINS!", NAMES[usize::from(t[1] > t[0])]) };
                s.text(31, title, Point::new(140, 125), 38., GOLD);
                for p in 0..2usize {
                    let row: Vec<String> = st.scores[p].iter().map(|v| v.to_string()).collect();
                    s.text(31, format!("{:<6} {:>6}   ({})", NAMES[p], t[p], row.join(" + ")), Point::new(140, 175 + p as i32 * 32), 22., tints[p]);
                }
                let r = &st.records;
                s.text(31, format!("Wins {}-{}   Best dive {} / {}   Hats {} / {}", r.wins[0], r.wins[1], r.best_dive[0], r.best_dive[1], HAT_NAMES[hat_tier(r.pearls[0])], HAT_NAMES[hat_tier(r.pearls[1])]), Point::new(140, 260), 17., TEAL);
                s.text(31, "Press for a rematch (the other fish dives first)", Point::new(140, 340), 16., WHITE);
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
        assert_eq!(g.state.phase, Phase::Handoff);
        g.step(&press());
        assert_eq!(g.state.phase, Phase::Dive);
    }
    /// Greedy pilot: chase the next pearl, puff up for walls. Proves every pattern is flyable.
    fn pilot(g: &Puff) -> Intent {
        let px = g.dist() + PLAYER_X;
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
        assert_eq!(g.state.first, 1, "the route picks Poppy to dive first");
        assert!(flew > 600, "the route actually dives and scrolls the reef");
        if let Ok(path) = std::env::var("BE2_VERIFY_REPORT") {
            std::fs::write(
                path,
                serde_json::json!({"hash":format!("{hash:016x}"),"outcome":outcome,"ticks":Puff::VERIFY_TICKS,
                    "purpose":"Hot-seat dive: picks Poppy, confirms the handoff, pulses the inflate control through pearls and coral until the dive ends"}).to_string(),
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
        let px = g.dist() + PLAYER_X;
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
        let px = small.dist() + PLAYER_X;
        small.push(Kind::Wall, px + 10, TOP - 10, 36, 400, 0);
        let mut hits = 0;
        for _ in 0..12 {
            small.step(&Intent::default());
            hits = HEARTS - small.state.hearts;
        }
        assert_eq!(hits, 1, "an unpuffed fish bumps the wall and loses a heart");
        let mut big = Puff::new(7);
        begin(&mut big);
        big.state.objs.clear();
        big.state.puff = 90;
        let px = big.dist() + PLAYER_X;
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
        let px = g.dist() + PLAYER_X;
        g.push(Kind::Pearl, px - 100, 100, 16, 16, 0);
        g.step(&Intent::default());
        assert_eq!(g.state.chain, 0, "a pearl left behind ends the chain");
        g.state.chain = 6;
        g.push(Kind::Urchin, g.dist() + PLAYER_X, g.y(), 36, 36, 0);
        g.step(&Intent::default());
        assert_eq!(g.state.hearts, HEARTS - 1);
        assert_eq!(g.state.chain, 0);
        assert!(g.state.inv > 0);
    }
    #[test]
    fn three_hits_end_the_dive_and_a_match_records_progress() {
        let mut g = Puff::new(7);
        begin(&mut g);
        for _ in 0..HEARTS {
            g.state.inv = 0;
            g.state.objs.clear();
            let (x, y) = (g.dist() + PLAYER_X, g.y());
            g.push(Kind::Urchin, x, y, 36, 36, 0);
            g.step(&Intent::default());
        }
        assert_eq!(g.state.phase, Phase::Sunk);
        g.state.pearls = 45;
        g.state.points = 500;
        g.step(&Intent::default());
        assert_eq!(g.state.records.pearls[g.player()], 45);
        assert!(g.state.unlocked, "45 lifetime pearls earn the first hat");
        assert!(g.state.records.best_dive[g.player()] >= 500);
        // Finish the whole match with instant sinks.
        let mut guard = 0;
        while g.state.phase != Phase::Results && guard < 20 {
            guard += 1;
            for _ in 0..50 {
                g.step(&Intent::default());
            }
            g.step(&press());
            if g.state.phase == Phase::Handoff {
                g.step(&press());
                g.state.hearts = 1;
                g.state.inv = 0;
                g.state.objs.clear();
                let (x, y) = (g.dist() + PLAYER_X, g.y());
                g.push(Kind::Urchin, x, y, 36, 36, 0);
                g.step(&Intent::default());
            }
        }
        assert_eq!(g.state.phase, Phase::Results);
        assert_eq!(g.state.records.matches, 1);
        let records = g.state.records.clone();
        g.restart();
        assert_eq!(g.state.records, records, "restart keeps records and hats");
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
            g.state.hearts = HEARTS;
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
        let mut bad = g.capture();
        bad.dive = 9;
        assert!(g.restore(bad).is_err());
        assert!(!g.probe_success());
        g.step(&Intent { x: 1, ..Default::default() });
        assert!(g.probe_success());
    }
}
