//! Deterministic falling-block rules. Devices, timing, saves and audio belong to BlueEngine.
use serde::{Deserialize, Serialize};
#[cfg(test)]
use vesper3d::runtime::snapshot;
use vesper3d::two_d::*;

// I, O, T, S, Z, J, L. SRS spawn matrices and rotation centres.
pub const WIDTH: usize = 10;
pub const HEIGHT: usize = 20;
pub const ROWS: usize = HEIGHT + 2;
pub mod controls;
const SHAPES: [[(i32, i32); 4]; 7] = [
    [(0, 1), (1, 1), (2, 1), (3, 1)],
    [(1, 0), (2, 0), (1, 1), (2, 1)],
    [(1, 0), (0, 1), (1, 1), (2, 1)],
    [(1, 0), (2, 0), (0, 1), (1, 1)],
    [(0, 0), (1, 0), (1, 1), (2, 1)],
    [(0, 0), (0, 1), (1, 1), (2, 1)],
    [(2, 0), (0, 1), (1, 1), (2, 1)],
];
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Piece {
    pub kind: u8,
    pub rotation: u8,
    pub x: i32,
    pub y: i32,
}
impl Piece {
    fn spawn(kind: u8) -> Self {
        Self {
            kind,
            rotation: 0,
            x: (WIDTH as i32 - 4) / 2,
            y: 2,
        }
    }
    fn cells(self) -> [(i32, i32); 4] {
        let mut c = SHAPES[self.kind as usize];
        if self.kind != 1 {
            for _ in 0..self.rotation {
                for (x, y) in &mut c {
                    (*x, *y) = (if self.kind == 0 { 3 } else { 2 } - *y, *x);
                }
            }
        }
        c.map(|(x, y)| (x + self.x, y + self.y))
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    pub tick: u32,
    pub board: Vec<u8>,
    pub active: Piece,
    pub queue: Vec<u8>,
    pub bag: Vec<u8>,
    pub rng: u64,
    pub hold: Option<u8>,
    pub held: bool,
    pub score: u64,
    pub lines: u32,
    pub pieces: u32,
    pub mode: u8,
    pub elapsed: u32,
    pub gravity: u32,
    pub lock: u32,
    pub resets: u32,
    pub horizontal: i32,
    pub repeat: u32,
    pub previous_y: i32,
    pub rotated: bool,
    pub combo: i32,
    pub b2b: bool,
    pub clear_rows: Vec<usize>,
    pub clear_timer: u32,
    pub message: String,
    pub message_timer: u32,
    pub ended: bool,
    pub won: bool,
}
pub struct Tetris {
    pub state: State,
    cues: Vec<usize>,
}
impl Tetris {
    fn random(&mut self) -> u64 {
        let mut x = self.state.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state.rng = x;
        x
    }
    fn next_kind(&mut self) -> u8 {
        if self.state.bag.is_empty() {
            self.state.bag = (0..7).collect();
            for n in (1..7).rev() {
                let j = self.random() as usize % (n + 1);
                self.state.bag.swap(n, j);
            }
        }
        self.state.bag.pop().unwrap()
    }
    fn refill(&mut self) {
        while self.state.queue.len() < 5 {
            let k = self.next_kind();
            self.state.queue.push(k);
        }
    }
    fn spawn(&mut self) {
        let k = self.state.queue.remove(0);
        self.refill();
        self.state.active = Piece::spawn(k);
        self.state.gravity = 0;
        self.state.lock = 0;
        self.state.resets = 0;
        self.state.rotated = false;
        if !self.fits(self.state.active) {
            self.state.ended = true;
            self.cues.push(1);
        }
    }
    pub fn level(&self) -> u32 {
        1 + self.state.lines / 10
    }
    fn interval(&self) -> u32 {
        [48, 38, 30, 24, 19, 15, 12, 9, 7, 5, 4, 3, 2, 1][(self.level() as usize - 1).min(13)]
    }
    fn fits(&self, p: Piece) -> bool {
        p.cells().iter().all(|&(x, y)| {
            (0..WIDTH as i32).contains(&x)
                && (0..ROWS as i32).contains(&y)
                && self.state.board[y as usize * WIDTH + x as usize] == 0
        })
    }
    fn grounded(&self) -> bool {
        !self.fits(Piece {
            y: self.state.active.y + 1,
            ..self.state.active
        })
    }
    pub fn ghost(&self) -> Piece {
        let mut p = self.state.active;
        while self.fits(Piece { y: p.y + 1, ..p }) {
            p.y += 1;
        }
        p
    }
    fn lock_reset(&mut self, was_grounded: bool) {
        if was_grounded && self.state.resets < 15 {
            self.state.lock = 0;
            self.state.resets += 1;
        }
    }
    fn shift(&mut self, dx: i32) -> bool {
        let p = Piece {
            x: self.state.active.x + dx,
            ..self.state.active
        };
        if !self.fits(p) {
            return false;
        }
        let grounded = self.grounded();
        self.state.active = p;
        self.state.rotated = false;
        self.lock_reset(grounded);
        true
    }
    fn rotate(&mut self, dir: i32) {
        let old = self.state.active;
        let to = (old.rotation as i32 + dir).rem_euclid(4) as u8;
        let tests = kicks(old.kind, old.rotation, to);
        for (dx, dy) in tests {
            let p = Piece {
                rotation: to,
                x: old.x + dx,
                y: old.y - dy,
                ..old
            };
            if self.fits(p) {
                let grounded = self.grounded();
                self.state.active = p;
                self.state.rotated = true;
                self.lock_reset(grounded);
                return;
            }
        }
    }
    fn hold(&mut self) {
        if self.state.held {
            return;
        }
        let k = self.state.active.kind;
        if let Some(h) = self.state.hold {
            self.state.active = Piece::spawn(h);
            self.state.lock = 0;
            self.state.gravity = 0;
            self.state.resets = 0;
            self.state.rotated = false;
            if !self.fits(self.state.active) {
                self.state.ended = true;
                self.cues.push(1);
            }
        } else {
            self.spawn();
        }
        self.state.hold = Some(k);
        self.state.held = true;
    }
    fn lock_piece(&mut self) {
        let p = self.state.active;
        let occupied = |x: i32, y: i32| {
            !(0..WIDTH as i32).contains(&x)
                || !(0..ROWS as i32).contains(&y)
                || self.state.board[y as usize * WIDTH + x as usize] != 0
        };
        let spin = p.kind == 2
            && self.state.rotated
            && [(0, 0), (2, 0), (0, 2), (2, 2)]
                .iter()
                .filter(|&&(x, y)| occupied(p.x + x, p.y + y))
                .count()
                >= 3;
        for (x, y) in p.cells() {
            self.state.board[y as usize * WIDTH + x as usize] = p.kind + 1;
        }
        self.state.pieces += 1;
        self.state.held = false;
        let rows: Vec<usize> = (0..ROWS)
            .filter(|&y| (0..WIDTH).all(|x| self.state.board[y * WIDTH + x] > 0))
            .collect();
        let n = rows.len();
        let difficult = n == 4 || (spin && n > 0);
        let scoring_level = self.level();
        let mut points = if spin {
            [400, 800, 1200, 1600][n.min(3)]
        } else {
            [0, 100, 300, 500, 800][n]
        };
        let back = difficult && self.state.b2b;
        if back {
            points = points * 3 / 2;
        }
        if n > 0 {
            self.state.combo += 1;
            points += 50 * self.state.combo.max(0) as u64;
            self.state.lines += n as u32;
            self.state.b2b = difficult;
        } else {
            self.state.combo = -1;
        }
        self.state.score += points * scoring_level as u64;
        self.state.message = if spin {
            format!("T-SPIN {}", ["", "SINGLE", "DOUBLE", "TRIPLE"][n.min(3)])
        } else {
            ["", "SINGLE", "DOUBLE", "TRIPLE", "TETRIS!"][n].into()
        };
        if back {
            self.state.message = format!("B2B {}", self.state.message);
        }
        self.state.message_timer = 100;
        if n > 0 {
            self.state.clear_rows = rows;
            self.state.clear_timer = 18;
            self.cues.push(2);
        } else {
            self.cues.push(0);
            self.finish_lock();
        }
    }
    fn finish_lock(&mut self) {
        if !self.state.clear_rows.is_empty() {
            let mut board = vec![0; self.state.clear_rows.len() * WIDTH];
            for y in 0..ROWS {
                if !self.state.clear_rows.contains(&y) {
                    board.extend_from_slice(&self.state.board[y * WIDTH..y * WIDTH + WIDTH]);
                }
            }
            self.state.board = board;
            self.state.clear_rows.clear();
            if self.state.board.iter().all(|&x| x == 0) {
                self.state.score += 3500 * self.level() as u64;
                self.state.message = "PERFECT CLEAR".into();
            }
        }
        if self.state.mode == 1 && self.state.lines >= 40 {
            self.state.won = true;
            self.state.ended = true;
            return;
        }
        if self.state.board[..WIDTH * 2].iter().any(|&x| x != 0) {
            self.state.ended = true;
            self.cues.push(1);
            return;
        }
        self.spawn();
    }
    fn select_mode(&mut self, mode: u8) {
        let seed = self.state.rng;
        *self = Self::new(seed);
        self.state.mode = mode;
    }
    fn command(i: &Intent) -> Option<i32> {
        i.action
            .then(|| i.pointer.filter(|p| p.x == -100).map(|p| p.y))
            .flatten()
    }
}
// Standard SRS offsets in y-up coordinates, tested in the listed order.
fn kicks(kind: u8, from: u8, to: u8) -> [(i32, i32); 5] {
    if kind == 0 {
        match (from, to) {
            (0, 1) | (3, 2) => [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)],
            (1, 0) | (2, 3) => [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)],
            (1, 2) | (0, 3) => [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)],
            _ => [(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)],
        }
    } else {
        match (from, to) {
            (0, 1) | (2, 1) => [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],
            (1, 0) | (1, 2) => [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],
            (2, 3) | (0, 3) => [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],
            _ => [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],
        }
    }
}
impl GameLogic for Tetris {
    const ID: &'static str = "tetris";
    const TITLE: &'static str = "Tetris";
    const CONTROLS:&'static str="Arrows move/rotate; Z reverse; C hold; Space drop; 1/2/3 mode; Esc pause; R retry; K/L save/load; M/N audio";
    const VERIFY_TICKS: u32 = 360;
    fn new(seed: u64) -> Self {
        let mut g = Self {
            state: State {
                tick: 0,
                board: vec![0; WIDTH * ROWS],
                active: Piece::spawn(0),
                queue: vec![],
                bag: vec![],
                rng: seed.max(1),
                hold: None,
                held: false,
                score: 0,
                lines: 0,
                pieces: 0,
                mode: 0,
                elapsed: 0,
                gravity: 0,
                lock: 0,
                resets: 0,
                horizontal: 0,
                repeat: 0,
                previous_y: 0,
                rotated: false,
                combo: -1,
                b2b: false,
                clear_rows: vec![],
                clear_timer: 0,
                message: "FIND YOUR FLOW".into(),
                message_timer: 180,
                ended: false,
                won: false,
            },
            cues: vec![],
        };
        g.refill();
        g.spawn();
        g
    }
    fn restart(&mut self) {
        let mode = self.state.mode;
        *self = Self::new(7);
        self.state.mode = mode;
    }
    fn tick(&self) -> u32 {
        self.state.tick
    }
    fn outcome(&self) -> &'static str {
        if self.state.won {
            "won"
        } else if self.state.ended {
            "lost"
        } else {
            "playing"
        }
    }
    fn pointer_target_only_on_press() -> bool {
        true
    }
    fn verification_input(t: u32) -> Intent {
        // Exercise movement, both rotations, hold and drops through public commands.
        let phase = t % 90;
        if phase == 5 {
            return Intent {
                action: true,
                pointer: Some(Point::new(-100, 2)),
                ..Default::default()
            };
        }
        if phase == 15 {
            return Intent {
                action: true,
                pointer: Some(Point::new(-100, 3)),
                ..Default::default()
            };
        }
        if t == 35 {
            return Intent {
                action: true,
                pointer: Some(Point::new(-100, 4)),
                ..Default::default()
            };
        }
        Intent {
            x: if phase < 25 {
                -1
            } else if phase < 45 {
                1
            } else {
                0
            },
            action: phase == 55,
            ..Default::default()
        }
    }
    fn probe_input() -> Intent {
        Intent {
            x: 1,
            ..Default::default()
        }
    }
    fn probe_success(&self) -> bool {
        self.state.active.x > (WIDTH as i32 - 4) / 2
    }
    fn take_cues(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.cues)
    }
    fn cue_point(&self, _: usize) -> Point {
        Point::new(-10000, -10000)
    }
    fn audio_banks() -> &'static [AudioBankSpec] {
        &[AudioBankSpec {
            id: "pulse",
            root: "assets/audio/pulse",
            music: true,
        }]
    }
    fn audio_level(&self, _: &str, layer: &str) -> f32 {
        if self.state.ended {
            0.
        } else if layer == "pulse" {
            0.30
        } else {
            0.20
        }
    }
}
impl Simulation for Tetris {
    type Input = Intent;
    fn step(&mut self, i: &Intent) {
        if let Some(c @ 10..=12) = Self::command(i) {
            self.select_mode((c - 10) as u8);
            return;
        }
        if self.state.ended {
            return;
        }
        self.state.tick += 1;
        self.state.elapsed += 1;
        self.state.message_timer = self.state.message_timer.saturating_sub(1);
        if self.state.mode == 2 && self.state.elapsed >= 7200 {
            self.state.won = true;
            self.state.ended = true;
            self.cues.push(2);
            return;
        }
        if self.state.clear_timer > 0 {
            self.state.clear_timer -= 1;
            if self.state.clear_timer == 0 {
                self.finish_lock();
            }
            return;
        }
        let cmd = Self::command(i);
        if cmd == Some(4) {
            self.hold();
            return;
        }
        let dx = i.x.clamp(-1, 1);
        if dx != self.state.horizontal {
            self.state.horizontal = dx;
            self.state.repeat = 0;
            if dx != 0 {
                self.shift(dx);
            }
        } else if dx != 0 {
            self.state.repeat += 1;
            if self.state.repeat >= 10 && (self.state.repeat - 10).is_multiple_of(2) {
                self.shift(dx);
            }
        }
        if cmd == Some(2) || (i.y < 0 && self.state.previous_y >= 0) {
            self.rotate(1);
        }
        if cmd == Some(3) {
            self.rotate(-1);
        }
        self.state.previous_y = i.y;
        if i.action && cmd.is_none() && i.pointer.is_none() {
            let ghost = self.ghost();
            self.state.score += 2 * (ghost.y - self.state.active.y) as u64;
            self.state.active = ghost;
            self.lock_piece();
            return;
        }
        self.state.gravity += 1;
        let soft = i.y > 0;
        if self.state.gravity >= if soft { 2 } else { self.interval() } {
            self.state.gravity = 0;
            let next = Piece {
                y: self.state.active.y + 1,
                ..self.state.active
            };
            if self.fits(next) {
                self.state.active = next;
                if soft {
                    self.state.score += 1;
                }
            }
        }
        if self.grounded() {
            self.state.lock += 1;
            if self.state.lock >= 30 {
                self.lock_piece();
            }
        } else {
            self.state.lock = 0;
        }
    }
    fn state_hash(&self) -> u64 {
        hash_json(&self.state)
    }
}
impl Snapshot for Tetris {
    const KIND: &'static str = "tetris-v1";
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, s: State) -> Result<(), String> {
        let mut unique = [false; 7];
        let bag_valid = s.bag.iter().all(|&k| {
            if k > 6 || unique[k as usize] {
                false
            } else {
                unique[k as usize] = true;
                true
            }
        });
        if s.board.len() != WIDTH * ROWS
            || s.board.iter().any(|&k| k > 7)
            || s.active.kind > 6
            || s.active.rotation > 3
            || s.queue.len() != 5
            || s.queue.iter().any(|&k| k > 6)
            || !bag_valid
            || s.bag.len() > 7
            || s.hold.is_some_and(|k| k > 6)
            || s.mode > 2
            || s.rng == 0
            || s.clear_rows.len() > 4
            || s.clear_rows.iter().any(|&r| r >= ROWS)
            || s.clear_timer > 18
            || s.resets > 15
            || s.message.len() > 80
            || s.active
                .cells()
                .iter()
                .any(|&(x, y)| !(0..WIDTH as i32).contains(&x) || !(0..ROWS as i32).contains(&y))
        {
            return Err("Invalid Tetris snapshot".into());
        }
        self.state = s;
        self.cues.clear();
        Ok(())
    }
}
#[cfg(feature = "client")]
mod presentation;
#[cfg(test)]
mod tests {
    use super::*;
    fn empty(g: &mut Tetris) {
        g.state.board.fill(0);
        g.state.clear_timer = 0;
        g.state.clear_rows.clear();
        g.state.ended = false;
    }
    #[test]
    fn public_route_is_deterministic_and_saves_resume() {
        let inputs: Vec<_> = (0..Tetris::VERIFY_TICKS)
            .map(Tetris::verification_input)
            .collect();
        vesper3d::runtime::assert_deterministic(|| Tetris::new(7), &inputs);
        snapshot::assert_resumes_exactly(|| Tetris::new(7), &inputs, 91);
        let mut g = Tetris::new(7);
        for i in &inputs {
            g.step(i);
        }
        assert!(g.state.pieces >= 3);
        assert!(g.state.hold.is_some());
        assert!(g.state.score > 0);
        let (hash, outcome) = verify::<Tetris>();
        assert_eq!(outcome, "playing");
        if let Ok(p) = std::env::var("BE2_VERIFY_REPORT") {
            std::fs::write(p,serde_json::json!({"hash":format!("{hash:016x}"),"outcome":outcome,"ticks":Tetris::VERIFY_TICKS,"purpose":"Move, SRS rotate in both directions, hold and hard-drop multiple pieces"}).to_string()).unwrap();
        }
    }
    #[test]
    fn each_bag_has_all_seven_and_seed_changes_sequence() {
        let mut g = Tetris::new(1);
        g.state.bag.clear();
        for _ in 0..40 {
            let mut a: Vec<_> = (0..7).map(|_| g.next_kind()).collect();
            a.sort();
            assert_eq!(a, (0..7).collect::<Vec<_>>());
        }
        assert_ne!(Tetris::new(1).state.queue, Tetris::new(2).state.queue);
    }
    #[test]
    fn wall_floor_kicks_and_four_turns() {
        let mut g = Tetris::new(7);
        empty(&mut g);
        g.state.active = Piece {
            kind: 2,
            rotation: 1,
            x: -1,
            y: 5,
        };
        assert!(g.fits(g.state.active));
        g.rotate(-1);
        assert!(g.fits(g.state.active));
        assert_eq!(g.state.active.x, 0);
        g.state.active = Piece {
            kind: 0,
            rotation: 0,
            x: 3,
            y: ROWS as i32 - 2,
        };
        assert!(g.fits(g.state.active));
        g.rotate(1);
        assert!(g.fits(g.state.active));
        assert!(g.state.active.y < ROWS as i32 - 2);
        g.state.active = Piece::spawn(2);
        let c = g.state.active.cells();
        for _ in 0..4 {
            g.rotate(1);
        }
        assert_eq!(c, g.state.active.cells());
    }
    #[test]
    fn hold_once_per_piece_and_blocked_spawn_loses() {
        let mut g = Tetris::new(7);
        g.hold();
        let k = g.state.active.kind;
        g.hold();
        assert_eq!(k, g.state.active.kind);
        assert!(g.state.held);
        g.state.board[..WIDTH * 5].fill(1);
        g.spawn();
        assert_eq!(g.outcome(), "lost");
        let h = g.state_hash();
        g.step(&Intent::default());
        assert_eq!(h, g.state_hash());
    }
    #[test]
    fn tetris_clears_delayed_rows_and_awards_back_to_back() {
        let mut g = Tetris::new(7);
        empty(&mut g);
        for repeat in 0..2 {
            for y in ROWS - 4..ROWS {
                for x in 0..WIDTH {
                    g.state.board[y * WIDTH + x] = if x == 4 { 0 } else { 2 };
                }
            }
            g.state.active = Piece {
                kind: 0,
                rotation: 1,
                x: 2,
                y: ROWS as i32 - 4,
            };
            assert!(g.fits(g.state.active));
            g.lock_piece();
            assert_eq!(g.state.clear_timer, 18);
            for _ in 0..18 {
                g.step(&Intent::default());
            }
            assert!(g.state.board.iter().all(|&x| x == 0));
            assert_eq!(g.state.lines, 4 * (repeat + 1));
        }
        assert!(g.state.message.contains("PERFECT"));
        assert!(g.state.b2b);
        assert!(g.state.score >= 9000);
    }
    #[test]
    fn t_spin_and_combo_reset() {
        let mut g = Tetris::new(7);
        empty(&mut g);
        g.state.active = Piece {
            kind: 2,
            rotation: 0,
            x: 3,
            y: 19,
        };
        for (x, y) in [(3, 19), (5, 19), (3, 21)] {
            g.state.board[y * WIDTH + x] = 1;
        }
        g.state.rotated = true;
        g.lock_piece();
        assert!(g.state.message.contains("T-SPIN"));
        assert_eq!(g.state.score, 400);
        assert_eq!(g.state.combo, -1);
    }
    #[test]
    fn drop_lands_on_ghost_and_soft_drop_scores() {
        let mut g = Tetris::new(7);
        let cells = g.ghost().cells();
        let kind = g.state.active.kind + 1;
        g.step(&Intent {
            action: true,
            ..Default::default()
        });
        for (x, y) in cells {
            assert_eq!(g.state.board[y as usize * WIDTH + x as usize], kind);
        }
        assert!(g.state.score > 0);
        let mut g = Tetris::new(7);
        for _ in 0..6 {
            g.step(&Intent {
                y: 1,
                ..Default::default()
            });
        }
        assert!(g.state.score >= 3);
    }
    #[test]
    fn sprint_goal_and_ultra_timeout() {
        let mut g = Tetris::new(7);
        g.state.mode = 1;
        g.state.lines = 40;
        g.finish_lock();
        assert_eq!(g.outcome(), "won");
        let mut g = Tetris::new(7);
        g.state.mode = 2;
        g.state.elapsed = 7199;
        g.step(&Intent::default());
        assert_eq!(g.outcome(), "won");
    }
    #[test]
    fn lock_delay_has_finite_resets_and_neutral_input_does_not_rotate() {
        let mut g = Tetris::new(7);
        g.state.active = g.ghost();
        for _ in 0..29 {
            g.step(&Intent::default());
        }
        assert_eq!(g.state.pieces, 0);
        g.step(&Intent::default());
        assert_eq!(g.state.pieces, 1);
        let mut g = Tetris::new(7);
        g.state.active = g.ghost();
        for _ in 0..50 {
            g.shift(-1);
            g.shift(1);
        }
        assert_eq!(g.state.resets, 15);
    }
    #[test]
    fn invalid_save_rejected_without_mutation() {
        let mut g = Tetris::new(7);
        let hash = g.state_hash();
        let mut s = g.capture();
        s.active.kind = 100;
        assert!(g.restore(s).is_err());
        assert_eq!(hash, g.state_hash());
        let mut s = g.capture();
        s.board.pop();
        assert!(g.restore(s).is_err());
    }
    #[test]
    fn mode_commands_and_retry_preserve_selected_rules() {
        let mut g = Tetris::new(7);
        g.step(&Intent {
            action: true,
            pointer: Some(Point::new(-100, 12)),
            ..Default::default()
        });
        assert_eq!(g.state.mode, 2);
        g.state.elapsed = 100;
        g.restart();
        assert_eq!(g.state.mode, 2);
        assert_eq!(g.state.elapsed, 0);
        assert_eq!(g.state.score, 0);
        g.step(&Intent {
            action: true,
            pointer: Some(Point::new(50, 380)),
            ..Default::default()
        });
        assert_eq!(g.state.mode, 2, "removed UI regions must not change modes");
    }
    #[test]
    fn identity_controls_fit_engine_contract() {
        let id =
            vesper3d::viewer::identity::Identity::parse(include_str!("../assets/identity.json"))
                .unwrap();
        assert_eq!(id.title, Tetris::TITLE);
    }
}
