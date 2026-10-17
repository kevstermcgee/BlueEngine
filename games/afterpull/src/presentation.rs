use super::*;
use draw::{Color, Scene};
use macroquad::prelude::{is_key_down, is_key_pressed, is_key_released, KeyCode};
const BG: Color = Color::new(0.035, 0.055, 0.09, 1.);
const FLOOR: Color = Color::new(0.085, 0.125, 0.17, 1.);
const EDGE: Color = Color::new(0.18, 0.26, 0.32, 1.);
const PAPER: Color = Color::new(0.90, 0.94, 0.91, 1.);
const MUTED: Color = Color::new(0.48, 0.61, 0.66, 1.);
const MINT: Color = Color::new(0.35, 0.94, 0.79, 1.);
const AMBER: Color = Color::new(1., 0.71, 0.33, 1.);
const RED: Color = Color::new(1., 0.37, 0.43, 1.);
const SIZE: i32 = 44;
fn center(c: Cell) -> Point {
    Point::new(360 + c.x * SIZE, 92 + c.y * SIZE)
}
fn border(s: &mut Scene, layer: i32, r: Rect, width: i32, col: Color) {
    s.rect(layer, Rect::new(r.x, r.y, r.w, width), col);
    s.rect(layer, Rect::new(r.x, r.y + r.h - width, r.w, width), col);
    s.rect(layer, Rect::new(r.x, r.y, width, r.h), col);
    s.rect(layer, Rect::new(r.x + r.w - width, r.y, width, r.h), col);
}
fn cross(s: &mut Scene, p: Point, col: Color) {
    for n in -4..=4 {
        s.rect(30, Rect::new(p.x + n, p.y + n, 2, 2), col);
        s.rect(30, Rect::new(p.x + n, p.y - n, 2, 2), col);
    }
}
fn arrow(s: &mut Scene, from: Cell, to: Cell, col: Color, jam: bool) {
    let a = center(from);
    let b = center(to);
    let dx = (b.x - a.x).signum();
    let dy = (b.y - a.y).signum();
    let a = Point::new(a.x + dx * 10, a.y + dy * 10);
    let tip = Point::new(b.x - dx * 5, b.y - dy * 5);
    s.rect(
        25,
        Rect::new(
            a.x.min(tip.x) - 1,
            a.y.min(tip.y) - 1,
            (tip.x - a.x).abs() + 3,
            (tip.y - a.y).abs() + 3,
        ),
        col,
    );
    for n in 0..7 {
        s.rect(
            26,
            Rect::new(
                tip.x - dx * n + dy * n - 1,
                tip.y - dy * n + dx * n - 1,
                3,
                3,
            ),
            col,
        );
        s.rect(
            26,
            Rect::new(
                tip.x - dx * n - dy * n - 1,
                tip.y - dy * n - dx * n - 1,
                3,
                3,
            ),
            col,
        );
    }
    if jam {
        cross(s, b, RED);
    }
}
impl draw::Game for Afterpull {
    fn show_hud() -> bool {
        false
    }
    fn menu_status(&self) -> String {
        if self.failed() {
            "Crate lost. Z undoes; R retries.".into()
        } else if self.state.finished {
            "Six rooms clear! Z undoes; R retries.".into()
        } else {
            "Fill all docks. Space+step previews.".into()
        }
    }
    fn device_input(&mut self, i: Intent) -> Intent {
        self.map_device_input(i, is_key_down(KeyCode::Space))
    }
    fn draw(&self, s: &mut Scene) {
        let st = &self.state;
        let room = &ROOMS[st.room];
        let b = &st.board;
        s.rect(-90, Rect::new(0, 0, 800, 450), BG);
        s.rect(-85, Rect::new(0, 0, 6, 450), MINT);
        s.text(5, "AFTERPULL", Point::new(34, 43), 32., PAPER);
        s.text(
            5,
            "LEAVE A LITTLE GRAVITY BEHIND",
            Point::new(36, 65),
            11.,
            MUTED,
        );
        s.text(
            5,
            format!("{:02} / 06", st.room + 1),
            Point::new(348, 35),
            19.,
            MINT,
        );
        s.text(
            5,
            format!("DOCKS  {} / {}", self.filled(), room.docks.len()),
            Point::new(492, 35),
            19.,
            AMBER,
        );
        s.rect(
            -20,
            Rect::new(28, 88, 283, 287),
            Color::new(0.065, 0.095, 0.135, 1.),
        );
        s.text(
            5,
            format!("ROOM {:02}", st.room + 1),
            Point::new(43, 113),
            12.,
            MINT,
        );
        s.text(5, room.name, Point::new(43, 140), 18., PAPER);
        s.rect(5, Rect::new(43, 153, 34, 2), MINT);
        for (n, line) in room.lesson.iter().enumerate() {
            s.text(5, *line, Point::new(43, 181 + n as i32 * 22), 15., PAPER);
        }
        s.text(
            5,
            format!("{} STEPS   /   UNLIMITED UNDO", b.turns),
            Point::new(43, 282),
            11.,
            MUTED,
        );
        if self.cleared() {
            s.text(5, "ALL DOCKS FILLED", Point::new(43, 311), 18., MINT);
            s.text(
                5,
                if st.room == 5 {
                    "ENTER: FINISH"
                } else {
                    "ENTER: NEXT ROOM"
                },
                Point::new(43, 338),
                16.,
                AMBER,
            );
            s.text(
                5,
                "Z: inspect the previous turn",
                Point::new(43, 359),
                11.,
                MUTED,
            );
        } else if self.failed() {
            s.text(5, "CRATE LOST", Point::new(43, 315), 18., RED);
            s.text(5, "Z: undo    R: retry", Point::new(43, 344), 15., PAPER);
        } else {
            let preview = st.preview.is_some();
            let forecast = st.preview.and_then(|d| self.plan(d));
            let trace = if preview {
                forecast.as_ref()
            } else if st.blocked > 0 {
                None
            } else {
                st.last.as_ref()
            };
            s.text(
                5,
                if preview {
                    "PREVIEW / NO TURN SPENT"
                } else if st.blocked > 0 {
                    "BLOCKED / NO TURN SPENT"
                } else if trace.is_some() {
                    "LAST VACUUM / WHOLE TURN BELOW"
                } else {
                    "SPACE + DIRECTION: LOOK AHEAD"
                },
                Point::new(43, 313),
                11.,
                if st.blocked > 0 { RED } else { MINT },
            );
            if let Some(plan) = trace {
                let moving = plan.pulls.iter().filter(|p| !p.jam).count();
                let jammed = plan.pulls.len() - moving;
                let arrivals = plan
                    .pulls
                    .iter()
                    .filter(|p| !p.jam && room.docks.contains(&p.to))
                    .count();
                let danger = plan
                    .pulls
                    .iter()
                    .any(|p| !p.jam && room.pits.contains(&p.to));
                s.text(
                    5,
                    format!("{moving} PULL  {jammed} JAMMED  {arrivals} DOCK"),
                    Point::new(43, 335),
                    13.,
                    PAPER,
                );
                s.text(
                    5,
                    if danger {
                        "PIT WARNING: a crate will fall!"
                    } else if preview {
                        "Release Space. Tap to commit."
                    } else if jammed > 0 {
                        "Jammed crates shield each other."
                    } else if arrivals > 0 {
                        "Dock reached! It can move again."
                    } else {
                        "Z undoes every pull in this turn."
                    },
                    Point::new(43, 357),
                    12.,
                    if danger { RED } else { MUTED },
                );
            } else {
                s.text(
                    5,
                    if preview {
                        "That step is blocked."
                    } else {
                        "Release Space. Tap to commit."
                    },
                    Point::new(43, 335),
                    12.,
                    MUTED,
                );
                s.text(
                    5,
                    "Crates cannot be pushed.",
                    Point::new(43, 357),
                    12.,
                    MUTED,
                );
            }
        }
        // Strict top-down tiles; exactly the same coordinates drive collision and drawing.
        s.rect(
            -12,
            Rect::new(334, 66, 316, 316),
            Color::new(0.025, 0.04, 0.06, 1.),
        );
        for y in 0..7 {
            for x in 0..7 {
                let c = Cell::new(x, y);
                let p = center(c);
                let r = Rect::new(p.x - 21, p.y - 21, 42, 42);
                if Self::wall(st.room, c) {
                    s.rect(-5, r, Color::new(0.12, 0.18, 0.23, 1.));
                    s.rect(-4, Rect::new(r.x + 2, r.y + 2, 38, 3), EDGE);
                    s.rect(
                        -4,
                        Rect::new(r.x + 3, r.y + 35, 36, 3),
                        Color::new(0.07, 0.105, 0.14, 1.),
                    );
                    s.rect(
                        -3,
                        Rect::new(p.x - 2, p.y - 1, 4, 2),
                        Color::new(0.21, 0.29, 0.34, 1.),
                    );
                } else {
                    s.rect(
                        -5,
                        r,
                        if (x + y) % 2 == 0 {
                            FLOOR
                        } else {
                            Color::new(0.095, 0.14, 0.18, 1.)
                        },
                    );
                    s.rect(-4, Rect::new(r.x + 3, r.y + 3, 2, 2), EDGE);
                    if room.pits.contains(&c) {
                        s.rect(
                            -3,
                            Rect::new(p.x - 18, p.y - 18, 36, 36),
                            Color::new(0.012, 0.018, 0.035, 1.),
                        );
                        border(s, -2, Rect::new(p.x - 19, p.y - 19, 38, 38), 1, RED);
                        for n in 0..4 {
                            s.rect(-1, Rect::new(p.x - 15 + n * 9, p.y - 18, 4, 3), AMBER);
                            s.rect(-1, Rect::new(p.x - 15 + n * 9, p.y + 15, 4, 3), RED);
                        }
                        s.text(0, "X", Point::new(p.x - 4, p.y + 4), 12., RED);
                    }
                    if room.docks.contains(&c) {
                        border(
                            s,
                            0,
                            Rect::new(p.x - 17, p.y - 17, 34, 34),
                            2,
                            if b.crates.contains(&c) { MINT } else { AMBER },
                        );
                        s.circle(0, p, 6., AMBER);
                        s.circle(1, p, 3., FLOOR);
                    }
                }
            }
        }
        // Suction pulse and route read the committed plan; this never changes the rules.
        if st.pulse > 0 {
            if let Some(plan) = &st.last {
                let p = center(plan.origin);
                let radius = 5 + (30 - st.pulse) as i32;
                border(
                    s,
                    2,
                    Rect::new(p.x - radius, p.y - radius, radius * 2, radius * 2),
                    1,
                    Color::new(0.35, 0.94, 0.79, st.pulse as f32 / 40.),
                );
                for pull in &plan.pulls {
                    arrow(
                        s,
                        pull.from,
                        pull.to,
                        if pull.jam {
                            RED
                        } else {
                            Color::new(0.35, 0.94, 0.79, st.pulse as f32 / 40.)
                        },
                        pull.jam,
                    );
                }
            }
        }
        for (index, c) in b.crates.iter().enumerate() {
            let mut p = center(*c);
            if st.pulse > 0
                && room.docks.contains(c)
                && st
                    .last
                    .as_ref()
                    .is_some_and(|plan| plan.pulls.iter().any(|pull| !pull.jam && pull.to == *c))
            {
                let radius = 18 + (30 - st.pulse) as i32 / 4;
                border(
                    s,
                    14,
                    Rect::new(p.x - radius, p.y - radius, radius * 2, radius * 2),
                    2,
                    Color::new(0.35, 0.94, 0.79, st.pulse as f32 / 30.),
                );
            }
            if let Some(last) = &st.last {
                if st.pulse > 22 {
                    if let Some(pull) = last.pulls.iter().find(|a| a.to == *c && !a.jam) {
                        let from = center(pull.from);
                        let remain = (st.pulse - 22) as i32;
                        p.x += (from.x - p.x) * remain / 8;
                        p.y += (from.y - p.y) * remain / 8;
                    }
                }
            }
            if room.pits.contains(c) {
                s.circle(12, p, 7., RED);
                continue;
            }
            s.rect(
                9,
                Rect::new(p.x - 14, p.y - 11, 30, 29),
                Color::new(0.025, 0.04, 0.06, 0.7),
            );
            s.rect(
                10,
                Rect::new(p.x - 15, p.y - 15, 30, 30),
                if room.docks.contains(c) { MINT } else { AMBER },
            );
            s.rect(
                11,
                Rect::new(p.x - 12, p.y - 12, 24, 24),
                Color::new(0.39, 0.25, 0.13, 1.),
            );
            border(
                s,
                12,
                Rect::new(p.x - 10, p.y - 10, 20, 20),
                1,
                Color::new(0.73, 0.49, 0.24, 1.),
            );
            s.text(
                13,
                format!("{}", index + 1),
                Point::new(p.x - 4, p.y + 5),
                13.,
                PAPER,
            );
            s.rect(
                12,
                Rect::new(p.x - 12, p.y - 13, 24, 2),
                Color::new(1., 0.83, 0.52, 1.),
            );
        }
        let mut p = center(b.player);
        if let Some(last) = &st.last {
            if st.pulse > 24 {
                let from = center(last.origin);
                let t = (st.pulse - 24) as i32;
                p.x += (from.x - p.x) * t / 6;
                p.y += (from.y - p.y) * t / 6;
            }
        }
        s.circle(16, Point::new(p.x + 1, p.y + 3), 15., BG);
        s.circle(17, p, 14., if st.blocked > 0 { RED } else { MINT });
        s.circle(18, p, 10., PAPER);
        s.rect(
            19,
            Rect::new(p.x - 9, p.y - 5, 18, 9),
            Color::new(0.06, 0.20, 0.24, 1.),
        );
        s.rect(20, Rect::new(p.x - 5, p.y - 3, 3, 3), MINT);
        s.rect(20, Rect::new(p.x + 3, p.y - 3, 3, 3), MINT);
        if let Some(dir) = st.preview {
            if let Some(plan) = self.plan(dir) {
                let p = center(plan.player);
                border(s, 24, Rect::new(p.x - 17, p.y - 17, 34, 34), 2, MINT);
                let o = center(plan.origin);
                s.circle(23, o, 5., MINT);
                for pull in &plan.pulls {
                    let a = center(pull.from);
                    let dx = (a.x - o.x).signum();
                    let dy = (a.y - o.y).signum();
                    for n in (8..(a.x - o.x).abs().max((a.y - o.y).abs()) - 10).step_by(8) {
                        s.rect(
                            22,
                            Rect::new(o.x + dx * n - 1, o.y + dy * n - 1, 2, 2),
                            MINT,
                        );
                    }
                    arrow(
                        s,
                        pull.from,
                        pull.to,
                        if pull.jam || room.pits.contains(&pull.to) {
                            RED
                        } else {
                            MINT
                        },
                        pull.jam,
                    );
                }
            } else {
                cross(s, center(b.player.offset(DIRS[dir])), RED);
            }
        }
        for n in 0..6 {
            let p = Point::new(707, 104 + n as i32 * 40);
            let col = if n < st.room {
                MINT
            } else if n == st.room {
                AMBER
            } else {
                EDGE
            };
            s.circle(2, p, 12., col);
            s.circle(3, p, 10., BG);
            s.text(
                4,
                format!("{}", n + 1),
                Point::new(p.x - 4, p.y + 5),
                13.,
                col,
            );
            if n < 5 {
                s.rect(1, Rect::new(p.x - 1, p.y + 15, 2, 10), EDGE);
            }
        }
        s.text(5, "JOURNEY", Point::new(675, 73), 10., MUTED);
        s.text(
            5,
            "MINT  pull   /   RED X  jam",
            Point::new(348, 55),
            11.,
            MUTED,
        );
        s.text(
            5,
            "WASD / ARROWS  STEP     SPACE + DIRECTION  PREVIEW     Z  UNDO     R  RETRY",
            Point::new(32, 425),
            13.,
            PAPER,
        );
        s.text(
            5,
            "ESC  PAUSE / RESUME    K  SAVE    L  LOAD    M  SOUND    NO TIME OR MOVE LIMIT",
            Point::new(32, 444),
            11.,
            MUTED,
        );
    }
}

impl Afterpull {
    pub fn map_device_input(&mut self, mut i: Intent, space_down: bool) -> Intent {
        // Treat a Space release frame conservatively: a direction pressed
        // during that same frame must not accidentally spend a turn.
        let preview =
            space_down || is_key_pressed(KeyCode::Space) || is_key_released(KeyCode::Space);
        i.sprint = space_down;
        i.action = false;
        i.pointer = None;
        if is_key_pressed(KeyCode::Z) {
            return Self::command(-11, 0);
        }
        if is_key_pressed(KeyCode::Enter) {
            return Self::command(-12, 0);
        }
        let keys = [
            (KeyCode::W, KeyCode::Up),
            (KeyCode::D, KeyCode::Right),
            (KeyCode::S, KeyCode::Down),
            (KeyCode::A, KeyCode::Left),
        ];
        for (dir, (a, b)) in keys.iter().enumerate() {
            if is_key_pressed(*a) || is_key_pressed(*b) {
                i.action = true;
                i.pointer = if preview {
                    Self::preview(dir).pointer
                } else {
                    Self::direction(dir).pointer
                };
                break;
            }
        }
        i
    }
}
