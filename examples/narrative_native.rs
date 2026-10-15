//! Tiny native spatial-3D preview, with a replaceable game-owned text/controller UI.
//! --fixture storyless|reactive|scene [--content FILE] [--quiet] [Lifecycle capture/script flags]
//! F6 validates and reloads content between ticks; F5/F9 use engine snapshots.
#[path = "../templates/native_focus.rs"]
mod platform;
#[path = "narrative/shared.rs"]
mod shared;
use macroquad::prelude::*;
use shared::{Fixture, Input, Sim};
use std::path::PathBuf;
use vesper3d::viewer::{
    devkit::{flag_value, has_flag, parse_size, Lifecycle},
    game_client::{self, GameShell},
    game_input::ClientInput,
    gamepad::Button,
    kit::{self, hud, Batch, Look, Materials, Template, Tint, View},
};
fn validate_args(args: &[String]) -> Result<(), String> {
    let mut index = 1;
    let mut seen = std::collections::BTreeSet::new();
    while index < args.len() {
        let option = args[index].as_str();
        if !seen.insert(option) {
            return Err(format!("duplicate option {option}"));
        }
        match option {
            "--help" | "--quiet" | "--mute" | "--audible" | "--perf" | "--novsync" => index += 1,
            "--fixture" | "--content" | "--seed" | "--script" | "--capture" | "--frames"
            | "--exit-after" | "--size" | "--load" | "--save-dir" => {
                if args.get(index + 1).is_none_or(|a| a.starts_with("--")) {
                    return Err(format!("{option} needs a value"));
                }
                index += 2;
            }
            _ => return Err(format!("unknown option {option:?}; use --help")),
        }
    }
    Ok(())
}
fn window() -> macroquad::conf::Conf {
    let mut conf = game_client::window_config("Relay Yard — optional narrative preview");
    let args: Vec<String> = std::env::args().collect();
    if let Err(error) = validate_args(&args) {
        eprintln!("{error}");
        std::process::exit(2);
    }
    if has_flag(&args, "--help") {
        println!("Relay Yard native preview\n--fixture storyless|reactive|scene --content FILE --quiet\n--seed N --capture DIR --frames N --script CUES --exit-after N --size WxH\nF5 save; F9 load; F6 validate/reload; Tab/RB reveal; Enter/A advance; Backspace/B interrupt. Enter/LB dismisses barks; A remains jump.\nScenes block movement; clock continues. Esc pauses all ticks.");
        std::process::exit(0);
    }
    if let Some((w, h)) = flag_value(&args, "--size").and_then(parse_size) {
        conf.miniquad_conf.window_width = w as i32;
        conf.miniquad_conf.window_height = h as i32;
    }
    conf
}
#[derive(Clone, Copy, Default)]
struct Held {
    forward: f32,
    right: f32,
}
const JUMP: u32 = 1;
const INTERACT: u32 = 2;
const CUES: [&str; 13] = [
    "fwd",
    "back",
    "left",
    "right",
    "jump",
    "look",
    "interact",
    "reveal",
    "advance",
    "choose",
    "reload",
    "interrupt",
    "walk",
];
#[derive(Default)]
struct TextFocus {
    token: Option<(bool, u64)>,
    revealed: usize,
    focus: usize,
    age: f32,
}
impl TextFocus {
    fn sync(&mut self, token: Option<(bool, u64)>) {
        if self.token != token {
            *self = Self {
                token,
                ..Default::default()
            };
        }
    }
    fn prefix(&self, text: &str) -> String {
        text.chars().take(self.revealed).collect()
    }
}
fn v(p: vesper3d::math::V) -> Vec3 {
    vec3(p.0, p.1, p.2)
}
fn draw_world(sim: &Sim, materials: &Materials, look: &Look, time: f32) {
    let world = &sim.world;
    let mut scene = Template::new();
    for (index, solid) in shared::solids(world.gate_open).iter().enumerate() {
        let center = (v(solid.min) + v(solid.max)) * 0.5;
        let half = (v(solid.max) - v(solid.min)) * 0.5;
        let color = if index == 0 {
            [0.18, 0.27, 0.30]
        } else if index == 8 {
            if world.gate_open {
                [0.22, 0.85, 0.55]
            } else {
                [0.8, 0.4, 0.22]
            }
        } else if index == 9 || index == 10 {
            [0.95, 0.71, 0.25]
        } else if index >= 5 {
            [0.38, 0.47, 0.50]
        } else {
            [0.28, 0.36, 0.40]
        };
        scene.box_(center, half, color, 0.);
    }
    if world.cell_available {
        scene.ball(
            v(shared::CELL),
            vec3(0.16, 0.25, 0.16),
            [1., 0.85, 0.3],
            0.5,
            12,
            7,
        );
    }
    scene.cylinder(
        v(shared::FINISH) + vec3(0., 0.004, 0.),
        0.65,
        0.002,
        if world.finished {
            [0.25, 1., 0.65]
        } else {
            [0.4, 0.7, 0.9]
        },
        0.3,
        20,
    );
    let eye = v(world.player.position);
    set_camera(
        &View::first_person(eye, world.player.yaw, world.player.pitch).camera_checked(0.15, 70.),
    );
    materials.set_scene(look, eye, time, 0.);
    let mut batch = Batch::new();
    batch.add(&scene, Mat4::IDENTITY, Tint::NONE);
    gl_use_material(&materials.world);
    batch.draw();
    gl_use_default_material();
    set_default_camera();
}
fn text_panel(speaker: &str, text: &str, choices: &[(String, bool)], hint: &str) {
    let w = screen_width();
    let h = screen_height();
    let panel_h = (190. + choices.len() as f32 * 35.).min(h - 100.);
    let top = h - panel_h - 20.;
    hud::panel(
        20.,
        top,
        w - 40.,
        panel_h,
        0.,
        Color::new(0.03, 0.07, 0.11, 1.0),
    );
    hud::text_outlined(
        &speaker.to_uppercase(),
        42.,
        top + 31.,
        22.,
        Color::new(0.95, 0.76, 0.35, 1.),
    );
    let mut y = top + 62.;
    for line in hud::wrap(text, w - 90., 24.) {
        hud::text_outlined(&line, 42., y, 24., WHITE);
        y += 29.;
    }
    y = y.max(top + 112.);
    for (choice, focused) in choices {
        hud::text_outlined(
            &format!("{} {}", if *focused { ">" } else { " " }, choice),
            42.,
            y,
            22.,
            if *focused { YELLOW } else { LIGHTGRAY },
        );
        y += 35.;
    }
    hud::text_outlined(hint, 42., h - 38., 17., LIGHTGRAY);
}
#[macroquad::main(window)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if has_flag(&args, "--help") {
        println!("Relay Yard native preview\n--fixture storyless|reactive|scene --content FILE --quiet\n--seed N --capture DIR --frames N --script CUES --exit-after N --size WxH\nF5 save; F9 load; F6 validate/reload; Tab/RB reveal; Enter/A advance; Backspace/B interrupt. Enter/LB dismisses barks; A remains jump.\nScenes block movement; clock continues. Esc pauses all ticks.");
        return;
    }
    let fixture =
        Fixture::parse(flag_value(&args, "--fixture").unwrap_or("scene")).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(2)
        });
    let path = flag_value(&args, "--content")
        .map(PathBuf::from)
        .or_else(|| fixture.source());
    let quiet = has_flag(&args, "--quiet");
    let mut life = Lifecycle::<Held>::start_or_exit(&args, &CUES);
    let mut sim = Sim::with_source(fixture, life.seed(), path.as_deref()).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    life.load_flag_or_exit(&mut sim);
    let materials = Materials::load().expect("native materials");
    let look = Look::daylight();
    let mut shell = GameShell::new();
    let mut device = ClientInput::new();
    let mut text = TextFocus::default();
    let mut status = String::new();
    let mut pending = Input::default();
    loop {
        let scene = sim.scene().unwrap_or_else(|e| {
            status = e;
            None
        });
        let response = sim.response().unwrap_or_else(|e| {
            status = e;
            None
        });
        let scene_active = sim
            .narrative
            .as_ref()
            .is_some_and(|r| r.state().scene.is_some());
        let focused = scene_active;
        device.begin_frame_with_keyboard(
            &mut shell,
            !focused,
            life.options.unattended() || platform::focused(),
            platform::keyboard(),
        );
        let dt = life.begin_frame(device.frame_seconds());
        let allowed = shell.accepting_input();
        let script = life.script();
        let mut movement = device.movement(&shell);
        let mut look_delta = device.look_delta_with(&shell, dt);
        let mut interact =
            allowed && (device.pressed(KeyCode::E) || device.gamepad().pressed(Button::West));
        let mut reveal = allowed
            && (device.pressed(KeyCode::Tab) || device.gamepad().pressed(Button::RightTrigger));
        let mut advance = allowed && device.menu_select();
        let mut bark_dismiss = allowed
            && (device.pressed(KeyCode::Enter) || device.gamepad().pressed(Button::LeftTrigger));
        let mut interrupt = allowed
            && (device.pressed(KeyCode::Backspace) || device.gamepad().pressed(Button::East));
        let mut reload = allowed && device.pressed(KeyCode::F6);
        let mut scripted_choice = None;
        if let Some(s) = script {
            movement.forward = s.axis("fwd", "back");
            movement.right = s.axis("right", "left");
            movement.jump = s.starts("jump");
            look_delta = [s.value("look", 0), s.value("look", 1)];
            interact = s.starts("interact");
            reveal = s.starts("reveal");
            advance = s.starts("advance");
            bark_dismiss = advance;
            interrupt = s.starts("interrupt");
            reload = s.starts("reload");
            if s.starts("choose") {
                scripted_choice = Some(s.value("choose", 0) as usize);
            }
            if s.held("walk") {
                let route = shared::walk_inputs(life.frame() as usize);
                movement.forward = route.forward;
                movement.right = route.right;
                interact = route.interact;
            }
        }
        text.sync(
            scene
                .as_ref()
                .map(|s| (true, s.token))
                .or_else(|| response.as_ref().map(|r| (false, r.token))),
        );
        if allowed {
            text.age += dt;
        }
        let active_text = scene
            .as_ref()
            .map(|s| s.text.as_str())
            .or_else(|| response.as_ref().map(|r| r.text.as_str()));
        if let Some(line) = active_text {
            if allowed {
                text.revealed = text.revealed.max((text.age * 42.) as usize);
            }
            if reveal {
                text.revealed = line.chars().count();
            }
            if let Some(s) = &scene {
                // Text focus consumes movement, jump and interact; only intentions reach the authority.
                movement = Default::default();
                look_delta = [0., 0.];
                interact = false;
                let nav = device.menu_step();
                if !s.choices.is_empty() {
                    if allowed && nav.down {
                        text.focus = (text.focus + 1) % s.choices.len();
                    }
                    if allowed && nav.up {
                        text.focus = (text.focus + s.choices.len() - 1) % s.choices.len();
                    }
                }
                if let Some(index) = scripted_choice {
                    text.focus = index.min(s.choices.len().saturating_sub(1));
                    advance = true;
                    text.revealed = line.chars().count();
                }
                if advance {
                    if text.revealed < line.chars().count() {
                        status = "Tab / RB reveals text; Enter / A advances after reveal.".into();
                    } else if let Some(choice) = s.choices.get(text.focus) {
                        pending.choice = Some((s.token, choice.id.clone()));
                    } else {
                        pending.advance = Some(s.token);
                    }
                }
                if interrupt {
                    pending.scene_interrupt = Some(s.token);
                }
            } else if let Some(r) = &response {
                if quiet || interrupt {
                    pending.interrupt = Some(r.token);
                } else {
                    let presented = sim
                        .narrative
                        .as_ref()
                        .and_then(|r| r.state().active_response.as_ref())
                        .is_some_and(|r| r.presented);
                    if !presented {
                        pending.presented = Some(r.token);
                    } else if bark_dismiss {
                        if text.revealed >= line.chars().count() {
                            pending.complete = Some(r.token);
                        } else {
                            pending.interrupt = Some(r.token);
                        }
                    } else if text.age >= 4.2 && text.revealed >= line.chars().count() {
                        pending.complete = Some(r.token);
                    }
                }
            }
        }
        if scene_active && scene.is_none() {
            movement = Default::default();
            look_delta = [0., 0.];
            interact = false;
            if interrupt {
                pending.scene_interrupt = sim
                    .narrative
                    .as_ref()
                    .and_then(|r| r.state().scene.as_ref())
                    .map(|s| s.token);
            }
        }
        life.feed(
            Held {
                forward: movement.forward,
                right: movement.right,
            },
            if movement.jump { JUMP } else { 0 } | if interact { INTERACT } else { 0 },
            look_delta,
        );
        let (save, load) =
            life.save_load_requested(device.pressed(KeyCode::F5), device.pressed(KeyCode::F9));
        if save {
            let n = life.quick_save(&sim, "Relay Yard");
            status = format!("{}: {}", n.title, n.detail);
        }
        if load {
            let n = life.quick_load(&mut sim);
            status = format!("{}: {}", n.title, n.detail);
            if n.ok {
                text = TextFocus::default();
                pending = Input::default();
            }
        }
        if reload {
            let began = std::time::Instant::now();
            status = match path.as_deref() {
                Some(path) => match sim.reload(path) {
                    Ok(()) => "Content validated and reloaded between ticks".into(),
                    Err(e) => format!("Reload rejected; previous content retained: {e}"),
                },
                None => "Storyless run has no narrative content to reload".into(),
            };
            let cost = began.elapsed().as_secs_f64() * 1000.;
            println!(
                "{}",
                serde_json::json!({"reload_ms":cost,"tick":sim.world.tick,"result":status})
            );
        }
        for _ in 0..life.ticks(dt, 1., !shell.paused) {
            let tick = life.take_tick();
            let mut intent = std::mem::take(&mut pending);
            intent.forward = tick.held.forward;
            intent.right = tick.held.right;
            intent.look = tick.look;
            intent.jump = tick.pressed(JUMP);
            intent.interact = tick.pressed(INTERACT);
            sim.step(&intent);
            if let Some(error) = &sim.notice {
                status = error.clone();
            }
        }
        clear_background(look.clear_color());
        draw_world(&sim, &materials, &look, life.time());
        hud::text_outlined("RELAY YARD", 24., 34., 25., WHITE);
        hud::text_outlined(
            &format!(
                "Cell: {}   Bridge: {}   {}",
                sim.world.cells,
                if sim.world.gate_open {
                    "powered"
                } else {
                    "closed"
                },
                if sim.world.finished {
                    "CROSSING COMPLETE"
                } else {
                    "Climb the left stairs; bring the cell to the orange relay."
                }
            ),
            24.,
            65.,
            19.,
            WHITE,
        );
        if !focused {
            hud::text_outlined("WASD / left stick move | mouse / right stick look | Space / A jump | E / X interact",24.,screen_height()-50.,18.,WHITE);
        }
        if let Ok(Some(scene)) = sim.scene() {
            let choices = scene
                .choices
                .iter()
                .enumerate()
                .map(|(i, c)| (c.text.clone(), i == text.focus))
                .collect::<Vec<_>>();
            text_panel(&scene.speaker,&text.prefix(&scene.text),&choices,"Tab / RB reveal | Up/Down / D-pad focus | Enter / A choose or advance | Backspace / B interrupt");
        } else if scene_active {
            text_panel(
                "Scene unavailable",
                "The speaker or context is unavailable.",
                &[],
                "Backspace / B interrupts safely; no movement input reaches gameplay.",
            );
        } else if !quiet {
            if let Ok(Some(line)) = sim.response() {
                text_panel(
                    &line.speaker,
                    &text.prefix(&line.text),
                    &[],
                    "Tab / RB reveal | Enter / LB dismiss | Backspace / B interrupt",
                );
            }
        }
        if !status.is_empty() {
            hud::text_outlined(&status, 24., 92., 16., YELLOW);
        }
        if shell.local_menu(
            "Relay Yard",
            &[
                "F5 save | F9 load | F6 reload content",
                "Scenes pause movement and gravity; game clock continues.",
                "Esc pauses all fixed ticks. Barks never change gameplay.",
            ],
        ) {
            game_client::request_exit();
        }
        if let Some(path) = life.capture_path() {
            life.captured(
                &path,
                kit::capture::save_frame(&path).map_err(|e| e.to_string()),
            );
        }
        if life.end_frame(dt) || game_client::exit_requested() {
            break;
        }
        next_frame().await;
    }
    println!(
        "{}",
        serde_json::json!({"native_fixture_report":1,"fixture":format!("{:?}",fixture),"world":sim.world.capture(),"state_hash":format!("{:016x}",vesper3d::viewer::devkit::Simulation::state_hash(&sim)),"narrative":sim.narrative.as_ref().map(|r|r.state())})
    );
    if let Some(report) = life.report() {
        println!("{report}");
    }
}
