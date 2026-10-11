use super::*;
use macroquad::prelude::*;
use vesper3d::portable::Rect;
const VERTEX: &str = "#version 100\nattribute vec3 position; attribute vec2 texcoord; varying vec2 uv; uniform mat4 Model; uniform mat4 Projection; void main(){ uv=texcoord; gl_Position=Projection*Model*vec4(position,1.0); }";
impl draw::Game for Wormhole {
    fn drag_look() -> bool {
        true
    }
    fn show_hud() -> bool {
        false
    }
    fn menu_status(&self) -> String {
        "No destination. Let yourself fall.".into()
    }
    fn show_overlays() -> bool {
        false
    }
    fn start_automatically() -> bool {
        true
    }
    fn device_input(&mut self, mut input: Intent) -> Intent {
        if is_mouse_button_pressed(MouseButton::Left) && !is_key_pressed(KeyCode::Space) {
            input.action = false;
        }
        input
    }
    fn draw<'a>(&'a self, scene: &mut draw::Scene<'a>) {
        scene.render_view(0, Rect::new(0, 0, 800, 450), move |viewport| {
            if self.material.borrow().is_none() {
                let material = load_material(
                    ShaderSource::Glsl {
                        vertex: VERTEX,
                        fragment: include_str!("wormhole.frag"),
                    },
                    MaterialParams {
                        uniforms: ["Journey", "Phase", "Seeds", "Camera"]
                            .into_iter()
                            .map(|n| UniformDesc::new(n, UniformType::Float4))
                            .collect(),
                        ..Default::default()
                    },
                )
                .map_err(|e| format!("Wormhole shader: {e}"))?;
                *self.material.borrow_mut() = Some(material);
            }
            let material = self.material.borrow();
            let material = material.as_ref().unwrap();
            let time = self.state.ticks as f64 / 60.;
            let phases = [0.071, 0.113, 0.037, 0.019].map(|f| (time * f).sin() as f32);
            let seeds =
                [-1, 0, 1, 2].map(|offset| (self.region_key(offset) >> 40) as f32 / 16_777_216.);
            material.set_uniform(
                "Journey",
                vec4(
                    (self.state.distance_mm % REGION_MM) as f32 / 1000.,
                    viewport.2 as f32 / viewport.3 as f32,
                    0.,
                    0.,
                ),
            );
            material.set_uniform("Phase", vec4(phases[0], phases[1], phases[2], phases[3]));
            material.set_uniform("Seeds", vec4(seeds[0], seeds[1], seeds[2], seeds[3]));
            material.set_uniform(
                "Camera",
                vec4(
                    self.state.yaw as f32 / 1_000_000.,
                    self.state.pitch as f32 / 1_000_000.,
                    0.,
                    0.,
                ),
            );
            let mut camera =
                Camera2D::from_display_rect(macroquad::prelude::Rect::new(0., 0., 800., 450.));
            camera.viewport = Some(viewport);
            set_camera(&camera);
            gl_use_material(material);
            draw_rectangle(0., 0., 800., 450., WHITE);
            gl_use_default_material();
            set_default_camera();
            Ok(())
        });
    }
}
