//! Resource evidence and failed hardware startup are separate from audible playback.
#![cfg(any(feature = "presentation", feature = "two-d"))]
#[cfg(target_os = "linux")]
#[test]
fn missing_audio_device_is_reported_without_panicking_gameplay() {
    if std::env::var_os("BE2_TEST_AUDIO_CHILD").is_none() {
        let missing = std::env::temp_dir().join(format!("be2-missing-alsa-{}", std::process::id()));
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "missing_audio_device_is_reported_without_panicking_gameplay",
                "--nocapture",
            ])
            .env("BE2_TEST_AUDIO_CHILD", "1")
            .env("ALSA_CONFIG_PATH", missing)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("AUDIO-BACKEND") && stderr.contains("ALSA"),
            "{stderr}"
        );
        assert!(!stderr.contains("panicked"), "{stderr}");
        return;
    }
    use std::{
        future::Future,
        pin::pin,
        task::{Context, Poll, Waker},
        time::{Duration, Instant},
    };
    use vesper3d::viewer::{
        audio_backend,
        devkit::synth,
        kit::{AudioState, Rendered, SoundBank},
    };
    fn ready<F: Future>(future: F) -> F::Output {
        let mut future = pin!(future);
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("native audio submission unexpectedly suspended"),
        }
    }
    assert!(ready(audio_backend::load_sound_from_bytes(b"bad resource")).is_err());
    assert_eq!(
        audio_backend::state(),
        audio_backend::BackendState::NotStarted
    );
    let mut bank = ready(SoundBank::start_checked(false, 1., 1., || {
        Ok(Rendered {
            sfx: vec![vec![synth::wav_bytes(&[0.; 100], synth::RATE)]],
            stems: vec![],
        })
    }));
    let started = Instant::now();
    while !matches!(
        bank.backend_state(),
        audio_backend::BackendState::Unavailable(_)
    ) {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "backend failed to report missing device"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    ready(bank.poll());
    bank.play(0, 1.);
    assert_eq!(bank.state(), AudioState::Failed);
    assert!(!bank.ready());
    assert!(!bank.errors().is_empty());
    assert!(!bank.resource_failed());
    assert_eq!(bank.status().load_failures, 0);
    assert!(bank.status().worker_failed);
    assert_eq!(bank.status().effect_plays, 0);
    assert!(bank.status().verify_playback(1, 0).is_err());
    let mut broken = ready(SoundBank::start_checked(false, 1., 1., || {
        Ok(Rendered {
            sfx: vec![vec![b"broken resource".to_vec()]],
            stems: vec![],
        })
    }));
    while broken.status().pending {
        assert!(started.elapsed() < Duration::from_secs(5));
        ready(broken.poll());
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        broken.resource_failed(),
        "unavailable hardware must not hide invalid assets"
    );
    assert_eq!(broken.status().load_failures, 1);
}
