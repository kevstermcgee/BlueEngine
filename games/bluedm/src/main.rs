use bluedm::{
    content_path,
    protocol::{ClientInput, MatchSnapshot, WireMessage, PROTOCOL_VERSION},
    server::Server,
    DEFAULT_KEY, DEFAULT_SERVER,
};
use macroquad::prelude::*;
use std::{
    net::SocketAddr,
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};
use vesper3d::{
    math::V,
    scene::{Shape, Track},
    viewer::{
        authoring::MapDocument,
        controller::Movement,
        fps::{armory, AimState, OperativeModel, Team, WeaponClass},
        net::{transport::DatagramTransport, UdpTransport},
    },
};

fn window_conf() -> Conf {
    Conf {
        window_title: "BlueDM".into(),
        window_width: 1280,
        window_height: 720,
        high_dpi: true,
        sample_count: 4,
        ..Default::default()
    }
}

struct Client {
    transport: UdpTransport,
    server: SocketAddr,
    key: String,
    token: Option<[u8; 16]>,
    player_id: Option<u64>,
    sequence: u64,
    fire_counter: u32,
    reload_counter: u32,
    weapon_slot: usize,
    yaw: f32,
    pitch: f32,
    aim: AimState,
    snapshot: Option<MatchSnapshot>,
    status: String,
    last_join: Instant,
}

impl Client {
    fn new(server: SocketAddr, key: String) -> vesper3d::Result<Self> {
        Ok(Self {
            transport: UdpTransport::bind("0.0.0.0:0")?,
            server,
            key,
            token: None,
            player_id: None,
            sequence: 0,
            fire_counter: 0,
            reload_counter: 0,
            weapon_slot: 4,
            yaw: 1.57,
            pitch: 0.0,
            aim: AimState::default(),
            snapshot: None,
            status: "Connecting to Cobalt Foundry...".into(),
            last_join: Instant::now() - Duration::from_secs(1),
        })
    }

    fn update_network(
        &mut self,
        movement: Movement,
        fire_pressed: bool,
        fire_held: bool,
        reload: bool,
        ads: bool,
    ) -> vesper3d::Result<()> {
        let datagrams = match self.transport.receive() {
            Ok(datagrams) => datagrams,
            Err(error) => {
                self.status = format!("Waiting for server: {error}");
                Vec::new()
            }
        };
        for datagram in datagrams {
            if datagram.peer != self.server {
                continue;
            }
            match WireMessage::decode(&datagram.data) {
                Some(WireMessage::Welcome { player_id, token }) => {
                    self.player_id = Some(player_id);
                    self.token = Some(token);
                    self.status = format!("Online as operative {player_id}");
                    set_cursor_grab(true);
                    show_mouse(false);
                }
                Some(WireMessage::Reject { reason }) => self.status = reason,
                Some(WireMessage::Snapshot { token, state }) if Some(token) == self.token => {
                    self.snapshot = Some(state);
                }
                _ => {}
            }
        }

        if self.token.is_none() && self.last_join.elapsed() >= Duration::from_millis(500) {
            self.last_join = Instant::now();
            let join = WireMessage::Join {
                version: PROTOCOL_VERSION,
                key: self.key.clone(),
                name: "Blue operative".into(),
            };
            self.transport.send(self.server, &join.encode()?)?;
        }
        if let Some(token) = self.token {
            self.sequence += 1;
            self.fire_counter = self.fire_counter.wrapping_add(u32::from(fire_pressed));
            self.reload_counter = self.reload_counter.wrapping_add(u32::from(reload));
            let input = ClientInput {
                sequence: self.sequence,
                movement,
                yaw: self.yaw,
                pitch: self.pitch,
                weapon_slot: self.weapon_slot as u8,
                fire_counter: self.fire_counter,
                fire_held,
                reload_counter: self.reload_counter,
                ads,
            };
            let message = WireMessage::Input { token, input };
            let _ = self.transport.send(self.server, &message.encode()?);
        }
        Ok(())
    }
}

#[macroquad::main(window_conf)]
async fn main() -> vesper3d::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let host = args.iter().any(|arg| arg == "--host");
    let address = value_after(&args, "--connect").unwrap_or_else(|| DEFAULT_SERVER.into());
    let key = value_after(&args, "--key").unwrap_or_else(|| DEFAULT_KEY.into());
    let capture_path = value_after(&args, "--capture");
    if host {
        let server_key = key.clone();
        std::thread::spawn(move || {
            let mut server =
                Server::bind(DEFAULT_SERVER, server_key).expect("start local BlueDM server");
            server
                .run(Arc::new(AtomicBool::new(false)), None)
                .expect("run local BlueDM server");
        });
        std::thread::sleep(Duration::from_millis(750));
    }
    let map = MapDocument::load(&content_path())?;
    let mut client = Client::new(address.parse()?, key)?;
    let catalog = armory();

    let mut frame = 0u32;
    loop {
        if is_key_pressed(KeyCode::Escape) {
            set_cursor_grab(false);
            show_mouse(true);
        }
        if is_mouse_button_pressed(MouseButton::Left) && client.token.is_some() {
            set_cursor_grab(true);
            show_mouse(false);
        }
        if is_key_down(KeyCode::LeftAlt) {
            set_cursor_grab(false);
            show_mouse(true);
        } else if client.token.is_some() {
            let delta = mouse_delta_position();
            client.yaw = (client.yaw - delta.x * 2.5).rem_euclid(std::f32::consts::TAU);
            client.pitch = (client.pitch - delta.y * 2.5).clamp(-1.48, 1.48);
        }
        select_weapon(&mut client.weapon_slot);
        let (_, wheel) = mouse_wheel();
        if wheel != 0.0 {
            client.weapon_slot = if wheel > 0.0 {
                (client.weapon_slot + catalog.weapons.len() - 1) % catalog.weapons.len()
            } else {
                (client.weapon_slot + 1) % catalog.weapons.len()
            };
        }
        let mut movement = Movement {
            forward: axis(KeyCode::W, KeyCode::S),
            right: axis(KeyCode::D, KeyCode::A),
            jump: is_key_pressed(KeyCode::Space),
            crouch: is_key_down(KeyCode::LeftControl),
            sprint: is_key_down(KeyCode::LeftShift),
        };
        if capture_path.is_some() && frame < 75 {
            movement.forward = 1.0;
            movement.sprint = true;
        }
        let ads = is_mouse_button_down(MouseButton::Right);
        client
            .aim
            .update(ads, get_frame_time(), &catalog.weapons[client.weapon_slot]);
        client.update_network(
            movement,
            is_mouse_button_pressed(MouseButton::Left),
            is_mouse_button_down(MouseButton::Left),
            is_key_pressed(KeyCode::R),
            ads,
        )?;

        draw_world(
            &map,
            &client,
            client.aim.fov(76.0, &catalog.weapons[client.weapon_slot]),
            capture_path.is_some(),
        );
        draw_hud(&client, &catalog.weapons[client.weapon_slot]);
        frame += 1;
        if frame == 90 {
            if let Some(path) = &capture_path {
                get_screen_data().export_png(path);
                return Ok(());
            }
        }
        next_frame().await;
    }
}

fn draw_world(map: &MapDocument, client: &Client, fov: f32, showcase: bool) {
    clear_background(Color::new(0.025, 0.045, 0.075, 1.0));
    let local = client.snapshot.as_ref().and_then(|snapshot| {
        let id = client.player_id?;
        snapshot.players.iter().find(|player| player.id == id)
    });
    let eye = if showcase {
        V(0.5, 1.68, -4.4)
    } else {
        local.map_or(V(-20.0, 1.68, 0.0), |player| player.position)
    };
    let direction = if showcase {
        vesper3d::viewer::fps::direction(1.15, -0.04)
    } else {
        vesper3d::viewer::fps::direction(client.yaw, client.pitch)
    };
    set_camera(&Camera3D {
        position: vec3(eye.0, eye.1, eye.2),
        target: vec3(
            eye.0 + direction.0,
            eye.1 + direction.1,
            eye.2 + direction.2,
        ),
        up: Vec3::Y,
        fovy: fov.to_radians(),
        ..Default::default()
    });
    for node in &map.scene.nodes {
        let (Track::Fixed(position), Track::Fixed(scale)) = (&node.pos, &node.scale) else {
            continue;
        };
        let material = map
            .scene
            .materials
            .get(&node.material)
            .cloned()
            .unwrap_or_default();
        let color = Color::new(material.color.0, material.color.1, material.color.2, 1.0);
        let center = vec3(position.0, position.1, position.2);
        match node.shape {
            Shape::Sphere => draw_sphere(center, scale.0.max(scale.1).max(scale.2), None, color),
            _ => draw_cube(
                center,
                vec3(scale.0 * 2.0, scale.1 * 2.0, scale.2 * 2.0),
                None,
                color,
            ),
        }
    }
    if let Some(snapshot) = &client.snapshot {
        for player in &snapshot.players {
            if Some(player.id) != client.player_id && player.health > 0 {
                draw_operative(player.position, player.team, player.model);
            }
        }
    }
    set_default_camera();
}

fn draw_operative(position: V, team: Team, model: OperativeModel) {
    let color = match team {
        Team::Azure => Color::new(0.12, 0.48, 0.95, 1.0),
        Team::Crimson => Color::new(0.92, 0.16, 0.23, 1.0),
    };
    let accent = match model {
        OperativeModel::Vanguard => GOLD,
        OperativeModel::Recon => LIME,
        OperativeModel::Breacher => ORANGE,
        OperativeModel::FieldTech => SKYBLUE,
    };
    let feet = position.1 - 1.68;
    let width = if model == OperativeModel::Breacher {
        0.62
    } else {
        0.50
    };
    draw_cube(
        vec3(position.0, feet + 1.02, position.2),
        vec3(width, 0.82, 0.36),
        None,
        color,
    );
    draw_sphere(
        vec3(position.0, feet + 1.61, position.2),
        0.22,
        None,
        accent,
    );
    for side in [-1.0, 1.0] {
        draw_cube(
            vec3(position.0 + side * 0.15, feet + 0.42, position.2),
            vec3(0.18, 0.78, 0.22),
            None,
            color,
        );
        if model == OperativeModel::Vanguard || model == OperativeModel::Breacher {
            draw_cube(
                vec3(position.0 + side * 0.31, feet + 1.14, position.2),
                vec3(0.18, 0.22, 0.42),
                None,
                accent,
            );
        }
    }
    if model == OperativeModel::Recon {
        draw_cube(
            vec3(position.0, feet + 1.70, position.2),
            vec3(0.54, 0.06, 0.36),
            None,
            accent,
        );
    }
    if model == OperativeModel::FieldTech {
        draw_cube(
            vec3(position.0, feet + 1.03, position.2 + 0.24),
            vec3(0.36, 0.52, 0.18),
            None,
            accent,
        );
    }
}

fn draw_hud(client: &Client, weapon: &vesper3d::viewer::fps::WeaponDefinition) {
    let width = screen_width();
    let height = screen_height();
    let aim = client.aim.amount();
    let gap = 11.0 * (1.0 - aim) + 2.0;
    let cx = width * 0.5;
    let cy = height * 0.5;
    draw_line(cx - gap - 9.0, cy, cx - gap, cy, 2.0, WHITE);
    draw_line(cx + gap, cy, cx + gap + 9.0, cy, 2.0, WHITE);
    draw_line(cx, cy - gap - 9.0, cx, cy - gap, 2.0, WHITE);
    draw_line(cx, cy + gap, cx, cy + gap + 9.0, 2.0, WHITE);

    let local = client.snapshot.as_ref().and_then(|snapshot| {
        let id = client.player_id?;
        snapshot.players.iter().find(|player| player.id == id)
    });
    let health = local.map_or(0, |player| player.health);
    let (magazine, reserve, reloading) = local.map_or(
        (weapon.magazine_size, weapon.reserve_ammo, false),
        |player| (player.magazine, player.reserve, player.reloading),
    );
    draw_rectangle(
        18.0,
        height - 86.0,
        260.0,
        62.0,
        Color::from_rgba(7, 14, 24, 220),
    );
    draw_text(
        &format!("HEALTH {health:03}"),
        32.0,
        height - 47.0,
        30.0,
        if health > 30 { WHITE } else { RED },
    );
    draw_rectangle(
        width - 318.0,
        height - 100.0,
        300.0,
        76.0,
        Color::from_rgba(7, 14, 24, 220),
    );
    draw_text(&weapon.name, width - 302.0, height - 66.0, 22.0, WHITE);
    draw_text(
        &if reloading {
            "RELOADING".into()
        } else {
            format!("{magazine:02} / {reserve:03}")
        },
        width - 302.0,
        height - 37.0,
        24.0,
        GOLD,
    );

    if let Some(snapshot) = &client.snapshot {
        draw_rectangle(
            width * 0.5 - 150.0,
            12.0,
            300.0,
            52.0,
            Color::from_rgba(7, 14, 24, 220),
        );
        draw_text(
            &format!(
                "AZURE {:02}   /   {:02} CRIMSON",
                snapshot.team_scores[0], snapshot.team_scores[1]
            ),
            width * 0.5 - 125.0,
            46.0,
            25.0,
            WHITE,
        );
        if let Some(winner) = snapshot.winner {
            draw_rectangle(
                0.0,
                height * 0.38,
                width,
                100.0,
                Color::from_rgba(5, 8, 14, 220),
            );
            draw_text(
                &format!("{:?} TEAM WINS", winner).to_uppercase(),
                width * 0.5 - 170.0,
                height * 0.38 + 62.0,
                42.0,
                GOLD,
            );
        }
    }
    draw_weapon_viewmodel(weapon.class, aim);
    draw_text(&client.status, 18.0, 28.0, 20.0, LIGHTGRAY);
    draw_text(
        "WASD move  SHIFT sprint  RMB aim  R reload  1-0 / wheel weapons  ALT release mouse",
        18.0,
        height - 112.0,
        17.0,
        LIGHTGRAY,
    );
}

fn draw_weapon_viewmodel(class: WeaponClass, aim: f32) {
    let width = screen_width();
    let height = screen_height();
    let center_x = width * 0.5;
    let base_x = width * 0.72 + (center_x - width * 0.72) * aim;
    let base_y = height * 0.83 + 34.0 * aim;
    let (length, body_h, accent) = match class {
        WeaponClass::Pistol => (155.0, 42.0, SKYBLUE),
        WeaponClass::SubmachineGun => (230.0, 52.0, LIME),
        WeaponClass::AssaultRifle => (310.0, 58.0, BLUE),
        WeaponClass::Shotgun => (340.0, 48.0, ORANGE),
        WeaponClass::MarksmanRifle => (360.0, 50.0, GOLD),
        WeaponClass::SniperRifle => (390.0, 46.0, PURPLE),
        WeaponClass::MachineGun => (355.0, 72.0, RED),
    };
    let dark = Color::new(0.055, 0.07, 0.09, 1.0);
    draw_rectangle(base_x - length * 0.5, base_y - body_h, length, body_h, dark);
    draw_rectangle(
        base_x - length * 0.28,
        base_y - body_h - 11.0,
        length * 0.50,
        12.0,
        accent,
    );
    draw_rectangle(
        base_x + length * 0.18,
        base_y - body_h - 22.0,
        9.0,
        22.0,
        dark,
    );
    draw_rectangle(base_x - length * 0.20, base_y, 38.0, 68.0, dark);
    if matches!(class, WeaponClass::MarksmanRifle | WeaponClass::SniperRifle) {
        draw_circle(base_x - 15.0, base_y - body_h - 13.0, 15.0, accent);
        draw_circle(base_x - 15.0, base_y - body_h - 13.0, 8.0, dark);
    }
    if aim > 0.55 {
        draw_rectangle(
            center_x - 19.0,
            screen_height() * 0.5 + 25.0,
            5.0,
            28.0,
            dark,
        );
        draw_rectangle(
            center_x + 14.0,
            screen_height() * 0.5 + 25.0,
            5.0,
            28.0,
            dark,
        );
    }
}

fn axis(positive: KeyCode, negative: KeyCode) -> f32 {
    is_key_down(positive) as u8 as f32 - is_key_down(negative) as u8 as f32
}

fn select_weapon(slot: &mut usize) {
    for (index, key) in [
        KeyCode::Key1,
        KeyCode::Key2,
        KeyCode::Key3,
        KeyCode::Key4,
        KeyCode::Key5,
        KeyCode::Key6,
        KeyCode::Key7,
        KeyCode::Key8,
        KeyCode::Key9,
        KeyCode::Key0,
    ]
    .iter()
    .enumerate()
    {
        if is_key_pressed(*key) {
            *slot = index;
        }
    }
}

fn value_after(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}
