//! Topsy-Turvy: two bats race a cave, flipping gravity to dodge spikes.
//!
//! The bat runs on its own. Press the action button to flip between floor and ceiling, or press up or
//! down to pick a side. One hit ends the run. Flipping right in front of a spike is a "close shave" and
//! builds the heat multiplier. Three runs each, the best run counts, and the controller passes after
//! every run. Integer rules, rendering-free.
use serde::{Deserialize, Serialize};
#[cfg(test)]
use vesper3d::runtime::snapshot;
use vesper3d::two_d::*;

pub const RUNS: usize = 3;
pub const NAMES: [&str; 2] = ["Dusk", "Dawn"];
pub const CEILING: i32 = 70;
pub const FLOOR: i32 = 380;
pub const BAT: i32 = 20;
pub const BAT_X: i32 = 160;
pub const BIOME_LENGTH: i32 = 2600;
pub const BIOME_NAMES: [&str; 4] = ["Glimmer Grotto", "Mossy Hollow", "Ember Deep", "Starlit Vault"];

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
    /// A row of spikes growing from the floor (`up`) or hanging from the ceiling.
    Spikes { floor: bool },
    /// A solid pillar from the floor or ceiling.
    Pillar { floor: bool },
    Gem,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Obj {
    pub kind: Kind,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub alive: bool,
}
impl Obj {
    /// The rectangle that hurts. Spikes are forgiving: narrower than they look.
    pub fn hurt(&self) -> Option<Rect> {
        match self.kind {
            Kind::Spikes { floor } => Some(if floor {
                Rect::new(self.x + 5, self.y + 8, self.w - 10, self.h - 8)
            } else {
                Rect::new(self.x + 5, self.y, self.w - 10, self.h - 8)
            }),
            Kind::Pillar { .. } => Some(Rect::new(self.x, self.y, self.w, self.h)),
            Kind::Gem => None,
        }
    }
    pub fn floor_side(&self) -> bool {
        match self.kind {
            Kind::Spikes { floor } | Kind::Pillar { floor } => floor,
            Kind::Gem => false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Intro,
    Handoff,
    Run,
    Crashed,
    Results,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Records {
    pub matches: u32,
    pub wins: [u32; 2],
    pub best_run: [u32; 2],
    pub best_distance: [u32; 2],
    pub gems: [u32; 2],
    pub shaves: [u32; 2],
    /// Deepest biome reached, 0-based, over all runs by that bat.
    pub deepest: [u32; 2],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub tick: u32,
    pub phase: Phase,
    pub timer: u32,
    pub first: u8,
    pub run: u8,
    pub scores: [[u32; RUNS]; 2],
    pub dist16: i32,
    pub ys: i32,
    pub vys: i32,
    /// -1 gravity pulls up to the ceiling, +1 down to the floor.
    pub side: i32,
    pub heat: i32,
    pub heat_timer: u32,
    pub gems: u32,
    pub shaves: u32,
    pub flips: u32,
    pub points: u32,
    pub banner: u32,
    pub new_biome: bool,
    pub rng: u64,
    pub gen_x: i32,
    pub objs: Vec<Obj>,
    pub records: Records,
}
pub struct Turvy {
    pub state: State,
    cues: Vec<usize>,
    last_event: Point,
}
impl Turvy {
    pub fn player(&self) -> usize {
        (self.state.first as usize + self.state.run as usize) % 2
    }
    pub fn dist(&self) -> i32 {
        self.state.dist16 >> 4
    }
    pub fn y(&self) -> i32 {
        self.state.ys >> 8
    }
    pub fn biome(dist: i32) -> usize {
        ((dist / BIOME_LENGTH) as usize).min(BIOME_NAMES.len() * 4) % BIOME_NAMES.len()
    }
    pub fn score(&self) -> u32 {
        (self.dist().max(0) as u32) / 10 + self.state.points
    }
    pub fn total(&self, p: usize) -> u32 {
        self.state.scores[p].iter().copied().max().unwrap_or(0)
    }
    pub fn multiplier(&self) -> u32 {
        1 + self.state.heat as u32
    }
    pub fn grounded(&self) -> bool {
        self.y() <= CEILING || self.y() >= FLOOR - BAT
    }
    fn speed16(&self) -> i32 {
        (46 + self.dist() / 90).min(88)
    }
    fn bat_rect(&self) -> Rect {
        Rect::new(BAT_X + 2, self.y() + 2, BAT - 4, BAT - 4)
    }
    fn start_run(&mut self) {
        let s = &mut self.state;
        s.dist16 = 0;
        s.ys = (FLOOR - BAT) << 8;
        s.vys = 0;
        s.side = 1;
        s.heat = 0;
        s.heat_timer = 0;
        s.gems = 0;
        s.shaves = 0;
        s.flips = 0;
        s.points = 0;
        s.banner = 0;
        s.new_biome = false;
        s.gen_x = 760;
        s.objs.clear();
        s.rng = 0xBA7_5EED ^ (u64::from(s.run) * 104729) ^ (u64::from(s.records.matches) << 24) ^ u64::from(s.tick);
        s.phase = Phase::Handoff;
        s.timer = 0;
        self.generate();
    }
    fn push(&mut self, kind: Kind, x: i32, y: i32, w: i32, h: i32) {
        self.state.objs.push(Obj { kind, x, y, w, h, alive: true });
    }
    fn spikes(&mut self, floor: bool, x: i32, count: i32) -> i32 {
        for k in 0..count {
            let y = if floor { FLOOR - 28 } else { CEILING };
            self.push(Kind::Spikes { floor }, x + k * 26, y, 26, 28);
        }
        count * 26
    }
    fn pillar(&mut self, floor: bool, x: i32, h: i32) -> i32 {
        let y = if floor { FLOOR - h } else { CEILING };
        self.push(Kind::Pillar { floor }, x, y, 34, h);
        34
    }
    fn gem_column(&mut self, x: i32) {
        for k in 0..4 {
            self.push(Kind::Gem, x, 130 + k * 52, 16, 16);
        }
    }
    /// Lay out the cave ahead. Each pattern starts after 220 clear pixels so a flip always fits.
    fn generate(&mut self) {
        while self.state.gen_x < self.dist() + BAT_X + 1400 {
            let mut x = self.state.gen_x + 220;
            let mut rng = self.state.rng;
            let dist = x;
            let first = dist < 1000;
            let roll = if first { 0 } else { rnd(&mut rng, 0, 99) };
            let floor = rnd(&mut rng, 0, 1) == 0;
            let count = (2 + dist / 2200 + rnd(&mut rng, 0, 1)).min(5);
            let end;
            if roll < 22 {
                end = x + self.spikes(floor, x, count);
            } else if roll < 38 {
                end = x + self.pillar(floor, x, rnd(&mut rng, 96, 150));
            } else if roll < 54 {
                // Gem column across the flip path, spikes after.
                self.gem_column(x);
                x += 230;
                end = x + self.spikes(floor, x, count);
            } else if roll < 74 && dist > 1800 {
                // Floor then ceiling: flip, and flip back.
                let w = self.spikes(floor, x, count);
                let x2 = x + w + 220;
                end = x2 + self.spikes(!floor, x2, count);
            } else if roll < 86 && dist > 2600 {
                let w = self.pillar(floor, x, rnd(&mut rng, 110, 160));
                let x2 = x + w + 230;
                end = x2 + self.pillar(!floor, x2, rnd(&mut rng, 110, 160));
            } else {
                // A row of gems hugging one surface: greed for the careful.
                let y = if floor { FLOOR - 36 } else { CEILING + 20 };
                for k in 0..6 {
                    self.push(Kind::Gem, x + k * 30, y, 16, 16);
                }
                x += 260;
                end = x + self.spikes(!floor, x, count);
            }
            self.state.rng = rng;
            self.state.gen_x = end;
        }
        let d = self.dist();
        self.state.objs.retain(|o| o.x + o.w + 60 > d + BAT_X - 200);
    }
    fn crash(&mut self) {
        self.state.phase = Phase::Crashed;
        self.state.timer = 0;
        self.cues.push(1);
        self.last_event = Point::new(BAT_X + BAT / 2, self.y() + BAT / 2);
    }
    fn finish_run(&mut self) {
        let p = self.player();
        let slot = self.state.run as usize / 2;
        let score = self.score();
        let dist = self.dist().max(0) as u32;
        let deepest = (dist as i32 / BIOME_LENGTH) as u32;
        let s = &mut self.state;
        s.scores[p][slot] = score;
        let r = &mut s.records;
        r.best_run[p] = r.best_run[p].max(score);
        r.best_distance[p] = r.best_distance[p].max(dist);
        r.gems[p] += s.gems;
        r.shaves[p] += s.shaves;
        r.deepest[p] = r.deepest[p].max(deepest);
    }
    fn after_crash(&mut self) {
        if (self.state.run as usize) + 1 < RUNS * 2 {
            self.state.run += 1;
            self.start_run();
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
        self.state.scores = [[0; RUNS]; 2];
        self.state.run = 0;
        self.start_run();
    }
    fn step_run(&mut self, i: &Intent) {
        let old_side = self.state.side;
        let mut side = old_side;
        if i.action {
            side = -side;
        } else if i.y < 0 {
            side = -1;
        } else if i.y > 0 {
            side = 1;
        }
        let was_grounded = self.grounded();
        let px = self.dist() + BAT_X;
        if side != old_side {
            self.state.flips += 1;
            self.cues.push(0);
            self.last_event = Point::new(BAT_X + BAT / 2, self.y() + BAT / 2);
            // Close shave: flipping away from a hazard that is about to reach us.
            if was_grounded {
                let near = self.state.objs.iter().any(|o| {
                    o.alive
                        && o.hurt().is_some()
                        && o.floor_side() == (old_side > 0)
                        && o.x > px + BAT - 4
                        && o.x - (px + BAT) <= 72
                });
                if near {
                    let mult = self.multiplier();
                    self.state.shaves += 1;
                    self.state.heat = (self.state.heat + 1).min(5);
                    self.state.heat_timer = 0;
                    self.state.points += 25 * mult;
                    self.cues.push(2);
                    self.last_event = Point::new(BAT_X + 40, self.y() + BAT / 2);
                }
            }
        }
        {
            let s = &mut self.state;
            s.side = side;
            s.vys = (s.vys + side * 256).clamp(-14 * 256, 14 * 256);
            s.ys += s.vys;
            let top = CEILING << 8;
            let bottom = (FLOOR - BAT) << 8;
            if s.ys <= top {
                s.ys = top;
                s.vys = 0;
            }
            if s.ys >= bottom {
                s.ys = bottom;
                s.vys = 0;
            }
            s.heat_timer += 1;
            if s.heat_timer > 240 && s.heat > 0 {
                s.heat -= 1;
                s.heat_timer = 0;
            }
        }
        let speed = self.speed16();
        self.state.dist16 += speed;
        self.generate();
        let d = self.dist();
        let px = d + BAT_X;
        let me = self.bat_rect();
        let me = Rect::new(px + me.x - BAT_X, me.y, me.w, me.h);
        let mult = self.multiplier();
        let mut crashed = false;
        let mut gems = 0;
        let mut gained = 0;
        let mut at = None;
        for o in self.state.objs.iter_mut() {
            if !o.alive {
                continue;
            }
            match o.kind {
                Kind::Gem => {
                    if Rect::new(o.x - 4, o.y - 4, o.w + 8, o.h + 8).overlaps(me) {
                        o.alive = false;
                        gems += 1;
                        gained += 10 * mult;
                        at = Some(Point::new(o.x - d, o.y));
                    }
                }
                _ => {
                    if o.hurt().is_some_and(|r| r.overlaps(me)) {
                        crashed = true;
                    }
                }
            }
        }
        if gems > 0 {
            self.state.gems += gems;
            self.state.points += gained;
            self.cues.push(0);
            if let Some(p) = at {
                self.last_event = p;
            }
        }
        let biome = (d / BIOME_LENGTH) as u32;
        if d % BIOME_LENGTH < 12 && biome > 0 {
            self.state.banner = 150;
            self.state.new_biome = true;
        }
        self.state.banner = self.state.banner.saturating_sub(1);
        if crashed {
            self.crash();
        }
    }
}
impl GameLogic for Turvy {
    const ID: &'static str = "topsy-turvy";
    const TITLE: &'static str = "Topsy-Turvy";
    const CONTROLS: &'static str = "A/Space flips gravity · UP = ceiling, DOWN = floor · Flip right at a spike for a close shave";
    const VERIFY_TICKS: u32 = 900;
    fn new(_: u64) -> Self {
        Self {
            state: State {
                tick: 0,
                phase: Phase::Intro,
                timer: 0,
                first: 0,
                run: 0,
                scores: [[0; RUNS]; 2],
                dist16: 0,
                ys: (FLOOR - BAT) << 8,
                vys: 0,
                side: 1,
                heat: 0,
                heat_timer: 0,
                gems: 0,
                shaves: 0,
                flips: 0,
                points: 0,
                banner: 0,
                new_biome: false,
                rng: 1,
                gen_x: 760,
                objs: vec![],
                records: Records::default(),
            },
            cues: vec![],
            last_event: Point::new(BAT_X, 300),
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
        // Pick Dawn to run first, confirm, then flip on a rhythm that crosses the first gem column
        // and a spike row; later presses clear the crash cards.
        let flips = [190, 260, 330, 520, 600];
        Intent {
            x: i32::from((3..12).contains(&tick)),
            action: matches!(tick, 6 | 30) || flips.contains(&tick) || (tick > 700 && tick % 80 == 0),
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
        let run = self.state.phase == Phase::Run;
        let heat = self.state.heat as f32 / 5.;
        let deep = (self.dist() as f32 / 6000.).min(1.);
        match layer {
            "cave" => 0.5,
            "pulse" => if run { 0.6 } else { 0.2 },
            "wings" => if run { 0.3 + 0.5 * heat } else { 0.0 },
            "echo" => if run { 0.2 + 0.6 * deep } else { 0.35 },
            "kick" => if run { 0.5 } else { 0.0 },
            _ => 0.0,
        }
    }
    fn restart(&mut self) {
        let records = self.state.records.clone();
        *self = Self::new(7);
        self.state.records = records;
    }
}
impl Simulation for Turvy {
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
                if i.action && self.state.timer > 10 {
                    self.state.phase = Phase::Run;
                    self.state.timer = 0;
                    self.cues.push(0);
                }
            }
            Phase::Run => self.step_run(i),
            Phase::Crashed => {
                self.state.timer += 1;
                if self.state.timer == 1 {
                    self.finish_run();
                }
                if i.action && self.state.timer > 40 {
                    self.after_crash();
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
impl Snapshot for Turvy {
    const KIND: &'static str = "topsy-turvy";
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, state: State) -> Result<(), String> {
        if state.first > 1
            || state.run as usize >= RUNS * 2
            || !(-1..=1).contains(&state.side)
            || state.side == 0
            || !(0..=5).contains(&state.heat)
            || state.objs.len() > 400
            || state.ys < CEILING << 8
            || state.ys > (FLOOR - BAT) << 8
        {
            return Err("Invalid Topsy-Turvy save".into());
        }
        self.state = state;
        self.cues.clear();
        Ok(())
    }
}
#[cfg(feature = "client")]
const DUSK_PNG: &[u8] = include_bytes!("../assets/sprites/dusk.png");
#[cfg(feature = "client")]
const DAWN_PNG: &[u8] = include_bytes!("../assets/sprites/dawn.png");
#[cfg(feature = "client")]
const GEM_PNG: &[u8] = include_bytes!("../assets/sprites/gem.png");
#[cfg(feature = "client")]
impl Turvy {
    fn bat(&self, s: &mut draw::Scene, who: usize, cx: i32, cy: i32, size: f32, turn: f32, layer: i32) {
        use draw::*;
        let (id, png) = if who == 0 { ("dusk", DUSK_PNG) } else { ("dawn", DAWN_PNG) };
        s.sprite_png(layer, id, png, Transform { position: [cx as f32 - size / 2., cy as f32 - size / 2.], rotation: turn, ..Default::default() }, [size, size], WHITE);
    }
}
#[cfg(feature = "client")]
impl draw::Game for Turvy {
    fn show_hud() -> bool {
        false
    }
    fn menu_status(&self) -> String {
        format!("Run {} of {}  ·  Dusk {}  Dawn {}", self.state.run + 1, RUNS * 2, self.total(0), self.total(1))
    }
    fn draw(&self, s: &mut draw::Scene) {
        use draw::*;
        let st = &self.state;
        let d = self.dist();
        let tints = [Color::new(0.7, 0.55, 1., 1.), Color::new(1., 0.7, 0.5, 1.)];
        let biome = Self::biome(d);
        let (bg, rock, glow) = match biome {
            0 => (Color::new(0.07, 0.07, 0.17, 1.), Color::new(0.20, 0.18, 0.36, 1.), Color::new(0.45, 0.65, 1., 1.)),
            1 => (Color::new(0.05, 0.11, 0.09, 1.), Color::new(0.16, 0.30, 0.22, 1.), Color::new(0.55, 1., 0.55, 1.)),
            2 => (Color::new(0.14, 0.05, 0.05, 1.), Color::new(0.34, 0.16, 0.13, 1.), Color::new(1., 0.55, 0.25, 1.)),
            _ => (Color::new(0.03, 0.03, 0.10, 1.), Color::new(0.12, 0.12, 0.30, 1.), Color::new(1., 0.95, 0.6, 1.)),
        };
        s.rect(-30, Rect::new(0, 0, 800, 450), bg);
        // Parallax rocks and glowing motes.
        for k in 0..9 {
            let x = k * 120 - (d / 4) % 120;
            let h = 20 + ((k * 53) % 4) * 14;
            s.rect(-25, Rect::new(x, CEILING - h, 36, h), Color::new(rock.r * 0.7, rock.g * 0.7, rock.b * 0.7, 1.));
            s.rect(-25, Rect::new(x + 50, FLOOR, 30, h), Color::new(rock.r * 0.7, rock.g * 0.7, rock.b * 0.7, 1.));
        }
        for k in 0..16 {
            let mx = (k * 97 + 300 - (d / 2 + st.tick as i32 / 2)) .rem_euclid(840) - 20;
            let my = 100 + ((k * 61) % 7) * 36 + sin256(st.tick as i32 * 2 + k * 31) * 6 / 4096;
            s.circle(-20, Point::new(mx, my), 2., Color::new(glow.r, glow.g, glow.b, 0.35));
        }
        // Cave walls: ceiling and floor bands with glowing edges.
        s.rect(-4, Rect::new(0, CEILING - 3, 800, 2), glow);
        s.rect(-4, Rect::new(0, FLOOR + 1, 800, 2), glow);
        for o in st.objs.iter().filter(|o| o.alive) {
            let x = o.x - d;
            if x < -60 || x > 860 {
                continue;
            }
            match o.kind {
                Kind::Gem => {
                    let bob = sin256(st.tick as i32 * 4 + o.x) * 3 / 4096;
                    s.sprite_png(5, "gem", GEM_PNG, Transform { position: [x as f32 - 2., (o.y + bob - 2) as f32], ..Default::default() }, [20., 20.], WHITE);
                }
                Kind::Pillar { floor } => {
                    s.rect(3, Rect::new(x, o.y, o.w, o.h), Color::new(rock.r * 1.5, rock.g * 1.5, rock.b * 1.5, 1.));
                    s.rect(4, Rect::new(x, o.y, 5, o.h), Color::new(glow.r, glow.g, glow.b, 0.5));
                    let tip = if floor { o.y } else { o.y + o.h - 5 };
                    s.rect(4, Rect::new(x, tip, o.w, 5), glow);
                }
                Kind::Spikes { floor } => {
                    for row in 0..7 {
                        let w = o.w - row * 4;
                        let y = if floor { o.y + row * 4 } else { o.y + o.h - 4 - row * 4 };
                        s.rect(3, Rect::new(x + (o.w - w) / 2, y, w, 4), Color::new(0.85, 0.88, 0.95, 1.));
                    }
                    s.rect(4, Rect::new(x + o.w / 2 - 1, if floor { o.y } else { o.y + o.h - 4 }, 3, 4), glow);
                }
            }
        }
        let showing = matches!(st.phase, Phase::Handoff | Phase::Run | Phase::Crashed);
        if showing {
            let who = self.player();
            let cy = self.y() + BAT / 2;
            let span = (FLOOR - BAT - CEILING).max(1);
            let frac = ((self.y() - CEILING) * 100 / span).clamp(0, 100);
            // Bat hangs from the ceiling, so it is upside down on the floor.
            let turn = std::f32::consts::PI * (1. - frac as f32 / 100.) + if st.phase == Phase::Crashed { st.timer as f32 * 0.4 } else { 0. };
            // Speed lines and a heat trail.
            if st.phase == Phase::Run {
                for k in 1..=(2 + st.heat) {
                    s.rect(4, Rect::new(BAT_X - k * 16 - 6, cy - 2 + (k % 2) * 6, 12, 3), Color::new(glow.r, glow.g, glow.b, 0.5 - k as f32 * 0.07));
                }
            }
            self.bat(s, who, BAT_X + BAT / 2, cy, 34., turn, 10);
            s.text(20, format!("{}  score {}", NAMES[who], self.score()), Point::new(300, 30), 22., tints[who]);
            s.text(20, format!("heat x{}   gems {}   shaves {}", self.multiplier(), st.gems, st.shaves), Point::new(300, 54), 15., GOLD);
            s.text(20, format!("{}  {}m", BIOME_NAMES[biome], d / 10), Point::new(560, 54), 15., glow);
            s.rect(20, Rect::new(24, 52, 100, 8), Color::new(0., 0., 0., 0.4));
            s.rect(21, Rect::new(25, 53, st.heat * 98 / 5, 6), PINK);
            s.text(20, "heat", Point::new(130, 61), 12., WHITE);
            if st.banner > 0 {
                s.text(22, format!("ENTERING {}", BIOME_NAMES[biome].to_uppercase()), Point::new(220, 200), 30., Color::new(glow.r, glow.g, glow.b, (st.banner as f32 / 60.).min(1.)));
            }
        }
        let dim = Color::new(0.02, 0.02, 0.08, 0.8);
        match st.phase {
            Phase::Intro => {
                s.rect(30, Rect::new(70, 60, 660, 340), dim);
                s.text(31, "TOPSY-TURVY", Point::new(250, 112), 46., GOLD);
                s.text(31, "Press to flip gravity. One hit and it's over.", Point::new(210, 146), 20., WHITE);
                s.text(31, "Flip right in front of a spike for a CLOSE SHAVE.", Point::new(200, 172), 18., TEAL);
                s.text(31, "Who runs first?  (left / right, then press)", Point::new(220, 204), 18., TEAL);
                for p in 0..2usize {
                    let x = 250 + p as i32 * 260;
                    if st.first as usize == p {
                        s.rect(31, Rect::new(x - 40, 215, 120, 120), Color::new(1., 1., 1., 0.12));
                    }
                    self.bat(s, p, x + 20, 268, 80., 0., 32);
                    s.text(32, NAMES[p], Point::new(x - 6, 330), 22., if st.first as usize == p { tints[p] } else { WHITE });
                }
                let r = &st.records;
                s.text(31, format!("Matches {}  Wins {}-{}  Best run {} / {}  Furthest {}m / {}m", r.matches, r.wins[0], r.wins[1], r.best_run[0], r.best_run[1], r.best_distance[0] / 10, r.best_distance[1] / 10), Point::new(90, 362), 16., TEAL);
                s.text(31, format!("Gems {} / {}  ·  Shaves {} / {}  ·  Deepest: {} / {}", r.gems[0], r.gems[1], r.shaves[0], r.shaves[1], BIOME_NAMES[r.deepest[0] as usize % 4], BIOME_NAMES[r.deepest[1] as usize % 4]), Point::new(90, 386), 16., GOLD);
            }
            Phase::Handoff => {
                s.rect(30, Rect::new(150, 110, 500, 210), dim);
                let p = self.player();
                s.text(31, format!("Pass the controller to {}", NAMES[p]), Point::new(190, 160), 26., tints[p]);
                s.text(31, format!("Run {} of {}.  Press to launch.", st.run + 1, RUNS * 2), Point::new(190, 200), 20., WHITE);
                s.text(31, format!("Dusk {}  ·  Dawn {}  (best run counts)", self.total(0), self.total(1)), Point::new(190, 240), 20., GOLD);
                s.text(31, "A flips, or UP for the ceiling and DOWN for the floor.", Point::new(190, 290), 16., TEAL);
            }
            Phase::Crashed if st.timer > 20 => {
                s.rect(30, Rect::new(150, 100, 500, 240), dim);
                let p = self.player();
                s.text(31, "SPLAT!", Point::new(190, 150), 34., GOLD);
                s.text(31, format!("{} scored {}  ({}m)", NAMES[p], self.score(), self.dist() / 10), Point::new(190, 190), 24., tints[p]);
                s.text(31, format!("Gems {}  ·  close shaves {}  ·  flips {}", st.gems, st.shaves, st.flips), Point::new(190, 224), 17., WHITE);
                if self.score() >= st.records.best_run[p] && self.score() > 0 {
                    s.text(31, "NEW PERSONAL BEST!", Point::new(190, 262), 22., PINK);
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
                    s.text(31, format!("{:<6} best {:>5}   ({})", NAMES[p], t[p], row.join(" / ")), Point::new(140, 175 + p as i32 * 32), 22., tints[p]);
                }
                let r = &st.records;
                s.text(31, format!("Wins {}-{}   Furthest {}m / {}m", r.wins[0], r.wins[1], r.best_distance[0] / 10, r.best_distance[1] / 10), Point::new(140, 262), 17., TEAL);
                s.text(31, "Press for a rematch (the other bat runs first)", Point::new(140, 340), 16., WHITE);
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
    fn begin(g: &mut Turvy) {
        g.step(&press());
        assert_eq!(g.state.phase, Phase::Handoff);
        for _ in 0..12 {
            g.step(&Intent::default());
        }
        g.step(&press());
        assert_eq!(g.state.phase, Phase::Run);
    }
    /// Flip when a hazard on our surface is within a flip's distance.
    fn isqrt(v: i64) -> i64 {
        (v as f64).sqrt() as i64
    }
    fn pilot(g: &Turvy) -> Intent {
        if !g.grounded() {
            return Intent::default();
        }
        let px = g.dist() + BAT_X;
        let floor = g.state.side > 0;
        // Climbing out of a hazard of height h takes about sqrt(2h) ticks at one pixel per tick squared.
        let danger = g.state.objs.iter().any(|o| {
            let ticks = 2 * o.h + 24;
            let lead = g.speed16() * (isqrt(ticks as i64) as i32) / 16 + 8;
            o.alive && o.hurt().is_some() && o.floor_side() == floor && o.x + o.w > px && o.x - (px + BAT) < lead
        });
        if danger {
            press()
        } else {
            Intent::default()
        }
    }
    #[test]
    fn verification_route_is_deterministic_and_resumes() {
        let inputs: Vec<Intent> = (0..Turvy::VERIFY_TICKS).map(Turvy::verification_input).collect();
        vesper3d::runtime::assert_deterministic(|| Turvy::new(7), &inputs);
        snapshot::assert_resumes_exactly(|| Turvy::new(7), &inputs, 113);
        let (hash, outcome) = verify::<Turvy>();
        assert_eq!(outcome, "playing");
        let mut g = Turvy::new(7);
        let mut far = 0;
        let mut flips = 0;
        for i in &inputs {
            g.step(i);
            far = far.max(g.dist());
            flips = flips.max(g.state.flips);
        }
        assert_eq!(g.state.first, 1, "the route picks Dawn to run first");
        assert!(far > 800 && flips >= 2, "the route actually runs and flips (far {far}, flips {flips})");
        if let Ok(path) = std::env::var("BE2_VERIFY_REPORT") {
            std::fs::write(
                path,
                serde_json::json!({"hash":format!("{hash:016x}"),"outcome":outcome,"ticks":Turvy::VERIFY_TICKS,
                    "purpose":"Hot-seat cave run: picks Dawn, launches, flips gravity across a gem column and spike rows until a crash card appears"}).to_string(),
            )
            .unwrap();
        }
    }
    #[test]
    fn flipping_switches_surfaces_and_direct_up_down_choose_a_side() {
        let mut g = Turvy::new(7);
        begin(&mut g);
        g.state.objs.clear();
        g.state.gen_x = 100_000;
        assert!(g.grounded() && g.state.side == 1);
        g.step(&press());
        assert_eq!(g.state.side, -1);
        let mut mid = false;
        for _ in 0..60 {
            g.step(&Intent::default());
            mid |= !g.grounded();
        }
        assert!(mid, "the bat travels through the air");
        assert_eq!(g.y(), CEILING, "and lands on the ceiling");
        g.step(&Intent { y: 1, ..Default::default() });
        assert_eq!(g.state.side, 1, "down picks the floor directly");
        for _ in 0..60 {
            g.step(&Intent::default());
        }
        assert_eq!(g.y(), FLOOR - BAT);
        g.step(&Intent { y: 1, ..Default::default() });
        assert_eq!(g.state.side, 1, "down on the floor stays on the floor");
        assert_eq!(g.state.flips, 2);
    }
    #[test]
    fn spikes_end_the_run_and_a_flip_in_time_is_a_close_shave() {
        let mut dead = Turvy::new(7);
        begin(&mut dead);
        dead.state.objs.clear();
        dead.state.gen_x = 100_000;
        let x = dead.dist() + BAT_X + 120;
        dead.spikes(true, x, 2);
        for _ in 0..60 {
            dead.step(&Intent::default());
        }
        assert_eq!(dead.state.phase, Phase::Crashed);
        let mut safe = Turvy::new(7);
        begin(&mut safe);
        safe.state.objs.clear();
        safe.state.gen_x = 100_000;
        let x = safe.dist() + BAT_X + 70;
        safe.spikes(true, x, 2);
        safe.step(&press());
        assert_eq!(safe.state.shaves, 1, "flipping 70px ahead of spikes is a close shave");
        assert_eq!(safe.state.heat, 1);
        assert!(safe.state.points >= 25);
        for _ in 0..80 {
            safe.step(&Intent::default());
        }
        assert_eq!(safe.state.phase, Phase::Run, "the shaved run survives");
        let mut early = Turvy::new(7);
        begin(&mut early);
        early.state.objs.clear();
        early.state.gen_x = 100_000;
        let x = early.dist() + BAT_X + 400;
        early.spikes(true, x, 2);
        early.step(&press());
        assert_eq!(early.state.shaves, 0, "flipping far too early earns nothing");
    }
    #[test]
    fn gems_score_with_the_heat_multiplier() {
        let mut g = Turvy::new(7);
        begin(&mut g);
        g.state.objs.clear();
        g.state.gen_x = 100_000;
        g.state.heat = 2;
        let x = g.dist() + BAT_X + 6;
        let y = g.y();
        g.push(Kind::Gem, x, y, 16, 16);
        g.step(&Intent::default());
        assert_eq!(g.state.gems, 1);
        assert_eq!(g.state.points, 30, "10 points times heat multiplier 3");
    }
    #[test]
    fn a_match_keeps_each_bats_best_run_and_records_survive_restart() {
        let mut g = Turvy::new(7);
        begin(&mut g);
        let mut guard = 0;
        while g.state.phase != Phase::Results && guard < 20 {
            guard += 1;
            // Crash immediately with a score that grows with the run number.
            g.state.objs.clear();
            g.state.gen_x = 100_000;
            g.state.points = 100 * (u32::from(g.state.run) + 1);
            let x = g.dist() + BAT_X + 6;
            g.spikes(true, x, 3);
            for _ in 0..10 {
                g.step(&Intent::default());
            }
            for _ in 0..50 {
                g.step(&Intent::default());
            }
            g.step(&press());
            if g.state.phase == Phase::Handoff {
                for _ in 0..12 {
                    g.step(&Intent::default());
                }
                g.step(&press());
            }
        }
        assert_eq!(g.state.phase, Phase::Results);
        assert_eq!(g.state.records.matches, 1);
        assert!(g.total(0) > 0 && g.total(1) > 0);
        assert_eq!(g.total(0), *g.state.scores[0].iter().max().unwrap(), "the best of three counts");
        let records = g.state.records.clone();
        g.restart();
        assert_eq!(g.state.records, records);
        assert_eq!(g.state.phase, Phase::Intro);
    }
    #[test]
    fn every_pattern_is_flyable_by_a_simple_pilot() {
        for seed in 0..6u32 {
            let mut g = Turvy::new(7);
            for _ in 0..seed * 13 {
                g.step(&Intent::default());
            }
            begin(&mut g);
            let mut gems = 0;
            for _ in 0..6000 {
                let i = pilot(&g);
                g.step(&i);
                gems = gems.max(g.state.gems);
                assert_eq!(g.state.phase, Phase::Run, "seed {seed}: crashed at {}m ({} flips)", g.dist() / 10, g.state.flips);
            }
            assert!(g.dist() > 6000, "seed {seed}: ran only {}", g.dist());
            assert!(g.state.flips > 20, "seed {seed}: flipped only {} times", g.state.flips);
        }
    }
    #[test]
    fn saves_reject_nonsense() {
        let mut g = Turvy::new(7);
        let mut bad = g.capture();
        bad.side = 0;
        assert!(g.restore(bad).is_err());
        let mut bad = g.capture();
        bad.run = 9;
        assert!(g.restore(bad).is_err());
        assert!(!g.probe_success());
        g.step(&Intent { x: 1, ..Default::default() });
        assert!(g.probe_success());
        assert_eq!(Turvy::biome(0), 0);
        assert_eq!(Turvy::biome(BIOME_LENGTH * 2), 2);
    }
}
