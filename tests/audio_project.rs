//! Observable audio-authoring contracts: strict failure, stereo pitch/timing, safe adaptive layers,
//! repeatable rendering and corruption detection, using the same renderer the command line uses.
use std::{fs, path::PathBuf};
use vesper3d::viewer::devkit::{
    audio_project::{AudioBundle, AudioProject, EffectSource, Music},
    synth,
};

fn project() -> AudioProject {
    AudioProject::load(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/audio/observatory/project.json"),
    )
    .unwrap()
}
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "be2-audio-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn named_bank_roundtrips_and_never_overwrites_completed_output() {
    let t = Temp::new();
    let p = project();
    let rendered = p.render(&t.0).unwrap();
    let out = t.0.join("bank");
    rendered.write_new(&out).unwrap();
    let bank = AudioBundle::load(&out).unwrap();
    let audio = bank.read_audio(&out).unwrap();
    assert_eq!(bank.effects["step"].len(), 4);
    assert_eq!(audio.effects.len(), 4);
    assert_eq!(audio.music.len(), 2);
    assert!(rendered.write_new(&out).is_err());
    assert_eq!(
        fs::read(out.join("bank.json")).unwrap(),
        serde_json::to_vec_pretty(&rendered.manifest).unwrap()
    );
    assert_eq!(
        rendered.files,
        p.render(&t.0).unwrap().files,
        "same platform/spec produces identical assets"
    );
    let file = out.join(&bank.music["orbit"].file);
    let mut bytes = fs::read(&file).unwrap();
    bytes[50] ^= 4;
    fs::write(file, bytes).unwrap();
    assert!(bank.read_audio(&out).unwrap_err_text().contains("checksum"));
}
// Payload intentionally is not Debug, so use a tiny helper instead of unwrap_err's Debug bound.
trait ErrorText {
    fn unwrap_err_text(self) -> String;
}
impl<T> ErrorText for Result<T, String> {
    fn unwrap_err_text(self) -> String {
        match self {
            Err(e) => e,
            Ok(_) => panic!("expected audio failure"),
        }
    }
}

#[test]
fn adaptive_subsets_cannot_clip_and_tails_are_present_at_loop_start() {
    let t = Temp::new();
    let p = project();
    let r = p.render(&t.0).unwrap();
    let layers: Vec<_> = r
        .manifest
        .music
        .values()
        .map(|f| synth::parse_wav(&r.files[&f.file]).unwrap().2)
        .collect();
    assert_eq!(layers[0].len(), layers[1].len());
    for mask in 1..(1usize << layers.len()) {
        let mixed: Vec<f32> = (0..layers[0].len())
            .map(|i| {
                layers
                    .iter()
                    .enumerate()
                    .filter(|(n, _)| mask & (1 << n) != 0)
                    .map(|(_, s)| s[i])
                    .sum()
            })
            .collect();
        assert!(synth::peak(&mixed) <= p.headroom + 0.0001);
    }
    let orbit = &r.manifest.music["orbit"];
    let data = synth::parse_wav(&r.files[&orbit.file]).unwrap().2;
    assert!(
        data[0].abs() > 0.0001,
        "late released notes wrap into the start"
    );
    assert!(orbit.seam_jump < 0.01, "no artificial loop fade or jump");
    assert!(r.manifest.music.values().all(|f| f.dc < 0.02));
}

#[test]
fn authored_notes_have_the_selected_pitch_pan_and_full_release() {
    let mut p = project();
    p.music = None;
    p.effects.retain(|n, _| n == "battery");
    let EffectSource::Score { score } = &mut p.effects.get_mut("battery").unwrap().source else {
        panic!()
    };
    score.beats = 1.;
    score.layers[0].notes.truncate(1);
    let note = &mut score.layers[0].notes[0];
    note.beats = 1.;
    note.midi = 69;
    note.pan = -1.;
    let r = p.render(std::path::Path::new(".")).unwrap();
    let f = &r.manifest.effects["battery"][0];
    let (_, _, s) = synth::parse_wav(&r.files[&f.file]).unwrap();
    let left: Vec<_> = s.iter().step_by(2).copied().collect();
    let right: Vec<_> = s.iter().skip(1).step_by(2).copied().collect();
    assert!(synth::rms(&right) < 0.00001);
    assert!((synth::dominant_hz(&left, 400., 480.) - 440.).abs() < 5.);
    assert!(
        f.frames > 22050,
        "the release after the last beat is not truncated"
    );
    assert_eq!(left.last(), Some(&0.));
}

#[test]
fn bad_schema_names_ranges_and_resource_budgets_fail_before_render() {
    assert!(serde_json::from_str::<AudioProject>(r#"{"version":1,"efects":{}}"#).is_err());
    let mut p = project();
    p.effects.insert("../bad".into(), p.effects["step"].clone());
    assert!(p.validate().unwrap_err().contains("name"));
    let mut p = project();
    let Music::Score { score } = p.music.as_mut().unwrap() else {
        panic!()
    };
    score.layers[0].notes[0].pan = f32::NAN;
    assert!(p.validate().is_err());
    let mut p = project();
    let Music::Score { score } = p.music.as_mut().unwrap() else {
        panic!()
    };
    score.layers[0].notes[0].at = score.beats;
    assert!(p.validate().unwrap_err().contains("beyond"));
    let mut p = project();
    let Music::Score { score } = p.music.as_mut().unwrap() else {
        panic!()
    };
    score.beats = 700.;
    assert!(p.validate().unwrap_err().contains("duration"));
}

#[test]
fn imports_reject_truncation_wrong_rate_missing_files_and_traversal() {
    let t = Temp::new();
    let mut p = project();
    p.music = None;
    p.effects.retain(|n, _| n == "step");
    p.effects.get_mut("step").unwrap().source = EffectSource::Wav {
        file: "input.wav".into(),
    };
    assert!(p.render(&t.0).unwrap_err_text().contains("input.wav"));
    let samples = synth::render(synth::Preset::Click, 0, 1);
    fs::write(t.0.join("input.wav"), synth::wav_bytes(&samples, 22050)).unwrap();
    assert!(p.render(&t.0).is_err());
    let mut bytes = synth::wav_bytes(&samples, synth::RATE);
    let mut oversized = bytes.clone();
    oversized[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(vesper3d::viewer::devkit::audio_project::checked_wav(&oversized).is_err());
    bytes.pop();
    fs::write(t.0.join("input.wav"), bytes).unwrap();
    assert!(p.render(&t.0).is_err());
    fs::write(
        t.0.join("input.wav"),
        synth::wav_bytes(&samples, synth::RATE),
    )
    .unwrap();
    assert!(p.render(&t.0).is_ok());
    p.effects.get_mut("step").unwrap().source = EffectSource::Wav {
        file: "../input.wav".into(),
    };
    assert!(p.validate().unwrap_err().contains("unsafe"));
}

#[test]
fn generated_music_and_music_only_banks_have_equal_measured_layers() {
    let t = Temp::new();
    let mut p = project();
    p.effects.clear();
    p.music = Some(Music::Generated {
        bpm: 240.,
        bars: 1,
        root_midi: 57,
        minor: true,
    });
    let r = p.render(&t.0).unwrap();
    assert_eq!(r.manifest.music.len(), 3);
    assert!(r
        .manifest
        .music
        .values()
        .all(|f| f.frames == synth::RATE as usize));
    r.write_new(&t.0.join("music")).unwrap();
    let mut bank = AudioBundle::load(&t.0.join("music")).unwrap();
    bank.music.get_mut("lead").unwrap().frames += 1;
    fs::write(
        t.0.join("music/bank.json"),
        serde_json::to_vec(&bank).unwrap(),
    )
    .unwrap();
    assert!(AudioBundle::load(&t.0.join("music"))
        .unwrap_err()
        .contains("unequal"));
}

#[test]
fn cli_describes_renders_checks_and_rejects_unknown_arguments() {
    let t = Temp::new();
    let binary = env!("CARGO_BIN_EXE_be2-tools");
    let run = |args: &[&std::ffi::OsStr]| {
        std::process::Command::new(binary)
            .args(args)
            .output()
            .unwrap()
    };
    let describe = run(&["audio".as_ref(), "describe".as_ref()]);
    assert!(describe.status.success());
    let description: serde_json::Value = serde_json::from_slice(&describe.stdout).unwrap();
    assert_eq!(description["version"], 1);
    assert!(description["presets"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "footstep"));
    assert!(
        !run(&["audio".as_ref(), "describe".as_ref(), "typo".as_ref()])
            .status
            .success()
    );
    let input = t.0.join("project.json");
    fs::write(&input, serde_json::to_vec(&project()).unwrap()).unwrap();
    let bundle = t.0.join("bank");
    assert!(run(&[
        "audio".as_ref(),
        "render".as_ref(),
        input.as_os_str(),
        bundle.as_os_str()
    ])
    .status
    .success());
    assert!(
        run(&["audio".as_ref(), "check".as_ref(), bundle.as_os_str()])
            .status
            .success()
    );
    fs::remove_file(bundle.join("music-signal.wav")).unwrap();
    assert!(
        !run(&["audio".as_ref(), "check".as_ref(), bundle.as_os_str()])
            .status
            .success()
    );
}
