//! Ready-to-play local static-map client. No hidden prop physics or network host.
//! For authoritative GameDocument rules/dynamic objects use the stock `be2 --game`
//! runtime. Custom games can reuse MapPlayer and own their loop/actions.
use super::{
    authoring::MapDocument,
    camera::{CameraRig, Perspective},
    character_skins::Avatar,
    controller::{Controller, Movement},
    game_client::{self, GameShell},
    game_input::ClientInput,
    game_visuals::SurfaceRenderer,
    gamepad::Button,
    room::Room,
    simulation::PlayerStepper,
};
use crate::Result;
use macroquad::prelude::*;

pub struct MapPlayer {
    pub player: Controller,
    pub perspective: Perspective,
    pub room: Room,
    stepper: PlayerStepper,
    camera: CameraRig,
    meshes: Vec<Mesh>,
    surface: SurfaceRenderer,
    avatar: Avatar,
}
impl MapPlayer {
    /// Build in a live graphics context. Fails without a valid standalone spawn.
    pub fn new(map: &MapDocument, character: &str) -> Result<Self> {
        let room = map.build_standalone()?;
        let spawn = map.default_spawn.ok_or("Map requires a spawn")?;
        let avatar = Avatar::new(character)?;
        let player = Controller::for_character_at(avatar.kind(), spawn.feet, spawn.yaw)?;
        let mut stepper = PlayerStepper::default();
        stepper.reset(&player);
        Ok(Self {
            meshes: game_client::static_meshes(&room.world),
            surface: SurfaceRenderer::new()?,
            room,
            player,
            avatar,
            stepper,
            camera: CameraRig::default(),
            perspective: Perspective::First,
        })
    }
    /// Fixed-rate collision simulation with interpolated presentation.
    pub fn update(&mut self, movement: Movement, look: [f32; 2], seconds: f32) {
        self.player.look(look[0], look[1], 1., false);
        let before = self.player.position;
        self.stepper
            .advance(&mut self.player, movement, seconds, &self.room.colliders);
        self.avatar.update(
            (self.player.position - before).length(),
            seconds,
            movement.forward != 0. || movement.right != 0.,
        );
    }
    pub fn pause(&mut self) {
        self.player.stop();
        self.stepper.reset(&self.player);
    }
    pub fn draw(&mut self, seconds: f32) {
        let pose = self.stepper.pose(&self.player);
        self.camera
            .advance(self.perspective, &pose, &self.room, seconds);
        let view = self.camera.view(self.perspective, &pose, &self.room);
        set_camera(&Camera3D {
            position: vec3(view.eye.0, view.eye.1, view.eye.2),
            target: vec3(view.target.0, view.target.1, view.target.2),
            up: Vec3::Y,
            fovy: 55_f32.to_radians(),
            z_near: 0.025,
            z_far: 250.,
            ..Default::default()
        });
        self.surface.draw(&self.meshes);
        if view.show_body {
            self.avatar.draw(&pose);
        }
        set_default_camera();
    }
}
/// Launch a local map from an application-owned Macroquad main. Optional
/// `--character ID`, `--third-person`, and `--capture DIR` aid reuse and verification.
pub async fn run_map(map: MapDocument) -> Result<()> {
    run_map_with_focus(map, || true).await
}
/// Host-aware entry point used by generated games. The callback is evaluated
/// every frame, including pause, to neutralize input and release cursor on focus loss.
pub async fn run_map_with_focus(map: MapDocument, focused: impl Fn() -> bool) -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let value = |flag| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
    };
    let character = value("--character")
        .map(String::as_str)
        .unwrap_or("scientist");
    let capture = value("--capture").map(std::path::PathBuf::from);
    if let Some(dir) = &capture {
        std::fs::create_dir(dir)?;
    }
    let mut view = MapPlayer::new(&map, character)?;
    if args.iter().any(|a| a == "--third-person") {
        view.perspective = Perspective::Third;
    }
    let mut shell = GameShell::new();
    let mut input = ClientInput::new();
    let mut frames = 0;
    loop {
        input.begin_frame(&mut shell, true, focused());
        if capture.is_some() {
            shell.paused = frames >= 30;
        }
        if shell.playing() {
            if is_key_pressed(KeyCode::Q) || input.gamepad().pressed(Button::RightThumb) {
                view.perspective.toggle();
            }
            view.update(
                input.movement(&shell),
                input.look_delta(&shell),
                get_frame_time(),
            );
        } else {
            view.pause();
        }
        clear_background(Color::new(0.48, 0.67, 0.8, 1.));
        view.draw(get_frame_time());
        if shell.local_menu(
            "BlueEngine",
            &[
                "Move: WASD / arrows / left stick",
                "Look: mouse / right stick",
                "Jump: Space / A; crouch: Ctrl / B",
                "Sprint: Shift / LS; camera: Q / RS",
                "Menu: Esc / Start; confirm: Enter / A",
                "Fullscreen: F / F11",
            ],
        ) {
            break;
        }
        if let Some(dir) = &capture {
            if frames == 20 || frames == 40 {
                get_screen_data().export_png(
                    dir.join(if frames == 20 {
                        "world.png"
                    } else {
                        "menu.png"
                    })
                    .to_str()
                    .ok_or("Invalid capture path")?,
                );
            }
            if frames == 40 {
                break;
            }
        }
        frames += 1;
        next_frame().await;
    }
    Ok(())
}
