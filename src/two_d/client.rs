//! One recommended offline client: fixed-step input, browser/native storage, audio, HUD and verification.
use super::draw::{GOLD, PINK, WHITE};
use super::{draw::*, Intent, Point, Rect};
use crate::runtime::{
    storage::{self, PlatformStorage},
    FixedStepper, InputAccumulator,
};
use macroquad::audio::{
    load_sound_from_bytes, play_sound, set_sound_volume, stop_sound, PlaySoundParams, Sound,
};
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
#[derive(Default, Serialize)]
pub struct AudioEvidence {
    pub loaded: u32,
    pub submitted: u32,
    pub enabled: bool,
    pub activated: bool,
}
#[derive(Default, Serialize)]
struct LoopEvidence {
    loaded: usize,
    submitted: usize,
    playing: bool,
    active_layers: usize,
}
struct LoopSound {
    bank: &'static str,
    layer: String,
    music: bool,
    sound: Sound,
    volume: f32,
}
struct Audio {
    sounds: Vec<Sound>,
    evidence: AudioEvidence,
    muted: bool,
    loops: Vec<LoopSound>,
    loop_evidence: LoopEvidence,
}
impl Audio {
    async fn new<G: Game>(muted: bool) -> Result<Self, String> {
        if muted {
            return Ok(Self {
                sounds: vec![],
                evidence: AudioEvidence::default(),
                muted,
                loops: vec![],
                loop_evidence: LoopEvidence::default(),
            });
        }
        use crate::runtime::synth::{self, Preset};
        let mut sounds = Vec::new();
        for preset in [Preset::Coin, Preset::Hit, Preset::Success] {
            let bytes = synth::wav_bytes(&synth::render(preset, 0, 7), synth::RATE);
            sounds.push(
                load_sound_from_bytes(&bytes)
                    .await
                    .map_err(|e| format!("Audio decode failed: {e}"))?,
            );
        }
        let mut loops = Vec::new();
        let mut ids = std::collections::BTreeSet::new();
        for bank in G::audio_banks() {
            platform::report(
                &serde_json::json!({"ready":false,"notice":format!("Loading {} audio…",bank.id)})
                    .to_string(),
            );
            if !ids.insert(bank.id) {
                return Err(format!("Duplicate audio bank ID {}", bank.id));
            }
            let bytes = macroquad::file::load_file(&format!("{}/bank.json", bank.root))
                .await
                .map_err(|e| {
                    format!(
                        "Audio bank {} unavailable: {e}; declare it in the web package",
                        bank.id
                    )
                })?;
            let metadata = crate::runtime::audio_project::AudioBundle::parse(&bytes)?;
            if !metadata.effects.is_empty() {
                return Err(
                    "Portable loop banks must contain only music/ambience; effects use take_cues"
                        .into(),
                );
            }
            for (layer, info) in metadata.music {
                let path = format!("{}/{}", bank.root, info.file);
                platform::report(&serde_json::json!({"ready":false,"notice":format!("Loading {}/{} audio…",bank.id,layer)}).to_string());
                let bytes = macroquad::file::load_file(&path)
                    .await
                    .map_err(|e| format!("Audio asset {path} unavailable: {e}"))?;
                crate::runtime::audio_project::AudioBundle::verify_file(&info, &bytes)?;
                let sound = load_sound_from_bytes(&bytes)
                    .await
                    .map_err(|e| format!("Audio asset {path} failed decoding: {e}"))?;
                loops.push(LoopSound {
                    bank: bank.id,
                    layer,
                    music: bank.music,
                    sound,
                    volume: 0.,
                });
                next_frame().await;
            }
        }
        Ok(Self {
            loop_evidence: LoopEvidence {
                loaded: loops.len(),
                ..Default::default()
            },
            loops,
            evidence: AudioEvidence {
                loaded: 3,
                ..Default::default()
            },
            sounds,
            muted,
        })
    }
    fn update<G: Game>(
        &mut self,
        game: &G,
        active: bool,
        settings: &Settings,
        dt: f32,
    ) -> Result<(), String> {
        let active = active && !self.muted && platform::audio_active();
        if active && !self.loop_evidence.playing && !self.loops.is_empty() {
            for track in &self.loops {
                play_sound(
                    &track.sound,
                    PlaySoundParams {
                        looped: true,
                        volume: 0.,
                    },
                );
            }
            self.loop_evidence.submitted += self.loops.len();
            self.loop_evidence.playing = true;
        } else if !active && self.loop_evidence.playing {
            for track in &mut self.loops {
                stop_sound(&track.sound);
                track.volume = 0.;
            }
            self.loop_evidence.playing = false;
        }
        self.loop_evidence.active_layers = 0;
        for track in &mut self.loops {
            let level = game.audio_level(track.bank, &track.layer);
            if !level.is_finite() || !(0. ..=1.).contains(&level) {
                return Err(format!(
                    "Audio level {}/{} must be finite 0..1",
                    track.bank, track.layer
                ));
            }
            let enabled = if track.music {
                settings.music
            } else {
                settings.sound
            };
            let target = if active && enabled { level } else { 0. };
            // Disable immediately; ordinary day/night transitions fade.
            track.volume = if !enabled {
                0.
            } else {
                track.volume + (target - track.volume) * (dt * 3.).clamp(0., 1.)
            };
            set_sound_volume(&track.sound, track.volume);
            if track.volume > 0.001 {
                self.loop_evidence.active_layers += 1;
            }
        }
        Ok(())
    }
    fn play(&mut self, cue: usize, enabled: bool) {
        self.evidence.enabled = enabled && !self.muted;
        self.evidence.activated = platform::audio_active();
        if self.evidence.enabled && self.evidence.activated {
            if let Some(sound) = self.sounds.get(cue) {
                play_sound(
                    sound,
                    PlaySoundParams {
                        looped: false,
                        volume: 0.3,
                    },
                );
                self.evidence.submitted += 1;
            }
        }
    }
}
pub fn config(title: &str) -> macroquad::conf::Conf {
    #[allow(unused_mut)]
    let mut size = (960_u32, 540_u32);
    #[cfg(not(target_arch = "wasm32"))]
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
    if let Err(error) = run_inner::<G>().await {
        platform::error(&error);
        #[cfg(not(target_arch = "wasm32"))]
        std::process::exit(1);
        #[cfg(target_arch = "wasm32")]
        loop {
            clear_background(INK);
            draw_text(&error, 20., 50., 22., PINK);
            next_frame().await;
        }
    }
}
async fn run_inner<G: Game>() -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    let capture = {
        let args: Vec<_> = std::env::args().collect();
        let plan =
            crate::runtime::playback::CapturePlan::from_args(&args).map_err(|e| e.to_string())?;
        if let Some(plan) = &plan {
            plan.create_dir().map_err(|e| e.to_string())?;
        }
        plan
    };
    let store = PlatformStorage::new(G::ID)?;
    let (mut settings, mut notice) = match storage::read_settings::<Settings>(&store) {
        Ok(settings) => (settings, String::new()),
        Err(error) => (
            Settings::default(),
            format!("{error}; playing with session defaults"),
        ),
    };
    let mut game = G::new(7);
    let verification = platform::verify_mode();
    #[allow(unused_mut)]
    let mut persist_progress = !verification;
    #[cfg(not(target_arch = "wasm32"))]
    {
        persist_progress &= capture.is_none();
    }
    if persist_progress {
        match storage::load_slot(&store, "progress", &mut game) {
            Ok(true) => notice = "Progress restored. Start to continue.".into(),
            Ok(false) => {}
            Err(error) => notice = error,
        }
    }
    let mut autosave_time = 0.;
    let mut autosave_hash = None;
    let mut started = verification && cfg!(not(target_arch = "wasm32"));
    #[cfg(not(target_arch = "wasm32"))]
    {
        started |= capture.is_some();
    }
    let mut verification_tick = 0;
    let mut paused = false;
    let mut audio = Audio::new::<G>(platform::muted()).await?;
    let mut stepper = FixedStepper::new();
    let mut inputs = InputAccumulator::<Intent>::new();
    let mut pointer_actions = ActionPointer::default();
    let mut particles = Particles::new(8);
    let mut renderer = Renderer::default();
    let mut last_tick = 0;
    let mut frame = 0;
    let mut last_pointer = None;
    let mut step_max_ms = 0_f64;
    let mut draw_max_ms = 0_f64;
    let mut chunk_max_ms = 0_f64;
    let mut chunk_updates = 0_u64;
    let mut chunk_stalls = 0_u64;
    let mut chunk_render_max_ms = 0_f64;
    let mut chunk_render_frames = 0_u64;
    let mut chunk_render_stalls = 0_u64;
    let mut movement_ticks = 0_u64;
    let mut action_ticks = 0_u64;
    let mut step_ticks = 0_u64;
    #[cfg(not(target_arch = "wasm32"))]
    let mut fullscreen = false;
    #[cfg(all(not(target_arch = "wasm32"), feature = "gamepad"))]
    let mut pads = crate::viewer::gamepad::Gamepads::new().ok();
    loop {
        frame += 1;
        #[cfg(not(target_arch = "wasm32"))]
        if is_key_pressed(KeyCode::F) {
            fullscreen = !fullscreen;
            macroquad::miniquad::window::set_fullscreen(fullscreen);
        }
        let dt = get_frame_time().clamp(0., 0.1);
        let view = Viewport::fit(800., 450., screen_width().max(1.), screen_height().max(1.));
        let (mx, my) = mouse_position();
        #[allow(unused_mut)]
        let mut pointer = view.pointer(mx, my);
        #[allow(unused_mut)]
        let mut digital = platform::touch();
        if let Some(point) = digital.pointer {
            pointer = Some(point);
        }
        #[allow(unused_mut)]
        #[cfg(not(target_arch = "wasm32"))]
        let mut x = i32::from(is_key_down(KeyCode::D) || is_key_down(KeyCode::Right))
            - i32::from(is_key_down(KeyCode::A) || is_key_down(KeyCode::Left))
            + digital.x;
        #[allow(unused_mut)]
        #[cfg(not(target_arch = "wasm32"))]
        let mut y = i32::from(is_key_down(KeyCode::S) || is_key_down(KeyCode::Down))
            - i32::from(is_key_down(KeyCode::W) || is_key_down(KeyCode::Up))
            + digital.y;
        #[cfg(target_arch = "wasm32")]
        let (mut x, mut y) = {
            let keyboard = platform::keyboard_movement();
            (keyboard.0 + digital.x, keyboard.1 + digital.y)
        };
        #[allow(unused_mut)]
        let mut action = platform::primary_key() || is_mouse_button_pressed(MouseButton::Left);
        #[allow(unused_mut)]
        let mut start =
            platform::command_key(KeyCode::Enter) || is_mouse_button_pressed(MouseButton::Left);
        action |= digital.action;
        start |= digital.commands & 1 != 0;
        #[cfg(target_arch = "wasm32")]
        {
            let pad = platform::pad();
            x += pad.0;
            y += pad.1;
            action |= pad.2;
            start |= pad.2;
            digital.commands |= pad.3;
        }
        #[allow(unused_mut)]
        let mut look_native = [0.; 2];
        #[cfg(all(not(target_arch = "wasm32"), feature = "gamepad"))]
        if let Some(pads) = pads.as_mut() {
            use crate::viewer::gamepad::Button;
            let pad = pads.poll(true);
            // Same contract as the browser's standard gamepad: stick or D-pad moves, A acts/starts,
            // Start plays or pauses, B pauses. See templates/web/controls.json.
            let dpad = pad.dpad();
            x += (pad.left_stick[0] * 1.5) as i32 + i32::from(dpad[3]) - i32::from(dpad[2]);
            y -= (pad.left_stick[1] * 1.5) as i32;
            y += i32::from(dpad[1]) - i32::from(dpad[0]);
            action |= pad.menu_select();
            start |= pad.menu_select();
            if pad.pressed(Button::Start) {
                digital.commands |= if started { 2 } else { 1 };
            }
            if pad.menu_back() {
                digital.commands |= 2;
            }
            if G::drag_look() {
                look_native = [pad.right_stick[0] * dt * 2., pad.right_stick[1] * dt * 2.];
            }
        }
        let mut look = [0.; 2];
        if G::drag_look() && is_mouse_button_down(MouseButton::Left) && pointer.is_some() {
            if let Some((x, y)) = last_pointer {
                look = [(mx - x) * 0.003, (y - my) * 0.003];
            }
            last_pointer = Some((mx, my));
        } else {
            last_pointer = None;
        }
        #[cfg(target_arch = "wasm32")]
        if G::drag_look() {
            let stick = platform::look_pad();
            look[0] += stick[0] * dt * 2.;
            look[1] -= stick[1] * dt * 2.;
        }
        look[0] += look_native[0];
        look[1] += look_native[1];
        if !started && start {
            started = true;
            audio.play(0, settings.sound);
        }
        if platform::command_key(KeyCode::R) || digital.commands & 4 != 0 {
            game.restart();
            inputs.clear();
            pointer_actions.clear();
            last_tick = 0;
            verification_tick = 0;
            started = true;
            notice.clear();
        }
        if platform::command_key(KeyCode::Escape) || digital.commands & 2 != 0 {
            paused = !paused;
        }
        if platform::command_key(KeyCode::M) || digital.commands & 8 != 0 {
            settings.sound = !settings.sound;
            match storage::write_settings(&store, &settings) {
                Ok(()) => {
                    notice = format!(
                        "Sound {} — saved",
                        if settings.sound { "on" } else { "off" }
                    )
                }
                Err(e) => notice = e,
            }
        }
        if platform::command_key(KeyCode::N) || digital.commands & 64 != 0 {
            settings.music = !settings.music;
            notice = match storage::write_settings(&store, &settings) {
                Ok(()) => format!(
                    "Music {} — saved",
                    if settings.music { "on" } else { "off" }
                ),
                Err(e) => e,
            };
        }
        if platform::command_key(KeyCode::K) || digital.commands & 16 != 0 {
            notice = match storage::save(&store, &game) {
                Ok(()) => "Game saved. L resumes it.".into(),
                Err(e) => e,
            };
        }
        if platform::command_key(KeyCode::L) || digital.commands & 32 != 0 {
            notice = match storage::load(&store, &mut game) {
                Ok(true) => {
                    inputs.clear();
                    pointer_actions.clear();
                    last_tick = game.tick();
                    "Game resumed".into()
                }
                Ok(false) => "No save yet. K saves.".into(),
                Err(e) => e,
            };
        }
        let focused = platform::focused();
        if G::pointer_target_only_on_press() {
            pointer_actions.feed(
                action,
                pointer,
                is_mouse_button_pressed(MouseButton::Left),
                digital.action && digital.pointer.is_some(),
            );
        }
        let intent = if focused && started && !paused {
            Intent {
                x: x.clamp(-1, 1),
                y: y.clamp(-1, 1),
                pointer,
                action: false,
                sprint: platform::sprint_key(),
                look: [0.; 2],
            }
        } else {
            Intent::default()
        };
        if !focused || paused || !started {
            inputs.clear();
            pointer_actions.clear();
            last_pointer = None;
        }
        inputs.feed(
            intent,
            u32::from(action && focused && started && !paused),
            if focused && started && !paused {
                look
            } else {
                [0.; 2]
            },
        );
        let ticks = if verification && started {
            // Replay uses the same fixed steps; graphics need not redraw after every eight ticks.
            // Real-device verification below runs normal timing independently.
            // A real touch gesture resumes Web Audio asynchronously. Do not finish a short
            // accelerated route before its ordinary gameplay cues can reach that context.
            if platform::audio_active() {
                60
            } else {
                0
            }
        } else {
            stepper.advance(if focused && started && !paused {
                dt
            } else {
                0.
            })
        };
        let mut streamed_this_frame = false;
        for _ in 0..ticks {
            if verification && verification_tick >= G::VERIFY_TICKS {
                break;
            }
            let tick = inputs.take_tick();
            let mut intent = tick.held;
            intent.action = tick.pressed(1);
            if G::pointer_target_only_on_press() {
                intent.pointer = pointer_actions.take(intent.action);
            }
            intent.look = tick.look;
            if verification {
                intent = G::verification_input(verification_tick);
                verification_tick += 1;
            }
            step_ticks += 1;
            movement_ticks += u64::from(intent.x != 0 || intent.y != 0);
            action_ticks += u64::from(intent.action);
            let marker = game.streaming_marker();
            let step_start = platform::now_ms();
            game.step(&intent);
            let elapsed = platform::now_ms() - step_start;
            step_max_ms = step_max_ms.max(elapsed);
            if game.streaming_marker() != marker {
                streamed_this_frame = true;
                chunk_updates += 1;
                chunk_max_ms = chunk_max_ms.max(elapsed);
                chunk_stalls += u64::from(elapsed > 50.);
            }
        }
        for cue in game.take_cues() {
            audio.play(cue, settings.sound);
            particles.burst(game.cue_point(cue), if cue == 1 { PINK } else { GOLD });
        }
        audio.update(&game, started && !paused && focused, &settings, dt)?;
        autosave_time += dt;
        if persist_progress
            && started
            && game.tick() > 0
            && (autosave_time >= 1. || !focused || paused || game.outcome() != "playing")
        {
            autosave_time = 0.;
            let hash = game.state_hash();
            if autosave_hash != Some(hash) {
                match storage::save_slot(&store, "progress", &game) {
                    Ok(()) => autosave_hash = Some(hash),
                    Err(error) => {
                        notice = format!("Autosave failed: {error}");
                        autosave_hash = Some(hash);
                    }
                }
            }
        }
        let mut scene = Scene::default();
        scene.rect(-100, Rect::new(0, 0, 800, 450), INK);
        game.draw(&mut scene);
        particles.update(dt, &mut scene);
        if G::show_hud() {
            scene.text(100, G::TITLE, Point::new(24, 30), 26., WHITE);
            scene.text(100, G::CONTROLS, Point::new(24, 428), 16., WHITE);
            scene.text(
                100,
                "Esc pause · M sound · K save · L load · R restart",
                Point::new(24, 448),
                14.,
                TEAL,
            );
        }
        if !notice.is_empty() {
            scene.text(101, &notice, Point::new(24, 395), 19., GOLD);
        }
        if !started || paused || game.outcome() != "playing" {
            scene.rect(
                200,
                Rect::new(145, 135, 510, 180),
                Color::new(0.1, 0.16, 0.22, 0.96),
            );
            let title = if !started {
                "CLICK OR ENTER TO START"
            } else if paused {
                "PAUSED"
            } else if game.outcome() == "won" {
                "COMPLETE!"
            } else {
                "TRY AGAIN"
            };
            scene.text(201, title, Point::new(180, 190), 34., GOLD);
            scene.text(201, game.menu_status(), Point::new(180, 274), 22., WHITE);
            scene.text(
                201,
                if !started {
                    "Sound activates with your input."
                } else {
                    "R restarts · K saves · L resumes"
                },
                Point::new(180, 238),
                22.,
                WHITE,
            );
        }
        clear_background(BLACK);
        let draw_start = platform::now_ms();
        scene.draw(view, Point::default(), &mut renderer)?;
        let draw_elapsed = platform::now_ms() - draw_start;
        draw_max_ms = draw_max_ms.max(draw_elapsed);
        if streamed_this_frame {
            // Include deferred presentation work on streaming frames. This is the whole draw,
            // not a claim that every millisecond was spent generating chunk art.
            chunk_render_frames += 1;
            chunk_render_max_ms = chunk_render_max_ms.max(draw_elapsed);
            chunk_render_stalls += u64::from(draw_elapsed > 50.);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(plan) = &capture {
            if plan.wants(frame) {
                let path = plan.path_for(frame);
                get_screen_data().export_png(path.to_str().ok_or("Capture path must be UTF-8")?);
                println!(
                    "{}",
                    serde_json::json!({"frame":frame,"path":path,"width":screen_width(),"height":screen_height()})
                );
            }
        }
        audio.evidence.enabled = settings.sound && !audio.muted;
        audio.evidence.activated = platform::audio_active();
        if game.tick() != last_tick || frame % 30 == 0 {
            let report = serde_json::json!({"ready":true,"verified":verification_tick>=G::VERIFY_TICKS,"probe":G::probe_input(),"probe_passed":game.probe_success(),"pointer_target_only_on_press":G::pointer_target_only_on_press(),"game":G::ID,"tick":game.tick(),"hash":format!("{:016x}",game.state_hash()),"outcome":game.outcome(),"started":started,"paused":paused,"focused":focused,"frame_seconds":dt,"accepted_input":{"step_ticks":step_ticks,"movement_ticks":movement_ticks,"action_ticks":action_ticks},"performance":{"step_max_ms":step_max_ms,"draw_max_ms":draw_max_ms,"chunk_update_max_ms":chunk_max_ms,"chunk_updates":chunk_updates,"chunk_stalls_over_50ms":chunk_stalls,"chunk_render_max_ms":chunk_render_max_ms,"chunk_render_frames":chunk_render_frames,"chunk_render_stalls_over_50ms":chunk_render_stalls},"notice":notice,"sound":settings.sound,"music_on":settings.music,"music":audio.loop_evidence,"audio":audio.evidence});
            platform::report(&report.to_string());
            last_tick = game.tick();
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if capture.as_ref().is_some_and(|c| c.finished(frame))
                || (capture.is_none() && verification && verification_tick >= G::VERIFY_TICKS)
            {
                println!(
                    "{}",
                    serde_json::json!({"tick":game.tick(),"hash":format!("{:016x}",game.state_hash()),"outcome":game.outcome()})
                );
                break;
            }
        }
        next_frame().await;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(())
    }
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
    #[derive(Default)]
    pub struct Digital {
        pub x: i32,
        pub y: i32,
        pub action: bool,
        pub commands: i32,
        pub pointer: Option<super::Point>,
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn touch() -> Digital {
        Digital::default()
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn muted() -> bool {
        let args: Vec<_> = std::env::args().collect();
        crate::runtime::playback::has_flag(&args, "--mute")
            || (crate::runtime::playback::has_flag(&args, "--capture")
                && !crate::runtime::playback::has_flag(&args, "--audible"))
    }
    #[cfg(target_arch = "wasm32")]
    pub fn muted() -> bool {
        false
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn verify_mode() -> bool {
        std::env::args().any(|a| a == "--verify")
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn primary_key() -> bool {
        macroquad::prelude::is_key_pressed(macroquad::prelude::KeyCode::Space)
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn sprint_key() -> bool {
        use macroquad::prelude::*;
        is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift)
    }
    pub fn command_key(key: macroquad::prelude::KeyCode) -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = key;
            false
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            macroquad::prelude::is_key_pressed(key)
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn focused() -> bool {
        true
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn audio_active() -> bool {
        true
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn report(_: &str) {}
    #[cfg(not(target_arch = "wasm32"))]
    pub fn now_ms() -> f64 {
        0. // Browser instrumentation only; native has its own performance/capture tooling.
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn error(error: &str) {
        eprintln!("2D client: {error}");
    }
    #[cfg(target_arch = "wasm32")]
    mod browser {
        #![allow(unsafe_code)]
        unsafe extern "C" {
            fn be2_report(ptr: *const u8, len: usize);
            fn be2_error(ptr: *const u8, len: usize);
            fn be2_verify() -> i32;
            fn be2_focused() -> i32;
            fn be2_audio_active() -> i32;
            fn be2_pad(axis: i32) -> f32;
            fn be2_clock() -> f64;
            fn be2_keyboard(axis: i32) -> i32;
            fn be2_touch(field: i32) -> i32;
        }
        pub fn report(value: &str) {
            unsafe {
                be2_report(value.as_ptr(), value.len());
            }
        }
        pub fn error(value: &str) {
            unsafe {
                be2_error(value.as_ptr(), value.len());
            }
        }
        pub fn verify_mode() -> bool {
            unsafe { be2_verify() != 0 }
        }
        pub fn focused() -> bool {
            unsafe { be2_focused() != 0 }
        }
        pub fn audio_active() -> bool {
            unsafe { be2_audio_active() != 0 }
        }
        pub fn now_ms() -> f64 {
            unsafe { be2_clock() }
        }
        pub fn keyboard_movement() -> (i32, i32) {
            unsafe { (be2_keyboard(0), be2_keyboard(1)) }
        }
        pub fn primary_key() -> bool {
            unsafe { be2_keyboard(3) != 0 }
        }
        pub fn sprint_key() -> bool {
            unsafe { be2_keyboard(2) != 0 }
        }
        pub fn pad() -> (i32, i32, bool, i32) {
            unsafe {
                (
                    (be2_pad(0) * 1.5) as i32,
                    (be2_pad(1) * 1.5) as i32,
                    be2_pad(2) > 0.,
                    be2_pad(5) as i32,
                )
            }
        }
        pub fn look_pad() -> [f32; 2] {
            unsafe { [be2_pad(3), be2_pad(4)] }
        }
        pub fn touch() -> super::Digital {
            unsafe {
                let x = be2_touch(5);
                let y = be2_touch(6);
                super::Digital {
                    x: be2_touch(0),
                    y: be2_touch(1),
                    action: be2_touch(2) != 0,
                    commands: be2_touch(3),
                    pointer: (x >= 0 && y >= 0).then_some(crate::two_d::Point::new(x, y)),
                }
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    pub use browser::*;
}

#[cfg(test)]
mod pointer_action_tests {
    use super::*;
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
