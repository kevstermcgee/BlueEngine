//! One recommended offline client: fixed-step input, browser/native storage, audio, HUD and verification.
use super::draw::{GOLD, PINK, WHITE};
use super::{draw::*, Intent, Point, Rect};
use crate::runtime::{
    storage::{self, PlatformStorage},
    FixedStepper, InputAccumulator,
};
use macroquad::audio::{load_sound_from_bytes, play_sound, PlaySoundParams, Sound};
use macroquad::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    sound: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self { sound: true }
    }
}
#[derive(Default, Serialize)]
pub struct AudioEvidence {
    pub loaded: u32,
    pub submitted: u32,
    pub enabled: bool,
    pub activated: bool,
}
struct Audio {
    sounds: Vec<Sound>,
    evidence: AudioEvidence,
    muted: bool,
}
impl Audio {
    async fn new(muted: bool) -> Result<Self, String> {
        if muted {
            return Ok(Self {
                sounds: vec![],
                evidence: AudioEvidence::default(),
                muted,
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
        Ok(Self {
            evidence: AudioEvidence {
                loaded: 3,
                ..Default::default()
            },
            sounds,
            muted,
        })
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
    let mut size = (960, 540);
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
    let mut started = verification && cfg!(not(target_arch = "wasm32"));
    #[cfg(not(target_arch = "wasm32"))]
    {
        started |= capture.is_some();
    }
    let mut verification_tick = 0;
    let mut paused = false;
    let mut audio = Audio::new(platform::muted()).await?;
    let mut stepper = FixedStepper::new();
    let mut inputs = InputAccumulator::<Intent>::new();
    let mut particles = Particles::new(8);
    let mut renderer = Renderer::default();
    let mut last_tick = 0;
    let mut frame = 0;
    #[cfg(all(not(target_arch = "wasm32"), feature = "gamepad"))]
    let mut pads = crate::viewer::gamepad::Gamepads::new().ok();
    loop {
        frame += 1;
        let dt = get_frame_time().clamp(0., 0.1);
        let view = Viewport::fit(800., 450., screen_width().max(1.), screen_height().max(1.));
        let (mx, my) = mouse_position();
        let pointer = view.pointer(mx, my);
        #[allow(unused_mut)]
        let mut x = i32::from(is_key_down(KeyCode::D) || is_key_down(KeyCode::Right))
            - i32::from(is_key_down(KeyCode::A) || is_key_down(KeyCode::Left));
        #[allow(unused_mut)]
        let mut y = i32::from(is_key_down(KeyCode::S) || is_key_down(KeyCode::Down))
            - i32::from(is_key_down(KeyCode::W) || is_key_down(KeyCode::Up));
        #[allow(unused_mut)]
        let mut action =
            is_key_pressed(KeyCode::Space) || is_mouse_button_pressed(MouseButton::Left);
        #[allow(unused_mut)]
        let mut start =
            is_key_pressed(KeyCode::Enter) || is_mouse_button_pressed(MouseButton::Left);
        #[cfg(target_arch = "wasm32")]
        {
            let pad = platform::pad();
            x += pad.0;
            y += pad.1;
            action |= pad.2;
            start |= pad.2;
        }
        #[cfg(all(not(target_arch = "wasm32"), feature = "gamepad"))]
        if let Some(pads) = pads.as_mut() {
            let pad = pads.poll(true);
            x += (pad.left_stick[0] * 1.5) as i32;
            y -= (pad.left_stick[1] * 1.5) as i32;
            action |= pad.menu_select();
            start |= pad.menu_select();
        }
        if !started && start {
            started = true;
            audio.play(0, settings.sound);
        }
        if is_key_pressed(KeyCode::R) {
            game = G::new(7);
            inputs.clear();
            last_tick = 0;
            verification_tick = 0;
            started = true;
            notice.clear();
        }
        if is_key_pressed(KeyCode::Escape) {
            paused = !paused;
        }
        if is_key_pressed(KeyCode::M) {
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
        if is_key_pressed(KeyCode::K) {
            notice = match storage::save(&store, &game) {
                Ok(()) => "Game saved. L resumes it.".into(),
                Err(e) => e,
            };
        }
        if is_key_pressed(KeyCode::L) {
            notice = match storage::load(&store, &mut game) {
                Ok(true) => {
                    inputs.clear();
                    last_tick = game.tick();
                    "Game resumed".into()
                }
                Ok(false) => "No save yet. K saves.".into(),
                Err(e) => e,
            };
        }
        let focused = platform::focused();
        let intent = if focused && started && !paused {
            Intent {
                x: x.clamp(-1, 1),
                y: y.clamp(-1, 1),
                pointer,
                action: false,
            }
        } else {
            Intent::default()
        };
        inputs.feed(intent, u32::from(action), [0.; 2]);
        let ticks = if verification && started {
            8
        } else {
            stepper.advance(if focused && started && !paused {
                dt
            } else {
                0.
            })
        };
        for _ in 0..ticks {
            if verification && verification_tick >= G::VERIFY_TICKS {
                break;
            }
            let tick = inputs.take_tick();
            let mut intent = tick.held;
            intent.action = tick.pressed(1);
            if verification {
                intent = G::verification_input(verification_tick);
                verification_tick += 1;
            }
            game.step(&intent);
        }
        for cue in game.take_cues() {
            audio.play(cue, settings.sound);
            particles.burst(Point::new(400, 200), if cue == 1 { PINK } else { GOLD });
        }
        let mut scene = Scene::default();
        scene.rect(-100, Rect::new(0, 0, 800, 450), INK);
        game.draw(&mut scene);
        particles.update(dt, &mut scene);
        scene.text(100, G::TITLE, Point::new(24, 30), 26., WHITE);
        scene.text(100, G::CONTROLS, Point::new(24, 428), 16., WHITE);
        scene.text(
            100,
            "Esc pause · M sound · K save · L load · R restart",
            Point::new(24, 448),
            14.,
            TEAL,
        );
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
        scene.draw(view, Point::default(), &mut renderer)?;
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
            let report = serde_json::json!({"ready":true,"verified":verification_tick>=G::VERIFY_TICKS,"probe":G::probe_input(),"probe_passed":game.probe_success(),"game":G::ID,"tick":game.tick(),"hash":format!("{:016x}",game.state_hash()),"outcome":game.outcome(),"started":started,"paused":paused,"notice":notice,"sound":settings.sound,"audio":audio.evidence});
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
mod platform {
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
        pub fn pad() -> (i32, i32, bool) {
            unsafe {
                (
                    (be2_pad(0) * 1.5) as i32,
                    (be2_pad(1) * 1.5) as i32,
                    be2_pad(2) > 0.,
                )
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    pub use browser::*;
}
