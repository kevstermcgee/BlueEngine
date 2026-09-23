#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod character;
mod impact_audio;
mod platform_window;
mod wrench_view;
use macroquad::{
    input::utils::{register_input_subscriber, repeat_all_miniquad_input},
    prelude::*,
};
use std::collections::HashSet;
use vesper3d::{
    math::V,
    viewer::{
        camera::Perspective,
        controller::{Controller, Movement},
        interaction::{activation_requested, Interactions},
        maps::{self, MapId},
        mesh,
        simulation::PlayerStepper,
        wrench::Wrench,
    },
};

fn config() -> macroquad::conf::Conf {
    macroquad::conf::Conf {
        miniquad_conf: Conf {
            window_title: "Blue Engine 2 | BE2".into(),
            window_width: 960,
            window_height: 600,
            fullscreen: false,
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
                (KeyCode::F, 0x46),
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
            self.pressed.insert(key);
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
    platform_window::maximize();
    // Process the queued maximize/resize before preparing or displaying the room.
    next_frame().await;
    #[cfg(windows)]
    if let Some(windows) = std::env::var_os("WINDIR") {
        let path = std::path::PathBuf::from(windows).join("Fonts/segoeui.ttf");
        if let Ok(font) = load_ttf_font(&path.to_string_lossy()).await {
            FONT.with(|f| *f.borrow_mut() = Some(font));
        }
    }
    clear_background(Color::new(0.035, 0.06, 0.10, 1.));
    text("BLUE ENGINE 2", 60., 90., 42., INK);
    text("Preparing the map...", 60., 132., 22., MUTED);
    next_frame().await;
    let started = std::time::Instant::now();
    let args: Vec<String> = std::env::args().collect();
    let house_capture = args.iter().any(|a| a == "--capture-house");
    let map = if args.iter().any(|a| a == "--studio") {
        MapId::Studio
    } else {
        MapId::House
    };
    let map_file = args.windows(2).find(|a| a[0] == "--map").map(|a| &a[1]);
    let room = match map_file.map_or_else(
        || maps::build(map),
        |p| {
            vesper3d::viewer::authoring::MapDocument::load(std::path::Path::new(p))
                .and_then(|d| d.build())
        },
    ) {
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
    let props_capture = args.iter().any(|a| a == "--capture-props");
    let motion_capture = args.iter().any(|a| a == "--capture-motion");
    let character_capture = args.iter().any(|a| a == "--capture-character");
    let wrench_capture = args.iter().any(|a| a == "--capture-wrench");
    let interaction_capture = args.iter().any(|a| a == "--capture-interactions");
    let capture_dir = args
        .windows(2)
        .find(|a| {
            a[0] == "--capture-house"
                || a[0] == "--capture-props"
                || a[0] == "--capture-character"
                || a[0] == "--capture-wrench"
                || a[0] == "--capture"
                || a[0] == "--capture-motion"
                || a[0] == "--capture-interactions"
        })
        .map(|a| std::path::PathBuf::from(&a[1]));
    if let Some(dir) = &capture_dir {
        if std::fs::create_dir_all(dir).is_err() {
            error_screen("Cannot create capture folder.").await;
            return;
        }
    }
    let mut controller = Controller::default();
    let mut stepper = PlayerStepper::default();
    let mut interactions = Interactions::default();
    let mut wrench = Wrench::default();
    let mut impact_audio = impact_audio::ImpactAudio::new().await;
    let mut wrench_view = wrench_view::View::new();
    let mut character = character::Character::default();
    let mut perspective = if args.iter().any(|a| a == "--third-person") {
        Perspective::Third
    } else {
        Perspective::default()
    };
    let mut keys = Keys::default();
    let subscriber = register_input_subscriber();
    let mut active = false;
    let mut entered = false;
    let mut skip_look = 0;
    let mut sensitivity = 50.;
    let mut fov: f32 = 65.;
    let mut invert = false;
    let mut hud = false;
    let mut debug = false;
    let mut fullscreen = false;
    let mut maximize_pending = false;
    let mut frame = 0;
    let mut samples = Vec::new();
    let mut captured_heights = Vec::new();
    loop {
        let previous_position = controller.position;
        let focused = foreground();
        keys.poll(focused);
        repeat_all_miniquad_input(&mut keys, subscriber);
        if !focused {
            keys.down.clear();
            keys.pressed.clear();
        }
        if active && !focused {
            active = false;
            capture(false);
            controller.stop();
            stepper.reset(&controller);
            wrench.cancel();
            keys.down.clear();
        }
        // Fullscreen requests are applied by the backend between frames.
        if maximize_pending {
            platform_window::maximize();
            maximize_pending = false;
        }
        if focused && (keys.pressed(KeyCode::F) || keys.pressed(KeyCode::F11)) {
            fullscreen = !fullscreen;
            set_fullscreen(fullscreen);
            maximize_pending = !fullscreen;
            skip_look = 3;
        }
        if focused && keys.pressed(KeyCode::Q) {
            perspective.toggle();
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
            stepper.reset(&controller);
            wrench.cancel();
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
            stepper.advance(
                &mut controller,
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
            if is_mouse_button_pressed(MouseButton::Left) {
                wrench.start(active, skip_look == 0);
            }
            wrench.tick(
                get_frame_time(),
                &room,
                perspective.view(&controller, &room).aim(&controller, &room),
            );
            if keys.pressed(KeyCode::Backspace) || is_mouse_button_pressed(MouseButton::Right) {
                interactions.dismiss();
            }
            if activation_requested(active, skip_look == 0, keys.pressed(KeyCode::E), false) {
                interactions.activate(
                    &room,
                    perspective.view(&controller, &room).aim(&controller, &room),
                );
            }
        }
        if capture_dir.is_some() && house_capture {
            let (eye, yaw, pitch) = match frame / 12 {
                0 => (V(14., 10., 17.), -0.69, -0.29),
                1 => (V(-1.35, 1.68, 3.65), -0.85, -0.12),
                2 => (V(-0.9, 1.68, -1.1), -1.10, -0.16),
                3 => (V(4.5, 1.68, 4.1), 0., 0.25),
                4 => (V(-1.3, 4.88, 0.5), -0.45, -0.18),
                5 => (V(-1., 2.1, -13.7), 2.85, -0.05),
                6 => (V(1.25, 4.88, -3.45), -0.20, -0.45),
                7 => (V(1.25, 4.88, 1.05), -0.55, -0.20),
                8 => (V(-2.0, 1.68, 0.95), -2.69, -0.12),
                9 => (V(-8.4, 1.68, 5.5), 0.10, -0.10),
                10 => (V(4.4, 0.98, -11.4), std::f32::consts::PI, 0.0),
                _ => (V(-1., 2.1, -13.7), 2.85, -0.05),
            };
            controller.position = eye;
            controller.yaw = yaw;
            controller.pitch = pitch;
        } else if capture_dir.is_some() && character_capture {
            if frame == 0 {
                controller.position.2 = 2.7;
                controller.yaw = 0.;
                controller.pitch = -0.10;
            }
            perspective = if frame < 12 || (60..72).contains(&frame) {
                Perspective::First
            } else {
                Perspective::Third
            };
            if frame == 24 {
                wrench.start(true, true);
            }
            if frame >= 36 {
                controller.update(
                    Movement {
                        crouch: frame < 48,
                        jump: frame == 54,
                        ..Default::default()
                    },
                    1. / 60.,
                    &room.colliders,
                );
            }
            wrench.tick(
                1. / 60.,
                &room,
                perspective.view(&controller, &room).aim(&controller, &room),
            );
        } else if capture_dir.is_some() && wrench_capture {
            controller.position = V(-3.3, 1.34, -1.6);
            controller.yaw = -std::f32::consts::FRAC_PI_2;
            controller.pitch = 0.;
            if frame == 12 {
                wrench.start(true, true);
            }
            wrench.tick(1. / 60., &room, controller.ray());
        } else if capture_dir.is_some() && interaction_capture {
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
                interactions.activate(
                    &room,
                    perspective.view(&controller, &room).aim(&controller, &room),
                );
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
        } else if capture_dir.is_some() && props_capture {
            controller.position = match frame / 12 {
                0 => V(2.7, 1.68, 1.4),
                1 => V(2.32, 1.30, 2.9),
                _ => V(1.25, 1.4, 2.3),
            };
            controller.yaw = std::f32::consts::PI;
            controller.pitch = if frame < 12 { -0.35 } else { -0.25 };
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
        impact_audio.update(wrench.hits);
        if active || capture_dir.is_some() {
            let d = controller.position - previous_position;
            let distance = (d.0 * d.0 + d.2 * d.2).sqrt();
            character.update(
                distance,
                if capture_dir.is_some() {
                    1. / 60.
                } else {
                    get_frame_time()
                },
                controller.is_grounded() && distance > 0.0001,
            );
        }
        let render_controller = if active && capture_dir.is_none() {
            stepper.pose(&controller)
        } else {
            controller.clone()
        };
        let view = perspective.view(&render_controller, &room);
        let aim = view.aim(&controller, &room);
        let mut camera_eye = view.eye;
        let mut camera_target = view.target;
        // Capture-only front portrait exposes the default skin for visual QA.
        if character_capture && (72..96).contains(&frame) {
            camera_eye = controller.position + V(1.4, -0.25, -2.8);
            camera_target = controller.position + V(0., -0.55, 0.);
        }
        clear_background(Color::new(0.48, 0.70, 0.86, 1.));
        set_camera(&Camera3D {
            position: mesh::vec(camera_eye),
            target: mesh::vec(camera_target),
            up: vec3(0., 1., 0.),
            fovy: fov.to_radians(),
            z_near: 0.045,
            z_far: 60.,
            ..Default::default()
        });
        material.set_uniform("Eye", mesh::vec(camera_eye));
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
        if view.show_body {
            character.draw(&render_controller, &wrench, &wrench_view);
        }
        if let Some(hit) = &wrench.impact {
            let p = mesh::vec(hit.point + hit.normal * 0.015);
            for i in 0..12 {
                let a = i as f32 * 2.399;
                let velocity = vec3(a.cos(), (i as f32 * 1.7).sin().abs(), a.sin()) * 0.7
                    + mesh::vec(hit.normal) * 0.8;
                let tip = p + velocity * hit.age + vec3(0., -1.8 * hit.age * hit.age, 0.);
                if hit.age < 0.3 {
                    draw_line_3d(
                        tip,
                        tip - velocity * 0.035,
                        Color::new(1., 0.72, 0.25, 1. - hit.age / 0.3),
                    );
                }
            }
        }
        set_default_camera();
        if perspective == Perspective::First || !view.show_body {
            wrench_view.draw(&wrench);
        }
        let sw = screen_width();
        let sh = screen_height();
        if (active || capture_dir.is_some()) && hud {
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
                "Left-click   Swing wrench     E   Interact     Right-click   Dismiss info",
                38.,
                sh - 34.,
                18.,
                INK,
            );
        }
        if active || capture_dir.is_some() {
            draw_circle(sw * 0.5, sh * 0.5, 2., Color::new(0.92, 0.96, 1., 0.85));
            if let Some(hit) = &wrench.impact {
                let c = Color::new(1., 0.76, 0.35, 1. - hit.age / 0.65);
                let (cx, cy) = (sw * 0.5, sh * 0.5);
                for (x, y) in [(-1., -1.), (1., 1.), (-1., 1.), (1., -1.)] {
                    draw_line(cx + x * 5., cy + y * 5., cx + x * 11., cy + y * 11., 2., c);
                }
            }
            if let Some(info) = &interactions.feedback {
                let width = 440_f32.min(sw - 48.);
                let x = sw - width - 24.;
                let y = 94.;
                let lines = wrapped_lines(info.description, width - 36.);
                let height = 84. + lines.len() as f32 * 24.;
                draw_rectangle(x, y, width, height, Color::new(0.025, 0.045, 0.08, 0.94));
                draw_rectangle(x, y, 3., height, BLUE);
                text(&info.title, x + 18., y + 31., 22., INK);
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
            if let Some(entity) = room.focus(aim) {
                draw_circle_lines(sw * 0.5, sh * 0.5, 7., 1.5, BLUE);
                let label = format!(
                    "{}  |  E: {}",
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
        if !active
            && (capture_dir.is_none()
                || ((interaction_capture || wrench_capture) && frame >= 60)
                || (character_capture && frame >= 96)
                || (house_capture && frame >= 132))
        {
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
            text(
                "B E 2   /   B L U E   E N G I N E  2",
                left,
                y + 47.,
                20.,
                BLUE,
            );
            text(
                if entered {
                    "Take your time."
                } else {
                    "A house to explore."
                },
                left,
                y + 101.,
                37.,
                INK,
            );
            text(&room.name, left, y + 135., 20., MUTED);
            if button(
                if entered {
                    "Resume exploring    /    Enter"
                } else {
                    "Start exploring    /    Enter"
                },
                Rect::new(left, y + 164., width, 52.),
                true,
            ) || (focused && keys.pressed(KeyCode::Enter))
            {
                active = true;
                entered = true;
                keys.down.clear();
                controller.stop();
                stepper.reset(&controller);
                wrench.cancel();
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
            text("Left-click / E", left, y + 338., 21., INK);
            text("Swing wrench / Use", left + 210., y + 338., 18., MUTED);
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
                stepper.reset(&controller);
                keys.down.clear();
            }
            text("Q  First / Third person", left, y + 555., 17., INK);
            text(
                "F  Fullscreen / Maximized     H  Toggle hints     F3  Stats",
                left,
                y + 574.,
                17.,
                MUTED,
            );
            if button("Quit", Rect::new(left, y + 594., 80., 34.), false) {
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
            let shot = if character_capture {
                [10, 22, 34, 46, 58, 70, 94, 106]
                    .iter()
                    .position(|f| *f == frame)
            } else if wrench_capture {
                [10, 24, 42, 70].iter().position(|f| *f == frame)
            } else if motion_capture {
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
                == if house_capture {
                    143
                } else if character_capture {
                    107
                } else if motion_capture {
                    85
                } else if interaction_capture || wrench_capture {
                    71
                } else {
                    35
                }
            {
                let mean = samples.iter().sum::<f32>() / samples.len() as f32;
                let report=format!("{}viewport={}x{}\nstartup_seconds={setup_seconds:.3}\ntriangles={triangles}\nvertices={}\nbatches={}\nmean_frame_ms={:.3}\ncaptured_eye_heights={captured_heights:?}\nwrench_hits={}\naudio_plays={}\n",platform_window::report(),screen_width(),screen_height(),meshes.iter().map(|m|m.vertices.len()).sum::<usize>(),meshes.len(),mean*1000.,wrench.hits,impact_audio.plays);
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
    fn complete_between_frame_tap_keeps_press_edge() {
        use miniquad::EventHandler;
        let mut keys = Keys::default();
        keys.key_down_event(KeyCode::Space, miniquad::KeyMods::default(), false);
        keys.key_up_event(KeyCode::Space, miniquad::KeyMods::default());
        assert!(keys.pressed.contains(&KeyCode::Space));
        assert!(!keys.down.contains(&KeyCode::Space));
    }
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
