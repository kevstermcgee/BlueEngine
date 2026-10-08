//! Rendering-free corridor reflection. The shared native client supplies fixed-step Intent input.
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use vesper3d::two_d::*;
mod rooms;
pub use rooms::ROOMS;
#[cfg(feature = "client")]
mod presentation;

pub const CELL: i32 = 48;
pub const HORIZONTAL: Rect = Rect::new(405, 220, 170, 42);
pub const VERTICAL: Rect = Rect::new(589, 220, 170, 42);
pub const UNDO: Rect = Rect::new(405, 274, 110, 38);
pub const RESTART: Rect = Rect::new(527, 274, 110, 38);
pub const NEXT: Rect = Rect::new(405, 330, 354, 40);
pub const EXCHANGE_TICKS: u8 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    Horizontal,
    Vertical,
}
impl Axis {
    fn delta(self) -> (i32, i32) {
        match self {
            Self::Horizontal => (1, 0),
            Self::Vertical => (0, 1),
        }
    }
    pub fn button(self) -> Rect {
        match self {
            Self::Horizontal => HORIZONTAL,
            Self::Vertical => VERTICAL,
        }
    }
}

pub struct Room {
    pub name: &'static str,
    pub hint: [&'static str; 2],
    pub rows: &'static [&'static str],
    pub start: &'static [u8],
    pub goals: &'static [u8],
    pub solution: &'static [(usize, Axis)],
}
impl Room {
    pub fn width(&self) -> i32 {
        self.rows[0].len() as i32
    }
    pub fn height(&self) -> i32 {
        self.rows.len() as i32
    }
    pub fn floor(&self, x: i32, y: i32) -> bool {
        x >= 0
            && y >= 0
            && x < self.width()
            && y < self.height()
            && self.rows[y as usize].as_bytes()[x as usize] != b'#'
    }
    pub fn xy(&self, cell: u8) -> (i32, i32) {
        (
            i32::from(cell) % self.width(),
            i32::from(cell) / self.width(),
        )
    }
    pub fn origin(&self) -> Point {
        Point::new(58 + (6 - self.width()) * 24, 82 + (6 - self.height()) * 24)
    }
    pub fn center(&self, cell: u8) -> Point {
        let (x, y) = self.xy(cell);
        let o = self.origin();
        Point::new(o.x + x * CELL + CELL / 2, o.y + y * CELL + CELL / 2)
    }
    fn cell_at(&self, point: Point) -> Option<u8> {
        let o = self.origin();
        let x = point.x - o.x;
        let y = point.y - o.y;
        if x < 0 || y < 0 || x >= self.width() * CELL || y >= self.height() * CELL {
            return None;
        }
        Some(((y / CELL) * self.width() + x / CELL) as u8)
    }
    fn valid_positions(&self, positions: &[u8]) -> bool {
        positions.len() == self.start.len()
            && positions.iter().enumerate().all(|(i, &p)| {
                let (x, y) = self.xy(p);
                self.floor(x, y) && !positions[..i].contains(&p)
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Corridor {
    pub destination: u8,
    pub before: u8,
    pub after: u8,
}
/// Bounds are floor edges, walls and every other token. Goals deliberately have no role.
pub fn corridor(room: &Room, tokens: &[u8], token: usize, axis: Axis) -> Corridor {
    let p = tokens[token];
    let (x, y) = room.xy(p);
    let (dx, dy) = axis.delta();
    let free = |x: i32, y: i32| {
        room.floor(x, y)
            && !tokens
                .iter()
                .enumerate()
                .any(|(i, &p)| i != token && room.xy(p) == (x, y))
    };
    let mut before = 0;
    let mut after = 0;
    while free(x - (before + 1) * dx, y - (before + 1) * dy) {
        before += 1;
    }
    while free(x + (after + 1) * dx, y + (after + 1) * dy) {
        after += 1;
    }
    // Equivalent to a+b-p, without assuming the corridor starts at coordinate zero.
    let destination =
        ((y + (after - before) * dy) * room.width() + x + (after - before) * dx) as u8;
    Corridor {
        destination,
        before: before as u8,
        after: after as u8,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exchange {
    pub token: usize,
    pub from: u8,
    pub to: u8,
    pub axis: Axis,
    pub before: u8,
    pub after: u8,
    pub remaining: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub tick: u32,
    pub room: usize,
    pub tokens: Vec<u8>,
    pub selected: Option<usize>,
    pub axis: Option<Axis>,
    pub history: Vec<Vec<u8>>,
    pub exchange: Option<Exchange>,
    pub finished: bool,
}
pub struct GapFlip {
    pub state: State,
    cues: Vec<usize>,
}
impl GapFlip {
    pub fn room(&self) -> &'static Room {
        &ROOMS[self.state.room]
    }
    pub fn solved(&self) -> bool {
        self.state.tokens == self.room().goals
    }
    pub fn preview(&self) -> Option<Corridor> {
        Some(corridor(
            self.room(),
            &self.state.tokens,
            self.state.selected?,
            self.state.axis?,
        ))
    }
    fn enter_room(&mut self, room: usize) {
        self.state.room = room;
        self.state.tokens = ROOMS[room].start.to_vec();
        self.state.selected = None;
        self.state.axis = None;
        self.state.history.clear();
        self.state.exchange = None;
        self.state.finished = false;
    }
    fn undo(&mut self) {
        if let Some(previous) = self.state.history.pop() {
            self.state.tokens = previous;
            self.state.axis = None;
            self.state.exchange = None;
            self.state.finished = false;
        }
    }
    fn click(&mut self, point: Point) {
        if UNDO.contains(point) {
            self.undo();
            return;
        }
        if RESTART.contains(point) {
            self.restart();
            return;
        }
        if NEXT.contains(point) && self.solved() {
            if self.state.room + 1 == ROOMS.len() {
                self.state.finished = true;
                self.cues.push(2);
            } else {
                self.enter_room(self.state.room + 1);
            }
            return;
        }
        if self.state.finished {
            return;
        }
        if let Some(cell) = self.room().cell_at(point) {
            if let Some(token) = self.state.tokens.iter().position(|&p| p == cell) {
                self.state.selected = Some(token);
                self.state.axis = None;
                return;
            }
            if let (Some(token), Some(axis), Some(gap)) =
                (self.state.selected, self.state.axis, self.preview())
            {
                if cell == gap.destination && gap.before != gap.after {
                    let from = self.state.tokens[token];
                    self.state.history.push(self.state.tokens.clone());
                    self.state.tokens[token] = cell;
                    self.state.exchange = Some(Exchange {
                        token,
                        from,
                        to: cell,
                        axis,
                        before: gap.before,
                        after: gap.after,
                        remaining: EXCHANGE_TICKS,
                    });
                    self.state.axis = None;
                    self.cues.push(if self.solved() { 2 } else { 0 });
                }
            }
            return;
        }
        if let Some(token) = self.state.selected {
            for axis in [Axis::Horizontal, Axis::Vertical] {
                if axis.button().contains(point) {
                    let gap = corridor(self.room(), &self.state.tokens, token, axis);
                    if gap.before != gap.after {
                        self.state.axis = Some(axis);
                        self.state.exchange = None;
                    }
                }
            }
        }
    }
}
impl GameLogic for GapFlip {
    const ID: &'static str = "gap-flip";
    const TITLE: &'static str = "Gap Flip";
    const CONTROLS: &'static str =
        "Click token > axis > ghost   |   Undo freely. Match every symbol.";
    const VERIFY_TICKS: u32 = 5300;
    fn new(_: u64) -> Self {
        Self {
            state: State {
                tick: 0,
                room: 0,
                tokens: ROOMS[0].start.to_vec(),
                selected: None,
                axis: None,
                history: vec![],
                exchange: None,
                finished: false,
            },
            cues: vec![],
        }
    }
    fn restart(&mut self) {
        self.enter_room(self.state.room);
        self.cues.clear();
    }
    fn pointer_target_only_on_press() -> bool {
        true
    }
    fn tick(&self) -> u32 {
        self.state.tick
    }
    fn outcome(&self) -> &'static str {
        if self.state.finished {
            "won"
        } else {
            "playing"
        }
    }
    fn verification_input(tick: u32) -> Intent {
        verification_route()
            .get(tick as usize)
            .copied()
            .unwrap_or_default()
    }
    fn probe_input() -> Intent {
        click(ROOMS[0].center(ROOMS[0].start[0]))
    }
    fn probe_success(&self) -> bool {
        self.state.selected == Some(0)
    }
    fn take_cues(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.cues)
    }
    fn cue_point(&self, _: usize) -> Point {
        self.state
            .selected
            .map(|i| self.room().center(self.state.tokens[i]))
            .unwrap_or(Point::new(210, 220))
    }
}
impl Simulation for GapFlip {
    type Input = Intent;
    fn step(&mut self, input: &Intent) {
        self.state.tick = self.state.tick.saturating_add(1);
        if let Some(exchange) = &mut self.state.exchange {
            exchange.remaining = exchange.remaining.saturating_sub(1);
            if exchange.remaining == 0 {
                self.state.exchange = None;
            }
        }
        if input.action {
            if let Some(point) = input.pointer {
                self.click(point);
            }
        }
    }
    fn state_hash(&self) -> u64 {
        hash_json(&self.state)
    }
}
impl Snapshot for GapFlip {
    const KIND: &'static str = "gap-flip";
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, state: State) -> Result<(), String> {
        let room = ROOMS.get(state.room).ok_or("Unknown Gap Flip room")?;
        if !room.valid_positions(&state.tokens)
            || state.selected.is_some_and(|i| i >= state.tokens.len())
            || (state.axis.is_some() && state.selected.is_none())
            || (state.finished && (state.room + 1 != ROOMS.len() || state.tokens != room.goals))
        {
            return Err("Invalid room, token or selection in save".into());
        }
        let mut previous = room.start;
        for next in state
            .history
            .iter()
            .skip(1)
            .map(Vec::as_slice)
            .chain(std::iter::once(state.tokens.as_slice()))
        {
            if !room.valid_positions(next) {
                return Err("Invalid undo position".into());
            }
            if state.history.is_empty() {
                if next != room.start {
                    return Err("Missing undo history".into());
                }
                break;
            }
            if state.history[0] != room.start {
                return Err("Undo history must start at the room entrance".into());
            }
            let changed: Vec<_> = previous
                .iter()
                .zip(next)
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .map(|(i, _)| i)
                .collect();
            if changed.len() != 1
                || ![Axis::Horizontal, Axis::Vertical].iter().any(|&axis| {
                    corridor(room, previous, changed[0], axis).destination == next[changed[0]]
                })
            {
                return Err("Undo history contains a non-flip move".into());
            }
            previous = next;
        }
        if let Some(e) = &state.exchange {
            let old = state
                .history
                .last()
                .ok_or("Exchange has no committed move")?;
            if e.token >= state.tokens.len()
                || e.remaining == 0
                || e.remaining > EXCHANGE_TICKS
                || old[e.token] != e.from
                || state.tokens[e.token] != e.to
            {
                return Err("Invalid gap exchange animation".into());
            }
            let gap = corridor(room, old, e.token, e.axis);
            if gap.destination != e.to || gap.before != e.before || gap.after != e.after {
                return Err("Exchange disagrees with corridor".into());
            }
        }
        self.state = state;
        self.cues.clear();
        Ok(())
    }
    fn content(&self) -> Option<u64> {
        Some(hash_json(&include_str!("rooms.rs")))
    }
    fn save_tick(&self) -> u64 {
        u64::from(self.state.tick)
    }
}

pub fn click(point: Point) -> Intent {
    Intent {
        pointer: Some(point),
        action: true,
        ..Default::default()
    }
}
pub fn center(rect: Rect) -> Point {
    Point::new(rect.x + rect.w / 2, rect.y + rect.h / 2)
}
/// Authored solution demonstrator. Every move selects a real token, previews an axis and clicks its ghost.
/// Ordinary idle ticks allow the read-only bands to exchange. No verification-only rules exist.
pub fn verification_route() -> &'static [Intent] {
    static ROUTE: OnceLock<Vec<Intent>> = OnceLock::new();
    ROUTE.get_or_init(|| {
        let mut route = vec![Intent::default(); 60];
        let append = |route: &mut Vec<Intent>, intent: Intent, wait: usize| {
            route.push(intent);
            route.extend(std::iter::repeat_n(Intent::default(), wait));
        };
        // Include undo and a repeated public move in the first room as well as the whole campaign.
        for (index, room) in ROOMS.iter().enumerate() {
            let mut positions = room.start.to_vec();
            for &(token, axis) in room.solution {
                let gap = corridor(room, &positions, token, axis);
                append(&mut route, click(room.center(positions[token])), 7);
                append(&mut route, click(center(axis.button())), 7);
                append(&mut route, click(room.center(gap.destination)), 19);
                if index == 0 {
                    append(&mut route, click(center(UNDO)), 7);
                    append(&mut route, click(center(axis.button())), 7);
                    append(&mut route, click(room.center(gap.destination)), 31);
                }
                positions[token] = gap.destination;
            }
            append(&mut route, Intent::default(), 119);
            append(&mut route, click(center(NEXT)), 35);
        }
        route
    })
}
#[cfg(test)]
mod tests;
