//! Static art proof, native or WASM: cargo run --no-default-features --features portable --example imported_room.
//! Native capture: python3 tools/xcapture.py target/itest/examples/imported_room --frames 5.
use macroquad::prelude::*;
use vesper3d::{
    asset_model::draw::Model,
    portable::{
        draw::{Renderer, Scene, Viewport, World},
        Point, Rect,
    },
};

fn window() -> Conf {
    Conf {
        window_title: "CC0 imported room".into(),
        window_width: 960,
        window_height: 640,
        ..Default::default()
    }
}

#[macroquad::main(window)]
async fn main() {
    let table = Model::from_json(include_bytes!("../assets/models/cc0/table/model.json")).unwrap();
    let chair = Model::from_json(include_bytes!("../assets/models/cc0/chair/model.json")).unwrap();
    let lamp =
        Model::from_json(include_bytes!("../assets/models/cc0/floor-lamp/model.json")).unwrap();
    let textured = Model::from_json(include_bytes!(
        "../assets/models/cc0/textured-cube/model.json"
    ))
    .unwrap();
    let mut renderer = Renderer::default();
    #[cfg(not(target_arch = "wasm32"))]
    let args: Vec<_> = std::env::args().collect();
    #[cfg(not(target_arch = "wasm32"))]
    let capture = args
        .windows(2)
        .find(|v| v[0] == "--capture")
        .map(|v| std::path::PathBuf::from(&v[1]));
    #[cfg(not(target_arch = "wasm32"))]
    let frames: Vec<u32> = args
        .windows(2)
        .find(|v| v[0] == "--frames")
        .map(|v| v[1].split(',').map(|s| s.parse().unwrap()).collect())
        .unwrap_or(vec![5]);
    #[cfg(not(target_arch = "wasm32"))]
    let mut frame = 0;
    loop {
        clear_background(Color::new(0.08, 0.12, 0.17, 1.0));
        let mut world = World::new([4.3, 3.0, 5.2], [0.1, 0.75, 0.0]);
        world.cube(
            [0.0, -0.05, 0.0],
            [5.0, 0.1, 4.0],
            Color::new(0.27, 0.34, 0.32, 1.0),
        );
        for mesh in table.meshes(Mat4::IDENTITY).unwrap() {
            world.mesh(mesh);
        }
        for x in [-0.9, 0.9] {
            for z in [-0.95, 0.95] {
                let transform = Mat4::from_translation(vec3(x, 0.0, z))
                    * Mat4::from_rotation_y(if z > 0.0 { std::f32::consts::PI } else { 0.0 });
                for mesh in chair.meshes(transform).unwrap() {
                    world.mesh(mesh);
                }
            }
        }
        for mesh in lamp
            .meshes(Mat4::from_translation(vec3(1.65, 0.0, -0.65)))
            .unwrap()
        {
            world.mesh(mesh);
        }
        for mesh in textured
            .meshes(Mat4::from_scale_rotation_translation(
                Vec3::splat(0.3),
                Quat::IDENTITY,
                vec3(0.0, 0.816835, 0.0),
            ))
            .unwrap()
        {
            world.mesh(mesh);
        }
        let mut scene = Scene::default();
        scene.world(0, Rect::new(0, 0, 960, 640), world);
        scene.text(
            1,
            "CC0 imported furniture + base-color texture",
            Point::new(20, 35),
            24.0,
            WHITE,
        );
        scene
            .draw(
                Viewport::fit(960.0, 640.0, screen_width(), screen_height()),
                Point::new(0, 0),
                &mut renderer,
            )
            .unwrap();
        #[cfg(not(target_arch = "wasm32"))]
        {
            frame += 1;
            if let Some(dir) = &capture {
                if frames.contains(&frame) {
                    std::fs::create_dir_all(dir).unwrap();
                    get_screen_data()
                        .export_png(&dir.join(format!("frame-{frame:04}.png")).to_string_lossy());
                }
                if frame > *frames.iter().max().unwrap() + 2 {
                    break;
                }
            }
        }
        next_frame().await;
    }
}
