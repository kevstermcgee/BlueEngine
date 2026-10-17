mod native;
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
        afterpull::Afterpull::TITLE,
        "Game title and identity disagree"
    );
    #[cfg(target_os = "linux")]
    {
        // Identify this process's X window before restoring the display title.
        config.miniquad_conf.window_title = native::window_title();
        config.miniquad_conf.platform.linux_backend =
            macroquad::miniquad::conf::LinuxBackend::X11Only;
    }
    config
}
#[macroquad::main(config)]
async fn main() {
    client::run::<native::NativeGame>().await;
}
