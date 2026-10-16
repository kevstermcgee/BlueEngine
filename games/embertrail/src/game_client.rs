//! Controller-first platform glue; gameplay remains in Embertrail::step.
use crate::{soundtrack::Score, window_focus};
use embertrail::Embertrail;
use macroquad::prelude::*;
use serde::{Deserialize, Serialize};
use vesper3d::{
    runtime::{
        playback::{flag_value, has_flag, CapturePlan, Timeline},
        storage::{self, PlatformStorage},
        FixedStepper, InputAccumulator,
    },
    two_d::{
        draw::{Game, Renderer, Scene, Viewport},
        GameLogic, Intent, Point, Rect, Simulation,
    },
    viewer::{
        devkit::MenuNav,
        gamepad::{Button, Gamepads},
    },
};

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    sound: bool,
    music: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            sound: true,
            music: true,
        }
    }
}
const PAPER: Color = Color::new(0.97, 0.93, 0.82, 1.);
const MINT: Color = Color::new(0.64, 0.88, 0.75, 1.);
const GOLD: Color = Color::new(0.98, 0.76, 0.42, 1.);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PauseAction {
    Resume,
    Atlas,
    RestartCheckpoint,
    RestartCourse,
    Music,
    Sound,
    Download,
    ReturnTitle,
    Quit,
}
fn pause_actions(started: bool) -> &'static [PauseAction] {
    use PauseAction::*;
    if started {
        &[
            Resume,
            Atlas,
            RestartCheckpoint,
            RestartCourse,
            Music,
            Sound,
            Download,
            ReturnTitle,
            Quit,
        ]
    } else {
        &[ReturnTitle, Music, Sound, Download, Quit]
    }
}
fn panel(scene: &mut Scene, rect: Rect) {
    scene.rect(
        200,
        Rect::new(0, 0, 800, 450),
        Color::new(0.025, 0.06, 0.09, 0.72),
    );
    scene.rect(
        201,
        Rect::new(rect.x - 2, rect.y - 2, rect.w + 4, rect.h + 4),
        Color::new(0.42, 0.61, 0.55, 1.),
    );
    scene.rect(202, rect, Color::new(0.055, 0.12, 0.15, 0.98));
    scene.rect(
        203,
        Rect::new(rect.x + 14, rect.y + 12, rect.w - 28, 2),
        GOLD,
    );
}
fn text(scene: &mut Scene, value: impl Into<String>, x: i32, y: i32, size: f32, color: Color) {
    scene.text(210, value, Point::new(x, y), size, color);
}
fn row(scene: &mut Scene, label: &str, selected: bool, index: usize, y: i32) {
    if selected {
        scene.rect(
            205,
            Rect::new(167, y - 19, 466, 25),
            Color::new(0.17, 0.29, 0.29, 1.),
        );
        scene.rect(206, Rect::new(167, y - 19, 4, 25), GOLD);
    }
    text(
        scene,
        format!("{}  {label}", if selected { ">" } else { " " }),
        184,
        y,
        21.,
        if selected { GOLD } else { PAPER },
    );
    let _ = index;
}
fn centered(scene: &mut Scene, font: &Font, value: &str, y: i32, size: u16, color: Color) {
    let scale = (screen_width() / 800.)
        .min(screen_height() / 450.)
        .max(0.01);
    let pixels = (size as f32 * scale).round().clamp(1., u16::MAX as f32) as u16;
    let measured = measure_text(value, Some(font), pixels, 1.);
    let x = ((800. - measured.width / scale) * 0.5).round() as i32;
    scene.text(
        208,
        value,
        Point::new(x + 1, y + 2),
        size as f32,
        Color::new(0.015, 0.04, 0.055, 0.9),
    );
    text(scene, value, x, y, size as f32, color);
}
fn title(
    scene: &mut Scene,
    game: &Embertrail,
    selected: usize,
    track: &str,
    font: &Font,
    seconds: f32,
) {
    centered(scene, font, "EMBERTRAIL", 95, 48, GOLD);
    centered(
        scene,
        font,
        "A little light. A very big adventure.",
        128,
        20,
        MINT,
    );
    centered(
        scene,
        font,
        "Vesper stole your village's Dawnheart.",
        171,
        22,
        PAPER,
    );
    centered(
        scene,
        font,
        "Without its warmth, your home will freeze.",
        198,
        22,
        PAPER,
    );
    centered(
        scene,
        font,
        "Free eight bells. Bring the Dawnheart home.",
        227,
        22,
        MINT,
    );
    let begin = if game.tick() > 0 {
        "CONTINUE THE TRAIL"
    } else {
        "CARRY THE FIRST SPARK"
    };
    scene.rect(
        205,
        Rect::new(228, 264, 344, 42),
        Color::new(0.075, 0.17, 0.19, 0.95),
    );
    if selected == 0 {
        let pulse = 0.75 + 0.2 * (seconds * 2.5).sin();
        scene.rect(
            206,
            Rect::new(228, 264, 344, 2),
            Color::new(GOLD.r, GOLD.g, GOLD.b, pulse),
        );
        scene.rect(206, Rect::new(228, 304, 344, 2), GOLD);
        scene.circle(207, Point::new(245, 285), 3., GOLD);
        scene.circle(207, Point::new(555, 285), 3., GOLD);
    }
    centered(
        scene,
        font,
        begin,
        291,
        22,
        if selected == 0 { GOLD } else { PAPER },
    );
    if selected == 1 {
        scene.rect(
            205,
            Rect::new(300, 320, 200, 32),
            Color::new(0.075, 0.17, 0.19, 0.92),
        );
        scene.rect(206, Rect::new(300, 350, 200, 2), GOLD);
    }
    centered(
        scene,
        font,
        "Quit game",
        343,
        21,
        if selected == 1 { GOLD } else { PAPER },
    );
    scene.rect(
        205,
        Rect::new(180, 357, 440, 44),
        Color::new(0.025, 0.075, 0.09, 0.88),
    );
    centered(
        scene,
        font,
        "A  Confirm     Stick / D-pad  Choose",
        375,
        18,
        MINT,
    );
    centered(scene, font, "Menu  Options / soundtrack", 393, 16, MINT);
    centered(
        scene,
        font,
        &format!("Now playing: {track}"),
        416,
        16,
        PAPER,
    );
    text(
        scene,
        format!("v{}", env!("CARGO_PKG_VERSION")),
        735,
        438,
        16.,
        MINT,
    );
}
fn pause(
    scene: &mut Scene,
    started: bool,
    settings: &Settings,
    selected: usize,
    track: &str,
    loading: bool,
    length: f32,
) {
    panel(scene, Rect::new(150, 30, 500, 390));
    text(
        scene,
        if started { "PAUSED" } else { "OPTIONS" },
        184,
        71,
        32.,
        GOLD,
    );
    text(
        scene,
        if loading {
            "Soundtrack loading"
        } else {
            "NOW PLAYING"
        },
        184,
        94,
        16.,
        MINT,
    );
    text(scene, track, 184, 119, 22., PAPER);
    for (index, action) in pause_actions(started).iter().enumerate() {
        let label = match action {
            PauseAction::Resume => "Resume".to_owned(),
            PauseAction::Atlas => "Trail atlas".to_owned(),
            PauseAction::RestartCheckpoint => "Restart checkpoint".to_owned(),
            PauseAction::RestartCourse => "Restart course".to_owned(),
            PauseAction::Music => {
                format!("Music: {}", if settings.music { "On" } else { "Off" })
            }
            PauseAction::Sound => format!(
                "Sound effects: {}",
                if settings.sound { "On" } else { "Off" }
            ),
            PauseAction::Download => format!(
                "Download this song (.wav)  {}:{:02}",
                length as u32 / 60,
                length as u32 % 60
            ),
            PauseAction::ReturnTitle => "Return to title".to_owned(),
            PauseAction::Quit => "Quit game".to_owned(),
        };
        row(
            scene,
            &label,
            selected == index,
            index,
            146 + index as i32 * 26,
        );
    }
    text(
        scene,
        if started {
            "A  Select     B  Resume     Stick / D-pad  Choose"
        } else {
            "A  Select     B  Title     Stick / D-pad  Choose"
        },
        178,
        398,
        17.,
        MINT,
    );
}
fn terminal(scene: &mut Scene, won: bool) {
    panel(scene, Rect::new(128, 115, 544, 220));
    text(
        scene,
        if won {
            "EVERY WINDOW HOLDS THE DAWN"
        } else {
            "YOUR SPARK IS STILL HERE"
        },
        154,
        164,
        27.,
        GOLD,
    );
    text(
        scene,
        if won {
            "The bells are free. Your village is warm again."
        } else {
            "A setback is another beginning. Keep your discoveries."
        },
        154,
        203,
        20.,
        PAPER,
    );
    text(
        scene,
        if won {
            "A  Return to the trail atlas"
        } else {
            "A  Try this course again"
        },
        154,
        257,
        23.,
        MINT,
    );
    text(
        scene,
        "Menu  Pause / soundtrack / quit",
        154,
        302,
        18.,
        PAPER,
    );
}
fn script_event(script: &Timeline, frame: u32, name: &str) -> bool {
    script.starting(frame).any(|c| c.name == name)
}
fn script_held(script: &Timeline, frame: u32, name: &str) -> bool {
    script.active(frame).any(|c| c.name == name)
}

pub async fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    // Launching an installed EXE from another working directory still finds its assets.
    if !std::path::Path::new("assets/audio/score").is_dir() {
        if let Some(directory) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        {
            if directory.join("assets/audio/score").is_dir() {
                std::env::set_current_dir(directory).map_err(|e| e.to_string())?;
            }
        }
    }
    let capture = CapturePlan::from_args(&args).map_err(|e| e.to_string())?;
    if let Some(plan) = &capture {
        plan.create_dir().map_err(|e| e.to_string())?;
    }
    let script = flag_value(&args, "--script")
        .map(|value| {
            Timeline::parse(
                value,
                &[
                    "start", "pause", "up", "down", "left", "right", "select", "back", "jump",
                    "cast", "run", "atlas", "download",
                ],
            )
        })
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let verify = has_flag(&args, "--verify");
    let persist = !verify && capture.is_none();
    let muted = has_flag(&args, "--mute") || (capture.is_some() && !has_flag(&args, "--audible"));
    let store = PlatformStorage::new(Embertrail::ID)?;
    let (mut settings, mut notice) = match storage::read_settings::<Settings>(&store) {
        Ok(settings) => (settings, String::new()),
        Err(error) => (Settings::default(), error),
    };
    let mut notice_time = if notice.is_empty() { 0. } else { 9. };
    let mut game = Embertrail::new(7);
    if persist {
        match storage::load_slot(&store, "progress", &mut game) {
            Ok(true) => {
                notice = "Your trail is safe. Continue when ready.".into();
                notice_time = 6.;
            }
            Ok(false) => {}
            Err(error) => {
                notice = error;
                notice_time = 9.;
            }
        }
    }
    let showcase_world = flag_value(&args, "--showcase-world");
    let showcase_boss = flag_value(&args, "--showcase-boss");
    let showcase_secret = flag_value(&args, "--showcase-secret");
    let showcase_ending = has_flag(&args, "--showcase-ending");
    let showcase_count = usize::from(showcase_world.is_some())
        + usize::from(showcase_boss.is_some())
        + usize::from(showcase_secret.is_some())
        + usize::from(showcase_ending);
    let showcase = showcase_count > 0;
    if showcase {
        if capture.is_none() || verify || showcase_count != 1 {
            return Err("Showcase fixtures require --capture, one fixture, and no --verify; they are visual review only".into());
        }
        let world = showcase_world
            .or(showcase_boss)
            .or(showcase_secret)
            .unwrap_or("0")
            .parse::<usize>()
            .ok()
            .filter(|world| *world < 8)
            .ok_or("Showcase world must be 0 through 7")?;
        game.state.unlocked = 31;
        game.enter_course(
            world * 4
                + if showcase_boss.is_some() {
                    3
                } else if showcase_secret.is_some() || showcase_ending {
                    0
                } else {
                    2
                },
        );
        if showcase_ending {
            game.state.won = true;
            game.state.cleared = u32::MAX;
        } else if showcase_boss.is_some() {
            let boss = game
                .state
                .level
                .enemies
                .iter_mut()
                .find(|enemy| enemy.kind == 4)
                .ok_or("Showcase boss unavailable")?;
            boss.phase = 1;
            game.state.px = (boss.r.x - 230) * embertrail::SUB;
        } else if showcase_secret.is_some() {
            game.state.px = (game.state.level.secret.x - 190).max(0) * embertrail::SUB;
        } else {
            game.state.px = 650 * embertrail::SUB;
        }
        game.state.camera = (game.player().x - 280).clamp(0, game.state.level.width - 800);
        game.state.shield = 1200;
        println!(
            "{}",
            serde_json::json!({"visual_fixture":true,"world":world,"boss":showcase_boss.is_some(),"secret":showcase_secret.is_some(),"ending":showcase_ending,"gameplay_evidence":false})
        );
    }
    let mut started = verify
        || showcase
        || (capture.is_some()
            && !has_flag(&args, "--title")
            && flag_value(&args, "--script").is_none());
    let mut paused = false;
    let mut selection = 0usize;
    let mut title_selection = 0usize;
    let mut nav = MenuNav::default();
    let mut pads = Gamepads::new().ok();
    let mut score = Score::new(muted).await?;
    let mut stepper = FixedStepper::new();
    let mut input = InputAccumulator::<Intent>::new();
    let mut renderer = Renderer::default();
    let font = load_ttf_font_from_bytes(include_bytes!("../assets/fonts/DejaVuSans.ttf"))
        .map_err(|error| format!("Interface font: {error}"))?;
    renderer.set_font(font.clone());
    let mut frame = 0;
    let mut verification_tick = 0;
    let mut save_clock = 0.;
    let mut saved_hash = None;
    let mut was_focused = true;
    let mut fullscreen = false;
    let mut presentation_seconds = 0.;
    prevent_quit();
    loop {
        frame += 1;
        let dt = get_frame_time().clamp(0., 0.1);
        presentation_seconds += dt;
        let focused = window_focus::focused();
        let pad = pads.as_mut().map(|p| p.poll(focused)).unwrap_or_default();
        if was_focused && !focused && started {
            paused = true;
            selection = 0;
            input.clear();
            stepper.reset();
            nav.reset();
        }
        was_focused = focused;
        let key = |k| focused && is_key_pressed(k);
        let held = |k| focused && is_key_down(k);
        let press = |name| focused && script_event(&script, frame, name);
        let hold = |name| focused && script_held(&script, frame, name);
        if key(KeyCode::F) {
            fullscreen = !fullscreen;
            macroquad::miniquad::window::set_fullscreen(fullscreen);
        }
        let confirm = key(KeyCode::Enter)
            || key(KeyCode::Space)
            || pad.pressed(Button::South)
            || press("select")
            || press("start");
        let back = key(KeyCode::Escape) || pad.menu_back() || press("back");
        let pause_press = key(KeyCode::Escape) || pad.pressed(Button::Start) || press("pause");
        let atlas_press = key(KeyCode::Tab)
            || pad.pressed(Button::Select)
            || pad.pressed(Button::North)
            || press("atlas");
        let mut dpad = pad.dpad();
        dpad[0] |= held(KeyCode::Up) || held(KeyCode::W) || hold("up");
        dpad[1] |= held(KeyCode::Down) || held(KeyCode::S) || hold("down");
        dpad[2] |= held(KeyCode::Left) || held(KeyCode::A) || hold("left");
        dpad[3] |= held(KeyCode::Right) || held(KeyCode::D) || hold("right");
        let menu_step = nav.update(dt, pad.left_stick, dpad);
        let mut consume = false;
        let mut atlas_command = false;
        let mut quit = is_quit_requested();
        if paused {
            let actions = pause_actions(started);
            if menu_step.up {
                selection = (selection + actions.len() - 1) % actions.len();
            }
            if menu_step.down {
                selection = (selection + 1) % actions.len();
            }
            if back || pause_press {
                paused = false;
                consume = true;
                nav.reset();
            } else if confirm || press("download") {
                consume = true;
                let selected = if press("download") {
                    PauseAction::Download
                } else {
                    actions[selection]
                };
                match selected {
                    PauseAction::Resume => paused = false,
                    PauseAction::Atlas => {
                        game.open_atlas();
                        paused = false;
                    }
                    PauseAction::RestartCheckpoint => {
                        game.restart_checkpoint();
                        paused = false;
                    }
                    PauseAction::RestartCourse => {
                        game.restart();
                        paused = false;
                    }
                    PauseAction::Music => {
                        settings.music = !settings.music;
                        notice = match storage::write_settings(&store, &settings) {
                            Ok(()) => {
                                format!("Music {}", if settings.music { "on" } else { "off" })
                            }
                            Err(e) => e,
                        };
                        notice_time = 4.;
                    }
                    PauseAction::Sound => {
                        settings.sound = !settings.sound;
                        notice = match storage::write_settings(&store, &settings) {
                            Ok(()) => format!(
                                "Sound effects {}",
                                if settings.sound { "on" } else { "off" }
                            ),
                            Err(e) => e,
                        };
                        notice_time = 4.;
                    }
                    PauseAction::Download => {
                        let playing = score.now_playing(score.wanted(&game, started));
                        notice = score.save_current(playing);
                        notice_time = 8.;
                    }
                    PauseAction::ReturnTitle => {
                        started = false;
                        paused = false;
                        title_selection = 0;
                    }
                    PauseAction::Quit => quit = true,
                }
                nav.reset();
            }
        } else if !started {
            if pause_press {
                paused = true;
                selection = 0;
                consume = true;
                nav.reset();
            } else {
                if menu_step.up || menu_step.down {
                    title_selection = 1 - title_selection;
                }
                if confirm {
                    consume = true;
                    if title_selection == 1 {
                        quit = true;
                    } else {
                        started = true;
                        nav.reset();
                        score.cue(0, settings.sound);
                    }
                }
            }
        } else if game.state.atlas && back {
            atlas_command = true;
            consume = true;
            nav.reset();
        } else if pause_press {
            paused = true;
            selection = 0;
            consume = true;
            input.clear();
            nav.reset();
        } else if game.outcome() != "playing" && confirm {
            if game.outcome() == "won" {
                game.open_atlas();
            } else {
                game.restart();
            }
            consume = true;
        } else if atlas_press {
            atlas_command = true;
            consume = true;
            nav.reset();
        }
        if quit {
            if persist && game.tick() > 0 {
                storage::save_slot(&store, "progress", &game)?;
            }
            println!(
                "{}",
                serde_json::json!({"client":"quit","tick":game.tick(),"hash":format!("{:016x}",game.state_hash()),"music_started":score.submitted,"music_downloads":score.downloads})
            );
            break;
        }
        if consume {
            input.clear();
            stepper.reset();
        }
        // Menu commands are edge bits too: they survive a frame without a simulation tick.
        if atlas_command && started && !paused {
            input.press(4);
        }
        if started && !paused && focused {
            let x = (i32::from(dpad[3]) - i32::from(dpad[2])
                + if pad.left_stick[0] > 0.35 {
                    1
                } else if pad.left_stick[0] < -0.35 {
                    -1
                } else {
                    0
                })
            .clamp(-1, 1);
            let y = (i32::from(dpad[1]) - i32::from(dpad[0])
                + if pad.left_stick[1] > 0.55 {
                    -1
                } else if pad.left_stick[1] < -0.55 {
                    1
                } else {
                    0
                })
            .clamp(-1, 1);
            let jump = !consume
                && (key(KeyCode::Space)
                    || pad.pressed(Button::South)
                    || press("jump")
                    || game.state.atlas && confirm);
            let cast = !consume && (key(KeyCode::X) || pad.pressed(Button::West) || press("cast"));
            let sprint = held(KeyCode::LeftShift)
                || held(KeyCode::RightShift)
                || pad.triggers[1] > 0.2
                || pad.down(Button::RightTrigger)
                || hold("run");
            let mut edges = u32::from(jump) | (u32::from(cast) * 2);
            if game.state.atlas {
                edges |= (u32::from(menu_step.up) * 8)
                    | (u32::from(menu_step.down) * 16)
                    | (u32::from(menu_step.left) * 32)
                    | (u32::from(menu_step.right) * 64);
            }
            input.feed(
                Intent {
                    x: if game.state.atlas { 0 } else { x },
                    y: if game.state.atlas { 0 } else { y },
                    sprint,
                    ..Default::default()
                },
                edges,
                [0.; 2],
            );
        } else {
            input.clear();
        }
        let ticks = if verify && started {
            60
        } else {
            stepper.advance(if started && !paused && focused {
                dt
            } else {
                0.
            })
        };
        for _ in 0..ticks {
            if verify && verification_tick >= Embertrail::VERIFY_TICKS {
                break;
            }
            let tick = input.take_tick();
            let mut intent = tick.held;
            intent.action = tick.pressed(1);
            intent.secondary = tick.pressed(2);
            if tick.pressed(4) {
                intent.action = true;
                intent.pointer = Some(Point::new(-1, 1));
            }
            if game.state.atlas {
                intent.y = i32::from(tick.pressed(16)) - i32::from(tick.pressed(8));
                intent.x = i32::from(tick.pressed(64)) - i32::from(tick.pressed(32));
            }
            if verify {
                intent = Embertrail::verification_input(verification_tick);
                verification_tick += 1;
            }
            game.step(&intent);
        }
        for cue in game.take_cues() {
            score.cue(cue, settings.sound);
        }
        let wanted = score.wanted(&game, started);
        if let Some(message) = score
            .update(wanted, settings.music, paused, focused, dt)
            .await
        {
            notice = message;
            notice_time = 12.;
        }
        save_clock += dt;
        if persist && started && game.tick() > 0 && (save_clock >= 1. || paused) {
            save_clock = 0.;
            let hash = game.state_hash();
            if saved_hash != Some(hash) {
                if let Err(error) = storage::save_slot(&store, "progress", &game) {
                    notice = error;
                    notice_time = 8.;
                }
                saved_hash = Some(hash);
            }
        }
        let mut scene = Scene::default();
        let playing = score.now_playing(wanted);
        if !started {
            game.draw_title_backdrop(&mut scene, presentation_seconds);
            title(
                &mut scene,
                &game,
                title_selection,
                score.title(playing),
                &font,
                presentation_seconds,
            );
        } else {
            game.draw(&mut scene);
        }
        if paused {
            pause(
                &mut scene,
                started,
                &settings,
                selection,
                score.title(playing),
                score.loading(playing),
                score.tracks[playing].seconds,
            );
        } else if started && game.outcome() != "playing" {
            terminal(&mut scene, game.outcome() == "won");
        }
        if notice_time > 0. && !notice.is_empty() {
            notice_time -= dt;
            scene.rect(
                220,
                Rect::new(14, 423, 772, 22),
                Color::new(0.04, 0.1, 0.13, 0.96),
            );
            // Full path goes to stdout; keep the actual in-game notice legible.
            let visible = if notice.starts_with("Saved WAV:") {
                "Song saved in Downloads / Embertrail Soundtrack"
            } else {
                &notice
            };
            scene.text(221, visible, Point::new(24, 439), 18., GOLD);
        }
        clear_background(BLACK);
        let view = Viewport::fit(800., 450., screen_width().max(1.), screen_height().max(1.));
        scene.draw(view, Point::default(), &mut renderer)?;
        if let Some(plan) = &capture {
            if plan.wants(frame) {
                let path = plan.path_for(frame);
                get_screen_data().export_png(path.to_str().ok_or("Capture path must be UTF-8")?);
                println!(
                    "{}",
                    serde_json::json!({"frame":frame,"path":path,"paused":paused,"started":started,"track":score.title(playing),"next_track":score.title(wanted),"menu_selection":selection,"atlas":game.state.atlas,"course":game.state.course,"x":game.player().x,"y":game.player().y,"vy":game.state.vy,"music_started":score.submitted,"visual_fixture":showcase})
                );
            }
        }
        if capture.as_ref().is_some_and(|c| c.finished(frame))
            || verify && verification_tick >= Embertrail::VERIFY_TICKS
        {
            println!(
                "{}",
                serde_json::json!({"tick":game.tick(),"hash":format!("{:016x}",game.state_hash()),"outcome":game.outcome(),"music_started":score.submitted,"music_downloads":score.downloads})
            );
            break;
        }
        next_frame().await;
    }
    Ok(())
}
