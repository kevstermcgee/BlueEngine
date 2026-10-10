//! Shared lifecycle and devices; games own interface appearance and semantic sounds.
use super::audio::Audio;
use super::draw::{GOLD, PINK, WHITE};
use super::{
    draw::*,
    ui::{Action, Layout, Lifecycle, Screen},
    Intent, Point, Rect,
};
use crate::runtime::{
    storage::{self, PlatformStorage, Storage},
    FixedStepper, InputAccumulator,
};
use crate::viewer::devkit::{MenuNav, Timeline};
use macroquad::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
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

/// Prototype defaults remain convenient; games can change these or replace Game::interface.
pub struct Theme {
    pub background: Color,
    pub panel: Color,
    pub text: Color,
    pub accent: Color,
    pub font: Option<&'static str>,
    pub panel_bounds: Rect,
    pub padding: i32,
    pub heading_size: f32,
    pub body_size: f32,
    /// Optional cosmetic pulse, seconds per cycle; zero disables it.
    pub pulse_seconds: f32,
}
impl Default for Theme {
    fn default() -> Self {
        Self {
            background: INK,
            panel: Color::new(0.1, 0.16, 0.22, 0.96),
            text: WHITE,
            accent: GOLD,
            font: None,
            panel_bounds: Rect::new(145, 135, 510, 180),
            padding: 35,
            heading_size: 34.,
            body_size: 22.,
            pulse_seconds: 0.,
        }
    }
}
/// Read-only presentation frame. All coordinates and font metrics use logical pixels.
pub struct UiFrame<'a> {
    pub screen: Screen,
    pub focused: bool,
    pub pointer: Option<Point>,
    pub selected: usize,
    pub elapsed: f32,
    pub canvas: [f32; 2],
    pub sound: bool,
    pub music: bool,
    pub notice: &'a str,
    pub status: &'a str,
    pub fonts: &'a Renderer,
}
pub fn default_interface<G: Game>(_: &G, scene: &mut Scene, frame: &UiFrame) -> Layout {
    let theme = G::theme();
    let text = |scene: &mut Scene, layer, label: &str, p, size, color| {
        if let Some(font) = theme.font {
            scene.text_with_font(layer, label, p, size, color, font);
        } else {
            scene.text(layer, label, p, size, color);
        }
    };
    if G::show_hud() {
        text(scene, 100, G::TITLE, Point::new(24, 30), 26., theme.text);
        text(
            scene,
            100,
            G::CONTROLS,
            Point::new(24, 428),
            16.,
            theme.text,
        );
        text(
            scene,
            100,
            "Esc pause · M sound · K save · L load · R restart",
            Point::new(24, 448),
            14.,
            theme.accent,
        );
    }
    if !frame.notice.is_empty() {
        text(
            scene,
            101,
            frame.notice,
            Point::new(24, 395),
            19.,
            theme.accent,
        );
    }
    let mut layout = Layout::default();
    if frame.screen != Screen::Playing {
        let panel = theme.panel_bounds;
        scene.rect(200, panel, theme.panel);
        let (heading, action) = match frame.screen {
            Screen::Start => ("CLICK OR ENTER TO START", Action::Start),
            Screen::Paused => ("RESUME", Action::Resume),
            Screen::Won => ("COMPLETE! · RESTART", Action::Restart),
            _ => ("TRY AGAIN", Action::Restart),
        };
        let mut accent = theme.accent;
        if theme.pulse_seconds > 0. {
            accent.a *=
                0.85 + 0.15 * (frame.elapsed * std::f32::consts::TAU / theme.pulse_seconds).cos();
        }
        text(
            scene,
            201,
            heading,
            Point::new(panel.x + theme.padding, panel.y + 55),
            theme.heading_size,
            accent,
        );
        text(
            scene,
            201,
            "R restarts · K saves · L loads",
            Point::new(panel.x + theme.padding, panel.y + 103),
            theme.body_size,
            theme.text,
        );
        text(
            scene,
            201,
            frame.status,
            Point::new(panel.x + theme.padding, panel.y + 139),
            theme.body_size,
            theme.text,
        );
        layout.button(Rect::new(panel.x, panel.y, panel.w, 80), action);
    }
    layout
}

fn execute_action<G: Game>(
    action: Action,
    game: &mut G,
    ui: &mut Lifecycle,
    settings: &mut Settings,
    notice: &mut String,
    store: &impl Storage,
) -> Option<&'static str> {
    if !ui.apply(action, game.outcome()) {
        return None;
    }
    let event = match action {
        Action::Start => "ui.start",
        Action::Resume => "ui.resume",
        Action::TogglePause => {
            if ui.screen(game.outcome()) == Screen::Paused {
                "ui.pause"
            } else {
                "ui.resume"
            }
        }
        Action::Restart => {
            game.restart();
            notice.clear();
            "ui.restart"
        }
        Action::Save => {
            *notice = match storage::save(store, game) {
                Ok(()) => "Game saved. L resumes it.".into(),
                Err(e) => e,
            };
            "ui.save"
        }
        Action::Load => {
            *notice = match storage::load(store, game) {
                Ok(true) => "Game resumed".into(),
                Ok(false) => "No save yet. K saves.".into(),
                Err(e) => e,
            };
            "ui.load"
        }
        Action::ToggleSound | Action::ToggleMusic => {
            let (label, enabled) = if action == Action::ToggleSound {
                settings.sound = !settings.sound;
                ("Sound", settings.sound)
            } else {
                settings.music = !settings.music;
                ("Music", settings.music)
            };
            *notice = match storage::write_settings(store, settings) {
                Ok(()) => format!("{label} {} — saved", if enabled { "on" } else { "off" }),
                Err(e) => e,
            };
            "ui.settings"
        }
        Action::Quit => "ui.quit",
    };
    Some(event)
}
pub fn config(title: &str) -> macroquad::conf::Conf {
    #[allow(unused_mut)]
    let mut size = (960_u32, 540_u32);
    {
        let args: Vec<_> = std::env::args().collect();
        if crate::runtime::playback::has_flag(&args, "--size") {
            size = crate::runtime::playback::flag_value(&args, "--size")
                .and_then(crate::runtime::playback::parse_size)
                .filter(|(w, h)| *w >= 160 && *h >= 120 && *w <= 4096 && *h <= 4096)
                .unwrap_or_else(|| {
                    eprintln!("--size requires WIDTHxHEIGHT (160×120 through 4096×4096)");
                    std::process::exit(2)
                });
        }
    }
    macroquad::conf::Conf {
        miniquad_conf: Conf {
            window_title: title.into(),
            window_width: size.0 as i32,
            window_height: size.1 as i32,
            high_dpi: true,
            ..Default::default()
        },
        ..Default::default()
    }
}
pub async fn run<G: Game>() {
    run_with_focus::<G>(platform::focused).await;
}
/// Native hosts can supply their existing foreground query, without placing OS calls in game rules.
pub async fn run_with_focus<G: Game>(focused: fn() -> bool) {
    if let Err(error) = run_inner::<G>(focused).await {
        platform::error(&error);
        std::process::exit(1);
    }
}
async fn run_inner<G: Game>(focused: fn() -> bool) -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    let capture =
        crate::runtime::playback::CapturePlan::from_args(&args).map_err(|e| e.to_string())?;
    if let Some(plan) = &capture {
        plan.create_dir().map_err(|e| e.to_string())?;
    }
    let script = match crate::runtime::playback::flag_value(&args, "--script") {
        Some(text) => Some(Timeline::parse(
            text,
            &[
                "start", "resume", "pause", "restart", "save", "load", "sound", "music", "quit",
                "left", "right", "up", "down", "action", "pointer", "click", "focus",
            ],
        )?),
        None if crate::runtime::playback::has_flag(&args, "--script") => {
            return Err("--script requires a timeline".into())
        }
        None => None,
    };
    let store = PlatformStorage::new(G::ID)?;
    let (mut settings, mut notice) = match storage::read_settings::<Settings>(&store) {
        Ok(s) => (s, String::new()),
        Err(e) => (
            Settings::default(),
            format!("{e}; playing with session defaults"),
        ),
    };
    let verification = platform::verify_mode();
    if verification && script.is_some() {
        return Err("Choose --verify or --script, not both".into());
    }
    let persist_progress = !verification && capture.is_none() && script.is_none();
    let unattended = verification || capture.is_some() || script.is_some();
    let mut game = G::new(7);
    if persist_progress {
        match storage::load_slot(&store, "progress", &mut game) {
            Ok(true) => notice = "Progress restored. Start to continue.".into(),
            Ok(false) => {}
            Err(e) => notice = e,
        }
    }
    let mut ui = Lifecycle::new(verification || (capture.is_some() && script.is_none()));
    let mut audio = Audio::new::<G>(platform::muted()).await?;
    let mut renderer = Renderer::default();
    renderer.load_fonts(G::fonts()).await?;
    let mut stepper = FixedStepper::new();
    let mut inputs = InputAccumulator::<Intent>::new();
    let mut pointer_actions = ActionPointer::default();
    let mut particles = Particles::new(8);
    let mut nav = MenuNav::default();
    let mut selected = 0;
    let mut verification_tick = 0;
    let mut frame = 0;
    let mut elapsed = 0.;
    let mut last_pointer = None;
    let mut fullscreen = false;
    let mut autosave_time = 0.;
    let mut autosave_hash = None;
    #[cfg(feature = "gamepad")]
    let mut pads = crate::viewer::gamepad::Gamepads::new().ok();
    loop {
        frame += 1;
        if is_key_pressed(KeyCode::F) {
            fullscreen = !fullscreen;
            macroquad::miniquad::window::set_fullscreen(fullscreen);
        }
        let dt = if capture.is_some() || script.is_some() {
            1. / 60.
        } else {
            get_frame_time().clamp(0., 0.1)
        };
        elapsed += dt;
        let view = Viewport::fit(800., 450., screen_width().max(1.), screen_height().max(1.));
        let (mx, my) = mouse_position();
        let mut pointer = view.pointer(mx, my);
        let mut x = i32::from(is_key_down(KeyCode::D) || is_key_down(KeyCode::Right))
            - i32::from(is_key_down(KeyCode::A) || is_key_down(KeyCode::Left));
        let mut y = i32::from(is_key_down(KeyCode::S) || is_key_down(KeyCode::Down))
            - i32::from(is_key_down(KeyCode::W) || is_key_down(KeyCode::Up));
        let mut click = is_mouse_button_pressed(MouseButton::Left);
        let mut action = platform::primary_key() || click;
        let mut select = is_key_pressed(KeyCode::Enter) || platform::primary_key();
        #[allow(unused_mut)]
        let mut stick = [0.; 2];
        #[allow(unused_mut)]
        let mut dpad = [
            is_key_down(KeyCode::Up),
            is_key_down(KeyCode::Down),
            is_key_down(KeyCode::Left),
            is_key_down(KeyCode::Right),
        ];
        #[allow(unused_mut)]
        let mut pad_look = [0.; 2];
        #[cfg(feature = "gamepad")]
        if let Some(pads) = pads.as_mut() {
            let pad = pads.poll(focused());
            x += (pad.left_stick[0] * 1.5) as i32;
            y -= (pad.left_stick[1] * 1.5) as i32;
            action |= pad.menu_select();
            select |= pad.menu_select();
            stick = pad.left_stick;
            for (key, pad) in dpad.iter_mut().zip(pad.dpad()) {
                *key |= pad;
            }
            if G::drag_look() {
                pad_look = [pad.right_stick[0] * dt * 2., pad.right_stick[1] * dt * 2.];
            }
        }
        let mut command = [
            (KeyCode::R, Action::Restart),
            (KeyCode::Escape, Action::TogglePause),
            (KeyCode::M, Action::ToggleSound),
            (KeyCode::N, Action::ToggleMusic),
            (KeyCode::K, Action::Save),
            (KeyCode::L, Action::Load),
        ]
        .into_iter()
        .find(|(key, _)| platform::command_key(*key))
        .map(|(_, a)| a);
        // Script/capture runs use the same action path without depending on which
        // CI window owns foreground. The focus cue still exercises focus gating.
        let mut has_focus = unattended || focused();
        if let Some(script) = &script {
            let held = |name| script.active(frame).any(|c| c.name == name);
            let edge = |name| script.starting(frame).any(|c| c.name == name);
            x = i32::from(held("right")) - i32::from(held("left"));
            y = i32::from(held("down")) - i32::from(held("up"));
            action = edge("action");
            click = edge("click");
            select = false;
            if let Some(cue) = script
                .active(frame)
                .find(|c| c.name == "pointer" || c.name == "click")
            {
                pointer = Some(Point::new(
                    cue.values.first().copied().unwrap_or(0.) as i32,
                    cue.values.get(1).copied().unwrap_or(0.) as i32,
                ));
            }
            if let Some(cue) = script.active(frame).find(|c| c.name == "focus") {
                has_focus = cue.values.first().is_none_or(|v| *v != 0.);
            }
            command = [
                ("start", Action::Start),
                ("resume", Action::Resume),
                ("pause", Action::TogglePause),
                ("restart", Action::Restart),
                ("save", Action::Save),
                ("load", Action::Load),
                ("sound", Action::ToggleSound),
                ("music", Action::ToggleMusic),
                ("quit", Action::Quit),
            ]
            .into_iter()
            .find(|(name, _)| edge(name))
            .map(|(_, a)| a);
        }
        action |= click;
        let prior_screen = ui.screen(game.outcome());
        ui.focus(has_focus);
        let status = game.menu_status();
        let ui_frame = UiFrame {
            screen: ui.screen(game.outcome()),
            focused: has_focus,
            pointer,
            selected,
            elapsed,
            canvas: [800., 450.],
            sound: settings.sound,
            music: settings.music,
            notice: &notice,
            status: &status,
            fonts: &renderer,
        };
        let layout = game.interface(&mut Scene::default(), &ui_frame);
        let menu_step = nav.update(dt, stick, dpad);
        if ui_frame.screen != Screen::Playing && !layout.is_empty() {
            if menu_step.up || menu_step.left {
                selected = (selected + layout.len() - 1) % layout.len();
            }
            if menu_step.down || menu_step.right {
                selected = (selected + 1) % layout.len();
            }
            selected = selected.min(layout.len() - 1);
            command = command.or_else(|| {
                if click {
                    pointer.and_then(|p| layout.hit(p))
                } else if select {
                    layout.selected(selected)
                } else {
                    None
                }
            });
        } else {
            selected = 0;
            // Custom playing HUDs may expose pause/settings buttons too.
            command = command.or_else(|| {
                if click {
                    pointer.and_then(|p| layout.hit(p))
                } else {
                    None
                }
            });
        }
        let mut consumed = false;
        if let Some(command) = command {
            if let Some(event) = execute_action(
                command,
                &mut game,
                &mut ui,
                &mut settings,
                &mut notice,
                &store,
            ) {
                consumed = true;
                inputs.clear();
                pointer_actions.clear();
                last_pointer = None;
                stepper = FixedStepper::new();
                audio.event::<G>(
                    event,
                    if command == Action::Start {
                        Some(0)
                    } else {
                        None
                    },
                    settings.sound,
                )?;
                if command == Action::Restart {
                    verification_tick = 0;
                }
                if command == Action::Quit {
                    break;
                }
            }
        }
        let accepting = ui.accepting_input(game.outcome()) && !consumed;
        if prior_screen != ui.screen(game.outcome()) {
            selected = 0;
            nav.reset();
        }
        let mut look = pad_look;
        if accepting
            && G::drag_look()
            && is_mouse_button_down(MouseButton::Left)
            && pointer.is_some()
        {
            if let Some((last_x, last_y)) = last_pointer {
                look[0] += (mx - last_x) * 0.003;
                look[1] += (last_y - my) * 0.003;
            }
            last_pointer = Some((mx, my));
        } else {
            last_pointer = None;
        }
        let mut intent = if accepting {
            game.device_input(Intent {
                x: x.clamp(-1, 1),
                y: y.clamp(-1, 1),
                pointer: if G::pointer_target_only_on_press() && !click {
                    None
                } else {
                    pointer
                },
                action,
                sprint: platform::sprint_key(),
                look: [0.; 2],
            })
        } else {
            Intent::default()
        };
        action = intent.action;
        if G::pointer_target_only_on_press() {
            pointer_actions.feed(
                action,
                intent.pointer,
                click || intent.pointer != pointer,
                false,
            );
        }
        intent.action = false;
        if !accepting {
            inputs.clear();
            pointer_actions.clear();
        }
        inputs.feed(
            intent,
            u32::from(action && accepting),
            if accepting { look } else { [0.; 2] },
        );
        let ticks = if verification && accepting {
            60
        } else {
            stepper.advance(if accepting { dt } else { 0. })
        };
        for _ in 0..ticks {
            if verification && verification_tick >= G::VERIFY_TICKS {
                break;
            }
            let tick = inputs.take_tick();
            let mut intent = tick.held;
            intent.action = tick.pressed(1);
            intent.look = tick.look;
            if G::pointer_target_only_on_press() {
                intent.pointer = pointer_actions.take(intent.action);
            }
            if verification {
                intent = G::verification_input(verification_tick);
                verification_tick += 1;
            }
            game.step(&intent);
            // Consume events per tick, preserving catch-up transitions and their location.
            for cue in game.take_cues() {
                audio.event::<G>(
                    G::cue_event(cue).unwrap_or("legacy"),
                    Some(cue),
                    settings.sound,
                )?;
                if G::cue_particles() {
                    particles.burst(game.cue_point(cue), if cue == 1 { PINK } else { GOLD });
                }
            }
            for event in game.take_audio_events() {
                audio.event::<G>(event, None, settings.sound)?;
            }
            if game.outcome() != "playing" {
                break;
            }
        }
        audio.update(
            &game,
            ui.accepting_input(game.outcome()),
            settings.sound,
            settings.music,
            dt,
        )?;
        autosave_time += dt;
        if persist_progress
            && ui.started()
            && game.tick() > 0
            && (autosave_time >= 1. || !ui.accepting_input(game.outcome()))
        {
            autosave_time = 0.;
            let hash = game.state_hash();
            if autosave_hash != Some(hash) {
                match storage::save_slot(&store, "progress", &game) {
                    Ok(()) => {}
                    Err(e) => notice = format!("Autosave failed: {e}"),
                }
                autosave_hash = Some(hash);
            }
        }
        let mut scene = Scene::default();
        scene.rect(-100, Rect::new(0, 0, 800, 450), G::theme().background);
        game.draw(&mut scene);
        particles.update(dt, &mut scene);
        let status = game.menu_status();
        game.interface(
            &mut scene,
            &UiFrame {
                screen: ui.screen(game.outcome()),
                focused: has_focus,
                pointer,
                selected,
                elapsed,
                canvas: [800., 450.],
                sound: settings.sound,
                music: settings.music,
                notice: &notice,
                status: &status,
                fonts: &renderer,
            },
        );
        clear_background(BLACK);
        scene.draw(view, Point::default(), &mut renderer)?;
        if let Some(plan) = &capture {
            if plan.wants(frame) {
                let path = plan.path_for(frame);
                get_screen_data().export_png(path.to_str().ok_or("Capture path must be UTF-8")?);
                println!(
                    "{}",
                    serde_json::json!({"frame":frame,"path":path,"width":screen_width(),"height":screen_height(),"screen":ui.screen(game.outcome()),"tick":game.tick(),"hash":format!("{:016x}",game.state_hash()),"audio":audio.evidence()})
                );
            }
        }
        if capture.as_ref().is_some_and(|c| c.finished(frame))
            || (capture.is_none()
                && verification
                && (verification_tick >= G::VERIFY_TICKS || game.outcome() != "playing"))
        {
            println!(
                "{}",
                serde_json::json!({"tick":game.tick(),"hash":format!("{:016x}",game.state_hash()),"outcome":game.outcome(),"audio":audio.evidence()})
            );
            break;
        }
        next_frame().await;
    }
    Ok(())
}
/// The action target travels with its press edge across frames with no fixed tick.
#[derive(Default)]
struct ActionPointer {
    target: Option<Point>,
}
impl ActionPointer {
    fn feed(&mut self, action: bool, pointer: Option<Point>, clicked: bool, tapped: bool) {
        if action {
            self.target = if clicked || tapped { pointer } else { None };
        }
    }
    fn take(&mut self, action: bool) -> Option<Point> {
        if action {
            self.target.take()
        } else {
            None
        }
    }
    fn clear(&mut self) {
        self.target = None;
    }
}

mod platform {
    pub fn muted() -> bool {
        let args: Vec<_> = std::env::args().collect();
        crate::runtime::playback::has_flag(&args, "--mute")
            || (crate::runtime::playback::has_flag(&args, "--capture")
                && !crate::runtime::playback::has_flag(&args, "--audible"))
    }
    pub fn verify_mode() -> bool {
        std::env::args().any(|a| a == "--verify")
    }
    pub fn primary_key() -> bool {
        macroquad::prelude::is_key_pressed(macroquad::prelude::KeyCode::Space)
    }
    pub fn sprint_key() -> bool {
        use macroquad::prelude::*;
        is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift)
    }
    pub fn command_key(key: macroquad::prelude::KeyCode) -> bool {
        {
            macroquad::prelude::is_key_pressed(key)
        }
    }
    pub fn focused() -> bool {
        true
    }
    pub fn error(error: &str) {
        eprintln!("2D client: {error}");
    }
}

#[cfg(test)]
mod pointer_action_tests {
    use super::*;
    use crate::runtime::{Simulation, Snapshot};
    #[derive(Default)]
    struct Memory(std::cell::RefCell<std::collections::BTreeMap<String, Vec<u8>>>);
    impl Storage for Memory {
        fn read(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(self.0.borrow().get(key).cloned())
        }
        fn write(&self, key: &str, bytes: &[u8]) -> Result<(), String> {
            self.0.borrow_mut().insert(key.into(), bytes.to_vec());
            Ok(())
        }
    }
    struct Counter(u32);
    impl Simulation for Counter {
        type Input = Intent;
        fn step(&mut self, _: &Intent) {
            self.0 += 1;
        }
        fn state_hash(&self) -> u64 {
            self.0 as u64
        }
    }
    impl Snapshot for Counter {
        const KIND: &'static str = "ui-counter";
        type State = u32;
        fn capture(&self) -> u32 {
            self.0
        }
        fn restore(&mut self, state: u32) -> Result<(), String> {
            self.0 = state;
            Ok(())
        }
    }
    impl super::super::GameLogic for Counter {
        const ID: &'static str = "ui-counter";
        const TITLE: &'static str = "Counter";
        const CONTROLS: &'static str = "Count";
        const VERIFY_TICKS: u32 = 1;
        fn new(_: u64) -> Self {
            Self(0)
        }
        fn tick(&self) -> u32 {
            self.0
        }
        fn outcome(&self) -> &'static str {
            "playing"
        }
        fn verification_input(_: u32) -> Intent {
            Intent::default()
        }
        fn probe_input() -> Intent {
            Intent::default()
        }
        fn probe_success(&self) -> bool {
            self.0 > 0
        }
    }
    impl Game for Counter {
        fn draw<'a>(&'a self, _: &mut Scene<'a>) {}
    }
    #[test]
    fn custom_interface_actions_use_shared_snapshots_settings_and_restart() {
        let store = Memory::default();
        let mut game = Counter(12);
        let mut ui = Lifecycle::new(true);
        let mut settings = Settings::default();
        let mut notice = String::new();
        for bounds in [Rect::new(20, 30, 90, 40), Rect::new(610, 350, 140, 60)] {
            let mut layout = Layout::default();
            layout.button(bounds, Action::Save);
            let action = layout.hit(Point::new(bounds.x + 1, bounds.y + 1)).unwrap();
            assert_eq!(
                execute_action(
                    action,
                    &mut game,
                    &mut ui,
                    &mut settings,
                    &mut notice,
                    &store
                ),
                Some("ui.save")
            );
            game.0 = 99;
            execute_action(
                Action::Load,
                &mut game,
                &mut ui,
                &mut settings,
                &mut notice,
                &store,
            );
            assert_eq!(game.0, 12);
        }
        execute_action(
            Action::ToggleSound,
            &mut game,
            &mut ui,
            &mut settings,
            &mut notice,
            &store,
        );
        assert!(!storage::read_settings::<Settings>(&store).unwrap().sound);
        execute_action(
            Action::Restart,
            &mut game,
            &mut ui,
            &mut settings,
            &mut notice,
            &store,
        );
        assert_eq!(game.0, 0);
        assert!(notice.is_empty());
        store
            .0
            .borrow_mut()
            .insert("quick".into(), b"corrupt snapshot".to_vec());
        execute_action(
            Action::Load,
            &mut game,
            &mut ui,
            &mut settings,
            &mut notice,
            &store,
        );
        assert_eq!(game.0, 0);
        assert!(!notice.is_empty());
    }
    #[test]
    fn mapped_device_command_survives_a_frame_without_a_tick_and_fires_once() {
        let hover = Some(Point::new(400, 200));
        let command = Some(Point::new(-1, 7));
        let mut targets = ActionPointer::default();
        let mut inputs = InputAccumulator::<Intent>::new();
        // The hook maps a keyboard edge to an explicit public command target.
        targets.feed(true, command, command != hover, false);
        inputs.feed(Intent::default(), 1, [0.; 2]);
        targets.feed(false, hover, false, false);
        inputs.feed(Intent::default(), 0, [0.; 2]);
        assert_eq!(targets.take(inputs.take_tick().pressed(1)), command);
        assert_eq!(targets.take(inputs.take_tick().pressed(1)), None);
    }
    #[test]
    fn keyboard_and_controller_ignore_hover_but_clicks_and_taps_keep_targets() {
        let cursor = Some(Point::new(400, 200));
        let mut targets = ActionPointer::default();
        targets.feed(true, cursor, false, false);
        assert_eq!(targets.take(true), None);
        targets.feed(true, cursor, true, false);
        assert_eq!(targets.take(true), cursor);
        targets.feed(true, cursor, false, true);
        assert_eq!(targets.take(true), cursor);
        assert_eq!(targets.take(true), None);
    }
    #[test]
    fn a_fast_click_keeps_its_location_until_the_press_edge_is_consumed() {
        let mut targets = ActionPointer::default();
        let mut inputs = InputAccumulator::<Intent>::new();
        let click = Some(Point::new(20, 30));
        targets.feed(true, click, true, false);
        inputs.feed(Intent::default(), 1, [0.; 2]);
        // A display frame runs no fixed tick, then the pointer moves before the next tick.
        targets.feed(false, Some(Point::new(500, 300)), false, false);
        inputs.feed(Intent::default(), 0, [0.; 2]);
        assert_eq!(targets.take(inputs.take_tick().pressed(1)), click);
        assert_eq!(targets.take(inputs.take_tick().pressed(1)), None);
        targets.feed(true, click, false, true);
        targets.clear();
        assert_eq!(targets.take(true), None);
    }
}
