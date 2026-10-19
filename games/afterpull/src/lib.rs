//! Fixed-step, rendering-free puzzle authority. Native devices send the same Intent as tests.
use serde::{Deserialize, Serialize};
use vesper3d::two_d::*;
pub mod focus_gate;
pub mod playtest;
pub mod rooms;
use rooms::ROOMS;
#[cfg(feature = "client")]
mod presentation;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Cell {
    pub x: i32,
    pub y: i32,
}
impl Cell {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
    pub fn offset(self, d: Self) -> Self {
        Self::new(self.x + d.x, self.y + d.y)
    }
}
pub const DIRS: [Cell; 4] = [
    Cell::new(0, -1),
    Cell::new(1, 0),
    Cell::new(0, 1),
    Cell::new(-1, 0),
];
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    pub player: Cell,
    pub crates: Vec<Cell>,
    pub turns: u32,
    pub jams: u32,
    pub multi: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pull {
    pub from: Cell,
    pub to: Cell,
    pub jam: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plan {
    pub origin: Cell,
    pub player: Cell,
    pub pulls: Vec<Pull>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    pub tick: u32,
    pub room: usize,
    pub board: Board,
    pub history: Vec<Board>,
    pub preview: Option<usize>,
    pub last: Option<Plan>,
    pub pulse: u32,
    pub finished: bool,
    pub blocked: u32,
}
pub struct Afterpull {
    pub state: State,
    cues: Vec<usize>,
}
impl Afterpull {
    fn initial(room: usize) -> Board {
        Board {
            player: ROOMS[room].player,
            crates: ROOMS[room].crates.to_vec(),
            turns: 0,
            jams: 0,
            multi: 0,
        }
    }
    pub fn wall(room: usize, c: Cell) -> bool {
        c.x <= 0 || c.x >= 6 || c.y <= 0 || c.y >= 6 || ROOMS[room].walls.contains(&c)
    }
    pub fn failed(&self) -> bool {
        self.state
            .board
            .crates
            .iter()
            .any(|c| ROOMS[self.state.room].pits.contains(c))
    }
    pub fn filled(&self) -> usize {
        ROOMS[self.state.room]
            .docks
            .iter()
            .filter(|c| self.state.board.crates.contains(c))
            .count()
    }
    pub fn cleared(&self) -> bool {
        !self.failed() && self.filled() == ROOMS[self.state.room].docks.len()
    }
    /// A complete immutable read phase. No pull sees another pull's result.
    pub fn plan(&self, dir: usize) -> Option<Plan> {
        let b = &self.state.board;
        let room = self.state.room;
        let player = b.player.offset(*DIRS.get(dir)?);
        if Self::wall(room, player)
            || b.crates.contains(&player)
            || ROOMS[room].pits.contains(&player)
        {
            return None;
        }
        let mut pulls = Vec::new();
        for d in DIRS {
            let mut at = b.player.offset(d);
            while !Self::wall(room, at) && at != player {
                if b.crates.contains(&at) {
                    pulls.push(Pull {
                        from: at,
                        to: at.offset(Cell::new(-d.x, -d.y)),
                        jam: false,
                    });
                    break;
                }
                at = at.offset(d);
            }
        }
        for n in 0..pulls.len() {
            pulls[n].jam = pulls.iter().filter(|p| p.to == pulls[n].to).count() > 1;
        }
        Some(Plan {
            origin: b.player,
            player,
            pulls,
        })
    }
    fn commit(&mut self, dir: usize) {
        let Some(plan) = self.plan(dir) else {
            self.state.blocked = 18;
            return;
        };
        self.state.history.push(self.state.board.clone());
        let dock_arrival = plan
            .pulls
            .iter()
            .any(|p| !p.jam && ROOMS[self.state.room].docks.contains(&p.to));
        let b = &mut self.state.board;
        b.player = plan.player;
        b.turns += 1;
        let moving = plan.pulls.iter().filter(|p| !p.jam).count();
        let jammed = plan.pulls.iter().any(|p| p.jam);
        if jammed {
            b.jams += 1;
        }
        if moving > 1 {
            b.multi += 1;
        }
        // Match each original crate once; destinations never become new sources.
        for c in &mut b.crates {
            if let Some(p) = plan.pulls.iter().find(|p| p.from == *c && !p.jam) {
                *c = p.to;
            }
        }
        self.state.preview = None;
        self.state.last = Some(plan);
        self.state.pulse = 30;
        if self.failed() {
            self.cues.push(1);
        } else if self.cleared() || dock_arrival {
            self.cues.push(2);
        } else if moving > 0 || jammed {
            self.cues.push(0);
        }
    }
    fn undo(&mut self) {
        if let Some(b) = self.state.history.pop() {
            self.state.board = b;
            self.state.finished = false;
            self.state.last = None;
            self.state.pulse = 0;
            self.state.preview = None;
            self.state.blocked = 0;
            self.cues.clear();
        }
    }
    fn next(&mut self) {
        if !self.cleared() {
            return;
        }
        if self.state.room == ROOMS.len() - 1 {
            self.state.finished = true;
            return;
        }
        self.state.room += 1;
        self.state.board = Self::initial(self.state.room);
        self.state.history.clear();
        self.state.preview = None;
        self.state.last = None;
        self.state.pulse = 0;
    }
    /// Explicit edge commands survive native frames with no fixed tick.
    pub fn command(code: i32, arg: i32) -> Intent {
        Intent {
            action: true,
            pointer: Some(Point::new(code, arg)),
            ..Intent::default()
        }
    }
    pub fn direction(dir: usize) -> Intent {
        Self::command(-10, dir as i32)
    }
    /// Preview is part of the command, never a modifier sampled at a later tick.
    pub fn preview(dir: usize) -> Intent {
        Self::command(-14, dir as i32)
    }
    fn validate_board(room: usize, b: &Board) -> bool {
        if Self::wall(room, b.player)
            || ROOMS[room].pits.contains(&b.player)
            || b.crates.len() != ROOMS[room].crates.len()
        {
            return false;
        }
        for (i, c) in b.crates.iter().enumerate() {
            if Self::wall(room, *c) || *c == b.player || b.crates[..i].contains(c) {
                return false;
            }
        }
        true
    }
}
impl GameLogic for Afterpull {
    const ID: &'static str = "afterpull";
    const TITLE: &'static str = "Afterpull";
    const CONTROLS: &'static str =
        "WASD / arrows: step   Space + direction: preview   Z: undo   R: retry room";
    const VERIFY_TICKS: u32 = 1300;
    fn new(_: u64) -> Self {
        Self {
            state: State {
                tick: 0,
                room: 0,
                board: Self::initial(0),
                history: vec![],
                preview: None,
                last: None,
                pulse: 0,
                finished: false,
                blocked: 0,
            },
            cues: vec![],
        }
    }
    fn restart(&mut self) {
        let room = self.state.room;
        let tick = self.state.tick;
        *self = Self::new(7);
        self.state.room = room;
        self.state.board = Self::initial(room);
        self.state.tick = tick;
    }
    fn tick(&self) -> u32 {
        self.state.tick
    }
    fn pointer_target_only_on_press() -> bool {
        true
    }
    fn outcome(&self) -> &'static str {
        if self.state.finished {
            "won"
        } else if self.failed() {
            "lost"
        } else {
            "playing"
        }
    }
    fn probe_input() -> Intent {
        Self::direction(0)
    }
    fn probe_success(&self) -> bool {
        self.state.board.turns > 0
    }
    fn verification_input(tick: u32) -> Intent {
        if !tick.is_multiple_of(20) {
            return Intent::default();
        }
        // Includes a dangerous preview, a real pit loss, undo and room retry,
        // then completes all docks. These are the same commands devices send.
        let route = b"UUURRNURUULRNUUDLUUURRNURULRRDUNUURLLRNRLLURUULpRZTUDDRURLLDLUN";
        match route.get((tick / 20) as usize) {
            Some(b'U') => Self::direction(0),
            Some(b'R') => Self::direction(1),
            Some(b'D') => Self::direction(2),
            Some(b'L') => Self::direction(3),
            Some(b'N') => Self::command(-12, 0),
            Some(b'p') => Self::preview(1),
            Some(b'Z') => Self::command(-11, 0),
            Some(b'T') => Self::command(-13, 0),
            _ => Intent::default(),
        }
    }
    fn take_cues(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.cues)
    }
    fn cue_point(&self, _: usize) -> Point {
        let p = self.state.board.player;
        Point::new(360 + p.x * 44, 92 + p.y * 44)
    }
}
impl Simulation for Afterpull {
    type Input = Intent;
    fn step(&mut self, i: &Intent) {
        self.state.tick = self.state.tick.wrapping_add(1);
        self.state.pulse = self.state.pulse.saturating_sub(1);
        self.state.blocked = self.state.blocked.saturating_sub(1);
        let held = if i.x != 0 {
            Some(if i.x > 0 { 1 } else { 3 })
        } else if i.y != 0 {
            Some(if i.y > 0 { 2 } else { 0 })
        } else {
            None
        };
        let command = if i.action { i.pointer } else { None };
        if command.is_some_and(|p| p.x == -11) {
            self.undo();
            return;
        }
        if command.is_some_and(|p| p.x == -13) {
            self.restart();
            return;
        }
        if self.failed() || self.state.finished {
            return;
        }
        if self.cleared() {
            if command.is_some_and(|p| p.x == -12) {
                self.next();
            }
            return;
        }
        let explicit = command
            .filter(|p| p.x == -10 && (0..4).contains(&p.y))
            .map(|p| p.y as usize);
        let preview = command
            .filter(|p| p.x == -14 && (0..4).contains(&p.y))
            .map(|p| p.y as usize);
        if let Some(d) = preview {
            self.state.preview = Some(d);
            return;
        }
        // An edge retains its original meaning even if Space changes before
        // the shared client's accumulator delivers it to a fixed tick.
        if let Some(d) = explicit {
            self.state.preview = None;
            self.commit(d);
            return;
        }
        if i.sprint {
            self.state.preview = held.or(self.state.preview);
            return;
        }
        self.state.preview = None;
        // Movement requires a press edge. Held axes only select preview direction.
        // Releasing one key in a chord, undoing, or resuming cannot create a step.
        if let Some(d) = if i.action { held } else { None } {
            self.commit(d);
        }
    }
    fn state_hash(&self) -> u64 {
        hash_json(&self.state)
    }
}
impl Snapshot for Afterpull {
    const KIND: &'static str = "afterpull";
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, state: State) -> Result<(), String> {
        if state.room >= ROOMS.len()
            || !Self::validate_board(state.room, &state.board)
            || state
                .history
                .iter()
                .any(|b| !Self::validate_board(state.room, b))
            || state.preview.is_some_and(|d| d >= 4)
            || state.pulse > 30
            || state.blocked > 18
            || state.finished
                && (state.room != 5
                    || !ROOMS[5]
                        .docks
                        .iter()
                        .all(|c| state.board.crates.contains(c)))
        {
            return Err("Save contains an invalid Afterpull room or board".into());
        }
        if let Some(p) = &state.last {
            if Self::wall(state.room, p.origin)
                || Self::wall(state.room, p.player)
                || p.pulls.len() > 4
                || p.pulls
                    .iter()
                    .any(|p| Self::wall(state.room, p.from) || Self::wall(state.room, p.to))
            {
                return Err("Invalid saved suction trace".into());
            }
        }
        self.state = state;
        self.cues.clear();
        Ok(())
    }
}
#[cfg(test)]
mod tests;
