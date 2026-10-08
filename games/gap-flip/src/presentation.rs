//! Read-only presentation: grid, matched symbols, exact destinations and exchanging dotted bands.
use super::*;
use draw::{Color, Scene};
const BG: Color = Color::new(0.035, 0.06, 0.10, 1.);
const PANEL: Color = Color::new(0.07, 0.105, 0.16, 1.);
const FLOOR: Color = Color::new(0.105, 0.15, 0.21, 1.);
const EDGE: Color = Color::new(0.19, 0.26, 0.34, 1.);
const TEXT: Color = Color::new(0.91, 0.94, 0.96, 1.);
const MUTED: Color = Color::new(0.56, 0.65, 0.73, 1.);
const CYAN: Color = Color::new(0.30, 0.89, 0.85, 1.);
const AMBER: Color = Color::new(1., 0.74, 0.36, 1.);
const PALETTE: [Color; 4] = [
    CYAN,
    AMBER,
    Color::new(1., 0.49, 0.64, 1.),
    Color::new(0.69, 0.61, 1., 1.),
];
fn alpha(mut color: Color, a: f32) -> Color {
    color.a = a;
    color
}
fn symbol(token: usize) -> &'static str {
    ["A", "B", "C", "D"][token]
}
fn lerp(a: i32, b: i32, t: f32) -> i32 {
    (a as f32 + (b - a) as f32 * t).round() as i32
}
fn token(s: &mut Scene, point: Point, i: usize, selected: bool, ghost: bool) {
    let color = PALETTE[i];
    if ghost {
        s.circle(7, point, 20., alpha(color, 0.5));
        s.circle(8, point, 17., FLOOR);
        s.text(
            9,
            symbol(i),
            Point::new(point.x - 6, point.y + 6),
            19.,
            alpha(color, 0.8),
        );
        for (x, y) in [(-22, -22), (19, -22), (-22, 19), (19, 19)] {
            s.rect(8, Rect::new(point.x + x, point.y + y, 3, 3), color);
        }
    } else {
        s.circle(8, Point::new(point.x, point.y + 3), 20., alpha(BG, 0.8));
        if selected {
            s.circle(9, point, 23., TEXT);
            s.circle(10, point, 21., BG);
        }
        s.circle(11, point, 18., color);
        s.circle(12, point, 14., PANEL);
        s.circle(
            12,
            Point::new(point.x - 7, point.y - 9),
            2.,
            alpha(TEXT, 0.8),
        );
        s.text(
            13,
            symbol(i),
            Point::new(point.x - 6, point.y + 6),
            19.,
            color,
        );
    }
}
fn dotted(s: &mut Scene, room: &Room, from: u8, axis: Axis, gap: Corridor, t: f32) {
    let p = room.center(from);
    let q = room.center(gap.destination);
    let (dx, dy) = axis.delta();
    // A band's dots reflect from one side to the opposite side, preserving its color and length.
    for (count, sign, color) in [(gap.before, -1, CYAN), (gap.after, 1, AMBER)] {
        for cell in 1..=i32::from(count) {
            for offset in [-13, 0, 13] {
                let distance = sign * (cell * CELL + offset);
                let x = lerp(p.x + distance * dx, q.x - distance * dx, t);
                let y = lerp(p.y + distance * dy, q.y - distance * dy, t);
                s.circle(5, Point::new(x, y), 2.4, color);
            }
        }
    }
}
fn button(s: &mut Scene, r: Rect, label: &str, enabled: bool, active: bool) {
    let edge = if active {
        CYAN
    } else if enabled {
        EDGE
    } else {
        PANEL
    };
    s.rect(20, Rect::new(r.x, r.y + 3, r.w, r.h), BG);
    s.rect(21, r, edge);
    s.rect(
        22,
        Rect::new(r.x + 1, r.y + 1, r.w - 2, r.h - 2),
        if active {
            Color::new(0.10, 0.23, 0.27, 1.)
        } else {
            PANEL
        },
    );
    s.text(
        23,
        label,
        Point::new(r.x + 12, r.y + r.h / 2 + 6),
        16.,
        if enabled { TEXT } else { MUTED },
    );
}
impl draw::Game for GapFlip {
    fn draw(&self, s: &mut Scene) {
        let room = self.room();
        let o = room.origin();
        s.rect(-99, Rect::new(0, 0, 800, 450), BG);
        s.rect(-90, Rect::new(24, 43, 92, 2), CYAN);
        s.text(20, "TRADE EMPTY SPACE", Point::new(182, 29), 13., MUTED);
        s.text(
            20,
            format!("ROOM {:02} / 10", self.state.room + 1),
            Point::new(618, 30),
            18.,
            CYAN,
        );
        s.rect(-20, Rect::new(40, 66, 334, 322), EDGE);
        s.rect(-19, Rect::new(41, 67, 332, 320), PANEL);
        // Calm framing and four registration marks keep even the smallest room intentional.
        for (x, y) in [(48, 74), (363, 74), (48, 377), (363, 377)] {
            s.rect(-15, Rect::new(x, y, 3, 3), MUTED);
        }
        for y in 0..room.height() {
            for x in 0..room.width() {
                let r = Rect::new(o.x + x * CELL + 2, o.y + y * CELL + 2, CELL - 4, CELL - 4);
                if room.floor(x, y) {
                    s.rect(0, r, FLOOR);
                    s.rect(1, Rect::new(r.x, r.y, r.w, 1), alpha(TEXT, 0.06));
                } else {
                    s.rect(0, r, BG);
                    s.rect(1, Rect::new(r.x + 3, r.y + 3, r.w - 6, r.h - 6), EDGE);
                    s.rect(
                        2,
                        Rect::new(r.x + 5, r.y + 5, r.w - 10, 2),
                        alpha(MUTED, 0.3),
                    );
                    s.rect(
                        2,
                        Rect::new(r.x + 5, r.y + 7, 2, r.h - 14),
                        alpha(MUTED, 0.15),
                    );
                    s.rect(2, Rect::new(r.x + 11, r.y + 20, r.w - 22, 3), BG);
                }
            }
        }
        for (i, &goal) in room.goals.iter().enumerate() {
            let p = room.center(goal);
            let c = PALETTE[i];
            let matched = self.state.tokens[i] == goal;
            s.circle(3, p, 21., alpha(c, if matched { 0.65 } else { 0.35 }));
            s.circle(4, p, 18., FLOOR);
            s.text(
                4,
                symbol(i),
                Point::new(p.x - 6, p.y + 6),
                19.,
                alpha(c, 0.6),
            );
            if matched {
                s.rect(4, Rect::new(p.x - 4, p.y + 21, 8, 2), c);
            }
        }
        if let Some(gap) = self.preview() {
            let i = self.state.selected.unwrap();
            let from = self.state.tokens[i];
            dotted(s, room, from, self.state.axis.unwrap(), gap, 0.);
            token(s, room.center(gap.destination), i, false, true);
        }
        let ease = self.state.exchange.as_ref().map(|e| {
            let t = 1. - f32::from(e.remaining) / f32::from(EXCHANGE_TICKS);
            t * t * (3. - 2. * t)
        });
        if let (Some(e), Some(t)) = (&self.state.exchange, ease) {
            dotted(
                s,
                room,
                e.from,
                e.axis,
                Corridor {
                    destination: e.to,
                    before: e.before,
                    after: e.after,
                },
                t,
            );
        }
        for (i, &cell) in self.state.tokens.iter().enumerate() {
            let mut p = room.center(cell);
            if let (Some(e), Some(t)) = (&self.state.exchange, ease) {
                if i == e.token {
                    let from = room.center(e.from);
                    p = Point::new(lerp(from.x, p.x, t), lerp(from.y, p.y, t));
                }
            }
            token(s, p, i, self.state.selected == Some(i), false);
        }
        for i in 0..10 {
            s.rect(
                20,
                Rect::new(405 + i * 35, 64, 27, 4),
                if i < self.state.room as i32 {
                    CYAN
                } else if i == self.state.room as i32 {
                    AMBER
                } else {
                    EDGE
                },
            );
        }
        s.text(20, room.name, Point::new(405, 101), 23., TEXT);
        let matched = self
            .state
            .tokens
            .iter()
            .zip(room.goals)
            .filter(|(a, b)| a == b)
            .count();
        s.text(
            20,
            format!("{} / {} SYMBOLS MATCHED", matched, room.goals.len()),
            Point::new(405, 127),
            14.,
            CYAN,
        );
        s.text(20, room.hint[0], Point::new(405, 157), 16., TEXT);
        s.text(20, room.hint[1], Point::new(405, 180), 16., MUTED);
        if let Some(gap) = self.preview() {
            let before = if self.state.axis == Some(Axis::Horizontal) {
                "LEFT"
            } else {
                "ABOVE"
            };
            let after = if self.state.axis == Some(Axis::Horizontal) {
                "RIGHT"
            } else {
                "BELOW"
            };
            s.text(
                20,
                format!("{before} {}", gap.before),
                Point::new(405, 207),
                15.,
                CYAN,
            );
            s.text(
                20,
                format!("{after} {}", gap.after),
                Point::new(560, 207),
                15.,
                AMBER,
            );
        } else if let Some(e) = &self.state.exchange {
            s.text(
                20,
                format!(
                    "{} : {}   >   {} : {}",
                    e.before, e.after, e.after, e.before
                ),
                Point::new(405, 207),
                16.,
                CYAN,
            );
        } else {
            s.text(
                20,
                if self.state.selected.is_some() {
                    "Choose an axis. The dots show the gap."
                } else {
                    "Select a token to see its possible flips."
                },
                Point::new(405, 207),
                15.,
                MUTED,
            );
        }
        for (axis, label) in [
            (Axis::Horizontal, "Horizontal Flip"),
            (Axis::Vertical, "Vertical Flip"),
        ] {
            let gap = self
                .state
                .selected
                .map(|i| corridor(room, &self.state.tokens, i, axis));
            let enabled = gap.is_some_and(|g| g.before != g.after);
            let label = if gap.is_some() && !enabled {
                if axis == Axis::Horizontal {
                    "Horizontal: equal"
                } else {
                    "Vertical: equal"
                }
            } else {
                label
            };
            button(
                s,
                axis.button(),
                label,
                enabled,
                self.state.axis == Some(axis),
            );
        }
        button(s, UNDO, "Undo", !self.state.history.is_empty(), false);
        button(s, RESTART, "Restart", true, false);
        s.text(
            20,
            format!("{} flips", self.state.history.len()),
            Point::new(654, 298),
            16.,
            MUTED,
        );
        if self.solved() {
            button(
                s,
                NEXT,
                if self.state.room + 1 == ROOMS.len() {
                    "All ten matched!  Finish >"
                } else {
                    "Room matched!  Next room >"
                },
                true,
                true,
            );
        } else if self.preview().is_some() {
            s.text(
                20,
                "Click the outlined ghost to commit.",
                Point::new(405, 343),
                16.,
                TEXT,
            );
            s.text(
                20,
                "Cyan and amber dots trade sides.",
                Point::new(405, 366),
                15.,
                MUTED,
            );
        } else {
            s.text(
                20,
                "Every move can be undone.",
                Point::new(405, 343),
                16.,
                TEXT,
            );
            s.text(
                20,
                "No time limit. No wrong turns.",
                Point::new(405, 366),
                15.,
                MUTED,
            );
        }
        s.text(
            20,
            "RINGS ARE GOALS. NEIGHBORS ARE WALLS.",
            Point::new(57, 381),
            12.,
            MUTED,
        );
    }
    fn menu_status(&self) -> String {
        if self.state.finished {
            "Ten rooms matched. Well played.".into()
        } else {
            "Click token > axis > ghost. Undo freely.".into()
        }
    }
}
