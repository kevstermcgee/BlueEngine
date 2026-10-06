use macroquad::prelude::*;
use vesper3d::portable::{
    draw::{Renderer, Scene, Viewport, World},
    Point, Rect,
};
#[path = "cases/after-1.rs"]
mod after_1;
#[path = "cases/after-2.rs"]
mod after_2;
#[path = "cases/after-3.rs"]
mod after_3;
#[path = "cases/before-1.rs"]
mod before_1;
#[path = "cases/before-2.rs"]
mod before_2;
#[path = "cases/before-3.rs"]
mod before_3;
#[path = "cases/final-1.rs"]
mod final_1;
#[path = "cases/final-2.rs"]
mod final_2;
#[path = "cases/final-3.rs"]
mod final_3;
#[macroquad::main("CC0 furniture comparison")]
async fn main() {
    let args: Vec<_> = std::env::args().collect();
    let case = args[1].as_str();
    let out = &args[2];
    let mut renderer = Renderer::default();
    for frame in 0..8 {
        clear_background(Color::new(0.08, 0.12, 0.17, 1.));
        let mut world = World::new([4.3, 3., 5.2], [0.1, 0.75, 0.]);
        world.cube(
            [0., -0.05, 0.],
            [5., 0.1, 4.],
            Color::new(0.27, 0.34, 0.32, 1.),
        );
        match case {
            "before-1" => before_1::furnish(&mut world),
            "before-2" => before_2::furnish(&mut world),
            "before-3" => before_3::furnish(&mut world),
            "after-1" => after_1::furnish(&mut world),
            "after-2" => after_2::furnish(&mut world),
            "after-3" => after_3::furnish(&mut world),
            "final-1" => final_1::furnish(&mut world),
            "final-2" => final_2::furnish(&mut world),
            "final-3" => final_3::furnish(&mut world),
            _ => panic!("unknown case"),
        };
        let mut scene = Scene::default();
        scene.world(0, Rect::new(0, 0, 960, 640), world);
        scene.text(
            1,
            "Furnished scene benchmark",
            Point::new(20, 35),
            24.,
            WHITE,
        );
        scene
            .draw(
                Viewport::fit(960., 640., screen_width(), screen_height()),
                Point::new(0, 0),
                &mut renderer,
            )
            .unwrap();
        if frame == 5 {
            get_screen_data().export_png(out);
        }
        next_frame().await;
    }
}
