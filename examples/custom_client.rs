//! Playable static-map starter using shared input, camera, avatars and menus.
#[path = "../templates/native_focus.rs"]
mod platform;
use vesper3d::prelude::*;
use vesper3d::viewer::{game_client, local_client};
fn window() -> macroquad::conf::Conf {
    game_client::window_config("BlueEngine custom client")
}
#[macroquad::main(window)]
async fn main() -> Result<()> {
    let map = SceneBuilder::new("Custom client")
        .spawn(V(0., 0., 4.6), 0.)
        .structural_box("floor", V(0., -0.1, 0.), V(8., 0.1, 8.), V(0.25, 0.3, 0.4))
        .prop("ball", "apple", V(2., 0.2, 0.))
        .build()?;
    local_client::run_map_with_focus(map, platform::focused).await
}
