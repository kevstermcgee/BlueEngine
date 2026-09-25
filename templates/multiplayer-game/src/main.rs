//! Main entrypoint for the multiplayer game template.
//!
//! Run as client:
//!   cargo run
//!
//! Run as headless dedicated server:
//!   cargo run -- --server 0.0.0.0:4000 --key my-secret-key
mod client;
mod game;
mod lobby;
mod menu;
mod server;

use std::sync::{atomic::AtomicBool, Arc};

#[macroquad::main("Blue Engine - Multiplayer Game")]
async fn main() -> vesper3d::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut is_server = false;
    let mut server_addr = "0.0.0.0:4000".to_string();
    let mut join_key = "blue-engine-key".to_string();
    let mut connect_addr = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--server" => {
                is_server = true;
                if let Some(next) = args.get(i + 1) {
                    if !next.starts_with("--") {
                        server_addr = next.clone();
                        i += 1;
                    }
                }
            }
            "--connect" => {
                i += 1;
                if let Some(next) = args.get(i) {
                    connect_addr = Some(next.clone());
                }
            }
            "--key" => {
                i += 1;
                if let Some(next) = args.get(i) {
                    join_key = next.clone();
                }
            }
            "--help" => {
                println!(
                    "Multiplayer Game Template\n\
                     Usage:\n\
                       cargo run                         # Start GUI client\n\
                       cargo run -- --connect IP:PORT    # Direct join server\n\
                       cargo run -- --server [IP:PORT]   # Run headless dedicated server"
                );
                return Ok(());
            }
            _ => {}
        }
        i += 1;
    }

    if is_server {
        let stop_signal = Arc::new(AtomicBool::new(false));
        let mut dedicated = server::DedicatedServer::bind(&server_addr, join_key, 8)?;
        dedicated.run(stop_signal, None)?;
        return Ok(());
    }

    // Graphical game client
    let mut client = client::GameClient::new()?;
    if let Some(addr_str) = connect_addr {
        let addr = addr_str.parse()?;
        client.connect(addr, join_key)?;
    }

    loop {
        let quit = client.update()?;
        if quit {
            break;
        }
        macroquad::prelude::next_frame().await;
    }

    Ok(())
}
