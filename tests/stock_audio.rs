//! Stock audio observes authority; no window or device required for transition checks.
use std::path::{Path, PathBuf};
use vesper3d::viewer::{
    game::GameDocument,
    game_session::{GameInput, GameSession},
    scenario::{load_scenario, InputDriver},
    stock_audio::{AudioCursor, StockAudio},
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/games/observatory/content")
        .join(name)
}
fn config() -> StockAudio {
    serde_json::from_slice(&std::fs::read(fixture("audio-bindings.json")).unwrap()).unwrap()
}

#[test]
fn loss_restart_win_cues_follow_actual_authority_without_replaying_reset_counters() {
    let mut session =
        GameSession::local(GameDocument::load(&fixture("game-audio.json")).unwrap()).unwrap();
    let runtime = session.world().game.as_ref().unwrap();
    let mut cursor = AudioCursor::new(&config(), runtime.document(), runtime.state()).unwrap();
    assert!(cursor.observe(runtime.state()).is_empty());
    let scenario = load_scenario(&fixture("audio-loss-restart-win.json")).unwrap();
    let mut driver = InputDriver::new(&scenario.inputs);
    let mut events = Vec::new();
    for tick in 1..=scenario.ticks {
        session.advance_scenario(&mut driver, tick).unwrap();
        let state = session.world().game.as_ref().unwrap().state();
        events.extend(cursor.observe(state).into_iter().map(|c| (tick, c.cue)));
    }
    assert_eq!(
        events.len(),
        8,
        "no replayed reset counters or repeated threshold alarms"
    );
    assert_eq!(events.iter().filter(|(_, cue)| cue == "step").count(), 3);
    assert_eq!(events.iter().filter(|(_, cue)| cue == "success").count(), 1);
    assert!(events.contains(&(900, "battery".into())));
    assert!(events.contains(&(1200, "battery".into())));
    assert!(events.contains(&(1201, "shutter".into())));
    assert_eq!(events.last().unwrap(), &(1539, "success".into()));
    let state = session.world().game.as_ref().unwrap().state();
    assert!(cursor.observe(state).is_empty(), "no repeat on same state");
    assert!(
        cursor.music(state, true).values().all(|v| *v == 0.),
        "finished music fades out"
    );
}

#[test]
fn catch_up_observes_each_tick_and_does_not_change_simulation() {
    let mut game = GameDocument::load(&fixture("game.json")).unwrap();
    game.document.timers[0].duration_ticks = 1;
    let config: StockAudio = serde_json::from_value(serde_json::json!({"bundle":"audio", "cues":[{"cue":"battery","on":{"kind":"counter_changed","counter":"battery"}}]})).unwrap();
    let mut session = GameSession::local(game.clone()).unwrap();
    let mut plain = GameSession::local(game).unwrap();
    let g = session.world().game.as_ref().unwrap();
    let mut cursor = AudioCursor::new(&config, g.document(), g.state()).unwrap();
    let mut cues = Vec::new();
    let before = session.world().checksum();
    assert_eq!(session.advance(GameInput::default(), 0., false).unwrap(), 0);
    assert_eq!(
        before,
        session.world().checksum(),
        "startup polling does not step local authority"
    );
    assert_eq!(
        session
            .advance_observed(GameInput::default(), 0.14, true, |s| cues
                .extend(cursor.observe(s)))
            .unwrap(),
        8
    );
    plain.advance(GameInput::default(), 0.14, true).unwrap();
    assert_eq!(
        cues.len(),
        8,
        "each executed step, not just the rendered frame"
    );
    assert_eq!(session.world().checksum(), plain.world().checksum());
    session
        .advance_observed(GameInput::default(), 1., false, |s| {
            cues.extend(cursor.observe(s))
        })
        .unwrap();
    assert_eq!(cues.len(), 8, "local pause freezes authority");
}

#[test]
fn load_baseline_and_music_mapping_are_read_only_bounded_and_pause_aware() {
    let game = GameDocument::load(&fixture("game.json")).unwrap();
    let world = game.clone().world().unwrap();
    let g = world.game.as_ref().unwrap();
    let mut cursor = AudioCursor::new(&config(), &game.document, g.state()).unwrap();
    let mut state = g.state().clone();
    state.counters[1] = 2;
    assert!((cursor.music(&state, true)["signal"] - 0.85 * 2. / 3.).abs() < 1e-6);
    assert!(cursor.music(&state, false).values().all(|v| *v == 0.));
    state.counters[1] = 999;
    assert_eq!(cursor.music(&state, true)["signal"], 0.85);
    cursor.rebase(&state);
    assert!(
        cursor.observe(&state).is_empty(),
        "loading is not a new gameplay event"
    );
    state.round += 1;
    state.counters[1] = 0;
    assert_eq!(
        cursor.observe(&state).len(),
        1,
        "restart cue alone, not reset counters"
    );
}

#[test]
fn invalid_bindings_and_missing_names_are_explicit_errors() {
    let game = GameDocument::load(&fixture("game.json")).unwrap();
    for patch in [
        serde_json::json!({"bundle":"../audio"}),
        serde_json::json!({"bundle":"audio","cues":[{"cue":"battery","volume":2,"on":{"kind":"failed"}}]}),
        serde_json::json!({"bundle":"audio","music":{"orbit":{"counter":{"name":"unknown","from":0,"to":1}}}}),
    ] {
        let bad: StockAudio = serde_json::from_value(patch).unwrap();
        assert!(bad.validate(&game.document.counters).is_err());
    }
    assert!(
        serde_json::from_value::<StockAudio>(serde_json::json!({"bundle":"audio","cuez":[]}))
            .is_err()
    );
    let mut bank =
        vesper3d::viewer::devkit::audio_project::AudioBundle::load(&fixture("audio")).unwrap();
    config().validate_bank(&bank).unwrap();
    bank.effects.remove("success");
    assert!(config()
        .validate_bank(&bank)
        .unwrap_err()
        .to_string()
        .contains("success"));
}

#[test]
fn loaded_audio_paths_are_cwd_independent_and_headless_needs_no_wavs() {
    let game = GameDocument::load(&fixture("game-audio.json")).unwrap();
    let audio = game
        .document
        .presentation
        .as_ref()
        .unwrap()
        .audio
        .as_ref()
        .unwrap();
    assert_eq!(
        audio.bundle_path(None).unwrap(),
        fixture("audio").canonicalize().unwrap()
    );
    let json = serde_json::to_string(audio).unwrap();
    assert!(
        !json.contains("root"),
        "local paths never enter replicated content identity"
    );
    let constructed: StockAudio = serde_json::from_str(&json).unwrap();
    assert!(constructed
        .bundle_path(None)
        .unwrap_err()
        .to_string()
        .contains("audio_root"));
    assert_eq!(
        constructed.bundle_path(Some(&fixture("."))).unwrap(),
        audio.bundle_path(None).unwrap()
    );
    let mut no_assets = game.clone();
    no_assets
        .document
        .presentation
        .as_mut()
        .unwrap()
        .audio
        .as_mut()
        .unwrap()
        .bundle = "does-not-exist".into();
    assert!(
        no_assets.world().is_ok(),
        "headless authority doesn't load audio assets"
    );
}

#[cfg(feature = "client")]
#[test]
fn stock_cli_exposes_mute_settings_and_constructed_content_root() {
    let args = [
        "--mute",
        "--settings",
        "custom/settings.json",
        "--audio-root",
        "content",
    ]
    .map(String::from);
    let options = vesper3d::viewer::playable::GameOptions::from_args(&args).unwrap();
    assert!(options.mute);
    assert_eq!(options.audio_root.unwrap(), Path::new("content"));
    assert_eq!(
        options.settings_path.unwrap(),
        Path::new("custom/settings.json")
    );
    assert!(vesper3d::viewer::playable::GameOptions::from_args(&["--settings".into()]).is_err());
}

#[cfg(unix)]
#[test]
fn bundle_directory_symlinks_cannot_escape_the_content_root() {
    let temp = std::env::temp_dir().join(format!(
        "stock-audio-path-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = temp.join("game");
    let outside = temp.join("outside");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("escaped")).unwrap();
    let mut audio = config();
    audio.bundle = "escaped".into();
    assert!(audio
        .bundle_path(Some(&root))
        .unwrap_err()
        .to_string()
        .contains("escapes"));
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn online_audio_waits_for_first_accepted_authority_and_rebases_on_reconnect() {
    use std::{cell::RefCell, rc::Rc};
    use vesper3d::viewer::net::{Datagram, DatagramTransport, Packet};
    struct Queue(Rc<RefCell<Vec<Datagram>>>);
    impl DatagramTransport for Queue {
        fn send(&self, _: std::net::SocketAddr, data: &[u8]) -> vesper3d::Result<usize> {
            Ok(data.len())
        }
        fn receive(&mut self) -> vesper3d::Result<Vec<Datagram>> {
            Ok(std::mem::take(&mut *self.0.borrow_mut()))
        }
        fn local_addr(&self) -> vesper3d::Result<std::net::SocketAddr> {
            Ok("127.0.0.1:11111".parse().unwrap())
        }
    }
    let address = "127.0.0.1:11112".parse().unwrap();
    // A newly connected session, including reconnection, must not replay prior counters/outcomes.
    for finished in [false, true] {
        for _connection in 0..2 {
            let queue = Rc::new(RefCell::new(Vec::new()));
            let mut client = GameSession::connect(
                GameDocument::load(&fixture("game.json")).unwrap(),
                Box::new(Queue(queue.clone())),
                address,
                None,
            )
            .unwrap();
            let runtime = client.world().game.as_ref().unwrap();
            let mut cursor =
                AudioCursor::new(&config(), runtime.document(), runtime.state()).unwrap();
            let mut state = runtime.state().clone();
            state.counters[1] = 2;
            state.completed = finished;
            let enqueue = |packet: Packet| {
                queue.borrow_mut().push(Datagram {
                    peer: address,
                    data: packet.encode().unwrap(),
                })
            };
            let mut cues = Vec::new();
            let mut observe = |s: &vesper3d::viewer::game::GameState, baseline: bool| {
                if baseline {
                    cursor.rebase(s)
                } else {
                    cues.extend(cursor.observe(s))
                }
            };
            // Welcome and GameState may arrive on different rendered frames.
            enqueue(Packet::Welcome {
                player_id: 1,
                server_tick: 100,
                map_name: "test".into(),
                session_token: Some([7, 8]),
            });
            client
                .advance_observed_with_baseline(GameInput::default(), 0., false, &mut observe)
                .unwrap();
            enqueue(Packet::GameState {
                session: Some([1, 2]),
                tick: 999,
                state: state.clone(),
            });
            client
                .advance_observed_with_baseline(GameInput::default(), 0., false, &mut observe)
                .unwrap();
            enqueue(Packet::GameState {
                session: Some([7, 8]),
                tick: 101,
                state: state.clone(),
            });
            client
                .advance_observed_with_baseline(GameInput::default(), 0., false, &mut observe)
                .unwrap();
            assert!(
                cues.is_empty(),
                "joining must not replay historical counters or completion"
            );
            let baseline_checksum = client.world().checksum();
            client.advance(GameInput::default(), 0., false).unwrap();
            assert_eq!(
                baseline_checksum,
                client.world().checksum(),
                "audio polling is read-only"
            );
            state.counters[1] = 3;
            enqueue(Packet::GameState {
                session: Some([7, 8]),
                tick: 102,
                state: state.clone(),
            });
            // Multiple accepted transitions in one poll must not collapse into a net-zero change.
            state.counters[1] = 2;
            enqueue(Packet::GameState {
                session: Some([7, 8]),
                tick: 103,
                state: state.clone(),
            });
            let mut observe = |s: &vesper3d::viewer::game::GameState, baseline: bool| {
                assert!(!baseline);
                cues.extend(cursor.observe(s));
            };
            client
                .advance_observed_with_baseline(GameInput::default(), 0., false, &mut observe)
                .unwrap();
            assert_eq!(
                cues.iter().map(|c| c.cue.as_str()).collect::<Vec<_>>(),
                vec!["step", "step"]
            );
            assert_eq!(client.world().game.as_ref().unwrap().state(), &state);
        }
    }
}
