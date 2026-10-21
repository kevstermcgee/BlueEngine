use super::*;
use draw::{Color, Scene};
const BG: Color = Color::new(0.026, 0.037, 0.065, 1.);
const EDGE: Color = Color::new(0.13, 0.19, 0.26, 1.);
const COLORS: [Color; 7] = [
    Color::new(0.18, 0.83, 0.95, 1.),
    Color::new(0.98, 0.79, 0.25, 1.),
    Color::new(0.67, 0.44, 0.97, 1.),
    Color::new(0.36, 0.88, 0.62, 1.),
    Color::new(0.98, 0.35, 0.45, 1.),
    Color::new(0.30, 0.52, 0.97, 1.),
    Color::new(1., 0.61, 0.27, 1.),
];
fn block(s: &mut Scene, x: i32, y: i32, n: i32, kind: u8, ghost: bool) {
    let c = COLORS[kind as usize];
    if ghost {
        s.rect(
            3,
            Rect::new(x + 1, y + 1, n - 2, n - 2),
            Color::new(c.r * 0.42, c.g * 0.42, c.b * 0.42, 1.),
        );
        s.rect(4, Rect::new(x + 2, y + 2, n - 4, n - 4), BG);
    } else {
        s.rect(
            3,
            Rect::new(x + 1, y + 1, n - 2, n - 2),
            Color::new(c.r * 0.5, c.g * 0.5, c.b * 0.5, 1.),
        );
        s.rect(4, Rect::new(x + 2, y + 1, n - 4, n - 4), c);
        s.rect(
            5,
            Rect::new(x + 3, y + 2, n - 6, 2.max(n / 12)),
            Color::new(c.r * 0.6 + 0.4, c.g * 0.6 + 0.4, c.b * 0.6 + 0.4, 1.),
        );
    }
}
impl draw::Game for Tetris {
    fn default_fullscreen() -> bool {
        false
    }
    fn show_hud() -> bool {
        false
    }
    fn device_input(&mut self, mut i: Intent) -> Intent {
        use macroquad::prelude::{is_key_pressed, KeyCode};
        let command = if is_key_pressed(KeyCode::Key1) {
            Some(10)
        } else if is_key_pressed(KeyCode::Key2) {
            Some(11)
        } else if is_key_pressed(KeyCode::Key3) {
            Some(12)
        } else if is_key_pressed(KeyCode::C)
            || is_key_pressed(KeyCode::LeftShift)
            || is_key_pressed(KeyCode::RightShift)
        {
            Some(4)
        } else if is_key_pressed(KeyCode::Z) || is_key_pressed(KeyCode::Q) {
            Some(3)
        } else if is_key_pressed(KeyCode::X) {
            Some(2)
        } else {
            None
        };
        // Pointer targets retain distinct command edges across device frames without a tick.
        i.pointer = command.map(|c| Point::new(-100, c));
        if command.is_some() {
            i.action = true;
        }
        i
    }
    fn gamepad_input(
        &mut self,
        input: Intent,
        pad: &vesper3d::viewer::gamepad::GamepadFrame,
    ) -> Intent {
        use vesper3d::viewer::gamepad::Button;
        controls::map(
            input,
            controls::Pad {
                up: pad.down(Button::DPadUp),
                clockwise: pad.pressed(Button::South),
                reverse: pad.pressed(Button::East),
                hold: pad.pressed(Button::West)
                    || pad.pressed(Button::LeftTrigger)
                    || pad.pressed(Button::RightTrigger),
                drop: pad.pressed(Button::North) || pad.pressed(Button::DPadUp),
                mode: pad.pressed(Button::RightThumb),
            },
            self.state.mode,
        )
    }
    fn menu_status(&self) -> String {
        format!(
            "{} points - {} lines - {}",
            self.state.score,
            self.state.lines,
            ["Marathon", "Sprint", "Ultra"][self.state.mode as usize]
        )
    }
    fn draw(&self, s: &mut Scene) {
        use macroquad::prelude::{screen_height, screen_width};
        // Scene primitives may extend into letterbox margins. Inverse-map the full
        // native window into the shared logical canvas so portrait windows remain
        // board-first without another renderer, timing loop or input authority.
        let (width, height) = (screen_width(), screen_height());
        let scale = (width / 800.).min(height / 450.);
        let full_w = (width / scale).ceil() as i32;
        let full_h = (height / scale).ceil() as i32;
        s.rect(
            -10,
            Rect::new(
                (800 - full_w) / 2 - 2,
                (450 - full_h) / 2 - 2,
                full_w + 4,
                full_h + 4,
            ),
            BG,
        );
        let cell = (((height - 66.) / HEIGHT as f32).min((width - 6.) / WIDTH as f32) / scale)
            .floor()
            .max(8.) as i32;
        let (left, top) = (
            (800 - cell * WIDTH as i32) / 2,
            (450 - cell * HEIGHT as i32) / 2 + (12. / scale) as i32,
        );
        s.rect(
            0,
            Rect::new(
                left - 1,
                top - 1,
                cell * WIDTH as i32 + 2,
                cell * HEIGHT as i32 + 2,
            ),
            EDGE,
        );
        s.rect(
            1,
            Rect::new(left, top, cell * WIDTH as i32, cell * HEIGHT as i32),
            BG,
        );
        for y in 0..HEIGHT as i32 {
            for x in 0..WIDTH as i32 {
                s.rect(
                    2,
                    Rect::new(left + x * cell, top + y * cell, cell - 1, cell - 1),
                    Color::new(0.050, 0.067, 0.103, 1.),
                );
            }
        }
        let st = &self.state;
        if !st.ended && st.clear_timer == 0 {
            let ghost = self.ghost();
            for (x, y) in ghost.cells() {
                if y >= 2 {
                    block(
                        s,
                        left + x * cell,
                        top + (y - 2) * cell,
                        cell,
                        ghost.kind,
                        true,
                    );
                }
            }
        }
        for y in 2..ROWS {
            for x in 0..WIDTH as i32 {
                let k = st.board[y * WIDTH + x as usize];
                if k > 0 {
                    block(
                        s,
                        left + x * cell,
                        top + (y as i32 - 2) * cell,
                        cell,
                        k - 1,
                        false,
                    );
                }
            }
        }
        if st.clear_timer > 0 {
            for &y in &st.clear_rows {
                if y >= 2 {
                    s.rect(
                        7,
                        Rect::new(left, top + (y as i32 - 2) * cell, cell * WIDTH as i32, cell),
                        if st.clear_timer % 6 < 3 {
                            Color::new(0.90, 0.94, 0.98, 1.)
                        } else {
                            Color::new(0.39, 0.95, 0.77, 1.)
                        },
                    );
                }
            }
        } else if !st.ended {
            for (x, y) in st.active.cells() {
                if y >= 2 {
                    block(
                        s,
                        left + x * cell,
                        top + (y - 2) * cell,
                        cell,
                        st.active.kind,
                        false,
                    );
                }
            }
        }
        // Just two unobtrusive piece previews above the board; no sidebars or boxes.
        let mini = (cell / 4).max(8);
        for (n, &kind) in st.queue.iter().take(2).enumerate() {
            let x = 400 - 5 * mini + n as i32 * 6 * mini;
            for (cx, cy) in SHAPES[kind as usize] {
                block(
                    s,
                    x + cx * mini,
                    top - 2 * mini - 3 + cy * mini,
                    mini,
                    kind,
                    false,
                );
            }
        }
    }
}
