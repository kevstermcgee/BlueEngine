#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
use macroquad::{
    input::utils::{register_input_subscriber, repeat_all_miniquad_input},
    prelude::*,
};
use std::collections::HashSet;
use vesper3d::{
    math::V,
    viewer::{
        controller::{Controller, Movement},
        interaction::{activation_requested, Interactions},
        mesh, room,
    },
};

fn config() -> macroquad::conf::Conf {
    macroquad::conf::Conf {
        miniquad_conf: Conf {
            window_title: "Blue Engine".into(),
            window_width: 1440,
            window_height: 900,
            high_dpi: true,
            sample_count: 4,
            window_resizable: true,
            ..Default::default()
        },
        draw_call_vertex_capacity: 30000,
        draw_call_index_capacity: 30000,
        ..Default::default()
    }
}
#[derive(Default)]
struct Keys {
    down: HashSet<KeyCode>,
    pressed: HashSet<KeyCode>,
    #[cfg(windows)]
    native_down: HashSet<KeyCode>,
}
impl Keys {
    fn pressed(&self, key: KeyCode) -> bool {
        self.pressed.contains(&key) || is_key_pressed(key)
    }
    fn poll(&mut self, focused: bool) {
        self.pressed.clear();
        #[cfg(windows)]
        {
            // Read only the viewer's bound keys. Polling also accepts accessibility-injected
            // keys without hardware scan codes and prevents a missed key-up sticking.
            for (key, vk) in [
                (KeyCode::W, 0x57),
                (KeyCode::A, 0x41),
                (KeyCode::S, 0x53),
                (KeyCode::D, 0x44),
                (KeyCode::Up, 0x26),
                (KeyCode::Down, 0x28),
                (KeyCode::Left, 0x25),
                (KeyCode::Right, 0x27),
                (KeyCode::LeftShift, 0xA0),
                (KeyCode::RightShift, 0xA1),
                (KeyCode::Space, 0x20),
                (KeyCode::LeftControl, 0xA2),
                (KeyCode::RightControl, 0xA3),
                (KeyCode::C, 0x43),
                (KeyCode::E, 0x45),
                (KeyCode::Backspace, 0x08),
                (KeyCode::Enter, 0x0D),
                (KeyCode::Escape, 0x1B),
                (KeyCode::Tab, 0x09),
                (KeyCode::F3, 0x72),
                (KeyCode::F11, 0x7A),
                (KeyCode::H, 0x48),
                (KeyCode::Q, 0x51),
            ] {
                // GetAsyncKeyState takes a virtual key integer and no pointers.
                let state = unsafe {
                    windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(vk)
                } as u16;
                let held = state & 0x8000 != 0;
                if focused && !self.native_down.contains(&key) && (held || state & 1 != 0) {
                    self.pressed.insert(key);
                }
                if held {
                    self.native_down.insert(key);
                } else {
                    self.native_down.remove(&key);
                }
                if focused && held {
                    self.down.insert(key);
                } else {
                    self.down.remove(&key);
                }
            }
        }
        if !focused {
            self.down.clear();
        }
    }
}
impl miniquad::EventHandler for Keys {
    fn update(&mut self) {}
    fn draw(&mut self) {}
    fn key_down_event(&mut self, key: KeyCode, _: miniquad::KeyMods, repeat: bool) {
        if !repeat {
            self.down.insert(key);
        }
    }
    fn key_up_event(&mut self, key: KeyCode, _: miniquad::KeyMods) {
        self.down.remove(&key);
    }
}
fn axes(keys: &HashSet<KeyCode>) -> (f32, f32) {
    let held = |a, b| {
        if keys.contains(&a) || keys.contains(&b) {
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
fn foreground() -> bool {
    #[cfg(windows)]
    {
        // Read-only check of this application's focus; both calls accept these arguments.
        unsafe {
            let mut pid = 0;
            windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
                windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow(),
                &mut pid,
            );
            pid == windows_sys::Win32::System::Threading::GetCurrentProcessId()
        }
    }
    #[cfg(not(windows))]
    {
        true
    }
}
fn capture(active: bool) {
    set_cursor_grab(active);
    show_mouse(!active);
}
thread_local! {static FONT: std::cell::RefCell<Option<Font>> = const {std::cell::RefCell::new(None)};}
fn text(s: &str, x: f32, y: f32, size: f32, c: Color) {
    FONT.with(|font| {
        draw_text_ex(
            s,
            x,
            y,
            TextParams {
                font: font.borrow().as_ref(),
                font_size: size as u16,
                color: c,
                ..Default::default()
            },
        );
    });
}
fn menu_scale() -> f32 {
    ((screen_height() - 24.) / 650.)
        .min((screen_width() - 24.) / 470.)
        .clamp(0.2, 1.)
}
fn wrapped_lines(message: &str, width: f32) -> Vec<String> {
    let mut lines = vec![];
    let mut line = String::new();
    for word in message.split_whitespace() {
        let candidate = if line.is_empty() {
            word.into()
        } else {
            format!("{line} {word}")
        };
        let size = FONT.with(|f| measure_text(&candidate, f.borrow().as_ref(), 18, 1.).width);
        if size > width && !line.is_empty() {
            lines.push(line);
            line = word.into();
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}
fn menu_mouse() -> Vec2 {
    Vec2::from(mouse_position()) / menu_scale()
}
const INK: Color = Color::new(0.91, 0.95, 1., 1.);
const MUTED: Color = Color::new(0.57, 0.66, 0.77, 1.);
const BLUE: Color = Color::new(0.23, 0.55, 1., 1.);
fn button(s: &str, r: Rect, primary: bool) -> bool {
    let hover = r.contains(menu_mouse());
    draw_rectangle(
        r.x,
        r.y,
        r.w,
        r.h,
        if primary {
            if hover {
                Color::new(0.32, 0.62, 1., 1.)
            } else {
                BLUE
            }
        } else if hover {
            Color::new(0.17, 0.23, 0.32, 1.)
        } else {
            Color::new(0.10, 0.15, 0.22, 1.)
        },
    );
    text(s, r.x + 18., r.y + r.h * 0.5 + 7., 22., INK);
    hover && is_mouse_button_pressed(MouseButton::Left)
}
#[allow(clippy::too_many_arguments)]
fn slider(label: &str, x: f32, y: f32, w: f32, value: &mut f32, min: f32, max: f32, suffix: &str) {
    text(label, x, y, 19., MUTED);
    text(
        &format!("{:.0}{}", *value, suffix),
        x + w - 55.,
        y,
        19.,
        INK,
    );
    let r = Rect::new(x, y + 13., w, 24.);
    if r.contains(menu_mouse()) && is_mouse_button_down(MouseButton::Left) {
        *value = (min + (menu_mouse().x - x) / w * (max - min)).clamp(min, max);
    }
    let end = x + (*value - min) / (max - min) * w;
    draw_rectangle(x, y + 24., w, 3., Color::new(0.19, 0.26, 0.35, 1.));
    draw_rectangle(x, y + 24., end - x, 3., BLUE);
    draw_circle(end, y + 25., 6., INK);
}

#[macroquad::main(config)]
async fn main() {
    #[cfg(windows)]
    if let Some(windows) = std::env::var_os("WINDIR") {
        let path = std::path::PathBuf::from(windows).join("Fonts/segoeui.ttf");
        if let Ok(font) = load_ttf_font(&path.to_string_lossy()).await {
            FONT.with(|f| *f.borrow_mut() = Some(font));
        }
    }
    clear_background(Color::new(0.035, 0.06, 0.10, 1.));
    text("BLUE ENGINE", 60., 90., 42., INK);
    text("Preparing the studio...", 60., 132., 22., MUTED);
    next_frame().await;
    let started = std::time::Instant::now();
    let room = match room::build() {
        Ok(r) => r,
        Err(e) => {
            error_screen(&format!("Could not load the room: {e}")).await;
            return;
        }
    };
    let meshes = mesh::bake_tagged(&room.world, &room.render_tags());
    let material = match mesh::material() {
        Ok(m) => m,
        Err(e) => {
            error_screen(&format!("Graphics initialization failed: {e}")).await;
            return;
        }
    };
    let setup_seconds = started.elapsed().as_secs_f32();
    let triangles: usize = meshes.iter().map(|m| m.indices.len() / 3).sum();
    let args: Vec<String> = std::env::args().collect();
    let motion_capture = args.iter().any(|a| a == "--capture-motion");
    let interaction_capture = args.iter().any(|a| a == "--capture-interactions");
    let capture_dir = args
        .windows(2)
        .find(|a| {
            a[0] == "--capture" || a[0] == "--capture-motion" || a[0] == "--capture-interactions"
        })
        .map(|a| std::path::PathBuf::from(&a[1]));
    if let Some(dir) = &capture_dir {
        if std::fs::create_dir_all(dir).is_err() {
            error_screen("Cannot create capture folder.").await;
            return;
        }
    }
    let mut controller = Controller::default();
    let mut interactions = Interactions::default();
    let mut keys = Keys::default();
    let subscriber = register_input_subscriber();
    let mut active = false;
    let mut entered = false;
    let mut skip_look = 0;
    let mut sensitivity = 50.;
    let mut fov: f32 = 65.;
    let mut invert = false;
    let mut hud = true;
    let mut debug = false;
    let mut fullscreen = false;
    let mut frame = 0;
    let mut samples = Vec::new();
    let mut captured_heights = Vec::new();
    loop {
        repeat_all_miniquad_input(&mut keys, subscriber);
        let focused = foreground();
        keys.poll(focused);
        if active && !focused {
            active = false;
            capture(false);
            controller.stop();
            keys.down.clear();
        }
        if focused && keys.pressed(KeyCode::F11) {
            fullscreen = !fullscreen;
            set_fullscreen(fullscreen);
            skip_look = 3;
        }
        if focused && keys.pressed(KeyCode::F3) {
            debug = !debug;
        }
        if focused && keys.pressed(KeyCode::H) && active {
            hud = !hud;
        }
        if focused && (keys.pressed(KeyCode::Escape) || keys.pressed(KeyCode::Tab)) {
            active = !active;
            entered |= active;
            capture(active);
            controller.stop();
            keys.down.clear();
            skip_look = 3;
        }
        if active && capture_dir.is_none() {
            if skip_look > 0 {
                skip_look -= 1;
            } else {
                let d = mouse_delta_position();
                controller.look(
                    -d.x * screen_width() * 0.5,
                    -d.y * screen_height() * 0.5,
                    0.0004 + sensitivity * 0.00004,
                    invert,
                );
            }
            // Preserve even a complete tap occurring between two rendered frames.
            let mut movement_keys = keys.down.clone();
            movement_keys.extend(keys.pressed.iter().copied());
            let (f, r) = axes(&movement_keys);
            controller.update(
                Movement {
                    forward: f,
                    right: r,
                    sprint: keys.down.contains(&KeyCode::LeftShift)
                        || keys.down.contains(&KeyCode::RightShift),
                    jump: keys.pressed(KeyCode::Space),
                    crouch: movement_keys.contains(&KeyCode::LeftControl)
                        || movement_keys.contains(&KeyCode::RightControl)
                        || movement_keys.contains(&KeyCode::C),
                },
                get_frame_time(),
                &room.colliders,
            );
            interactions.tick(get_frame_time());
            if keys.pressed(KeyCode::Backspace) || is_mouse_button_pressed(MouseButton::Right) {
                interactions.dismiss();
            }
            if activation_requested(
                active,
                skip_look == 0,
                keys.pressed(KeyCode::E),
                is_mouse_button_pressed(MouseButton::Left),
            ) {
                interactions.activate(&room, controller.ray());
            }
        }
        if capture_dir.is_some() && interaction_capture {
            match frame / 12 {
                0 | 1 => {
                    controller.position = V(-3.3, 1.34, -1.6);
                    controller.yaw = -std::f32::consts::FRAC_PI_2;
                    controller.pitch = 0.;
                }
                2 | 3 => {
                    controller.position = V(-1.65, 1.55, 0.);
                    controller.yaw = 0.;
                    controller.pitch = 0.;
                }
                _ => {
                    controller.position = V(2.12, 1.68, 1.6);
                    controller.yaw = 0.;
                    controller.pitch = -0.855;
                }
            }
            if [12, 36, 48].contains(&frame) {
                interactions.activate(&room, controller.ray());
            }
            if frame == 24 {
                interactions.dismiss();
            }
        } else if capture_dir.is_some() && motion_capture {
            controller.update(
                Movement {
                    jump: frame == 1,
                    crouch: (25..55).contains(&frame),
                    ..Default::default()
                },
                1. / 60.,
                &room.colliders,
            );
        } else if capture_dir.is_some() {
            match frame / 12 {
                0 => {}
                1 => {
                    controller.position = V(-2., 1.68, 1.5);
                    controller.yaw = 0.9;
                    controller.pitch = -0.08;
                }
                2 => {
                    controller.position = V(0., 1.68, -3.6);
                    controller.yaw = -1.3;
                    controller.pitch = -0.04;
                }
                _ => {}
            }
        }
        clear_background(Color::new(0.12, 0.17, 0.24, 1.));
        set_camera(&Camera3D {
            position: mesh::vec(controller.position),
            target: mesh::vec(controller.position + controller.direction()),
            up: vec3(0., 1., 0.),
            fovy: fov.to_radians(),
            z_near: 0.045,
            z_far: 60.,
            ..Default::default()
        });
        material.set_uniform("Eye", mesh::vec(controller.position));
        material.set_uniform(
            "ObjectStates",
            vec2(
                if interactions.monitor_on { 1. } else { 0. },
                if interactions.crystal_amber { 1. } else { 0. },
            ),
        );
        gl_use_material(&material);
        for m in &meshes {
            draw_mesh(m);
        }
        gl_use_default_material();
        set_default_camera();
        let sw = screen_width();
        let sh = screen_height();
        if (active || capture_dir.is_some()) && hud {
            draw_rectangle(24., 22., 218., 52., Color::new(0.025, 0.045, 0.08, 0.86));
            draw_rectangle(38., 37., 7., 22., BLUE);
            text("BLUE ENGINE", 57., 56., 22., INK);
            draw_circle(sw * 0.5, sh * 0.5, 2., Color::new(0.92, 0.96, 1., 0.85));
            draw_rectangle(
                24.,
                sh - 103.,
                sw.min(780.) - 48.,
                79.,
                Color::new(0.025, 0.045, 0.08, 0.8),
            );
            text(
                "WASD / Arrows   Move     Mouse   Look     Shift   Sprint",
                38.,
                sh - 80.,
                18.,
                INK,
            );
            text(
                "Space   Small jump     Hold Ctrl / C   Crouch     Esc   Pause",
                38.,
                sh - 57.,
                18.,
                INK,
            );
            text(
                "E / Left-click   Interact     Right-click / Backspace   Dismiss info",
                38.,
                sh - 34.,
                18.,
                INK,
            );
        }
        if active || capture_dir.is_some() {
            if let Some(info) = &interactions.feedback {
                let width = 440_f32.min(sw - 48.);
                let x = sw - width - 24.;
                let y = 94.;
                let lines = wrapped_lines(info.description, width - 36.);
                let height = 84. + lines.len() as f32 * 24.;
                draw_rectangle(x, y, width, height, Color::new(0.025, 0.045, 0.08, 0.94));
                draw_rectangle(x, y, 3., height, BLUE);
                text(info.title, x + 18., y + 31., 22., INK);
                for (i, line) in lines.iter().enumerate() {
                    text(line, x + 18., y + 60. + i as f32 * 24., 18., INK);
                }
                text(
                    "Right-click / Backspace to dismiss",
                    x + 18.,
                    y + height - 15.,
                    16.,
                    MUTED,
                );
            }
            if let Some(entity) = room.focus(controller.ray()) {
                draw_circle_lines(sw * 0.5, sh * 0.5, 7., 1.5, BLUE);
                let label = format!(
                    "{}  |  E / Click: {}",
                    entity.label,
                    interactions.prompt(entity.action)
                );
                let width = FONT.with(|f| measure_text(&label, f.borrow().as_ref(), 18, 1.).width);
                draw_rectangle(
                    sw * 0.5 - width * 0.5 - 16.,
                    sh * 0.5 + 24.,
                    width + 32.,
                    36.,
                    Color::new(0.025, 0.045, 0.08, 0.84),
                );
                text(&label, sw * 0.5 - width * 0.5, sh * 0.5 + 48., 18., INK);
            }
        }
        if !active && (capture_dir.is_none() || (interaction_capture && frame >= 60)) {
            let scale = menu_scale();
            let sw = sw / scale;
            let sh = sh / scale;
            let mut ui_camera = Camera2D::from_display_rect(Rect::new(0., 0., sw, sh));
            ui_camera.zoom.y = -ui_camera.zoom.y;
            set_camera(&ui_camera);
            draw_rectangle(0., 0., sw, sh, Color::new(0.01, 0.025, 0.055, 0.53));
            // Scale the panel on small windows without letting controls leave the viewport.
            let panel_w = 470_f32.min(sw - 32.);
            let x = (sw - panel_w) * 0.5;
            let y = ((sh - 660.) * 0.5).max(12.);
            draw_rectangle(x, y, panel_w, 650., Color::new(0.025, 0.046, 0.077, 0.97));
            draw_rectangle(x, y, 4., 650., BLUE);
            let left = x + 32.;
            let width = panel_w - 64.;
            text("B L U E   E N G I N E", left, y + 47., 20., BLUE);
            text(
                if entered {
                    "Take your time."
                } else {
                    "A room to explore."
                },
                left,
                y + 101.,
                37.,
                INK,
            );
            text("First-person studio  /  01", left, y + 135., 20., MUTED);
            if button(
                if entered {
                    "Resume exploring    /    Enter"
                } else {
                    "Enter the room    /    Enter"
                },
                Rect::new(left, y + 164., width, 52.),
                true,
            ) || (focused && keys.pressed(KeyCode::Enter))
            {
                active = true;
                entered = true;
                keys.down.clear();
                controller.stop();
                capture(true);
                skip_look = 3;
            }
            text("WASD or arrow keys", left, y + 242., 21., INK);
            text("Walk in any direction", left + 210., y + 242., 18., MUTED);
            text("Mouse", left, y + 266., 21., INK);
            text("Look around", left + 210., y + 266., 18., MUTED);
            text("Space / Hold Ctrl or C", left, y + 290., 19., INK);
            text("Small jump / Crouch", left + 210., y + 290., 18., MUTED);
            text("Shift / Esc", left, y + 314., 21., INK);
            text("Sprint / Pause", left + 210., y + 314., 18., MUTED);
            text("E / Left-click", left, y + 338., 21., INK);
            text("Use the aimed object", left + 210., y + 338., 18., MUTED);
            draw_line(
                left,
                y + 354.,
                left + width,
                y + 354.,
                1.,
                Color::new(0.16, 0.22, 0.30, 1.),
            );
            slider(
                "Mouse sensitivity",
                left,
                y + 375.,
                width,
                &mut sensitivity,
                1.,
                100.,
                "%",
            );
            slider(
                "Field of view",
                left,
                y + 441.,
                width,
                &mut fov,
                50.,
                90.,
                " deg",
            );
            if button(
                if invert {
                    "Invert look: On"
                } else {
                    "Invert look: Off"
                },
                Rect::new(left, y + 493., width * 0.5 - 6., 42.),
                false,
            ) {
                invert = !invert;
            }
            if button(
                "Reset position",
                Rect::new(left + width * 0.5 + 6., y + 493., width * 0.5 - 6., 42.),
                false,
            ) {
                controller = Controller::default();
                keys.down.clear();
            }
            text(
                "F11  Fullscreen     H  Hide hints     F3  Stats",
                left,
                y + 574.,
                17.,
                MUTED,
            );
            if button("Quit", Rect::new(left, y + 594., 80., 34.), false)
                || (focused && keys.pressed(KeyCode::Q))
            {
                break;
            }
            text(
                "Built on Vesper3D",
                left + width - 151.,
                y + 617.,
                17.,
                MUTED,
            );
        }
        set_default_camera();
        if debug {
            draw_rectangle(sw - 350., 20., 330., 90., Color::new(0., 0., 0., 0.8));
            text(
                &format!("{} FPS  |  {} triangles", get_fps(), triangles),
                sw - 336.,
                45.,
                18.,
                INK,
            );
            text(
                &format!(
                    "x {:.2}  y {:.2}  z {:.2}",
                    controller.position.0, controller.position.1, controller.position.2
                ),
                sw - 336.,
                70.,
                18.,
                INK,
            );
            text(
                &format!(
                    "Startup {:.2}s   /   {} batches",
                    setup_seconds,
                    meshes.len()
                ),
                sw - 336.,
                95.,
                18.,
                INK,
            );
        }
        if let Some(dir) = &capture_dir {
            let shot = if motion_capture {
                [15, 50, 80].iter().position(|f| *f == frame)
            } else if frame % 12 == 10 {
                Some(frame / 12)
            } else {
                None
            };
            if let Some(index) = shot {
                captured_heights.push(controller.position.1);
                get_screen_data().export_png(
                    &dir.join(format!("blue-engine-{}.png", index + 1))
                        .to_string_lossy(),
                );
            }
            if frame > 3 {
                samples.push(get_frame_time());
            }
            if frame
                == if motion_capture {
                    85
                } else if interaction_capture {
                    71
                } else {
                    35
                }
            {
                let mean = samples.iter().sum::<f32>() / samples.len() as f32;
                let report=format!("startup_seconds={setup_seconds:.3}\ntriangles={triangles}\nbatches={}\nmean_frame_ms={:.3}\ncaptured_eye_heights={captured_heights:?}\n",meshes.len(),mean*1000.);
                let _ = std::fs::write(dir.join("render-report.txt"), report);
                break;
            }
        }
        frame += 1;
        next_frame().await;
    }
    capture(false);
}
async fn error_screen(message: &str) {
    loop {
        clear_background(Color::new(0.035, 0.06, 0.10, 1.));
        text("Blue Engine", 40., 65., 36., INK);
        text(message, 40., 112., 20., INK);
        text("Press Escape to close.", 40., 155., 20., MUTED);
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        next_frame().await;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arrows_match_wasd_and_aliases_do_not_double_speed() {
        for (a, b) in [
            (KeyCode::W, KeyCode::Up),
            (KeyCode::S, KeyCode::Down),
            (KeyCode::A, KeyCode::Left),
            (KeyCode::D, KeyCode::Right),
        ] {
            assert_eq!(axes(&HashSet::from([a])), axes(&HashSet::from([b])));
            assert_eq!(axes(&HashSet::from([a, b])), axes(&HashSet::from([a])));
        }
        assert_eq!(
            axes(&HashSet::from([
                KeyCode::Up,
                KeyCode::S,
                KeyCode::Left,
                KeyCode::D
            ])),
            (0., 0.)
        );
    }
}
