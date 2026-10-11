#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use vesper3d::two_d::{client, GameLogic};
fn config() -> macroquad::conf::Conf {
    let identity =
        vesper3d::viewer::identity::Identity::parse(include_str!("../assets/identity.json"))
            .unwrap();
    let mut config = client::config(&identity.title);
    config.miniquad_conf.icon = Some(macroquad::miniquad::conf::Icon {
        small: *include_bytes!("../assets/icon_16.rgba"),
        medium: *include_bytes!("../assets/icon_32.rgba"),
        big: *include_bytes!("../assets/icon_64.rgba"),
    });
    assert_eq!(
        identity.title,
        wormhole::Wormhole::TITLE,
        "Game title and identity disagree"
    );
    config
}
#[macroquad::main(config)]
async fn main() {
    macroquad::input::show_mouse(false);
    let assets = vesper3d::viewer::devkit::runtime_assets(
        "assets",
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")),
    )
    .unwrap_or_else(|e| {
        eprintln!("Wormhole: {e}");
        std::process::exit(1)
    });
    macroquad::file::set_pc_assets_folder(
        assets
            .parent()
            .expect("asset parent")
            .to_str()
            .expect("UTF-8 asset path"),
    );
    client::run::<wormhole::Wormhole>().await;
}
