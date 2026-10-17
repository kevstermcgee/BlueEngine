//! Backstop: deterministic integer-grid logistics. Drawing never decides a rule.
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use vesper3d::two_d::*;
mod rooms;
pub use rooms::ROOMS;
#[cfg(feature = "client")]
mod presentation;

pub const UNDO: Rect = Rect::new(486, 344, 124, 32);
pub const RETRY: Rect = Rect::new(624, 344, 124, 32);
pub const NEXT: Rect = Rect::new(486, 298, 262, 34);
const DIRS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
const COMMAND_TICKS: u32 = 12;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Frame {
    pub player: u8,
    pub facing: u8,
    pub crates: Vec<u8>,
    pub moves: u32,
    pub pushes: u32,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    pub tick: u32,
    pub room: usize,
    pub frame: Frame,
    pub history: Vec<Frame>,
    pub deadlocked: bool,
    pub held: Option<u8>,
    pub repeat: u8,
    pub wait_release: bool,
    pub slide: Option<(u8, u8)>,
    pub animation: u8,
    pub rejected: u8,
}
pub struct Backstop {
    pub state: State,
    cues: Vec<usize>,
}
#[derive(Debug, Clone)]
pub struct Preview {
    pub crate_cell: u8,
    pub rear: Vec<u8>,
    pub route: Vec<u8>,
    pub landing: Option<u8>,
    pub obstruction: Option<u8>,
    pub distance: usize,
    pub valid: bool,
    pub crate_backstop: bool,
}
pub fn xy(c: u8) -> (i32, i32) {
    (i32::from(c % 9), i32::from(c / 9))
}
fn offset(c: u8, d: u8, amount: i32) -> Option<u8> {
    let (x, y) = xy(c);
    let (dx, dy) = DIRS[d as usize];
    let (x, y) = (x + dx * amount, y + dy * amount);
    (0..9).contains(&x).then_some(())?;
    (0..7).contains(&y).then_some(())?;
    Some((y * 9 + x) as u8)
}
pub fn tile(room: usize, c: u8) -> u8 {
    let (x, y) = xy(c);
    ROOMS[room].rows[y as usize].as_bytes()[x as usize]
}
fn floor(room: usize, c: u8) -> bool {
    tile(room, c) != b'#'
}
fn initial(room: usize) -> Frame {
    let mut f = Frame {
        player: 0,
        facing: 0,
        crates: vec![],
        moves: 0,
        pushes: 0,
    };
    for c in 0..63 {
        match tile(room, c) {
            b'@' => f.player = c,
            b'$' | b'*' => f.crates.push(c),
            _ => {}
        }
    }
    f
}
fn solved(room: usize, f: &Frame) -> bool {
    f.crates
        .iter()
        .all(|&c| matches!(tile(room, c), b'.' | b'*'))
}
fn preview(room: usize, f: &Frame) -> Option<Preview> {
    let b = offset(f.player, f.facing, 1)?;
    if !f.crates.contains(&b) {
        return None;
    }
    let mut rear = vec![];
    let mut n = 1;
    while let Some(c) = offset(f.player, f.facing, -n) {
        if !floor(room, c) || f.crates.contains(&c) {
            break;
        }
        rear.push(c);
        n += 1;
    }
    let crate_backstop = offset(f.player, f.facing, -n).is_some_and(|c| f.crates.contains(&c));
    let mut route = vec![];
    let mut obstruction = None;
    let distance = rear.len() + 1;
    let mut valid = true;
    for n in 1..=distance {
        match offset(b, f.facing, n as i32) {
            Some(c) => {
                route.push(c);
                if !floor(room, c) || f.crates.contains(&c) {
                    if valid {
                        obstruction = Some(c);
                    }
                    valid = false;
                }
            }
            None => valid = false,
        }
    }
    Some(Preview {
        crate_cell: b,
        rear,
        route,
        landing: offset(b, f.facing, distance as i32),
        obstruction,
        distance,
        valid,
        crate_backstop,
    })
}
fn reachable(room: usize, p: u8, crates: &[u8]) -> [bool; 63] {
    let mut seen = [false; 63];
    seen[p as usize] = true;
    let mut q = VecDeque::from([p]);
    while let Some(a) = q.pop_front() {
        for d in 0..4 {
            if let Some(c) = offset(a, d, 1) {
                if floor(room, c) && !crates.contains(&c) && !seen[c as usize] {
                    seen[c as usize] = true;
                    q.push_back(c);
                }
            }
        }
    }
    seen
}
fn key(crates: &[u8], reach: &[bool; 63]) -> u64 {
    let mut sorted = crates.to_vec();
    sorted.sort_unstable();
    let mut k = reach.iter().position(|&b| b).unwrap() as u64;
    for c in sorted {
        k = (k << 6) | u64::from(c);
    }
    k
}
/// Exact push-space search. Walking regions are equivalent; no heuristic declares loss.
pub fn can_solve(room: usize, frame: &Frame) -> bool {
    let r = reachable(room, frame.player, &frame.crates);
    let mut seen = HashSet::from([key(&frame.crates, &r)]);
    let mut q = VecDeque::from([(frame.clone(), r)]);
    while let Some((f, r)) = q.pop_front() {
        if solved(room, &f) {
            return true;
        }
        for &b in &f.crates {
            for d in 0..4 {
                let Some(p) = offset(b, d, -1) else {
                    continue;
                };
                if !r[p as usize] {
                    continue;
                }
                let mut next = f.clone();
                next.player = p;
                next.facing = d;
                let Some(v) = preview(room, &next) else {
                    continue;
                };
                if !v.valid {
                    continue;
                }
                let at = next.crates.iter().position(|&c| c == b).unwrap();
                next.crates[at] = v.landing.unwrap();
                next.player = b;
                let nr = reachable(room, next.player, &next.crates);
                if seen.insert(key(&next.crates, &nr)) {
                    q.push_back((next, nr));
                }
            }
        }
    }
    false
}
impl Backstop {
    pub fn preview(&self) -> Option<Preview> {
        preview(self.state.room, &self.state.frame)
    }
    pub fn solved(&self) -> bool {
        solved(self.state.room, &self.state.frame)
    }
    pub fn docked(&self) -> usize {
        self.state
            .frame
            .crates
            .iter()
            .filter(|&&c| matches!(tile(self.state.room, c), b'.' | b'*'))
            .count()
    }
    fn remember(&mut self) {
        self.state.history.push(self.state.frame.clone());
    }
    fn reset_room(&mut self) {
        self.state.frame = initial(self.state.room);
        self.state.history.clear();
        self.state.deadlocked = false;
        self.state.slide = None;
        self.state.animation = 0;
        self.state.rejected = 0;
        self.state.held = None;
        self.state.repeat = 0;
        self.state.wait_release = true;
        self.cues.clear();
    }
    fn undo(&mut self) {
        self.state.wait_release = true;
        if let Some(f) = self.state.history.pop() {
            self.state.frame = f;
            self.state.deadlocked = !can_solve(self.state.room, &self.state.frame);
            self.state.slide = None;
            self.state.animation = 0;
            self.state.rejected = 0;
        }
    }
    fn shove(&mut self) {
        let Some(v) = self.preview() else {
            self.state.rejected = 30;
            return;
        };
        if !v.valid {
            self.state.rejected = 45;
            return;
        }
        self.remember();
        let at = self
            .state
            .frame
            .crates
            .iter()
            .position(|&c| c == v.crate_cell)
            .unwrap();
        let landing = v.landing.unwrap();
        self.state.frame.crates[at] = landing;
        self.state.frame.player = v.crate_cell;
        self.state.frame.pushes += 1;
        self.state.frame.moves += 1;
        self.state.slide = Some((v.crate_cell, landing));
        self.state.animation = 8;
        self.state.rejected = 0;
        self.state.deadlocked = !can_solve(self.state.room, &self.state.frame);
        if self.solved() {
            self.cues.push(2);
        } else {
            self.cues.push(0);
        }
    }
}
impl GameLogic for Backstop {
    const ID: &'static str = "backstop";
    const TITLE: &'static str = "Backstop";
    const CONTROLS: &'static str =
        "WASD / arrows walk  |  Space shove  |  Z undo  |  R retry  |  Enter next room";
    const VERIFY_TICKS: u32 = 4608;
    fn new(_: u64) -> Self {
        Self {
            state: State {
                tick: 0,
                room: 0,
                frame: initial(0),
                history: vec![],
                deadlocked: false,
                held: None,
                repeat: 0,
                wait_release: false,
                slide: None,
                animation: 0,
                rejected: 0,
            },
            cues: vec![],
        }
    }
    fn restart(&mut self) {
        self.reset_room();
    }
    fn tick(&self) -> u32 {
        self.state.tick
    }
    fn outcome(&self) -> &'static str {
        if self.state.room == 7 && self.solved() {
            "won"
        } else {
            "playing"
        }
    }
    fn pointer_target_only_on_press() -> bool {
        true
    }
    fn verification_input(tick: u32) -> Intent {
        if !tick.is_multiple_of(COMMAND_TICKS) {
            return Intent::default();
        }
        let mut at = (tick / COMMAND_TICKS) as usize;
        for (r, room) in ROOMS.iter().enumerate() {
            if at < room.route.len() {
                return command(room.route.as_bytes()[at]);
            }
            at -= room.route.len();
            if r < 7 {
                if at == 0 {
                    return command(b'n');
                }
                at -= 1;
            }
        }
        Intent::default()
    }
    fn probe_input() -> Intent {
        command(b'v')
    }
    fn probe_success(&self) -> bool {
        self.state.frame.player != initial(0).player || self.state.frame.pushes > 0
    }
    fn take_cues(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.cues)
    }
}
/// The same public intentions used by keyboard, buttons, tests and native replay.
pub fn command(c: u8) -> Intent {
    match c {
        b'>' => Intent {
            x: 1,
            ..Default::default()
        },
        b'<' => Intent {
            x: -1,
            ..Default::default()
        },
        b'v' => Intent {
            y: 1,
            ..Default::default()
        },
        b'^' => Intent {
            y: -1,
            ..Default::default()
        },
        b'!' => Intent {
            action: true,
            ..Default::default()
        },
        b'z' => Intent {
            action: true,
            pointer: Some(Point::new(540, 360)),
            ..Default::default()
        },
        b'r' => Intent {
            action: true,
            pointer: Some(Point::new(680, 360)),
            ..Default::default()
        },
        b'n' => Intent {
            action: true,
            pointer: Some(Point::new(600, 310)),
            ..Default::default()
        },
        _ => Intent::default(),
    }
}
impl Simulation for Backstop {
    type Input = Intent;
    fn step(&mut self, i: &Intent) {
        self.state.tick = self.state.tick.wrapping_add(1);
        self.state.animation = self.state.animation.saturating_sub(1);
        self.state.rejected = self.state.rejected.saturating_sub(1);
        if i.action {
            if let Some(p) = i.pointer {
                if UNDO.contains(p) {
                    self.undo();
                    return;
                }
                if RETRY.contains(p) {
                    self.reset_room();
                    return;
                }
                if NEXT.contains(p) {
                    if self.solved() && self.state.room < 7 {
                        self.state.room += 1;
                        self.reset_room();
                    }
                    return;
                }
            }
            if !self.solved() {
                self.shove();
            }
            return;
        }
        let dir = if i.x != 0 {
            Some(if i.x > 0 { 0 } else { 2 })
        } else if i.y != 0 {
            Some(if i.y > 0 { 1 } else { 3 })
        } else {
            None
        };
        if self.state.wait_release {
            if dir.is_none() {
                self.state.wait_release = false;
                self.state.held = None;
                self.state.repeat = 0;
            }
            return;
        }
        if dir != self.state.held {
            self.state.held = dir;
            self.state.repeat = 0;
        }
        let Some(d) = dir else {
            return;
        };
        if self.state.repeat > 0 {
            self.state.repeat -= 1;
            return;
        }
        self.state.repeat = 10;
        if self.solved() {
            return;
        }
        let f = &self.state.frame;
        let dest =
            offset(f.player, d, 1).filter(|&c| floor(self.state.room, c) && !f.crates.contains(&c));
        if f.facing != d || dest.is_some() {
            self.remember();
            self.state.frame.facing = d;
            if let Some(c) = dest {
                self.state.frame.player = c;
                self.state.frame.moves += 1;
            }
            self.state.rejected = 0;
        }
    }
    fn state_hash(&self) -> u64 {
        hash_json(&self.state)
    }
}
impl Snapshot for Backstop {
    const KIND: &'static str = "backstop";
    type State = State;
    fn content(&self) -> Option<u64> {
        Some(hash_json(&(
            "backstop-exact-shove-v1",
            ROOMS.iter().map(|r| r.rows).collect::<Vec<_>>(),
        )))
    }
    fn save_tick(&self) -> u64 {
        u64::from(self.state.tick)
    }
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, s: State) -> Result<(), String> {
        if s.room >= ROOMS.len()
            || s.held.is_some_and(|d| d > 3)
            || s.animation > 8
            || s.repeat > 10
            || s.rejected > 45
        {
            return Err("Invalid Backstop save metadata".into());
        }
        let valid = |f: &Frame| {
            f.facing < 4
                && f.player < 63
                && floor(s.room, f.player)
                && f.crates.len() == initial(s.room).crates.len()
                && !f.crates.contains(&f.player)
                && f.crates.iter().all(|&c| c < 63 && floor(s.room, c))
                && f.crates.iter().copied().collect::<HashSet<_>>().len() == f.crates.len()
        };
        if !valid(&s.frame)
            || s.history.iter().any(|f| !valid(f))
            || s.slide
                .is_some_and(|(a, b)| a >= 63 || b >= 63 || !floor(s.room, a) || !floor(s.room, b))
        {
            return Err("Invalid Backstop board in save".into());
        }
        if s.deadlocked == can_solve(s.room, &s.frame) {
            return Err("Inconsistent Backstop deadlock flag".into());
        }
        self.state = s;
        self.cues.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vesper3d::runtime::snapshot;
    fn act(g: &mut Backstop, c: u8) {
        g.step(&command(c));
        for _ in 1..COMMAND_TICKS {
            g.step(&Intent::default());
        }
    }
    #[test]
    fn rear_clearance_sets_exact_distance_and_player_follows() {
        let mut g = Backstop::new(7);
        let v = g.preview().unwrap();
        assert_eq!(v.rear, vec![28]);
        assert_eq!(v.distance, 2);
        assert!(v.valid);
        act(&mut g, b'!');
        assert_eq!(g.state.frame.crates, vec![32]);
        assert_eq!(g.state.frame.player, 30);
        assert!(g.solved());
        act(&mut g, b'z');
        assert_eq!(g.state.frame, initial(0));
        assert!(!g.solved());
        // An immediately adjacent wall is a one-cell backstop.
        g.state.frame.player = 28;
        g.state.frame.crates = vec![29];
        assert_eq!(g.preview().unwrap().distance, 1);
    }
    #[test]
    fn crate_backstop_targets_are_floor_and_no_chain_push() {
        let mut g = Backstop::new(7);
        g.state.frame.player = 31;
        g.state.frame.crates = vec![29, 32];
        let v = g.preview().unwrap();
        assert_eq!(v.rear, vec![30]);
        assert_eq!(v.distance, 2);
        assert!(v.crate_backstop);
        act(&mut g, b'!');
        assert_eq!(g.state.frame.crates, vec![29, 34]);
        assert_eq!(g.state.frame.player, 32);
        // Docked crate on cell 32 blocks the rear scan exactly like any other crate.
        g.state.frame.player = 33;
        g.state.frame.crates = vec![32, 34];
        g.state.frame.facing = 0;
        let v = g.preview().unwrap();
        assert_eq!(v.distance, 1);
        assert!(v.crate_backstop);
        // Forward crate collision rejects the whole shove, never chain-pushes.
        g.state.frame.player = 29;
        g.state.frame.crates = vec![30, 31];
        g.state.frame.facing = 0;
        let before = g.state.frame.clone();
        act(&mut g, b'!');
        assert_eq!(g.state.frame, before);
        assert!(g.state.history.is_empty() || g.state.history.len() == 1);
        assert!(!g.preview().unwrap().valid);
    }
    #[test]
    fn obstruction_rejects_entire_route_without_penalty() {
        let mut g = Backstop::new(7);
        g.state.room = 1;
        g.state.frame.player = 31;
        g.state.frame.crates = vec![32];
        let v = g.preview().unwrap();
        assert_eq!(v.distance, 4);
        assert_eq!(v.obstruction, Some(35));
        let before = g.state.frame.clone();
        let h = g.state.history.len();
        act(&mut g, b'!');
        assert_eq!(g.state.frame, before);
        assert_eq!(g.state.history.len(), h);
        assert!(g.state.rejected > 0);
        // Turning against a crate is allowed; walking never pushes it.
        g = Backstop::new(7);
        act(&mut g, b'>');
        assert_eq!(g.state.frame, initial(0));
    }
    #[test]
    fn exact_deadlock_detection_undo_and_room_retry() {
        let mut g = Backstop::new(7);
        // Launch down into the boundary row. The crate can never leave that edge.
        g.state.frame.player = 21;
        g.state.frame.crates = vec![30];
        g.state.frame.facing = 1;
        assert!(can_solve(0, &g.state.frame));
        act(&mut g, b'!');
        assert_eq!(g.state.frame.crates, vec![48]);
        assert!(g.state.deadlocked);
        assert_eq!(g.outcome(), "playing"); // Recoverable loss, no modal blocks Undo.
        act(&mut g, b'z');
        assert!(!g.state.deadlocked);
        assert_eq!(g.state.frame.crates, vec![30]);
        g.state.room = 3;
        g.reset_room();
        act(&mut g, b'>');
        g.restart();
        assert_eq!(g.state.room, 3);
        assert_eq!(g.state.frame, initial(3));
        assert!(g.state.history.is_empty());
    }
    #[test]
    fn eight_rooms_use_backstops_reuse_and_undock_deliveries() {
        let mut g = Backstop::new(7);
        let mut helpers = 0;
        let mut undocks = 0;
        let mut largest = 0;
        for (r, room) in ROOMS.iter().enumerate() {
            assert_eq!(g.state.room, r);
            assert!(can_solve(r, &g.state.frame));
            let mut count = 0;
            for &c in room.route.as_bytes() {
                if c == b'!' {
                    if let Some(v) = g.preview() {
                        assert!(v.valid, "room {r}");
                        helpers += usize::from(v.crate_backstop);
                        if matches!(tile(r, v.crate_cell), b'.' | b'*') {
                            undocks += 1;
                        }
                        count += 1;
                    }
                }
                act(&mut g, c);
            }
            largest = largest.max(count);
            assert!(g.solved(), "Room {} did not solve", r + 1);
            if r < 7 {
                act(&mut g, b'n');
            }
        }
        assert!(helpers >= 10);
        assert!(undocks >= 1);
        assert!(largest >= 8);
        assert_eq!(g.outcome(), "won");
    }
    #[test]
    fn real_public_route_is_deterministic_and_save_resumes_exactly() {
        let inputs: Vec<_> = (0..Backstop::VERIFY_TICKS)
            .map(Backstop::verification_input)
            .collect();
        vesper3d::runtime::assert_deterministic(|| Backstop::new(7), &inputs);
        snapshot::assert_resumes_exactly(|| Backstop::new(7), &inputs, 379);
        let (hash, outcome) = verify::<Backstop>();
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
    fn snapshots_preserve_undo_reject_invalid_and_resume_sliding() {
        let mut g = Backstop::new(7);
        g.step(&command(b'!'));
        assert_eq!(g.state.animation, 8);
        let state = g.capture();
        let mut loaded = Backstop::new(88);
        loaded.restore(state).unwrap();
        assert_eq!(loaded.state_hash(), g.state_hash());
        act(&mut loaded, b'z');
        assert_eq!(loaded.state.frame, initial(0));
        let hash = loaded.state_hash();
        let mut bad = loaded.capture();
        bad.frame.player = 255;
        assert!(loaded.restore(bad).is_err());
        assert_eq!(loaded.state_hash(), hash);
        let mut bad = loaded.capture();
        bad.room = 8;
        assert!(loaded.restore(bad).is_err());
        let mut bad = loaded.capture();
        bad.frame.crates = vec![0];
        assert!(loaded.restore(bad).is_err());
    }
    #[test]
    fn undo_and_retry_wait_for_held_direction_to_release() {
        let mut g = Backstop::new(7);
        g.step(&command(b'v'));
        assert_eq!(g.state.frame.player, 38);
        g.step(&command(b'z'));
        assert_eq!(g.state.frame.player, 29);
        for _ in 0..30 {
            g.step(&command(b'v'));
        }
        assert_eq!(g.state.frame.player, 29);
        g.step(&Intent::default());
        g.step(&command(b'v'));
        assert_eq!(g.state.frame.player, 38);
        g.restart();
        for _ in 0..30 {
            g.step(&command(b'v'));
        }
        assert_eq!(g.state.frame.player, 29);
        g.step(&Intent::default());
        g.step(&command(b'v'));
        assert_eq!(g.state.frame.player, 38);
    }
    #[test]
    fn held_controls_repeat_but_single_tap_is_one_cell() {
        let mut g = Backstop::new(7);
        act(&mut g, b'v');
        assert_eq!(g.state.frame.player, 38);
        for _ in 0..40 {
            g.step(&command(b'v'));
        }
        assert_eq!(g.state.frame.player, 47);
        act(&mut g, b'r');
        assert_eq!(g.state.frame, initial(0));
        g.step(&Backstop::probe_input());
        assert!(g.probe_success());
    }
}
