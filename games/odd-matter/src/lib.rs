//! Odd Matter's only authority: integer voxels, one intention per fixed tick.
//! Presentation never changes occupancy. All six vaults, undo and save continuation are headless.
use serde::{Deserialize, Serialize};
use vesper3d::viewer::devkit::{SavePolicy, Simulation, Snapshot, StateHasher};

pub const SIDE: i8 = 5;
pub const ROOMS: usize = 6;
pub type Cell = [i8; 3];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Left,
    Right,
    Down,
    Up,
    Front,
    Back,
}
impl Direction {
    pub const ALL: [Self; 6] = [Self::Left, Self::Right, Self::Down, Self::Up, Self::Front, Self::Back];
    pub fn axis(self) -> usize {
        match self {
            Self::Left | Self::Right => 0,
            Self::Down | Self::Up => 1,
            _ => 2,
        }
    }
    pub fn sign(self) -> i8 {
        match self {
            Self::Left | Self::Down | Self::Back => -1,
            _ => 1,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Left => "A",
            Self::Right => "D",
            Self::Down => "Q",
            Self::Up => "E",
            Self::Front => "S",
            Self::Back => "W",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "LEFT",
            Self::Right => "RIGHT",
            Self::Down => "DOWN",
            Self::Up => "UP",
            Self::Front => "FRONT",
            Self::Back => "BACK",
        }
    }
    pub fn moved(self, mut cell: Cell) -> Cell {
        cell[self.axis()] += self.sign();
        cell
    }
}

/// Exactly the public commands the native keyboard, buttons and verification route send.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Intent {
    /// 0 = drone, 1..=3 = bar. Selecting never counts as a move.
    pub select: Option<u8>,
    pub direction: Option<Direction>,
    pub undo: bool,
    pub restart: bool,
    pub advance: bool,
}
impl Intent {
    pub fn select(object: u8) -> Self {
        Self { select: Some(object), ..Self::default() }
    }
    pub fn move_(direction: Direction) -> Self {
        Self { direction: Some(direction), ..Self::default() }
    }
}

#[derive(Clone, Debug)]
pub struct Bar {
    pub name: &'static str,
    pub origin: Cell,
    pub size: Cell,
    pub rail: usize,
    pub initial: i8,
}
impl Bar {
    pub fn origin_at(&self, offset: i8) -> Cell {
        let mut p = self.origin;
        p[self.rail] = offset;
        p
    }
    pub fn contains(&self, offset: i8, p: Cell) -> bool {
        let origin = self.origin_at(offset);
        (0..3).all(|a| p[a] >= origin[a] && p[a] < origin[a] + self.size[a])
    }
    pub fn max_offset(&self) -> i8 {
        SIDE - self.size[self.rail]
    }
}

#[derive(Clone, Debug)]
pub struct Chamber {
    pub title: &'static str,
    pub lesson: &'static str,
    pub hint: &'static str,
    pub spawn: Cell,
    pub exit: Cell,
    pub bars: Vec<Bar>,
    fixed: [bool; 125],
}
fn index(p: Cell) -> usize {
    p[0] as usize + 5 * p[1] as usize + 25 * p[2] as usize
}
pub fn inside(p: Cell) -> bool {
    p.iter().all(|v| (0..SIDE).contains(v))
}
pub fn cells() -> impl Iterator<Item = Cell> {
    (0..SIDE).flat_map(|z| (0..SIDE).flat_map(move |y| (0..SIDE).map(move |x| [x, y, z])))
}
fn bar(name: &'static str, origin: Cell, size: Cell, rail: usize, initial: i8) -> Bar {
    Bar { name, origin, size, rail, initial }
}
impl Chamber {
    pub fn fixed(&self, p: Cell) -> bool {
        inside(p) && self.fixed[index(p)]
    }
    pub fn layers(&self, offsets: &[i8; 3], p: Cell) -> u8 {
        if !inside(p) {
            return 1;
        } // Frame cannot be cancelled by a bar.
        u8::from(self.fixed(p)) + self.bars.iter().enumerate().filter(|(i, b)| b.contains(offsets[*i], p)).count() as u8
    }
    pub fn solid(&self, offsets: &[i8; 3], p: Cell) -> bool {
        self.layers(offsets, p) % 2 == 1
    }
    pub fn initial_offsets(&self) -> [i8; 3] {
        let mut o = [0; 3];
        for (i, b) in self.bars.iter().enumerate() {
            o[i] = b.initial;
        }
        o
    }
}

/// Small authored spatial puzzles. Coordinates are X right, Y up, Z front.
pub fn chamber(room: usize) -> Chamber {
    assert!(room < ROOMS);
    let (title, lesson, hint, spawn, exit, bars): (_, _, _, Cell, Cell, Vec<Bar>) = match room {
        0 => (
            "A wall of two",
            "One layer blocks. Two layers disappear.",
            "Raise amber to the drone's height. Cross the wall, then rise to the socket.",
            [0, 1, 2],
            [4, 3, 2],
            vec![bar("AMBER", [1, 0, 2], [3, 1, 1], 1, 0)],
        ),
        1 => (
            "Into the depth",
            "The empty tunnel reaches all the way through.",
            "Slide amber right twice. W travels toward the back of the vault.",
            [2, 1, 4],
            [4, 3, 0],
            vec![bar("AMBER", [0, 1, 1], [1, 1, 3], 0, 0)],
        ),
        2 => (
            "Rail exchange",
            "Hand a horizontal passage to a vertical one.",
            "Slide amber toward the front twice and rose three times. The cell between their ends is a safe dock.",
            [0, 1, 4],
            [4, 4, 1],
            vec![bar("AMBER", [1, 1, 0], [2, 1, 1], 2, 1), bar("ROSE", [3, 2, 0], [1, 2, 1], 2, 0)],
        ),
        3 => (
            "Three is a plug",
            "Fixed + two bars is solid. Add the third: empty again.",
            "Lower lime twice. Four layers open the junction; then ascend through rose.",
            [0, 1, 2],
            [2, 4, 2],
            vec![
                bar("AMBER", [1, 0, 2], [3, 1, 1], 1, 1),
                bar("ROSE", [2, 0, 0], [1, 5, 1], 2, 2),
                bar("LIME", [2, 0, 1], [1, 1, 3], 1, 3),
            ],
        ),
        4 => (
            "A place to wait",
            "A cavity can move. Its passenger cannot.",
            "Cross to the far dock first. Reuse amber at Z4. Park beside the shaft before shifting rose and lime.",
            [0, 1, 1],
            [2, 3, 0],
            vec![
                bar("AMBER", [1, 1, 0], [3, 1, 1], 2, 1),
                bar("ROSE", [0, 1, 3], [1, 3, 1], 0, 2),
                bar("LIME", [2, 0, 1], [1, 1, 3], 1, 4),
            ],
        ),
        _ => (
            "The upper lock",
            "Reopen the intersection at a new height AND depth.",
            "Lime opens the first junction. Park in the far-right berth outside amber's sweep. Raise amber and lime; move rose toward the back.",
            [0, 1, 3],
            [2, 4, 1],
            vec![
                bar("AMBER", [1, 0, 1], [3, 1, 3], 1, 1),
                bar("ROSE", [2, 0, 0], [1, 5, 1], 2, 3),
                bar("LIME", [2, 0, 1], [1, 1, 3], 1, 0),
            ],
        ),
    };
    let mut fixed = [false; 125];
    for p in cells() {
        fixed[index(p)] = match room {
            0 => (1..=3).contains(&p[0]),
            1 => (1..=3).contains(&p[2]),
            2 => (1..=2).contains(&p[0]) || (p[0] >= 3 && (2..=3).contains(&p[1])),
            3 => (1..=3).contains(&p[0]),
            4 => {
                !(p == spawn
                    || p == exit
                    || (p[0] == 4 && p[1] == 1 && (1..=3).contains(&p[2]))
                    || (p[0] == 3 && (2..=3).contains(&p[1]) && p[2] == 3))
            }
            _ => p != spawn && p != [3, 2, 3] && p != [4, 2, 3] && p != [4, 3, 3],
        };
    }
    Chamber { title, lesson, hint, spawn, exit, bars, fixed }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Playing,
    Crushed,
    Cleared,
    Won,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    pub room: usize,
    pub drone: Cell,
    pub offsets: [i8; 3],
    pub selected: u8,
    pub status: Status,
    pub moves: u32,
}
impl Board {
    fn new(room: usize) -> Self {
        let c = chamber(room);
        Self { room, drone: c.spawn, offsets: c.initial_offsets(), selected: 0, status: Status::Playing, moves: 0 }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    Solid,
    Frame,
    Rail,
    RailEnd,
    Finished,
}
impl Block {
    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "SOLID CELL",
            Self::Frame => "VAULT FRAME",
            Self::Rail => "ONLY ALONG THE RAIL",
            Self::RailEnd => "END OF RAIL",
            Self::Finished => "UNDO / RETRY",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Selected,
    Moved { from: Cell, to: Cell },
    Shifted { opened: usize, closed: usize },
    Blocked(Block),
    Crushed,
    Cleared,
    Won,
    Undone,
    Restarted,
    Advanced,
}
#[derive(Clone, Debug)]
pub struct Forecast {
    pub blocked: Option<Block>,
    pub opened: Vec<Cell>,
    pub closed: Vec<Cell>,
    pub crush: bool,
    pub destination: Cell,
    pub offsets: [i8; 3],
}

pub struct Sim {
    pub tick: u64,
    pub board: Board,
    history: Vec<Board>,
    seed: u64,
    events: Vec<Event>,
}
impl Sim {
    pub fn new(seed: u64) -> Self {
        Self { tick: 0, board: Board::new(0), history: Vec::new(), seed, events: Vec::new() }
    }
    pub fn undo_len(&self) -> usize {
        self.history.len()
    }
    pub fn room(&self) -> Chamber {
        chamber(self.board.room)
    }
    /// Read-only forecast uses exactly the same occupancy query as step, including crushing.
    pub fn forecast(&self, direction: Direction) -> Forecast {
        let b = &self.board;
        let c = self.room();
        let mut f = Forecast {
            blocked: None,
            opened: Vec::new(),
            closed: Vec::new(),
            crush: false,
            destination: b.drone,
            offsets: b.offsets,
        };
        if b.status != Status::Playing {
            f.blocked = Some(Block::Finished);
            return f;
        }
        if b.selected == 0 {
            f.destination = direction.moved(b.drone);
            if !inside(f.destination) {
                f.blocked = Some(Block::Frame);
            } else if c.solid(&b.offsets, f.destination) {
                f.blocked = Some(Block::Solid);
            }
        } else {
            let i = (b.selected - 1) as usize;
            let bar = &c.bars[i];
            if direction.axis() != bar.rail {
                f.blocked = Some(Block::Rail);
                return f;
            }
            let next = b.offsets[i] + direction.sign();
            if next < 0 || next > bar.max_offset() {
                f.blocked = Some(Block::RailEnd);
                return f;
            }
            f.offsets[i] = next;
            for p in cells() {
                let before = c.solid(&b.offsets, p);
                let after = c.solid(&f.offsets, p);
                if before && !after {
                    f.opened.push(p);
                }
                if !before && after {
                    f.closed.push(p);
                }
            }
            f.crush = c.solid(&f.offsets, b.drone);
        }
        f
    }
    /// One fixed 60 Hz tick. A directional intention is an edge, never a held repeat.
    pub fn step(&mut self, intent: &Intent) {
        self.tick = self.tick.wrapping_add(1);
        if intent.restart {
            self.board = Board::new(self.board.room);
            self.history.clear();
            self.events.push(Event::Restarted);
            return;
        }
        if intent.undo {
            if let Some(previous) = self.history.pop() {
                self.board = previous;
                self.events.push(Event::Undone);
            }
            return;
        }
        if intent.advance {
            if self.board.status == Status::Cleared {
                self.board = Board::new(self.board.room + 1);
                self.history.clear();
                self.events.push(Event::Advanced);
            } else if self.board.status == Status::Won {
                self.board = Board::new(0);
                self.history.clear();
                self.events.push(Event::Advanced);
            }
            return;
        }
        if let Some(object) = intent.select {
            if object as usize <= self.room().bars.len() && self.board.selected != object {
                self.board.selected = object;
                self.events.push(Event::Selected);
            }
        }
        let Some(direction) = intent.direction else {
            return;
        };
        let f = self.forecast(direction);
        if let Some(block) = f.blocked {
            self.events.push(Event::Blocked(block));
            return;
        }
        self.history.push(self.board.clone());
        self.board.moves = self.board.moves.saturating_add(1);
        if self.board.selected == 0 {
            let from = self.board.drone;
            self.board.drone = f.destination;
            self.events.push(Event::Moved { from, to: f.destination });
        } else {
            self.board.offsets = f.offsets;
            self.events.push(Event::Shifted { opened: f.opened.len(), closed: f.closed.len() });
            if f.crush {
                self.board.status = Status::Crushed;
                self.events.push(Event::Crushed);
                return;
            }
        }
        if self.board.drone == self.room().exit {
            self.board.status = if self.board.room + 1 == ROOMS { Status::Won } else { Status::Cleared };
            self.events.push(if self.board.status == Status::Won { Event::Won } else { Event::Cleared });
        }
    }
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
}

/// Authored solutions through public select/move/advance commands. No warping, state mutation or solver in play.
/// 1..4 select; A/D, Q/E, W/S move; > advances a cleared chamber.
pub const SOLUTIONS: [&str; ROOMS] = [
    "2E1DDDDEE",
    "2DD1WWWWDDEE",
    "2SS1WDDD3SSS1EEEWWD",
    "4QQ1DDEEE",
    "1DDDD2SS4QQQ1SSAAED4EE3A1EAWWW",
    "4E1DDEDD2EE4EE3WW1EAWWAE",
];
pub fn command(c: char) -> Intent {
    match c {
        '1'..='4' => Intent::select(c as u8 - b'1'),
        'A' => Intent::move_(Direction::Left),
        'D' => Intent::move_(Direction::Right),
        'Q' => Intent::move_(Direction::Down),
        'E' => Intent::move_(Direction::Up),
        'W' => Intent::move_(Direction::Back),
        'S' => Intent::move_(Direction::Front),
        'Z' => Intent { undo: true, ..Intent::default() },
        'R' => Intent { restart: true, ..Intent::default() },
        '>' => Intent { advance: true, ..Intent::default() },
        _ => Intent::default(),
    }
}
pub fn verification_route() -> Vec<Intent> {
    SOLUTIONS
        .iter()
        .enumerate()
        .flat_map(|(i, route)| route.chars().map(command).chain((i + 1 < ROOMS).then_some(command('>'))))
        .collect()
}
/// Native --verify-route and tests use the exact same timed public intentions.
pub fn verification_input(tick: u64) -> Intent {
    if !tick.is_multiple_of(12) {
        return Intent::default();
    }
    verification_route().get((tick / 12) as usize).copied().unwrap_or_default()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimState {
    pub tick: u64,
    pub seed: u64,
    pub board: Board,
    pub history: Vec<Board>,
}
fn hash_board(h: &mut StateHasher, b: &Board) {
    h.u64(b.room as u64).u32(b.selected as u32).u32(b.status as u32).u32(b.moves);
    for v in b.drone.into_iter().chain(b.offsets) {
        h.u32((v as i32) as u32);
    }
}
impl Simulation for Sim {
    type Input = Intent;
    fn step(&mut self, input: &Intent) {
        Sim::step(self, input);
    }
    fn state_hash(&self) -> u64 {
        let mut h = StateHasher::new();
        h.u64(self.tick).u64(self.seed);
        hash_board(&mut h, &self.board);
        h.u64(self.history.len() as u64);
        for b in &self.history {
            hash_board(&mut h, b);
        }
        h.finish()
    }
    fn hash_parts(&self) -> Vec<(&'static str, u64)> {
        let mut b = StateHasher::new();
        hash_board(&mut b, &self.board);
        let mut undo = StateHasher::new();
        for b in &self.history {
            hash_board(&mut undo, b);
        }
        let mut clock = StateHasher::new();
        clock.u64(self.tick).u64(self.seed);
        vec![("board", b.finish()), ("undo", undo.finish()), ("clock", clock.finish())]
    }
}
fn validate_board(b: &Board) -> Result<(), String> {
    if b.room >= ROOMS || !inside(b.drone) {
        return Err("The saved drone is outside this vault.".into());
    }
    let c = chamber(b.room);
    if b.selected as usize > c.bars.len() {
        return Err("The saved selection does not exist.".into());
    }
    for (i, o) in b.offsets.iter().enumerate() {
        let max = c.bars.get(i).map_or(0, Bar::max_offset);
        if *o < 0 || *o > max {
            return Err("A saved bar is outside its rail.".into());
        }
    }
    let solid = c.solid(&b.offsets, b.drone);
    if (b.status == Status::Crushed) != solid {
        return Err("The saved drone and matter disagree.".into());
    }
    if matches!(b.status, Status::Cleared | Status::Won)
        && (b.drone != c.exit || (b.status == Status::Won) != (b.room + 1 == ROOMS))
    {
        return Err("The saved completion does not match its socket.".into());
    }
    Ok(())
}
impl Snapshot for Sim {
    const KIND: &'static str = "odd-matter";
    const POLICY: SavePolicy = SavePolicy::Exact;
    type State = SimState;
    fn capture(&self) -> SimState {
        SimState { tick: self.tick, seed: self.seed, board: self.board.clone(), history: self.history.clone() }
    }
    fn content(&self) -> Option<u64> {
        let mut h = StateHasher::new();
        for n in 0..ROOMS {
            let c = chamber(n);
            for p in cells() {
                h.bool(c.fixed(p));
            }
            for v in c.spawn.into_iter().chain(c.exit) {
                h.u32(v as u32);
            }
            for b in c.bars {
                h.u32(b.rail as u32).u32(b.initial as u32);
                for v in b.origin.into_iter().chain(b.size) {
                    h.u32(v as u32);
                }
            }
        }
        Some(h.finish())
    }
    fn save_tick(&self) -> u64 {
        self.tick
    }
    fn restore(&mut self, s: SimState) -> Result<(), String> {
        validate_board(&s.board)?;
        for b in &s.history {
            validate_board(b)?;
            if b.room != s.board.room || b.status != Status::Playing {
                return Err("The saved undo trail belongs to another attempt.".into());
            }
        }
        self.tick = s.tick;
        self.seed = s.seed;
        self.board = s.board;
        self.history = s.history;
        self.events.clear();
        Ok(())
    }
}
