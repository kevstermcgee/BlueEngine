//! Peelturn's integer, fixed-step authority. Presentation never changes the board.
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use vesper3d::two_d::*;

const SIZE: i32 = 6;
const DIRS: [[i32; 2]; 4] = [[1, 0], [0, 1], [-1, 0], [0, -1]];
const SLIDE: u8 = 5;
const IMPACT: u8 = 20;
pub const CYCLE: i32 = 10;
pub const UNDO: i32 = 11;
pub const HELP: i32 = 12;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Packet {
    pub cell: [i32; 2],
    pub beads: u8,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Level {
    pub name: String,
    pub tip: String,
    pub walls: Vec<[i32; 2]>,
    pub packets: Vec<Packet>,
    pub sockets: Vec<[i32; 2]>,
    pub solution: Vec<[usize; 2]>,
}
pub fn levels() -> &'static [Level] {
    static DATA: OnceLock<Vec<Level>> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/boards.json")).expect("authored boards")
    })
}
fn adjacent(p: [i32; 2], dir: usize) -> [i32; 2] {
    [p[0] + DIRS[dir][0], p[1] + DIRS[dir][1]]
}
fn in_bounds(p: [i32; 2]) -> bool {
    (0..SIZE).contains(&p[0]) && (0..SIZE).contains(&p[1])
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Deposit {
    pub cell: [i32; 2],
    pub packet: usize,
    pub impact: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Flight {
    pub cell: [i32; 2],
    pub dir: usize,
    pub left: u8,
    pub packet: usize,
    pub progress: u8,
    pub wait: u8,
    pub impact: u8,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResultState {
    Playing,
    Clear,
    Jammed,
    EmptySockets,
    Trapped,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Board {
    pub alive: Vec<bool>,
    pub deposits: Vec<Deposit>,
    pub selected: usize,
    pub dir: usize,
    pub flight: Option<Flight>,
    pub result: ResultState,
    pub launches: u8,
    pub rejected: u8,
}
impl Board {
    fn new(level: &Level) -> Self {
        Self {
            alive: vec![true; level.packets.len()],
            deposits: vec![],
            selected: 0,
            dir: 0,
            flight: None,
            result: ResultState::Playing,
            launches: 0,
            rejected: 0,
        }
    }
    fn blocked(&self, level: &Level, cell: [i32; 2]) -> bool {
        !in_bounds(cell)
            || level.walls.contains(&cell)
            || self.deposits.iter().any(|b| b.cell == cell)
            || level
                .packets
                .iter()
                .enumerate()
                .any(|(i, p)| self.alive[i] && p.cell == cell)
    }
    fn first_impact(&self, level: &Level) -> Option<[i32; 2]> {
        if self.flight.is_some() || !self.alive[self.selected] {
            return None;
        }
        let mut p = level.packets[self.selected].cell;
        if self.blocked(level, adjacent(p, self.dir)) {
            return None;
        }
        while !self.blocked(level, adjacent(p, self.dir)) {
            p = adjacent(p, self.dir);
        }
        Some(p)
    }
    fn filled(&self, level: &Level) -> usize {
        level
            .sockets
            .iter()
            .filter(|p| self.deposits.iter().any(|d| d.cell == **p))
            .count()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State {
    pub tick: u32,
    pub level: usize,
    pub board: Board,
    pub history: Vec<Board>,
    pub help: bool,
}
pub struct Peelturn {
    pub state: State,
    cues: Vec<usize>,
}
impl Peelturn {
    pub fn level(&self) -> &'static Level {
        &levels()[self.state.level]
    }
    fn begin_level(&mut self, index: usize) {
        self.state.level = index;
        self.state.board = Board::new(&levels()[index]);
        self.state.history.clear();
        self.state.help = false;
    }
    fn cycle(&mut self) {
        let b = &mut self.state.board;
        for offset in 1..=b.alive.len() {
            let next = (b.selected + offset) % b.alive.len();
            if b.alive[next] {
                b.selected = next;
                break;
            }
        }
    }
    fn launch(&mut self) {
        if self.state.board.first_impact(self.level()).is_none() {
            self.state.board.rejected = 50;
            self.cues.push(1);
            return;
        }
        self.state.history.push(self.state.board.clone());
        let l = self.level();
        let b = &mut self.state.board;
        let p = &l.packets[b.selected];
        b.alive[b.selected] = false;
        b.launches += 1;
        b.rejected = 0;
        b.flight = Some(Flight {
            cell: p.cell,
            dir: b.dir,
            left: p.beads,
            packet: b.selected,
            progress: 0,
            wait: 0,
            impact: 0,
        });
    }
    fn finish_launch(&mut self) {
        let l = self.level();
        let b = &mut self.state.board;
        if b.alive.iter().all(|a| !a) {
            b.result = if b.filled(l) == l.sockets.len() {
                ResultState::Clear
            } else {
                ResultState::EmptySockets
            };
            self.cues
                .push(if b.result == ResultState::Clear { 2 } else { 1 });
        } else if !l
            .packets
            .iter()
            .enumerate()
            .any(|(i, p)| b.alive[i] && (0..4).any(|dir| !b.blocked(l, adjacent(p.cell, dir))))
        {
            b.result = ResultState::Trapped;
            self.cues.push(1);
        } else {
            self.cycle();
        }
    }
    fn advance_flight(&mut self) {
        let l = self.level();
        let mut f = self.state.board.flight.take().expect("active flight");
        if f.wait > 0 {
            f.wait -= 1;
            if f.wait == 0 {
                if f.left == 0 {
                    self.finish_launch();
                    return;
                }
                if self.state.board.blocked(l, adjacent(f.cell, f.dir)) {
                    self.state.board.result = ResultState::Jammed;
                    self.state.board.flight = Some(f);
                    self.cues.push(1);
                    return;
                }
            }
        } else {
            let next = adjacent(f.cell, f.dir);
            if self.state.board.blocked(l, next) {
                f.left -= 1;
                f.impact += 1;
                f.wait = IMPACT;
                f.progress = 0;
                self.state.board.deposits.push(Deposit {
                    cell: f.cell,
                    packet: f.packet,
                    impact: f.impact,
                });
                if f.left > 0 {
                    f.dir = (f.dir + 1) % 4;
                }
                self.cues.push(0);
            } else {
                f.progress += 1;
                if f.progress == SLIDE {
                    f.cell = next;
                    f.progress = 0;
                }
            }
        }
        self.state.board.flight = Some(f);
    }
    pub fn command(code: i32) -> Intent {
        Intent {
            pointer: Some(Point::new(-code, -1)),
            action: true,
            ..Intent::default()
        }
    }
    pub fn aim(dir: usize) -> Intent {
        Intent {
            x: DIRS[dir][0],
            y: DIRS[dir][1],
            ..Intent::default()
        }
    }
}
impl GameLogic for Peelturn {
    const ID: &'static str = "peelturn";
    const TITLE: &'static str = "Peelturn";
    const CONTROLS: &'static str = "Tab packet / Arrows aim / Space launch / Backspace undo";
    const VERIFY_TICKS: u32 = 22000;
    fn new(_: u64) -> Self {
        Self {
            state: State {
                tick: 0,
                level: 0,
                board: Board::new(&levels()[0]),
                history: vec![],
                help: false,
            },
            cues: vec![],
        }
    }
    fn restart(&mut self) {
        self.begin_level(self.state.level);
        self.cues.clear();
    }
    fn pointer_target_only_on_press() -> bool {
        true
    }
    fn tick(&self) -> u32 {
        self.state.tick
    }
    fn outcome(&self) -> &'static str {
        match self.state.board.result {
            ResultState::Clear if self.state.level == levels().len() - 1 => "won",
            ResultState::Jammed | ResultState::EmptySockets | ResultState::Trapped => "lost",
            _ => "playing",
        }
    }
    fn probe_input() -> Intent {
        Self::aim(1)
    }
    fn probe_success(&self) -> bool {
        self.state.board.dir == 1
    }
    fn verification_input(tick: u32) -> Intent {
        // Only public commands: select a packet, aim, launch, wait, next board.
        // Each launch has 500 ticks, longer than the longest possible route.
        let mut t = tick;
        for level in levels() {
            let span = level.solution.len() as u32 * 500 + 20;
            if t < span {
                let index = (t / 500) as usize;
                if index == level.solution.len() {
                    return if t % 500 == 1 {
                        Intent {
                            action: true,
                            ..Intent::default()
                        }
                    } else {
                        Intent::default()
                    };
                }
                let [packet, dir] = level.solution[index];
                return match t % 500 {
                    0 => Intent {
                        pointer: Some(packet_center(level.packets[packet].cell)),
                        action: true,
                        ..Intent::default()
                    },
                    1 => Self::aim(dir),
                    2 => Intent {
                        action: true,
                        ..Intent::default()
                    },
                    _ => Intent::default(),
                };
            }
            t -= span;
        }
        Intent::default()
    }
    fn take_cues(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.cues)
    }
    fn cue_point(&self, _: usize) -> Point {
        self.state
            .board
            .deposits
            .last()
            .map(|d| packet_center(d.cell))
            .unwrap_or(Point::new(400, 220))
    }
}
impl Simulation for Peelturn {
    type Input = Intent;
    fn step(&mut self, input: &Intent) {
        self.state.tick = self.state.tick.wrapping_add(1);
        self.state.board.rejected = self.state.board.rejected.saturating_sub(1);
        if input.action && input.pointer == Some(Point::new(-HELP, -1)) {
            self.state.help = !self.state.help;
            return;
        }
        if self.state.help {
            return;
        }
        if input.action && input.pointer == Some(Point::new(-UNDO, -1)) {
            if let Some(b) = self.state.history.pop() {
                self.state.board = b;
                self.cues.clear();
            }
            return;
        }
        if self.state.board.result == ResultState::Clear {
            if input.action
                && input
                    .pointer
                    .is_none_or(|p| Rect::new(395, 337, 352, 32).contains(p))
                && self.state.level + 1 < levels().len()
            {
                self.begin_level(self.state.level + 1);
            }
            return;
        }
        if self.state.board.result != ResultState::Playing {
            return;
        }
        if self.state.board.flight.is_some() {
            self.advance_flight();
            return;
        }
        if input.action && input.pointer == Some(Point::new(-CYCLE, -1)) {
            self.cycle();
            return;
        }
        if input.x.abs() <= 1 && input.y.abs() <= 1 {
            if input.x != 0 {
                self.state.board.dir = if input.x > 0 { 0 } else { 2 };
            } else if input.y != 0 {
                self.state.board.dir = if input.y > 0 { 1 } else { 3 };
            }
        }
        if input.action {
            if let Some(p) = input.pointer {
                if let Some(index) =
                    self.level()
                        .packets
                        .iter()
                        .enumerate()
                        .position(|(i, packet)| {
                            self.state.board.alive[i] && cell_rect(packet.cell).contains(p)
                        })
                {
                    self.state.board.selected = index;
                    return;
                }
                if let Some(index) = (0..self.state.board.alive.len()).find(|i| {
                    self.state.board.alive[*i]
                        && Rect::new(395 + *i as i32 * 85, 209, 76, 49).contains(p)
                }) {
                    self.state.board.selected = index;
                    return;
                }
                if Rect::new(395, 337, 352, 32).contains(p) {
                    self.launch();
                }
            } else {
                self.launch();
            }
        }
    }
    fn state_hash(&self) -> u64 {
        hash_json(&self.state)
    }
}
impl Snapshot for Peelturn {
    const KIND: &'static str = "peelturn";
    type State = State;
    fn content(&self) -> Option<u64> {
        Some(
            StateHasher::new()
                .bytes(include_bytes!("../assets/boards.json"))
                .finish(),
        )
    }
    fn save_tick(&self) -> u64 {
        self.state.tick as u64
    }
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, state: State) -> Result<(), String> {
        let level = levels().get(state.level).ok_or("Invalid board number")?;
        if state.history.len() > level.packets.len()
            || state.history.len() != state.board.launches as usize
            || state
                .history
                .iter()
                .any(|b| b.flight.is_some() || b.result != ResultState::Playing)
        {
            return Err("Invalid undo history".into());
        }
        for b in std::iter::once(&state.board).chain(state.history.iter()) {
            if b.alive.len() != level.packets.len()
                || b.selected >= b.alive.len()
                || b.dir > 3
                || b.launches as usize != b.alive.iter().filter(|a| !**a).count()
                || b.deposits.len() > 32
                || b.rejected > 50
            {
                return Err("Invalid board state".into());
            }
            let mut seen = Vec::new();
            for d in &b.deposits {
                if !in_bounds(d.cell)
                    || level.walls.contains(&d.cell)
                    || seen.contains(&d.cell)
                    || d.packet >= b.alive.len()
                    || d.impact == 0
                    || d.impact > level.packets[d.packet].beads
                    || b.alive[d.packet]
                    || level
                        .packets
                        .iter()
                        .enumerate()
                        .any(|(i, p)| b.alive[i] && p.cell == d.cell)
                {
                    return Err("Invalid deposited bead".into());
                }
                seen.push(d.cell);
            }
            if let Some(f) = &b.flight {
                if !in_bounds(f.cell)
                    || f.dir > 3
                    || f.packet >= b.alive.len()
                    || b.alive[f.packet]
                    || f.left > level.packets[f.packet].beads
                    || f.impact > level.packets[f.packet].beads
                    || f.impact + f.left != level.packets[f.packet].beads
                    || f.wait > IMPACT
                    || f.progress >= SLIDE
                    || level.walls.contains(&f.cell)
                    || (f.left == 0 && f.wait == 0)
                    || (f.wait > 0 && (f.impact == 0 || f.progress > 0))
                    || (f.progress > 0 && b.blocked(level, adjacent(f.cell, f.dir)))
                    || level
                        .packets
                        .iter()
                        .enumerate()
                        .any(|(i, p)| b.alive[i] && p.cell == f.cell)
                {
                    return Err("Invalid moving packet".into());
                }
            }
            for (id, packet) in level.packets.iter().enumerate() {
                let impacts = b.deposits.iter().filter(|d| d.packet == id).count();
                let expected = if b.alive[id] {
                    0
                } else {
                    b.flight
                        .as_ref()
                        .filter(|f| f.packet == id)
                        .map_or(packet.beads, |f| f.impact) as usize
                };
                if impacts != expected {
                    return Err("Bead conservation failed".into());
                }
            }
        }
        self.state = state;
        self.cues.clear();
        Ok(())
    }
}
const ORIGIN: [i32; 2] = [47, 77];
const TILE: i32 = 48;
fn cell_rect(p: [i32; 2]) -> Rect {
    Rect::new(ORIGIN[0] + p[0] * TILE, ORIGIN[1] + p[1] * TILE, TILE, TILE)
}
fn packet_center(p: [i32; 2]) -> Point {
    let r = cell_rect(p);
    Point::new(r.x + TILE / 2, r.y + TILE / 2)
}

#[cfg(feature = "client")]
mod presentation;
#[cfg(test)]
mod tests;
