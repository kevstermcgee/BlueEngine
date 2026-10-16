//! Controller-first platform glue; gameplay remains in Embertrail::step.
use crate::{
    frame_profile::{FrameProfile, FrameSample},
    soundtrack::Score,
    window_focus,
};
use embertrail::{cues::Cue, Embertrail};
use macroquad::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Instant;
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
    Controls,
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
            Controls,
            RestartCheckpoint,
            RestartCourse,
            Music,
            Sound,
            Download,
            ReturnTitle,
            Quit,
        ]
    } else {
        &[ReturnTitle, Controls, Music, Sound, Download, Quit]
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
fn ui(
    scene: &mut Scene,
    value: &str,
    bounds: Rect,
    size: f32,
    line_height: i32,
    color: Color,
    centered: bool,
) {
    Embertrail::draw_ui_text(
        scene,
        210,
        value,
        bounds,
        size,
        line_height,
        color,
        centered,
    );
}
fn title(scene: &mut Scene, game: &Embertrail, selected: usize, seconds: f32) {
    ui(
        scene,
        "EMBERTRAIL",
        Rect::new(96, 42, 608, 63),
        48.,
        63,
        GOLD,
        true,
    );
    ui(
        scene,
        "A little light. A very big adventure.",
        Rect::new(110, 109, 580, 28),
        20.,
        28,
        MINT,
        true,
    );
    for (text, y, color) in [
        ("Vesper stole your village's Dawnheart.", 150, PAPER),
        ("Without its warmth, your home will freeze.", 178, PAPER),
        ("Free eight bells. Bring the Dawnheart home.", 206, MINT),
    ] {
        ui(
            scene,
            text,
            Rect::new(112, y, 576, 28),
            22.,
            28,
            color,
            true,
        );
    }
    let begin = if game.tick() > 0 {
        "CONTINUE THE TRAIL"
    } else {
        "CARRY THE FIRST SPARK"
    };
    for (index, label, rect, size) in [
        (0, begin, Rect::new(210, 264, 380, 42), 22.),
        (1, "Options", Rect::new(300, 316, 200, 32), 21.),
        (2, "Quit game", Rect::new(300, 354, 200, 32), 21.),
    ] {
        if index == 0 || selected == index {
            scene.rect(205, rect, Color::new(0.075, 0.17, 0.19, 0.95));
        }
        if selected == index {
            let pulse = 0.75 + 0.2 * (seconds * 2.5).sin();
            scene.rect(
                206,
                Rect::new(rect.x, rect.y, rect.w, 2),
                Color::new(GOLD.r, GOLD.g, GOLD.b, pulse),
            );
            scene.rect(206, Rect::new(rect.x, rect.y + rect.h - 2, rect.w, 2), GOLD);
        }
        ui(
            scene,
            label,
            Rect::new(rect.x + 16, rect.y + 5, rect.w - 32, rect.h - 8),
            size,
            rect.h - 8,
            if selected == index { GOLD } else { PAPER },
            true,
        );
    }
    ui(
        scene,
        &format!("v{}", env!("CARGO_PKG_VERSION")),
        Rect::new(708, 10, 78, 24),
        16.,
        24,
        MINT,
        true,
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
    panel(scene, Rect::new(88, 20, 624, 410));
    ui(
        scene,
        if started { "PAUSED" } else { "OPTIONS" },
        Rect::new(112, 36, 576, 36),
        30.,
        36,
        GOLD,
        false,
    );
    ui(
        scene,
        if loading {
            "Soundtrack loading"
        } else {
            "Now playing"
        },
        Rect::new(112, 76, 576, 21),
        16.,
        21,
        MINT,
        false,
    );
    ui(
        scene,
        track,
        Rect::new(112, 99, 576, 48),
        21.,
        24,
        PAPER,
        false,
    );
    for (index, action) in pause_actions(started).iter().enumerate() {
        let label = match action {
            PauseAction::Resume => "Resume".to_owned(),
            PauseAction::Atlas => "Trail atlas".to_owned(),
            PauseAction::Controls => "Controls".to_owned(),
            PauseAction::RestartCheckpoint => "Restart checkpoint".to_owned(),
            PauseAction::RestartCourse => "Restart course".to_owned(),
            PauseAction::Music => format!("Music: {}", if settings.music { "On" } else { "Off" }),
            PauseAction::Sound => format!(
                "Sound effects: {}",
                if settings.sound { "On" } else { "Off" }
            ),
            PauseAction::Download => format!(
                "Download this song (.wav)  {}:{:02}",
                length as u32 / 60,
                length as u32 % 60
            ),
            PauseAction::ReturnTitle => {
                if started {
                    "Return to title".to_owned()
                } else {
                    "Back to title".to_owned()
                }
            }
            PauseAction::Quit => "Quit game".to_owned(),
        };
        let top = 153 + index as i32 * 24;
        if selected == index {
            scene.rect(
                205,
                Rect::new(108, top, 584, 24),
                Color::new(0.17, 0.29, 0.29, 1.),
            );
            scene.rect(206, Rect::new(108, top, 4, 24), GOLD);
        }
        ui(
            scene,
            &label,
            Rect::new(126, top, 550, 24),
            20.,
            24,
            if selected == index { GOLD } else { PAPER },
            false,
        );
    }
    ui(
        scene,
        "A  Select     B  Back",
        Rect::new(112, 399, 576, 22),
        16.,
        22,
        MINT,
        true,
    );
}
fn controls(scene: &mut Scene) {
    panel(scene, Rect::new(88, 20, 624, 410));
    ui(
        scene,
        "CONTROLS",
        Rect::new(112, 38, 576, 40),
        30.,
        40,
        GOLD,
        false,
    );
    for (index, (label, binding)) in [
        ("Move", "Left stick / D-pad"),
        ("Jump", "A  (Space)"),
        ("Cast", "X  (X key)"),
        ("Run", "RT / RB  (Shift)"),
        ("Enter hollows", "Down"),
        ("Pause / options", "Menu  (Esc)"),
        ("Trail atlas", "Y / View  (Tab)"),
    ]
    .iter()
    .enumerate()
    {
        let y = 98 + index as i32 * 39;
        ui(
            scene,
            label,
            Rect::new(112, y, 222, 30),
            20.,
            30,
            PAPER,
            false,
        );
        ui(
            scene,
            binding,
            Rect::new(342, y, 346, 30),
            20.,
            30,
            MINT,
            false,
        );
    }
    ui(
        scene,
        "A or B  Back to options",
        Rect::new(112, 388, 576, 28),
        18.,
        28,
        MINT,
        true,
    );
}
fn terminal(scene: &mut Scene, won: bool) {
    panel(scene, Rect::new(96, 104, 608, 256));
    ui(
        scene,
        if won {
            "Every window holds the dawn"
        } else {
            "Your spark is still here"
        },
        Rect::new(120, 122, 560, 72),
        28.,
        36,
        GOLD,
        false,
    );
    ui(
        scene,
        if won {
            "The bells are free. Your village is warm again."
        } else {
            "A setback is another beginning. Keep your discoveries."
        },
        Rect::new(120, 199, 560, 56),
        20.,
        28,
        PAPER,
        false,
    );
    ui(
        scene,
        if won {
            "A  Return to the trail atlas"
        } else {
            "A  Try this course again"
        },
        Rect::new(120, 262, 560, 32),
        22.,
        32,
        MINT,
        false,
    );
    ui(
        scene,
        "Menu  Options, soundtrack and quit",
        Rect::new(120, 314, 560, 28),
        18.,
        28,
        PAPER,
        false,
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
    let mut profile = FrameProfile::from_args(&args)?;
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
    let showcase_grotto = flag_value(&args, "--showcase-grotto");
    let showcase_layout = has_flag(&args, "--showcase-layout");
    let showcase_count = usize::from(showcase_world.is_some())
        + usize::from(showcase_boss.is_some())
        + usize::from(showcase_secret.is_some())
        + usize::from(showcase_ending)
        + usize::from(showcase_grotto.is_some())
        + usize::from(showcase_layout);
    let showcase = showcase_count > 0;
    if showcase {
        if capture.is_none() || verify || showcase_count != 1 {
            return Err("Showcase fixtures require --capture, one fixture, and no --verify; they are visual review only".into());
        }
        let world = showcase_world
            .or(showcase_boss)
            .or(showcase_secret)
            .or(showcase_grotto)
            .unwrap_or("0")
            .parse::<usize>()
            .ok()
            .filter(|world| *world < 8)
            .ok_or("Showcase world must be 0 through 7")?;
        game.state.unlocked = 31;
        game.enter_course(
            world * 4
                + if showcase_grotto.is_some() {
                    flag_value(&args, "--grotto-variant")
                        .unwrap_or("0")
                        .parse::<usize>()
                        .ok()
                        .filter(|v| *v < 4)
                        .ok_or("Grotto variant must be0through3")?
                } else if showcase_boss.is_some() {
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
        } else if showcase_grotto.is_some() {
            let room = game.state.level.grotto.clone();
            game.state.secret_inside = true;
            game.state.level.secret_used = true;
            let x = match flag_value(&args, "--grotto-view").unwrap_or("middle") {
                "entrance" => room.entrance.x,
                "middle" => room.bounds.x + room.bounds.w / 2,
                "reward" => room.reward.x - 80,
                "exit" => room.bounds.x + room.bounds.w - 112,
                _ => return Err("Grotto view must be entrance, middle, reward or exit".into()),
            };
            game.state.px = x * embertrail::SUB;
            game.state.py = 338 * embertrail::SUB;
            game.state.banner = embertrail::levels::GROTTO_NAMES[world].into();
            game.state.banner_ticks = 300;
        } else if showcase_secret.is_some() {
            game.state.px = (game.state.level.secret.x - 190).max(0) * embertrail::SUB;
        } else {
            game.state.px = 650 * embertrail::SUB;
        }
        game.state.camera = if showcase_grotto.is_some() {
            let r = game.state.level.grotto.bounds;
            (game.player().x - 280).clamp(r.x, r.x + r.w - 800)
        } else {
            (game.player().x - 280).clamp(0, game.state.level.width - 800)
        };
        game.state.shield = 1200;
        println!(
            "{}",
            serde_json::json!({"visual_fixture":true,"world":world,"boss":showcase_boss.is_some(),"secret":showcase_secret.is_some(),"ending":showcase_ending,"inside_grotto":showcase_grotto.is_some(),"layout_review":showcase_layout,"gameplay_evidence":false})
        );
    }
    let mut started = verify
        || showcase
        || (capture.is_some()
            && !has_flag(&args, "--title")
            && flag_value(&args, "--script").is_none());
    let mut paused = false;
    let mut show_controls = false;
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
    Embertrail::set_ui_font(font);
    let prewarm_started = Instant::now();
    Embertrail::prewarm_presentation_assets();
    let prewarmed_assets = Embertrail::take_presentation_asset_events();
    if profile.enabled() {
        println!(
            "{}",
            serde_json::json!({"presentation_prewarm_ms":prewarm_started.elapsed().as_secs_f64()*1000.,"assets":prewarmed_assets})
        );
    }
    let mut layout_track = None;
    if showcase_layout {
        let mut candidates = Vec::new();
        for world in 0..8 {
            for course in 0..4 {
                let value = format!(
                    "{}: {}",
                    embertrail::story::NPC_NAMES[world],
                    embertrail::story::NPC_LINES[world][course]
                );
                candidates.push((
                    Embertrail::ui_text_width(&value, 18.),
                    world * 4 + course,
                    value,
                ));
            }
        }
        let (_, course, value) = candidates.into_iter().max_by_key(|entry| entry.0).unwrap();
        game.enter_course(course);
        game.state.unlocked = 31;
        game.state.npc_greeted = true;
        game.state.banner = value;
        game.state.banner_ticks = 3600;
        layout_track = score
            .tracks
            .iter()
            .enumerate()
            .max_by_key(|(_, t)| Embertrail::ui_text_width(&t.title, 21.))
            .map(|(i, _)| i);
        println!(
            "{}",
            serde_json::json!({"layout_review":true,"story":game.state.banner,"track":layout_track.map(|i|score.title(i))})
        );
    }
    let mut frame = 0;
    let mut verification_tick = 0;
    let mut save_clock = 0.;
    let mut saved_hash = None;
    let mut was_focused = true;
    let mut fullscreen = false;
    let mut presentation_seconds = 0.;
    prevent_quit();
    'client: loop {
        frame += 1;
        let mut frame_clock = Instant::now();
        let dt_ms = f64::from(get_frame_time()) * 1000.;
        let sprite_misses_before = Embertrail::presentation_asset_misses();
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
        let menu_navigation = if paused {
            !show_controls && (menu_step.up || menu_step.down)
        } else if !started {
            menu_step.up || menu_step.down
        } else {
            game.state.atlas
                && (menu_step.up || menu_step.down || menu_step.left || menu_step.right)
        };
        if menu_navigation {
            score.cue(Cue::MenuNavigate as usize, settings.sound);
        }
        if paused && show_controls {
            if back || pause_press || confirm {
                show_controls = false;
                consume = true;
                nav.reset();
                score.cue(Cue::MenuBack as usize, settings.sound);
            }
        } else if paused {
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
                score.cue(Cue::MenuBack as usize, settings.sound);
            } else if confirm || press("download") {
                consume = true;
                score.cue(Cue::MenuConfirm as usize, settings.sound);
                let selected = if press("download") {
                    PauseAction::Download
                } else {
                    actions[selection]
                };
                match selected {
                    PauseAction::Resume => paused = false,
                    PauseAction::Controls => show_controls = true,
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
                show_controls = false;
                selection = 0;
                consume = true;
                nav.reset();
                score.cue(Cue::MenuConfirm as usize, settings.sound);
            } else {
                if menu_step.up {
                    title_selection = (title_selection + 2) % 3;
                }
                if menu_step.down {
                    title_selection = (title_selection + 1) % 3;
                }
                if confirm {
                    consume = true;
                    score.cue(Cue::MenuConfirm as usize, settings.sound);
                    match title_selection {
                        2 => quit = true,
                        1 => {
                            paused = true;
                            show_controls = false;
                            selection = 0;
                        }
                        _ => {
                            started = true;
                            nav.reset();
                        }
                    }
                }
            }
        } else if game.state.atlas && back {
            score.cue(Cue::MenuBack as usize, settings.sound);
            atlas_command = true;
            consume = true;
            nav.reset();
        } else if pause_press {
            score.cue(Cue::MenuConfirm as usize, settings.sound);
            show_controls = false;
            paused = true;
            selection = 0;
            consume = true;
            input.clear();
            nav.reset();
        } else if game.outcome() != "playing" && confirm {
            score.cue(Cue::MenuConfirm as usize, settings.sound);
            if game.outcome() == "won" {
                game.open_atlas();
            } else {
                game.restart();
            }
            consume = true;
        } else if atlas_press {
            score.cue(Cue::MenuConfirm as usize, settings.sound);
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
                serde_json::json!({"client":"quit","tick":game.tick(),"hash":format!("{:016x}",game.state_hash()),"music_started":score.submitted,"music_downloads":score.downloads,"effects_submitted":score.effects_submitted,"effect_counts":score.effect_counts,"last_effect":score.last_effect,"decode_count":score.decode_count,"last_decode_ms":score.last_decode_ms,"max_decode_ms":score.max_decode_ms,"decoded_tracks":score.decoded_tracks,"max_decoded_tracks":score.max_decoded_tracks,"decoded_pcm_bytes":score.decoded_pcm_bytes,"max_decoded_pcm_bytes":score.max_decoded_pcm_bytes})
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
        // Preparation has its own rendered frames. The public input/capture
        // frame clock remains still, and no action is buffered into gameplay.
        while !score.context_ready(&game, started) {
            input.clear();
            stepper.reset();
            let preparing_presentation = Instant::now();
            let mut loading_scene = Scene::default();
            if started {
                game.draw(&mut loading_scene);
            } else {
                game.draw_title_backdrop(&mut loading_scene, presentation_seconds);
            }
            loading_scene.rect(
                300,
                Rect::new(204, 178, 392, 78),
                Color::new(0.03, 0.1, 0.14, 0.98),
            );
            Embertrail::draw_ui_text(
                &mut loading_scene,
                301,
                "Kindling the trail…",
                Rect::new(224, 194, 352, 46),
                24.,
                46,
                GOLD,
                true,
            );
            let presentation_ms = preparing_presentation.elapsed().as_secs_f64() * 1000.;
            clear_background(BLACK);
            let preparing_renderer = Instant::now();
            loading_scene.draw(
                Viewport::fit(800., 450., screen_width().max(1.), screen_height().max(1.)),
                Point::default(),
                &mut renderer,
            )?;
            let renderer_ms = preparing_renderer.elapsed().as_secs_f64() * 1000.;
            next_frame().await;
            let preparing_audio = Instant::now();
            if let Some(message) = score.prepare_context(&game, started).await {
                notice = message;
                notice_time = 10.;
            }
            let audio_update_ms = preparing_audio.elapsed().as_secs_f64() * 1000.;
            if profile.record(FrameSample {
                frame,
                dt_ms: f64::from(get_frame_time()) * 1000.,
                gameplay: false,
                preparing: true,
                simulation_ms: 0.,
                presentation_ms,
                renderer_ms,
                audio_update_ms,
                save_ms: 0.,
                cpu_ms: presentation_ms + renderer_ms + audio_update_ms,
                sprite_cache_misses: Embertrail::presentation_asset_misses() - sprite_misses_before,
            }) {
                break 'client;
            }
            if is_quit_requested() {
                break 'client;
            }
            if score.context_ready(&game, started) {
                // Consume the final decoding interval while the preparation
                // card is still visible. The next interactive frame must not
                // count that work as elapsed simulation time.
                clear_background(BLACK);
                loading_scene.draw(
                    Viewport::fit(800., 450., screen_width().max(1.), screen_height().max(1.)),
                    Point::default(),
                    &mut renderer,
                )?;
                next_frame().await;
                input.clear();
                stepper.reset();
            }
            frame_clock = Instant::now();
        }
        let simulation_clock = Instant::now();
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
        let simulation_ms = simulation_clock.elapsed().as_secs_f64() * 1000.;
        let audio_clock = Instant::now();
        for cue in game.take_cues() {
            score.cue(cue, settings.sound);
        }
        let wanted = layout_track.unwrap_or_else(|| score.wanted(&game, started));
        if let Some(message) = score
            .update(wanted, settings.music, paused, focused, dt)
            .await
        {
            notice = message;
            notice_time = 12.;
        }
        let audio_update_ms = audio_clock.elapsed().as_secs_f64() * 1000.;
        let save_started = Instant::now();
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
        let save_ms = save_started.elapsed().as_secs_f64() * 1000.;
        let presentation_clock = Instant::now();
        let mut scene = Scene::default();
        let playing = score.now_playing(wanted);
        if !started {
            game.draw_title_backdrop(&mut scene, presentation_seconds);
            if !paused {
                title(&mut scene, &game, title_selection, presentation_seconds);
            }
        } else {
            game.draw(&mut scene);
        }
        if paused && show_controls {
            controls(&mut scene);
        } else if paused {
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
            let visible = if notice.starts_with("Saved WAV:") {
                "Song saved to Downloads / Embertrail Soundtrack"
            } else {
                &notice
            };
            let width = (Embertrail::ui_text_width(visible, 18.) + 28).clamp(260, 680);
            let lines = Embertrail::ui_text_lines(visible, width - 28, 18.)
                .len()
                .min(2) as i32;
            let height = lines * 24 + 20;
            let bounds = if paused && !show_controls {
                // A temporary status replaces the song header area rather
                // than covering menu actions or their Back hint.
                Rect::new(108, 74, 584, 76)
            } else {
                Rect::new((800 - width) / 2, 438 - height, width, height)
            };
            scene.rect(220, bounds, Color::new(0.04, 0.1, 0.13, 0.98));
            Embertrail::draw_ui_text(
                &mut scene,
                221,
                visible,
                Rect::new(bounds.x + 14, bounds.y + 10, bounds.w - 28, bounds.h - 20),
                18.,
                24,
                GOLD,
                false,
            );
        }
        let presentation_ms = presentation_clock.elapsed().as_secs_f64() * 1000.;
        clear_background(BLACK);
        let renderer_clock = Instant::now();
        let view = Viewport::fit(800., 450., screen_width().max(1.), screen_height().max(1.));
        scene.draw(view, Point::default(), &mut renderer)?;
        let renderer_ms = renderer_clock.elapsed().as_secs_f64() * 1000.;
        let profile_finished = profile.record(FrameSample {
            frame,
            dt_ms,
            gameplay: started
                && !paused
                && !game.state.atlas
                && focused
                && game.outcome() == "playing",
            preparing: false,
            simulation_ms,
            presentation_ms,
            renderer_ms,
            audio_update_ms,
            save_ms,
            cpu_ms: frame_clock.elapsed().as_secs_f64() * 1000.,
            sprite_cache_misses: Embertrail::presentation_asset_misses() - sprite_misses_before,
        });
        let first_use_assets = Embertrail::take_presentation_asset_events();
        if profile.enabled() && !first_use_assets.is_empty() {
            println!(
                "{}",
                serde_json::json!({"first_use_presentation_assets":first_use_assets,"frame":frame})
            );
        }
        if let Some(plan) = &capture {
            if plan.wants(frame) {
                let path = plan.path_for(frame);
                get_screen_data().export_png(path.to_str().ok_or("Capture path must be UTF-8")?);
                println!(
                    "{}",
                    serde_json::json!({"frame":frame,"path":path,"paused":paused,"started":started,"track":score.title(playing),"next_track":score.title(wanted),"menu_selection":selection,"atlas":game.state.atlas,"course":game.state.course,"x":game.player().x,"y":game.player().y,"vy":game.state.vy,"music_started":score.submitted,"visual_fixture":showcase,"controls_page":show_controls,"effects_submitted":score.effects_submitted,"effect_counts":score.effect_counts,"last_effect":score.last_effect,"decode_count":score.decode_count,"last_decode_ms":score.last_decode_ms,"max_decode_ms":score.max_decode_ms,"decoded_tracks":score.decoded_tracks,"max_decoded_tracks":score.max_decoded_tracks,"decoded_pcm_bytes":score.decoded_pcm_bytes,"max_decoded_pcm_bytes":score.max_decoded_pcm_bytes})
                );
            }
        }
        if capture.as_ref().is_some_and(|c| c.finished(frame))
            || verify && verification_tick >= Embertrail::VERIFY_TICKS
            || profile_finished
        {
            println!(
                "{}",
                serde_json::json!({"tick":game.tick(),"hash":format!("{:016x}",game.state_hash()),"outcome":game.outcome(),"music_started":score.submitted,"music_downloads":score.downloads,"effects_submitted":score.effects_submitted,"effect_counts":score.effect_counts,"last_effect":score.last_effect,"decode_count":score.decode_count,"last_decode_ms":score.last_decode_ms,"max_decode_ms":score.max_decode_ms,"decoded_tracks":score.decoded_tracks,"max_decoded_tracks":score.max_decoded_tracks,"decoded_pcm_bytes":score.decoded_pcm_bytes,"max_decoded_pcm_bytes":score.max_decoded_pcm_bytes})
            );
            break;
        }
        next_frame().await;
    }
    if let Some(summary) = profile.summary() {
        println!("{summary}");
    }
    score.shutdown();
    Embertrail::release_presentation_assets();
    Ok(())
}
