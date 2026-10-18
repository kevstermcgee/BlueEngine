//! Native presentation. The only authority is `Sim::step`; all devices and scripts submit Intent.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod platform;
use macroquad::prelude::*;
use pinstride::*;
use vesper3d::viewer::{
    devkit::{beside_exe, flag_value, has_flag, parse_size, synth, FpsCamera, Lifecycle, MouseLook, Settings},
    game_client::{self, AudioMenu, GameShell},
    game_input::{mouse_pixels, ClientInput},
    identity::Identity,
    kit::{self, Batch, Look, Materials, Rendered, SoundBank, Template, Tint, View},
};
const BLUE: [f32; 3] = [0.15, 0.64, 1.];
const ORANGE: [f32; 3] = [1., 0.43, 0.15];
const MINT: [f32; 3] = [0.37, 0.95, 0.77];
const SOUNDS: [synth::Preset; 5] =
    [synth::Preset::Select, synth::Preset::Error, synth::Preset::Land, synth::Preset::Success, synth::Preset::Back];
fn identity() -> Identity {
    Identity::parse(include_str!("../assets/identity.json")).expect("invalid game identity")
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
    let args: Vec<_> = std::env::args().collect();
    let (w, h) = flag_value(&args, "--size").and_then(parse_size).unwrap_or((1440, 900));
    c.miniquad_conf.window_width = w as i32;
    c.miniquad_conf.window_height = h as i32;
    c
}
#[derive(Clone, Copy, Default)]
struct Held;
const CUES: [&str; 10] = ["one", "two", "three", "four", "commit", "undo", "restart", "next", "orbit", "hint"];
const COMMIT: u32 = 1;
const UNDO: u32 = 2;
const RESTART: u32 = 4;
const NEXT: u32 = 8;
const SELECT_SHIFT: u32 = 4;
fn v(a: [f32; 3]) -> Vec3 {
    vec3(a[0], a[1], a[2])
}
fn pw(a: Point) -> Vec3 {
    v(world(a))
}
fn rgb(c: [f32; 3]) -> Color {
    Color::new(c[0], c[1], c[2], 1.)
}
fn tip(t: &mut Template, at: Vec3, which: usize, scale: f32, glow: f32) {
    if which == 0 {
        t.ball(at, Vec3::splat(scale), BLUE, glow, 16, 10);
    } else {
        t.ball(at, Vec3::splat(scale), ORANGE, glow, 4, 2);
    }
}
fn ring_at(t: &mut Template, at: Vec3, r: f32, color: [f32; 3], normal: Vec3) {
    let mut ring = Template::new();
    ring.ring(Vec3::ZERO, r - 0.018, r + 0.018, color, 0.8, 40);
    t.append(&ring.transformed(Mat4::from_rotation_translation(Quat::from_rotation_arc(Vec3::Y, normal), at)));
}
fn chamber_mesh(c: &Chamber) -> Vec<Mesh> {
    let mut t = Template::new();
    let min = v(c.bounds.min);
    let max = v(c.bounds.max);
    let mid = (min + max) * 0.5;
    t.box_(
        vec3(mid.x, min.y - 0.32, mid.z),
        vec3((max.x - min.x) * 0.5, 0.22, (max.z - min.z) * 0.5),
        [0.055, 0.085, 0.115],
        0.05,
    );
    t.box_(
        vec3(mid.x, min.y - 0.62, mid.z),
        vec3((max.x - min.x) * 0.5 + 0.12, 0.1, (max.z - min.z) * 0.5 + 0.12),
        [0.025, 0.04, 0.06],
        0.,
    );
    let grid_y = min.y - 0.088;
    for x in -4..=4 {
        let x = x as f32;
        if x >= min.x && x <= max.x {
            t.rod(vec3(x, grid_y, min.z), vec3(x, grid_y, max.z), 0.008, 0.008, [0.12, 0.2, 0.25], 0.35, 5);
        }
    }
    for z in -4..=4 {
        let z = z as f32;
        if z >= min.z && z <= max.z {
            t.rod(vec3(min.x, grid_y, z), vec3(max.x, grid_y, z), 0.008, 0.008, [0.12, 0.2, 0.25], 0.35, 5);
        }
    }
    // The thin cage shows the actual playable volume, including its ceiling.
    for axis in 0..3 {
        for a in 0..2 {
            for b in 0..2 {
                let mut p = min;
                let axes: Vec<_> = (0..3).filter(|i| *i != axis).collect();
                p[axes[0]] = if a == 0 { min[axes[0]] } else { max[axes[0]] };
                p[axes[1]] = if b == 0 { min[axes[1]] } else { max[axes[1]] };
                let mut q = p;
                q[axis] = max[axis];
                t.rod(p, q, 0.013, 0.013, [0.17, 0.32, 0.37], 0.55, 6);
            }
        }
    }
    for b in &c.blocks {
        let lo = v(b.min);
        let hi = v(b.max);
        // Narrow amber edge inlays expose thickness and aperture shape from any orbit.
        for axis in 0..3 {
            for a in 0..2 {
                for b in 0..2 {
                    let axes: Vec<_> = (0..3).filter(|j| *j != axis).collect();
                    let mut p = lo;
                    p[axes[0]] = if a == 0 { lo[axes[0]] } else { hi[axes[0]] };
                    p[axes[1]] = if b == 0 { lo[axes[1]] } else { hi[axes[1]] };
                    let mut q = p;
                    q[axis] = hi[axis];
                    // Lift exterior wires above the surface to avoid coplanar seams.
                    t.rod(p, q, 0.016, 0.016, [0.58, 0.46, 0.27], 0.45, 5);
                }
            }
        }
    }
    // Socket frames are hollow: blue circular cradle, orange faceted cradle.
    let d = (pw(c.sockets[1]) - pw(c.sockets[0])).normalize();
    for (i, p) in c.sockets.iter().enumerate() {
        let at = pw(*p);
        let color = if i == 0 { BLUE } else { ORANGE };
        if i == 0 {
            ring_at(&mut t, at, 0.34, color, d);
            ring_at(&mut t, at, 0.34, color, if d.y.abs() < 0.5 { Vec3::Y } else { Vec3::Z });
        } else {
            let right = if d.y.abs() < 0.5 { d.cross(Vec3::Y).normalize() } else { Vec3::X };
            let up = d.cross(right).normalize();
            let points = [at + right * 0.37, at + up * 0.37, at - right * 0.37, at - up * 0.37];
            for j in 0..4 {
                t.rod(points[j], points[(j + 1) % 4], 0.023, 0.023, color, 0.8, 6);
            }
        }
        let floor = vec3(at.x, min.y - 0.065, at.z);
        ring_at(&mut t, floor, 0.23, color, Vec3::Y);
        // Dotted vertical locator; these are guides, never supporting surfaces.
        let n = ((at.y - floor.y) / 0.3) as usize;
        for k in 0..n {
            let p = floor + Vec3::Y * (k as f32 * 0.3 + 0.1);
            t.rod(p, p + Vec3::Y * 0.07, 0.012, 0.012, color, 0.4, 5);
        }
    }
    t.to_meshes()
}
fn endpoints(s: &Sim) -> [Vec3; 2] {
    let p = s.state.pose;
    let mut out = p.tips.map(pw);
    if let Some(a) = s.state.swing {
        let f = a.elapsed as f32 / SWING_TICKS as f32;
        let ease = f * f * (3. - 2. * f);
        let angle = ease * std::f32::consts::FRAC_PI_2;
        let pin = pw(a.from.tips[a.from.pin]);
        let from = pw(a.from.tips[1 - a.from.pin]) - pin;
        let to = v(world(a.from.directions()[a.choice]));
        out[1 - a.from.pin] = pin + from * angle.cos() + to * angle.sin();
    }
    out
}
fn draw_actor(s: &Sim, time: f32, t: &mut Template) {
    let p = endpoints(s);
    t.rod(p[0], p[1], 0.075, 0.075, [0.83, 0.89, 0.90], 0.25, 16);
    let d = (p[1] - p[0]).normalize();
    for (i, at) in p.iter().enumerate() {
        tip(t, *at, i, RADIUS, 0.8);
        let a = *at + d * if i == 0 { 0.25 } else { -0.25 };
        t.rod(a - d * 0.035, a + d * 0.035, 0.105, 0.105, if i == 0 { BLUE } else { ORANGE }, 0.8, 12);
    }
    let pin = s.state.pose.pin;
    let at = p[pin];
    let color = if pin == 0 { BLUE } else { ORANGE };
    let pulse = 0.29 + 0.025 * (time * 4.).sin();
    ring_at(t, at, pulse, color, Vec3::Y);
    ring_at(t, at, pulse, color, Vec3::Z);
}
fn preview(s: &Sim, t: &mut Template) -> [Option<Obstruction>; 4] {
    let p = s.state.pose;
    let hits = std::array::from_fn(|i| obstruction(s.room(), p, i));
    if s.won() || s.state.swing.is_some() {
        return hits;
    }
    let pin = pw(p.tips[p.pin]);
    let from = pw(p.tips[1 - p.pin]) - pin;
    for (i, hit) in hits.iter().enumerate() {
        let end = v(world(p.directions()[i]));
        let dest = pin + end;
        let selected = s.state.selected == i;
        let color = if hit.is_some() { [0.9, 0.29, 0.31] } else { MINT };
        let n = if selected { 32 } else { 12 };
        for k in 0..n {
            if !selected && k % 2 == 1 {
                continue;
            }
            let a = k as f32 / n as f32 * std::f32::consts::FRAC_PI_2;
            let b = (k as f32 + 0.60) / n as f32 * std::f32::consts::FRAC_PI_2;
            t.rod(
                pin + from * a.cos() + end * a.sin(),
                pin + from * b.cos() + end * b.sin(),
                if selected { 0.025 } else { 0.011 },
                if selected { 0.025 } else { 0.011 },
                color,
                0.8,
                5,
            );
        }
        // Destination ghosts preserve the identity of the arriving tip.
        tip(t, dest, 1 - p.pin, if selected { 0.135 } else { 0.09 }, 0.65);
        if selected {
            t.rod(pin, dest, 0.026, 0.026, color, 0.6, 8);
            ring_at(t, dest, 0.24, color, end.normalize());
            if let Some(h) = hit {
                let at = v(h.at);
                t.ball(at, Vec3::splat(0.11), [1., 0.2, 0.25], 1., 10, 6);
                ring_at(t, at, 0.22, [1., 0.3, 0.3], Vec3::Y);
            }
        }
    }
    hits
}
struct Fonts {
    regular: Font,
    bold: Font,
}
fn text(font: &Font, s: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text_ex(s, x, y, TextParams { font: Some(font), font_size: size.round() as u16, color, ..Default::default() });
}
fn wrapped(font: &Font, s: &str, x: f32, mut y: f32, width: f32, size: f32, color: Color) -> f32 {
    let mut line = String::new();
    for word in s.split_whitespace() {
        let test = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
        if measure_text(&test, Some(font), size as u16, 1.).width > width && !line.is_empty() {
            text(font, &line, x, y, size, color);
            y += size * 1.45;
            line = word.to_owned();
        } else {
            line = test;
        }
    }
    if !line.is_empty() {
        text(font, &line, x, y, size, color);
        y += size * 1.45;
    }
    y
}
fn direction_name(d: Point) -> &'static str {
    match d {
        [1, 0, 0] => "RIGHT  +X",
        [-1, 0, 0] => "LEFT  -X",
        [0, 1, 0] => "UP  +Y",
        [0, -1, 0] => "DOWN  -Y",
        [0, 0, 1] => "NEAR  +Z",
        _ => "FAR  -Z",
    }
}
fn ui(
    s: &Sim,
    fonts: &Fonts,
    view: &View,
    hits: &[Option<Obstruction>; 4],
    hint: &str,
    notice: &str,
) -> Vec<(Rect, usize)> {
    let w = screen_width();
    let h = screen_height();
    let u = (w / 1280.).min(h / 800.);
    let margin = 32. * u;
    let white = Color::new(0.88, 0.94, 0.96, 1.);
    let muted = Color::new(0.49, 0.62, 0.68, 1.);
    // Top identity strip.
    text(&fonts.bold, "PINSTRIDE", margin, 48. * u, 30. * u, white);
    text(&fonts.regular, "EVERY SWING CHANGES YOUR FOOTHOLD", margin, 69. * u, 11. * u, muted);
    for i in 0..ROOM_COUNT {
        let x = w - 284. * u + i as f32 * 31. * u;
        let filled = (s.state.completed & (1 << i)) != 0;
        draw_circle(
            x,
            43. * u,
            if i == s.state.room { 6. * u } else { 3. * u },
            if filled {
                rgb(MINT)
            } else if i == s.state.room {
                white
            } else {
                Color::new(0.23, 0.33, 0.38, 1.)
            },
        );
    }
    draw_line(margin, 87. * u, w - margin, 87. * u, 1., Color::new(0.2, 0.3, 0.34, 0.6));
    // Side briefing is deliberately separate from world labels and the bottom controls.
    let x = w - 306. * u;
    let top = 125. * u;
    text(&fonts.bold, &format!("CHAMBER {:02} / 08", s.state.room + 1), x, top, 12. * u, rgb(MINT));
    text(&fonts.bold, s.room().name, x, top + 36. * u, 29. * u, white);
    let yy = wrapped(&fonts.regular, s.room().lesson, x, top + 72. * u, 270. * u, 17. * u, muted);
    draw_line(x, yy + 8. * u, w - margin, yy + 8. * u, 1., Color::new(0.2, 0.3, 0.34, 0.7));
    let y = yy + 42. * u;
    let pin = s.state.pose.pin;
    let pin_color = if pin == 0 { BLUE } else { ORANGE };
    draw_circle(x + 6. * u, y - 5. * u, 5. * u, rgb(pin_color));
    text(&fonts.bold, if pin == 0 { "BLUE PINNED" } else { "ORANGE PINNED" }, x + 22. * u, y, 14. * u, rgb(pin_color));
    text(
        &fonts.regular,
        if pin == 0 { "Next pin: orange diamond" } else { "Next pin: blue circle" },
        x,
        y + 27. * u,
        14. * u,
        muted,
    );
    let mut buttons = vec![];
    for (i, hit) in hits.iter().enumerate() {
        let by = y + 59. * u + i as f32 * 45. * u;
        let selected = i == s.state.selected;
        draw_rectangle(
            x - 8. * u,
            by - 22. * u,
            282. * u,
            38. * u,
            if selected { Color::new(0.10, 0.18, 0.22, 0.96) } else { Color::new(0.055, 0.09, 0.12, 0.8) },
        );
        if selected {
            draw_rectangle(
                x - 8. * u,
                by - 22. * u,
                2. * u,
                38. * u,
                rgb(if hit.is_some() { [0.95, 0.35, 0.36] } else { MINT }),
            );
        }
        text(&fonts.bold, &(i + 1).to_string(), x + 5. * u, by + 3. * u, 18. * u, white);
        text(&fonts.regular, direction_name(s.state.pose.directions()[i]), x + 37. * u, by + 2. * u, 13. * u, white);
        text(
            &fonts.bold,
            if hit.is_some() { "BLOCKED" } else { "CLEAR" },
            x + 197. * u,
            by + 2. * u,
            10. * u,
            if hit.is_some() { rgb([0.9, 0.4, 0.4]) } else { rgb(MINT) },
        );
        buttons.push((Rect::new(x - 8. * u, by - 22. * u, 282. * u, 38. * u), i));
    }
    let status_y = y + 253. * u;
    let selected_hit = hits[s.state.selected];
    let status = if s.state.swing.is_some() {
        "SWINGING / HANDING OFF"
    } else if s.won() {
        "BOTH TIPS DOCKED"
    } else if s.trapped() {
        "NO LEGAL SWING / Z TO UNDO"
    } else if selected_hit.is_some() {
        "ARC OBSTRUCTED"
    } else {
        "SPACE TO SWING"
    };
    text(
        &fonts.bold,
        status,
        x,
        status_y,
        13. * u,
        if s.won() {
            rgb(MINT)
        } else if selected_hit.is_some() {
            rgb([0.96, 0.46, 0.4])
        } else {
            white
        },
    );
    let detail = if let Some(hit) = selected_hit {
        if hit.block.is_some() {
            "Red marker = first contact with a solid. Try another direction or orbit for a clearer view."
        } else {
            "This arc leaves the chamber cage. Choose another direction or undo."
        }
    } else {
        "The arriving tip locks. Your current pin releases automatically."
    };
    wrapped(&fonts.regular, detail, x, status_y + 27. * u, 270. * u, 13. * u, muted);
    let hint_y = (status_y + 123. * u).min(h - 145. * u);
    if !hint.is_empty() {
        wrapped(&fonts.regular, hint, x, hint_y, 270. * u, 13. * u, rgb(MINT));
    }
    // Projected destination labels are click targets as well as keyboard reminders.
    if !s.won() && s.state.swing.is_none() {
        for (i, hit) in hits.iter().enumerate() {
            let p = s.state.pose;
            let end = pw(p.tips[p.pin]) + v(world(p.directions()[i]));
            if let Some(at) = view.project(end, w, h) {
                let at = at + vec2(0., -24. * u);
                let col = if hit.is_some() { rgb([0.9, 0.35, 0.35]) } else { rgb(MINT) };
                draw_circle(at.x, at.y, 13. * u, Color::new(0.02, 0.05, 0.07, 0.94));
                draw_circle_lines(at.x, at.y, 13. * u, 1.5 * u, col);
                text(&fonts.bold, &(i + 1).to_string(), at.x - 4.5 * u, at.y + 5. * u, 14. * u, col);
                buttons.push((Rect::new(at.x - 22. * u, at.y - 22. * u, 44. * u, 66. * u), i));
            }
        }
        if let Some(at) = view.project(pw(s.state.pose.tips[pin]), w, h) {
            text(&fonts.bold, "PIN", at.x + 18. * u, at.y + 4. * u, 10. * u, rgb(pin_color));
        }
    }
    // Bottom controls / objective.
    draw_rectangle(0., h - 91. * u, w, 91. * u, Color::new(0.025, 0.045, 0.065, 0.97));
    draw_line(margin, h - 91. * u, w - margin, h - 91. * u, 1., Color::new(0.2, 0.3, 0.34, 0.7));
    text(&fonts.bold, "SEAT BOTH COLOURS IN THEIR MATCHING SOCKETS", margin, h - 61. * u, 13. * u, white);
    text(
        &fonts.regular,
        "1-4  Preview     SPACE  Swing     Z  Undo     R  Restart     H  Hint",
        margin,
        h - 35. * u,
        13. * u,
        muted,
    );
    text(
        &fonts.regular,
        "Right drag  Orbit     Wheel  Zoom     F5 / F9  Save / Load     ESC  Pause",
        margin,
        h - 13. * u,
        12. * u,
        muted,
    );
    if !notice.is_empty() {
        text(&fonts.bold, notice, margin, 112. * u, 13. * u, rgb(MINT));
    }
    if s.won() {
        let bw = 540. * u;
        let bx = (w - 320. * u - bw) * 0.5;
        let by = h * 0.4;
        draw_rectangle(bx, by, bw, 166. * u, Color::new(0.025, 0.07, 0.085, 0.96));
        draw_rectangle(bx, by, bw, 3. * u, rgb(MINT));
        text(
            &fonts.bold,
            if s.finished() { "A PERFECT HANDOFF" } else { "CHAMBER COMPLETE" },
            bx + 28. * u,
            by + 49. * u,
            28. * u,
            white,
        );
        text(
            &fonts.regular,
            if s.finished() {
                "Eight chambers. Two tips. One changing foothold."
            } else {
                "Both colours seated. Take your new foothold forward."
            },
            bx + 28. * u,
            by + 83. * u,
            16. * u,
            muted,
        );
        text(
            &fonts.bold,
            if s.finished() {
                "Z  Revisit last swing     R  Replay finale"
            } else {
                "SPACE  Continue     Z  Reconsider"
            },
            bx + 28. * u,
            by + 129. * u,
            15. * u,
            rgb(MINT),
        );
    }
    if !s.trapped() && !s.room().blocks.is_empty() {
        text(
            &fonts.regular,
            "CUTAWAY VIEW / Occluding panels fade; outlined volumes remain solid",
            margin,
            h - 111. * u,
            11. * u,
            muted,
        );
    }
    if s.trapped() {
        text(
            &fonts.bold,
            "ROUTE ENDED - Z TO UNDO OR R TO RESTART",
            margin,
            h - 119. * u,
            19. * u,
            rgb([1., 0.5, 0.37]),
        );
    }
    buttons
}
#[macroquad::main(window)]
async fn main() {
    vesper3d::viewer::startup::show().await;
    let args: Vec<_> = std::env::args().collect();
    let mut life = Lifecycle::<Held>::start_or_exit(&args, &CUES);
    let unattended = life.options.unattended();
    let verify = has_flag(&args, "--verify");
    let input_report = has_flag(&args, "--input-report");
    let mut sim = Sim::new(life.seed());
    life.load_flag_or_exit(&mut sim);
    let fonts = Fonts {
        regular: load_ttf_font_from_bytes(include_bytes!("../assets/fonts/Regular.ttf")).unwrap(),
        bold: load_ttf_font_from_bytes(include_bytes!("../assets/fonts/Bold.ttf")).unwrap(),
    };
    let materials = Materials::load().expect("world material");
    let mut look = Look::night();
    look.ambient_sky = [0.35, 0.43, 0.5];
    look.fog_color = [0.025, 0.045, 0.065];
    look.fog_density = 0.006;
    let settings_path = beside_exe("settings.json");
    let mut settings = Settings::load(&settings_path);
    let mut sounds = SoundBank::start(life.options.silent(), settings.sfx_level(), 0., || Rendered {
        sfx: SOUNDS
            .iter()
            .map(|p| (0..p.variants()).map(|n| synth::wav_bytes(&synth::render(*p, n, 17), synth::RATE)).collect())
            .collect(),
        stems: vec![],
    })
    .await;
    let mut shell = GameShell::new();
    let mut input = ClientInput::new();
    let mut batch = Batch::new();
    let mut camera = FpsCamera::new(0.72, -0.42);
    let mouse = MouseLook::new(0.006, false);
    let mut distance = 15.5;
    let mut room = sim.state.room;
    let mut meshes = chamber_mesh(sim.room());
    let mut buttons: Vec<(Rect, usize)> = vec![];
    let mut hint = String::new();
    let mut notice = String::new();
    let mut notice_time = 0.;
    loop {
        input.begin_frame_with_keyboard(&mut shell, false, unattended || platform::focused(), platform::keyboard());
        let dt = life.begin_frame(input.frame_seconds());
        let time = life.time();
        sounds.poll().await;
        let accepting = shell.accepting_input();
        let mut edges = 0;
        if let Some(s) = life.script() {
            for (i, cue) in ["one", "two", "three", "four"].iter().enumerate() {
                if s.starts(cue) {
                    edges |= 1 << (SELECT_SHIFT + i as u32);
                }
            }
            if s.starts("commit") {
                edges |= COMMIT;
            }
            if s.starts("undo") {
                edges |= UNDO;
            }
            if s.starts("restart") {
                edges |= RESTART;
            }
            if s.starts("next") {
                edges |= NEXT;
            }
            camera.turn([s.value("orbit", 0), 0.]);
            if s.starts("hint") {
                hint = sim.room().hint.into();
            }
        } else if accepting {
            for (i, key) in [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4].iter().enumerate() {
                if input.pressed(*key) {
                    edges |= 1 << (SELECT_SHIFT + i as u32);
                }
            }
            if input.pressed(KeyCode::Space) {
                edges |= if sim.won() { NEXT } else { COMMIT };
            }
            if input.pressed(KeyCode::Z) {
                edges |= UNDO;
            }
            if input.pressed(KeyCode::R) {
                edges |= RESTART;
            }
            if input.pressed(KeyCode::H) {
                hint = if let Some(route) = solve(sim.room(), sim.state.pose) {
                    if let Some(i) = route.first() {
                        format!(
                            "Try {} ({}) next. {}",
                            i + 1,
                            direction_name(sim.state.pose.directions()[*i]),
                            sim.room().hint
                        )
                    } else {
                        sim.room().hint.into()
                    }
                } else {
                    "This pose cannot reach the sockets. Undo a handoff and try another route.".into()
                };
            }
            if is_mouse_button_down(MouseButton::Right) {
                let p = mouse_pixels();
                camera.turn(mouse.look(p[0], p[1]));
            }
            camera.turn([
                if input.down(KeyCode::Left) {
                    -dt
                } else if input.down(KeyCode::Right) {
                    dt
                } else {
                    0.
                },
                if input.down(KeyCode::Up) {
                    -dt
                } else if input.down(KeyCode::Down) {
                    dt
                } else {
                    0.
                },
            ]);
            camera.pitch = camera.pitch.clamp(-1.2, 0.3);
            distance = (distance - mouse_wheel().1 * 0.8).clamp(8., 26.);
            if is_mouse_button_pressed(MouseButton::Left) {
                let m = Vec2::from(mouse_position());
                if let Some((_, i)) = buttons.iter().rev().find(|(r, _)| r.contains(m)) {
                    edges |= 1 << (SELECT_SHIFT + *i as u32);
                }
            }
        }
        life.feed(Held, edges, [0.; 2]);
        let (save, load) = life.save_load_requested(input.pressed(KeyCode::F5), input.pressed(KeyCode::F9));
        if save {
            let n = life.quick_save(&sim, "Pinstride / chamber progress");
            notice = format!("{} / {}", n.title, n.detail);
            notice_time = 4.;
        }
        if load {
            let n = life.quick_load(&mut sim);
            notice = format!("{} / {}", n.title, n.detail);
            notice_time = 4.;
            hint.clear();
        }
        for _ in 0..life.ticks(dt, if verify { 4. } else { 1. }, !shell.paused) {
            let tick = life.take_tick();
            let mut intent = Intent {
                select: (0..4).find(|i| tick.pressed(1 << (SELECT_SHIFT + *i as u32))),
                commit: tick.pressed(COMMIT),
                undo: tick.pressed(UNDO),
                restart: tick.pressed(RESTART),
                next: tick.pressed(NEXT),
            };
            if verify {
                intent = verification_input(&sim);
            }
            sim.step(&intent);
            for e in sim.drain_events() {
                if input_report {
                    println!(
                        "input: {e:?} chamber={} pin={} tips={:?} moves={}",
                        sim.state.room + 1,
                        sim.state.pose.pin,
                        sim.state.pose.tips,
                        sim.state.history.len()
                    );
                }
                match e {
                    Event::Selected => sounds.play(0, 0.25),
                    Event::Rejected => {
                        sounds.play(1, 0.5);
                        notice = "Blocked swing: pose and pin unchanged".into();
                        notice_time = 2.;
                    }
                    Event::Snapped => {
                        sounds.play(2, 0.5);
                        hint.clear();
                    }
                    Event::Solved => sounds.play(3, 0.7),
                    Event::Undone | Event::Restarted => {
                        sounds.play(4, 0.4);
                        hint.clear();
                    }
                    Event::ChamberChanged => {
                        sounds.play(0, 0.5);
                        hint.clear();
                    }
                }
            }
        }
        if room != sim.state.room {
            room = sim.state.room;
            meshes = chamber_mesh(sim.room());
            distance = if room == 0 { 15.5 } else { 21.5 };
            camera = FpsCamera::new(0.72, -0.42);
        }
        let c = sim.room();
        let center = (v(c.bounds.min) + v(c.bounds.max)) * 0.5;
        let mut view = View::first_person(Vec3::ZERO, camera.yaw, camera.pitch);
        view.fov = 43f32.to_radians();
        view.eye = center + view.right() * 1.9 - view.dir() * distance;
        clear_background(Color::new(0.025, 0.045, 0.065, 1.));
        set_camera(&view.camera_checked(0.15, 80.));
        materials.set_scene(&look, view.eye, time, 0.5);
        materials.draw_static(&meshes);
        // Only presentation fades panels intersecting the camera-to-baton rays. All collision
        // volumes remain solid and outlined; this prevents planning from becoming camera wrestling.
        let points = endpoints(&sim);
        let mut opaque = Template::new();
        let mut cutaway = Template::new();
        for (i, b) in c.blocks.iter().enumerate() {
            let blocked = points
                .iter()
                .chain(std::iter::once(&((points[0] + points[1]) * 0.5)))
                .any(|p| segment_hits(view.eye.to_array(), p.to_array(), *b, 0.05));
            let target = if blocked { &mut cutaway } else { &mut opaque };
            target.box_(
                (v(b.min) + v(b.max)) * 0.5,
                (v(b.max) - v(b.min)) * 0.5,
                if i % 2 == 0 { [0.18, 0.23, 0.27] } else { [0.23, 0.28, 0.30] },
                0.03,
            );
        }
        batch.clear();
        batch.add(&opaque, Mat4::IDENTITY, Tint::NONE);
        gl_use_material(&materials.world);
        batch.draw();
        batch.clear();
        batch.add(&cutaway, Mat4::IDENTITY, Tint::alpha(0.17));
        gl_use_material(&materials.fx_alpha);
        batch.draw();
        let mut dynamic = Template::new();
        draw_actor(&sim, time, &mut dynamic);
        let hits = preview(&sim, &mut dynamic);
        batch.clear();
        batch.add(&dynamic, Mat4::IDENTITY, Tint::NONE);
        gl_use_material(&materials.world);
        batch.draw();
        gl_use_default_material();
        set_default_camera();
        if notice_time > 0. {
            notice_time -= dt;
        } else {
            notice.clear();
        }
        buttons = ui(&sim, &fonts, &view, &hits, &hint, &notice);
        let controls: Vec<_> = identity().controls.split(", ").map(str::to_owned).collect();
        let refs: Vec<_> = controls.iter().map(String::as_str).collect();
        let menu = shell.local_menu_with_audio(
            "Pinstride",
            &refs,
            AudioMenu { music_on: false, sfx_on: settings.sfx_on, has_music: false },
        );
        if menu.toggle_sfx {
            settings.toggle_sfx();
            sounds.sfx_volume = settings.sfx_level();
            settings.store(&settings_path);
        }
        if menu.quit {
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
    println!(
        "Pinstride: chamber={} swings={} completed={} hash={:016x}",
        sim.state.room + 1,
        sim.state.history.len(),
        sim.state.completed,
        vesper3d::viewer::devkit::Simulation::state_hash(&sim)
    );
    if let Some(report) = life.report() {
        println!("{report}");
    }
}
