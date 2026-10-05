//! Macroquad-backed drawing in logical pixels; stable painter layers, no 3D initialization.
use super::{Point, Rect};
use macroquad::prelude::*;
pub use macroquad::prelude::{Color, Texture2D};
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
enum Shape {
    Rect(Rect, Color),
    Circle(Point, f32, Color),
    Text(String, Point, f32, Color),
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
    textures: std::collections::BTreeMap<&'static str, (Texture2D, u64)>,
}
#[derive(Default)]
pub struct Scene {
    items: Vec<(i32, Shape)>,
}
impl Scene {
    pub fn rect(&mut self, layer: i32, r: Rect, c: Color) {
        self.items.push((layer, Shape::Rect(r, c)));
    }
    pub fn circle(&mut self, layer: i32, p: Point, r: f32, c: Color) {
        self.items.push((layer, Shape::Circle(p, r, c)));
    }
    pub fn text(&mut self, layer: i32, text: impl Into<String>, p: Point, size: f32, c: Color) {
        self.items
            .push((layer, Shape::Text(text.into(), p, size, c)));
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
        // Atlas growth can replace its GL texture. Populate every glyph before queuing draw calls,
        // so a later notice cannot invalidate text already submitted in this same frame.
        for (_, shape) in &self.items {
            if let Shape::Text(text, _, size, _) = shape {
                measure_text(text, None, (size * view.scale) as u16, 1.);
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
                Shape::Text(text, pos, size, c) => {
                    let xy = p(pos.x as f32, pos.y as f32);
                    draw_text(&text, xy[0], xy[1], size * view.scale, c);
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
    fn draw(&self, scene: &mut Scene);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn letterbox_coordinates_and_animation_are_explicit() {
        let v = Viewport::fit(800., 450., 1000., 1000.);
        assert_eq!(v.pointer(500., 500.), Some(Point::new(400, 225)));
        assert_eq!(v.pointer(0., 0.), None);
        assert!(Animation::new(0, 5).is_err());
        assert_eq!(Animation::new(4, 6).unwrap().frame(25), 0);
    }
}
