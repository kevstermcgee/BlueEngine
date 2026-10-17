//! Pinstride's rendering-free, fixed-step authority. Positions are lattice points two metres apart;
//! each accepted move is a continuous quarter-circle followed by an automatic anchor handoff.
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use vesper3d::viewer::devkit::{SavePolicy, Simulation, Snapshot, StateHasher};

pub const LENGTH: f32 = 2.;
pub const RADIUS: f32 = 0.18; // encloses the shaft AND the diamond/sphere tips
pub const SWING_TICKS: u8 = 24;
pub const ROOM_COUNT: usize = 8;
pub type Point = [i16; 3];
pub const AXES: [Point; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];
pub fn world(p: Point) -> [f32; 3] {
    p.map(|v| v as f32 * LENGTH)
}
fn add(a: Point, b: Point) -> Point {
    std::array::from_fn(|i| a[i] + b[i])
}
fn sub(a: Point, b: Point) -> Point {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: Point, b: Point) -> i16 {
    (0..3).map(|i| a[i] * b[i]).sum()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pose {
    pub tips: [Point; 2],
    pub pin: usize,
}
impl Pose {
    pub fn directions(self) -> [Point; 4] {
        let d = sub(self.tips[1 - self.pin], self.tips[self.pin]);
        let mut out = [[0; 3]; 4];
        let mut n = 0;
        for a in AXES {
            if dot(a, d) == 0 {
                out[n] = a;
                n += 1;
            }
        }
        out
    }
    pub fn destination(self, choice: usize) -> Self {
        let mut next = self;
        next.tips[1 - self.pin] = add(self.tips[self.pin], self.directions()[choice]);
        next.pin = 1 - self.pin;
        next
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub min: [f32; 3],
    pub max: [f32; 3],
}
impl Block {
    pub fn new(min: [f32; 3], max: [f32; 3]) -> Self {
        Self { min, max }
    }
}
#[derive(Clone, Debug)]
pub struct Chamber {
    pub name: &'static str,
    pub lesson: &'static str,
    pub hint: &'static str,
    pub bounds: Block,
    pub start: Pose,
    pub sockets: [Point; 2],
    pub blocks: Vec<Block>,
}
fn wall_x(x: f32, bottom: f32, top: f32, z0: f32, z1: f32) -> Block {
    Block::new([x - 0.17, bottom, z0], [x + 0.17, top, z1])
}
pub fn chamber(index: usize) -> Chamber {
    let mut c = Chamber {
        name: "Handoff",
        lesson: "A swing gives you a new pin. The old one always releases.",
        hint: "Go up, then right. Watch the anchor change colour.",
        bounds: Block::new([-4.3, -0.3, -4.3], [4.3, 6.3, 4.3]),
        start: Pose { tips: [[-1, 1, 0], [0, 1, 0]], pin: 0 },
        sockets: [[0, 2, 0], [-1, 2, 0]],
        blocks: vec![],
    };
    match index {
        0 => {
            c.bounds = Block::new([-2.3, -0.3, -2.3], [2.3, 4.3, 2.3]);
        }
        1 => {
            c.name = "The window";
            c.lesson = "The shaft and both tips must fit through the window.";
            c.hint = "Approach the opening at mid-height. A pin can float in empty space.";
            c.sockets = [[1, 1, 0], [1, 1, -1]];
            c.blocks = vec![
                wall_x(1., 4.7, 6.3, -4.3, 4.3),
                wall_x(1., -0.3, 4.7, -4.3, -0.65),
                wall_x(1., -0.3, 4.7, 0.65, 4.3),
            ];
        }
        2 => {
            c.name = "False clearance";
            c.lesson = "Clear endpoints can hide a blocked arc. Look for the red contact marker.";
            c.hint = "The amber block catches the upward swing. Use the space behind it.";
            c.sockets = [[1, 2, -1], [0, 2, -1]];
            c.blocks = vec![
                Block::new([-1.1, 2.9, -0.45], [-0.55, 3.5, 0.45]),
                Block::new([0.7, -0.3, 0.65], [1.3, 4.5, 2.5]),
                // A blind corner pocket: entering is legal, but the arriving pin has no exit.
                Block::new([3.65, 1.65, 3.65], [4.3, 2.35, 4.3]),
                Block::new([3.65, -0.3, 1.65], [4.3, 0.35, 2.35]),
            ];
        }
        3 => {
            c.name = "Overpass";
            c.lesson = "Climb above the bulkhead, then descend on its far side.";
            c.hint = "A route over the top needs several alternating pins. There is no gravity.";
            c.sockets = [[1, 1, 0], [2, 1, 0]];
            c.blocks = vec![wall_x(1., -0.3, 3.25, -4.3, 4.3)];
        }
        4 => {
            c.name = "Under / over";
            c.lesson = "One baffle hangs from above. The next grows from below.";
            c.hint = "Low through the first opening; high through the second. Plan two swings ahead.";
            c.start = Pose { tips: [[-2, 1, -1], [-1, 1, -1]], pin: 0 };
            c.sockets = [[1, 2, 1], [2, 2, 1]];
            c.blocks = vec![wall_x(-1., 1.0, 6.3, -4.3, 4.3), wall_x(1., -0.3, 3.1, -4.3, 4.3)];
        }
        5 => {
            c.name = "Dogleg";
            c.lesson = "The two windows sit at different heights and depths.";
            c.hint = "Cross the first wall low and forward, then shift back and climb.";
            c.start = Pose { tips: [[-2, 1, -1], [-1, 1, -1]], pin: 0 };
            c.sockets = [[1, 2, 1], [2, 2, 1]];
            c.blocks = vec![
                wall_x(-1., 4.7, 6.3, -4.3, 4.3),
                wall_x(-1., -0.3, 4.7, -4.3, -2.7),
                wall_x(-1., -0.3, 4.7, -1.3, 4.3),
                wall_x(1., -0.3, 1.3, -4.3, 4.3),
                wall_x(1., 1.3, 6.3, -4.3, 1.3),
                wall_x(1., 1.3, 6.3, 2.7, 4.3),
            ];
        }
        6 => {
            c.name = "Cross current";
            c.lesson = "Turning behind the screen changes which end can reach the sockets.";
            c.hint = "Depth is your escape route. Orbit to see the passage behind the central pillar.";
            c.sockets = [[1, 2, 1], [2, 2, 1]];
            c.start = Pose { tips: [[-2, 1, -1], [-1, 1, -1]], pin: 0 };
            c.blocks = vec![
                Block::new([-0.5, -0.3, -0.5], [0.5, 4.8, 0.5]),
                Block::new([0.7, 2.8, -4.3], [1.3, 6.3, 0.5]),
                Block::new([-4.3, 0.7, 0.7], [-0.7, 1.3, 1.3]),
            ];
        }
        _ => {
            c.name = "Pinstride";
            c.lesson = "Thread the windows, change depth, and dock both colours at once.";
            c.hint = "The first window is low and near; the last is high and far. Undo is always free.";
            c.start = Pose { tips: [[-2, 1, -1], [-1, 1, -1]], pin: 0 };
            c.sockets = [[2, 3, 1], [2, 2, 1]];
            c.blocks = chamber(5).blocks;
            c.blocks.push(Block::new([-0.45, 2.7, -0.6], [0.45, 3.3, 0.6]));
            c.blocks.push(Block::new([2.7, 4.7, -0.4], [4.3, 5.3, 0.4]));
        }
    }
    c
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obstruction {
    pub at: [f32; 3],
    pub block: Option<usize>,
    pub fraction: f32,
}
/// Exact segment vs expanded AABB. The expansion encloses the baton cross section.
pub fn segment_hits(a: [f32; 3], b: [f32; 3], block: Block, r: f32) -> bool {
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for i in 0..3 {
        let d = b[i] - a[i];
        if d.abs() < 1e-7 {
            if a[i] < block.min[i] - r || a[i] > block.max[i] + r {
                return false;
            }
        } else {
            let x = (block.min[i] - r - a[i]) / d;
            let y = (block.max[i] + r - a[i]) / d;
            lo = lo.max(x.min(y));
            hi = hi.min(x.max(y));
            if lo > hi {
                return false;
            }
        }
    }
    true
}
/// Public preview and authority share this path. Sampling uses an angular Lipschitz bound:
/// between adjacent samples every point moves <= LENGTH * half-angle-step. Inflating by
/// that bound makes the sweep conservative, including tiny obstacles between samples.
pub fn obstruction(c: &Chamber, p: Pose, choice: usize) -> Option<Obstruction> {
    if choice >= 4 {
        return Some(Obstruction { at: world(p.tips[p.pin]), block: None, fraction: 0. });
    }
    let pin = world(p.tips[p.pin]);
    let start = world(sub(p.tips[1 - p.pin], p.tips[p.pin]));
    let end = world(p.directions()[choice]);
    const SAMPLES: usize = 96;
    let r = RADIUS + LENGTH * std::f32::consts::FRAC_PI_2 / (2. * SAMPLES as f32);
    for n in 0..=SAMPLES {
        let f = n as f32 / SAMPLES as f32;
        let angle = f * std::f32::consts::FRAC_PI_2;
        let tip = std::array::from_fn(|i| pin[i] + start[i] * angle.cos() + end[i] * angle.sin());
        if (0..3).any(|i| tip[i] - r < c.bounds.min[i] || tip[i] + r > c.bounds.max[i]) {
            return Some(Obstruction { at: tip, block: None, fraction: f });
        }
        for (i, b) in c.blocks.iter().enumerate() {
            if segment_hits(pin, tip, *b, r) {
                // Find the earliest contact along the shaft for a useful visual marker.
                let mut contact = tip;
                for k in 0..=100 {
                    let q = std::array::from_fn(|j| pin[j] + (tip[j] - pin[j]) * k as f32 / 100.);
                    if segment_hits(q, q, *b, r) {
                        contact = q;
                        break;
                    }
                }
                return Some(Obstruction { at: contact, block: Some(i), fraction: f });
            }
        }
    }
    None
}
pub fn pose_clear(c: &Chamber, p: Pose) -> bool {
    if p.pin > 1 || p.tips.iter().flatten().any(|v| i32::from(*v).abs() > 16) {
        return false;
    }
    let d = sub(p.tips[0], p.tips[1]);
    if dot(d, d) != 1 {
        return false;
    }
    let a = world(p.tips[0]);
    let b = world(p.tips[1]);
    !(0..3).any(|i| {
        a[i] - RADIUS < c.bounds.min[i]
            || a[i] + RADIUS > c.bounds.max[i]
            || b[i] - RADIUS < c.bounds.min[i]
            || b[i] + RADIUS > c.bounds.max[i]
    }) && !c.blocks.iter().any(|o| segment_hits(a, b, *o, RADIUS))
}
pub fn docked(c: &Chamber, p: Pose) -> bool {
    p.tips == c.sockets
}
pub fn legal_choices(c: &Chamber, p: Pose) -> [bool; 4] {
    std::array::from_fn(|i| obstruction(c, p, i).is_none())
}
/// Enumerate the finite pose graph. Used for authored chamber verification and the optional hint.
pub fn solve(c: &Chamber, from: Pose) -> Option<Vec<usize>> {
    let mut queue = VecDeque::from([from]);
    let mut prev = HashMap::new();
    prev.insert(from, None);
    while let Some(p) = queue.pop_front() {
        if docked(c, p) {
            let mut route = vec![];
            let mut at = p;
            while let Some((parent, choice)) = prev[&at] {
                route.push(choice);
                at = parent;
            }
            route.reverse();
            return Some(route);
        }
        for i in 0..4 {
            if obstruction(c, p, i).is_some() {
                continue;
            }
            let n = p.destination(i);
            if let std::collections::hash_map::Entry::Vacant(e) = prev.entry(n) {
                e.insert(Some((p, i)));
                queue.push_back(n);
            }
        }
    }
    None
}

/// Intent contains only player requests; presentation cannot move tips or choose the next pin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Intent {
    pub select: Option<usize>,
    pub commit: bool,
    pub undo: bool,
    pub restart: bool,
    pub next: bool,
}
pub type Input = Intent;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swing {
    pub from: Pose,
    pub choice: usize,
    pub elapsed: u8,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Selected,
    Rejected,
    Snapped,
    Solved,
    Undone,
    Restarted,
    ChamberChanged,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimState {
    pub tick: u64,
    pub room: usize,
    pub pose: Pose,
    pub selected: usize,
    pub swing: Option<Swing>,
    pub history: Vec<Pose>,
    pub completed: u8,
    pub rejected: u16,
}
pub struct Sim {
    pub state: SimState,
    events: Vec<Event>,
    room: Chamber,
}
impl Sim {
    pub fn new(_seed: u64) -> Self {
        let room = chamber(0);
        Self {
            state: SimState {
                tick: 0,
                room: 0,
                pose: room.start,
                selected: 0,
                swing: None,
                history: vec![],
                completed: 0,
                rejected: 0,
            },
            events: vec![],
            room,
        }
    }
    pub fn room(&self) -> &Chamber {
        &self.room
    }
    pub fn won(&self) -> bool {
        self.state.swing.is_none() && docked(&self.room, self.state.pose)
    }
    pub fn finished(&self) -> bool {
        self.won() && self.state.room == ROOM_COUNT - 1
    }
    pub fn trapped(&self) -> bool {
        !self.won() && self.state.swing.is_none() && legal_choices(&self.room, self.state.pose).iter().all(|x| !*x)
    }
    pub fn step(&mut self, input: &Intent) {
        self.state.tick += 1;
        if input.restart {
            self.state.pose = self.room.start;
            self.state.swing = None;
            self.state.history.clear();
            self.state.rejected = 0;
            self.events.push(Event::Restarted);
            return;
        }
        if input.undo {
            if let Some(s) = self.state.swing.take() {
                self.state.pose = s.from;
                self.events.push(Event::Undone);
            } else if let Some(p) = self.state.history.pop() {
                self.state.pose = p;
                self.events.push(Event::Undone);
            }
            return;
        }
        if let Some(mut s) = self.state.swing {
            s.elapsed += 1;
            if s.elapsed >= SWING_TICKS {
                self.state.history.push(s.from);
                self.state.pose = s.from.destination(s.choice);
                self.state.swing = None;
                self.events.push(Event::Snapped);
                if self.won() {
                    self.state.completed |= 1 << self.state.room;
                    self.events.push(Event::Solved);
                }
            } else {
                self.state.swing = Some(s);
            }
            return;
        }
        if input.next && self.won() && self.state.room + 1 < ROOM_COUNT {
            self.state.room += 1;
            self.room = chamber(self.state.room);
            self.state.pose = self.room.start;
            self.state.selected = 0;
            self.state.history.clear();
            self.state.rejected = 0;
            self.events.push(Event::ChamberChanged);
            return;
        }
        if let Some(i) = input.select.filter(|i| *i < 4) {
            self.state.selected = i;
            self.events.push(Event::Selected);
        }
        if input.commit && !self.won() {
            if obstruction(&self.room, self.state.pose, self.state.selected).is_some() {
                self.state.rejected = 90;
                self.events.push(Event::Rejected);
            } else {
                self.state.swing = Some(Swing { from: self.state.pose, choice: self.state.selected, elapsed: 0 });
            }
        }
        self.state.rejected = self.state.rejected.saturating_sub(1);
    }
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
}
impl Simulation for Sim {
    type Input = Intent;
    fn step(&mut self, input: &Intent) {
        Sim::step(self, input)
    }
    fn state_hash(&self) -> u64 {
        // Stable explicit fields; history matters because it controls future undo.
        let mut h = StateHasher::new();
        h.u64(self.state.tick)
            .u64(self.state.room as u64)
            .u64(self.state.selected as u64)
            .u32(self.state.completed as u32)
            .u32(self.state.rejected as u32);
        let mut pose = |p: Pose| {
            h.u64(p.pin as u64);
            for a in p.tips {
                for v in a {
                    h.u32(v as i32 as u32);
                }
            }
        };
        pose(self.state.pose);
        for p in &self.state.history {
            pose(*p);
        }
        h.u64(self.state.history.len() as u64).bool(self.state.swing.is_some());
        if let Some(s) = self.state.swing {
            h.u64(s.choice as u64).u32(s.elapsed as u32);
            for a in s.from.tips {
                for v in a {
                    h.u32(v as i32 as u32);
                }
            }
            h.u64(s.from.pin as u64);
        }
        h.finish()
    }
}
impl Snapshot for Sim {
    const KIND: &'static str = "pinstride";
    const POLICY: SavePolicy = SavePolicy::Exact;
    type State = SimState;
    fn capture(&self) -> SimState {
        self.state.clone()
    }
    fn restore(&mut self, s: SimState) -> Result<(), String> {
        if s.room >= ROOM_COUNT || s.selected >= 4 || s.history.len() > 100_000 {
            return Err("Invalid chamber or history".into());
        }
        let c = chamber(s.room);
        if !pose_clear(&c, s.pose) || s.history.iter().any(|p| !pose_clear(&c, *p)) {
            return Err("Invalid baton pose".into());
        }
        // Validate undo history as actual legal public transitions, never arbitrary teleports.
        let mut prior = c.start;
        for (k, p) in s.history.iter().enumerate() {
            if *p != prior {
                return Err("Broken undo history".into());
            }
            // The next pose is checked below against its predecessor.
            let next = if k + 1 < s.history.len() { s.history[k + 1] } else { s.pose };
            if !(0..4).any(|i| p.destination(i) == next && obstruction(&c, *p, i).is_none()) {
                return Err("Illegal saved handoff".into());
            }
            prior = next;
        }
        if s.history.is_empty() && s.pose != c.start {
            return Err("Pose has no route history".into());
        }
        if let Some(a) = s.swing {
            if a.from != s.pose
                || a.choice >= 4
                || a.elapsed >= SWING_TICKS
                || obstruction(&c, a.from, a.choice).is_some()
            {
                return Err("Invalid saved swing".into());
            }
        }
        self.room = c;
        self.state = s;
        self.events.clear();
        Ok(())
    }
    fn save_tick(&self) -> u64 {
        self.state.tick
    }
}

/// Verification uses only the same public intentions as a player. No pose, goal, chamber or
/// completion state is injected. Each resting pose is solved using the very same sweep contract.
pub fn verification_input(sim: &Sim) -> Intent {
    if sim.state.swing.is_some() {
        return Intent::default();
    }
    if sim.won() {
        return Intent { next: true, ..Default::default() };
    }
    let select = solve(sim.room(), sim.state.pose).and_then(|route| route.first().copied());
    Intent { select, commit: select.is_some(), ..Default::default() }
}
