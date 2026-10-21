use vesper3d::two_d::{client, GameLogic};
fn config() -> macroquad::conf::Conf {
    let identity =
        vesper3d::viewer::identity::Identity::parse(include_str!("../assets/identity.json"))
            .unwrap();
    let mut config = client::config(&identity.title);
    if !std::env::args().any(|a| a == "--size") {
        config.miniquad_conf.fullscreen = std::env::args().any(|a| a == "--fullscreen");
        config.miniquad_conf.window_width = 460;
        config.miniquad_conf.window_height = 960;
    }
    config.miniquad_conf.icon = Some(macroquad::miniquad::conf::Icon {
        small: *include_bytes!("../assets/icon_16.rgba"),
        medium: *include_bytes!("../assets/icon_32.rgba"),
        big: *include_bytes!("../assets/icon_64.rgba"),
    });
    assert_eq!(
        identity.title,
        tetris::Tetris::TITLE,
        "Game title and identity disagree"
    );
    config
}
#[macroquad::main(config)]
async fn main() {
    client::run::<tetris::Tetris>().await;
}
