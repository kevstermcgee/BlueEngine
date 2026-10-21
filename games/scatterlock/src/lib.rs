//! Scatterlock authority. Coordinates are dyadic fixed point; no graphics or wall clock.
pub mod controls;
use serde::{Deserialize, Serialize};
use vesper3d::viewer::devkit::{SavePolicy, Simulation, Snapshot, StateHasher};
pub const UNIT: i64 = 1024;
pub const HALF: i64 = 384;
pub type Point = [i64; 3];
pub fn point(x: f32, y: f32, z: f32) -> Point {
    [(x * 1024.) as i64, (y * 1024.) as i64, (z * 1024.) as i64]
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pulse {
    #[default]
    Spread,
    Gather,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intent {
    pub beacon: Option<usize>,
    pub cycle: bool,
    pub mode: Option<Pulse>,
    pub execute: bool,
    pub undo: bool,
    pub restart: bool,
    pub next: bool,
}
#[derive(Clone, Debug)]
pub struct Baffle {
    pub center: Point,
    pub half: Point,
}
#[derive(Clone, Debug)]
pub struct Chamber {
    pub title: &'static str,
    pub hint: &'static str,
    pub cargo: Vec<Point>,
    pub sockets: Vec<Point>,
    pub beacons: Vec<Point>,
    pub baffles: Vec<Baffle>,
    pub allowance: u8,
    pub solution: Vec<(usize, Pulse)>,
}
fn pts(a: &[[f32; 3]]) -> Vec<Point> {
    a.iter().map(|v| point(v[0], v[1], v[2])).collect()
}
pub fn chamber(n: usize) -> Chamber {
    use Pulse::*;
    let base = || pts(&[[-2., 2., -2.], [0., 3., 0.]]);
    let common = || pts(&[[-3., 1., -3.], [1., 3., 1.], [-1., 5., -1.], [1., 3., 1.], [-4., 3., -3.]]);
    let mut c = match n {
        0 => Chamber {
            title: "Origin shift",
            hint: "Spread at A, then Gather at B. Changing the origin transports the formation.",
            cargo: base(),
            sockets: pts(&[[0., 3., 0.], [2., 4., 2.]]),
            beacons: common()[..2].to_vec(),
            baffles: vec![],
            allowance: 2,
            solution: vec![(0, Spread), (1, Gather)],
        },
        1 => Chamber {
            title: "The upper berth",
            hint: "Gather first. The elevated cargo must clear the lower baffle before spreading.",
            cargo: pts(&[[-2., 1., -2.], [0., 3., 0.]]),
            sockets: pts(&[[0., 3., 0.], [2., 5., 2.]]),
            beacons: pts(&[[1., 5., 1.], [-1., 3., -1.], [3., 1., -3.]]),
            baffles: vec![],
            allowance: 3,
            solution: vec![(0, Gather), (1, Spread)],
        },
        2 => Chamber {
            title: "One stays behind",
            hint: "Docking freezes a cube. Use the high beacon to spread its partner into the far berth.",
            cargo: base(),
            sockets: pts(&[[0., 3., 0.], [3., 3., 3.]]),
            beacons: pts(&[[-3., 1., -3.], [1., 3., 1.], [1., 5., 1.]]),
            baffles: vec![],
            allowance: 3,
            solution: vec![(0, Spread), (1, Gather), (2, Spread)],
        },
        3 => Chamber {
            title: "Three-way traffic",
            hint: "Three paths, one pulse. Gather the two loose cubes above the secured first delivery.",
            cargo: pts(&[[-2., 2., -2.], [0., 3., 0.], [-2., 4., 1.]]),
            sockets: pts(&[[0., 3., 0.], [0., 6., 0.], [-2., 7., 1.]]),
            beacons: common()[..4].to_vec(),
            baffles: vec![],
            allowance: 4,
            solution: vec![(0, Spread), (1, Gather), (2, Gather), (3, Spread)],
        },
        4 => Chamber {
            title: "Delivery is an obstacle",
            hint: "The tempting Spread at A docks amber too soon. Deliver cyan first, then detour UP and BACK.",
            cargo: pts(&[[-2., 2., 0.], [0., 2., 0.]]),
            sockets: pts(&[[0., 2., 0.], [-1., 2., 0.]]),
            beacons: pts(&[[-4., 2., 0.], [-2., 2., 0.], [-2., 6., 4.], [-4., 4., 2.], [0., 0., -2.]]),
            baffles: vec![],
            allowance: 4,
            solution: vec![(1, Gather), (2, Gather), (3, Spread), (4, Gather)],
        },
        _ => Chamber {
            title: "The last constellation",
            hint: "Secure amber, lift cyan, then bring violet through the rear channel. Watch every destination.",
            cargo: pts(&[[-2., 2., -2.], [0., 3., 0.], [-2., 4., 1.]]),
            sockets: pts(&[[0., 3., 0.], [0., 6., 0.], [-3., 5., -1.]]),
            beacons: common(),
            baffles: vec![],
            allowance: 5,
            solution: vec![(0, Spread), (1, Gather), (2, Gather), (3, Spread), (4, Gather)],
        },
    };
    if n == 1 {
        c.baffles.push(Baffle { center: point(1., 1., 1.), half: point(1.2, 0.4, 1.2) });
    }
    if n == 2 {
        c.baffles.push(Baffle { center: point(-2., 4., 2.), half: point(0.35, 2., 1.2) });
    }
    if n == 3 || n == 5 {
        c.baffles.push(Baffle { center: point(2., 1., 2.), half: point(1.7, 0.4, 1.7) });
        c.baffles.push(Baffle { center: point(3., 6.8, -2.), half: point(0.35, 0.8, 2.) });
        c.baffles.push(Baffle { center: point(-3.8, 4., 2.), half: point(0.3, 2., 1.3) });
    }
    c
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Configuration {
    pub positions: Vec<Point>,
    pub docked: Vec<bool>,
    pub remaining: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimState {
    pub tick: u64,
    pub level: usize,
    pub config: Configuration,
    pub history: Vec<Configuration>,
    pub selected: usize,
    pub mode: Pulse,
    pub rejected: bool,
}
pub struct Sim {
    pub state: SimState,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Obstruction {
    pub cube: usize,
    pub at: f32,
    pub reason: &'static str,
}
pub struct Preview {
    pub destinations: Vec<Point>,
    pub deliveries: Vec<bool>,
    pub obstruction: Option<Obstruction>,
}
// A rational slab interval. All comparisons use integer cross products, including simultaneous motion.
#[derive(Clone, Copy)]
struct Rat(i64, i64);
impl Rat {
    fn new(n: i64, d: i64) -> Self {
        if d < 0 {
            Self(-n, -d)
        } else {
            Self(n, d)
        }
    }
    fn lt(self, b: Self) -> bool {
        (self.0 as i128) * (b.1 as i128) < (b.0 as i128) * (self.1 as i128)
    }
    fn value(self) -> f32 {
        self.0 as f32 / self.1 as f32
    }
}
pub fn sweep(p: Point, q: Point, center: Point, half: Point) -> Option<f32> {
    let (mut lo, mut hi) = (Rat(0, 1), Rat(1, 1));
    for j in 0..3 {
        let d = q[j] - p[j];
        let a = center[j] - half[j] - p[j];
        let b = center[j] + half[j] - p[j];
        if d == 0 {
            if a > 0 || b < 0 {
                return None;
            }
        } else {
            let (mut x, mut y) = (Rat::new(a, d), Rat::new(b, d));
            if y.lt(x) {
                std::mem::swap(&mut x, &mut y);
            }
            if lo.lt(x) {
                lo = x;
            }
            if y.lt(hi) {
                hi = y;
            }
            if hi.lt(lo) {
                return None;
            }
        }
    }
    Some(lo.value())
}
impl Sim {
    pub fn new(_seed: u64) -> Self {
        Self::at_level(0)
    }
    pub fn at_level(level: usize) -> Self {
        let c = chamber(level);
        Self {
            state: SimState {
                tick: 0,
                level,
                config: Configuration {
                    positions: c.cargo.clone(),
                    docked: vec![false; c.cargo.len()],
                    remaining: c.allowance,
                },
                history: vec![],
                selected: 0,
                mode: Pulse::Spread,
                rejected: false,
            },
        }
    }
    pub fn won(&self) -> bool {
        self.state.config.docked.iter().all(|x| *x)
    }
    pub fn lost(&self) -> bool {
        self.state.config.remaining == 0 && !self.won()
    }
    pub fn preview(&self) -> Preview {
        let s = &self.state;
        let c = chamber(s.level);
        let a = c.beacons[s.selected];
        let cfg = &s.config;
        let destinations: Vec<Point> = cfg
            .positions
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if cfg.docked[i] {
                    *p
                } else {
                    std::array::from_fn(|j| {
                        a[j] + match s.mode {
                            Pulse::Spread => 2 * (p[j] - a[j]),
                            Pulse::Gather => (p[j] - a[j]) / 2,
                        }
                    })
                }
            })
            .collect();
        let mut obstruction: Option<Obstruction> = None;
        let mut offer = |cube, at, reason| {
            if obstruction.as_ref().is_none_or(|o| at < o.at) {
                obstruction = Some(Obstruction { cube, at, reason });
            }
        };
        for (i, (&p, &q)) in cfg.positions.iter().zip(&destinations).enumerate() {
            if cfg.docked[i] {
                continue;
            }
            for j in 0..3 {
                let (low, high) = if j == 1 { (HALF, 8 * UNIT - HALF) } else { (-6 * UNIT + HALF, 6 * UNIT - HALF) };
                if q[j] < low || q[j] > high {
                    let edge = if q[j] < low { low } else { high };
                    let t = (edge - p[j]) as f32 / (q[j] - p[j]) as f32;
                    offer(i, t, "chamber boundary");
                }
            }
            for b in &c.baffles {
                if let Some(t) = sweep(p, q, b.center, b.half.map(|v| v + HALF)) {
                    offer(i, t, "solid baffle");
                }
            }
            for (k, destination) in destinations.iter().enumerate().take(i) {
                let rp = std::array::from_fn(|j| p[j] - cfg.positions[k][j]);
                let rq = std::array::from_fn(|j| q[j] - destination[j]);
                if let Some(t) = sweep(rp, rq, [0; 3], [HALF * 2; 3]) {
                    offer(i, t, if cfg.docked[k] { "docked cargo" } else { "cargo path" });
                }
            }
            // Later indexed docked cubes also obstruct earlier moving cubes.
            for k in i + 1..cfg.positions.len() {
                if cfg.docked[k] {
                    if let Some(t) = sweep(p, q, cfg.positions[k], [HALF * 2; 3]) {
                        offer(i, t, "docked cargo");
                    }
                }
            }
        }
        let deliveries = destinations
            .iter()
            .zip(&c.sockets)
            .enumerate()
            .map(|(i, (p, g))| !cfg.docked[i] && (0..3).all(|j| (p[j] - g[j]).abs() <= 160))
            .collect();
        Preview { destinations, deliveries, obstruction }
    }
    pub fn step(&mut self, i: &Intent) {
        self.state.tick += 1;
        if i.restart {
            let tick = self.state.tick;
            *self = Self::at_level(self.state.level);
            self.state.tick = tick;
            return;
        }
        if i.undo {
            if let Some(c) = self.state.history.pop() {
                self.state.config = c;
            }
            self.state.rejected = false;
            return;
        }
        if i.next && self.won() && self.state.level < 5 {
            let tick = self.state.tick;
            *self = Self::at_level(self.state.level + 1);
            self.state.tick = tick;
            return;
        }
        let c = chamber(self.state.level);
        if let Some(b) = i.beacon {
            if b < c.beacons.len() {
                self.state.selected = b;
                self.state.rejected = false;
            }
        }
        if i.cycle {
            self.state.selected = (self.state.selected + 1) % c.beacons.len();
            self.state.rejected = false;
        }
        if let Some(m) = i.mode {
            self.state.mode = m;
            self.state.rejected = false;
        }
        if i.execute && !self.won() && !self.lost() {
            let p = self.preview();
            if p.obstruction.is_some() {
                self.state.rejected = true;
            } else {
                self.state.history.push(self.state.config.clone());
                self.state.config.positions = p.destinations;
                for (d, deliver) in self.state.config.docked.iter_mut().zip(p.deliveries) {
                    *d |= deliver;
                }
                self.state.config.remaining -= 1;
                self.state.rejected = false;
            }
        }
    }
}
impl Simulation for Sim {
    type Input = Intent;
    fn step(&mut self, i: &Intent) {
        Sim::step(self, i)
    }
    fn state_hash(&self) -> u64 {
        let mut h = StateHasher::new();
        for b in serde_json::to_vec(&self.state).expect("state") {
            h.u32(b as u32);
        }
        h.finish()
    }
}
impl Snapshot for Sim {
    const KIND: &'static str = "scatterlock";
    const POLICY: SavePolicy = SavePolicy::Exact;
    type State = SimState;
    fn capture(&self) -> SimState {
        self.state.clone()
    }
    fn restore(&mut self, s: SimState) -> Result<(), String> {
        if s.level >= 6 || s.selected >= chamber(s.level).beacons.len() {
            return Err("Unknown chamber or beacon".into());
        }
        let c = chamber(s.level);
        let valid = |x: &Configuration| {
            x.positions.len() == c.cargo.len()
                && x.docked.len() == c.cargo.len()
                && x.remaining <= c.allowance
                && x.positions.iter().enumerate().all(|(i, p)| {
                    (0..3).all(|j| {
                        if j == 1 {
                            p[j] >= HALF && p[j] <= 8 * UNIT - HALF
                        } else {
                            p[j].abs() <= 6 * UNIT - HALF
                        }
                    }) && (!x.docked[i] || (0..3).all(|j| (p[j] - c.sockets[i][j]).abs() <= 160))
                })
        };
        if !valid(&s.config) || s.history.len() > c.allowance as usize || !s.history.iter().all(valid) {
            return Err("Invalid cargo configuration".into());
        }
        self.state = s;
        Ok(())
    }
    fn save_tick(&self) -> u64 {
        self.state.tick
    }
}

/// Real public-input verification; never teleports cargo or bypasses puzzle rules.
pub fn verification_input() -> Vec<Intent> {
    let mut route = vec![
        Intent { mode: Some(Pulse::Gather), execute: true, ..Default::default() },
        Intent { undo: true, ..Default::default() },
    ];
    for level in 0..6 {
        for (beacon, mode) in chamber(level).solution {
            route.push(Intent { beacon: Some(beacon), mode: Some(mode), execute: true, ..Default::default() });
        }
        if level < 5 {
            route.push(Intent { next: true, ..Default::default() });
        }
    }
    route
}
