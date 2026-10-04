//! Native preview of a named audio bundle: no game or recompilation needed for audio edits.
//! `cargo run --profile fast --example audio_preview -- BUNDLE [--smoke CAPTURE.png]`.
use macroquad::prelude::*;
use std::path::Path;
use vesper3d::viewer::{
    devkit::audio_project::AudioBundle,
    kit::{audio::AudioBank, AudioState},
};

fn window() -> Conf {
    Conf {
        window_title: "BlueEngine Audio Preview".into(),
        window_width: 960,
        window_height: 600,
        ..Default::default()
    }
}
#[macroquad::main(window)]
async fn main() {
    if let Err(error) = preview().await {
        eprintln!(
            "{}",
            serde_json::json!({"ok":false,"error":error.to_string()})
        );
        std::process::exit(1);
    }
}

async fn preview() -> vesper3d::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let root = args
        .first()
        .ok_or("Expected BUNDLE_DIRECTORY [--smoke CAPTURE.png]")?;
    let capture = args
        .iter()
        .position(|a| a == "--smoke")
        .map(|i| args.get(i + 1).ok_or("--smoke requires capture path"))
        .transpose()?;
    let metadata = AudioBundle::load(Path::new(root))?;
    let effects: Vec<_> = metadata.effects.keys().cloned().collect();
    let layers: Vec<_> = metadata.music.keys().cloned().collect();
    let mut bank = AudioBank::load(root, false, 0.7, 0.5).await?;
    let mut level = 0.4f32;
    let mut clock = vesper3d::viewer::devkit::FrameClock::new();
    let mut ready_frames = 0;
    let mut last = "Waiting for verified audio".to_string();
    let started = std::time::Instant::now();
    let keys = [
        KeyCode::Key1,
        KeyCode::Key2,
        KeyCode::Key3,
        KeyCode::Key4,
        KeyCode::Key5,
        KeyCode::Key6,
        KeyCode::Key7,
        KeyCode::Key8,
        KeyCode::Key9,
    ];
    loop {
        if capture.is_some() && started.elapsed().as_secs() > 30 {
            return Err("audio preview timed out".into());
        }
        bank.sounds.poll().await;
        if bank.sounds.state() == AudioState::Failed {
            return Err(format!("audio failed: {:?}", bank.sounds.errors()).into());
        }
        if bank.sounds.ready() {
            ready_frames += 1;
            if ready_frames == 1 {
                println!(
                    "{}",
                    serde_json::json!({"event":"assets_ready","effects":effects,"layers":layers,"audibility_verified":false})
                );
            }
            if is_key_pressed(KeyCode::Up) {
                level = (level + 0.1).min(1.);
            }
            if is_key_pressed(KeyCode::Down) {
                level = (level - 0.1).max(0.);
            }
            if capture.is_some() {
                level = (ready_frames as f32 / 180.).min(1.);
            }
            let levels: Vec<_> = layers
                .iter()
                .enumerate()
                .map(|(i, n)| (n.as_str(), if i == 0 { 0.65 } else { level }))
                .collect();
            bank.music(clock.tick(), &levels)?;
            for (i, cue) in effects.iter().enumerate().take(keys.len()) {
                if is_key_pressed(keys[i]) || (capture.is_some() && ready_frames == 20 + i * 30) {
                    bank.play(cue, 1.)?;
                    last = format!("Submitted cue: {cue}");
                    println!(
                        "{}",
                        serde_json::json!({"event":"cue_submitted","cue":cue,"ready_frame":ready_frames})
                    );
                }
            }
        }
        if is_key_pressed(KeyCode::Escape) || is_quit_requested() {
            bank.sounds.stop_music();
            break;
        }
        clear_background(Color::from_rgba(13, 19, 33, 255));
        draw_text(
            "OBSERVATORY / AUDIO LAB",
            36.,
            62.,
            32.,
            Color::from_rgba(138, 220, 232, 255),
        );
        draw_text(
            &format!("Asset state: {:?}", bank.sounds.state()),
            36.,
            106.,
            24.,
            WHITE,
        );
        draw_text(
            "1-9: cue    Up/Down: adaptive layer level    Escape: close",
            36.,
            146.,
            21.,
            GRAY,
        );
        for (i, cue) in effects.iter().enumerate().take(9) {
            let f = &metadata.effects[cue][0];
            draw_text(
                &format!(
                    "{}  {}    {} variants    peak {:.3}",
                    i + 1,
                    cue,
                    metadata.effects[cue].len(),
                    f.peak
                ),
                48.,
                198. + i as f32 * 34.,
                23.,
                WHITE,
            );
        }
        let y = 226. + effects.len().min(9) as f32 * 34.;
        for (i, layer) in layers.iter().enumerate().take(3) {
            let value = if i == 0 { 0.65 } else { level };
            draw_text(layer, 48., y + i as f32 * 39., 23., WHITE);
            draw_rectangle(
                210.,
                y - 17. + i as f32 * 39.,
                600.,
                12.,
                Color::from_rgba(32, 46, 64, 255),
            );
            draw_rectangle(
                210.,
                y - 17. + i as f32 * 39.,
                value * 600.,
                12.,
                Color::from_rgba(88, 182, 192, 255),
            );
        }
        draw_text(&last, 36., screen_height() - 68., 22., WHITE);
        draw_text(
            "Ready = checked assets; physical device audibility is not reported.",
            36.,
            screen_height() - 32.,
            19.,
            GRAY,
        );
        if let Some(path) = capture.filter(|_| ready_frames == 180) {
            let size = vesper3d::viewer::kit::capture::save_frame(Path::new(path))?;
            println!(
                "{}",
                serde_json::json!({"event":"captured","width":size.0,"height":size.1,"frames":ready_frames})
            );
        }
        if capture.is_some() && ready_frames >= 240 {
            bank.sounds.stop_music();
            break;
        }
        next_frame().await;
    }
    Ok(())
}
