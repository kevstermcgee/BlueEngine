//! Shared, opt-in presentation defaults for standalone games.
//! F/F11 changes fullscreen in place. Escape opens the menu and gates all gameplay
//! input. Online authority continues while the menu is open. No OS unsafe calls.
use super::game_text::draw_text;
use super::{controller::Collider, mesh};
use crate::{geometry::World, math::V};
use macroquad::{
    input::utils::{register_input_subscriber, repeat_all_miniquad_input},
    prelude::*,
};

pub fn window_config(title: &str) -> macroquad::conf::Conf {
    macroquad::conf::Conf {
        miniquad_conf: Conf {
            window_title: title.into(),
            window_width: 1280,
            window_height: 720,
            high_dpi: true,
            sample_count: 4,
            ..Default::default()
        },
        draw_call_vertex_capacity: 30000,
        draw_call_index_capacity: 30000,
        ..Default::default()
    }
}

/// Cache transformed, shaded primitives once, including cylinders and cones.
pub fn static_meshes(world: &World) -> Vec<Mesh> {
    mesh::bake_tagged(
        world,
        &[(
            Collider {
                min: V(-100000., -100000., -100000.),
                max: V(100000., 100000., 100000.),
            },
            3.,
        )],
    )
}

struct Focus {
    active: bool,
}
impl miniquad::EventHandler for Focus {
    fn update(&mut self) {}
    fn draw(&mut self) {}
    fn window_minimized_event(&mut self) {
        self.active = false;
    }
    fn window_restored_event(&mut self) {
        self.active = true;
    }
}

pub struct GameShell {
    pub paused: bool,
    pub fullscreen: bool,
    pub diagnostics: bool,
    subscriber: usize,
    focus: Focus,
    captured: bool,
    controls: bool,
    selection: usize,
    suppress: bool,
    key_pressed: fn(KeyCode) -> bool,
}
impl Default for GameShell {
    fn default() -> Self {
        Self::new()
    }
}
impl GameShell {
    pub fn new() -> Self {
        Self {
            paused: false,
            fullscreen: false,
            diagnostics: false,
            subscriber: register_input_subscriber(),
            focus: Focus { active: true },
            captured: false,
            controls: false,
            selection: 0,
            suppress: false,
            key_pressed: is_key_pressed,
        }
    }
    pub fn begin_frame(&mut self, connected: bool) {
        self.begin_frame_with_input(connected, is_key_pressed);
    }
    /// An executable can supply focus-aware native key edges without unsafe library code.
    pub fn begin_frame_with_input(&mut self, connected: bool, pressed: fn(KeyCode) -> bool) {
        self.key_pressed = pressed;
        repeat_all_miniquad_input(&mut self.focus, self.subscriber);
        self.suppress = false;
        if !self.focus.active {
            self.paused = true;
        }
        if self.focus.active && ((self.key_pressed)(KeyCode::F) || (self.key_pressed)(KeyCode::F11))
        {
            self.fullscreen = !self.fullscreen;
            set_fullscreen(self.fullscreen);
            self.suppress = true;
        }
        if self.focus.active && (self.key_pressed)(KeyCode::Escape) {
            self.paused = !self.paused;
            self.controls = false;
            self.suppress = true;
        }
        if (self.key_pressed)(KeyCode::F3) {
            self.diagnostics = !self.diagnostics;
        }
        if (self.key_pressed)(KeyCode::LeftAlt) {
            self.paused = true;
        }
        let capture = connected && !self.paused && self.focus.active;
        if capture != self.captured {
            set_cursor_grab(capture);
            show_mouse(!capture);
            self.captured = capture;
            self.suppress = true;
        }
    }
    pub fn playing(&self) -> bool {
        self.captured && !self.paused && !self.suppress
    }
    /// Draw a small, keyboard/mouse accessible overlay. Returns true on Quit.
    pub fn menu(&mut self, title: &str, controls: &[&str]) -> bool {
        self.menu_with_status(title, controls, "MENU  /  ONLINE MATCH CONTINUES")
    }
    /// Shared pause menu for an application that pauses its local simulation.
    pub fn local_menu(&mut self, title: &str, controls: &[&str]) -> bool {
        self.menu_with_status(title, controls, "MENU  /  LOCAL SESSION PAUSED")
    }
    fn menu_with_status(&mut self, title: &str, controls: &[&str], status: &str) -> bool {
        if !self.paused {
            return false;
        }
        set_default_camera();
        let w = screen_width();
        let h = screen_height();
        draw_rectangle(0., 0., w, h, Color::from_rgba(8, 18, 26, 130));
        let scale = (w / 700.).min(h / 520.).min(1.0);
        let pw = 420. * scale;
        let ph = if self.controls { 360. } else { 320. } * scale;
        let x = (w - pw) * 0.5;
        let y = (h - ph) * 0.5;
        let ink = Color::from_rgba(24, 43, 53, 255);
        draw_rectangle(x, y, pw, ph, Color::from_rgba(243, 242, 232, 250));
        draw_text(title, x + 28. * scale, y + 43. * scale, 29. * scale, ink);
        draw_text(status, x + 28. * scale, y + 68. * scale, 14. * scale, ink);
        if self.controls {
            for (i, line) in controls.iter().enumerate() {
                draw_text(
                    line,
                    x + 28. * scale,
                    y + (108. + i as f32 * 26.) * scale,
                    18. * scale,
                    ink,
                );
            }
            if self.button(
                "Back",
                x + 24. * scale,
                y + ph - 64. * scale,
                pw - 48. * scale,
                42. * scale,
                true,
            ) || (self.key_pressed)(KeyCode::Enter)
            {
                self.controls = false;
                self.suppress = true;
            }
        } else {
            if (self.key_pressed)(KeyCode::Down) {
                self.selection = (self.selection + 1) % 3;
            }
            if (self.key_pressed)(KeyCode::Up) {
                self.selection = (self.selection + 2) % 3;
            }
            for (i, label) in ["Resume", "Controls", "Quit game"].iter().enumerate() {
                let clicked = self.button(
                    label,
                    x + 24. * scale,
                    y + (94. + i as f32 * 57.) * scale,
                    pw - 48. * scale,
                    46. * scale,
                    self.selection == i,
                );
                if clicked || (self.selection == i && (self.key_pressed)(KeyCode::Enter)) {
                    self.suppress = true;
                    match i {
                        0 => self.paused = false,
                        1 => self.controls = true,
                        _ => return true,
                    }
                }
            }
        }
        false
    }
    fn button(&self, text: &str, x: f32, y: f32, w: f32, h: f32, selected: bool) -> bool {
        let (mx, my) = mouse_position();
        let hover = Rect::new(x, y, w, h).contains(vec2(mx, my));
        draw_rectangle(
            x,
            y,
            w,
            h,
            if hover || selected {
                Color::from_rgba(29, 94, 108, 255)
            } else {
                Color::from_rgba(221, 226, 220, 255)
            },
        );
        draw_text(
            text,
            x + 16.,
            y + h * 0.65,
            h * 0.48,
            if hover || selected {
                WHITE
            } else {
                Color::from_rgba(24, 43, 53, 255)
            },
        );
        hover && is_mouse_button_pressed(MouseButton::Left)
    }
}

/// Both key layouts normalize diagonals in the shared movement controller.
pub fn movement_axes() -> (f32, f32) {
    let held = |a, b| {
        if is_key_down(a) || is_key_down(b) {
            1.
        } else {
            0.
        }
    };
    (
        held(KeyCode::W, KeyCode::Up) - held(KeyCode::S, KeyCode::Down),
        held(KeyCode::D, KeyCode::Right) - held(KeyCode::A, KeyCode::Left),
    )
}
