//! Portable layered presentation: logical 2D coordinates and optional depth-tested 3D views.
use super::{Point, Rect};
use macroquad::prelude::*;
pub use macroquad::prelude::{Camera3D, Mesh, Vec3, Vertex};
pub use macroquad::prelude::{Color, Texture2D};
/// An optional 3D view, placed anywhere in the same layer list as sprites and HUD.
/// Camera/meshes are presentation only; simulation owns collision and game rules.
pub struct World {
    pub camera: Camera3D,
    items: Vec<Solid>,
}
enum Solid {
    Cube(Vec3, Vec3, Color),
    Sphere(Vec3, f32, Color),
    Mesh(Mesh),
}
impl World {
    pub fn new(position: [f32; 3], target: [f32; 3]) -> Self {
        Self {
            camera: Camera3D {
                position: Vec3::from_array(position),
                target: Vec3::from_array(target),
                up: Vec3::Y,
                ..Default::default()
            },
            items: vec![],
        }
    }
    pub fn cube(&mut self, position: [f32; 3], size: [f32; 3], color: Color) {
        self.items.push(Solid::Cube(
            Vec3::from_array(position),
            Vec3::from_array(size),
            color,
        ));
    }
    pub fn sphere(&mut self, position: [f32; 3], radius: f32, color: Color) {
        self.items
            .push(Solid::Sphere(Vec3::from_array(position), radius, color));
    }
    /// Arbitrary textured geometry using the engine's pinned renderer vertex/mesh format.
    pub fn mesh(&mut self, mesh: Mesh) {
        self.items.push(Solid::Mesh(mesh));
    }
    fn draw(mut self, rect: Rect, view: Viewport) -> Result<(), String> {
        self.camera.viewport =
            Some(view.physical_rect(rect, screen_height(), screen_dpi_scale())?);
        let (_, _, width, height) = self.camera.viewport.unwrap();
        self.camera.aspect = Some(width as f32 / height as f32);
        set_camera(&self.camera);
        for item in self.items {
            match item {
                Solid::Cube(p, s, c) => draw_cube(p, s, None, c),
                Solid::Sphere(p, r, c) => draw_sphere(p, r, None, c),
                Solid::Mesh(m) => draw_mesh(&m),
            }
        }
        set_default_camera();
        Ok(())
    }
}
pub const INK: Color = Color::new(0.06, 0.10, 0.16, 1.);
pub const WHITE: Color = Color::new(0.94, 0.94, 0.86, 1.);
pub const GOLD: Color = Color::new(1., 0.72, 0.25, 1.);
pub const TEAL: Color = Color::new(0.28, 0.83, 0.75, 1.);
pub const PINK: Color = Color::new(0.95, 0.39, 0.49, 1.);
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
    pub scale: f32,
    pub offset: [f32; 2],
}
impl Viewport {
    pub fn fit(width: f32, height: f32, screen_w: f32, screen_h: f32) -> Self {
        assert!(
            width > 0. && height > 0. && screen_w > 0. && screen_h > 0.,
            "Viewport dimensions must be positive"
        );
        let scale = (screen_w / width).min(screen_h / height);
        Self {
            width,
            height,
            scale,
            offset: [
                (screen_w - width * scale) / 2.,
                (screen_h - height * scale) / 2.,
            ],
        }
    }
    /// Convert logical top-left coordinates into framebuffer bottom-left pixels.
    /// 3D viewports bypass the 2D projection and therefore must include device DPI.
    pub fn physical_rect(
        &self,
        rect: Rect,
        screen_h: f32,
        dpi: f32,
    ) -> Result<(i32, i32, i32, i32), String> {
        if rect.w <= 0
            || rect.h <= 0
            || rect.x < 0
            || rect.y < 0
            || rect.x as f32 + rect.w as f32 > self.width
            || rect.y as f32 + rect.h as f32 > self.height
            || !dpi.is_finite()
            || dpi <= 0.
        {
            return Err("3D view must have positive dimensions inside the logical canvas and a valid device scale".into());
        }
        Ok((
            ((self.offset[0] + rect.x as f32 * self.scale) * dpi).round() as i32,
            ((screen_h - self.offset[1] - (rect.y as f32 + rect.h as f32) * self.scale) * dpi)
                .round() as i32,
            (rect.w as f32 * self.scale * dpi).round().max(1.) as i32,
            (rect.h as f32 * self.scale * dpi).round().max(1.) as i32,
        ))
    }
    pub fn pointer(&self, x: f32, y: f32) -> Option<Point> {
        let x = (x - self.offset[0]) / self.scale;
        let y = (y - self.offset[1]) / self.scale;
        (x >= 0. && y >= 0. && x < self.width && y < self.height)
            .then_some(Point::new(x.floor() as i32, y.floor() as i32))
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Transform {
    pub position: [f32; 2],
    pub scale: [f32; 2],
    pub rotation: f32,
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            position: [0.; 2],
            scale: [1.; 2],
            rotation: 0.,
        }
    }
}
/// Validated sprite strip; frame choice uses simulation ticks, never wall time.
pub struct Animation {
    frames: u32,
    ticks: u32,
}
impl Animation {
    pub fn new(frames: u32, ticks_per_frame: u32) -> Result<Self, String> {
        if frames == 0 || ticks_per_frame == 0 {
            return Err("Animation needs at least one frame and one tick per frame".into());
        }
        Ok(Self {
            frames,
            ticks: ticks_per_frame,
        })
    }
    pub fn frame(&self, tick: u32) -> u32 {
        (tick / self.ticks) % self.frames
    }
}
type RenderView<'a> = Box<dyn FnOnce((i32, i32, i32, i32)) -> Result<(), String> + 'a>;
enum Shape<'a> {
    RenderView(Rect, RenderView<'a>),
    World(Rect, World),
    Rect(Rect, Color),
    Circle(Point, f32, Color),
    Text(String, Point, f32, Color, Option<&'static str>),
    Sprite(
        Texture2D,
        Transform,
        [f32; 2],
        Option<macroquad::prelude::Rect>,
        Color,
    ),
    Png(&'static str, &'static [u8], Transform, [f32; 2], Color),
}
/// Keeps decoded embedded textures across frames. Construct after the graphics context starts.
#[derive(Default)]
pub struct Renderer {
    fonts: std::collections::BTreeMap<&'static str, Font>,
    textures: std::collections::BTreeMap<&'static str, (Texture2D, u64)>,
}
/// Runtime font assets, loaded once after window creation and included in the game's package.
pub struct FontAsset {
    pub id: &'static str,
    pub file: &'static str,
}
impl Renderer {
    pub async fn load_fonts(&mut self, assets: &[FontAsset]) -> Result<(), String> {
        for asset in assets {
            if asset.id.is_empty() || self.fonts.contains_key(asset.id) {
                return Err(format!("Empty or duplicate font ID {:?}", asset.id));
            }
            let font = load_ttf_font(asset.file)
                .await
                .map_err(|e| format!("Font {} ({}) failed: {e}", asset.id, asset.file))?;
            self.fonts.insert(asset.id, font);
        }
        Ok(())
    }
    /// Logical-pixel metrics using the exact same font, raster size and scale as drawing.
    pub fn measure(
        &self,
        text: &str,
        font: Option<&str>,
        size: f32,
    ) -> Result<TextDimensions, String> {
        if !size.is_finite() || size <= 0. {
            return Err("Text size must be positive and finite".into());
        }
        match font {
            Some(id) => {
                let font = self
                    .fonts
                    .get(id)
                    .ok_or_else(|| format!("Unknown font {id:?}; declare Game::fonts()"))?;
                Ok(measure_text(text, Some(font), 64, size / 64.))
            }
            None => Ok(measure_text(text, None, 64, size / 64.)),
        }
    }
}
#[derive(Default)]
pub struct Scene<'a> {
    items: Vec<(i32, Shape<'a>)>,
}
impl<'a> Scene<'a> {
    /// Reuse an existing read-only 3D kit renderer in a logical viewport.
    /// The callback receives a validated physical viewport and must apply it to its cameras.
    /// It must not clear the entire framebuffer or mutate simulation. Shared overlays follow normally.
    pub fn render_view(
        &mut self,
        layer: i32,
        viewport: Rect,
        render: impl FnOnce((i32, i32, i32, i32)) -> Result<(), String> + 'a,
    ) {
        self.items
            .push((layer, Shape::RenderView(viewport, Box::new(render))));
    }
    /// Insert a depth-tested 3D scene at this layer, bounded by logical pixels.
    /// Later 2D layers can be maps/HUD; earlier layers can be backgrounds.
    pub fn world(&mut self, layer: i32, viewport: Rect, world: World) {
        self.items.push((layer, Shape::World(viewport, world)));
    }
    pub fn rect(&mut self, layer: i32, r: Rect, c: Color) {
        self.items.push((layer, Shape::Rect(r, c)));
    }
    pub fn circle(&mut self, layer: i32, p: Point, r: f32, c: Color) {
        self.items.push((layer, Shape::Circle(p, r, c)));
    }
    pub fn text(&mut self, layer: i32, text: impl Into<String>, p: Point, size: f32, c: Color) {
        self.items
            .push((layer, Shape::Text(text.into(), p, size, c, None)));
    }
    /// Game-owned TTF/OTF selected by an ID from Game::fonts(). Position is a baseline.
    pub fn text_with_font(
        &mut self,
        layer: i32,
        text: impl Into<String>,
        p: Point,
        size: f32,
        c: Color,
        font: &'static str,
    ) {
        self.items
            .push((layer, Shape::Text(text.into(), p, size, c, Some(font))));
    }
    pub fn sprite(
        &mut self,
        layer: i32,
        texture: &Texture2D,
        transform: Transform,
        size: [f32; 2],
        source: Option<macroquad::prelude::Rect>,
        color: Color,
    ) {
        self.items.push((
            layer,
            Shape::Sprite(texture.clone(), transform, size, source, color),
        ));
    }
    pub fn sprite_png(
        &mut self,
        layer: i32,
        id: &'static str,
        png: &'static [u8],
        transform: Transform,
        size: [f32; 2],
        color: Color,
    ) {
        self.items
            .push((layer, Shape::Png(id, png, transform, size, color)));
    }
    pub fn draw(
        &mut self,
        view: Viewport,
        camera: Point,
        renderer: &mut Renderer,
    ) -> Result<(), String> {
        self.items.sort_by_key(|(layer, _)| *layer);
        // Populate all glyphs before any draw batches: later atlas growth must not
        // invalidate text already queued. Custom fonts rasterize at a fixed 64px.
        for (_, shape) in &self.items {
            if let Shape::Text(text, _, size, _, font) = shape {
                renderer.measure(text, *font, *size * view.scale)?;
            }
        }
        let p = |x: f32, y: f32| {
            [
                view.offset[0] + (x - camera.x as f32) * view.scale,
                view.offset[1] + (y - camera.y as f32) * view.scale,
            ]
        };
        for (_, shape) in self.items.drain(..) {
            match shape {
                Shape::RenderView(rect, render) => {
                    render(view.physical_rect(rect, screen_height(), screen_dpi_scale())?)?;
                    gl_use_default_material();
                    set_default_camera();
                }
                Shape::World(rect, world) => world.draw(rect, view)?,
                Shape::Rect(r, c) => {
                    let xy = p(r.x as f32, r.y as f32);
                    draw_rectangle(
                        xy[0],
                        xy[1],
                        r.w as f32 * view.scale,
                        r.h as f32 * view.scale,
                        c,
                    );
                }
                Shape::Circle(pos, r, c) => {
                    let xy = p(pos.x as f32, pos.y as f32);
                    draw_circle(xy[0], xy[1], r * view.scale, c);
                }
                Shape::Text(text, pos, size, c, font) => {
                    let xy = p(pos.x as f32, pos.y as f32);
                    if let Some(id) = font {
                        draw_text_ex(
                            &text,
                            xy[0],
                            xy[1],
                            TextParams {
                                font: Some(&renderer.fonts[id]),
                                font_size: 64,
                                font_scale: size * view.scale / 64.,
                                color: c,
                                ..Default::default()
                            },
                        );
                    } else {
                        draw_text_ex(
                            &text,
                            xy[0],
                            xy[1],
                            TextParams {
                                font_size: 64,
                                font_scale: size * view.scale / 64.,
                                color: c,
                                ..Default::default()
                            },
                        );
                    }
                }
                Shape::Sprite(texture, t, size, source, c) => {
                    let xy = p(t.position[0], t.position[1]);
                    draw_texture_ex(
                        &texture,
                        xy[0],
                        xy[1],
                        c,
                        DrawTextureParams {
                            dest_size: Some(vec2(
                                size[0] * t.scale[0] * view.scale,
                                size[1] * t.scale[1] * view.scale,
                            )),
                            source,
                            rotation: t.rotation,
                            ..Default::default()
                        },
                    );
                }
                Shape::Png(id, bytes, t, size, c) => {
                    let hash = crate::runtime::StateHasher::new().bytes(bytes).finish();
                    if let Some((_, previous)) = renderer.textures.get(id) {
                        if *previous != hash {
                            return Err(format!(
                                "Sprite ID {id} names different PNGs; use a unique asset ID"
                            ));
                        }
                    } else {
                        let image = Image::from_file_with_format(bytes, Some(ImageFormat::Png))
                            .map_err(|e| format!("Sprite {id} failed PNG decoding: {e}"))?;
                        renderer
                            .textures
                            .insert(id, (Texture2D::from_image(&image), hash));
                    }
                    let texture = &renderer.textures[id].0;
                    let xy = p(t.position[0], t.position[1]);
                    draw_texture_ex(
                        texture,
                        xy[0],
                        xy[1],
                        c,
                        DrawTextureParams {
                            dest_size: Some(vec2(
                                size[0] * t.scale[0] * view.scale,
                                size[1] * t.scale[1] * view.scale,
                            )),
                            rotation: t.rotation,
                            ..Default::default()
                        },
                    );
                }
            }
        }
        Ok(())
    }
}
struct Particle {
    p: [f32; 2],
    v: [f32; 2],
    life: f32,
    color: Color,
}
pub struct Particles {
    items: Vec<Particle>,
    rng: crate::runtime::Rng,
}
impl Particles {
    pub fn new(seed: u64) -> Self {
        Self {
            items: Vec::new(),
            rng: crate::runtime::Rng::new(seed),
        }
    }
    pub fn burst(&mut self, at: Point, color: Color) {
        for _ in 0..12 {
            if self.items.len() < 512 {
                self.items.push(Particle {
                    p: [at.x as f32, at.y as f32],
                    v: [self.rng.range(-65., 65.), self.rng.range(-90., -15.)],
                    life: 0.7,
                    color,
                });
            }
        }
    }
    pub fn update(&mut self, dt: f32, scene: &mut Scene) {
        for p in &mut self.items {
            p.life -= dt;
            p.p[0] += p.v[0] * dt;
            p.p[1] += p.v[1] * dt;
            p.v[1] += 90. * dt;
            scene.circle(
                50,
                Point::new(p.p[0] as i32, p.p[1] as i32),
                3.,
                Color {
                    a: (p.life / 0.7).clamp(0., 1.),
                    ..p.color
                },
            );
        }
        self.items.retain(|p| p.life > 0.);
    }
}
pub trait Game: super::GameLogic {
    fn draw<'a>(&'a self, scene: &mut Scene<'a>);
    /// Map native device edges into public Intent before fixed-step accumulation.
    /// Called only while focused, started and unpaused. Camera-only state may be
    /// updated here; gameplay must still change exclusively in Simulation::step.
    /// A changed pointer with action=true is retained as an explicit command target.
    fn device_input(&mut self, input: super::Intent) -> super::Intent {
        input
    }
    /// Optional third-person look: drag inside the canvas or use the controller's right stick.
    fn drag_look() -> bool {
        false
    }
    fn fonts() -> &'static [FontAsset] {
        &[]
    }
    fn theme() -> super::client::Theme {
        super::client::Theme::default()
    }
    /// Replace every overlay with game-owned drawing and action hit regions. The shared
    /// client still owns pause, saves, focus, timing, input gating and restart.
    fn interface(&self, scene: &mut Scene, frame: &super::client::UiFrame) -> super::ui::Layout {
        super::client::default_interface::<Self>(self, scene, frame)
    }
    fn cue_particles() -> bool {
        true
    }
    fn show_hud() -> bool {
        true
    }
    fn menu_status(&self) -> String {
        String::new()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hybrid_views_align_with_2d_at_high_device_dpi() {
        let view = Viewport::fit(800., 450., 400., 225.);
        assert_eq!(
            view.physical_rect(Rect::new(20, 55, 760, 345), 225., 2.)
                .unwrap(),
            (20, 50, 760, 345)
        );
        assert!(view
            .physical_rect(Rect::new(800, 0, 1, 1), 225., 2.)
            .is_err());
        assert!(view
            .physical_rect(Rect::new(i32::MAX, 0, i32::MAX, 1), 225., 2.)
            .is_err());
    }
    #[test]
    fn letterbox_coordinates_and_animation_are_explicit() {
        let v = Viewport::fit(800., 450., 1000., 1000.);
        assert_eq!(v.pointer(500., 500.), Some(Point::new(400, 225)));
        assert_eq!(v.pointer(0., 0.), None);
        assert!(Animation::new(0, 5).is_err());
        assert_eq!(Animation::new(4, 6).unwrap().frame(25), 0);
    }
}
