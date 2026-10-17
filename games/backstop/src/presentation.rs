//! Flat loading-bay presentation; interpolation reads the committed board.
use super::*;
use vesper3d::two_d::draw::{Color, Game, Scene};
const BG: Color = Color::new(0.035, 0.060, 0.085, 1.);
const PANEL: Color = Color::new(0.065, 0.105, 0.140, 1.);
const FLOOR: Color = Color::new(0.110, 0.170, 0.205, 1.);
const WALL: Color = Color::new(0.190, 0.265, 0.310, 1.);
const EDGE: Color = Color::new(0.270, 0.360, 0.400, 1.);
const TEXT: Color = Color::new(0.90, 0.94, 0.91, 1.);
const MUTED: Color = Color::new(0.53, 0.66, 0.70, 1.);
const AMBER: Color = Color::new(1.0, 0.69, 0.29, 1.);
const CYAN: Color = Color::new(0.32, 0.87, 0.77, 1.);
const BLUE: Color = Color::new(0.33, 0.65, 0.98, 1.);
const RED: Color = Color::new(1.0, 0.40, 0.40, 1.);
const CELL: i32 = 43;
const OX: i32 = 53;
const OY: i32 = 81;
fn cell(c: u8, inset: i32) -> Rect {
    let (x, y) = xy(c);
    Rect::new(
        OX + x * CELL + inset,
        OY + y * CELL + inset,
        CELL - 2 * inset,
        CELL - 2 * inset,
    )
}
fn outline(s: &mut Scene<'_>, layer: i32, r: Rect, c: Color, w: i32) {
    s.rect(layer, Rect::new(r.x, r.y, r.w, w), c);
    s.rect(layer, Rect::new(r.x, r.y + r.h - w, r.w, w), c);
    s.rect(layer, Rect::new(r.x, r.y, w, r.h), c);
    s.rect(layer, Rect::new(r.x + r.w - w, r.y, w, r.h), c);
}
fn text(s: &mut Scene<'_>, x: i32, y: i32, size: f32, value: impl Into<String>, color: Color) {
    s.text(30, value, Point::new(x, y), size, color);
}
fn button(s: &mut Scene<'_>, r: Rect, label: &str, active: bool) {
    s.rect(20, r, if active { WALL } else { PANEL });
    outline(s, 21, r, if active { CYAN } else { EDGE }, 1);
    text(
        s,
        r.x + 12,
        r.y + 9,
        15.,
        label,
        if active { TEXT } else { MUTED },
    );
}
impl Game for Backstop {
    fn show_hud() -> bool {
        false
    }
    fn menu_status(&self) -> String {
        if self.outcome() == "won" {
            "All eight bays delivered. Excellent backstop work.".into()
        } else {
            "Space shoves exactly 1 + clear cells BEHIND you.\nArrows / WASD walk. Z undo. R retry. K save / L load.".into()
        }
    }
    fn device_input(&mut self, mut i: Intent) -> Intent {
        use macroquad::prelude::{is_key_pressed, KeyCode};
        let shortcut = if is_key_pressed(KeyCode::Z) {
            Some(b'z')
        } else if is_key_pressed(KeyCode::Enter) {
            Some(b'n')
        } else {
            None
        };
        if let Some(c) = shortcut {
            let mut mapped = command(c);
            // The shared client recognizes a mapped target by its difference from
            // the live cursor. Keep the command inside its button in this corner case.
            if mapped.pointer == i.pointer {
                if let Some(p) = mapped.pointer.as_mut() {
                    p.x += 1;
                }
            }
            return mapped;
        }
        if is_key_pressed(KeyCode::Space) {
            i.action = true;
            i.pointer = None;
        }
        i
    }
    fn draw(&self, s: &mut Scene<'_>) {
        s.rect(-90, Rect::new(0, 0, 800, 450), BG);
        s.rect(-89, Rect::new(0, 0, 8, 450), AMBER);
        text(s, 30, 17, 30., "BACKSTOP", TEXT);
        text(
            s,
            32,
            51,
            12.,
            "EXACT-DISTANCE CARGO / NO CLOCK. NO PENALTIES.",
            MUTED,
        );
        text(
            s,
            485,
            22,
            14.,
            format!("BAY {:02} / 08", self.state.room + 1),
            CYAN,
        );
        for n in 0..8 {
            s.rect(
                0,
                Rect::new(602 + n * 19, 23, 13, 13),
                if n < self.state.room as i32 {
                    CYAN
                } else if n == self.state.room as i32 {
                    AMBER
                } else {
                    WALL
                },
            );
        }
        s.rect(
            -3,
            Rect::new(OX - 5, OY - 5, 9 * CELL + 10, 7 * CELL + 10),
            PANEL,
        );
        for c in 0..63 {
            let r = cell(c, 1);
            let wall = tile(self.state.room, c) == b'#';
            s.rect(0, r, if wall { WALL } else { FLOOR });
            if wall {
                s.rect(1, Rect::new(r.x, r.y, r.w, 3), EDGE);
                s.rect(1, Rect::new(r.x + 4, r.y + 7, 2, 21), PANEL);
                if c % 2 == 0 {
                    s.rect(1, Rect::new(r.x + 26, r.y + 30, 8, 2), EDGE);
                }
            } else {
                s.rect(1, Rect::new(r.x + 3, r.y + 3, 2, 2), EDGE);
                if matches!(tile(self.state.room, c), b'.' | b'*') {
                    outline(s, 2, cell(c, 7), CYAN, 2);
                    s.circle(2, Point::new(r.x + 20, r.y + 20), 4., CYAN);
                }
            }
        }
        let v = self.preview();
        if let Some(v) = &v {
            for (n, &c) in v.rear.iter().enumerate() {
                s.rect(3, cell(c, 5), Color::new(0.11, 0.30, 0.43, 1.));
                outline(s, 4, cell(c, 5), BLUE, 1);
                let r = cell(c, 0);
                text(s, r.x + 18, r.y + 15, 14., format!("{}", n + 1), BLUE);
            }
            for (n, &c) in v.route.iter().enumerate() {
                let r = cell(c, 3);
                let col = if v.valid { AMBER } else { RED };
                outline(s, 5, r, col, if Some(c) == v.landing { 3 } else { 1 });
                // Labels occupy a corner, leaving targets and crates readable.
                text(s, r.x + 4, r.y + 3, 12., format!("{}", n + 1), col);
            }
            if let Some(c) = v.obstruction {
                let r = cell(c, 8);
                outline(s, 16, r, RED, 3);
                text(s, r.x + 9, r.y + 7, 20., "X", RED);
            }
        }
        for (n, &c) in self.state.frame.crates.iter().enumerate() {
            let mut r = cell(c, 7);
            if self.state.animation > 0 {
                if let Some((from, to)) = self.state.slide {
                    if c == to {
                        let a = cell(from, 7);
                        let t = i32::from(self.state.animation);
                        r.x += (a.x - r.x) * t / 8;
                        r.y += (a.y - r.y) * t / 8;
                    }
                }
            }
            let docked = matches!(tile(self.state.room, c), b'.' | b'*');
            let col = if docked { CYAN } else { AMBER };
            s.rect(7, Rect::new(r.x + 2, r.y + 4, r.w, r.h), BG);
            s.rect(8, r, col);
            s.rect(9, Rect::new(r.x + 3, r.y + 3, r.w - 6, 3), TEXT);
            s.rect(
                9,
                Rect::new(r.x + 4, r.y + 11, r.w - 8, 12),
                if docked {
                    Color::new(0.10, 0.39, 0.35, 1.)
                } else {
                    Color::new(0.49, 0.30, 0.13, 1.)
                },
            );
            text(
                s,
                r.x + 10,
                r.y + 10,
                13.,
                format!("{}", (b'A' + n as u8) as char),
                TEXT,
            );
            if docked {
                s.circle(10, Point::new(r.x + r.w - 4, r.y + r.h - 4), 3., TEXT);
            }
        }
        let p = cell(self.state.frame.player, 0);
        let cx = p.x + 21;
        let cy = p.y + 21;
        s.circle(11, Point::new(cx + 1, cy + 3), 14., BG);
        s.circle(12, Point::new(cx, cy), 13., TEXT);
        s.circle(13, Point::new(cx, cy), 8., BLUE);
        let (dx, dy) = DIRS[self.state.frame.facing as usize];
        s.rect(
            14,
            Rect::new(cx + dx * 11 - 3, cy + dy * 11 - 3, 7, 7),
            TEXT,
        );
        s.circle(
            15,
            Point::new(cx + dx * 19, cy + dy * 19),
            3.,
            if self.state.rejected > 0 { RED } else { AMBER },
        );

        s.rect(0, Rect::new(471, 81, 292, 302), PANEL);
        text(s, 486, 95, 12., "DISPATCH / SPATIAL LOGISTICS", MUTED);
        text(s, 486, 117, 19., ROOMS[self.state.room].title, TEXT);
        text(
            s,
            486,
            149,
            14.,
            format!(
                "DOCKED  {} / {}     SHOVES  {}",
                self.docked(),
                self.state.frame.crates.len(),
                self.state.frame.pushes
            ),
            CYAN,
        );
        s.rect(1, Rect::new(486, 174, 262, 1), EDGE);
        if self.solved() {
            text(
                s,
                486,
                191,
                24.,
                if self.state.room == 7 {
                    "SHIFT COMPLETE"
                } else {
                    "BAY DELIVERED"
                },
                CYAN,
            );
            text(s, 486, 224, 14., "All cargo is on target.", TEXT);
            text(
                s,
                486,
                250,
                13.,
                "No timer. Every idea is reversible.",
                MUTED,
            );
            button(
                s,
                NEXT,
                if self.state.room == 7 {
                    "ALL EIGHT BAYS COMPLETE"
                } else {
                    "ENTER  /  NEXT BAY  >"
                },
                true,
            );
        } else if self.state.deadlocked {
            text(s, 486, 192, 23., "NO ROUTE TO GOAL", RED);
            text(s, 486, 225, 14., "This arrangement cannot finish.", TEXT);
            text(s, 486, 249, 14., "Z undo / R retry this bay.", TEXT);
            text(
                s,
                486,
                304,
                13.,
                "Nothing lost. Try another backstop.",
                MUTED,
            );
        } else {
            if let Some(v) = &v {
                text(s, 486, 188, 12., "REAR CLEARANCE  +  1  =  SHOVE", MUTED);
                text(
                    s,
                    501,
                    209,
                    37.,
                    format!("{}  +  1  =  {}", v.rear.len(), v.distance),
                    if v.valid { AMBER } else { RED },
                );
                text(
                    s,
                    486,
                    256,
                    14.,
                    if self.state.rejected > 0 {
                        "REJECTED / no cargo moved"
                    } else if v.valid {
                        "SPACE  /  LAUNCH CARGO"
                    } else {
                        "BLOCKED / exact route required"
                    },
                    if v.valid && self.state.rejected == 0 {
                        CYAN
                    } else {
                        RED
                    },
                );
                text(
                    s,
                    486,
                    280,
                    12.,
                    if v.crate_backstop {
                        "A crate is your backstop."
                    } else {
                        "Blue = rear / Gold = forward"
                    },
                    if v.crate_backstop { BLUE } else { MUTED },
                );
            } else {
                text(s, 486, 193, 20., "ARRANGE YOUR RUNWAY", AMBER);
                text(s, 486, 228, 14., "Face a crate to see its route.", TEXT);
                text(
                    s,
                    486,
                    252,
                    13.,
                    "Walls and crates stop the measure.",
                    MUTED,
                );
                text(
                    s,
                    486,
                    278,
                    13.,
                    "Cargo moves 1 + clear cells behind.",
                    MUTED,
                );
            }
            // Wrap room hints explicitly for the fixed-width native bitmap font.
            let hint = ROOMS[self.state.room].hint;
            let mut line = String::new();
            let mut y = 305;
            for word in hint.split_whitespace() {
                if line.len() + word.len() > 35 {
                    text(s, 486, y, 12., std::mem::take(&mut line), TEXT);
                    y += 16;
                }
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(word);
            }
            text(s, 486, y, 12., line, TEXT);
        }
        button(s, UNDO, "Z  UNDO", !self.state.history.is_empty());
        button(s, RETRY, "R  RETRY", true);
        text(
            s,
            31,
            412,
            14.,
            "WASD / ARROWS  walk     SPACE  shove     Z  undo     R  retry",
            TEXT,
        );
        text(
            s,
            31,
            434,
            12.,
            "ENTER next bay    K save / L load    ESC pause    M sound    F fullscreen",
            MUTED,
        );
    }
}
