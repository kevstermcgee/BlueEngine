//! Shared BlueEngine native shell + fixed-step lifecycle; this file only maps devices and presents state.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod platform;
use macroquad::prelude::*;
use odd_matter::{
    cells, verification_input, verification_route, Block, Cell, Direction, Event, Forecast, Intent, Sim, Status, ROOMS,
};
use vesper3d::viewer::{
    devkit::{beside_exe, flag_value, has_flag, parse_size, synth, Lifecycle, Notice, Settings, Simulation},
    game_client::{self, AudioMenu, GameShell},
    game_input::{mouse_pixels, ClientInput},
    identity::Identity,
    kit::{self, hud, Batch, Look, Materials, Rendered, SoundBank, Template, Tint},
};
const IDENTITY: &str = include_str!("../assets/identity.json");
const INK: Color = Color::new(0.035, 0.055, 0.09, 1.);
const PANEL: Color = Color::new(0.055, 0.08, 0.125, 0.97);
const MUTED: Color = Color::new(0.48, 0.59, 0.68, 1.);
const PAPER: Color = Color::new(0.88, 0.94, 0.96, 1.);
const TEAL: Color = Color::new(0.22, 0.94, 0.81, 1.);
const RED: Color = Color::new(1., 0.34, 0.4, 1.);
const BAR_RGB: [[f32; 3]; 3] = [[1., 0.69, 0.23], [1., 0.38, 0.64], [0.64, 0.96, 0.38]];
fn identity() -> Identity {
    Identity::parse(IDENTITY).expect("invalid game identity")
}
fn window() -> macroquad::conf::Conf {
    platform::attach_console();
    let mut c = game_client::window_config_with_icon(
        &identity().title,
        game_client::icon_from_rgba(
            include_bytes!("../assets/icon_16.rgba"),
            include_bytes!("../assets/icon_32.rgba"),
            include_bytes!("../assets/icon_64.rgba"),
        ),
    );
    c.miniquad_conf.window_width = 1280;
    c.miniquad_conf.window_height = 800;
    let args: Vec<String> = std::env::args().collect();
    if let Some((w, h)) = flag_value(&args, "--size").and_then(parse_size) {
        c.miniquad_conf.window_width = w.clamp(640, 7680) as i32;
        c.miniquad_conf.window_height = h.clamp(400, 4320) as i32;
    }
    if has_flag(&args, "--novsync") {
        c.miniquad_conf.platform.swap_interval = Some(0);
    }
    c
}

const KEYS: [KeyCode; 13] = [
    KeyCode::Key1,
    KeyCode::Key2,
    KeyCode::Key3,
    KeyCode::Key4,
    KeyCode::A,
    KeyCode::D,
    KeyCode::Q,
    KeyCode::E,
    KeyCode::S,
    KeyCode::W,
    KeyCode::Z,
    KeyCode::R,
    KeyCode::Enter,
];
const CUES: [&str; 15] = [
    "drone", "bar1", "bar2", "bar3", "left", "right", "down", "up", "front", "back", "undo", "restart", "next", "cut",
    "orbit",
];
#[derive(Clone, Copy, Default)]
struct Held;
fn intent(bits: u32) -> Intent {
    Intent {
        select: (0..4).find(|i| bits & (1 << i) != 0).map(|i| i as u8),
        direction: (0..6).find(|i| bits & (1 << (i + 4)) != 0).map(|i| Direction::ALL[i]),
        undo: bits & (1 << 10) != 0,
        restart: bits & (1 << 11) != 0,
        advance: bits & (1 << 12) != 0,
    }
}
const SOUNDS: [synth::Preset; 7] = [
    synth::Preset::Select,
    synth::Preset::Blip,
    synth::Preset::Chime,
    synth::Preset::Thump,
    synth::Preset::GameOver,
    synth::Preset::Success,
    synth::Preset::Back,
];
fn render_audio() -> Rendered {
    Rendered {
        sfx: SOUNDS
            .iter()
            .map(|p| (0..p.variants()).map(|v| synth::wav_bytes(&synth::render(*p, v, 17), synth::RATE)).collect())
            .collect(),
        stems: Vec::new(),
    }
}

/// Uniform logical layout, including pointer hit testing, survives window resizing.
#[derive(Clone, Copy)]
struct Ui<'a> {
    s: f32,
    x: f32,
    y: f32,
    font: &'a Font,
}
impl<'a> Ui<'a> {
    fn new(font: &'a Font) -> Self {
        let s = (screen_width() / 1280.).min(screen_height() / 800.);
        Self { s, x: (screen_width() - 1280. * s) * 0.5, y: (screen_height() - 800. * s) * 0.5, font }
    }
    fn point(self, x: f32, y: f32) -> Vec2 {
        vec2(self.x + x * self.s, self.y + y * self.s)
    }
    fn mouse(self) -> Vec2 {
        let (x, y) = mouse_position();
        vec2((x - self.x) / self.s, (y - self.y) / self.s)
    }
    fn rect(self, r: Rect, c: Color) {
        draw_rectangle(self.x + r.x * self.s, self.y + r.y * self.s, r.w * self.s, r.h * self.s, c);
    }
    fn stroke(self, r: Rect, c: Color) {
        draw_rectangle_lines(self.x + r.x * self.s, self.y + r.y * self.s, r.w * self.s, r.h * self.s, 1.2 * self.s, c);
    }
    fn line(self, a: Vec2, b: Vec2, c: Color, width: f32) {
        let a = self.point(a.x, a.y);
        let b = self.point(b.x, b.y);
        draw_line(a.x, a.y, b.x, b.y, width * self.s, c);
    }
    fn text(self, t: &str, x: f32, y: f32, size: f32, c: Color) {
        draw_text_ex(
            t,
            self.x + x * self.s,
            self.y + y * self.s,
            TextParams {
                font: Some(self.font),
                font_size: size as u16,
                font_scale: self.s,
                color: c,
                ..Default::default()
            },
        );
    }
    fn center(self, t: &str, x: f32, y: f32, size: f32, c: Color) {
        let width = measure_text(t, Some(self.font), size as u16, 1.).width * self.s;
        draw_text_ex(
            t,
            self.x + x * self.s - width * 0.5,
            self.y + y * self.s,
            TextParams {
                font: Some(self.font),
                font_size: size as u16,
                font_scale: self.s,
                color: c,
                ..Default::default()
            },
        );
    }
    fn wrap(self, t: &str, x: f32, mut y: f32, width: f32, size: f32, c: Color) -> f32 {
        let mut line = String::new();
        for word in t.split_whitespace() {
            let next = if line.is_empty() { word.into() } else { format!("{line} {word}") };
            if measure_text(&next, Some(self.font), size as u16, 1.).width > width && !line.is_empty() {
                self.text(&line, x, y, size, c);
                y += size * 1.3;
                line = word.into();
            } else {
                line = next;
            }
        }
        self.text(&line, x, y, size, c);
        y + size * 1.3
    }
}
fn color(rgb: [f32; 3], a: f32) -> Color {
    Color::new(rgb[0], rgb[1], rgb[2], a)
}
fn tint(rgb: [f32; 3], a: f32) -> Tint {
    Tint { mul: rgb, alpha: a, ..Tint::NONE }
}
fn pos(p: Cell) -> Vec3 {
    vec3(p[0] as f32 - 2., p[1] as f32, p[2] as f32 - 2.)
}
fn select_rect(i: usize) -> Rect {
    Rect::new(28., 238. + i as f32 * 56., 236., 48.)
}
fn direction_rect(i: usize) -> Rect {
    Rect::new(298. + i as f32 * 111., 631., 103., 40.)
}
fn undo_rect() -> Rect {
    Rect::new(1022., 715., 107., 38.)
}
fn retry_rect() -> Rect {
    Rect::new(1137., 715., 107., 38.)
}
fn next_rect() -> Rect {
    Rect::new(468., 423., 347., 42.)
}

struct Models {
    cube: Template,
    wire: Template,
    drone: Template,
    socket: Template,
    stage: Vec<Mesh>,
}
fn models() -> Models {
    let mut cube = Template::new();
    cube.box_top(Vec3::ZERO, Vec3::splat(0.5), [0.65, 0.72, 0.85], [0.93, 0.96, 1.], 0.18);
    let mut wire = Template::new();
    for a in 0..3 {
        for b in [-0.5, 0.5] {
            for c in [-0.5, 0.5] {
                let mut p = Vec3::ZERO;
                p[(a + 1) % 3] = b;
                p[(a + 2) % 3] = c;
                let mut half = Vec3::splat(0.007);
                half[a] = 0.507;
                wire.box_(p, half, [1.; 3], 1.);
            }
        }
    }
    let mut drone = Template::new();
    drone.rounded_box(Vec3::ZERO, vec3(0.16, 0.10, 0.15), 0.045, [0.83, 0.96, 1.], 0.3);
    drone.box_(vec3(0., 0.045, -0.16), vec3(0.065, 0.04, 0.015), [0.15, 1., 0.85], 1.);
    for x in [-0.23, 0.23] {
        for z in [-0.23, 0.23] {
            drone.rod(Vec3::ZERO, vec3(x, 0., z), 0.027, 0.027, [0.22, 0.4, 0.48], 0.2, 6);
            drone.ring(vec3(x, 0.04, z), 0.064, 0.09, [0.2, 0.95, 0.85], 0.95, 12);
            drone.ball(vec3(x, 0., z), Vec3::splat(0.043), [0.7, 0.93, 1.], 0.7, 8, 5);
        }
    }
    let mut socket = Template::new();
    socket.ring(Vec3::ZERO, 0.28, 0.37, [0.2, 0.94, 0.7], 1., 28);
    for x in [-0.32, 0.32] {
        socket.box_(vec3(x, 0., 0.), vec3(0.025, 0.08, 0.06), [0.65, 1., 0.86], 1.);
    }
    let mut stage = Template::new();
    stage.rounded_box(vec3(0., -0.75, 0.), vec3(3.25, 0.24, 3.25), 0.12, [0.06, 0.09, 0.14], 0.1);
    stage.box_(vec3(0., -0.48, 0.), vec3(3.02, 0.015, 3.02), [0.11, 0.2, 0.26], 0.3);
    for edge in [-2.65, 2.65] {
        stage.box_(vec3(edge, 2., -2.65), vec3(0.035, 2.5, 0.035), [0.13, 0.26, 0.32], 0.3);
        stage.box_(vec3(edge, 2., 2.65), vec3(0.035, 2.5, 0.035), [0.13, 0.26, 0.32], 0.3);
        for y in [-0.5, 4.5] {
            stage.box_(vec3(edge, y, 0.), vec3(0.035, 0.035, 2.65), [0.13, 0.26, 0.32], 0.3);
            stage.box_(vec3(0., y, edge), vec3(2.65, 0.035, 0.035), [0.13, 0.26, 0.32], 0.3);
        }
    }
    for i in -2..=2 {
        stage.box_(vec3(i as f32, -0.45, 0.), vec3(0.007, 0.009, 2.5), [0.17, 0.38, 0.41], 0.6);
        stage.box_(vec3(0., -0.45, i as f32), vec3(2.5, 0.009, 0.007), [0.17, 0.38, 0.41], 0.6);
    }
    for corner in [-2.9, 2.9] {
        for z in [-2.9, 2.9] {
            stage.box_(vec3(corner, -0.42, z), vec3(0.08, 0.04, 0.08), [0.1, 0.9, 0.77], 1.);
        }
    }
    Models { cube, wire, drone, socket, stage: stage.to_meshes() }
}
fn instance(batch: &mut Batch, t: &Template, p: Vec3, scale: Vec3, rgb: [f32; 3], a: f32) {
    batch.add(t, Mat4::from_scale_rotation_translation(scale, Quat::IDENTITY, p), tint(rgb, a));
}
fn rail(batch: &mut Batch, m: &Models, axis: usize, a: Vec3, b: Vec3, rgb: [f32; 3]) {
    let mut s = Vec3::splat(0.035);
    s[axis] = (a[axis] - b[axis]).abs() + 0.04;
    instance(batch, &m.cube, (a + b) * 0.5, s, rgb, 1.);
}
struct Orbit {
    yaw: f32,
    pitch: f32,
    distance: f32,
}
impl Orbit {
    fn new() -> Self {
        Self { yaw: 0.63, pitch: 0.51, distance: 13.7 }
    }
    fn eye(&self) -> Vec3 {
        let flat = self.pitch.cos() * self.distance;
        vec3(self.yaw.sin() * flat, 2. + self.pitch.sin() * self.distance, self.yaw.cos() * flat)
    }
    fn camera(&self, u: Ui) -> Camera3D {
        let p = u.point(277., 132.);
        let w = (726. * u.s) as i32;
        let h = (481. * u.s) as i32;
        Camera3D {
            position: self.eye(),
            target: vec3(0., 1.8, 0.),
            up: Vec3::Y,
            fovy: 0.69,
            aspect: Some(w as f32 / h as f32),
            z_near: 0.1,
            z_far: 50.,
            viewport: Some((p.x as i32, (screen_height() - p.y - h as f32) as i32, w, h)),
            ..Default::default()
        }
    }
}
fn projected(camera: &Camera3D, p: Vec3, u: Ui) -> Vec2 {
    let q = camera.matrix() * p.extend(1.);
    let q = q.truncate() / q.w;
    vec2(277. + (q.x * 0.5 + 0.5) * 726., 132. + (0.5 - q.y * 0.5) * 481.) * u.s + vec2(u.x, u.y)
}

/// Read-only presentation data shared by the world and HUD for this frame.
#[derive(Clone, Copy)]
struct Frame<'a> {
    direction: Direction,
    forecast: &'a Forecast,
    cut: bool,
    hints: bool,
    message: &'a str,
    time: f32,
    flash: f32,
}

/// Geometry is derived from parity. Hidden obstruction outlines are intentional x-ray overlays.
fn draw_world(s: &Sim, m: &Models, materials: &Materials, look: &Look, orbit: &Orbit, u: Ui, frame: &Frame) {
    let Frame { forecast: f, cut, time, flash, .. } = *frame;
    let c = s.room();
    let b = &s.board;
    let eye = orbit.eye();
    let drone = pos(b.drone);
    let mut opaque = Batch::new();
    let mut outlines = Batch::new();
    let mut effects = Batch::new();
    let toward = (eye - drone).normalize();
    for p in cells() {
        let n = c.layers(&b.offsets, p);
        let at = pos(p);
        if n % 2 == 1 {
            let removed = cut && ((at.y > drone.y + 0.1) || ((at - drone).dot(toward) > 0.25));
            let rgb = if n >= 3 {
                [0.8, 0.24, 0.36]
            } else if c.fixed(p) {
                [0.32, 0.46, 0.60]
            } else {
                c.bars
                    .iter()
                    .enumerate()
                    .find(|(i, bar)| bar.contains(b.offsets[*i], p))
                    .map_or([0.32, 0.46, 0.60], |(i, _)| BAR_RGB[i])
            };
            if removed {
                instance(&mut outlines, &m.wire, at, Vec3::splat(0.91), rgb, 0.07);
            } else {
                instance(&mut opaque, &m.cube, at, Vec3::splat(0.9), rgb, 1.);
            }
        } else if n > 0 {
            instance(&mut outlines, &m.wire, at, Vec3::splat(0.91), [0.13, 0.88, 0.76], 0.22);
            instance(&mut effects, &m.cube, at, Vec3::splat(0.045), [0.2, 0.95, 0.8], 0.5);
        }
    }
    for (i, bar) in c.bars.iter().enumerate() {
        let o = bar.origin_at(b.offsets[i]);
        let center = pos(o) + vec3((bar.size[0] - 1) as f32, (bar.size[1] - 1) as f32, (bar.size[2] - 1) as f32) * 0.5;
        let dims = vec3(bar.size[0] as f32, bar.size[1] as f32, bar.size[2] as f32);
        instance(
            &mut outlines,
            &m.wire,
            center,
            dims * 0.975,
            BAR_RGB[i],
            if b.selected == i as u8 + 1 { 0.9 } else { 0.36 },
        );
        let mut from = vec3(-3.12, 0., 3.03); // rail guides sit outside the voxel frame
        match bar.rail {
            0 => {
                from = vec3(-2., -0.30, 3.1 + i as f32 * 0.16);
            }
            1 => {
                from.y = 0.;
                from.z = 2.8 - i as f32 * 0.35;
            }
            _ => {
                from = vec3(2.9 + i as f32 * 0.22, -0.25, -2.);
            }
        }
        let mut to = from;
        to[bar.rail] += bar.max_offset() as f32;
        rail(&mut opaque, m, bar.rail, from, to, BAR_RGB[i]);
        for stop in 0..=bar.max_offset() {
            let mut p = from;
            p[bar.rail] += stop as f32;
            instance(
                &mut opaque,
                &m.cube,
                p,
                Vec3::splat(if stop == b.offsets[i] { 0.14 } else { 0.05 }),
                BAR_RGB[i],
                1.,
            );
        }
    }
    if f.blocked.is_none() && b.selected != 0 {
        for p in &f.opened {
            instance(&mut effects, &m.wire, pos(*p), Vec3::splat(0.97), [0.24, 1., 0.68], 0.43);
        }
        for p in &f.closed {
            instance(&mut effects, &m.wire, pos(*p), Vec3::splat(0.97), [1., 0.3, 0.4], 0.38);
        }
    } else if f.blocked != Some(Block::Finished) && b.selected == 0 && odd_matter::inside(f.destination) {
        instance(
            &mut effects,
            &m.wire,
            pos(f.destination),
            Vec3::splat(0.97),
            if f.blocked.is_some() { [1., 0.3, 0.4] } else { [0.2, 1., 0.75] },
            0.5,
        );
    }
    let drone_color = if b.status == Status::Crushed || f.crush { [1., 0.25, 0.3] } else { [0.65, 1., 0.94] };
    instance(&mut opaque, &m.drone, drone, Vec3::ONE, drone_color, 1.);
    instance(
        &mut effects,
        &m.wire,
        drone,
        Vec3::splat(0.62 + flash * 0.10),
        drone_color,
        0.55 + 0.2 * (time * 3.).sin(),
    );
    let goal = pos(c.exit);
    effects.add(
        &m.socket,
        Mat4::from_scale_rotation_translation(Vec3::ONE, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2), goal),
        Tint::alpha(0.8),
    );
    let camera = orbit.camera(u);
    set_camera(&camera);
    materials.set_scene(look, eye, time, 0.25);
    materials.draw_static(&m.stage);
    gl_use_material(&materials.world);
    opaque.draw();
    gl_use_material(&materials.fx_alpha);
    outlines.draw();
    effects.draw();
    gl_use_default_material();
    set_default_camera();
    let p = projected(&camera, drone, u);
    hud::text_outlined(
        "1",
        p.x + 13. * u.s,
        p.y - 17. * u.s,
        19. * u.s,
        if b.status == Status::Crushed { RED } else { TEAL },
    );
    let p = projected(&camera, goal, u);
    hud::text_outlined("EXIT", p.x - 16. * u.s, p.y - 28. * u.s, 14. * u.s, TEAL);
    // A projected, labeled compass follows the orbit while the world axes stay fixed.
    let origin = vec2(943., 582.);
    let center = projected(&camera, vec3(0., 2., 0.), u);
    for (axis, label, rgb) in
        [(Vec3::X, "D +X", [1., 0.55, 0.42]), (Vec3::Y, "E +Y", [0.5, 1., 0.65]), (-Vec3::Z, "W -Z", [0.44, 0.73, 1.])]
    {
        let v = (projected(&camera, vec3(0., 2., 0.) + axis, u) - center).normalize_or_zero() * 31.;
        let end = origin + v;
        u.line(origin, end, color(rgb, 1.), 2.);
        u.text(label, end.x - 16., end.y - 6., 13., color(rgb, 1.));
    }
}

fn slices(u: Ui, s: &Sim, f: &Forecast) {
    let c = s.room();
    let b = &s.board;
    u.text("DRONE CROSS-SECTIONS", 1022., 160., 17., PAPER);
    for (i, (a, vertical, fixed, name)) in [(0, 1, 2, "XY"), (0, 2, 1, "XZ"), (2, 1, 0, "ZY")].into_iter().enumerate() {
        let y = 175. + i as f32 * 153.;
        u.rect(Rect::new(1022., y, 222., 139.), PANEL);
        u.text(&format!("{name} / {} = {}", ["X", "Y", "Z"][fixed], b.drone[fixed] + 1), 1036., y + 23., 15., PAPER);
        let ox = 1038.;
        let oy = y + 39.;
        let size = 17.;
        for row in 0..5 {
            for col in 0..5 {
                let mut p = b.drone;
                p[a] = col;
                p[vertical] = 4 - row;
                let n = c.layers(&b.offsets, p);
                let fill = match n {
                    0 => Color::new(0.075, 0.12, 0.17, 1.),
                    1 => Color::new(0.25, 0.34, 0.44, 1.),
                    2 => Color::new(0.07, 0.42, 0.39, 1.),
                    3 => Color::new(0.6, 0.18, 0.27, 1.),
                    _ => Color::new(0.2, 0.52, 0.28, 1.),
                };
                let r = Rect::new(ox + col as f32 * size, oy + row as f32 * size, size - 2., size - 2.);
                u.rect(r, fill);
                if f.opened.contains(&p) {
                    u.stroke(r, TEAL);
                }
                if f.closed.contains(&p) {
                    u.stroke(r, RED);
                }
                u.text(&n.to_string(), r.x + 4.5, r.y + 11., 11., Color::new(0.8, 0.89, 0.95, 0.72));
                if p == b.drone {
                    let center = u.point(r.x + 7., r.y + 7.);
                    draw_circle(center.x, center.y, 4.2 * u.s, TEAL);
                    draw_circle_lines(center.x, center.y, 6. * u.s, 1.4 * u.s, PAPER);
                }
                if p == c.exit {
                    u.stroke(r, TEAL);
                }
            }
        }
        u.text("ODD", 1140., y + 61., 13., MUTED);
        u.text("solid", 1140., y + 79., 17., PAPER);
        u.text("EVEN", 1140., y + 106., 13., TEAL);
        u.text("empty", 1140., y + 124., 17., PAPER);
    }
}
fn button(u: Ui, r: Rect, t: &str, selected: bool, enabled: bool, accent: Color) {
    u.rect(r, if selected { Color::new(0.09, 0.2, 0.24, 1.) } else { PANEL });
    if selected || (enabled && r.contains(u.mouse())) {
        u.stroke(r, accent);
    } else {
        u.stroke(r, Color::new(0.15, 0.22, 0.29, 1.));
    }
    u.center(
        t,
        r.x + r.w * 0.5,
        r.y + r.h * 0.5 + 6.,
        16.,
        if enabled { PAPER } else { Color::new(0.29, 0.36, 0.41, 1.) },
    );
}
fn draw_ui(u: Ui, s: &Sim, frame: &Frame) {
    let Frame { direction, forecast: f, cut, hints, message, flash, .. } = *frame;
    let c = s.room();
    let b = &s.board;
    u.text("ODD MATTER", 28., 56., 42., PAPER);
    u.text("MORE MATTER. LESS WALL.", 30., 80., 14., TEAL);
    u.line(vec2(28., 102.), vec2(1252., 102.), Color::new(0.16, 0.26, 0.3, 1.), 1.);
    u.text("PARITY LAB / 06 VAULTS", 1008., 53., 17., MUTED);
    for i in 0..ROOMS {
        let r = Rect::new(1009. + i as f32 * 39., 70., 29., 5.);
        u.rect(
            r,
            if i < b.room {
                TEAL
            } else if i == b.room {
                PAPER
            } else {
                Color::new(0.16, 0.24, 0.3, 1.)
            },
        );
    }
    u.text(&format!("VAULT {:02} / 06", b.room + 1), 28., 141., 17., TEAL);
    u.wrap(c.title, 28., 173., 235., 27., PAPER);
    u.wrap(c.lesson, 28., 207., 235., 15., MUTED);
    for i in 0..4 {
        let r = select_rect(i);
        let available = i == 0 || i <= c.bars.len();
        let selected = b.selected as usize == i;
        u.rect(r, if selected { Color::new(0.08, 0.19, 0.22, 1.) } else { PANEL });
        let rgb = if i == 0 { [0.2, 0.94, 0.81] } else { BAR_RGB[i - 1] };
        if selected {
            u.stroke(r, color(rgb, 1.));
        }
        u.rect(Rect::new(r.x + 10., r.y + 10., 27., 27.), color(rgb, if available { 0.8 } else { 0.2 }));
        u.center(&(i + 1).to_string(), r.x + 23., r.y + 30., 20., INK);
        u.text(
            if i == 0 { "DRONE" } else { ["AMBER", "ROSE", "LIME"][i - 1] },
            r.x + 49.,
            r.y + 20.,
            18.,
            if available { PAPER } else { MUTED },
        );
        let detail = if i == 0 {
            format!("X{} Y{} Z{} / free flight", b.drone[0] + 1, b.drone[1] + 1, b.drone[2] + 1)
        } else if available {
            let bar = &c.bars[i - 1];
            format!(
                "{} rail / {} / slot {}",
                ["X", "Y", "Z"][bar.rail],
                ["A-D", "Q-E", "W-S"][bar.rail],
                b.offsets[i - 1] + 1
            )
        } else {
            "not in this vault".into()
        };
        u.text(&detail, r.x + 49., r.y + 38., 13., MUTED);
    }
    u.rect(Rect::new(28., 479., 236., 162.), PANEL);
    u.text("COUNT THE LAYERS", 42., 505., 16., PAPER);
    for (i, (n, t, accent)) in
        [(1, "SOLID", MUTED), (2, "EMPTY", TEAL), (3, "SOLID", RED), (4, "EMPTY", TEAL)].into_iter().enumerate()
    {
        let x = 42. + (i % 2) as f32 * 110.;
        let y = 532. + (i / 2) as f32 * 43.;
        u.text(&n.to_string(), x, y, 24., accent);
        u.text(t, x + 27., y - 2., 14., accent);
        u.text(if n % 2 == 0 { "pass through" } else { "blocks you" }, x, y + 15., 11., MUTED);
    }
    u.text("Fixed matter counts as one.", 42., 626., 13., MUTED);
    u.text("NO TIMER. NO MOVE LIMIT.", 28., 667., 13., TEAL);
    u.text(&format!("{} moves / {} undo steps", b.moves, s.undo_len()), 28., 689., 14., MUTED);
    u.wrap("Z undoes a crush, too. R resets only this vault.", 28., 718., 234., 15., PAPER);

    u.text(&format!("{:02}  /  {}", b.room + 1, c.title.to_uppercase()), 298., 128., 21., PAPER);
    u.text(
        if cut { "CUTAWAY / hidden solids stay outlined" } else { "HOLD C to see inside / right-drag to orbit" },
        298.,
        602.,
        14.,
        MUTED,
    );
    for (i, d) in Direction::ALL.into_iter().enumerate() {
        let allowed = b.selected == 0 || c.bars[(b.selected - 1) as usize].rail == d.axis();
        button(
            u,
            direction_rect(i),
            &format!("{}  {}", d.key(), d.label()),
            d == direction,
            allowed,
            if f.crush { RED } else { TEAL },
        );
    }
    let summary = if let Some(block) = f.blocked {
        format!("{} / {}", direction.key(), block.label())
    } else if f.crush {
        format!("{} / CRUSHES DRONE  -  Z can undo", direction.key())
    } else if b.selected == 0 {
        format!("{} / EMPTY destination - safe to move", direction.key())
    } else {
        format!("{} / {} open + {} close / DRONE SAFE", direction.key(), f.opened.len(), f.closed.len())
    };
    u.text(&summary, 301., 697., 18., if f.crush { RED } else { TEAL });
    u.text("Hover a direction to forecast. Click or press its key to move.", 301., 720., 14., MUTED);
    u.text("Shift + direction previews without moving.", 301., 741., 14., MUTED);
    slices(u, s, f);
    u.text("CONFIRMED ACTION", 1022., 660., 14., MUTED);
    u.wrap(message, 1022., 680., 222., 13., if flash > 0.1 { TEAL } else { PAPER });
    button(u, undo_rect(), "Z  UNDO", false, s.undo_len() > 0, TEAL);
    button(u, retry_rect(), "R  RETRY", false, true, TEAL);
    u.text("F5 save / F9 load", 1022., 775., 14., MUTED);
    u.text("Esc pause / H field notes", 1022., 797., 13., MUTED);
    u.line(vec2(28., 777.), vec2(977., 777.), Color::new(0.15, 0.24, 0.29, 1.), 1.);
    u.text("W BACK (-Z)   S FRONT (+Z)   A/D SIDEWAYS   Q DOWN   E UP", 30., 794., 13., MUTED);

    if hints {
        u.rect(Rect::new(292., 169., 694., 170.), Color::new(0.035, 0.07, 0.10, 0.96));
        u.stroke(Rect::new(292., 169., 694., 170.), TEAL);
        u.text("FIELD NOTES / H TO CLOSE", 314., 198., 21., TEAL);
        let y = u.wrap(c.hint, 314., 229., 649., 19., PAPER);
        u.wrap("Bars interpenetrate. The drone stays put while a bar slides. If its cell turns odd, it is crushed. Preview green opens / red closes.",314.,y+8.,649.,16.,MUTED);
    }
    if b.status != Status::Playing {
        let (title, detail, accent) = match b.status {
            Status::Crushed => ("DRONE CRUSHED", "The occupied cell became odd. Undo keeps your progress.", RED),
            Status::Cleared => ("VAULT STABILIZED", "Drone docked. The next chamber adds another layer.", TEAL),
            _ => ("ALL SIX VAULTS OPEN", "You made room by adding matter. Oddly satisfying.", TEAL),
        };
        let r = Rect::new(372., 309., 539., 178.);
        u.rect(r, Color::new(0.025, 0.055, 0.08, 0.96));
        u.stroke(r, accent);
        u.center(title, 641., 351., 31., accent);
        u.center(detail, 641., 383., 16., PAPER);
        button(
            u,
            next_rect(),
            match b.status {
                Status::Crushed => "Z UNDO  /  R RETRY",
                Status::Cleared => "ENTER  /  NEXT VAULT",
                _ => "ENTER  /  PLAY AGAIN",
            },
            false,
            true,
            accent,
        );
    }
}

fn announce(message: &mut String, notice: Notice) {
    *message = format!("{}: {}", notice.title, notice.detail);
}
#[macroquad::main(window)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut life = Lifecycle::<Held>::start_or_exit(&args, &CUES);
    let unattended = life.options.unattended();
    let verify = has_flag(&args, "--verify-route");
    let mut sim = Sim::new(life.seed());
    life.load_flag_or_exit(&mut sim);
    let mut shell = GameShell::new();
    let mut input = ClientInput::new();
    let font = load_ttf_font_from_bytes(include_bytes!("../assets/ui.ttf")).expect("bundled UI font");
    let materials = Materials::load().expect("native materials failed to compile");
    let m = models();
    let look = Look {
        ambient_sky: [0.42, 0.55, 0.69],
        ambient_ground: [0.11, 0.19, 0.25],
        key_direction: [-0.5, 0.8, 0.3],
        key_color: [0.9, 1., 1.05],
        rim_color: [0.1, 0.7, 0.65],
        rim_strength: 0.25,
        fog_color: [0.035, 0.055, 0.09],
        fog_density: 0.015,
        exposure: 1.,
    };
    let settings_path = beside_exe("settings.json");
    let mut settings = Settings::load(&settings_path);
    let mut sounds = SoundBank::start(life.options.silent(), settings.sfx_level(), 0., render_audio).await;
    let mut orbit = Orbit::new();
    let mut direction = Direction::Right;
    let mut message = "Select amber (2), then E to open a tunnel.".to_string();
    let mut flash = 0.;
    let mut hints = false;
    let mut last_selected = sim.board.selected;
    let mut last_room = sim.board.room;
    let mut native_commands = 0u64;
    let mut cut_frames = 0u64;
    let mut orbit_pixels = 0f32;
    let mut zoom_steps = 0f32;
    let mut saves = 0u64;
    let mut loads = 0u64;
    let mut preview_frames = 0u64;
    let mut cleared_vaults = 0u64;
    let mut crushes = 0u64;
    let mut undos = 0u64;
    loop {
        input.begin_frame_with_keyboard(&mut shell, false, unattended || platform::focused(), platform::keyboard());
        let dt = life.begin_frame(input.frame_seconds());
        let time = life.time();
        let u = Ui::new(&font);
        sounds.poll().await;
        let mut bits = 0u32;
        let mut cut = false;
        if let Some(script) = life.script() {
            for (i, cue) in CUES.iter().take(13).enumerate() {
                if script.starts(cue) {
                    bits |= 1 << i;
                }
            }
            cut = script.held("cut");
            orbit.yaw += script.value("orbit", 0);
            orbit.pitch = (orbit.pitch + script.value("orbit", 1)).clamp(0.12, 1.2);
        } else if shell.accepting_input() {
            let preview_only = input.down(KeyCode::LeftShift) || input.down(KeyCode::RightShift);
            for (i, key) in KEYS.iter().enumerate() {
                if input.pressed(*key) {
                    if (4..10).contains(&i) {
                        direction = Direction::ALL[i - 4];
                    }
                    if !(preview_only && (4..10).contains(&i)) {
                        bits |= 1 << i;
                        native_commands += 1;
                    }
                }
            }
            if preview_only {
                preview_frames += 1;
            }
            cut = input.down(KeyCode::C);
            if input.pressed(KeyCode::H) {
                hints = !hints;
            }
            if is_mouse_button_down(MouseButton::Right) {
                let d = mouse_pixels();
                orbit.yaw += d[0] * 0.006;
                orbit.pitch = (orbit.pitch + d[1] * 0.004).clamp(0.12, 1.2);
                orbit_pixels += d[0].abs() + d[1].abs();
            }
            let wheel = mouse_wheel().1;
            orbit.distance = (orbit.distance - wheel * 0.6).clamp(9.5, 19.);
            zoom_steps += wheel.abs();
            let mouse = u.mouse();
            for (i, d) in Direction::ALL.iter().enumerate() {
                if direction_rect(i).contains(mouse) {
                    direction = *d;
                    if is_mouse_button_pressed(MouseButton::Left) {
                        bits |= 1 << (i + 4);
                        native_commands += 1;
                    }
                }
            }
            if is_mouse_button_pressed(MouseButton::Left) {
                for i in 0..=sim.room().bars.len() {
                    if select_rect(i).contains(mouse) {
                        bits |= 1 << i;
                        native_commands += 1;
                    }
                }
                if undo_rect().contains(mouse) || (sim.board.status == Status::Crushed && next_rect().contains(mouse)) {
                    bits |= 1 << 10;
                }
                if retry_rect().contains(mouse) {
                    bits |= 1 << 11;
                }
                if matches!(sim.board.status, Status::Cleared | Status::Won) && next_rect().contains(mouse) {
                    bits |= 1 << 12;
                }
            }
        }
        if cut {
            cut_frames += 1;
        }
        life.feed(Held, bits, [0., 0.]);
        let (save, load) = life.save_load_requested(
            shell.accepting_input() && input.pressed(KeyCode::F5),
            shell.accepting_input() && input.pressed(KeyCode::F9),
        );
        if save {
            saves += 1;
            announce(
                &mut message,
                life.quick_save(&sim, &format!("Vault {} / {} moves", sim.board.room + 1, sim.board.moves)),
            );
        }
        if load {
            loads += 1;
            announce(&mut message, life.quick_load(&mut sim));
        }
        for _ in 0..life.ticks(dt, 1., !shell.paused) {
            let tick = life.take_tick();
            let cmd = if verify { verification_input(sim.tick) } else { intent(tick.edges) };
            if let Some(d) = cmd.direction {
                direction = d;
            }
            sim.step(&cmd);
            for event in sim.drain_events() {
                let sound = match event {
                    Event::Selected => 0,
                    Event::Moved { .. } => {
                        message = "Drone moved into an empty cell.".into();
                        1
                    }
                    Event::Shifted { opened, closed } => {
                        message = format!("{opened} opened / {closed} closed.");
                        if opened > 0 {
                            2
                        } else {
                            1
                        }
                    }
                    Event::Blocked(reason) => {
                        message = format!("Blocked: {}.", reason.label());
                        flash = 0.65;
                        3
                    }
                    Event::Crushed => {
                        crushes += 1;
                        message = "Closure crushed the drone. Z reverses it.".into();
                        flash = 1.;
                        4
                    }
                    Event::Cleared | Event::Won => {
                        cleared_vaults += 1;
                        message = "Exit reached. Press Enter to continue.".into();
                        5
                    }
                    Event::Undone => {
                        undos += 1;
                        message = "Previous move restored, including matter.".into();
                        6
                    }
                    Event::Restarted => {
                        message = "This vault restarted. No progress penalty.".into();
                        6
                    }
                    Event::Advanced => {
                        message = sim.room().lesson.into();
                        hints = false;
                        orbit = Orbit::new();
                        0
                    }
                };
                sounds.play(sound, 0.6);
            }
        }
        if last_selected != sim.board.selected || last_room != sim.board.room {
            if sim.board.selected > 0 {
                let rail = sim.room().bars[(sim.board.selected - 1) as usize].rail;
                direction = Direction::ALL[rail * 2 + 1];
            }
            last_selected = sim.board.selected;
            last_room = sim.board.room;
        }
        flash = (flash - dt * 2.).max(0.);
        let forecast = sim.forecast(direction);
        clear_background(INK);
        // Quiet background grid gives the vault an instrument-panel setting.
        for x in (277..1002).step_by(40) {
            u.line(vec2(x as f32, 139.), vec2(x as f32, 610.), Color::new(0.05, 0.09, 0.13, 1.), 1.);
        }
        for y in (139..611).step_by(40) {
            u.line(vec2(277., y as f32), vec2(1002., y as f32), Color::new(0.05, 0.09, 0.13, 1.), 1.);
        }
        let frame = Frame { direction, forecast: &forecast, cut, hints, message: &message, time, flash };
        draw_world(&sim, &m, &materials, &look, &orbit, u, &frame);
        draw_ui(u, &sim, &frame);
        if shell.paused {
            hints = false;
        }
        let controls = [
            "1 drone / 2-4 bars",
            "W/S depth, A/D sideways, Q/E vertical",
            "Bars move only on their colored rail",
            "Right-drag orbit / wheel zoom / hold C cutaway",
            "Hover arrows or Shift+key to preview",
            "Z undo (also after a crush), R retry vault",
            "Enter next vault; F5 save, F9 load",
        ];
        let outcome = shell.local_menu_with_audio(
            "Odd Matter",
            &controls,
            AudioMenu { music_on: false, sfx_on: settings.sfx_on, has_music: false },
        );
        if outcome.toggle_sfx {
            settings.toggle_sfx();
            sounds.sfx_volume = settings.sfx_level();
            settings.store(&settings_path);
        }
        if outcome.quit || game_client::exit_requested() {
            break;
        }
        if let Some(path) = life.capture_path() {
            life.captured(&path, kit::capture::save_frame(&path).map_err(|e| e.to_string()));
        }
        if life.end_frame(dt) {
            break;
        }
        next_frame().await;
    }
    if let Some(report) = life.report() {
        println!("{report}");
    }
    println!(
        "{}",
        serde_json::json!({"game":"odd-matter","state_hash":format!("{:016x}",sim.state_hash()),"tick":sim.tick,"room":sim.board.room+1,"drone":sim.board.drone,"offsets":sim.board.offsets,"selected":sim.board.selected,"status":format!("{:?}",sim.board.status),"moves":sim.board.moves,"undo":sim.undo_len(),"native_commands":native_commands,"cut_frames":cut_frames,"orbit_pixels":orbit_pixels,"zoom_steps":zoom_steps,"preview_frames":preview_frames,"saves":saves,"loads":loads,"cleared_vaults":cleared_vaults,"crushes":crushes,"undos":undos,"verification_commands":verification_route().len()})
    );
    if verify && sim.board.status != Status::Won {
        panic!("public verification route did not reach all six exits");
    }
    if life.options.audible {
        let status = sounds.status();
        println!("audio: {status:?}");
        status
            .verify_playback(SOUNDS.iter().map(|p| p.variants()).sum(), 0)
            .expect("audio submission verification failed");
    }
}
