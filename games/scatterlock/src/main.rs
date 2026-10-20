#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod platform;
use macroquad::prelude::*;
use scatterlock::{chamber, controls::*, Point, Pulse, Sim};
use vesper3d::viewer::{
    devkit::{flag_value, parse_size, synth, Lifecycle},
    game_client::{self, GameShell},
    game_input::{self, ClientInput},
    identity::Identity,
    kit::{self, Batch, Look, Materials, Rendered, SoundBank, Template, Tint, View},
};
fn identity() -> Identity {
    Identity::parse(include_str!("../assets/identity.json")).expect("identity")
}
fn window() -> macroquad::conf::Conf {
    platform::attach_console();
    let mut c = game_client::window_config_with_icon(
        "Scatterlock",
        game_client::icon_from_rgba(
            include_bytes!("../assets/icon_16.rgba"),
            include_bytes!("../assets/icon_32.rgba"),
            include_bytes!("../assets/icon_64.rgba"),
        ),
    );
    let a: Vec<_> = std::env::args().collect();
    if let Some((w, h)) = flag_value(&a, "--size").and_then(parse_size) {
        c.miniquad_conf.window_width = w as i32;
        c.miniquad_conf.window_height = h as i32;
    }
    c
}
const CUES: [&str; 12] = ["tab", "spread", "gather", "pulse", "undo", "restart", "a", "b", "c", "d", "e", "orbit"];
const COLORS: [[f32; 3]; 3] = [[1., 0.65, 0.23], [0.20, 0.86, 0.96], [0.76, 0.53, 1.]];
fn v(p: Point) -> Vec3 {
    vec3(p[0] as f32 / 1024., p[1] as f32 / 1024., p[2] as f32 / 1024.)
}
fn color(c: [f32; 3]) -> Color {
    Color::new(c[0], c[1], c[2], 1.)
}
fn rod(t: &mut Template, a: Vec3, b: Vec3, r: f32, c: [f32; 3], glow: f32) {
    if a.distance(b) > 0.001 {
        t.rod(a, b, r, r, c, glow, 6);
    }
}
fn cage(t: &mut Template, p: Vec3, h: Vec3, r: f32, c: [f32; 3], g: f32) {
    for axis in 0..3 {
        for a in [-1., 1.] {
            for b in [-1., 1.] {
                let mut x = p;
                let mut y = p;
                x[axis] -= h[axis];
                y[axis] += h[axis];
                x[(axis + 1) % 3] += h[(axis + 1) % 3] * a;
                y[(axis + 1) % 3] += h[(axis + 1) % 3] * a;
                x[(axis + 2) % 3] += h[(axis + 2) % 3] * b;
                y[(axis + 2) % 3] += h[(axis + 2) % 3] * b;
                rod(t, x, y, r, c, g);
            }
        }
    }
}
fn stage(s: &Sim) -> Template {
    let mut t = Template::new();
    t.box_(vec3(0., -0.35, 0.), vec3(6.35, 0.35, 6.35), [0.045, 0.07, 0.105], 0.05);
    for i in -6..=6 {
        let x = i as f32;
        rod(&mut t, vec3(x, 0.015, -6.), vec3(x, 0.015, 6.), 0.009, [0.12, 0.22, 0.28], 0.5);
        rod(&mut t, vec3(-6., 0.015, x), vec3(6., 0.015, x), 0.009, [0.12, 0.22, 0.28], 0.5);
    }
    cage(&mut t, vec3(0., 4., 0.), vec3(6., 4., 6.), 0.025, [0.15, 0.35, 0.43], 0.5);
    for x in [-6., 6.] {
        for z in [-6., 6.] {
            for y in 0..9 {
                t.box_(vec3(x, y as f32, z), vec3(0.065, 0.018, 0.065), [0.30, 0.68, 0.77], 0.8);
            }
        }
    }
    for b in chamber(s.state.level).baffles {
        let p = v(b.center);
        let h = v(b.half);
        t.box_(p, h, [0.19, 0.22, 0.26], 0.15);
        cage(&mut t, p, h + Vec3::splat(0.015), 0.025, [0.95, 0.39, 0.20], 0.6);
    }
    t
}
fn text(s: &str, x: f32, y: f32, size: f32, c: Color) {
    draw_text(s, x, y, size, c);
}
fn panel(x: f32, y: f32, w: f32, h: f32) {
    draw_rectangle(x, y, w, h, Color::new(0.035, 0.065, 0.095, 0.96));
    draw_rectangle(x, y, 3., h, Color::new(0.18, 0.68, 0.74, 1.));
}
fn wrapped(s: &str, x: f32, y: f32, width: f32, size: f32, c: Color) -> f32 {
    let mut line = String::new();
    let mut y = y;
    for word in s.split_whitespace() {
        let candidate = format!("{line}{word} ");
        if measure_text(&candidate, None, size as u16, 1.).width > width && !line.is_empty() {
            text(&line, x, y, size, c);
            y += size * 1.35;
            line.clear();
        }
        line.push_str(word);
        line.push(' ');
    }
    text(&line, x, y, size, c);
    y + size * 1.35
}
#[macroquad::main(window)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let id = identity();
    let mut life = Lifecycle::<usize>::start_or_exit(&args, &CUES);
    let mut controls = Controls::default();
    let unattended = life.options.unattended();
    let mut sim = Sim::new(life.seed());
    life.load_flag_or_exit(&mut sim);
    let materials = Materials::load().expect("materials");
    let mut look = Look::night();
    look.ambient_sky = [0.5, 0.6, 0.72];
    look.fog_density = 0.003;
    look.fog_color = [0.025, 0.045, 0.075];
    let presets = [synth::Preset::Whoosh, synth::Preset::Chime, synth::Preset::Error, synth::Preset::Select];
    let mut sounds = SoundBank::start(life.options.silent(), 0.6, 0., move || Rendered {
        sfx: presets
            .iter()
            .map(|p| (0..p.variants()).map(|i| synth::wav_bytes(&synth::render(*p, i, 37), synth::RATE)).collect())
            .collect(),
        stems: vec![],
    })
    .await;
    let (mut shell, mut input) = (GameShell::new(), ClientInput::new());
    let mut batch = Batch::new();
    let mut statics = stage(&sim).to_meshes();
    let mut built_level = sim.state.level;
    // Look across the formation's diagonal so cargo and its separate paths stay readable.
    let (mut yaw, mut pitch, mut radius) = (-0.72f32, 0.48f32, 18.5f32);
    let mut opening = if unattended { 0. } else { 1.15f32 };
    let mut previous = sim.state.config.positions.clone();
    let mut anim = 1f32;
    let mut notice = String::new();
    let mut notice_time = 0f32;
    loop {
        input.begin_frame_with_keyboard(&mut shell, false, unattended || platform::focused(), platform::keyboard());
        let dt = life.begin_frame(input.frame_seconds());
        opening = (opening - dt).max(0.);
        sounds.poll().await;
        let time = life.time();
        let sw = screen_width();
        let sh = screen_height();
        let ui = (sh / 800.).min(sw / 1200.).clamp(0.65, 2.);
        let rail = 270. * ui;
        if shell.accepting_input() {
            if is_mouse_button_down(MouseButton::Right) {
                let d = game_input::mouse_pixels();
                yaw -= d[0] * 0.006;
                pitch = (pitch + d[1] * 0.006).clamp(0.12, 1.25);
            }
            radius = (radius - mouse_wheel().1 * 1.1).clamp(15., 32.);
        }
        let target = vec3(-3.2, 3.6, 0.8);
        let eye = target + vec3(yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos()) * radius;
        let direction = (target - eye).normalize();
        let mut view = View::first_person(eye, direction.x.atan2(-direction.z), direction.y.asin());
        view.fov = 46f32.to_radians();
        let mut beacon = None;
        let mut edges = 0;
        if let Some(script) = life.script() {
            for (name, bit) in [
                ("tab", TAB),
                ("spread", SPREAD),
                ("gather", GATHER),
                ("pulse", EXEC),
                ("undo", UNDO),
                ("restart", RESET),
            ] {
                if script.starts(name) {
                    edges |= bit;
                }
            }
            for (i, name) in ["a", "b", "c", "d", "e"].iter().enumerate() {
                if script.starts(name) {
                    beacon = Some(i);
                }
            }
            yaw += script.value("orbit", 0);
        } else if shell.accepting_input() && opening == 0. {
            for (key, bit) in [
                (KeyCode::Tab, TAB),
                (KeyCode::E, SPREAD),
                (KeyCode::Q, GATHER),
                (KeyCode::Space, EXEC),
                (KeyCode::Z, UNDO),
                (KeyCode::R, RESET),
                (KeyCode::Enter, EXEC),
            ] {
                if input.pressed(key) {
                    edges |= bit;
                }
            }
            for (i, key) in
                [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4, KeyCode::Key5].iter().enumerate()
            {
                if input.pressed(*key) {
                    beacon = Some(i);
                }
            }
            if is_mouse_button_pressed(MouseButton::Left) {
                let m = Vec2::from(mouse_position());
                for (i, p) in chamber(sim.state.level).beacons.iter().enumerate() {
                    let on_world =
                        view.project(v(*p) + Vec3::Y * 0.75, sw, sh).is_some_and(|p| p.distance(m) < 26. * ui);
                    let y = (306. + i as f32 * 34.) * ui;
                    let on_panel = m.x >= 28. * ui && m.x <= 248. * ui && m.y > y - 23. * ui && m.y < y + 6. * ui;
                    if on_world || on_panel {
                        beacon = Some(i);
                    }
                }
            }
        }
        if anim < 1. {
            edges &= !EXEC;
        }
        controls.feed(&mut life, edges, beacon);
        let (save, load) = life.save_load_requested(input.pressed(KeyCode::F5), input.pressed(KeyCode::F9));
        if save {
            let n = life.quick_save(&sim, "Scatterlock cargo configuration");
            notice = format!("{}  {}", n.title, n.detail);
            notice_time = 4.;
        }
        if load {
            let n = life.quick_load(&mut sim);
            notice = format!("{}  {}", n.title, n.detail);
            notice_time = 4.;
            anim = 1.;
        }
        for _ in 0..life.ticks(dt, 1., !shell.paused) {
            let t = life.take_tick();
            let was = sim.state.config.clone();
            let old_level = sim.state.level;
            let was_rejected = sim.state.rejected;
            let command = intent(t);
            sim.step(&command);
            if sim.state.config.positions != was.positions
                && old_level == sim.state.level
                && !t.pressed(RESET)
                && !t.pressed(UNDO)
            {
                previous = was.positions;
                anim = 0.;
                sounds.play(if sim.state.config.docked != was.docked { 1 } else { 0 }, 0.7);
            } else if was != sim.state.config {
                anim = 1.;
            }
            if sim.state.rejected && (!was_rejected || t.pressed(EXEC)) {
                sounds.play(2, 0.6);
            }
            if command.beacon.is_some() || command.cycle {
                sounds.play(3, 0.4);
            }
        }
        if built_level != sim.state.level {
            statics = stage(&sim).to_meshes();
            built_level = sim.state.level;
            anim = 1.;
        }
        if !shell.paused {
            anim = (anim + dt / 0.55).min(1.);
            notice_time -= dt;
        }
        let c = chamber(sim.state.level);
        let preview = sim.preview();
        let cfg = &sim.state.config;
        let blocked = preview.obstruction.is_some();
        batch.clear();
        let mut objects = Template::new();
        for (i, socket) in c.sockets.iter().enumerate() {
            let pos = v(*socket);
            let lit = preview.deliveries[i] && !blocked;
            let col = if lit { [0.6, 1., 0.7] } else { COLORS[i] };
            let thick = if cfg.docked[i] { 0.038 } else { 0.022 };
            cage(&mut objects, pos, Vec3::splat(0.47), thick, col, if lit { 1. } else { 0.6 });
            rod(&mut objects, vec3(pos.x, 0.05, pos.z), pos - Vec3::Y * 0.55, 0.009, [0.18, 0.28, 0.34], 0.3);
        }
        for (i, p) in cfg.positions.iter().enumerate() {
            let end = v(*p);
            let at = if anim < 1. && i < previous.len() {
                v(previous[i]).lerp(end, anim * anim * (3. - 2. * anim))
            } else {
                end
            };
            let col = COLORS[i];
            objects.rounded_box(at, Vec3::splat(0.375), 0.065, col, 0.25);
            cage(
                &mut objects,
                at,
                Vec3::splat(0.38),
                0.018,
                if cfg.docked[i] { [0.8, 1., 0.85] } else { [0.95, 0.96, 1.] },
                0.8,
            );
            // A face stripe and dark inset retain cargo identity under changing light.
            objects.box_(at + vec3(0., 0., 0.378), vec3(0.21, 0.12, 0.006), [0.05, 0.09, 0.13], 0.2);
            if !cfg.docked[i] && !sim.won() && !sim.lost() && anim >= 1. {
                let dest = v(preview.destinations[i]);
                let pc = if blocked { [0.9, 0.35, 0.3] } else { col };
                cage(&mut objects, dest, Vec3::splat(0.375), 0.012, pc, 0.8);
                let distance = at.distance(dest);
                let count = (distance / 0.22).ceil().clamp(1., 80.) as usize;
                for j in 0..count {
                    if j % 2 == 0 {
                        let a = j as f32 / count as f32;
                        let b = ((j + 1) as f32 / count as f32).min(1.);
                        rod(&mut objects, at.lerp(dest, a), at.lerp(dest, b), 0.028, pc, 0.9);
                    }
                }
            }
        }
        for (i, p) in c.beacons.iter().enumerate() {
            let pos = v(*p);
            let chosen = i == sim.state.selected;
            let col = if chosen { [0.75, 1., 0.65] } else { [0.3, 0.52, 0.62] };
            objects.ball(pos, Vec3::splat(if chosen { 0.17 } else { 0.12 }), col, 1., 10, 6);
            objects.ring(pos, 0.29, if chosen { 0.36 } else { 0.32 }, col, 0.8, 28);
            if chosen {
                objects.ring(pos + Vec3::Y * 0.08, 0.40, 0.425, col, 0.7, 28);
            }
        }
        if let Some(o) = &preview.obstruction {
            let p = v(cfg.positions[o.cube]).lerp(v(preview.destinations[o.cube]), o.at);
            objects.ball(p, Vec3::splat(0.13), [1., 0.16, 0.14], 1., 10, 6);
            cage(&mut objects, p, Vec3::splat(0.45), 0.024, [1., 0.2, 0.18], 1.);
        }
        batch.add(&objects, Mat4::IDENTITY, Tint::NONE);
        clear_background(Color::new(0.025, 0.045, 0.075, 1.));
        set_camera(&view.camera_checked(0.1, 150.));
        materials.set_scene(&look, eye, time, 0.5);
        materials.draw_static(&statics);
        gl_use_material(&materials.world);
        batch.draw();
        gl_use_default_material();
        set_default_camera();
        for (i, p) in c.beacons.iter().enumerate() {
            if let Some(pos) = view.project(v(*p) + Vec3::Y * 0.75, sw, sh) {
                let col = if i == sim.state.selected {
                    Color::new(0.78, 1., 0.65, 1.)
                } else {
                    Color::new(0.55, 0.74, 0.8, 1.)
                };
                if let Some(anchor) = view.project(v(*p), sw, sh) {
                    draw_line(pos.x, pos.y, anchor.x, anchor.y, ui, col);
                }
                draw_circle(pos.x, pos.y - 6. * ui, 13. * ui, Color::new(0.04, 0.09, 0.12, 1.));
                draw_circle_lines(pos.x, pos.y - 6. * ui, 13. * ui, 1.5 * ui, col);
                text(&format!("{}", (b'A' + i as u8) as char), pos.x - 6. * ui, pos.y, 19. * ui, col);
            }
        }
        for (i, p) in cfg.positions.iter().enumerate() {
            if let Some(pos) = view.project(v(*p) + Vec3::Y * 0.65, sw, sh) {
                text(
                    &format!("{}{}", i + 1, if cfg.docked[i] { " LOCKED" } else { "" }),
                    pos.x - 4. * ui,
                    pos.y,
                    15. * ui,
                    color(COLORS[i]),
                );
            }
        }
        let ink = Color::new(0.9, 0.94, 0.97, 1.);
        for (i, p) in c.sockets.iter().enumerate() {
            if !cfg.docked[i] {
                if let Some(pos) = view.project(v(*p) - Vec3::Y * 0.78, sw, sh) {
                    text(&format!("{} BERTH", i + 1), pos.x - 22. * ui, pos.y, 13. * ui, color(COLORS[i]));
                }
            }
        }
        let dim = Color::new(0.50, 0.65, 0.72, 1.);
        let mint = Color::new(0.70, 0.96, 0.68, 1.);
        panel(18. * ui, 18. * ui, rail - 18. * ui, sh - 36. * ui);
        text("S C A T T E R L O C K", 32. * ui, 51. * ui, 22. * ui, ink);
        text("SPATIAL CARGO / NO GRAVITY", 32. * ui, 76. * ui, 12. * ui, dim);
        for n in 0..6 {
            draw_rectangle(
                (32. + n as f32 * 34.) * ui,
                98. * ui,
                25. * ui,
                4. * ui,
                if n <= sim.state.level { mint } else { Color::new(0.17, 0.25, 0.3, 1.) },
            );
        }
        text(&format!("CHAMBER  0{} / 06", sim.state.level + 1), 32. * ui, 137. * ui, 16. * ui, dim);
        wrapped(c.title, 32. * ui, 168. * ui, 212. * ui, 26. * ui, ink);
        text("PULSES REMAINING", 32. * ui, 228. * ui, 13. * ui, dim);
        for n in 0..c.allowance {
            draw_rectangle(
                (32. + n as f32 * 35.) * ui,
                241. * ui,
                27. * ui,
                9. * ui,
                if n < cfg.remaining { mint } else { Color::new(0.2, 0.28, 0.33, 1.) },
            );
        }
        text("ORIGIN   /   TAB TO CYCLE", 32. * ui, 279. * ui, 13. * ui, dim);
        for (i, _) in c.beacons.iter().enumerate() {
            let y = (306. + i as f32 * 34.) * ui;
            let chosen = i == sim.state.selected;
            if chosen {
                draw_rectangle(28. * ui, y - 23. * ui, 220. * ui, 30. * ui, Color::new(0.12, 0.23, 0.25, 1.));
            }
            text(
                &format!("{}    BEACON {}", i + 1, (b'A' + i as u8) as char),
                38. * ui,
                y,
                18. * ui,
                if chosen { mint } else { dim },
            );
        }
        let mut y = (322. + c.beacons.len() as f32 * 34.) * ui;
        text(
            if sim.state.mode == Pulse::Spread { "E  SPREAD  x2" } else { "Q  GATHER  /2" },
            32. * ui,
            y,
            25. * ui,
            mint,
        );
        y += 25. * ui;
        text("Q / E change preview", 32. * ui, y, 15. * ui, dim);
        y += 34. * ui;
        let status = if sim.won() {
            if sim.state.level == 5 {
                "ALL SIX CHAMBERS SECURED"
            } else {
                "CHAMBER SECURED"
            }
        } else if sim.lost() {
            "ALLOWANCE EXHAUSTED"
        } else if blocked {
            "PATH BLOCKED"
        } else {
            "PATHS CLEAR"
        };
        y = wrapped(
            status,
            32. * ui,
            y,
            210. * ui,
            19. * ui,
            if sim.lost() || blocked { Color::new(1., 0.49, 0.36, 1.) } else { mint },
        );
        let detail = if sim.won() {
            if sim.state.level == 5 {
                "Constellation complete. R to replay this chamber; Z to revisit the last pulse.".to_string()
            } else {
                "SPACE  Enter the next chamber".to_string()
            }
        } else if sim.lost() {
            "Z restores your last pulse and allowance. R restarts this chamber.".to_string()
        } else if let Some(o) = &preview.obstruction {
            format!("Cargo {} hits {}. Rejected pulses cost nothing.", o.cube + 1, o.reason)
        } else {
            format!(
                "SPACE  Execute pulse\n{} of {} cargo secured",
                cfg.docked.iter().filter(|x| **x).count(),
                cfg.docked.len()
            )
        };
        wrapped(&detail, 32. * ui, y + 7. * ui, 208. * ui, 16. * ui, dim);
        text("Z undo    R retry    Esc pause", 32. * ui, sh - 91. * ui, 14. * ui, ink);
        text("Right drag orbit / wheel zoom", 32. * ui, sh - 68. * ui, 14. * ui, dim);
        text("F5 save  /  F9 load", 32. * ui, sh - 45. * ui, 14. * ui, dim);
        let hx = rail + 30. * ui;
        panel(hx, sh - 95. * ui, sw - hx - 25. * ui, 70. * ui);
        wrapped(c.hint, hx + 17. * ui, sh - 68. * ui, sw - hx - 55. * ui, 18. * ui, ink);
        text("FORMATION CONTROL", rail + 30. * ui, 45. * ui, 15. * ui, dim);
        text(
            "Dock every cube in its matching wire berth. Dotted paths preview the pulse.",
            rail + 30. * ui,
            69. * ui,
            16. * ui,
            dim,
        );
        text(
            "E doubles distance / Q halves distance. Cube size never changes.",
            rail + 30. * ui,
            92. * ui,
            15. * ui,
            dim,
        );
        if opening > 0. {
            clear_background(Color::new(0.025, 0.045, 0.075, 1.));
            let title = "BlueEngine Games";
            let size = 38. * ui;
            let width = measure_text(title, None, size as u16, 1.).width;
            text(title, (sw - width) * 0.5, sh * 0.5, size, ink);
        }
        if notice_time > 0. {
            panel(rail + 30. * ui, 86. * ui, sw - rail - 55. * ui, 48. * ui);
            text(&notice, rail + 44. * ui, 116. * ui, 17. * ui, ink);
        }
        if sim.state.rejected && !sim.lost() {
            text(
                "PULSE REJECTED - configuration preserved",
                rail + 30. * ui,
                sh - 115. * ui,
                18. * ui,
                Color::new(1., 0.48, 0.36, 1.),
            );
        }
        let controls: Vec<_> = id.controls.split(", ").collect();
        if shell.local_menu("Scatterlock", &controls) {
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
    if let Some(r) = life.report() {
        println!("{r}");
    }
    println!(
        "scatterlock: chamber={} remaining={} docked={} hash={}",
        sim.state.level + 1,
        sim.state.config.remaining,
        sim.state.config.docked.iter().filter(|x| **x).count(),
        vesper3d::viewer::devkit::Simulation::state_hash(&sim)
    );
}
