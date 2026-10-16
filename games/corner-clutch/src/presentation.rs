use super::*;
use draw::{Color, Scene};
const BG: Color = Color::new(0.035, 0.055, 0.085, 1.);
const PANEL: Color = Color::new(0.065, 0.092, 0.13, 1.);
const LINE: Color = Color::new(0.15, 0.21, 0.27, 1.);
const TEXT: Color = Color::new(0.90, 0.93, 0.92, 1.);
const MUTED: Color = Color::new(0.52, 0.64, 0.68, 1.);
const GOLD: Color = Color::new(1., 0.76, 0.36, 1.);
const MINT: Color = Color::new(0.32, 0.87, 0.73, 1.);
const RED: Color = Color::new(1., 0.37, 0.40, 1.);
const COLORS: [Color; 4] = [
    Color::new(0.92, 0.57, 0.29, 1.),
    Color::new(0.27, 0.71, 0.70, 1.),
    Color::new(0.58, 0.52, 0.87, 1.),
    Color::new(0.80, 0.43, 0.60, 1.),
];
fn faded(c: Color, a: f32) -> Color {
    Color { a, ..c }
}
fn dark(c: Color, n: f32) -> Color {
    Color::new(c.r * n, c.g * n, c.b * n, c.a)
}
fn outline(s: &mut Scene, layer: i32, r: Rect, c: Color, t: i32) {
    s.rect(layer, Rect::new(r.x, r.y, r.w, t), c);
    s.rect(layer, Rect::new(r.x, r.y + r.h - t, r.w, t), c);
    s.rect(layer, Rect::new(r.x, r.y, t, r.h), c);
    s.rect(layer, Rect::new(r.x + r.w - t, r.y, t, r.h), c);
}
fn label(s: &mut Scene, txt: impl Into<String>, x: i32, y: i32, size: f32, c: Color) {
    s.text(30, txt, Point::new(x, y), size, c);
}
fn wrap(s: &mut Scene, text: &str, x: i32, y: i32, width: usize, size: f32, c: Color) {
    let mut line = String::new();
    let mut row = 0;
    for word in text.split_whitespace() {
        if line.len() + word.len() + 1 > width && !line.is_empty() {
            label(s, &line, x, y + row * 20, size, c);
            line.clear();
            row += 1;
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        label(s, line, x, y + row * 20, size, c);
    }
}
fn cell_rect(c: Cell, inset: i32) -> Rect {
    Rect::new(
        ORIGIN[0] + c[0] * TILE + inset,
        ORIGIN[1] + c[1] * TILE + inset,
        TILE - 2 * inset,
        TILE - 2 * inset,
    )
}
fn grippers(s: &mut Scene, r: Rect, c: Color) {
    for (x, y, sx, sy) in [
        (r.x, r.y, 1, 1),
        (r.x + r.w - 1, r.y, -1, 1),
        (r.x, r.y + r.h - 1, 1, -1),
        (r.x + r.w - 1, r.y + r.h - 1, -1, -1),
    ] {
        s.rect(22, Rect::new(x.min(x + sx * 8), y, 9, 3), c);
        s.rect(22, Rect::new(x, y.min(y + sy * 8), 3, 9), c);
    }
}
fn button(s: &mut Scene, r: Rect, text: &str, active: bool, hover: bool) {
    s.rect(
        20,
        r,
        if active {
            if hover {
                dark(MINT, 0.28)
            } else {
                LINE
            }
        } else {
            PANEL
        },
    );
    outline(
        s,
        21,
        r,
        if active {
            if hover {
                MINT
            } else {
                MUTED
            }
        } else {
            LINE
        },
        1,
    );
    label(
        s,
        text,
        r.x + 12,
        r.y + 21,
        14.,
        if active { TEXT } else { MUTED },
    );
}
impl draw::Game for Clutch {
    fn show_hud() -> bool {
        false
    }
    fn start_automatically() -> bool {
        true
    }
    fn menu_status(&self) -> String {
        if self.outcome() == "won" {
            "Eight rooms solved! Z undo; R retry.".into()
        } else {
            "Z undo. R reset room. No move limit.".into()
        }
    }
    fn device_input(&mut self, i: Intent) -> Intent {
        use input::NativeCommand;
        use macroquad::prelude::{is_key_pressed, KeyCode};
        self.hover = i.pointer;
        // This puzzle consumes physical presses, not the shared client's summed
        // held axes. Opposing keys can cancel that sum; releasing one would look
        // like a new press. Click commands also survive zero-tick display frames.
        let mut commands = vec![];
        if is_key_pressed(KeyCode::Z) || is_key_pressed(KeyCode::Backspace) {
            commands.push(NativeCommand::Target(click(UNDO).pointer.unwrap()));
            return self.native_input(i, commands);
        }
        if i.action {
            if let Some(point) = i.pointer {
                commands.extend(NativeCommand::pointer(&self.state.cells, point));
            }
        }
        for (j, key) in [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4]
            .iter()
            .enumerate()
        {
            if j < self.state.cells.len() && is_key_pressed(*key) {
                commands.push(NativeCommand::Select(j));
                break;
            }
        }
        if is_key_pressed(KeyCode::Tab) {
            commands.push(NativeCommand::Cycle);
        }
        if is_key_pressed(KeyCode::Enter) && self.solved() {
            commands.push(NativeCommand::Target(click(NEXT).pointer.unwrap()));
        }
        // Selection precedes movement when both arrive in one display frame.
        for (n, keys) in [
            [KeyCode::W, KeyCode::Up],
            [KeyCode::A, KeyCode::Left],
            [KeyCode::S, KeyCode::Down],
            [KeyCode::D, KeyCode::Right],
        ]
        .iter()
        .enumerate()
        {
            if keys.iter().any(|key| is_key_pressed(*key)) {
                commands.push(NativeCommand::Target(click(BUTTONS[n]).pointer.unwrap()));
                break;
            }
        }
        self.native_input(i, commands)
    }
    fn draw(&self, s: &mut Scene) {
        let room = self.room();
        let solved = self.solved();
        let member = passengers(&self.state.cells, self.state.selected);
        let hovered = |r: Rect| self.hover.is_some_and(|p| r.contains(p));
        let preview = BUTTONS
            .iter()
            .position(|r| hovered(*r))
            .filter(|_| !solved)
            .map(|j| plan(room, &self.state.cells, self.state.selected, DIRS[j]));
        s.rect(-100, Rect::new(0, 0, 800, 450), BG);
        for n in 0..10 {
            s.rect(-99, Rect::new(0, n * 45, 800, 1), dark(LINE, 0.33));
        }
        s.rect(-90, Rect::new(24, 18, 8, 8), MINT);
        s.rect(-90, Rect::new(32, 26, 8, 8), GOLD);
        label(s, "CORNER CLUTCH", 48, 39, 29., TEXT);
        label(s, "CHOOSE THE DRIVER. CHANGE THE CREW.", 48, 60, 12., MUTED);
        label(
            s,
            format!("ROOM {:02} / 08", self.state.room + 1),
            618,
            34,
            17.,
            GOLD,
        );
        for n in 0..8 {
            s.rect(
                1,
                Rect::new(620 + n * 18, 46, 12, 5),
                if n < self.state.room as i32 || (n == self.state.room as i32 && solved) {
                    MINT
                } else if n == self.state.room as i32 {
                    GOLD
                } else {
                    LINE
                },
            );
        }
        // Flush top-down grid; the same cell rectangles are used for selection.
        s.rect(-20, Rect::new(39, 79, 306, 306), LINE);
        s.rect(-19, Rect::new(42, 82, 300, 300), PANEL);
        for y in 0..6 {
            for x in 0..6 {
                let c = [x, y];
                let r = cell_rect(c, 1);
                s.rect(
                    -10,
                    r,
                    if (x + y) % 2 == 0 {
                        Color::new(0.095, 0.13, 0.17, 1.)
                    } else {
                        Color::new(0.08, 0.115, 0.15, 1.)
                    },
                );
                s.circle(-9, Point::new(r.x + 3, r.y + 3), 0.8, LINE);
            }
        }
        for &w in &room.walls {
            let r = cell_rect(w, 2);
            s.rect(-5, r, Color::new(0.20, 0.26, 0.31, 1.));
            s.rect(
                -4,
                Rect::new(r.x + 2, r.y + 2, r.w - 4, 3),
                Color::new(0.29, 0.36, 0.40, 1.),
            );
            s.rect(-4, Rect::new(r.x + 5, r.y + 20, r.w - 10, 3), LINE);
            for dx in [8, 34] {
                for dy in [8, 34] {
                    s.circle(-3, Point::new(r.x + dx, r.y + dy), 1.5, MUTED);
                }
            }
        }
        for (j, &c) in room.goals.iter().enumerate() {
            let r = cell_rect(c, 7);
            let col = COLORS[j];
            outline(s, -2, r, dark(col, 0.7), 2);
            s.text(
                -1,
                ((b'A' + j as u8) as char).to_string(),
                Point::new(r.x + 11, r.y + 23),
                20.,
                dark(col, 0.72),
            );
            for dx in [0, r.w - 3] {
                for dy in [0, r.h - 3] {
                    s.rect(-1, Rect::new(r.x + dx, r.y + dy, 3, 3), col);
                }
            }
        }
        // Diagonal stitch marks show only direct driver-to-passenger grips.
        let a = center(self.state.cells[self.state.selected]);
        for &j in &member {
            if j == self.state.selected {
                continue;
            }
            let b = center(self.state.cells[j]);
            for k in 0..8 {
                s.circle(
                    2,
                    Point::new(a.x + (b.x - a.x) * k / 7, a.y + (b.y - a.y) * k / 7),
                    1.6,
                    MINT,
                );
            }
        }
        if let Some(p) = &preview {
            for &(j, c) in &p.destinations {
                // Off-board landings use the edge rail below, keeping ghosts on the board.
                if !inside(c) {
                    continue;
                }
                let col = if p.blocked.iter().any(|b| b.cell == c) {
                    RED
                } else {
                    MINT
                };
                let r = cell_rect(c, 4);
                s.rect(4, r, faded(col, 0.13));
                outline(s, 5, r, faded(col, 0.8), 2);
                s.text(
                    6,
                    ((b'A' + j as u8) as char).to_string(),
                    Point::new(r.x + 4, r.y + 13),
                    13.,
                    col,
                );
            }
        }
        for (j, &c) in self.state.cells.iter().enumerate() {
            let mut r = cell_rect(c, 7);
            if self.state.slide_ticks > 0 {
                let from = self.state.slide_from[j];
                let t = self.state.slide_ticks as i32;
                r.x += (from[0] - c[0]) * TILE * t * t / 64;
                r.y += (from[1] - c[1]) * TILE * t * t / 64;
            }
            let col = COLORS[j];
            let driving = j == self.state.selected;
            let riding = member.contains(&j);
            if driving {
                outline(
                    s,
                    8,
                    Rect::new(r.x - 5, r.y - 5, r.w + 10, r.h + 10),
                    faded(GOLD, 0.4),
                    1,
                );
            }
            s.rect(
                10,
                Rect::new(r.x + 2, r.y + 4, r.w, r.h),
                Color::new(0.02, 0.03, 0.05, 0.8),
            );
            s.rect(11, r, dark(col, 0.55));
            s.rect(12, Rect::new(r.x + 2, r.y + 2, r.w - 4, r.h - 6), col);
            s.rect(
                13,
                Rect::new(r.x + 3, r.y + 3, r.w - 6, 3),
                Color::new(1., 1., 1., 0.16),
            );
            s.rect(13, Rect::new(r.x + 7, r.y + 8, 20, 20), dark(col, 0.40));
            label(
                s,
                ((b'A' + j as u8) as char).to_string(),
                r.x + 11,
                r.y + 24,
                20.,
                TEXT,
            );
            if c == room.goals[j] {
                s.circle(23, Point::new(r.x + r.w - 4, r.y + r.h - 4), 3., MINT);
            }
            if riding {
                grippers(
                    s,
                    Rect::new(r.x - 2, r.y - 2, r.w + 3, r.h + 3),
                    if driving { GOLD } else { MINT },
                );
            }
        }
        let blocks = preview
            .as_ref()
            .map(|p| p.blocked.as_slice())
            .unwrap_or(&self.state.blocked);
        for b in blocks {
            let c = [b.cell[0].clamp(0, 5), b.cell[1].clamp(0, 5)];
            let r = cell_rect(c, 2);
            if b.kind == Obstacle::Boundary {
                // Mark the actual board edge, not the occupied cell beside it.
                let rail = if b.cell[0] < 0 {
                    Rect::new(ORIGIN[0] - 5, r.y, 5, r.h)
                } else if b.cell[0] > 5 {
                    Rect::new(ORIGIN[0] + 6 * TILE, r.y, 5, r.h)
                } else if b.cell[1] < 0 {
                    Rect::new(r.x, ORIGIN[1] - 5, r.w, 5)
                } else {
                    Rect::new(r.x, ORIGIN[1] + 6 * TILE, r.w, 5)
                };
                s.rect(25, rail, RED);
                continue;
            }
            outline(s, 25, r, RED, 3);
            label(s, "X", r.x + 17, r.y + 28, 22., RED);
        }
        s.rect(0, Rect::new(378, 82, 398, 303), PANEL);
        s.rect(
            1,
            Rect::new(378, 82, 3, 303),
            if solved { MINT } else { GOLD },
        );
        label(
            s,
            format!("{:02}  {}", self.state.room + 1, room.title.to_uppercase()),
            396,
            108,
            20.,
            if solved { MINT } else { TEXT },
        );
        wrap(
            s,
            if solved {
                "All letters home. Continue when you're ready."
            } else {
                &room.intro
            },
            396,
            137,
            43,
            16.,
            if solved { MINT } else { MUTED },
        );
        s.rect(1, Rect::new(396, 176, 368, 1), LINE);
        let crew: String = member
            .iter()
            .filter(|&&j| j != self.state.selected)
            .map(|&j| format!("{} ", (b'A' + j as u8) as char))
            .collect();
        label(
            s,
            format!("DRIVER {}", (b'A' + self.state.selected as u8) as char),
            396,
            199,
            16.,
            GOLD,
        );
        label(
            s,
            format!(
                "CARGO {}",
                if crew.is_empty() { "NONE" } else { crew.trim() }
            ),
            548,
            199,
            16.,
            MINT,
        );
        label(s, "ONLY DIRECT", 396, 232, 13., MUTED);
        label(s, "CORNERS RIDE", 396, 250, 13., MUTED);
        label(
            s,
            format!("{} MOVES", self.state.history.len()),
            396,
            284,
            14.,
            TEXT,
        );
        let home = self
            .state
            .cells
            .iter()
            .zip(&room.goals)
            .filter(|(cell, socket)| cell == socket)
            .count();
        label(
            s,
            format!("{}/{} HOME", home, self.state.cells.len()),
            668,
            238,
            14.,
            if solved { MINT } else { TEXT },
        );
        label(s, "NO MOVE LIMIT", 668, 259, 11., MUTED);
        for (j, &r) in BUTTONS.iter().enumerate() {
            s.rect(10, Rect::new(r.x, r.y + 3, r.w, r.h), BG);
            s.rect(11, r, if hovered(r) { dark(MINT, 0.25) } else { LINE });
            outline(
                s,
                12,
                r,
                if hovered(r) {
                    if blocks.is_empty() {
                        MINT
                    } else {
                        RED
                    }
                } else {
                    dark(MUTED, 0.65)
                },
                1,
            );
            label(
                s,
                ["^", "<", "v", ">"][j],
                r.x + 14,
                r.y + 26,
                24.,
                if solved { MUTED } else { TEXT },
            );
        }
        let status = if let Some(b) = blocks.first() {
            match b.kind {
                Obstacle::Wall => "BLOCKED: WALL. NOTHING MOVES.",
                Obstacle::Boundary => "BLOCKED: EDGE. NOTHING MOVES.",
                Obstacle::Crate => "BLOCKED: OTHER CRATE. NOTHING MOVES.",
            }
        } else if solved {
            "ROOM COMPLETE. ENTER TO CONTINUE."
        } else if preview.is_some() {
            "CLEAR: THE WHOLE CREW CAN MOVE."
        } else if self.state.room == 0 && self.state.history.is_empty() {
            "PARK ALL LETTERS ON MATCHING SOCKETS."
        } else {
            "HOVER AN ARROW TO PREVIEW."
        };
        label(
            s,
            status,
            396,
            321,
            12.,
            if !blocks.is_empty() {
                RED
            } else if solved {
                MINT
            } else {
                MUTED
            },
        );
        button(
            s,
            UNDO,
            "Z  UNDO",
            !self.state.history.is_empty(),
            hovered(UNDO),
        );
        button(
            s,
            HINT,
            if self.state.hint { "HIDE HINT" } else { "HINT" },
            true,
            hovered(HINT),
        );
        button(
            s,
            NEXT,
            "NEXT ROOM >",
            solved && self.state.room < 7,
            hovered(NEXT),
        );
        // Bottom text is outside the client's transient-notice area (y395).
        let tip = if self.state.hint {
            room.hint.as_str()
        } else {
            room.tip.as_str()
        };
        label(
            s,
            tip,
            48,
            411,
            15.,
            if self.state.hint { GOLD } else { MINT },
        );
        label(s,"CLICK / 1-4 select   WASD / arrows step   Z undo   R reset   Esc pause   K save / L load",48,436,12.,MUTED);
    }
}
