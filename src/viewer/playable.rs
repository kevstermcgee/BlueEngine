//! Shared authored-game presentation. Generated games and `be2 --game` call this
//! runner; `local_client::run_map` remains an explicitly static viewer.
use super::{
    camera::{CameraRig, Perspective},
    character_skins::Avatar,
    game::LoadedGame,
    game_client::GameShell,
    game_input::ClientInput,
    game_session::{GameInput, GameSession},
    gamepad::Button,
    mesh,
    net::{DatagramTransport, SecureSocket, TransportProfile, UdpTransport},
    prop_view::Props,
    savestate::{SaveError, SaveSlots, Source, QUICK_SLOT},
};
use crate::Result;
use macroquad::prelude::*;
use std::{net::ToSocketAddrs, path::PathBuf};

/// Host configuration, separate from authored rules. No socket is opened by default.
#[derive(Default)]
pub struct GameOptions {
    /// Optional host key-state reader; unsafe OS queries stay outside the library.
    pub keyboard: Option<fn(i32) -> i16>,
    pub connect: Option<std::net::SocketAddr>,
    /// Optional listen-and-play mode; owns a shared DedicatedServer until this runner exits.
    pub server: Option<std::net::SocketAddr>,
    pub transport: TransportProfile,
    pub auth_key: Option<String>,
    pub third_person: bool,
    pub character: Option<String>,
    /// New directory for actual GPU captures and a machine-readable run report.
    pub capture: Option<PathBuf>,
    /// Optional fixed-tick public-input playback for repeatable playable-path checks.
    pub playback: Option<PathBuf>,
    /// Directory of save slots (F5 quick-saves into it, F9 loads). Default: `saves` next to the executable.
    pub save_dir: Option<PathBuf>,
    /// Resume a saved game at start: a slot name in the save directory (`quick`) or a save file path.
    pub load: Option<String>,
}
impl GameOptions {
    /// Shared stock/generated CLI: --connect, --transport, --auth-key, --character,
    /// --third-person, --capture, --playback, --save-dir and --load. Content location belongs to the host.
    pub fn from_args(args: &[String]) -> Result<Self> {
        let value = |flag: &str| -> Result<Option<&str>> {
            args.iter()
                .position(|a| a == flag)
                .map(|i| {
                    args.get(i + 1)
                        .filter(|v| !v.starts_with("--"))
                        .map(String::as_str)
                        .ok_or_else(|| format!("{flag} requires a value").into())
                })
                .transpose()
        };
        Ok(Self {
            keyboard: None,
            server: args
                .iter()
                .position(|a| a == "--server")
                .map(|i| {
                    args.get(i + 1)
                        .filter(|a| !a.starts_with("--"))
                        .map(String::as_str)
                        .unwrap_or("127.0.0.1:4000")
                        .parse()
                })
                .transpose()?,
            connect: value("--connect")?
                .map(|s| {
                    s.to_socket_addrs().and_then(|mut a| {
                        a.next()
                            .ok_or_else(|| std::io::Error::other("No server address"))
                    })
                })
                .transpose()?,
            transport: value("--transport")?
                .map(str::parse)
                .transpose()?
                .unwrap_or_default(),
            auth_key: value("--auth-key")?.or(value("--auth")?).map(str::to_owned),
            character: value("--character")?.map(str::to_owned),
            third_person: args.iter().any(|a| a == "--third-person"),
            capture: value("--capture")?.map(PathBuf::from),
            playback: value("--playback")?.map(PathBuf::from),
            save_dir: value("--save-dir")?.map(PathBuf::from),
            load: value("--load")?.map(str::to_owned),
        })
    }
}
/// Launch using process options; a local game needs no network connection.
pub async fn run_game(game: LoadedGame) -> Result<()> {
    run_game_with_focus(game, || true).await
}
pub async fn run_game_with_focus(game: LoadedGame, focused: impl Fn() -> bool) -> Result<()> {
    let options = GameOptions::from_args(&std::env::args().collect::<Vec<_>>())?;
    run_game_with_options(game, options, focused).await
}

/// Reuses shared input, camera, menus, cached geometry and prop transforms.
/// Online mode renders server state and only predicts the local controller.
pub async fn run_game_with_options(
    game: LoadedGame,
    options: GameOptions,
    focused: impl Fn() -> bool,
) -> Result<()> {
    let title = game.document.name.clone();
    if options.server.is_some() && options.connect.is_some() {
        return Err("Choose --server or --connect".into());
    }
    let hosted = if let Some(address) = options.server {
        let world = game.clone().world()?;
        Some(match options.transport {
            TransportProfile::Development => Hosted::start(
                UdpTransport::bind(&address.to_string())?,
                world,
                options.auth_key.as_deref(),
            )?,
            TransportProfile::Production => Hosted::start(
                SecureSocket::server(address, super::net::Identity::load()?)?,
                world,
                options.auth_key.as_deref(),
            )?,
        })
    } else {
        None
    };
    let mut session = if let Some(mut address) = options
        .connect
        .or_else(|| hosted.as_ref().map(|h| h.address))
    {
        if address.ip().is_unspecified() {
            address.set_ip(if address.is_ipv4() {
                std::net::Ipv4Addr::LOCALHOST.into()
            } else {
                std::net::Ipv6Addr::LOCALHOST.into()
            });
        }
        let transport: Box<dyn DatagramTransport> = match options.transport {
            TransportProfile::Development => Box::new(UdpTransport::bind(if address.is_ipv4() {
                "0.0.0.0:0"
            } else {
                "[::]:0"
            })?),
            TransportProfile::Production => Box::new(SecureSocket::client(
                address,
                super::net::trusted_certificate()?,
            )?),
        };
        GameSession::connect(game, transport, address, options.auth_key)?
    } else {
        GameSession::local(game)?
    };
    let mut view = GameView::new(
        &session,
        options.character.as_deref().unwrap_or("scientist"),
    )?;
    let slots = options
        .save_dir
        .clone()
        .map_or_else(SaveSlots::beside_exe, SaveSlots::new);
    let mut toast = Toast::default();
    if let Some(target) = &options.load {
        if !session.can_save() {
            return Err("--load needs a local game; an online game is loaded on the server".into());
        }
        let note = load_target(&mut session, &slots, target)
            .map_err(|e| format!("--load {target}: {e}"))?;
        view.reset_camera();
        toast.show(note);
    }
    let mut shell = GameShell::new();
    let mut input = ClientInput::new();
    let mut perspective = if options.third_person {
        Perspective::Third
    } else {
        Perspective::First
    };
    let playback: Option<Vec<GameInput>> = options
        .playback
        .as_ref()
        .map(|p| -> Result<Vec<GameInput>> { Ok(serde_json::from_slice(&std::fs::read(p)?)?) })
        .transpose()?;
    if playback.is_some() && session.is_online() {
        return Err("Playback is local; use scripted network clients for online checks".into());
    }
    if let Some(dir) = &options.capture {
        std::fs::create_dir(dir)?;
    }
    let mut frame = 0usize;
    let mut completed = false;
    let mut round = 0;
    let mut history = Vec::new();
    loop {
        input.begin_frame_with_keyboard(
            &mut shell,
            session.connected(),
            focused(),
            options.keyboard,
        );
        if playback.is_none() && options.capture.is_none() {
            if input.pressed(KeyCode::F5) {
                toast.show(quick_save(&session, &slots));
            }
            if input.pressed(KeyCode::F9) {
                let note = quick_load(&mut session, &slots);
                if session.can_save() {
                    view.reset_camera();
                }
                toast.show(note);
            }
        }
        // Wall-clock interval between frame starts, not macroquad's get_frame_time(), which is
        // stamped after the GL flush and turns one stalled frame into a doubled step plus a repeat.
        let seconds = if playback.is_some() || options.capture.is_some() {
            1. / 60.
        } else {
            input.frame_seconds()
        };
        let playback_done = playback.as_ref().is_some_and(|p| frame >= p.len());
        if playback.is_some() {
            shell.paused = playback_done;
        }
        if playback_done || (options.capture.is_some() && playback.is_none() && frame >= 30) {
            shell.paused = true;
        }
        let match_over = session.world().game.as_ref().is_some_and(|g| g.state().finished());
        let mut intent = GameInput {
            movement: input.movement(&shell),
            look: input.look_delta(&shell),
            // E / X interact and replay; R is an extra replay hotkey once the match has ended
            // (the engine's one restart convention, see ClientInput::restart_requested).
            interact: shell.playing()
                && (input.pressed(KeyCode::E)
                    || input.gamepad().pressed(Button::West)
                    || (match_over && input.pressed(KeyCode::R))),
        };
        if let Some(frames) = &playback {
            intent = frames.get(frame).copied().unwrap_or_default();
        }
        if shell.playing()
            && (input.pressed(KeyCode::Q) || input.gamepad().pressed(Button::RightThumb))
        {
            perspective.toggle();
        }
        session.advance(
            intent,
            seconds,
            if playback.is_some() {
                !playback_done
            } else {
                shell.playing()
            },
        )?;
        clear_background(Color::new(0.48, 0.67, 0.8, 1.));
        view.draw(&session, perspective, seconds);
        let game = session.world().game.as_ref().unwrap();
        let status = if !session.connected() {
            "Connecting...".to_owned()
        } else if game.state().completed {
            "Objective complete! E / X / R to play again".to_owned()
        } else if game.state().failed {
            "Objective failed! E / X / R to try again".to_owned()
        } else {
            game.document()
                .counters
                .keys()
                .zip(&game.state().counters)
                .map(|(name, value)| format!("{name}: {value}"))
                .collect::<Vec<_>>()
                .join("   ")
        };
        draw_rectangle(
            12.,
            12.,
            (status.len() as f32 * 11. + 24.).min(screen_width() - 24.),
            35.,
            Color::new(0.04, 0.08, 0.1, 0.85),
        );
        super::game_text::draw_text(&status, 24., 36., 21., WHITE);
        draw_circle(screen_width() * 0.5, screen_height() * 0.5, 2., WHITE);
        if shell.playing()
            && game
                .target(&session.world().room, session.controller())
                .is_some()
            && !game.state().finished()
        {
            super::game_text::draw_text(
                "E / X: interact",
                screen_width() * 0.5 - 70.,
                screen_height() * 0.5 + 32.,
                20.,
                WHITE,
            );
        }
        toast.draw();
        let controls = [
            "Move: WASD / arrows / left stick",
            "Look: mouse / right stick",
            "Jump: Space / A; crouch: Ctrl / B",
            "Interact / carry / replay: E / X (R replays)",
            "Camera: Q / RS; fullscreen: F / F11",
            "Save / load: F5 / F9 (local games)",
            "Menu: Esc / Start; confirm: Enter / A",
        ];
        let quit = if session.is_online() {
            shell.menu(&title, &controls)
        } else {
            shell.local_menu(&title, &controls)
        };
        if quit {
            break;
        }
        if let Some(dir) = &options.capture {
            let now_completed = game.state().completed;
            let now_round = game.state().round;
            if frame == 10 || (now_completed && !completed) || now_round != round {
                let name = if now_round != round {
                    format!("reset-{now_round}.png")
                } else if now_completed {
                    format!("win-{now_round}.png")
                } else {
                    "world.png".into()
                };
                get_screen_data()
                    .export_png(dir.join(name).to_str().ok_or("Invalid capture path")?);
            }
            history.push(serde_json::json!({"frame":frame,"tick":session.world().tick,"position":session.controller().position,"completed":now_completed,"round":now_round}));
            completed = now_completed;
            round = now_round;
            if playback_done || (playback.is_none() && frame == 40) {
                get_screen_data().export_png(
                    dir.join("menu.png")
                        .to_str()
                        .ok_or("Invalid capture path")?,
                );
                std::fs::write(dir.join("run.json"), serde_json::to_vec_pretty(&history)?)?;
                break;
            }
        } else if playback_done {
            break;
        }
        frame += 1;
        next_frame().await;
    }
    Ok(())
}

/// A short message near the top of the screen: what the last save or load did.
#[derive(Default)]
struct Toast {
    text: String,
    until: f64,
}
impl Toast {
    fn show(&mut self, text: String) {
        self.text = text;
        self.until = get_time() + 3.;
    }
    fn draw(&self) {
        if get_time() < self.until && !self.text.is_empty() {
            let size = 22.;
            let width = super::game_text::measure_text(&self.text, None, size as u16, 1.).width;
            let x = (screen_width() - width) * 0.5;
            draw_rectangle(
                x - 12.,
                52.,
                width + 24.,
                34.,
                Color::new(0.04, 0.08, 0.1, 0.85),
            );
            super::game_text::draw_text(&self.text, x, 76., size, WHITE);
        }
    }
}

fn quick_save(session: &GameSession, slots: &SaveSlots) -> String {
    if !session.can_save() {
        return "Saving is not available in an online game".into();
    }
    let label = format!("Quick save, tick {}", session.world().tick);
    match session.save_to_slot(slots, QUICK_SLOT, &label) {
        Ok(()) => "Game saved (F9 loads it)".into(),
        Err(error) => format!("Save failed: {error}"),
    }
}

fn quick_load(session: &mut GameSession, slots: &SaveSlots) -> String {
    if !session.can_save() {
        return "Loading is not available in an online game".into();
    }
    match session.load_from_slot(slots, QUICK_SLOT) {
        Ok((_, Source::Primary)) => "Game loaded".into(),
        Ok((_, Source::Backup(why))) => format!("Loaded the previous save ({why})"),
        Err(SaveError::NotFound(_)) => "No quick save yet (F5 saves)".into(),
        Err(error) => format!("Load failed: {error}"),
    }
}

/// `--load`: a slot name in the save directory, or the path of a save file.
fn load_target(session: &mut GameSession, slots: &SaveSlots, target: &str) -> Result<String> {
    let loaded = slots.open(target)?;
    let (header, source) = session.restore_loaded(&loaded)?;
    Ok(match source {
        Source::Primary => format!("Loaded {}", header.label),
        Source::Backup(why) => format!("Loaded the previous save ({why})"),
    })
}

struct GameView {
    camera: CameraRig,
    avatar: Avatar,
    props: Props,
    static_meshes: Vec<Mesh>,
    entities: Vec<Vec<Mesh>>,
    original: Vec<Vec<Vec<Vertex>>>,
    ids: Vec<String>,
    material: Material,
}
impl GameView {
    fn new(session: &GameSession, character: &str) -> Result<Self> {
        let world = session.world();
        let game = world.game.as_ref().unwrap();
        let mut ids: Vec<_> = game
            .document()
            .interactables
            .iter()
            .map(|i| i.entity.clone())
            .collect();
        for mover in &game.document().movers {
            if !ids.contains(&mover.entity) {
                ids.push(mover.entity.clone());
            }
        }
        let bounds: Vec<_> = ids
            .iter()
            .map(|id| {
                world
                    .room
                    .entities
                    .iter()
                    .find(|e| &e.id == id)
                    .unwrap()
                    .bounds
                    .clone()
            })
            .collect();
        let (static_meshes, entities) =
            mesh::bake_tagged_entities(&world.room.world, &world.room.render_tags(), &bounds);
        let original = entities
            .iter()
            .map(|ms| ms.iter().map(|m| m.vertices.clone()).collect())
            .collect();
        Ok(Self {
            camera: CameraRig::default(),
            avatar: Avatar::new(character)?,
            props: Props::new(world.prop_physics.as_ref().unwrap()),
            static_meshes,
            entities,
            original,
            ids,
            material: mesh::material()?,
        })
    }
    /// Forget camera smoothing: after a load the player is somewhere else entirely.
    fn reset_camera(&mut self) {
        self.camera = CameraRig::default();
    }
    fn draw(&mut self, session: &GameSession, perspective: Perspective, seconds: f32) {
        let world = session.world();
        let game = world.game.as_ref().unwrap();
        let pose = session.pose();
        self.camera
            .advance(perspective, &pose, &world.room, seconds);
        let view = self.camera.view(perspective, &pose, &world.room);
        set_camera(&Camera3D {
            position: mesh::vec(view.eye),
            target: mesh::vec(view.target),
            up: Vec3::Y,
            fovy: 55_f32.to_radians(),
            z_near: 0.025,
            z_far: 250.,
            ..Default::default()
        });
        self.material.set_uniform("Eye", mesh::vec(view.eye));
        self.material.set_uniform("ObjectStates", vec2(1., 0.));
        gl_use_material(&self.material);
        for mesh in &self.static_meshes {
            draw_mesh(mesh);
        }
        for (i, meshes) in self.entities.iter_mut().enumerate() {
            if game.visible_entity(&self.ids[i]) == Some(false) {
                continue;
            }
            let offset = game
                .movers()
                .iter()
                .find(|m| m.entity == self.ids[i])
                .map_or(crate::math::V::ZERO, |m| m.translation * m.progress());
            for (mesh, original) in meshes.iter_mut().zip(&self.original[i]) {
                for (v, base) in mesh.vertices.iter_mut().zip(original) {
                    v.position = base.position + super::mesh::vec(offset);
                }
                draw_mesh(mesh);
            }
        }
        self.props.draw(world.prop_physics.as_ref().unwrap());
        gl_use_default_material();
        if view.show_body {
            self.avatar.draw(&pose);
        }
        for player in session.remote_players().values() {
            self.avatar.draw(player);
        }
        set_default_camera();
    }
}

// A narrow lifetime guard for the existing listen-and-play entry point.
struct Hosted {
    address: std::net::SocketAddr,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<Result<()>>>,
}
impl Hosted {
    fn start<T: DatagramTransport + Send + 'static>(
        transport: T,
        world: super::simulation::HeadlessWorld,
        key: Option<&str>,
    ) -> Result<Self> {
        let mut server = super::server::DedicatedServer::with_transport(transport, world)?;
        if let Some(key) = key {
            server = server.with_auth(key);
        }
        let address = server.local_addr;
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let signal = stop.clone();
        let thread = std::thread::spawn(move || server.run_realtime(signal, None));
        Ok(Self {
            address,
            stop,
            thread: Some(thread),
        })
    }
}
impl Drop for Hosted {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            if let Ok(Err(error)) = thread.join() {
                eprintln!("Hosted server failed: {error}");
            }
        }
    }
}
