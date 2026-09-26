//! BlueEngine's local, reproducible content workbench. No network service required.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#[allow(dead_code)]
mod character;
#[path = "sandbox/content.rs"]
mod content;
#[path = "sandbox/creative.rs"]
mod creative;
#[path = "sandbox/characters.rs"]
mod fun_characters;
#[path = "sandbox/input.rs"]
mod input;
#[path = "sandbox/ui.rs"]
mod ui;
#[allow(dead_code)]
mod wrench_view;

use macroquad::prelude as mq;
use std::path::{Path, PathBuf};
use vesper3d::prelude::*;
use vesper3d::viewer::{
    camera::{CameraRig, Perspective},
    controller::CharacterKind,
    game_client::{self, GameShell},
    game_text,
    game_visuals::SurfaceRenderer,
    room::Room,
    simulation::PlayerStepper,
    wrench::Wrench,
};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Maps,
    Assets,
    Characters,
}

struct Stage {
    doc: MapDocument,
    room: Room,
    meshes: Vec<mq::Mesh>,
    surface: SurfaceRenderer,
    player: Controller,
    stepper: PlayerStepper,
    camera: CameraRig,
}
impl Stage {
    fn new(doc: MapDocument, kind: CharacterKind, signs: &[content::Sign]) -> Result<Self> {
        let room = doc.build()?;
        let meshes = game_client::static_meshes(&room.world);
        let spawn = doc.default_spawn.ok_or("Missing spawn")?;
        let player = Controller::for_character_at(kind, spawn.feet, spawn.yaw)?;
        let mut stepper = PlayerStepper::default();
        stepper.reset(&player);
        let mut surface = SurfaceRenderer::new()?;
        for s in signs {
            surface.add_sign(
                &s.text,
                mq::Vec3::from_array(s.position),
                mq::Vec3::X,
                s.height,
            );
        }
        Ok(Self {
            doc,
            room,
            meshes,
            surface,
            player,
            stepper,
            camera: CameraRig::default(),
        })
    }
    fn reset(&mut self, kind: CharacterKind) -> Result<()> {
        let spawn = self.doc.default_spawn.ok_or("Missing spawn")?;
        self.player = Controller::for_character_at(kind, spawn.feet, spawn.yaw)?;
        self.stepper.reset(&self.player);
        self.camera = CameraRig::default();
        Ok(())
    }
}
struct Pending {
    asset: usize,
    doc: MapDocument,
    meshes: Vec<mq::Mesh>,
    ghost: Vec<mq::Mesh>,
    at: V,
    turns: u8,
    distance: f32,
    elevation: f32,
    snap: bool,
}
fn posed_meshes(meshes: &[mq::Mesh], at: V, turns: u8) -> Vec<mq::Mesh> {
    meshes
        .iter()
        .map(|m| mq::Mesh {
            vertices: m
                .vertices
                .iter()
                .map(|v| {
                    let mut v = *v;
                    let p =
                        creative::rotate(V(v.position.x, v.position.y, v.position.z), turns) + at;
                    v.position = mq::vec3(p.0, p.1, p.2);
                    let n = creative::rotate(V(v.normal.x, v.normal.y, v.normal.z), turns);
                    v.normal = mq::vec4(n.0, n.1, n.2, v.normal.w);
                    v
                })
                .collect(),
            indices: m.indices.clone(),
            texture: m.texture.clone(),
        })
        .collect()
}
struct App {
    root: PathBuf,
    save_directory: PathBuf,
    catalog: content::Catalog,
    tab: Tab,
    selected: usize,
    browser: bool,
    query: String,
    search_focus: bool,
    scroll: usize,
    stage: Stage,
    shell: GameShell,
    kind: CharacterKind,
    perspective: Perspective,
    skin_index: usize,
    skin: fun_characters::Skin,
    actor: character::Character,
    tool: wrench_view::View,
    wrench: Wrench,
    orbit: f32,
    zoom: f32,
    spin: bool,
    bounds: bool,
    notice: String,
    logo: mq::Texture2D,
    ticks: u64,
    frame_times: Vec<f32>,
    physics_child: Option<std::process::Child>,
    setup: bool,
    play_map: usize,
    play_character: usize,
    world_map: Option<usize>,
    asset_menu: bool,
    spawn_query: String,
    spawn_scroll: usize,
    pending: Option<Pending>,
    undo: Vec<MapDocument>,
    input_delay: u8,
}
impl App {
    fn new(root: PathBuf) -> Result<Self> {
        let catalog = content::Catalog::load(&root)?;
        let stage = Stage::new(
            content::load_map(&root, &catalog.maps[0].path)?,
            CharacterKind::Scientist,
            &catalog.maps[0].signs,
        )?;
        let logo = mq::Texture2D::from_file_with_format(
            include_bytes!("../../assets/branding/blueengine.png"),
            Some(mq::ImageFormat::Png),
        );
        let save_directory = root.join(".be2-work/sandbox-worlds");
        Ok(Self {
            root,
            save_directory,
            catalog,
            tab: Tab::Maps,
            selected: 0,
            browser: true,
            query: String::new(),
            search_focus: false,
            scroll: 0,
            stage,
            shell: GameShell::new(),
            kind: CharacterKind::Scientist,
            perspective: Perspective::First,
            skin_index: 0,
            skin: fun_characters::Skin::new(2),
            actor: character::Character::default(),
            tool: wrench_view::View::new(),
            wrench: Wrench::default(),
            orbit: 0.55,
            zoom: 1.,
            spin: false,
            bounds: false,
            notice: String::new(),
            logo,
            ticks: 0,
            frame_times: Vec::new(),
            physics_child: None,
            setup: false,
            play_map: 0,
            play_character: 0,
            world_map: None,
            asset_menu: false,
            spawn_query: String::new(),
            spawn_scroll: 0,
            pending: None,
            undo: Vec::new(),
            input_delay: 0,
        })
    }
    fn select(&mut self, tab: Tab, index: usize) -> Result<()> {
        self.world_map = None;
        self.pending = None;
        self.undo.clear();
        if tab == Tab::Maps {
            self.play_map = index;
        }
        if tab == Tab::Characters {
            self.play_character = index;
        }
        let (doc, signs) = match tab {
            Tab::Maps => (
                content::load_map(&self.root, &self.catalog.maps[index].path)?,
                self.catalog.maps[index].signs.as_slice(),
            ),
            Tab::Assets => (
                content::load_map(&self.root, &self.catalog.assets[index].map)?,
                &[][..],
            ),
            Tab::Characters => (
                SceneBuilder::new("Character Studio")
                    .spawn(V(0., 0., 5.), 0.)
                    .structural_box(
                        "floor",
                        V(0., -0.1, 0.),
                        V(8., 0.1, 8.),
                        V(0.32, 0.40, 0.47),
                    )
                    .build()?,
                &[][..],
            ),
        };
        let kind = if tab == Tab::Characters {
            if index == 1 {
                CharacterKind::Feta
            } else {
                CharacterKind::Scientist
            }
        } else {
            self.kind
        };
        let stage = Stage::new(doc, kind, signs)?;
        if tab == Tab::Characters {
            self.skin_index = index;
            self.skin = fun_characters::Skin::new(index);
        }
        self.stage = stage;
        self.tab = tab;
        self.selected = index;
        self.kind = kind;
        self.orbit = 0.55;
        self.zoom = 1.;
        self.notice.clear();
        self.search_focus = false;
        Ok(())
    }
    fn title(&self) -> &str {
        match self.tab {
            Tab::Maps => &self.catalog.maps[self.selected].name,
            Tab::Assets => &self.catalog.assets[self.selected].name,
            Tab::Characters => fun_characters::NAMES[self.skin_index],
        }
    }
    fn map_path(&self) -> Option<&str> {
        match self.tab {
            Tab::Maps => Some(&self.catalog.maps[self.selected].path),
            Tab::Assets => Some(&self.catalog.assets[self.selected].map),
            Tab::Characters => None,
        }
    }
    fn physics(&mut self) -> Result<()> {
        if let Some(child) = &mut self.physics_child {
            if child.try_wait()?.is_none() {
                return Err(
                    "A physics session is already open. Close it before launching another.".into(),
                );
            }
        }
        let exe = std::env::current_exe()?;
        let filename = if cfg!(windows) { "be2.exe" } else { "be2" };
        let sibling = exe.parent().unwrap().join(filename);
        if !sibling.is_file() {
            return Err("Build be2 beside the sandbox to use physics sessions.".into());
        }
        let mut cmd = std::process::Command::new(sibling);
        cmd.current_dir(&self.root);
        if let Some(path) = self.map_path() {
            if !path.starts_with("builtin:") {
                cmd.arg("--map").arg(self.root.join(path));
            }
        }
        if self.kind == CharacterKind::Feta {
            cmd.arg("--feta");
        }
        self.physics_child = Some(cmd.spawn()?);
        self.notice="Physics session opened. Choose a character there; E carries/drops eligible native props.".into();
        Ok(())
    }
    fn export(&mut self) -> Result<()> {
        let directory = self.root.join(".be2-work/sandbox-exports");
        std::fs::create_dir_all(&directory)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let path = directory.join(format!("session-{now}.json"));
        let mut doc = self.stage.doc.clone();
        // A saved session opens at the player's current standing location.
        if !self.browser && self.stage.player.is_grounded() && !self.stage.player.is_crouched() {
            doc.default_spawn = Some(vesper3d::viewer::authoring::MapSpawn {
                feet: V(
                    self.stage.player.position.0,
                    self.stage.player.feet_height(),
                    self.stage.player.position.2,
                ),
                yaw: self.stage.player.yaw,
            });
        }
        doc.validate()?;
        use std::io::Write;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?
            .write_all(serde_json::to_string_pretty(&doc)?.as_bytes())?;
        self.notice = format!("Saved .be2-work/sandbox-exports/session-{now}.json");
        Ok(())
    }
    fn draw_world(&mut self) {
        mq::clear_background(mq::Color::new(0.55, 0.69, 0.78, 1.));
        let w = mq::screen_width();
        let h = mq::screen_height();
        let (eye, target, body) = if self.browser {
            let (center, radius) = match self.tab {
                Tab::Assets => {
                    let a = &self.catalog.assets[self.selected];
                    (
                        mq::vec3(0., a.half[1], 0.),
                        a.half.iter().copied().fold(0.12, f32::max) * 4.5,
                    )
                }
                Tab::Characters => (
                    mq::vec3(0., self.kind.standing_height() * 0.60, 0.),
                    self.kind.standing_height() * 1.7,
                ),
                Tab::Maps => {
                    let p = self.stage.doc.default_spawn.unwrap().feet;
                    (mq::vec3(p.0, 0., p.2 - 9.), 19.)
                }
            };
            let radius = radius * self.zoom;
            (
                center
                    + mq::vec3(
                        self.orbit.sin() * radius,
                        radius * 0.5,
                        self.orbit.cos() * radius,
                    ),
                center,
                false,
            )
        } else {
            let pose = self.stage.stepper.pose(&self.stage.player);
            self.stage.camera.advance(
                self.perspective,
                &pose,
                &self.stage.room,
                mq::get_frame_time(),
            );
            let v = self
                .stage
                .camera
                .view(self.perspective, &pose, &self.stage.room);
            (
                mq::vec3(v.eye.0, v.eye.1, v.eye.2),
                mq::vec3(v.target.0, v.target.1, v.target.2),
                v.show_body,
            )
        };
        let mut camera = mq::Camera3D {
            position: eye,
            target,
            up: mq::Vec3::Y,
            fovy: 55_f32.to_radians(),
            z_near: 0.025,
            z_far: 250.,
            ..Default::default()
        };
        if self.browser {
            let left = sidebar_width();
            let bottom = if h < 640. { 252. } else { 287. };
            camera.aspect = Some((w - left) / (h - bottom - 116.).max(90.));
            camera.viewport = Some((
                left as i32,
                bottom as i32,
                (w - left) as i32,
                (h - bottom - 116.).max(90.) as i32,
            ));
        }
        mq::set_camera(&camera);
        self.stage.surface.draw(&self.stage.meshes);
        if !self.browser && !self.asset_menu {
            if let Some(p) = &mut self.pending {
                let blocked = p.doc.colliders.values().any(|b| {
                    let b = creative::bounds(b, p.at, p.turns);
                    let player = &self.stage.player;
                    let body = b.max.1 > player.feet_height() + 0.001
                        && b.min.1 < player.feet_height() + player.body_height() - 0.001
                        && b.overlaps_xz(player.position, self.kind.radius());
                    let spawn = self.stage.doc.default_spawn.is_some_and(|spawn| {
                        b.max.1 > spawn.feet.1 + 0.001
                            && b.min.1 < spawn.feet.1 + 1.80 - 0.001
                            && b.overlaps_xz(spawn.feet, 0.23)
                    });
                    body || spawn
                });
                for (source, ghost) in p.meshes.iter().zip(&mut p.ghost) {
                    for (original, vertex) in source.vertices.iter().zip(&mut ghost.vertices) {
                        let point = creative::rotate(
                            V(
                                original.position.x,
                                original.position.y,
                                original.position.z,
                            ),
                            p.turns,
                        ) + p.at;
                        vertex.position = mq::vec3(point.0, point.1, point.2);
                        vertex.color = if blocked {
                            [240, 85, 70, 160]
                        } else {
                            [90, 225, 195, 145]
                        };
                    }
                    mq::draw_mesh(ghost);
                }
                let b = creative::bounds(&p.doc.entities[0].bounds, p.at, p.turns);
                let c = (b.min + b.max) * 0.5;
                let size = b.max - b.min;
                mq::draw_cube_wires(
                    mq::vec3(c.0, c.1, c.2),
                    mq::vec3(size.0, size.1, size.2),
                    ui::ACCENT,
                );
            }
        }
        if self.tab == Tab::Characters && self.browser {
            let mut player =
                Controller::for_character_at(self.kind, V(0., 0., 0.), std::f32::consts::PI)
                    .unwrap();
            player.pitch = 0.;
            self.actor
                .update(mq::get_frame_time() * 0.45, mq::get_frame_time(), self.spin);
            if self.skin_index >= 2 {
                self.skin
                    .update(mq::get_frame_time() * 0.45, mq::get_frame_time(), self.spin);
                self.skin.draw(&player);
            } else {
                self.actor.draw(&player, &self.wrench, &self.tool, false);
            }
        } else if body {
            if self.skin_index >= 2 {
                self.skin.draw(&self.stage.stepper.pose(&self.stage.player));
            } else {
                self.actor.draw(
                    &self.stage.stepper.pose(&self.stage.player),
                    &self.wrench,
                    &self.tool,
                    false,
                );
            }
        }
        if self.bounds {
            for c in &self.stage.room.colliders {
                let p = (c.min + c.max) * 0.5;
                let s = c.max - c.min;
                mq::draw_cube_wires(mq::vec3(p.0, p.1, p.2), mq::vec3(s.0, s.1, s.2), ui::ACCENT);
            }
        }
        mq::set_default_camera();
    }
    fn browser_ui(&mut self) -> Result<()> {
        let w = mq::screen_width();
        let h = mq::screen_height();
        let left = sidebar_width();
        let enabled = !self.shell.paused && !self.setup;
        ui::panel(0., 0., left, h, ui::PAPER);
        mq::draw_texture_ex(
            &self.logo,
            22.,
            22.,
            mq::WHITE,
            mq::DrawTextureParams {
                dest_size: Some(mq::vec2(42., 42.)),
                ..Default::default()
            },
        );
        ui::text("BLUEENGINE", 76., 38., 16., ui::BLUE);
        ui::text("Sandbox", 76., 65., 30., ui::INK);
        ui::text("EXPLORE / INSPECT / EXPERIMENT", 22., 98., 12., ui::MUTED);
        let bw = (left - 44.) / 3.;
        for (i, (tab, label)) in [
            (Tab::Maps, "Maps"),
            (Tab::Assets, "Assets"),
            (Tab::Characters, "Characters"),
        ]
        .into_iter()
        .enumerate()
        {
            if ui::button(
                label,
                mq::Rect::new(22. + i as f32 * bw, 116., bw - 3., 35.),
                self.tab == tab,
                enabled,
            ) {
                self.select(tab, 0)?;
                self.scroll = 0;
                self.query.clear();
            }
        }
        let mut items: Vec<(usize, String, String)> = match self.tab {
            Tab::Maps => self
                .catalog
                .maps
                .iter()
                .enumerate()
                .map(|(i, m)| (i, m.name.clone(), m.group.clone()))
                .collect(),
            Tab::Assets => self
                .catalog
                .matching_assets(&self.query)
                .into_iter()
                .map(|i| {
                    (
                        i,
                        self.catalog.assets[i].name.clone(),
                        self.catalog.assets[i].group.clone(),
                    )
                })
                .collect(),
            Tab::Characters => fun_characters::NAMES
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    (
                        i,
                        (*n).into(),
                        if i == 1 {
                            "0.30 m / rat profile".into()
                        } else {
                            "1.80 m / human profile".into()
                        },
                    )
                })
                .collect(),
        };
        if self.tab == Tab::Assets {
            let rect = mq::Rect::new(22., 165., left - 44., 36.);
            if enabled && mq::is_mouse_button_pressed(mq::MouseButton::Left) {
                self.search_focus = rect.contains(mq::Vec2::from(mq::mouse_position()));
            }
            ui::panel(
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                mq::Color::new(1., 1., 1., 1.),
            );
            if self.search_focus {
                mq::draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 2., ui::BLUE);
            }
            ui::text(
                &ui::fit(
                    if self.query.is_empty() {
                        "Search name, category or tag..."
                    } else {
                        &self.query
                    },
                    rect.w - 22.,
                    16.,
                ),
                32.,
                189.,
                16.,
                ui::MUTED,
            );
            if self.search_focus && enabled {
                while let Some(c) = mq::get_char_pressed() {
                    if c.is_ascii() && !c.is_control() && self.query.len() < 80 {
                        self.query.push(c);
                        self.scroll = 0;
                    }
                }
                if input::pressed(mq::KeyCode::Backspace) {
                    self.query.pop();
                    self.scroll = 0;
                }
            }
        } else {
            ui::text(
                &format!(
                    "{} {}",
                    items.len(),
                    if self.tab == Tab::Characters {
                        "characters"
                    } else {
                        "destinations"
                    }
                ),
                22.,
                188.,
                15.,
                ui::MUTED,
            );
        }
        let top = 215.;
        let bottom = (h - 88.).max(top + 52.);
        let rows = ((bottom - top) / 56.).floor().max(1.) as usize;
        let max_scroll = items.len().saturating_sub(rows);
        self.scroll = self.scroll.min(max_scroll);
        if enabled && mq::mouse_position().0 < left {
            let scroll = mq::mouse_wheel().1;
            if scroll < 0. {
                self.scroll = (self.scroll + 1).min(max_scroll);
            } else if scroll > 0. {
                self.scroll = self.scroll.saturating_sub(1);
            }
        }
        if enabled && !self.search_focus {
            let current = items.iter().position(|i| i.0 == self.selected).unwrap_or(0);
            let next = if input::pressed(mq::KeyCode::Down) {
                Some((current + 1).min(items.len().saturating_sub(1)))
            } else if input::pressed(mq::KeyCode::Up) {
                Some(current.saturating_sub(1))
            } else {
                None
            };
            if let Some(n) = next {
                if let Some(item) = items.get(n) {
                    self.select(self.tab, item.0)?;
                    self.scroll = n.saturating_sub(rows - 1);
                }
            }
        }
        if items.is_empty() {
            ui::paragraph(
                "No matching assets. Try furniture, plant, food or sandbox.",
                22.,
                245.,
                left - 44.,
                17.,
            );
        }
        for (row, (index, name, group)) in items.drain(..).skip(self.scroll).take(rows).enumerate()
        {
            let rect = mq::Rect::new(22., top + row as f32 * 56., left - 44., 50.);
            let selected = index == self.selected;
            if ui::button("", rect, selected, enabled) {
                self.select(self.tab, index)?;
            }
            ui::text(
                &ui::fit(&name, rect.w - 24., 18.),
                rect.x + 12.,
                rect.y + 22.,
                18.,
                if selected { mq::WHITE } else { ui::INK },
            );
            ui::text(
                &group,
                rect.x + 12.,
                rect.y + 41.,
                12.,
                if selected { ui::ACCENT } else { ui::MUTED },
            );
        }
        ui::text(
            "Scroll list / arrows to select",
            22.,
            h - 48.,
            13.,
            ui::MUTED,
        );
        ui::text(
            "Tab: walk preview / browser   Esc: menu",
            22.,
            h - 27.,
            13.,
            ui::MUTED,
        );
        let x = left + 24.;
        let rw = w - x - 24.;
        ui::panel(x, 22., rw, 86., mq::Color::new(0.06, 0.13, 0.20, 0.94));
        ui::text("CONTENT WORKBENCH", x + 18., 45., 12., ui::ACCENT);
        ui::text(
            &ui::fit(self.title(), rw - 36., 32.),
            x + 18.,
            83.,
            32.,
            mq::WHITE,
        );
        let card_h = if h < 640. { 200. } else { 235. };
        let y = h - card_h - 22.;
        ui::panel(x, y, rw, card_h, mq::Color::new(0.95, 0.96, 0.93, 0.97));
        let description = match self.tab {
            Tab::Maps => self.catalog.maps[self.selected].description.clone(),
            Tab::Assets => self.catalog.assets[self.selected].description.clone(),
            Tab::Characters => fun_characters::DESCRIPTIONS[self.skin_index].into(),
        };
        ui::paragraph(
            &description,
            x + 18.,
            y + 29.,
            rw - 36.,
            if rw < 500. { 13. } else { 16. },
        );
        let detail = match self.tab {
            Tab::Assets => {
                let a = &self.catalog.assets[self.selected];
                format!(
                    "{:.2} x {:.2} x {:.2} m  /  {}",
                    a.half[0] * 2.,
                    a.half[1] * 2.,
                    a.half[2] * 2.,
                    a.group
                )
            }
            _ => format!(
                "{} render parts / {} collision bounds",
                self.stage.doc.scene.nodes.len(),
                self.stage.room.colliders.len()
            ),
        };
        ui::text(
            &ui::fit(&detail, rw - 36., 14.),
            x + 18.,
            y + 97.,
            14.,
            ui::BLUE,
        );
        if self.tab == Tab::Assets {
            ui::text(
                &ui::fit(&self.catalog.assets[self.selected].source, rw - 36., 11.),
                x + 18.,
                y + 116.,
                11.,
                ui::MUTED,
            );
        }
        let b = (rw - 48.) / 3.;
        let by = y + card_h - 93.;
        if ui::button(
            "Play / Create",
            mq::Rect::new(x + 12., by, b, 35.),
            true,
            enabled,
        ) || (enabled && !self.search_focus && input::pressed(mq::KeyCode::Enter))
        {
            self.setup = true;
            self.search_focus = false;
        }
        if ui::button(
            "Physics session",
            mq::Rect::new(x + 24. + b, by, b, 35.),
            false,
            enabled && self.tab != Tab::Characters,
        ) {
            if let Err(e) = self.physics() {
                self.notice = e.to_string();
            }
        }
        if ui::button(
            "Export map",
            mq::Rect::new(x + 36. + b * 2., by, b, 35.),
            false,
            enabled,
        ) {
            if let Err(e) = self.export() {
                self.notice = e.to_string();
            }
        }
        let controls_y = by + 43.;
        if ui::button(
            if self.spin {
                "Stop orbit"
            } else {
                "Auto orbit"
            },
            mq::Rect::new(x + 12., controls_y, b, 32.),
            self.spin,
            enabled,
        ) {
            self.spin = !self.spin;
        }
        if ui::button(
            "Collision bounds",
            mq::Rect::new(x + 24. + b, controls_y, b, 32.),
            self.bounds,
            enabled,
        ) {
            self.bounds = !self.bounds;
        }
        if ui::button(
            "Reset view",
            mq::Rect::new(x + 36. + b * 2., controls_y, b, 32.),
            false,
            enabled,
        ) {
            self.orbit = 0.55;
            self.zoom = 1.;
        }
        if !self.notice.is_empty() {
            ui::panel(x, 115., rw, 70., ui::PAPER);
            ui::paragraph(&self.notice, x + 12., 139., rw - 24., 14.);
        }
        ui::text(
            "Drag to orbit / wheel to zoom",
            x + 12.,
            y - 14.,
            14.,
            ui::INK,
        );
        if enabled
            && mq::mouse_position().0 > left
            && mq::mouse_position().1 > 185.
            && mq::mouse_position().1 < y - 25.
        {
            if mq::is_mouse_button_down(mq::MouseButton::Left) {
                self.orbit -= mq::mouse_delta_position().x * 3.;
            }
            self.zoom = (self.zoom - mq::mouse_wheel().1 * 0.08).clamp(0.25, 3.);
        }
        if self.spin && enabled {
            self.orbit += mq::get_frame_time() * 0.3;
        }
        Ok(())
    }
    fn aim_ray(&self) -> vesper3d::math::Ray {
        self.stage
            .camera
            .view(self.perspective, &self.stage.player, &self.stage.room)
            .aim(&self.stage.player, &self.stage.room)
    }
    fn world_save(&self, map: usize) -> Result<PathBuf> {
        let id = &self.catalog.maps[map].id;
        if id.is_empty()
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err("Invalid map save identifier".into());
        }
        Ok(self.save_directory.join(format!("{id}.json")))
    }
    fn save_path(&self) -> Result<PathBuf> {
        self.world_save(
            self.world_map
                .ok_or("Choose a map using Play / Create first")?,
        )
    }
    fn start_play(&mut self) -> Result<()> {
        let map = &self.catalog.maps[self.play_map];
        let save = self.world_save(self.play_map)?;
        let doc = if save.is_file() {
            MapDocument::load(&save)?
        } else {
            content::load_map(&self.root, &map.path)?
        };
        let kind = if self.play_character == 1 {
            CharacterKind::Feta
        } else {
            CharacterKind::Scientist
        };
        let stage = Stage::new(doc, kind, &map.signs)?;
        self.stage = stage;
        self.skin_index = self.play_character;
        self.skin = fun_characters::Skin::new(self.skin_index);
        self.kind = kind;
        self.tab = Tab::Maps;
        self.selected = self.play_map;
        self.world_map = Some(self.play_map);
        self.browser = false;
        self.setup = false;
        self.asset_menu = false;
        self.search_focus = false;
        self.shell.paused = false;
        self.pending = None;
        self.undo.clear();
        self.input_delay = 2;
        self.perspective = Perspective::Third;
        self.notice = if save.is_file() {
            "Saved world loaded. V opens the asset palette."
        } else {
            "Creative mode. V opens the asset palette; edits save automatically."
        }
        .into();
        Ok(())
    }
    fn choose_asset(&mut self, index: usize) -> Result<()> {
        if self.world_map.is_none() {
            return Err("Choose a map using Play / Create first".into());
        }
        let doc = creative::specimen(content::load_map(
            &self.root,
            &self.catalog.assets[index].map,
        )?)?;
        let room = doc.build()?;
        let meshes = game_client::static_meshes(&room.world);
        let ghost = posed_meshes(&meshes, V::ZERO, 0);
        self.pending = Some(Pending {
            asset: index,
            doc,
            meshes,
            ghost,
            at: V::ZERO,
            turns: 0,
            distance: 6.,
            elevation: 0.,
            snap: true,
        });
        self.asset_menu = false;
        self.search_focus = false;
        self.input_delay = 2;
        self.position_pending();
        self.notice = "Preview ready. Left click places a copy; right click cancels.".into();
        Ok(())
    }
    fn position_pending(&mut self) {
        let ray = self.aim_ray();
        let Some(p) = &mut self.pending else {
            return;
        };
        let b = creative::bounds(&p.doc.entities[0].bounds, V::ZERO, p.turns);
        let center = (b.min + b.max) * 0.5;
        let half = (b.max - b.min) * 0.5;
        p.at = if let Some(hit) = self.stage.room.hit(ray, p.distance) {
            let support = hit.n.0.abs() * half.0 + hit.n.1.abs() * half.1 + hit.n.2.abs() * half.2;
            hit.p + hit.n * (support + 0.005) - center
        } else {
            ray.at(p.distance) - center
        };
        p.at.1 += p.elevation;
        if p.snap {
            p.at = V(
                (p.at.0 * 4.).round() / 4.,
                (p.at.1 * 4.).round() / 4.,
                (p.at.2 * 4.).round() / 4.,
            );
        }
        p.at.1 = p.at.1.max(0.);
    }
    fn place_pending(&mut self) -> Result<()> {
        let p = self.pending.as_ref().ok_or("Choose an asset first")?;
        let doc = creative::place(&self.stage.doc, &p.doc, p.at, p.turns, &self.stage.player)?;
        self.apply_edit(doc, true, true)?;
        self.notice = "Placed and saved. Click to place another copy; Z undoes.".into();
        Ok(())
    }
    fn apply_edit(&mut self, doc: MapDocument, remember: bool, addition: bool) -> Result<()> {
        for (id, b) in &doc.colliders {
            if !self.stage.doc.colliders.contains_key(id)
                && b.max.1 > self.stage.player.feet_height() + 0.001
                && b.min.1
                    < self.stage.player.feet_height() + self.stage.player.body_height() - 0.001
                && b.overlaps_xz(self.stage.player.position, self.kind.radius())
            {
                return Err("Move away before restoring an object at your position".into());
            }
        }
        let room = doc.build()?;
        // Bake only the new object's geometry during placement, never the whole map per frame.
        let meshes = if addition {
            let p = self.pending.as_ref().ok_or("No placement preview")?;
            posed_meshes(&p.meshes, p.at, p.turns)
        } else {
            game_client::static_meshes(&room.world)
        };
        creative::save(&doc, &self.save_path()?)?;
        if remember {
            if self.undo.len() == 16 {
                self.undo.remove(0);
            }
            self.undo.push(self.stage.doc.clone());
        }
        if addition {
            self.stage.meshes.extend(meshes);
        } else {
            self.stage.meshes = meshes;
        }
        self.stage.doc = doc;
        self.stage.room = room;
        self.stage.stepper.reset(&self.stage.player);
        Ok(())
    }
    fn creative_smoke(&mut self, dir: &Path) -> Result<()> {
        self.save_directory = dir.join("worlds");
        if self.save_directory.exists() {
            return Err("Creative smoke output must be new".into());
        }
        self.play_map = self
            .catalog
            .maps
            .iter()
            .position(|m| m.id == "calibration")
            .ok_or("Missing calibration map")?;
        self.play_character = 5;
        self.start_play()?;
        let base = self.stage.doc.entities.len();
        for (id, at, turns) in [
            ("sandbox/shipping-crate", V(0., 0., 12.), 1),
            ("sandbox/bench", V(-3., 0., 10.), 0),
            ("sandbox/work-light", V(3., 0., 10.), 3),
        ] {
            let index = self
                .catalog
                .assets
                .iter()
                .position(|a| a.id == id)
                .ok_or("Missing smoke asset")?;
            self.choose_asset(index)?;
            let p = self.pending.as_mut().unwrap();
            p.at = at;
            p.turns = turns;
            self.place_pending()?;
        }
        let placed = serde_json::to_value(&self.stage.doc)?;
        let removed = creative::remove(&self.stage.doc, "creative-3")?;
        self.apply_edit(removed, true, false)?;
        if self.stage.doc.entities.len() != base + 2 {
            return Err("Removal failed".into());
        }
        let previous = self.undo.last().ok_or("Missing undo")?.clone();
        self.apply_edit(previous, false, false)?;
        self.undo.pop();
        if serde_json::to_value(&self.stage.doc)? != placed {
            return Err("Undo failed".into());
        }
        let map = self.play_map;
        self.play_map = 0;
        self.play_character = 1;
        self.start_play()?;
        if self.skin_index != 1 {
            return Err("Character switch failed".into());
        }
        self.play_map = map;
        self.play_character = 5;
        self.start_play()?;
        if serde_json::to_value(&self.stage.doc)? != placed {
            return Err("World reload failed".into());
        }
        let index = self
            .catalog
            .assets
            .iter()
            .position(|a| a.id == "sandbox/shipping-crate")
            .unwrap();
        self.choose_asset(index)?;
        self.pending.as_mut().unwrap().at = V(2., 0., 13.);
        self.notice =
            "Creative session verified: three objects saved, deletion undone, world reloaded."
                .into();
        std::fs::write(
            dir.join("creative-check.json"),
            serde_json::to_vec_pretty(
                &serde_json::json!({"ok":true,"placed":3,"delete_undo_reload":true,"character_switch":true}),
            )?,
        )?;
        Ok(())
    }
    fn setup_ui(&mut self) -> Result<()> {
        let w = mq::screen_width();
        let h = mq::screen_height();
        ui::panel(0., 0., w, h, mq::Color::new(0.02, 0.06, 0.10, 0.94));
        let pw = (w - 36.).min(960.);
        let ph = (h - 36.).min(660.);
        let x = (w - pw) * 0.5;
        let y = (h - ph) * 0.5;
        ui::panel(x, y, pw, ph, ui::PAPER);
        ui::text("PLAY / CREATE", x + 24., y + 35., 25., ui::INK);
        ui::text(
            "Choose a world and a character. Your saved build loads automatically.",
            x + 24.,
            y + 61.,
            14.,
            ui::MUTED,
        );
        let gap = 20.;
        let col = (pw - 48. - gap) * 0.5;
        let right = x + 24. + col + gap;
        ui::text("WORLD", x + 24., y + 93., 14., ui::BLUE);
        ui::text("CHARACTER", right, y + 93., 14., ui::BLUE);
        let count = self.catalog.maps.len().max(fun_characters::NAMES.len());
        let row = ((ph - 200.) / count as f32).min(42.);
        let enabled = !self.shell.paused;
        for i in 0..self.catalog.maps.len() {
            if ui::button(
                &self.catalog.maps[i].name,
                mq::Rect::new(x + 24., y + 104. + i as f32 * row, col, row - 4.),
                self.play_map == i,
                enabled,
            ) {
                self.play_map = i;
            }
        }
        for (i, name) in fun_characters::NAMES.iter().enumerate() {
            if ui::button(
                name,
                mq::Rect::new(right, y + 104. + i as f32 * row, col, row - 4.),
                self.play_character == i,
                enabled,
            ) {
                self.play_character = i;
            }
        }
        ui::text(
            &ui::fit(&self.notice, pw - 48., 13.),
            x + 24.,
            y + ph - 62.,
            13.,
            ui::MUTED,
        );
        if ui::button(
            "Play selected world",
            mq::Rect::new(right, y + ph - 51., col, 36.),
            true,
            enabled,
        ) {
            if let Err(error) = self.start_play() {
                self.notice = error.to_string();
            }
        }
        if ui::button(
            "Cancel",
            mq::Rect::new(x + 24., y + ph - 51., col, 36.),
            false,
            enabled,
        ) {
            self.setup = false;
            self.input_delay = 2;
        }
        Ok(())
    }
    fn palette_ui(&mut self) -> Result<()> {
        let w = mq::screen_width();
        let h = mq::screen_height();
        ui::panel(0., 0., w, h, mq::Color::new(0.02, 0.06, 0.10, 0.86));
        let pw = (w - 36.).min(960.);
        let ph = (h - 36.).min(650.);
        let x = (w - pw) * 0.5;
        let y = (h - ph) * 0.5;
        ui::panel(x, y, pw, ph, ui::PAPER);
        ui::text("CREATIVE ASSETS", x + 24., y + 37., 27., ui::INK);
        ui::text(
            "Choose an asset, aim its preview, then click to place. Esc closes this palette.",
            x + 24.,
            y + 63.,
            14.,
            ui::MUTED,
        );
        let search = mq::Rect::new(x + 24., y + 80., pw - 48., 36.);
        ui::panel(search.x, search.y, search.w, search.h, mq::WHITE);
        mq::draw_rectangle_lines(search.x, search.y, search.w, search.h, 2., ui::BLUE);
        if !self.shell.paused {
            while let Some(c) = mq::get_char_pressed() {
                if !c.is_control() && self.spawn_query.len() < 80 {
                    self.spawn_query.push(c);
                    self.spawn_scroll = 0;
                }
            }
            if input::pressed(mq::KeyCode::Backspace) {
                self.spawn_query.pop();
                self.spawn_scroll = 0;
            }
        }
        let label = if self.spawn_query.is_empty() {
            "Type to search assets..."
        } else {
            &self.spawn_query
        };
        ui::text(
            &ui::fit(label, search.w - 24., 17.),
            search.x + 12.,
            search.y + 25.,
            17.,
            ui::INK,
        );
        let items = self.catalog.matching_assets(&self.spawn_query);
        let rows = ((ph - 204.) / 53.).floor().max(1.) as usize;
        let pages = items.len().div_ceil(rows * 2).max(1);
        self.spawn_scroll = self.spawn_scroll.min(pages - 1);
        let wheel = mq::mouse_wheel().1;
        if wheel < 0. {
            self.spawn_scroll = (self.spawn_scroll + 1).min(pages - 1);
        }
        if wheel > 0. {
            self.spawn_scroll = self.spawn_scroll.saturating_sub(1);
        }
        let col = (pw - 60.) * 0.5;
        for (slot, &i) in items
            .iter()
            .skip(self.spawn_scroll * rows * 2)
            .take(rows * 2)
            .enumerate()
        {
            let rx = x + 24. + (slot % 2) as f32 * (col + 12.);
            let ry = y + 132. + (slot / 2) as f32 * 53.;
            let label = format!(
                "{} / {}",
                self.catalog.assets[i].name, self.catalog.assets[i].group
            );
            if ui::button(
                &label,
                mq::Rect::new(rx, ry, col, 47.),
                false,
                !self.shell.paused,
            ) {
                if let Err(error) = self.choose_asset(i) {
                    self.notice = error.to_string();
                }
                break;
            }
        }
        if items.is_empty() {
            ui::text(
                "No matching assets. Backspace to adjust your search.",
                x + 24.,
                y + 160.,
                16.,
                ui::MUTED,
            );
        }
        let by = y + ph - 54.;
        if ui::button(
            "Previous",
            mq::Rect::new(x + 24., by, 120., 35.),
            false,
            self.spawn_scroll > 0,
        ) {
            self.spawn_scroll -= 1;
        }
        ui::text(
            &format!(
                "{} assets / page {} of {}",
                items.len(),
                self.spawn_scroll + 1,
                pages
            ),
            x + 158.,
            by + 24.,
            14.,
            ui::INK,
        );
        if ui::button(
            "Next",
            mq::Rect::new(x + pw - 284., by, 120., 35.),
            false,
            self.spawn_scroll + 1 < pages,
        ) {
            self.spawn_scroll += 1;
        }
        if ui::button(
            "Close",
            mq::Rect::new(x + pw - 152., by, 128., 35.),
            false,
            true,
        ) {
            self.asset_menu = false;
            self.search_focus = false;
            self.input_delay = 2;
        }
        Ok(())
    }

    fn update(&mut self) -> Result<()> {
        input::poll(foreground());
        if self.asset_menu && !self.shell.paused {
            self.search_focus = true;
        }
        if !self.search_focus {
            while mq::get_char_pressed().is_some() {}
        }
        if !foreground() {
            self.shell.paused = true;
        }
        if (self.asset_menu || self.setup) && input::pressed(mq::KeyCode::Escape) {
            self.asset_menu = false;
            self.setup = false;
            self.search_focus = false;
            self.input_delay = 2;
            self.stage.stepper.reset(&self.stage.player);
            return Ok(());
        }
        // Text entry must not treat letters F/C/etc. as game shortcuts.
        self.shell.begin_frame_with_input(
            !self.browser && !self.asset_menu && !self.setup,
            if self.search_focus {
                search_keys
            } else {
                input::pressed
            },
        );
        if let Some(child) = &mut self.physics_child {
            let _ = child.try_wait();
        }
        if !self.shell.paused && !self.asset_menu && !self.setup && input::pressed(mq::KeyCode::Tab)
        {
            self.browser = !self.browser;
            self.search_focus = false;
            self.input_delay = 2;
            self.stage.stepper.reset(&self.stage.player);
        }
        if self.input_delay > 0 {
            self.input_delay -= 1;
            return Ok(());
        }
        if !self.browser && !self.asset_menu && !self.setup && self.shell.playing() {
            if input::pressed(mq::KeyCode::V) {
                self.asset_menu = self.world_map.is_some();
                self.setup = self.world_map.is_none();
                self.search_focus = self.asset_menu;
                self.spawn_scroll = 0;
                mq::set_cursor_grab(false);
                mq::show_mouse(true);
                self.stage.player.stop();
                self.stage.stepper.reset(&self.stage.player);
                return Ok(());
            }
            let mouse = mq::mouse_delta_position();
            self.stage.player.yaw -= mouse.x * 2.5;
            self.stage.player.pitch = (self.stage.player.pitch + mouse.y * 2.5).clamp(-1.45, 1.45);
            if input::pressed(mq::KeyCode::Q) {
                self.perspective.toggle();
            }
            if input::pressed(mq::KeyCode::Home) {
                self.stage.reset(self.kind)?;
            }
            if let Some(p) = &mut self.pending {
                if input::pressed(mq::KeyCode::R) {
                    p.turns = (p.turns + 1) % 4;
                }
                if input::pressed(mq::KeyCode::G) {
                    p.snap = !p.snap;
                }
                let scroll = mq::mouse_wheel().1;
                if input::down(mq::KeyCode::LeftShift) {
                    p.elevation = (p.elevation + scroll * 0.25).clamp(-10., 30.);
                } else {
                    p.distance = (p.distance + scroll * 0.5).clamp(2., 30.);
                }
            }
            self.position_pending();
            if mq::is_mouse_button_pressed(mq::MouseButton::Right) {
                self.pending = None;
            }
            if mq::is_mouse_button_pressed(mq::MouseButton::Left) && self.pending.is_some() {
                if let Err(error) = self.place_pending() {
                    self.notice = error.to_string();
                }
            }
            if input::pressed(mq::KeyCode::Z) {
                if let Some(previous) = self.undo.last().cloned() {
                    if let Err(error) = self.apply_edit(previous, false, false) {
                        self.notice = error.to_string();
                    } else {
                        self.undo.pop();
                        self.notice = "Last edit undone and saved".into();
                    }
                }
            }
            if input::pressed(mq::KeyCode::Delete) {
                let id = self.stage.room.hit(self.aim_ray(), 4.5).and_then(|hit| {
                    self.stage
                        .doc
                        .entities
                        .iter()
                        .rev()
                        .find(|e| e.id.starts_with("creative-") && e.bounds.contains(hit.p))
                        .map(|e| e.id.clone())
                });
                let result = id
                    .ok_or_else(|| "Aim at a placed object within reach".into())
                    .and_then(|id| creative::remove(&self.stage.doc, &id))
                    .and_then(|doc| self.apply_edit(doc, true, false));
                if let Err(error) = result {
                    self.notice = error.to_string();
                } else {
                    self.notice = "Object removed and world saved".into();
                }
            }
            if input::pressed(mq::KeyCode::B) {
                self.bounds = !self.bounds;
            }
            if input::pressed(mq::KeyCode::E) {
                if let Some(e) = self.stage.room.focus(self.aim_ray()) {
                    let label = e.label.clone();
                    if let Some(index) = self.catalog.assets.iter().position(|a| a.name == label) {
                        if self.world_map.is_some() {
                            if let Err(error) = self.choose_asset(index) {
                                self.notice = error.to_string();
                            }
                        } else {
                            self.select(Tab::Assets, index)?;
                            self.browser = true;
                        }
                        return Ok(());
                    }
                    self.notice = format!("{} / {}", e.label, e.id);
                }
            }
            let (forward, right) = input::axes();
            let before = self.stage.player.position;
            self.ticks += self.stage.stepper.advance(
                &mut self.stage.player,
                Movement {
                    forward,
                    right,
                    sprint: input::down(mq::KeyCode::LeftShift)
                        || input::down(mq::KeyCode::RightShift),
                    jump: input::pressed(mq::KeyCode::Space),
                    crouch: input::down(mq::KeyCode::C) || input::down(mq::KeyCode::LeftControl),
                },
                mq::get_frame_time(),
                &self.stage.room.colliders,
            ) as u64;
            let distance = (self.stage.player.position - before).length();
            self.actor
                .update(distance, mq::get_frame_time(), forward != 0. || right != 0.);
            self.skin
                .update(distance, mq::get_frame_time(), forward != 0. || right != 0.);
        } else {
            self.stage.player.stop();
            self.stage.stepper.reset(&self.stage.player);
        }
        Ok(())
    }
    fn draw(&mut self) -> Result<bool> {
        self.draw_world();
        if self.browser {
            self.browser_ui()?;
        } else {
            let w = mq::screen_width();
            let h = mq::screen_height();
            mq::draw_circle(w / 2., h / 2., 2., mq::WHITE);
            ui::panel(18., 18., 300., 53., mq::Color::new(0.06, 0.13, 0.20, 0.85));
            ui::text(self.title(), 30., 43., 20., mq::WHITE);
            ui::text(
                fun_characters::NAMES[self.skin_index],
                30.,
                61.,
                11.,
                ui::ACCENT,
            );
            if let Some(e) = self.stage.room.focus(self.aim_ray()) {
                ui::text(
                    &format!("E  Inspect {}", e.label),
                    w / 2. - 120.,
                    h / 2. + 38.,
                    17.,
                    mq::WHITE,
                );
            }
            if self.shell.diagnostics {
                ui::text(
                    &format!(
                        "{} FPS / {} ticks / x {:.2} z {:.2} / feet {:.2}",
                        mq::get_fps(),
                        self.ticks,
                        self.stage.player.position.0,
                        self.stage.player.position.2,
                        self.stage.player.feet_height()
                    ),
                    20.,
                    h - 22.,
                    16.,
                    ui::INK,
                );
            }
        }
        if !self.browser && !self.asset_menu {
            let y = mq::screen_height() - 62.;
            ui::panel(
                16.,
                y - 18.,
                mq::screen_width() - 32.,
                68.,
                mq::Color::new(0.04, 0.10, 0.15, 0.90),
            );
            let hint = if let Some(p) = &self.pending {
                format!("{}  |  Click: place   R: rotate   Wheel: reach   Shift+wheel: height   G: snap {}   Right click: cancel", self.catalog.assets[p.asset].name, if p.snap {"ON"} else {"OFF"})
            } else {
                "V: assets   E: copy aimed asset   Delete: remove placed object   Z: undo   Tab: workbench   Q: camera".into()
            };
            ui::text(
                &ui::fit(&hint, mq::screen_width() - 60., 14.),
                28.,
                y + 4.,
                14.,
                ui::ACCENT,
            );
            ui::text(
                &ui::fit(&self.notice, mq::screen_width() - 60., 14.),
                28.,
                y + 29.,
                14.,
                mq::WHITE,
            );
        }
        if self.setup {
            self.setup_ui()?;
        }
        if self.asset_menu {
            self.palette_ui()?;
        }
        if self.shell.paused {
            self.search_focus = false;
        }
        Ok(self.shell.local_menu(
            "BlueEngineSandbox",
            &[
                "WASD / arrows: move. Mouse: look.",
                "Space: jump. Shift: sprint. C: crouch.",
                "V: asset palette. Click: place. Right click: cancel.",
                "R: rotate. Wheel: reach. Shift+wheel: height.",
                "G: snap. Delete: remove. Z: undo. Home: spawn.",
                "F / F11: fullscreen. Esc: menu.",
            ],
        ))
    }
}
fn sidebar_width() -> f32 {
    (mq::screen_width() * 0.29).clamp(265., 370.)
}
fn search_keys(key: mq::KeyCode) -> bool {
    matches!(key, mq::KeyCode::Escape | mq::KeyCode::F11) && input::pressed(key)
}
fn foreground() -> bool {
    #[cfg(windows)]
    {
        // Read-only focus query for this executable's process; no foreign window mutation.
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
fn config() -> macroquad::conf::Conf {
    let mut config = game_client::window_config("BlueEngineSandbox");
    // Decode the existing PNG entries in the approved ICO, without changing artwork.
    let ico = include_bytes!("../../assets/branding/blueengine.ico");
    let read =
        |offset: usize| u32::from_le_bytes(ico[offset..offset + 4].try_into().unwrap()) as usize;
    let pixels = |entry: usize| {
        let start = read(6 + entry * 16 + 12);
        let length = read(6 + entry * 16 + 8);
        mq::Image::from_file_with_format(&ico[start..start + length], Some(mq::ImageFormat::Png))
            .expect("approved icon entry")
            .bytes
    };
    config.miniquad_conf.icon = Some(mq::miniquad::conf::Icon {
        small: pixels(0).try_into().unwrap(),
        medium: pixels(1).try_into().unwrap(),
        big: pixels(3).try_into().unwrap(),
    });
    config
}

#[macroquad::main(config)]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("BlueEngineSandbox: {e}");
        // Keep load failures readable in packaged Windows builds with no console.
        loop {
            input::poll(foreground());
            mq::clear_background(ui::PAPER);
            ui::text("BlueEngineSandbox could not load", 30., 65., 28., ui::INK);
            ui::paragraph(&e.to_string(), 30., 110., mq::screen_width() - 60., 18.);
            ui::text(
                "Escape to close",
                30.,
                mq::screen_height() - 30.,
                18.,
                ui::BLUE,
            );
            if input::pressed(mq::KeyCode::Escape) {
                break;
            }
            mq::next_frame().await;
        }
    }
}
async fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let value = |flag: &str| {
        args.iter()
            .position(|x| x == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let root = content::find_root(value("--root").map(PathBuf::from))?;
    game_text::initialize();
    mq::clear_background(ui::PAPER);
    ui::text("Loading BlueEngineSandbox...", 40., 70., 28., ui::INK);
    mq::next_frame().await;
    let mut app = App::new(root)?;
    if let Some(id) = value("--map") {
        let i = app
            .catalog
            .maps
            .iter()
            .position(|m| m.id == id)
            .ok_or("Unknown --map ID")?;
        app.select(Tab::Maps, i)?;
    }
    if let Some(id) = value("--asset") {
        let i = app
            .catalog
            .assets
            .iter()
            .position(|a| a.id == id)
            .ok_or("Unknown --asset ID")?;
        app.select(Tab::Assets, i)?;
    }
    if let Some(id) = value("--character") {
        let i = fun_characters::IDS
            .iter()
            .position(|s| *s == id)
            .ok_or("Unknown character")?;
        app.select(Tab::Characters, i)?;
    }
    if let Some(directory) = value("--save-dir") {
        app.save_directory = PathBuf::from(directory);
    }
    if args.iter().any(|arg| arg == "--play") {
        app.start_play()?;
    }
    let creative_capture = value("--creative-smoke").map(PathBuf::from);
    let capture = creative_capture
        .clone()
        .or_else(|| value("--capture").map(PathBuf::from));
    if let Some(dir) = &capture {
        std::fs::create_dir_all(dir)?;
    }
    if let Some(dir) = &creative_capture {
        app.creative_smoke(dir)?;
        app.setup = true;
    }
    let mut frames = 0;
    loop {
        if capture.is_none() {
            app.update()?;
        } else {
            app.shell.begin_frame(false);
        }
        if app.draw()? {
            break;
        }
        if frames > 10 {
            app.frame_times.push(mq::get_frame_time() * 1000.);
        }
        if let Some(dir) = &capture {
            if frames == 10 && creative_capture.is_some() {
                capture_frame(dir, "setup.png").await;
                app.setup = false;
            }
            if frames == 20 {
                capture_frame(dir, "browser.png").await;
                app.browser = false;
                if creative_capture.is_some() {
                    app.asset_menu = true;
                    app.search_focus = true;
                }
            }
            if frames == 45 {
                capture_frame(dir, "walk.png").await;
                app.asset_menu = false;
                app.search_focus = false;
                app.shell.paused = true;
            }
            if frames == 65 {
                capture_frame(dir, "menu.png").await;
                app.frame_times.sort_by(f32::total_cmp);
                let len = app.frame_times.len();
                let report = serde_json::json!({"frames":len,"frame_ms_p50":app.frame_times[len/2],"frame_ms_p95":app.frame_times[len*95/100],"map":app.title(),"note":"Capture timing, not interactive movement verification."});
                std::fs::write(
                    dir.join("capture.json"),
                    serde_json::to_vec_pretty(&report)?,
                )?;
                break;
            }
        }
        frames += 1;
        mq::next_frame().await;
    }
    Ok(())
}
async fn capture_frame(dir: &Path, name: &str) {
    mq::get_screen_data().export_png(dir.join(name).to_str().unwrap());
    mq::next_frame().await;
}
