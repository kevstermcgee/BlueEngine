use signal_garden::audio;
#[test]
fn every_game_cue_and_music_are_real_nonempty_pcm_wavs() {
    let effects = audio::effects();
    let music = audio::music();
    let out = std::env::var_os("GARDEN_AUDIO_OUT").map(std::path::PathBuf::from);
    if let Some(out) = &out {
        std::fs::create_dir_all(out).unwrap();
    }
    for (name, bytes) in effects
        .iter()
        .enumerate()
        .flat_map(|(i, vs)| vs.iter().enumerate().map(move |(v, b)| (format!("effect-{i}-{v}"), b)))
        .chain(std::iter::once(("ambience".into(), &music)))
    {
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert!(bytes.len() > 1000);
        assert!(bytes[44..].iter().any(|&v| v != 0), "{name} cannot be silent");
        if let Some(out) = &out {
            std::fs::write(out.join(format!("{name}.wav")), bytes).unwrap();
        }
    }
}
