#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
use vesper3d::viewer::controller::CharacterKind;
mod character;
mod impact_audio;
mod platform_window;
mod prop_view;
mod wrench_view;
use macroquad::{
    input::utils::{register_input_subscriber, repeat_all_miniquad_input},
    prelude::*,
};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use vesper3d::{
    math::V,
    viewer::{
        camera::{CameraRig, Perspective},
        controller::{Controller, Movement},
        gamepad::{Button as PadButton, GamepadFrame, Gamepads},
        maps::{self, MapId},
        mesh,
        net::{
            DatagramTransport, Identity, InputFrame, InterpolationBuffer, Packet, PlayerNetState,
            PredictionBuffer, SecureSocket, TransportProfile, UdpTransport, PROTOCOL_VERSION,
        },
        prop_physics::PropPhysics,
        server::DedicatedServer,
        simulation::PlayerStepper,
        test_lab::{SPAWN_PLAYER_1, SPAWN_PLAYER_2},
        weapons::{Loadout, Weapon},
        wrench::Wrench,
    },
};

fn config() -> macroquad::conf::Conf {
    macroquad::conf::Conf {
        miniquad_conf: Conf {
            window_title: "Blue Engine 2 | BE2".into(),
            window_width: 960,
            window_height: 600,
            fullscreen: false,
            high_dpi: true,
            sample_count: 4,
            window_resizable: true,
            ..Default::default()
        },
        draw_call_vertex_capacity: 30000,
        draw_call_index_capacity: 30000,
        ..Default::default()
    }
}
#[derive(Default)]
struct Keys {
    down: HashSet<KeyCode>,
    pressed: HashSet<KeyCode>,
    #[cfg(windows)]
    native_down: HashSet<KeyCode>,
}
impl Keys {
    fn pressed(&self, key: KeyCode) -> bool {
        self.pressed.contains(&key) || is_key_pressed(key)
    }
    fn poll(&mut self, focused: bool) {
        self.pressed.clear();
        #[cfg(windows)]
        {
            // Read only the viewer's bound keys. Polling also accepts accessibility-injected
            // keys without hardware scan codes and prevents a missed key-up sticking.
            for (key, vk) in [
                (KeyCode::W, 0x57),
                (KeyCode::A, 0x41),
                (KeyCode::S, 0x53),
                (KeyCode::D, 0x44),
                (KeyCode::Up, 0x26),
                (KeyCode::Down, 0x28),
                (KeyCode::Left, 0x25),
                (KeyCode::Right, 0x27),
                (KeyCode::LeftShift, 0xA0),
                (KeyCode::RightShift, 0xA1),
                (KeyCode::Space, 0x20),
                (KeyCode::LeftControl, 0xA2),
                (KeyCode::RightControl, 0xA3),
                (KeyCode::C, 0x43),
                (KeyCode::Enter, 0x0D),
                (KeyCode::Escape, 0x1B),
                (KeyCode::Tab, 0x09),
                (KeyCode::F, 0x46),
                (KeyCode::F3, 0x72),
                (KeyCode::F11, 0x7A),
                (KeyCode::H, 0x48),
                (KeyCode::Q, 0x51),
                (KeyCode::E, 0x45),
            ] {
                // GetAsyncKeyState takes a virtual key integer and no pointers.
                let state = unsafe {
                    windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(vk)
                } as u16;
                let held = state & 0x8000 != 0;
                if focused && !self.native_down.contains(&key) && (held || state & 1 != 0) {
                    self.pressed.insert(key);
                }
                if held {
                    self.native_down.insert(key);
                } else {
                    self.native_down.remove(&key);
                }
                if focused && held {
                    self.down.insert(key);
                } else {
                    self.down.remove(&key);
                }
            }
        }
        if !focused {
            self.down.clear();
        }
    }
}
impl miniquad::EventHandler for Keys {
    fn update(&mut self) {}
    fn draw(&mut self) {}
    fn key_down_event(&mut self, key: KeyCode, _: miniquad::KeyMods, repeat: bool) {
        if !repeat {
            self.down.insert(key);
            self.pressed.insert(key);
        }
    }
    fn key_up_event(&mut self, key: KeyCode, _: miniquad::KeyMods) {
        self.down.remove(&key);
    }
}
fn axes(keys: &HashSet<KeyCode>) -> (f32, f32) {
    let held = |a, b| {
        if keys.contains(&a) || keys.contains(&b) {
            1.
        } else {
            0.
        }
    };
    (
        held(KeyCode::W, KeyCode::Up) - held(KeyCode::S, KeyCode::Down),
        held(KeyCode::D, KeyCode::Right) - held(KeyCode::A, KeyCode::Left),
    )
}
fn foreground() -> bool {
    #[cfg(windows)]
    {
        // Read-only check of this application's focus; both calls accept these arguments.
        unsafe {
            let mut pid = 0;
            windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
                windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow(),
                &mut pid,
            );
            pid == windows_sys::Win32::System::Threading::GetCurrentProcessId()
        }
    }
    #[cfg(not(windows))]
    {
        true
    }
}
fn capture(active: bool) {
    set_cursor_grab(active);
    show_mouse(!active);
}
thread_local! {static FONT: std::cell::RefCell<Option<Font>> = const {std::cell::RefCell::new(None)};}
fn text(s: &str, x: f32, y: f32, size: f32, c: Color) {
    FONT.with(|font| {
        draw_text_ex(
            s,
            x,
            y,
            TextParams {
                font: font.borrow().as_ref(),
                font_size: size as u16,
                color: c,
                ..Default::default()
            },
        );
    });
}
fn menu_scale() -> f32 {
    ((screen_height() - 24.) / 650.)
        .min((screen_width() - 24.) / 470.)
        .clamp(0.2, 1.)
}
fn menu_mouse() -> Vec2 {
    Vec2::from(mouse_position()) / menu_scale()
}
const INK: Color = Color::new(0.91, 0.95, 1., 1.);
const MUTED: Color = Color::new(0.57, 0.66, 0.77, 1.);
const BLUE: Color = Color::new(0.23, 0.55, 1., 1.);
fn button(s: &str, r: Rect, primary: bool) -> bool {
    let hover = r.contains(menu_mouse());
    draw_rectangle(
        r.x,
        r.y,
        r.w,
        r.h,
        if primary {
            if hover {
                Color::new(0.32, 0.62, 1., 1.)
            } else {
                BLUE
            }
        } else if hover {
            Color::new(0.17, 0.23, 0.32, 1.)
        } else {
            Color::new(0.10, 0.15, 0.22, 1.)
        },
    );
    text(s, r.x + 18., r.y + r.h * 0.5 + 7., 22., INK);
    hover && is_mouse_button_pressed(MouseButton::Left)
}
#[allow(clippy::too_many_arguments)]
fn slider(label: &str, x: f32, y: f32, w: f32, value: &mut f32, min: f32, max: f32, suffix: &str) {
    text(label, x, y, 19., MUTED);
    text(
        &format!("{:.0}{}", *value, suffix),
        x + w - 55.,
        y,
        19.,
        INK,
    );
    let r = Rect::new(x, y + 13., w, 24.);
    if r.contains(menu_mouse()) && is_mouse_button_down(MouseButton::Left) {
        *value = (min + (menu_mouse().x - x) / w * (max - min)).clamp(min, max);
    }
    let end = x + (*value - min) / (max - min) * w;
    draw_rectangle(x, y + 24., w, 3., Color::new(0.19, 0.26, 0.35, 1.));
    draw_rectangle(x, y + 24., end - x, 3., BLUE);
    draw_circle(end, y + 25., 6., INK);
}

#[macroquad::main(config)]
async fn main() {
    platform_window::maximize();
    // Process the queued maximize/resize before preparing or displaying the room.
    next_frame().await;
    #[cfg(windows)]
    if let Some(windows) = std::env::var_os("WINDIR") {
        let path = std::path::PathBuf::from(windows).join("Fonts/segoeui.ttf");
        if let Ok(font) = load_ttf_font(&path.to_string_lossy()).await {
            FONT.with(|f| *f.borrow_mut() = Some(font));
        }
    }
    clear_background(Color::new(0.035, 0.06, 0.10, 1.));
    text("BLUE ENGINE 2", 60., 90., 42., INK);
    text("Preparing the map...", 60., 132., 22., MUTED);
    next_frame().await;
    let started = std::time::Instant::now();
    let args: Vec<String> = std::env::args().collect();
    let transport_profile = match args
        .windows(2)
        .find(|a| a[0] == "--transport")
        .map(|a| a[1].parse::<TransportProfile>())
        .transpose()
    {
        Ok(profile) => profile.unwrap_or_default(),
        Err(error) => {
            error_screen(error).await;
            return;
        }
    };
    let physics_capture = args.iter().any(|a| a == "--capture-physics");
    let house_capture = args.iter().any(|a| a == "--capture-house");
    let map = if args.iter().any(|a| a == "--studio") {
        MapId::Studio
    } else if args.iter().any(|a| a == "--house") || house_capture {
        MapId::House
    } else {
        MapId::TestLab
    };
    let map_file = args.windows(2).find(|a| a[0] == "--map").map(|a| &a[1]);
    let game_file = args.windows(2).find(|a| a[0] == "--game").map(|a| &a[1]);
    if game_file.is_some() && map_file.is_some() {
        error_screen("Choose --game or --map, not both").await;
        return;
    }
    let loaded_game = if let Some(path) = game_file {
        match vesper3d::viewer::game::GameDocument::load(std::path::Path::new(path)) {
            Ok(game) => Some(game),
            Err(e) => {
                error_screen(&format!("Could not load game: {e}")).await;
                return;
            }
        }
    } else {
        None
    };
    let room_result = if let Some(loaded) = &loaded_game {
        loaded.map.build()
    } else {
        map_file.map_or_else(
            || maps::build(map),
            |p| {
                vesper3d::viewer::authoring::MapDocument::load(std::path::Path::new(p))
                    .and_then(|d| d.build_standalone())
            },
        )
    };
    let mut room = match room_result {
        Ok(room) => room,
        Err(e) => {
            error_screen(&format!("Could not load room: {e}")).await;
            return;
        }
    };
    let mut game = if let Some(loaded) = loaded_game {
        match vesper3d::viewer::game::GameRuntime::compile(loaded.document, &loaded.map) {
            Ok(game) => Some(game),
            Err(e) => {
                error_screen(&format!("Could not compile game: {e}")).await;
                return;
            }
        }
    } else {
        None
    };
    let content_hash = game.as_ref().map_or_else(
        || vesper3d::viewer::content::fingerprint(&room),
        |game| game.content_hash(&room),
    );
    let mut pending_game_interaction = false;
    let mut prop_physics = match PropPhysics::new(&mut room) {
        Ok(p) => p,
        Err(e) => {
            error_screen(&format!("Could not prepare prop physics: {e}")).await;
            return;
        }
    };
    let mut prop_view = prop_view::Props::new(&prop_physics);
    let (meshes, game_meshes) = if let Some(game) = &game {
        mesh::bake_tagged_entities(&room.world, &room.render_tags(), game.targets())
    } else {
        (
            mesh::bake_tagged(&room.world, &room.render_tags()),
            Vec::new(),
        )
    };
    let material = match mesh::material() {
        Ok(m) => m,
        Err(e) => {
            error_screen(&format!("Graphics initialization failed: {e}")).await;
            return;
        }
    };
    let setup_seconds = started.elapsed().as_secs_f32();
    let triangles: usize = meshes
        .iter()
        .chain(game_meshes.iter().flatten())
        .map(|m| m.indices.len() / 3)
        .sum();
    let props_capture = args.iter().any(|a| a == "--capture-props");
    let motion_capture = args.iter().any(|a| a == "--capture-motion");
    let character_capture = args.iter().any(|a| a == "--capture-character");
    let pistol_capture = args.iter().any(|a| a == "--capture-pistol");
    let wrench_capture = args.iter().any(|a| a == "--capture-wrench");
    let game_capture = args.iter().any(|a| a == "--capture-game");
    let camera_capture = args.iter().any(|a| a == "--capture-camera");
    let interaction_capture = args.iter().any(|a| a == "--capture-interactions");
    let capture_dir = args
        .windows(2)
        .find(|a| {
            a[0] == "--capture-pistol"
                || a[0] == "--capture-physics"
                || a[0] == "--capture-house"
                || a[0] == "--capture-props"
                || a[0] == "--capture-character"
                || a[0] == "--capture-wrench"
                || a[0] == "--capture"
                || a[0] == "--capture-motion"
                || a[0] == "--capture-game"
                || a[0] == "--capture-camera"
                || a[0] == "--capture-interactions"
        })
        .map(|a| std::path::PathBuf::from(&a[1]));
    if let Some(dir) = &capture_dir {
        if std::fs::create_dir_all(dir).is_err() {
            error_screen("Cannot create capture folder.").await;
            return;
        }
    }
    let capture_kind = if args.iter().any(|a| a == "--feta") {
        CharacterKind::Feta
    } else {
        CharacterKind::Scientist
    };
    let mut controller = if let Some(game) = &game {
        game.controller(1)
    } else {
        let Some(spawn) = room.default_spawn else {
            error_screen("Standalone map requires default_spawn").await;
            return;
        };
        match Controller::for_character_at(capture_kind, spawn.feet, spawn.yaw) {
            Ok(controller) => controller,
            Err(e) => {
                error_screen(&format!("Invalid map spawn: {e}")).await;
                return;
            }
        }
    };
    let mut character_chosen = capture_dir.is_some() || game.is_some();
    let mut stepper = PlayerStepper::default();
    let mut wrench = Wrench::default();
    let mut loadout = Loadout::default();
    let mut pistol_audio = impact_audio::ImpactAudio::pistol().await;
    let mut pistol_view = wrench_view::View::pistol();
    let mut impact_audio = impact_audio::ImpactAudio::new().await;
    let mut wrench_view = wrench_view::View::new();
    let mut character = character::Character::default();
    let mut perspective = if args.iter().any(|a| a == "--third-person") {
        Perspective::Third
    } else {
        Perspective::default()
    };
    let mut keys = Keys::default();
    let mut gamepads = Gamepads::new()
        .map_err(|error| {
            eprintln!("Controller input unavailable: {error}");
        })
        .ok();
    let mut menu_selection = 0usize;
    let mut character_selection = 0usize;
    let subscriber = register_input_subscriber();
    let mut active = false;
    let mut settings_open = false;
    let mut camera_rig = CameraRig::default();
    let mut entered = false;
    let mut skip_look = 0;
    let mut sensitivity = 50.;
    let mut fov: f32 = 90.;
    let mut invert = false;
    let mut hud = false;
    let mut debug = false;
    let mut fullscreen = false;
    let mut maximize_pending = false;
    let mut frame = 0;
    let mut samples = Vec::new();
    let mut captured_heights = Vec::new();

    let host_mode = args
        .windows(2)
        .find(|a| a[0] == "--server")
        .filter(|a| !a[1].starts_with("--"))
        .map(|a| a[1].clone())
        .or_else(|| {
            if args.iter().any(|a| a == "--server") {
                Some("127.0.0.1:4000".to_string())
            } else {
                None
            }
        });
    let client_auth_key = args
        .windows(2)
        .find(|a| a[0] == "--auth-key" || a[0] == "--auth")
        .map(|a| a[1].clone());

    if let Some(ref s_addr) = host_mode {
        let s_addr_clone = s_addr.clone();
        let host_game = game_file.cloned();
        let host_map = map_file.cloned();
        let host_transport = transport_profile;
        let host_auth_key = client_auth_key.clone();
        std::thread::spawn(move || {
            let world = if let Some(path) = host_game {
                vesper3d::viewer::game::GameDocument::load(std::path::Path::new(&path))
                    .and_then(|loaded| loaded.world())
            } else {
                let room = host_map.map_or_else(
                    || maps::build(map),
                    |path| {
                        vesper3d::viewer::authoring::MapDocument::load(std::path::Path::new(&path))
                            .and_then(|document| document.build_standalone())
                    },
                );
                room.and_then(vesper3d::viewer::simulation::HeadlessWorld::try_with_room)
            };
            let result = world.and_then(|world| match host_transport {
                TransportProfile::Development => {
                    let transport = UdpTransport::bind(&s_addr_clone)?;
                    let mut server = DedicatedServer::with_transport(transport, world)?;
                    if let Some(key) = host_auth_key.as_deref() {
                        server = server.with_auth(key);
                    }
                    server.run_realtime(
                        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                        None,
                    )
                }
                TransportProfile::Production => {
                    let transport = SecureSocket::server(s_addr_clone.parse()?, Identity::load()?)?;
                    let mut server = DedicatedServer::with_transport(transport, world)?;
                    if let Some(key) = host_auth_key.as_deref() {
                        server = server.with_auth(key);
                    }
                    server.run_realtime(
                        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                        None,
                    )
                }
            });
            if let Err(error) = result {
                eprintln!("Could not run {host_transport} host: {error}");
            }
        });
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    let connect_addr_str = args
        .windows(2)
        .find(|a| a[0] == "--connect")
        .map(|a| a[1].clone())
        .or(host_mode);

    let (mut net_transport, net_server_dest) = if let Some(ref addr_str) = connect_addr_str {
        let parsed = addr_str.parse::<SocketAddr>().or_else(|_| {
            use std::net::ToSocketAddrs;
            addr_str
                .to_socket_addrs()
                .ok()
                .and_then(|mut it| it.next())
                .ok_or_else(|| "Failed to resolve address".to_string())
        });
        match parsed {
            Ok(dest) => {
                let transport: vesper3d::Result<Box<dyn DatagramTransport>> =
                    match transport_profile {
                        TransportProfile::Development => {
                            let bind = if dest.is_ipv4() {
                                "0.0.0.0:0"
                            } else {
                                "[::]:0"
                            };
                            UdpTransport::bind(bind)
                                .map(|transport| Box::new(transport) as Box<dyn DatagramTransport>)
                        }
                        TransportProfile::Production => {
                            vesper3d::viewer::net::trusted_certificate()
                                .and_then(|certificate| SecureSocket::client(dest, certificate))
                                .map(|transport| Box::new(transport) as Box<dyn DatagramTransport>)
                        }
                    };
                match transport {
                    Ok(transport) => {
                        let _ = transport.send_packet(
                            &Packet::Hello {
                                protocol_version: PROTOCOL_VERSION,
                                content_hash,
                                player_id: 0,
                            },
                            dest,
                        );
                        (Some(transport), Some(dest))
                    }
                    Err(e) => {
                        eprintln!("Failed to start {transport_profile} transport: {e}");
                        (None, None)
                    }
                }
            }
            Err(e) => {
                eprintln!("Invalid server address '{addr_str}': {e}");
                (None, None)
            }
        }
    } else {
        (None, None)
    };

    let mut net_player_id: Option<u64> = None;
    let mut net_session_token: Option<vesper3d::viewer::net::SessionToken> = None;
    let mut prediction_buffer = PredictionBuffer::new(128);
    let mut remote_interpolators: HashMap<u64, InterpolationBuffer<PlayerNetState>> =
        HashMap::new();
    let mut remote_prop_interpolators: HashMap<
        String,
        InterpolationBuffer<vesper3d::viewer::net::PropNetState>,
    > = HashMap::new();
    let mut client_baseline_snapshot: Option<vesper3d::viewer::net::WorldSnapshot> = None;
    let mut client_acked_server_tick = 0u64;
    let mut remote_controllers: HashMap<u64, Controller> = HashMap::new();
    let mut remote_characters: HashMap<u64, character::Character> = HashMap::new();
    let mut client_tick = 0u64;
    let mut last_server_tick = 0u64;
    let mut last_hello_sent = std::time::Instant::now();
    let mut current_ping_ms = 0u64;

    loop {
        if let (Some(ref mut transport), Some(server_addr)) = (&mut net_transport, net_server_dest)
        {
            if net_player_id.is_none()
                && last_hello_sent.elapsed() > std::time::Duration::from_millis(500)
            {
                let _ = transport.send_packet(
                    &Packet::Hello {
                        protocol_version: PROTOCOL_VERSION,
                        content_hash,
                        player_id: 0,
                    },
                    server_addr,
                );
                last_hello_sent = std::time::Instant::now();
            }

            let incoming = transport.receive_packets().unwrap_or_default();
            for (packet, src) in incoming {
                if src == server_addr {
                    let mut incoming_snap = None;
                    match packet {
                        Packet::Rejected { reason } => {
                            error_screen(&reason).await;
                            return;
                        }
                        Packet::AuthChallenge { nonce, salt } => {
                            if let Some(ref key) = client_auth_key {
                                let proof = vesper3d::viewer::net::session::compute_auth_proof(
                                    key, nonce, 0, &salt,
                                );
                                let _ = transport.send_packet(
                                    &Packet::AuthResponse {
                                        player_id: 0,
                                        nonce,
                                        proof,
                                        content_hash,
                                    },
                                    server_addr,
                                );
                            } else {
                                error_screen("Server requires authentication (--auth-key <KEY>)")
                                    .await;
                                return;
                            }
                        }
                        Packet::Welcome {
                            player_id,
                            server_tick,
                            session_token,
                            ..
                        } => {
                            net_player_id = Some(player_id);
                            net_session_token = session_token;
                            last_server_tick = server_tick;
                            let spawn = match player_id {
                                1 => SPAWN_PLAYER_1,
                                2 => SPAWN_PLAYER_2,
                                n => V((n as f32 - 1.0) * 1.5, 1.68, 6.0),
                            };
                            if let Some(game) = &game {
                                controller = game.controller(player_id);
                            } else {
                                controller.position = spawn;
                            }
                            stepper.reset(&controller);
                            character_chosen = true;
                            active = true;
                        }
                        Packet::GameState { tick, state } => {
                            if let Some(game) = &mut game {
                                game.accept_snapshot(tick, state);
                                game.step_movers(&mut room);
                            }
                        }
                        Packet::Snapshot(snap) => {
                            client_baseline_snapshot = Some(snap.clone());
                            client_acked_server_tick = snap.tick;
                            incoming_snap = Some(snap);
                        }
                        Packet::Delta(delta) => {
                            if let Some(ref base) = client_baseline_snapshot {
                                if delta.base_tick == base.tick {
                                    let reconstructed = delta.apply_to(base);
                                    client_baseline_snapshot = Some(reconstructed.clone());
                                    client_acked_server_tick = reconstructed.tick;
                                    incoming_snap = Some(reconstructed);
                                } else {
                                    // Delta base mismatch due to packet loss or reordering: request keyframe recovery
                                    let _ = transport
                                        .send_packet(&Packet::RequestKeyframe, server_addr);
                                }
                            } else {
                                let _ =
                                    transport.send_packet(&Packet::RequestKeyframe, server_addr);
                            }
                        }
                        Packet::Pong { send_time_ms, .. } => {
                            let now_ms = (get_time() * 1000.0) as u64;
                            current_ping_ms = now_ms.saturating_sub(send_time_ms);
                        }
                        _ => {}
                    }

                    if let Some(snap) = incoming_snap {
                        last_server_tick = snap.tick;
                        if let Some(my_id) = net_player_id {
                            if let Some(my_server) = snap.players.iter().find(|p| p.id == my_id) {
                                prediction_buffer.reconcile(
                                    snap.ack_client_tick,
                                    my_server,
                                    &mut controller,
                                    &room.colliders,
                                    0.05,
                                );
                            }

                            // Reconcile authoritative prop ownership with local prop physics
                            for prop in &snap.props {
                                if let Some(prop_idx) =
                                    prop_physics.props.iter().position(|p| p.id == prop.id)
                                {
                                    if prop.held_by == Some(my_id) {
                                        prop_physics.set_held_for_player(my_id, prop_idx);
                                    } else if prop_physics.held_for_player(my_id) == Some(prop_idx)
                                    {
                                        prop_physics.drop_for_player(my_id);
                                    }
                                }
                            }
                        }
                        let active_ids: HashSet<u64> = snap.players.iter().map(|p| p.id).collect();
                        remote_controllers.retain(|id, _| active_ids.contains(id));
                        remote_interpolators.retain(|id, _| active_ids.contains(id));
                        remote_characters.retain(|id, _| active_ids.contains(id));

                        for p in &snap.players {
                            if Some(p.id) != net_player_id {
                                remote_interpolators
                                    .entry(p.id)
                                    .or_insert_with(|| InterpolationBuffer::new(32))
                                    .push(snap.tick, p.clone());
                            }
                        }

                        for prop in &snap.props {
                            remote_prop_interpolators
                                .entry(prop.id.clone())
                                .or_insert_with(|| InterpolationBuffer::new(32))
                                .push(snap.tick, prop.clone());
                        }
                    }
                }
            }

            let render_tick = (last_server_tick as f32) - 2.0;
            for (&pid, interp) in &remote_interpolators {
                if let Some(state) = interp.interpolate_state_at(render_tick) {
                    let remote_c = remote_controllers.entry(pid).or_insert_with(|| {
                        let mut c = game.as_ref().map_or_else(
                            || Controller::for_character(state.character_kind),
                            |g| g.controller(pid),
                        );
                        c.position = state.position;
                        c
                    });
                    remote_c.position = state.position;
                    remote_c.yaw = state.yaw;
                    remote_c.pitch = state.pitch;
                }
            }

            // Interpolate remote props authoritatively
            let mut props_updated = false;
            for (prop_id, interp) in &remote_prop_interpolators {
                if let Some(state) = interp.interpolate_at(render_tick) {
                    let is_held_by_me =
                        net_player_id.is_some_and(|my_id| state.held_by == Some(my_id));
                    if !is_held_by_me {
                        prop_physics.set_prop_transform_and_vel(
                            prop_id,
                            state.position,
                            state.rotation,
                            state.linear_velocity,
                            state.angular_velocity,
                            state.sleeping,
                        );
                        props_updated = true;
                    }
                }
            }
            if props_updated {
                prop_physics.sync(&mut room);
            }
        }

        let previous_position = controller.position;
        let focused = foreground();
        let pad = gamepads
            .as_mut()
            .map(|pads| pads.poll(focused))
            .unwrap_or_default();
        keys.poll(focused);
        repeat_all_miniquad_input(&mut keys, subscriber);
        if !focused {
            keys.down.clear();
            keys.pressed.clear();
        }
        if active && !focused {
            active = false;
            capture(false);
            controller.stop();
            stepper.reset(&controller);
            wrench.cancel();
            prop_physics.pause();
            keys.down.clear();
        }
        // Fullscreen requests are applied by the backend between frames.
        if maximize_pending {
            platform_window::maximize();
            maximize_pending = false;
        }
        if focused && (keys.pressed(KeyCode::F) || keys.pressed(KeyCode::F11)) {
            fullscreen = !fullscreen;
            set_fullscreen(fullscreen);
            maximize_pending = !fullscreen;
            skip_look = 3;
        }
        if focused && (keys.pressed(KeyCode::Q) || pad.pressed(PadButton::North)) {
            perspective.toggle();
            skip_look = 3;
        }
        if focused && keys.pressed(KeyCode::F3) {
            debug = !debug;
        }
        if focused && keys.pressed(KeyCode::H) && active {
            hud = !hud;
        }
        if character_chosen
            && focused
            && (keys.pressed(KeyCode::Escape)
                || keys.pressed(KeyCode::Tab)
                || pad.pressed(PadButton::Start)
                || (!active && pad.pressed(PadButton::East)))
        {
            if settings_open {
                settings_open = false;
            } else {
                active = !active;
            }
            entered |= active;
            capture(active);
            controller.stop();
            stepper.reset(&controller);
            wrench.cancel();
            prop_physics.pause();
            keys.down.clear();
            skip_look = 3;
        }
        camera_rig.advance(
            perspective,
            &controller,
            &room,
            if capture_dir.is_some() {
                1. / 60.
            } else if active {
                get_frame_time()
            } else {
                0.
            },
        );
        if active && capture_dir.is_none() {
            // Do not let menu confirmation or held controls leak into the resume frame.
            let gameplay_pad = if skip_look == 0 {
                pad.clone()
            } else {
                GamepadFrame::default()
            };
            let interact = keys.pressed(KeyCode::E) || gameplay_pad.pressed(PadButton::West);
            let fire = is_mouse_button_pressed(MouseButton::Left)
                || gameplay_pad.pressed(PadButton::RightTrigger2);
            if skip_look > 0 {
                skip_look -= 1;
            } else {
                let d = mouse_delta_position();
                controller.look(
                    -d.x * screen_width() * 0.5,
                    -d.y * screen_height() * 0.5,
                    0.0004 + sensitivity * 0.00004,
                    invert,
                );
            }
            let look = gameplay_pad.look_delta(get_frame_time());
            controller.look(look[0], look[1], 1., invert);
            // Preserve even a complete tap occurring between two rendered frames.
            let mut movement_keys = keys.down.clone();
            movement_keys.extend(keys.pressed.iter().copied());
            let (f, r) = axes(&movement_keys);
            let movement = gameplay_pad.movement(Movement {
                forward: f,
                right: r,
                sprint: keys.down.contains(&KeyCode::LeftShift)
                    || keys.down.contains(&KeyCode::RightShift),
                jump: keys.pressed(KeyCode::Space),
                crouch: movement_keys.contains(&KeyCode::LeftControl)
                    || movement_keys.contains(&KeyCode::RightControl)
                    || movement_keys.contains(&KeyCode::C),
            });
            let simulation_steps =
                stepper.advance(&mut controller, movement, get_frame_time(), &room.colliders);
            if let (Some(ref transport), Some(server_addr), Some(_)) =
                (&net_transport, net_server_dest, net_player_id)
            {
                client_tick += 1;
                let input_frame = InputFrame {
                    client_tick,
                    movement,
                    yaw: controller.yaw,
                    pitch: controller.pitch,
                    fire_wrench: game.is_none() && fire && loadout.selected == Weapon::Wrench,
                    fire_pistol: game.is_none() && fire && loadout.selected == Weapon::Pistol,
                    interact,
                    ack_server_tick: client_acked_server_tick,
                    session_token: net_session_token,
                };
                let _ = transport.send_packet(&Packet::Input(input_frame.clone()), server_addr);
                prediction_buffer.push(input_frame, controller.clone());
            }
            loadout.pistol.tick(get_frame_time());
            if loadout.scroll(
                mouse_wheel().1 + f32::from(gameplay_pad.pressed(PadButton::RightTrigger))
                    - f32::from(gameplay_pad.pressed(PadButton::LeftTrigger)),
                controller.character_kind(),
                skip_look == 0 && active,
                prop_physics.held().is_some(),
            ) {
                wrench.cancel();
            }
            if game.is_none()
                && Loadout::can_use(
                    controller.character_kind(),
                    active,
                    prop_physics.held().is_some(),
                )
                && fire
            {
                if loadout.selected == Weapon::Wrench {
                    wrench.start(active, skip_look == 0);
                } else {
                    loadout.pistol.fire(
                        skip_look == 0,
                        &room,
                        camera_rig
                            .view(perspective, &controller, &room)
                            .aim(&controller, &room),
                    );
                }
            }
            if net_transport.is_none() {
                if let Some(game) = &mut game {
                    if simulation_steps > 0 {
                        game.step_movers(&mut room);
                        game.step_timers();
                    }
                    pending_game_interaction |= interact && skip_look == 0;
                    if simulation_steps > 0 && std::mem::take(&mut pending_game_interaction) {
                        game.interact(&room, &controller, 1);
                    }
                    if simulation_steps > 0 {
                        game.step_triggers(&controller, 1);
                    }
                } else if interact && skip_look == 0 {
                    let ray = camera_rig
                        .view(perspective, &controller, &room)
                        .aim(&controller, &room);
                    if prop_physics.toggle(&room, ray) {
                        wrench.cancel();
                    }
                }
                prop_physics.advance(get_frame_time(), &controller, &mut room);
            } else if let Some(my_id) = net_player_id {
                if prop_physics.held_for_player(my_id).is_some() {
                    let mut players = HashMap::new();
                    players.insert(my_id, &controller);
                    prop_physics.step_simulation_with_players(
                        get_frame_time(),
                        &players,
                        &mut room,
                    );
                }
            }
            wrench.tick(
                get_frame_time(),
                &room,
                camera_rig
                    .view(perspective, &controller, &room)
                    .aim(&controller, &room),
            );
        }
        if capture_dir.is_some() && camera_capture {
            if frame == 0 {
                controller = Controller::for_character(CharacterKind::Feta);
                controller.position = V(0., 0.22, 2.3);
                controller.yaw = 0.;
                controller.pitch = -0.08;
            }
            perspective = Perspective::Third;
            entered = true;
            if frame < 64 {
                controller.update(
                    Movement {
                        forward: 1.,
                        ..Default::default()
                    },
                    1. / 60.,
                    &room.colliders,
                );
            }
        } else if capture_dir.is_some() && game_capture {
            entered = true;
            if let Some(game) = &mut game {
                let index = (frame / 8).min(game.document().interactables.len() - 1);
                let id = &game.document().interactables[index].entity;
                if let Some(entity) = room.entities.iter().find(|e| &e.id == id) {
                    let center = (entity.bounds.min + entity.bounds.max) * 0.5;
                    controller = Controller::for_profile(
                        game.document().player_profile,
                        V(center.0, 0., center.2 + 2.),
                        0.,
                    )
                    .unwrap();
                    let aim = center - controller.position;
                    controller.pitch = (aim.1 / aim.length()).asin();
                    if frame % 8 == 4 {
                        game.interact(&room, &controller, 1);
                    }
                }
            }
        } else if capture_dir.is_some() && pistol_capture {
            controller.position = V(0., 1.68, 2.5);
            controller.yaw = 0.;
            controller.pitch = -0.04;
            loadout.pistol.tick(1. / 60.);
            if (frame == 1 || frame == 84 || frame == 96)
                && loadout.scroll(1., controller.character_kind(), true, false)
            {
                wrench.cancel();
            }
            if [20, 40, 64].contains(&frame)
                && controller.character_kind() == CharacterKind::Scientist
            {
                loadout
                    .pistol
                    .fire(loadout.selected == Weapon::Pistol, &room, controller.ray());
            }
            perspective = if (55..84).contains(&frame) {
                Perspective::Third
            } else {
                Perspective::First
            };
        } else if capture_dir.is_some() && physics_capture {
            if frame == 0 {
                controller.position.0 = 2.32;
                controller.position.2 = 2.7;
                controller.yaw = std::f32::consts::PI;
                let target = V(2.32, 1.02, 3.8) - controller.position;
                controller.pitch = (target.1 / target.length()).asin();
                perspective = Perspective::Third;
            }
            if frame == 2 {
                prop_physics.toggle(&room, controller.ray());
            }
            if (30..90).contains(&frame) {
                controller.position.0 += 0.015;
                controller.pitch = 0.2;
            }
            if frame == 95 {
                prop_physics.drop_held();
            }
            prop_physics.advance(1. / 60., &controller, &mut room);
        } else if capture_dir.is_some() && house_capture {
            let (eye, yaw, pitch) = match frame / 12 {
                0 => (V(14., 10., 17.), -0.69, -0.29),
                1 => (V(-1.35, 1.68, 3.65), -0.85, -0.12),
                2 => (V(-0.9, 1.68, -1.1), -1.10, -0.16),
                3 => (V(4.5, 1.68, 4.1), 0., 0.25),
                4 => (V(-1.3, 4.88, 0.5), -0.45, -0.18),
                5 => (V(-1., 2.1, -13.7), 2.85, -0.05),
                6 => (V(1.25, 4.88, -3.45), -0.20, -0.45),
                7 => (V(1.25, 4.88, 1.05), -0.55, -0.20),
                8 => (V(-2.0, 1.68, 0.95), -2.69, -0.12),
                9 => (V(-8.4, 1.68, 5.5), 0.10, -0.10),
                10 => (V(4.4, 0.98, -11.4), std::f32::consts::PI, 0.0),
                _ => (V(-1., 2.1, -13.7), 2.85, -0.05),
            };
            controller.position = eye;
            controller.yaw = yaw;
            controller.pitch = pitch;
        } else if capture_dir.is_some() && character_capture {
            if frame == 0 {
                controller.position.2 = 2.7;
                controller.yaw = 0.;
                controller.pitch = -0.10;
            }
            perspective = if frame < 12 || (60..72).contains(&frame) {
                Perspective::First
            } else {
                Perspective::Third
            };
            if frame == 24 && controller.character_kind() == CharacterKind::Scientist {
                wrench.start(true, true);
            }
            if frame >= 36 {
                controller.update(
                    Movement {
                        crouch: frame < 48,
                        jump: frame == 54,
                        ..Default::default()
                    },
                    1. / 60.,
                    &room.colliders,
                );
            }
            wrench.tick(
                1. / 60.,
                &room,
                camera_rig
                    .view(perspective, &controller, &room)
                    .aim(&controller, &room),
            );
        } else if capture_dir.is_some() && wrench_capture {
            controller.position = V(-3.3, 1.34, -1.6);
            controller.yaw = -std::f32::consts::FRAC_PI_2;
            controller.pitch = 0.;
            if frame == 12 {
                wrench.start(true, true);
            }
            wrench.tick(1. / 60., &room, controller.ray());
        } else if capture_dir.is_some() && interaction_capture {
            match frame / 12 {
                0 | 1 => {
                    controller.position = V(-3.3, 1.34, -1.6);
                    controller.yaw = -std::f32::consts::FRAC_PI_2;
                    controller.pitch = 0.;
                }
                2 | 3 => {
                    controller.position = V(-1.65, 1.55, 0.);
                    controller.yaw = 0.;
                    controller.pitch = 0.;
                }
                _ => {
                    controller.position = V(2.12, 1.68, 1.6);
                    controller.yaw = 0.;
                    controller.pitch = -0.855;
                }
            }
            if [12, 48].contains(&frame) {
                wrench.start(true, true);
            }
            wrench.tick(1. / 60., &room, controller.ray());
        } else if capture_dir.is_some() && motion_capture {
            controller.update(
                Movement {
                    jump: frame == 1,
                    crouch: (25..55).contains(&frame),
                    ..Default::default()
                },
                1. / 60.,
                &room.colliders,
            );
        } else if capture_dir.is_some() && props_capture {
            controller.position = match frame / 12 {
                0 => V(2.7, 1.68, 1.4),
                1 => V(2.32, 1.30, 2.9),
                _ => V(1.25, 1.4, 2.3),
            };
            controller.yaw = std::f32::consts::PI;
            controller.pitch = if frame < 12 { -0.35 } else { -0.25 };
        } else if capture_dir.is_some() {
            match frame / 12 {
                0 => {}
                1 => {
                    controller.position = V(-2., 1.68, 1.5);
                    controller.yaw = 0.9;
                    controller.pitch = -0.08;
                }
                2 => {
                    controller.position = V(0., 1.68, -3.6);
                    controller.yaw = -1.3;
                    controller.pitch = -0.04;
                }
                _ => {}
            }
        }
        impact_audio.update(wrench.hits);
        pistol_audio.update(loadout.pistol.shots);
        if active || capture_dir.is_some() {
            let d = controller.position - previous_position;
            let distance = (d.0 * d.0 + d.2 * d.2).sqrt();
            character.update(
                distance,
                if capture_dir.is_some() {
                    1. / 60.
                } else {
                    get_frame_time()
                },
                controller.is_grounded() && distance > 0.0001,
            );
        }
        let render_controller = if active && capture_dir.is_none() {
            stepper.pose(&controller)
        } else {
            controller.clone()
        };
        let view = camera_rig.view(perspective, &render_controller, &room);
        let mut camera_eye = view.eye;
        let mut camera_target = view.target;
        // Capture-only front portrait exposes the default skin for visual QA.
        if character_capture && (72..96).contains(&frame) {
            camera_eye = controller.position
                + if controller.character_kind() == CharacterKind::Feta {
                    V(0.65, 0.28, -1.1)
                } else {
                    V(1.4, -0.25, -2.8)
                };
            camera_target = controller.position
                + if controller.character_kind() == CharacterKind::Feta {
                    V(0., -0.02, 0.)
                } else {
                    V(0., -0.55, 0.)
                };
        }
        clear_background(Color::new(0.48, 0.70, 0.86, 1.));
        set_camera(&Camera3D {
            position: mesh::vec(camera_eye),
            target: mesh::vec(camera_target),
            up: vec3(0., 1., 0.),
            fovy: fov.to_radians(),
            z_near: 0.045,
            z_far: 60.,
            ..Default::default()
        });
        material.set_uniform("Eye", mesh::vec(camera_eye));
        // Legacy studio surfaces keep their default appearance; seeker input is melee only.
        material.set_uniform("ObjectStates", vec2(1., 0.));
        gl_use_material(&material);
        for m in &meshes {
            draw_mesh(m);
        }
        if let Some(game) = &game {
            for (index, entity_meshes) in game_meshes.iter().enumerate() {
                let moving = game
                    .document()
                    .movers
                    .iter()
                    .position(|mover| mover.entity == game.document().interactables[index].entity)
                    .and_then(|mover| game.mover_progress(mover))
                    .is_some_and(|progress| progress > 0.001);
                if game.visible(index) && !moving {
                    for mesh in entity_meshes {
                        draw_mesh(mesh);
                    }
                }
            }
        }
        prop_view.draw(&prop_physics);
        gl_use_default_material();
        if let Some(game) = &game {
            let aimed = game
                .target(&room, &controller)
                .filter(|index| game.visible(*index))
                .filter(|_| !game.state().completed);
            for (i, zone) in game.trigger_zones().iter().enumerate() {
                let center = mesh::vec((zone.min + zone.max) * 0.5);
                let size = mesh::vec(zone.max - zone.min);
                let color = if game.zone_enabled(i) {
                    Color::new(0.2, 0.75, 1.0, 0.35)
                } else {
                    Color::new(0.4, 0.4, 0.4, 0.15)
                };
                draw_cube_wires(center, size, color);
            }
            for (i, target) in game.targets().iter().enumerate() {
                if !game.visible(i) {
                    continue;
                }
                let center = mesh::vec((target.min + target.max) * 0.5);
                let size = mesh::vec(target.max - target.min);
                if !game.enabled(i) {
                    draw_cube(
                        center,
                        size * 1.002,
                        None,
                        Color::new(0.02, 0.03, 0.05, 0.6),
                    );
                    draw_cube_wires(center, size * 1.002, Color::new(0.5, 0.2, 0.2, 0.4));
                } else if aimed == Some(i) {
                    draw_cube_wires(center, size * 1.025, Color::new(1.0, 0.84, 0.2, 0.95));
                }
            }
            for i in 0..game.mover_count() {
                if let Some(bounds) = game.mover_bounds(i) {
                    let progress = game.mover_progress(i).unwrap_or(0.);
                    if progress > 0.001 {
                        let center = mesh::vec((bounds.min + bounds.max) * 0.5);
                        let size = mesh::vec(bounds.max - bounds.min);
                        draw_cube(center, size * 1.001, None, Color::new(0.3, 0.45, 0.65, 0.9));
                        draw_cube_wires(center, size * 1.001, Color::new(0.4, 0.7, 0.95, 0.8));
                    }
                }
            }
        }
        if view.show_body {
            character.draw(
                &render_controller,
                &wrench,
                if loadout.selected == Weapon::Pistol {
                    &pistol_view
                } else {
                    &wrench_view
                },
                prop_physics.held().is_some(),
            );
        }
        for (&pid, remote_c) in &mut remote_controllers {
            let remote_char = remote_characters.entry(pid).or_default();
            remote_char.draw(remote_c, &wrench, &wrench_view, false);
        }
        if let Some(hit) = if loadout.selected == Weapon::Pistol {
            &loadout.pistol.impact
        } else {
            &wrench.impact
        } {
            let p = mesh::vec(hit.point + hit.normal * 0.015);
            for i in 0..12 {
                let a = i as f32 * 2.399;
                let velocity = vec3(a.cos(), (i as f32 * 1.7).sin().abs(), a.sin()) * 0.7
                    + mesh::vec(hit.normal) * 0.8;
                let tip = p + velocity * hit.age + vec3(0., -1.8 * hit.age * hit.age, 0.);
                if hit.age < 0.3 {
                    draw_line_3d(
                        tip,
                        tip - velocity * 0.035,
                        Color::new(1., 0.72, 0.25, 1. - hit.age / 0.3),
                    );
                }
            }
        }
        set_default_camera();
        if game.is_none()
            && controller.character_kind() == CharacterKind::Scientist
            && prop_physics.held().is_none()
            && (perspective == Perspective::First || !view.show_body)
        {
            if loadout.selected == Weapon::Pistol {
                pistol_view.draw_pistol(&loadout.pistol);
            } else {
                wrench_view.draw(&wrench);
            }
        }
        let sw = screen_width();
        let sh = screen_height();
        if let Some(game) = &game {
            if active || capture_dir.is_some() {
                draw_rectangle(18., 16., 440., 82., Color::new(0.02, 0.04, 0.08, 0.85));
                text(&game.document().name, 30., 42., 22., INK);
                let status = if game.state().completed {
                    "Objective complete!".to_string()
                } else {
                    game.document()
                        .counters
                        .keys()
                        .zip(&game.state().counters)
                        .map(|(id, value)| format!("{id}: {value}"))
                        .collect::<Vec<_>>()
                        .join("   ")
                };
                text(&status, 30., 70., 20., INK);
                if let Some(index) = game
                    .target(&room, &controller)
                    .filter(|index| game.visible(*index))
                    .filter(|_| !game.state().completed)
                {
                    let target = &game.document().interactables[index].entity;
                    let status = if game.enabled(index) {
                        "E  Interact"
                    } else {
                        "Inactive / locked"
                    };
                    text(&format!("{target}   {status}"), 30., 94., 18., INK);
                }
            }
        }
        if (active || capture_dir.is_some()) && hud {
            draw_rectangle(
                24.,
                sh - 103.,
                sw.min(780.) - 48.,
                79.,
                Color::new(0.025, 0.045, 0.08, 0.8),
            );
            text(
                if controller.character_kind() == CharacterKind::Feta {
                    "WASD / Arrows   Scurry     Mouse   Look"
                } else {
                    "WASD / Arrows   Move     Mouse   Look     Shift   Sprint"
                },
                38.,
                sh - 80.,
                18.,
                INK,
            );
            text(
                "Space   Small jump     Hold Ctrl / C   Crouch     Esc   Pause",
                38.,
                sh - 57.,
                18.,
                INK,
            );
            text(
                if game.is_some() {
                    "E  Interact with switches / exit"
                } else if controller.character_kind() == CharacterKind::Feta {
                    "E   Pick up / Drop     Q   Switch camera"
                } else {
                    "E Pick up/drop   Click Attack   Scroll Weapon   Q Camera"
                },
                38.,
                sh - 34.,
                18.,
                INK,
            );
        }
        if game.is_none() && (active || physics_capture) {
            let prompt = if let Some(p) = prop_physics.held() {
                Some(format!("E  Drop {}", p.label))
            } else {
                prop_physics
                    .target(&room, view.aim(&controller, &room))
                    .map(|i| format!("E  Pick up {}", prop_physics.props[i].label))
            };
            if let Some(prompt) = prompt {
                let width = prompt.len() as f32 * 10. + 28.;
                draw_rectangle(
                    (sw - width) * 0.5,
                    sh * 0.5 + 28.,
                    width,
                    32.,
                    Color::new(0.02, 0.035, 0.055, 0.8),
                );
                text(&prompt, (sw - width) * 0.5 + 14., sh * 0.5 + 50., 18., INK);
            }
        }
        if (active || capture_dir.is_some())
            && game.is_none()
            && controller.character_kind() == CharacterKind::Scientist
            && prop_physics.held().is_none()
        {
            text(
                if loadout.selected == Weapon::Pistol {
                    "Pistol  /  Unlimited ammo"
                } else {
                    "Wrench"
                },
                28.,
                sh - 22.,
                18.,
                MUTED,
            );
        }
        if let Some(pid) = net_player_id {
            let net_info = format!(
                "MULTIPLAYER | Player #{} | Server Tick {} | Remote Players: {} | Ping: {}ms",
                pid,
                last_server_tick,
                remote_controllers.len(),
                current_ping_ms
            );
            text(
                &net_info,
                28.,
                sh - 46.,
                18.,
                Color::new(0.35, 0.95, 0.55, 0.95),
            );
        }
        if active || capture_dir.is_some() {
            let (crosshair_color, crosshair_radius) = if let Some(game) = &game {
                if let Some(target) = game
                    .target(&room, &controller)
                    .filter(|index| game.visible(*index))
                    .filter(|_| !game.state().completed)
                {
                    if game.enabled(target) {
                        (Color::new(0.25, 0.95, 0.55, 0.95), 3.5)
                    } else {
                        (Color::new(0.95, 0.35, 0.35, 0.85), 2.5)
                    }
                } else {
                    (Color::new(0.92, 0.96, 1., 0.85), 2.)
                }
            } else if prop_physics
                .target(&room, view.aim(&controller, &room))
                .is_some()
            {
                (Color::new(1., 0.82, 0.3, 0.95), 3.0)
            } else {
                (Color::new(0.92, 0.96, 1., 0.85), 2.)
            };
            draw_circle(sw * 0.5, sh * 0.5, crosshair_radius, crosshair_color);
            if let Some(hit) = if loadout.selected == Weapon::Pistol {
                &loadout.pistol.impact
            } else {
                &wrench.impact
            } {
                let c = Color::new(1., 0.76, 0.35, 1. - hit.age / 0.65);
                let (cx, cy) = (sw * 0.5, sh * 0.5);
                for (x, y) in [(-1., -1.), (1., 1.), (-1., 1.), (1., -1.)] {
                    draw_line(cx + x * 5., cy + y * 5., cx + x * 11., cy + y * 11., 2., c);
                }
            }
        }

        if !character_chosen {
            if pad.pressed(PadButton::DPadUp) || pad.pressed(PadButton::DPadDown) {
                character_selection = 1 - character_selection;
            }
            let scale = menu_scale();
            let (w, h) = (sw / scale, sh / scale);
            let mut camera = Camera2D::from_display_rect(Rect::new(0., 0., w, h));
            camera.zoom.y = -camera.zoom.y;
            set_camera(&camera);
            draw_rectangle(0., 0., w, h, Color::new(0.01, 0.025, 0.055, 0.8));
            let x = (w - 460.) * 0.5;
            let y = (h - 330.) * 0.5;
            text("Choose your character", x, y + 36., 36., INK);
            text(&room.name, x, y + 72., 20., MUTED);
            let feta = button(
                "Feta  /  Lab rat",
                Rect::new(x, y + 102., 460., 56.),
                character_selection == 0,
            ) || (character_selection == 0 && pad.pressed(PadButton::South));
            text(
                "Small, white, red-eyed. Fast on all four paws.",
                x,
                y + 184.,
                19.,
                INK,
            );
            let scientist = button(
                "The Scientist  /  Seeker",
                Rect::new(x, y + 214., 460., 56.),
                character_selection == 1,
            ) || (character_selection == 1 && pad.pressed(PadButton::South));
            text(
                "Lab coat, eyeglasses, wrench and pistol.",
                x,
                y + 296.,
                19.,
                INK,
            );
            if feta || scientist {
                controller = Controller::for_character(if feta {
                    CharacterKind::Feta
                } else {
                    CharacterKind::Scientist
                });
                perspective = if args.iter().any(|a| a == "--third-person") {
                    Perspective::Third
                } else {
                    Perspective::First
                };
                character_chosen = true;
                active = true;
                entered = true;
                stepper.reset(&controller);
                keys.down.clear();
                wrench.cancel();
                prop_physics.pause();
                capture(true);
                skip_look = 3;
            }
        }
        if character_chosen
            && !active
            && (capture_dir.is_none()
                || (game_capture && frame >= 30)
                || (camera_capture && frame >= 64)
                || (pistol_capture && frame >= 108)
                || ((interaction_capture || wrench_capture) && frame >= 60)
                || (character_capture && frame >= 96)
                || (house_capture && frame >= 132))
        {
            let scale = menu_scale();
            let sw = sw / scale;
            let sh = sh / scale;
            let mut ui_camera = Camera2D::from_display_rect(Rect::new(0., 0., sw, sh));
            ui_camera.zoom.y = -ui_camera.zoom.y;
            set_camera(&ui_camera);
            draw_rectangle(0., 0., sw, sh, Color::new(0.01, 0.025, 0.055, 0.53));
            if capture_dir.is_some() && args.iter().any(|a| a == "--capture-settings") {
                settings_open = true;
            }
            let panel_w = if settings_open { 460_f32 } else { 320_f32 }.min(sw - 32.);
            let panel_h = if settings_open { 600. } else { 280. };
            let x = (sw - panel_w) * 0.5;
            let y = (sh - panel_h) * 0.5;
            draw_rectangle(
                x,
                y,
                panel_w,
                panel_h,
                Color::new(0.025, 0.046, 0.077, 0.97),
            );
            let left = x + 28.;
            let width = panel_w - 56.;
            text(
                if settings_open {
                    "Settings"
                } else if entered {
                    "Paused"
                } else {
                    "Play"
                },
                left,
                y + 48.,
                28.,
                INK,
            );
            if !settings_open {
                if pad.pressed(PadButton::DPadDown) {
                    menu_selection = (menu_selection + 1) % 3;
                }
                if pad.pressed(PadButton::DPadUp) {
                    menu_selection = (menu_selection + 2) % 3;
                }
                let confirm = pad.pressed(PadButton::South);
                if button(
                    if entered { "Resume" } else { "Start" },
                    Rect::new(left, y + 78., width, 48.),
                    menu_selection == 0,
                ) || (focused && keys.pressed(KeyCode::Enter))
                    || (menu_selection == 0 && confirm)
                {
                    active = true;
                    entered = true;
                    keys.down.clear();
                    controller.stop();
                    stepper.reset(&controller);
                    wrench.cancel();
                    prop_physics.pause();
                    capture(true);
                    skip_look = 3;
                }
                if button(
                    "Settings",
                    Rect::new(left, y + 140., width, 48.),
                    menu_selection == 1,
                ) || (menu_selection == 1 && confirm)
                {
                    settings_open = true;
                }
                if button(
                    "Quit",
                    Rect::new(left, y + 202., width, 48.),
                    menu_selection == 2,
                ) || (menu_selection == 2 && confirm)
                {
                    break;
                }
            } else {
                slider(
                    "Mouse sensitivity",
                    left,
                    y + 86.,
                    width,
                    &mut sensitivity,
                    1.,
                    100.,
                    "%",
                );
                slider(
                    "Field of view",
                    left,
                    y + 153.,
                    width,
                    &mut fov,
                    50.,
                    90.,
                    " deg",
                );
                if button(
                    if invert {
                        "Invert look: On"
                    } else {
                        "Invert look: Off"
                    },
                    Rect::new(left, y + 204., width, 38.),
                    false,
                ) {
                    invert = !invert;
                }
                if button(
                    if hud { "Hints: On" } else { "Hints: Off" },
                    Rect::new(left, y + 252., width, 38.),
                    false,
                ) {
                    hud = !hud;
                }
                if button(
                    "Reset position",
                    Rect::new(left, y + 300., width, 38.),
                    false,
                ) {
                    prop_physics.drop_held();
                    controller = game.as_ref().map_or_else(
                        || Controller::for_character(controller.character_kind()),
                        |g| g.controller(net_player_id.unwrap_or(1)),
                    );
                    stepper.reset(&controller);
                    keys.down.clear();
                }
                text("Controls", left, y + 373., 20., INK);
                text(
                    "WASD / arrows   Move     Mouse   Look",
                    left,
                    y + 402.,
                    17.,
                    MUTED,
                );
                text(
                    "Space   Jump     Ctrl / C   Crouch",
                    left,
                    y + 426.,
                    17.,
                    MUTED,
                );
                text(
                    if game.is_some() {
                        "E   Interact     Q   Camera"
                    } else {
                        "E   Pick up / drop     Q   Camera"
                    },
                    left,
                    y + 450.,
                    17.,
                    MUTED,
                );
                text(
                    "Shift   Sprint     F   Fullscreen     Esc   Back",
                    left,
                    y + 474.,
                    17.,
                    MUTED,
                );
                if game.is_none() && controller.character_kind() == CharacterKind::Scientist {
                    text(
                        "Left click   Attack     Scroll   Weapon",
                        left,
                        y + 498.,
                        17.,
                        MUTED,
                    );
                }
                if button("Back", Rect::new(left, y + 531., width, 42.), false) {
                    settings_open = false;
                }
            }
        }
        set_default_camera();
        if debug {
            draw_rectangle(sw - 350., 20., 330., 90., Color::new(0., 0., 0., 0.8));
            text(
                &format!("{} FPS  |  {} triangles", get_fps(), triangles),
                sw - 336.,
                45.,
                18.,
                INK,
            );
            text(
                &format!(
                    "x {:.2}  y {:.2}  z {:.2}",
                    controller.position.0, controller.position.1, controller.position.2
                ),
                sw - 336.,
                70.,
                18.,
                INK,
            );
            text(
                &format!(
                    "Startup {:.2}s   /   {} batches",
                    setup_seconds,
                    meshes.len()
                ),
                sw - 336.,
                95.,
                18.,
                INK,
            );
        }
        if let Some(dir) = &capture_dir {
            let shot = if camera_capture {
                [1, 20, 30, 40, 60, 70].iter().position(|f| *f == frame)
            } else if game_capture {
                [1, 6, 14, 22, 29, 35].iter().position(|f| *f == frame)
            } else if pistol_capture {
                [10, 20, 24, 56, 64, 88, 100, 110]
                    .iter()
                    .position(|f| *f == frame)
            } else if physics_capture {
                [1, 28, 90, 110, 170, 350].iter().position(|f| *f == frame)
            } else if character_capture {
                [10, 22, 34, 46, 58, 70, 94, 106]
                    .iter()
                    .position(|f| *f == frame)
            } else if wrench_capture {
                [10, 24, 42, 70].iter().position(|f| *f == frame)
            } else if motion_capture {
                [15, 50, 80].iter().position(|f| *f == frame)
            } else if frame % 12 == 10 {
                Some(frame / 12)
            } else {
                None
            };
            if let Some(index) = shot {
                captured_heights.push(controller.position.1);
                get_screen_data().export_png(
                    &dir.join(format!("blue-engine-{}.png", index + 1))
                        .to_string_lossy(),
                );
            }
            if frame > 3 {
                samples.push(get_frame_time());
            }
            if frame
                == if pistol_capture {
                    111
                } else if physics_capture {
                    351
                } else if house_capture {
                    143
                } else if character_capture {
                    107
                } else if motion_capture {
                    85
                } else if interaction_capture || wrench_capture || camera_capture {
                    71
                } else {
                    35
                }
            {
                let mean = samples.iter().sum::<f32>() / samples.len() as f32;
                let report=format!("{}viewport={}x{}\nstartup_seconds={setup_seconds:.3}\ntriangles={triangles}\nvertices={}\nbatches={}\nmean_frame_ms={:.3}\ncaptured_eye_heights={captured_heights:?}\nwrench_hits={}\naudio_plays={}\n",platform_window::report(),screen_width(),screen_height(),meshes.iter().chain(game_meshes.iter().flatten()).map(|m|m.vertices.len()).sum::<usize>(),meshes.len()+game_meshes.iter().map(Vec::len).sum::<usize>(),mean*1000.,wrench.hits,impact_audio.plays);
                let props_report:Vec<_> = prop_physics.props.iter().map(|p| serde_json::json!({"id":p.id,"position":[p.transform.p.0,p.transform.p.1,p.transform.p.2]})).collect();
                let _ = std::fs::write(
                    dir.join("physics-report.json"),
                    serde_json::to_string_pretty(&props_report).unwrap(),
                );
                let report = format!(
                    "{report}pistol_shots={}\npistol_audio_plays={}\nselected_weapon={:?}\n",
                    loadout.pistol.shots, pistol_audio.plays, loadout.selected
                );
                let _ = std::fs::write(dir.join("render-report.txt"), report);
                if let Some(game) = &game {
                    let _ = std::fs::write(
                        dir.join("game-state.json"),
                        serde_json::to_vec_pretty(game.state()).unwrap(),
                    );
                }
                break;
            }
        }
        frame += 1;
        next_frame().await;
    }
    if let (Some(ref transport), Some(server_addr), Some(pid)) =
        (&net_transport, net_server_dest, net_player_id)
    {
        let _ = transport.send_packet(
            &Packet::Disconnect {
                player_id: pid,
                session_token: net_session_token,
            },
            server_addr,
        );
    }
    capture(false);
}
async fn error_screen(message: &str) {
    loop {
        clear_background(Color::new(0.035, 0.06, 0.10, 1.));
        text("Blue Engine", 40., 65., 36., INK);
        text(message, 40., 112., 20., INK);
        text("Press Escape to close.", 40., 155., 20., MUTED);
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        next_frame().await;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_between_frame_tap_keeps_press_edge() {
        use miniquad::EventHandler;
        let mut keys = Keys::default();
        keys.key_down_event(KeyCode::Space, miniquad::KeyMods::default(), false);
        keys.key_up_event(KeyCode::Space, miniquad::KeyMods::default());
        assert!(keys.pressed.contains(&KeyCode::Space));
        assert!(!keys.down.contains(&KeyCode::Space));
    }
    #[test]
    fn arrows_match_wasd_and_aliases_do_not_double_speed() {
        for (a, b) in [
            (KeyCode::W, KeyCode::Up),
            (KeyCode::S, KeyCode::Down),
            (KeyCode::A, KeyCode::Left),
            (KeyCode::D, KeyCode::Right),
        ] {
            assert_eq!(axes(&HashSet::from([a])), axes(&HashSet::from([b])));
            assert_eq!(axes(&HashSet::from([a, b])), axes(&HashSet::from([a])));
        }
        assert_eq!(
            axes(&HashSet::from([
                KeyCode::Up,
                KeyCode::S,
                KeyCode::Left,
                KeyCode::D
            ])),
            (0., 0.)
        );
    }
}
