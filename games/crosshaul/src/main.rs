#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod docking_audio;
mod platform;
use crosshaul::{Cell, Intent, Sim, TRAVEL};
use docking_audio::DockingArrival;
use macroquad::prelude::*;
use vesper3d::viewer::{
    devkit::{self, flag_value, parse_size, synth, Lifecycle, Settings},
    game_client::{self, GameShell},
    game_input::ClientInput,
    identity::Identity,
    kit::{self, hud, Batch, Look, Materials, Rendered, SoundBank, Template, Tint, View},
};
const IDENTITY: &str = include_str!("../assets/identity.json");
const COLORS: [[f32; 3]; 4] = [[0.15, 0.85, 0.94], [1., 0.57, 0.19], [0.79, 0.47, 1.], [0.48, 0.92, 0.39]];
const SYMBOLS: [&str; 4] = ["A / +", "B / =", "C / X", "D / :"];
const CUES: [&str; 13] = ["a", "b", "c", "d", "x", "y", "z", "swap", "undo", "restart", "next", "orbit", "hint"];
fn window() -> macroquad::conf::Conf {
    platform::attach_console();
    let identity = Identity::parse(IDENTITY).unwrap();
    let mut c = game_client::window_config_with_icon(
        &identity.title,
        game_client::icon_from_rgba(
            include_bytes!("../assets/icon_16.rgba"),
            include_bytes!("../assets/icon_32.rgba"),
            include_bytes!("../assets/icon_64.rgba"),
        ),
    );
    c.miniquad_conf.window_width = 1440;
    c.miniquad_conf.window_height = 900;
    let args: Vec<_> = std::env::args().collect();
    if let Some((w, h)) = flag_value(&args, "--size").and_then(parse_size) {
        c.miniquad_conf.window_width = w as i32;
        c.miniquad_conf.window_height = h as i32;
    }
    c
}
#[derive(Clone, Copy, Default)]
struct Held;
fn at(p: Cell) -> Vec3 {
    vec3((p[0] as f32 - 2.) * 1.65, 1. + p[1] as f32 * 1.65, (p[2] as f32 - 2.) * 1.65)
}
fn color(c: [f32; 3]) -> Color {
    Color::new(c[0], c[1], c[2], 1.)
}
fn box_lines(t: &mut Template, p: Vec3, half: f32, width: f32, c: [f32; 3]) {
    for k in 0..3 {
        for a in [-1., 1.] {
            for b in [-1., 1.] {
                let mut center = p;
                let mut h = Vec3::splat(width);
                h[k] = half;
                center[(k + 1) % 3] += a * half;
                center[(k + 2) % 3] += b * half;
                t.box_(center, h, c, 0.8);
            }
        }
    }
}
fn line(t: &mut Template, a: Vec3, b: Vec3, r: f32, c: [f32; 3]) {
    if a.distance(b) > 0.01 {
        t.rod(a, b, r, r, c, 0.75, 7);
    }
}
fn symbol(t: &mut Template, p: Vec3, i: usize, c: [f32; 3]) {
    // Physical symbols on both Z faces, readable independently of color.
    for z in [-0.604, 0.604] {
        let c0 = p + vec3(0., 0., z);
        let mut stroke = |a: Vec3, b: Vec3| line(t, c0 + a, c0 + b, 0.038, c);
        match i {
            0 => {
                stroke(vec3(-0.22, 0., 0.), vec3(0.22, 0., 0.));
                stroke(vec3(0., -0.22, 0.), vec3(0., 0.22, 0.));
            }
            1 => {
                for y in [-0.12, 0.12] {
                    stroke(vec3(-0.23, y, 0.), vec3(0.23, y, 0.));
                }
            }
            2 => {
                stroke(vec3(-0.19, -0.19, 0.), vec3(0.19, 0.19, 0.));
                stroke(vec3(-0.19, 0.19, 0.), vec3(0.19, -0.19, 0.));
            }
            _ => {
                for y in [-0.15, 0.15] {
                    stroke(vec3(0., y - 0.03, 0.), vec3(0., y + 0.03, 0.));
                }
            }
        }
    }
}
fn cargo_template(i: usize) -> Template {
    let mut t = Template::new();
    let c = COLORS[i];
    t.rounded_box(Vec3::ZERO, Vec3::splat(0.594), 0.09, c.map(|v| v * 0.47), 0.16);
    for z in [-0.594, 0.594] {
        t.box_(vec3(0., 0., z), vec3(0.4, 0.4, 0.008), [0.025, 0.055, 0.075], 0.2);
    }
    box_lines(&mut t, Vec3::ZERO, 0.51, 0.025, c);
    symbol(&mut t, Vec3::ZERO, i, c);
    t.box_(vec3(0., 0.6, 0.), vec3(0.25, 0.014, 0.25), c, 0.9);
    t
}
fn stage() -> Vec<Mesh> {
    let mut t = Template::new();
    t.box_(vec3(0., -0.4, 0.), vec3(4.65, 0.3, 4.65), [0.045, 0.075, 0.10], 0.05);
    t.box_(vec3(0., -0.13, 0.), vec3(4.42, 0.025, 4.42), [0.07, 0.12, 0.16], 0.);
    for n in -2..=2 {
        let f = n as f32 * 1.65;
        t.box_(vec3(f, -0.095, 0.), vec3(0.013, 0.008, 4.1), [0.15, 0.27, 0.33], 0.35);
        t.box_(vec3(0., -0.095, f), vec3(4.1, 0.008, 0.013), [0.15, 0.27, 0.33], 0.35);
    }
    for x in [-4.45, 4.45] {
        for z in [-4.45, 4.45] {
            t.box_(vec3(x, 3.9, z), vec3(0.065, 4.3, 0.065), [0.12, 0.23, 0.28], 0.2);
            for y in [0., 1., 2., 3., 4.] {
                t.box_(vec3(x, 1. + y * 1.65, z), vec3(0.13, 0.025, 0.13), [0.25, 0.49, 0.55], 0.6);
            }
        }
    }
    for h in [-0.03, 8.15] {
        t.box_(vec3(0., h, -4.45), vec3(4.5, 0.035, 0.035), [0.16, 0.37, 0.43], 0.6);
        t.box_(vec3(0., h, 4.45), vec3(4.5, 0.035, 0.035), [0.16, 0.37, 0.43], 0.6);
        t.box_(vec3(-4.45, h, 0.), vec3(0.035, 0.035, 4.5), [0.16, 0.37, 0.43], 0.6);
        t.box_(vec3(4.45, h, 0.), vec3(0.035, 0.035, 4.5), [0.16, 0.37, 0.43], 0.6);
    }
    t.to_meshes()
}
fn view(angle: f32, elevation: f32, radius: f32) -> View {
    let dir = vec3(-angle.sin() * elevation.cos(), -elevation.sin(), -angle.cos() * elevation.cos());
    let right = dir.cross(Vec3::Y).normalize();
    let focus = vec3(0., 3.65, 0.) - right * 2.;
    let eye = focus - dir * radius;
    let yaw = devkit::path::yaw_of(vesper3d::math::V(dir.x, dir.y, dir.z));
    let mut v = View::first_person(eye, yaw, -elevation);
    v.fov = 44f32.to_radians();
    v
}
fn ray_box(origin: Vec3, dir: Vec3, p: Vec3, half: f32) -> Option<f32> {
    let mut lo = 0f32;
    let mut hi = 1000f32;
    for k in 0..3 {
        if dir[k].abs() < 0.00001 {
            if (origin[k] - p[k]).abs() > half {
                return None;
            }
        } else {
            let a = (p[k] - half - origin[k]) / dir[k];
            let b = (p[k] + half - origin[k]) / dir[k];
            lo = lo.max(a.min(b));
            hi = hi.min(a.max(b));
        }
    }
    (lo <= hi).then_some(lo)
}
fn pick(sim: &Sim, v: &View, m: Vec2) -> Option<u8> {
    let t = (v.fov * 0.5).tan();
    let x = (m.x / screen_width() * 2. - 1.) * t * screen_width() / screen_height();
    let y = (1. - m.y / screen_height() * 2.) * t;
    let dir = (v.dir() + v.right() * x + v.up() * y).normalize();
    let obstruction =
        sim.level().baffles.iter().filter_map(|p| ray_box(v.eye, dir, at(*p), 0.792)).fold(f32::INFINITY, f32::min);
    sim.positions
        .iter()
        .enumerate()
        .filter_map(|(i, p)| ray_box(v.eye, dir, at(*p), 0.594).map(|d| (i, d)))
        .filter(|(_, d)| *d < obstruction)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i as u8)
}
fn txt(s: &str, x: f32, y: f32, size: f32, c: Color, u: f32) {
    hud::text_outlined(s, x * u, y * u, size * u, c);
}
fn panel(x: f32, y: f32, w: f32, h: f32, u: f32) {
    hud::panel(x * u, y * u, w * u, h * u, 10. * u, Color::new(0.025, 0.045, 0.065, 0.96));
}
#[macroquad::main(window)]
async fn main() {
    vesper3d::viewer::startup::show().await;
    let args: Vec<_> = std::env::args().collect();
    let id = Identity::parse(IDENTITY).unwrap();
    let mut life = Lifecycle::<Held>::start_or_exit(&args, &CUES);
    let mut sim = Sim::new(life.seed());
    life.load_flag_or_exit(&mut sim);
    let mut input = ClientInput::new();
    let mut shell = GameShell::new();
    let settings_path = devkit::beside_exe("settings.json");
    let settings = Settings::load(&settings_path);
    let presets = [
        synth::Preset::Select,
        synth::Preset::Whoosh,
        synth::Preset::Error,
        synth::Preset::Success,
        synth::Preset::Back,
    ];
    let mut sounds =
        SoundBank::start(life.options.silent(), settings.sfx_level(), settings.music_level(), move || Rendered {
            sfx: presets
                .iter()
                .map(|p| (0..p.variants()).map(|v| synth::wav_bytes(&synth::render(*p, v, 17), synth::RATE)).collect())
                .collect(),
            stems: vec![],
        })
        .await;
    let materials = Materials::load().expect("world materials");
    let statics = stage();
    let cubes: Vec<_> = (0..4).map(cargo_template).collect();
    let mut world = Batch::new();
    let mut look = Look::night();
    look.ambient_sky = [0.42, 0.53, 0.65];
    look.fog_density = 0.003;
    look.fog_color = [0.025, 0.045, 0.075];
    look.key_color = [0.95, 0.86, 0.73];
    look.rim_color = [0.3, 0.7, 0.95];
    let (mut angle, mut elevation, mut radius) = (0.65f32, 0.48f32, 25.5f32);
    let mut last_mouse = Vec2::ZERO;
    let mut notice = String::new();
    let mut hint = String::new();
    let mut instructions = false;
    let mut was_won = false;
    loop {
        input.begin_frame_with_keyboard(
            &mut shell,
            false,
            life.options.unattended() || platform::focused(),
            platform::keyboard(),
        );
        let dt = life.begin_frame(input.frame_seconds());
        sounds.poll().await;
        let mouse = Vec2::from(mouse_position());
        let delta = mouse - last_mouse;
        last_mouse = mouse;
        if shell.accepting_input() {
            if is_mouse_button_down(MouseButton::Right) {
                angle -= delta.x * 0.007;
                elevation = (elevation + delta.y * 0.005).clamp(0.12, 1.18);
            }
            radius = (radius - mouse_wheel().1 * 0.8).clamp(18., 34.);
            if input.pressed(KeyCode::Tab) {
                instructions = !instructions;
            }
        }
        let v = view(angle, elevation, radius);
        let mut intent = Intent::default();
        if let Some(s) = life.script() {
            for (i, cue) in ["a", "b", "c", "d"].iter().enumerate() {
                if s.starts(cue) {
                    intent.select = Some(i as u8);
                }
            }
            for (i, cue) in ["x", "y", "z"].iter().enumerate() {
                if s.starts(cue) {
                    intent.axis = Some(i as u8);
                }
            }
            intent.execute = s.starts("swap");
            intent.undo = s.starts("undo");
            intent.restart = s.starts("restart");
            intent.next = s.starts("next");
            angle += s.value("orbit", 0);
        } else if shell.accepting_input() {
            for (i, key) in [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4].iter().enumerate() {
                if input.pressed(*key) {
                    intent.select = Some(i as u8);
                }
            }
            if is_mouse_button_pressed(MouseButton::Left) && mouse.x > 330. * (screen_height() / 900.) {
                intent.select = pick(&sim, &v, mouse);
            }
            for (i, key) in [KeyCode::X, KeyCode::Y, KeyCode::Z].iter().enumerate() {
                if input.pressed(*key) {
                    intent.axis = Some(i as u8);
                }
            }
            intent.execute = input.pressed(KeyCode::Space);
            intent.undo = input.pressed(KeyCode::Backspace);
            intent.restart = input.pressed(KeyCode::R);
            intent.next = input.pressed(KeyCode::Enter);
            if input.pressed(KeyCode::H) {
                hint = sim
                    .hint()
                    .map(|[a, b, k]| {
                        format!(
                            "Try {} + {}, then {}",
                            (b'A' + a as u8) as char,
                            (b'A' + b as u8) as char,
                            ["X", "Y", "Z"][k]
                        )
                    })
                    .unwrap_or_else(|| "All cargo delivered.".into());
            }
        }
        let mut bits = 0u32;
        if let Some(i) = intent.select {
            bits |= 1 << i;
        }
        if let Some(k) = intent.axis {
            bits |= 1 << (4 + k);
        }
        for (i, on) in [intent.execute, intent.undo, intent.restart, intent.next].iter().enumerate() {
            if *on {
                bits |= 1 << (7 + i);
            }
        }
        life.feed(Held, bits, [0., 0.]);
        let (save, load) = life.save_load_requested(input.pressed(KeyCode::F5), input.pressed(KeyCode::F9));
        if save {
            let n = life.quick_save(&sim, "Crosshaul manifest");
            notice = format!("{}: {}", n.title, n.detail);
        }
        if load {
            let n = life.quick_load(&mut sim);
            notice = format!("{}: {}", n.title, n.detail);
            hint.clear();
        }
        for _ in 0..life.ticks(dt, 1., !shell.paused) {
            let t = life.take_tick();
            let mut command = Intent::default();
            for i in 0..4 {
                if t.pressed(1 << i) {
                    command.select = Some(i);
                }
            }
            for k in 0..3 {
                if t.pressed(1 << (4 + k)) {
                    command.axis = Some(k);
                }
            }
            command.execute = t.pressed(1 << 7);
            command.undo = t.pressed(1 << 8);
            command.restart = t.pressed(1 << 9);
            command.next = t.pressed(1 << 10);
            let old_used = sim.used;
            let old_reject = sim.rejected;
            let old_chamber = sim.chamber;
            let old_pair = sim.pair.clone();
            let docking = DockingArrival::finishing(&sim);
            sim.step(&command);
            if docking.is_some_and(|arrival| arrival.completed(&sim)) {
                sounds.play(0, 0.35);
            }
            if sim.used > old_used {
                sounds.play(1, 0.55);
                hint.clear();
            }
            if sim.rejected > old_reject {
                sounds.play(2, 0.45);
            }
            if sim.pair != old_pair {
                sounds.play(0, 0.4);
            }
            if command.undo || command.restart {
                sounds.play(4, 0.45);
                hint.clear();
            }
            if sim.chamber != old_chamber {
                hint.clear();
                notice.clear();
            }
        }
        if sim.won() && !was_won {
            sounds.play(3, 0.7);
        }
        was_won = sim.won();
        clear_background(Color::new(0.025, 0.043, 0.071, 1.));
        let mut geo = Template::new();
        world.clear();
        // Solid baffles exactly match the collision cells. Hazard stripes communicate obstruction.
        for p in &sim.level().baffles {
            let p = at(*p);
            geo.box_(p, Vec3::splat(0.792), [0.19, 0.25, 0.29], 0.1);
            for x in [-0.48, 0., 0.48] {
                geo.box_(p + vec3(x, 0.8, 0.), vec3(0.1, 0.01, 0.7), [0.91, 0.57, 0.19], 0.55);
            }
        }
        for (i, p) in sim.level().docks.iter().enumerate() {
            let p = at(*p);
            let c = COLORS[i];
            symbol(&mut geo, p - vec3(0., 0.85, 0.), i, c);
            box_lines(&mut geo, p, 0.73, 0.025, c.map(|v| v * 0.65));
            geo.box_(p - vec3(0., 0.69, 0.), vec3(0.66, 0.025, 0.66), c.map(|v| v * 0.35), 0.45);
            for dx in [-0.54, 0.54] {
                geo.box_(p + vec3(dx, 0.71, 0.), vec3(0.05, 0.025, 0.6), c, 0.8);
            }
        }
        for (i, p) in sim.positions.iter().enumerate() {
            let mut pos = at(*p);
            if let Some(m) = &sim.motion {
                let t = 1. - m.remaining as f32 / TRAVEL as f32;
                let t = t * t * (3. - 2. * t);
                pos = at(m.from[i]).lerp(pos, t);
            }
            world.add(&cubes[i], Mat4::from_translation(pos), Tint::NONE);
            let c = COLORS[i];
            let floor = vec3(pos.x, -0.06, pos.z);
            geo.ring(floor, 0.31, 0.36, c, 0.8, 20);
            // Broken plumb lines preserve height cues without hiding the freight.
            let mut y = 0.2;
            while y < pos.y - 0.7 {
                line(
                    &mut geo,
                    vec3(pos.x, y, pos.z),
                    vec3(pos.x, (y + 0.12).min(pos.y - 0.7), pos.z),
                    0.012,
                    c.map(|v| v * 0.5),
                );
                y += 0.42;
            }
            if sim.pair.contains(&i) {
                box_lines(&mut geo, pos, 0.68, 0.035, [0.95, 0.98, 1.]);
            }
        }
        if sim.motion.is_none() {
            if let Some(p) = sim.preview() {
                for side in 0..2 {
                    let i = sim.pair[side];
                    let a = at(sim.positions[i]);
                    let b = at(p.destinations[side]);
                    let c = if p.hits[side].is_some() { [1., 0.23, 0.27] } else { COLORS[i] };
                    box_lines(&mut geo, b, 0.64, 0.035, c);
                    let d = b - a;
                    let length = d.length();
                    if length > 0.01 {
                        let dir = d / length;
                        let cross = if sim.axis == 1 { Vec3::Z } else { Vec3::Y };
                        let wing = dir.cross(cross).normalize() * 0.19;
                        let mut f = 0.15;
                        while f < length {
                            line(&mut geo, a + dir * f, a + dir * (f + 0.17).min(length), 0.025, c);
                            f += 0.32;
                        }
                        let tip = b - dir * 0.85;
                        line(&mut geo, tip, tip - dir * 0.38 + wing, 0.04, c);
                        line(&mut geo, tip, tip - dir * 0.38 - wing, 0.04, c);
                    }
                    if let Some(hit) = p.hits[side] {
                        box_lines(&mut geo, at(hit.at), 0.84, 0.05, [1., 0.22, 0.25]);
                    }
                }
            }
        }
        // Fixed world-axis arrows at the near corner; camera orbit cannot relabel them.
        let axis_base = vec3(-4., 0.14, 4.);
        for (k, c) in [[1., 0.36, 0.32], [0.46, 0.95, 0.48], [0.32, 0.64, 1.]].iter().enumerate() {
            let mut d = Vec3::ZERO;
            d[k] = 1.8;
            line(&mut geo, axis_base, axis_base + d, 0.04, *c);
            geo.ball(axis_base + d, Vec3::splat(0.09), *c, 1., 8, 5);
        }
        world.add(&geo, Mat4::IDENTITY, Tint::NONE);
        set_camera(&v.camera_checked(0.2, 90.));
        materials.set_scene(&look, v.eye, life.time(), 0.4);
        materials.draw_static(&statics);
        gl_use_material(&materials.world);
        world.draw();
        gl_use_default_material();
        set_default_camera();
        let u = screen_height() / 900.;
        let w = screen_width() / u;
        let dim = Color::new(0.53, 0.68, 0.76, 1.);
        let cyan = color(COLORS[0]);
        // Small projected tags identify cargo AND dock even when one is occluded. Numbers select hidden cargo.
        for (i, p) in sim.level().docks.iter().enumerate() {
            if let Some(q) = v.project(at(*p) + vec3(0., -0.95, 0.), screen_width(), screen_height()) {
                hud::text_centered(&format!("{} DOCK", SYMBOLS[i]), q.x, q.y, 13. * u, color(COLORS[i]));
            }
        }
        for (i, p) in sim.positions.iter().enumerate() {
            if let Some(q) = v.project(at(*p) + vec3(0., 0.96, 0.), screen_width(), screen_height()) {
                hud::text_centered(
                    &format!("{}{}", SYMBOLS[i], if sim.positions[i] == sim.level().docks[i] { "  OK" } else { "" }),
                    q.x,
                    q.y,
                    18. * u,
                    color(COLORS[i]),
                );
            }
        }
        for (k, label) in ["X WIDTH", "Y HEIGHT", "Z DEPTH"].iter().enumerate() {
            let mut p = axis_base;
            p[k] += 2.;
            if let Some(q) = v.project(p, screen_width(), screen_height()) {
                hud::text_centered(label, q.x, q.y, 13. * u, WHITE);
            }
        }
        panel(20., 20., 290., 860., u);
        txt("CROSSHAUL", 40., 64., 34., WHITE, u);
        txt("SPATIAL FREIGHT / 08", 41., 89., 13., cyan, u);
        for n in 0..8 {
            draw_rectangle(
                (41. + n as f32 * 31.) * u,
                110. * u,
                23. * u,
                4. * u,
                if n <= sim.chamber { cyan } else { Color::new(0.16, 0.25, 0.3, 1.) },
            );
        }
        txt(&format!("CHAMBER {:02}", sim.chamber + 1), 40., 152., 14., dim, u);
        txt(&sim.level().name, 40., 184., 25., WHITE, u);
        txt(
            &format!("{:02} / {:02}", sim.level().allowance.saturating_sub(sim.used), sim.level().allowance),
            40.,
            237.,
            39.,
            if sim.failed() { RED } else { cyan },
            u,
        );
        txt("EXCHANGES LEFT", 42., 259., 12., dim, u);
        txt(&format!("{} / {} CARGO DOCKED", sim.docked(), sim.positions.len()), 40., 296., 15., WHITE, u);
        txt("01  SELECT TWO CUBES", 40., 343., 14., dim, u);
        for i in 0..sim.positions.len() {
            let y = 360. + i as f32 * 37.;
            let selected = sim.pair.contains(&i);
            draw_rectangle(
                39. * u,
                y * u,
                250. * u,
                30. * u,
                if selected { Color::new(0.16, 0.25, 0.3, 1.) } else { Color::new(0.055, 0.09, 0.12, 1.) },
            );
            txt(&format!("{}  {}", i + 1, SYMBOLS[i]), 49., y + 21., 17., color(COLORS[i]), u);
            let p = sim.positions[i];
            txt(&format!("({},{},{})", p[0], p[1], p[2]), 176., y + 21., 16., dim, u);
            if life.script().is_none()
                && shell.accepting_input()
                && is_mouse_button_pressed(MouseButton::Left)
                && mouse.x / u > 39.
                && mouse.x / u < 289.
                && mouse.y / u > y
                && mouse.y / u < y + 30.
            {
                // A UI click travels through the same next-tick intention accumulator as a world click.
                life.feed(Held, 1 << i, [0., 0.]);
            }
        }
        txt("02  CHOOSE WORLD AXIS", 40., 539., 14., dim, u);
        for (k, label) in ["X", "Y", "Z"].iter().enumerate() {
            let x = 40. + k as f32 * 85.;
            draw_rectangle(
                x * u,
                553. * u,
                76. * u,
                42. * u,
                if sim.axis == k { Color::new(0.12, 0.4, 0.48, 1.) } else { Color::new(0.07, 0.12, 0.16, 1.) },
            );
            txt(label, x + 28., 582., 24., WHITE, u);
        }
        txt(["WIDTH / LEFT-RIGHT", "HEIGHT / UP-DOWN", "DEPTH / FRONT-BACK"][sim.axis], 40., 618., 13., cyan, u);
        let status = if sim.pair.len() < 2 {
            "Choose a pair to preview"
        } else if sim.preview().is_some_and(|p| !p.changes) {
            "Same coordinate: no exchange"
        } else if sim.preview().is_some_and(|p| !p.legal()) {
            "BLOCKED / red route"
        } else {
            "CLEAR / Space to exchange"
        };
        txt(status, 40., 654., 16., if status.starts_with("BLOCKED") { color([1., 0.4, 0.35]) } else { WHITE }, u);
        txt("Backspace  Undo + refund", 40., 704., 15., dim, u);
        txt("R  Restart     H  Hint", 40., 730., 15., dim, u);
        txt("Right drag  Orbit", 40., 772., 15., dim, u);
        txt("Wheel  Zoom / 1-4  Select", 40., 797., 15., dim, u);
        txt("Tab  Rules    F5/F9  Save/load", 40., 845., 13., dim, u);
        txt("TWO CARGOES. ONE COORDINATE.", 345., 42., 15., dim, u);
        txt("Matching docks are frames. Filled cubes are cargo.", 345., 68., 13., dim, u);
        let mut lines = vec![];
        let mut line_text = String::new();
        for word in sim.level().hint.split_whitespace() {
            if line_text.len() + word.len() > 70 {
                lines.push(std::mem::take(&mut line_text));
            }
            if !line_text.is_empty() {
                line_text.push(' ');
            }
            line_text.push_str(word);
        }
        lines.push(line_text);
        for (i, s) in lines.iter().enumerate() {
            txt(s, 345., 825. + i as f32 * 22., 17., dim, u);
        }
        if !hint.is_empty() {
            txt(&hint, 345., 791., 20., cyan, u);
        }
        if !notice.is_empty() {
            txt(&notice, 345., 885., 13., dim, u);
        }
        if sim.rejected > 0 {
            txt("EXCHANGE REJECTED / no move spent", 345., 109., 18., color([1., 0.45, 0.35]), u);
        }
        if sim.won() || sim.failed() {
            let x = (w + 310.) * 0.5 - 230.;
            panel(x, 360., 460., 150., u);
            let title = if sim.failed() {
                "MANIFEST STALLED"
            } else if sim.chamber == 7 {
                "ALL FREIGHT DELIVERED"
            } else {
                "CHAMBER CLEAR"
            };
            txt(title, x + 24., 411., 29., if sim.failed() { color([1., 0.55, 0.35]) } else { cyan }, u);
            txt(
                if sim.failed() {
                    "Backspace: undo and recover a move"
                } else if sim.chamber == 7 {
                    "Eight chambers. Every cargo home."
                } else {
                    "Enter: continue to the next chamber"
                },
                x + 24.,
                448.,
                19.,
                WHITE,
                u,
            );
            txt("R: retry chamber   /   Backspace: undo", x + 24., 482., 15., dim, u);
        }
        if instructions {
            panel(335., 140., (w - 365.).min(850.), 570., u);
            for (i, s) in [
                "HOW TO CROSSHAUL",
                "1. Click two filled cargo cubes, or press their numbers.",
                "2. X exchanges width, Y height, Z depth.",
                "3. Read BOTH destination frames and dashed travel paths.",
                "4. Space slides both cubes together. Red paths cannot move.",
                "Only one coordinate changes. No complete-position swaps.",
                "Cargo cannot pass through baffles, cargo, or its partner.",
                "Every legal exchange costs one move. Rejections are free.",
                "Deliver every cube to its matching color AND symbol dock.",
                "Docked cubes remain selectable. Borrow their coordinates!",
                "No timer. Backspace undoes and refunds; R restarts.",
                "Right-drag orbits. Wheel zooms. Axes never rotate.",
                "F5 saves the manifest and undo history; F9 restores it.",
                "Tab closes these instructions.",
            ]
            .iter()
            .enumerate()
            {
                txt(
                    s,
                    357.,
                    184. + i as f32 * 36.,
                    if i == 0 { 27. } else { 18. },
                    if i == 0 { cyan } else { WHITE },
                    u,
                );
            }
        }
        let controls: Vec<&str> = id.controls.split(", ").collect();
        if shell.local_menu(&id.title, &controls) {
            break;
        }
        if let Some(p) = life.capture_path() {
            life.captured(&p, kit::capture::save_frame(&p).map_err(|e| e.to_string()));
        }
        if life.end_frame(dt) {
            break;
        }
        next_frame().await;
    }
    println!(
        "crosshaul: chamber={} moves={} docked={} won={} rejected={} hash={}",
        sim.chamber + 1,
        sim.used,
        sim.docked(),
        sim.won(),
        sim.rejected,
        vesper3d::viewer::devkit::Simulation::state_hash(&sim)
    );
    if let Some(report) = life.report() {
        println!("{report}");
    }
}
