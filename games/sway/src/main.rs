#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod dropfiles;
mod ui;
mod visuals;
use macroquad::prelude::*;
use rodio::{OutputStream, OutputStreamBuilder, Sink, Source};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    time::{SystemTime, UNIX_EPOCH},
};
use sway::{
    audio::{self, Opened, Playlist, Timeline},
    Sway,
};
use vesper3d::portable::{
    draw::{Renderer, Scene, Viewport},
    GameLogic, Intent, Point, Rect as EngineRect, Simulation,
};
use vesper3d::runtime::FixedStepper;
type QuietSource = audio::Tap<rodio::Decoder<std::io::BufReader<std::fs::File>>>;
struct Player {
    stream: Option<OutputStream>,
    sink: Option<Sink>,
    quiet: Option<QuietSource>,
    timeline: Option<Timeline>,
    position: f64,
    fraction: f64,
    ended: bool,
    paused: bool,
    muted: bool,
}
impl Player {
    fn new(muted: bool) -> Self {
        Self {
            stream: None,
            sink: None,
            quiet: None,
            timeline: None,
            position: 0.,
            fraction: 0.,
            ended: true,
            paused: false,
            muted,
        }
    }
    fn start(&mut self, opened: Opened) -> Result<u64, String> {
        if !self.muted && self.stream.is_none() {
            self.stream = Some(
                OutputStreamBuilder::open_default_stream()
                    .map_err(|e| format!("Audio device unavailable: {e}"))?,
            );
        }
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.quiet = None;
        self.position = 0.;
        self.fraction = 0.;
        self.ended = false;
        self.timeline = Some(opened.timeline);
        if self.muted {
            self.quiet = Some(opened.source);
        } else {
            let sink = Sink::connect_new(self.stream.as_ref().unwrap().mixer());
            sink.append(opened.source);
            if self.paused {
                sink.pause();
            }
            self.sink = Some(sink);
        }
        Ok(opened.fingerprint)
    }
    fn pause(&mut self, paused: bool) {
        self.paused = paused;
        if let Some(sink) = &self.sink {
            if paused {
                sink.pause();
            } else {
                sink.play();
            }
        }
    }
    fn update(&mut self, dt: f32) {
        if self.paused {
            return;
        }
        if let Some(source) = &mut self.quiet {
            let rate = source.sample_rate() as f64 * source.channels() as f64;
            let count = dt as f64 * rate + self.fraction;
            self.fraction = count.fract();
            let mut consumed = 0;
            for _ in 0..count as usize {
                if source.next().is_none() {
                    self.ended = true;
                    break;
                }
                consumed += 1;
            }
            self.position += consumed as f64 / rate;
        }
        if let Some(sink) = &self.sink {
            self.position = sink.get_pos().as_secs_f64();
            self.ended = sink.empty();
        }
    }
    fn features(&self) -> audio::Features {
        self.timeline
            .as_ref()
            .map(|t| audio::at_time(t, self.position))
            .unwrap_or_default()
    }
    fn active(&self) -> bool {
        self.timeline.is_some() && !self.ended
    }
}
struct Loading {
    index: usize,
    result: Receiver<Result<Opened, String>>,
}
struct App {
    model: Sway,
    player: Player,
    list: Playlist,
    visuals: visuals::Visuals,
    loading: Option<Loading>,
    picker: Option<Receiver<Option<Vec<PathBuf>>>>,
    attempts: usize,
    menu: bool,
    error: String,
    title_time: f32,
    now_playing: String,
}
impl App {
    fn load(&mut self) {
        if self.list.files.is_empty() || self.loading.is_some() {
            return;
        }
        let index = self.list.current;
        let path = self.list.files[index].clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(audio::open(&path));
        });
        self.loading = Some(Loading { index, result: rx });
    }
    fn import(&mut self, paths: Vec<PathBuf>) {
        let first = self.list.files.len();
        let added = self.list.import(paths);
        if added == 0 {
            self.error = "No files added. Choose or drop audio files, rather than folders.".into();
            self.menu = true;
            return;
        }
        if !self.player.active() {
            self.list.current = first;
            self.attempts = 0;
            self.player.pause(false);
            self.load();
        }
    }
    fn pick(&mut self) {
        if self.picker.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let paths = tinyfiledialogs::open_file_dialog_multi(
                "Choose music — select one or more files",
                "",
                Some((
                    &[
                        "*.mp3", "*.wav", "*.flac", "*.ogg", "*.oga", "*.m4a", "*.aac", "*.aif",
                        "*.aiff", "*.mp4", "*.mkv",
                    ],
                    "Audio and media files",
                )),
            )
            .map(|v| v.into_iter().map(PathBuf::from).collect());
            let _ = tx.send(paths);
        });
        self.picker = Some(rx);
    }
    fn track(&mut self, direction: i32) {
        if self.list.files.is_empty() {
            return;
        }
        self.list.advance(direction);
        self.attempts = 0;
        self.player.pause(false);
        self.load();
    }
    fn poll(&mut self) {
        if let Some(result) = self.picker.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.picker = None;
            if let Some(paths) = result {
                self.import(paths);
            }
        }
        let result = self.loading.as_ref().and_then(|l| l.result.try_recv().ok());
        if let Some(result) = result {
            let index = self.loading.take().unwrap().index;
            if index != self.list.current {
                self.load();
                return;
            }
            match result.and_then(|opened| self.player.start(opened)) {
                Ok(fingerprint) => {
                    self.model.begin_song(fingerprint);
                    self.visuals = visuals::Visuals::default();
                    self.title_time = 0.;
                    self.now_playing = self.list.title();
                    self.menu = false;
                    self.error.clear();
                    self.attempts = 0;
                }
                Err(error) => {
                    self.error = format!("{}: {}", ui::short(&self.list.title(), 25), error);
                    self.attempts += 1;
                    if self.attempts < self.list.files.len() {
                        self.list.advance(1);
                        self.load();
                    } else {
                        self.menu = true;
                    }
                }
            }
        }
    }
    fn actions(&mut self, a: ui::Actions) {
        if a.pick {
            self.pick();
        }
        if a.close {
            self.menu = false;
        }
        if a.reroll {
            self.model.reroll();
        }
        if let Some(v) = a.style {
            self.model.state.style = v;
        }
        if let Some(v) = a.palette {
            self.model.state.palette = v;
        }
        if let Some(v) = a.motion {
            self.model.state.motion = v;
        }
        if let Some(v) = a.track {
            self.list.current = v;
            self.attempts = 0;
            self.player.pause(false);
            self.load();
        }
    }
}
fn flag(args: &[String], key: &str) -> bool {
    args.iter().any(|a| a == key)
}
fn value<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    args.windows(2).find(|a| a[0] == key).map(|a| a[1].as_str())
}
fn config() -> Conf {
    let args: Vec<_> = std::env::args().collect();
    let size = value(&args, "--size")
        .and_then(|s| s.split_once('x'))
        .and_then(|(w, h)| Some((w.parse::<i32>().ok()?, h.parse::<i32>().ok()?)))
        .filter(|(w, h)| *w >= 320 && *h >= 180)
        .unwrap_or((1280, 720));
    Conf {
        window_title: "Sway".into(),
        window_width: size.0,
        window_height: size.1,
        window_resizable: true,
        high_dpi: true,
        icon: Some(macroquad::miniquad::conf::Icon {
            small: *include_bytes!("../assets/icon_16.rgba"),
            medium: *include_bytes!("../assets/icon_32.rgba"),
            big: *include_bytes!("../assets/icon_64.rgba"),
        }),
        ..Default::default()
    }
}
#[macroquad::main(config)]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("Sway: {error}");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    let capture =
        vesper3d::runtime::playback::CapturePlan::from_args(&args).map_err(|e| e.to_string())?;
    if let Some(plan) = &capture {
        plan.create_dir().map_err(|e| e.to_string())?;
    }
    let muted = flag(&args, "--mute") || (capture.is_some() && !flag(&args, "--audible"));
    let verify = flag(&args, "--verify");
    let seed = value(&args, "--seed")
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(7)
        });
    let mut app = App {
        model: Sway::new(seed),
        player: Player::new(muted),
        list: Playlist::default(),
        visuals: Default::default(),
        loading: None,
        picker: None,
        attempts: 0,
        menu: false,
        error: String::new(),
        title_time: 0.,
        now_playing: String::new(),
    };
    if let Some(s) = value(&args, "--style")
        .and_then(|s| s.parse::<u8>().ok())
        .filter(|s| *s <= 6)
    {
        app.model.state.style = s;
    }
    let files: Vec<_> = args
        .windows(2)
        .filter(|a| a[0] == "--file")
        .map(|a| PathBuf::from(&a[1]))
        .collect();
    if !files.is_empty() {
        app.import(files);
    } else if verify {
        let assets = vesper3d::viewer::devkit::runtime_assets(
            "assets",
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")),
        )?;
        app.import(vec![
            assets.join("check-tone.wav"),
            assets.join("check-tone.wav"),
        ]);
    }
    dropfiles::install(flag(&args, "--verify-native-controls"))?;
    let mut renderer = Renderer::default();
    let mut stepper = FixedStepper::new();
    let mut frame = 0_u32;
    let mut fullscreen = false;
    let mut pacing = Vec::new();
    let mut started = get_time();
    let mut was_active = false;
    let benchmark = value(&args, "--benchmark-seconds")
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|s| *s > 0. && *s <= 120.);
    loop {
        frame += 1;
        let raw_dt = get_frame_time();
        let dt = raw_dt.clamp(0., 0.1);
        let drops = dropfiles::take();
        if !drops.is_empty() {
            app.import(drops);
        }
        app.poll();
        if is_key_pressed(KeyCode::Space) && app.player.timeline.is_some() {
            app.player.pause(!app.player.paused);
        }
        if is_key_pressed(KeyCode::O) {
            app.pick();
        }
        if is_key_pressed(KeyCode::Escape)
            || is_key_pressed(KeyCode::Tab)
            || is_mouse_button_pressed(MouseButton::Right)
        {
            app.menu = !app.menu;
        }
        if is_key_pressed(KeyCode::F) {
            fullscreen = !fullscreen;
            set_fullscreen(fullscreen);
        }
        if is_key_pressed(KeyCode::R) {
            app.model.reroll();
        }
        if is_key_pressed(KeyCode::Right) {
            app.track(1);
        }
        if is_key_pressed(KeyCode::Left) {
            app.track(-1);
        }
        app.player.update(dt);
        let active = app.player.active();
        if active && !was_active {
            started = get_time();
        }
        was_active = active;
        if active && !app.player.paused {
            for _ in 0..stepper.advance(dt) {
                app.model.step(&Intent::default());
            }
            app.visuals.advance(
                dt,
                app.player.features(),
                [0.55, 1., 1.65][app.model.state.motion as usize],
            );
            app.title_time += raw_dt.max(0.);
            if benchmark.is_some() && get_time() - started > 1. {
                pacing.push(raw_dt as f64);
            }
        }
        if app.player.timeline.is_some()
            && app.player.ended
            && app.loading.is_none()
            && app.attempts == 0
        {
            app.track(1);
        }
        clear_background(Color::new(0.015, 0.02, 0.04, 1.));
        let mut scene = Scene::default();
        scene.render_view(0, EngineRect::new(0, 0, 1280, 720), |viewport| {
            app.visuals.draw(&app.model, viewport);
            Ok(())
        });
        scene.draw(
            Viewport::fit(1280., 720., screen_width().max(1.), screen_height().max(1.)),
            Point::new(0, 0),
            &mut renderer,
        )?;
        drop(scene);
        show_mouse(app.menu || !active);
        let actions = if !active && app.player.timeline.is_none() {
            ui::empty(&app.error, app.picker.is_some(), app.loading.is_some())
        } else if app.menu {
            ui::options(&app.model, &app.list, &app.error, app.picker.is_some())
        } else {
            ui::title(&app.now_playing, app.title_time);
            ui::Actions::default()
        };
        app.actions(actions);
        if let Some(plan) = &capture {
            if plan.wants(frame) {
                let path = plan.path_for(frame);
                get_screen_data().export_png(path.to_str().ok_or("Capture path is not UTF-8")?);
                println!(
                    "{}",
                    serde_json::json!({"frame":frame,"path":path,"width":screen_width(),"height":screen_height(),"playlist":app.list.files.len(),"listens":app.model.state.listens,"audio_energy":app.visuals.features.energy,"paused":app.player.paused,"animation_time":app.visuals.time,"title_time":app.title_time,"audio_position":app.player.position,"song":app.now_playing,"state_hash":format!("{:016x}",app.model.state_hash())})
                );
            }
            if plan.finished(frame) {
                break;
            }
        }
        if benchmark.is_some_and(|seconds| get_time() - started >= seconds) {
            pacing.sort_by(f64::total_cmp);
            let mean = pacing.iter().sum::<f64>() / pacing.len().max(1) as f64;
            let p95 = pacing
                .get((pacing.len() * 95 / 100).min(pacing.len().saturating_sub(1)))
                .copied()
                .unwrap_or(0.);
            println!(
                "{}",
                serde_json::json!({"fps":1./mean,"frame_ms_p95":p95*1000.,"frames":pacing.len(),"seconds":get_time()-started,"size":[screen_width(),screen_height()]})
            );
            break;
        }
        next_frame().await;
    }
    Ok(())
}
