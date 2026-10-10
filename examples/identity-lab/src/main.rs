use vesper3d::two_d::client;
mod platform;
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

    config
}
#[macroquad::main(config)]
async fn main() {
    platform::attach_console();
    let args: Vec<_> = std::env::args().collect();
    let style = vesper3d::runtime::playback::flag_value(&args, "--identity").unwrap_or("notebook");
    let assets = vesper3d::viewer::devkit::runtime_assets(
        "assets",
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")),
    )
    .unwrap();
    std::env::set_current_dir(assets.parent().unwrap()).unwrap();
    match style {
        "notebook" => client::run_with_focus::<identity_lab::Lab<0>>(platform::focused).await,
        "instrument" => client::run_with_focus::<identity_lab::Lab<1>>(platform::focused).await,
        "arcade" => client::run_with_focus::<identity_lab::Lab<2>>(platform::focused).await,
        _ => {
            eprintln!("--identity must be notebook, instrument or arcade");
            std::process::exit(2);
        }
    }
}
