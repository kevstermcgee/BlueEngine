//! The exact WAVs passed to SoundBank. The headless export tests these same bytes.
use vesper3d::viewer::devkit::synth::{self, Preset};
pub const SOUNDS: [Preset; 8] = [
    Preset::Coin,
    Preset::Chime,
    Preset::Success,
    Preset::Hurt,
    Preset::PowerUp,
    Preset::Success,
    Preset::GameOver,
    Preset::PowerUp,
];
pub fn effects() -> Vec<Vec<Vec<u8>>> {
    SOUNDS
        .iter()
        .map(|p| (0..p.variants()).map(|v| synth::wav_bytes(&synth::render(*p, v, 7), synth::RATE)).collect())
        .collect()
}
pub fn music() -> Vec<u8> {
    let id: serde_json::Value = serde_json::from_str(include_str!("../assets/identity.json")).unwrap();
    synth::wav_bytes(
        &synth::ambient_loop(&synth::ambient_spec_for(id["title"].as_str().unwrap(), id["tagline"].as_str().unwrap())),
        synth::RATE,
    )
}
