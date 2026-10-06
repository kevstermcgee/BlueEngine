use vesper3d::portable::client;
fn config() -> macroquad::conf::Conf {
    let mut config = client::config("Leo");
    // Original cached kit batches require the same capacities as the native client.
    config.draw_call_vertex_capacity = 30000;
    config.draw_call_index_capacity = 30000;
    config.miniquad_conf.icon = Some(macroquad::miniquad::conf::Icon {
        small: *include_bytes!("../assets/icon_16.rgba"),
        medium: *include_bytes!("../assets/icon_32.rgba"),
        big: *include_bytes!("../assets/icon_64.rgba"),
    });
    config
}
#[macroquad::main(config)]
async fn main() {
    client::run::<leo::browser::Walker>().await;
}
