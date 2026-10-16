//! Corner Clutch: integer, fixed-step rules. Presentation never moves a crate.
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use vesper3d::two_d::*;

#[cfg(any(feature = "client", test))]
mod input;

pub type Cell = [i32; 2];
pub const ORIGIN: Cell = [48, 88];
pub const TILE: i32 = 48;
pub const DIRS: [Cell; 4] = [[0, -1], [-1, 0], [0, 1], [1, 0]];
pub const BUTTONS: [Rect; 4] = [
    Rect::new(554, 218, 42, 38),
    Rect::new(506, 262, 42, 38),
    Rect::new(554, 262, 42, 38),
    Rect::new(602, 262, 42, 38),
];
pub const UNDO: Rect = Rect::new(396, 336, 104, 32);
pub const HINT: Rect = Rect::new(508, 336, 104, 32);
pub const NEXT: Rect = Rect::new(620, 336, 144, 32);

#[derive(Clone, Debug, Deserialize)]
pub struct Room {
    pub title: String,
    pub intro: String,
    pub hint: String,
    pub tip: String,
    pub start: Vec<Cell>,
    pub goals: Vec<Cell>,
    pub walls: Vec<Cell>,
    pub solution: Vec<[i32; 3]>,
}
pub fn rooms() -> &'static [Room] {
    static ROOMS: OnceLock<Vec<Room>> = OnceLock::new();
    ROOMS.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/rooms.json")).expect("authored rooms")
    })
}
fn add(a: Cell, b: Cell) -> Cell {
    [a[0] + b[0], a[1] + b[1]]
}
fn inside(c: Cell) -> bool {
    (0..6).contains(&c[0]) && (0..6).contains(&c[1])
}
pub fn center(c: Cell) -> Point {
    Point::new(
        ORIGIN[0] + c[0] * TILE + TILE / 2,
        ORIGIN[1] + c[1] * TILE + TILE / 2,
    )
}
pub fn crate_at(cells: &[Cell], point: Point) -> Option<usize> {
    cells.iter().position(|&c| {
        let p = center(c);
        Rect::new(p.x - TILE / 2, p.y - TILE / 2, TILE, TILE).contains(point)
    })
}
pub fn click(rect: Rect) -> Intent {
    Intent {
        pointer: Some(Point::new(rect.x + rect.w / 2, rect.y + rect.h / 2)),
        action: true,
        ..Intent::default()
    }
}
pub fn select(c: Cell) -> Intent {
    Intent {
        pointer: Some(center(c)),
        action: true,
        ..Intent::default()
    }
}

/// Map a native hotkey to an ordinary click without releasing held movement.
/// Targets are button/crate centers. Offset coincident cursors by one pixel so
/// the shared client's press-target detection retains this synthetic click.
pub fn keyboard_target(held: Intent, mut target: Point) -> Intent {
    if held.pointer == Some(target) {
        target.x += 1;
    }
    Intent {
        pointer: Some(target),
        action: true,
        ..held
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Obstacle {
    Boundary,
    Wall,
    Crate,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blocked {
    pub cell: Cell,
    pub kind: Obstacle,
}
#[derive(Clone, Debug)]
pub struct MovePlan {
    pub members: Vec<usize>,
    pub destinations: Vec<(usize, Cell)>,
    pub blocked: Vec<Blocked>,
}
/// Collect once around the driver. Passengers never recruit their own neighbors.
pub fn passengers(cells: &[Cell], driver: usize) -> Vec<usize> {
    let a = cells[driver];
    cells
        .iter()
        .enumerate()
        .filter_map(|(j, b)| {
            (j == driver || ((a[0] - b[0]).abs() == 1 && (a[1] - b[1]).abs() == 1)).then_some(j)
        })
        .collect()
}
pub fn plan(room: &Room, cells: &[Cell], driver: usize, dir: Cell) -> MovePlan {
    let members = passengers(cells, driver);
    let destinations: Vec<_> = members.iter().map(|&j| (j, add(cells[j], dir))).collect();
    let blocked = destinations
        .iter()
        .filter_map(|&(_, cell)| {
            let kind = if !inside(cell) {
                Some(Obstacle::Boundary)
            } else if room.walls.contains(&cell) {
                Some(Obstacle::Wall)
            } else if cells
                .iter()
                .enumerate()
                .any(|(j, &c)| c == cell && !members.contains(&j))
            {
                Some(Obstacle::Crate)
            } else {
                None
            };
            kind.map(|kind| Blocked { cell, kind })
        })
        .collect();
    MovePlan {
        members,
        destinations,
        blocked,
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    pub cells: Vec<Cell>,
    pub selected: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub tick: u32,
    pub room: usize,
    pub cells: Vec<Cell>,
    pub selected: usize,
    pub history: Vec<Frame>,
    pub held_axes: Cell,
    pub slide_from: Vec<Cell>,
    pub slide_ticks: u8,
    pub blocked: Vec<Blocked>,
    pub feedback_ticks: u8,
    pub hint: bool,
}
pub struct Clutch {
    pub state: State,
    cues: Vec<usize>,
    // Device events are buffered outside authoritative state, like the shared
    // client's input accumulator. Loading/restarting discards pending devices.
    #[cfg(any(feature = "client", test))]
    native: input::NativeInput,
    // Native hover is cosmetic; it never decides a command or a state hash.
    #[cfg(feature = "client")]
    hover: Option<Point>,
}
impl Clutch {
    pub fn room(&self) -> &Room {
        &rooms()[self.state.room]
    }
    pub fn solved(&self) -> bool {
        self.state.cells == self.room().goals
    }
    fn clear_feedback(&mut self) {
        self.state.blocked.clear();
        self.state.feedback_ticks = 0;
    }
    fn load_room(&mut self, n: usize) {
        #[cfg(any(feature = "client", test))]
        {
            self.native = input::NativeInput::default();
        }
        self.state.room = n;
        self.state.cells = rooms()[n].start.clone();
        self.state.selected = 0;
        self.state.slide_from = self.state.cells.clone();
        self.state.slide_ticks = 0;
        self.state.history.clear();
        self.state.held_axes = [0, 0];
        self.state.hint = false;
        self.clear_feedback();
        self.cues.clear();
    }
    fn undo(&mut self) {
        if let Some(frame) = self.state.history.pop() {
            self.state.cells = frame.cells;
            self.state.selected = frame.selected;
            self.state.slide_from = self.state.cells.clone();
            self.state.slide_ticks = 0;
            self.clear_feedback();
            self.cues.push(0);
        }
    }
    fn attempt(&mut self, dir: Cell) {
        if self.solved() {
            return;
        }
        let p = plan(self.room(), &self.state.cells, self.state.selected, dir);
        self.state.blocked = p.blocked;
        self.state.feedback_ticks = 75;
        if !self.state.blocked.is_empty() {
            self.cues.push(1);
            return;
        }
        self.state.history.push(Frame {
            cells: self.state.cells.clone(),
            selected: self.state.selected,
        });
        self.state.slide_from = self.state.cells.clone();
        self.state.slide_ticks = 8;
        for (j, c) in p.destinations {
            self.state.cells[j] = c;
        }
        self.cues.push(if self.solved() { 2 } else { 0 });
    }
    fn pointer_command(&mut self, point: Point) -> Option<Cell> {
        if UNDO.contains(point) {
            self.undo();
            return None;
        }
        if HINT.contains(point) {
            self.state.hint = !self.state.hint;
            return None;
        }
        if NEXT.contains(point) && self.solved() {
            if self.state.room + 1 < rooms().len() {
                self.load_room(self.state.room + 1);
            }
            return None;
        }
        for (n, r) in BUTTONS.iter().enumerate() {
            if r.contains(point) {
                return Some(DIRS[n]);
            }
        }
        if let Some(j) = crate_at(&self.state.cells, point) {
            self.state.selected = j;
            self.clear_feedback();
        }
        None
    }
}
impl GameLogic for Clutch {
    const ID: &'static str = "corner-clutch";
    const TITLE: &'static str = "Corner Clutch";
    const CONTROLS: &'static str = "Click crate | WASD/arrows: step | Z: undo | R: room reset";
    const VERIFY_TICKS: u32 = 2100;
    fn new(_: u64) -> Self {
        let cells = rooms()[0].start.clone();
        Self {
            state: State {
                tick: 0,
                room: 0,
                cells: cells.clone(),
                selected: 0,
                history: vec![],
                held_axes: [0, 0],
                slide_from: cells,
                slide_ticks: 0,
                blocked: vec![],
                feedback_ticks: 0,
                hint: false,
            },
            cues: vec![],
            #[cfg(any(feature = "client", test))]
            native: input::NativeInput::default(),
            #[cfg(feature = "client")]
            hover: None,
        }
    }
    fn restart(&mut self) {
        self.load_room(self.state.room);
    }
    fn pointer_target_only_on_press() -> bool {
        true
    }
    fn tick(&self) -> u32 {
        self.state.tick
    }
    fn outcome(&self) -> &'static str {
        if self.state.room == rooms().len() - 1 && self.solved() {
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
        Intent {
            y: -1,
            ..Intent::default()
        }
    }
    fn probe_success(&self) -> bool {
        self.state.cells[0] == [1, 1]
            && self.state.cells[1] == [2, 2]
            && !self.state.history.is_empty()
    }
    fn take_cues(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.cues)
    }
    fn cue_point(&self, _: usize) -> Point {
        center(self.state.cells[self.state.selected])
    }
}
impl Simulation for Clutch {
    type Input = Intent;
    fn step(&mut self, i: &Intent) {
        self.state.tick = self.state.tick.wrapping_add(1);
        self.state.slide_ticks = self.state.slide_ticks.saturating_sub(1);
        self.state.feedback_ticks = self.state.feedback_ticks.saturating_sub(1);
        if self.state.feedback_ticks == 0 {
            self.state.blocked.clear();
        }
        let mut command = None;
        if i.action {
            if let Some(point) = i.pointer {
                command = self.pointer_command(point);
            }
        }
        let axes = [i.x.signum(), i.y.signum()];
        let pressed_x = axes[0] != 0 && axes[0] != self.state.held_axes[0];
        let pressed_y = axes[1] != 0 && axes[1] != self.state.held_axes[1];
        // A targeted command wins over a simultaneous direction edge. Still
        // remember BOTH held axes: releasing one must not retrigger the other.
        if !(i.action && i.pointer.is_some()) {
            if pressed_x {
                command = Some([axes[0], 0]);
            } else if pressed_y {
                command = Some([0, axes[1]]);
            }
        }
        self.state.held_axes = axes;
        if let Some(dir) = command {
            self.attempt(dir);
        }
    }
    fn state_hash(&self) -> u64 {
        hash_json(&self.state)
    }
}
fn valid_cells(room: &Room, cells: &[Cell]) -> bool {
    cells.len() == room.start.len()
        && cells
            .iter()
            .enumerate()
            .all(|(i, c)| inside(*c) && !room.walls.contains(c) && !cells[..i].contains(c))
}
impl Snapshot for Clutch {
    const KIND: &'static str = "corner-clutch";
    const VERSION: u32 = 2;
    const MIGRATIONS: &'static [vesper3d::viewer::devkit::snapshot::Migration] =
        &[vesper3d::viewer::devkit::snapshot::Migration {
            from: 1,
            step: |mut value| {
                let state = value
                    .as_object_mut()
                    .ok_or("Save state must be an object")?;
                let held = state
                    .remove("last_dir")
                    .ok_or("Save has no held direction")?;
                state.insert("held_axes".into(), held);
                Ok(value)
            },
        }];
    type State = State;
    fn content(&self) -> Option<u64> {
        // Layout changes must not silently reinterpret a saved arrangement.
        Some(hash_json(&include_str!("../assets/rooms.json")))
    }
    fn save_tick(&self) -> u64 {
        u64::from(self.state.tick)
    }
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, s: State) -> Result<(), String> {
        let Some(room) = rooms().get(s.room) else {
            return Err("Save has an unknown room".into());
        };
        if !valid_cells(room, &s.cells)
            || s.selected >= s.cells.len()
            || !valid_cells(room, &s.slide_from)
            || s.slide_ticks > 8
            || s.feedback_ticks > 75
            || s.held_axes.iter().any(|v| !(-1..=1).contains(v))
            || s.history
                .iter()
                .any(|f| !valid_cells(room, &f.cells) || f.selected >= f.cells.len())
            || s.blocked.len() > 4
            || s.blocked
                .iter()
                .any(|b| b.cell.iter().any(|v| !(-1..=6).contains(v)))
        {
            return Err("Save contains invalid board data".into());
        }
        self.state = s;
        self.cues.clear();
        #[cfg(any(feature = "client", test))]
        {
            self.native = input::NativeInput::default();
        }
        #[cfg(feature = "client")]
        {
            self.hover = None;
        }
        Ok(())
    }
}
/// A complete campaign entered through the same click/keyboard commands as a player.
/// The extra first-room sequence exercises undo before continuing the campaign.
pub fn verification_route() -> &'static [Intent] {
    static ROUTE: OnceLock<Vec<Intent>> = OnceLock::new();
    ROUTE.get_or_init(|| {
        let mut inputs = vec![];
        let mut g = Clutch::new(7);
        let feed = |i: Intent, g: &mut Clutch, inputs: &mut Vec<Intent>| {
            g.step(&i);
            inputs.push(i);
        };
        for n in 0..rooms().len() {
            for (k, &[driver, dx, dy]) in rooms()[n].solution.iter().enumerate() {
                let i = select(g.state.cells[driver as usize]);
                feed(i, &mut g, &mut inputs);
                feed(
                    click(BUTTONS[DIRS.iter().position(|&d| d == [dx, dy]).unwrap()]),
                    &mut g,
                    &mut inputs,
                );
                for _ in 0..14 {
                    feed(Intent::default(), &mut g, &mut inputs);
                }
                if n == 0 && k == 0 {
                    feed(click(UNDO), &mut g, &mut inputs);
                    feed(click(BUTTONS[0]), &mut g, &mut inputs);
                    for _ in 0..14 {
                        feed(Intent::default(), &mut g, &mut inputs);
                    }
                }
            }
            if n + 1 < rooms().len() {
                feed(click(NEXT), &mut g, &mut inputs);
            }
            for _ in 0..30 {
                feed(Intent::default(), &mut g, &mut inputs);
            }
        }
        inputs
    })
}
#[cfg(feature = "client")]
mod presentation;
#[cfg(test)]
mod tests;
