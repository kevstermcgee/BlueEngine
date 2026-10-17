//! Crosshaul's integer 3D authority. No drawing, devices, wall clock, or floating point rules.
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::OnceLock;
use vesper3d::viewer::devkit::{SavePolicy, Simulation, Snapshot, StateHasher};
pub type Cell = [i8; 3];
pub const TRAVEL: u8 = 26;
#[derive(Clone, Debug, Deserialize)]
pub struct Chamber {
    pub name: String,
    pub hint: String,
    pub start: Vec<Cell>,
    pub docks: Vec<Cell>,
    pub baffles: Vec<Cell>,
    pub allowance: u8,
    pub solution: Vec<[usize; 3]>,
}
pub fn chambers() -> &'static [Chamber] {
    static DATA: OnceLock<Vec<Chamber>> = OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(include_str!("../assets/chambers.json")).expect("checked chambers"))
}
/// Every device action is an intention; the renderer cannot move cargo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Intent {
    pub select: Option<u8>,
    pub axis: Option<u8>,
    pub execute: bool,
    pub undo: bool,
    pub restart: bool,
    pub next: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    pub positions: Vec<Cell>,
    pub used: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Motion {
    pub from: Vec<Cell>,
    pub remaining: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sim {
    pub tick: u64,
    pub chamber: usize,
    pub positions: Vec<Cell>,
    pub used: u8,
    pub pair: Vec<usize>,
    pub axis: usize,
    pub history: Vec<Turn>,
    pub motion: Option<Motion>,
    pub rejected: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Obstruction {
    Baffle(usize),
    Cargo(usize),
    Partner,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub at: Cell,
    pub kind: Obstruction,
}
#[derive(Clone, Debug)]
pub struct Preview {
    pub destinations: [Cell; 2],
    pub hits: [Option<Hit>; 2],
    pub changes: bool,
}
impl Preview {
    pub fn legal(&self) -> bool {
        self.changes && self.hits.iter().all(Option::is_none)
    }
}
/// Cubes are 0.72 cells wide; baffles 0.96. Integer parallel lanes are disjoint.
/// Sweeping a closed interval on the changed axis is therefore an exact box test,
/// including destinations. The moving partner is tested in relative motion.
pub fn preview(c: &Chamber, positions: &[Cell], a: usize, b: usize, axis: usize) -> Preview {
    let mut destinations = [positions[a], positions[b]];
    destinations[0][axis] = positions[b][axis];
    destinations[1][axis] = positions[a][axis];
    let changes = positions[a][axis] != positions[b][axis];
    let mut hits = [None, None];
    for (side, cargo) in [a, b].into_iter().enumerate() {
        let p = positions[cargo];
        let end = destinations[side];
        let on_route = |o: Cell| {
            (0..3).all(|k| k == axis || o[k] == p[k])
                && (p[axis].min(end[axis])..=p[axis].max(end[axis])).contains(&o[axis])
        };
        let mut nearest = i32::MAX / 4;
        for (at, kind) in c.baffles.iter().copied().enumerate().map(|(i, p)| (p, Obstruction::Baffle(i))).chain(
            positions
                .iter()
                .copied()
                .enumerate()
                .filter(|(i, _)| *i != a && *i != b)
                .map(|(i, p)| (p, Obstruction::Cargo(i))),
        ) {
            let clearance = if matches!(kind, Obstruction::Baffle(_)) { 84 } else { 72 };
            let distance = (at[axis] as i32 - p[axis] as i32).abs() * 100 - clearance;
            if on_route(at) && distance < nearest {
                nearest = distance;
                hits[side] = Some(Hit { at, kind });
            }
        }
        let other = positions[if cargo == a { b } else { a }];
        if changes && (0..3).all(|k| k == axis || p[k] == other[k]) {
            // Relative velocity is twice either cube's velocity: they meet halfway.
            let distance = (other[axis] as i32 - p[axis] as i32).abs() * 50 - 36;
            if distance <= nearest {
                hits[side] = Some(Hit { at: other, kind: Obstruction::Partner });
            }
        }
    }
    Preview { destinations, hits, changes }
}
impl Sim {
    pub fn new(_seed: u64) -> Self {
        Self::at(0)
    }
    pub fn at(chamber: usize) -> Self {
        Self {
            tick: 0,
            chamber,
            positions: chambers()[chamber].start.clone(),
            used: 0,
            pair: vec![],
            axis: 1,
            history: vec![],
            motion: None,
            rejected: 0,
        }
    }
    pub fn level(&self) -> &'static Chamber {
        &chambers()[self.chamber]
    }
    pub fn docked(&self) -> usize {
        self.positions.iter().zip(&self.level().docks).filter(|(a, b)| a == b).count()
    }
    pub fn won(&self) -> bool {
        self.motion.is_none() && self.docked() == self.positions.len()
    }
    pub fn failed(&self) -> bool {
        self.motion.is_none() && self.used >= self.level().allowance && !self.won()
    }
    pub fn preview(&self) -> Option<Preview> {
        (self.pair.len() == 2).then(|| preview(self.level(), &self.positions, self.pair[0], self.pair[1], self.axis))
    }
    pub fn step(&mut self, input: &Intent) {
        self.tick += 1;
        self.rejected = self.rejected.saturating_sub(1);
        if input.restart {
            let tick = self.tick;
            *self = Self::at(self.chamber);
            self.tick = tick;
            return;
        }
        if input.undo {
            if let Some(t) = self.history.pop() {
                self.positions = t.positions;
                self.used = t.used;
                self.motion = None;
                self.rejected = 0;
            }
            return;
        }
        if let Some(m) = &mut self.motion {
            m.remaining -= 1;
            if m.remaining == 0 {
                self.motion = None;
            }
            return;
        }
        if input.next && self.won() && self.chamber + 1 < chambers().len() {
            let tick = self.tick;
            *self = Self::at(self.chamber + 1);
            self.tick = tick;
            return;
        }
        if let Some(k) = input.axis.filter(|k| *k < 3) {
            self.axis = k as usize;
        }
        if let Some(i) = input.select.map(usize::from).filter(|i| *i < self.positions.len()) {
            if let Some(p) = self.pair.iter().position(|j| *j == i) {
                self.pair.remove(p);
            } else {
                if self.pair.len() == 2 {
                    self.pair.clear();
                }
                self.pair.push(i);
            }
        }
        if input.execute && !self.failed() && !self.won() {
            if let Some(p) = self.preview().filter(Preview::legal) {
                let from = self.positions.clone();
                self.history.push(Turn { positions: from.clone(), used: self.used });
                self.positions[self.pair[0]] = p.destinations[0];
                self.positions[self.pair[1]] = p.destinations[1];
                self.used += 1;
                self.motion = Some(Motion { from, remaining: TRAVEL });
                self.rejected = 0;
            } else {
                self.rejected = 120;
            }
        }
    }
    /// Breadth-first hint uses exactly the public route test. It never changes authority.
    pub fn hint(&self) -> Option<[usize; 3]> {
        if self.won() {
            return None;
        }
        let start = self.positions.clone();
        let mut q = VecDeque::from([start.clone()]);
        let mut seen = HashMap::from([(start, None)]);
        while let Some(s) = q.pop_front() {
            for a in 0..s.len() {
                for b in a + 1..s.len() {
                    for k in 0..3 {
                        let p = preview(self.level(), &s, a, b, k);
                        if !p.legal() {
                            continue;
                        }
                        let mut t = s.clone();
                        t[a] = p.destinations[0];
                        t[b] = p.destinations[1];
                        if seen.contains_key(&t) {
                            continue;
                        }
                        let first = seen[&s].unwrap_or([a, b, k]);
                        if t == self.level().docks {
                            return Some(first);
                        }
                        seen.insert(t.clone(), Some(first));
                        q.push_back(t);
                    }
                }
            }
        }
        None
    }
}
impl Simulation for Sim {
    type Input = Intent;
    fn step(&mut self, input: &Intent) {
        Sim::step(self, input);
    }
    fn state_hash(&self) -> u64 {
        // Stable serialization includes selection, in-flight motion and every undo entry.
        let mut h = StateHasher::new();
        for b in serde_json::to_vec(self).expect("integer state") {
            h.u32(b as u32);
        }
        h.finish()
    }
}
impl Snapshot for Sim {
    const KIND: &'static str = "crosshaul";
    const POLICY: SavePolicy = SavePolicy::Exact;
    type State = Self;
    fn capture(&self) -> Self {
        self.clone()
    }
    fn restore(&mut self, state: Self) -> Result<(), String> {
        let c = chambers().get(state.chamber).ok_or("unknown chamber")?;
        let valid = |p: &[Cell]| {
            p.len() == c.start.len()
                && p.iter().all(|v| v.iter().all(|n| (0..=4).contains(n)) && !c.baffles.contains(v))
                && p.iter().enumerate().all(|(i, v)| !p[..i].contains(v))
                && (0..3).all(|k| {
                    let mut a: Vec<_> = p.iter().map(|v| v[k]).collect();
                    let mut b: Vec<_> = c.start.iter().map(|v| v[k]).collect();
                    a.sort();
                    b.sort();
                    a == b
                })
        };
        if !valid(&state.positions)
            || state.used > c.allowance
            || state.axis > 2
            || state.pair.len() > 2
            || state.pair.iter().any(|i| *i >= c.start.len())
            || (state.pair.len() == 2 && state.pair[0] == state.pair[1])
            || state.history.len() != state.used as usize
            || state.history.iter().enumerate().any(|(i, t)| t.used as usize != i || !valid(&t.positions))
            || state.motion.as_ref().is_some_and(|m| {
                m.remaining == 0
                    || m.remaining > TRAVEL
                    || !valid(&m.from)
                    || state.history.last().is_none_or(|h| h.positions != m.from)
            })
        {
            return Err("save violates the chamber's coordinate inventory, routes or undo history".into());
        }
        // Validate that every history edge really is one legal coordinate exchange.
        let states: Vec<_> =
            state.history.iter().map(|t| &t.positions).chain(std::iter::once(&state.positions)).collect();
        if states.first().is_none_or(|p| **p != c.start) {
            return Err("history must start at the chamber entrance".into());
        }
        for pair in states.windows(2) {
            let changed: Vec<_> = (0..c.start.len()).filter(|i| pair[0][*i] != pair[1][*i]).collect();
            if changed.len() != 2 {
                return Err("invalid exchange history".into());
            }
            let (a, b) = (changed[0], changed[1]);
            if !(0..3).any(|k| {
                let p = preview(c, pair[0], a, b, k);
                p.legal() && pair[1][a] == p.destinations[0] && pair[1][b] == p.destinations[1]
            }) {
                return Err("blocked exchange in save".into());
            }
        }
        *self = state;
        Ok(())
    }
    fn save_tick(&self) -> u64 {
        self.tick
    }
}
