//! The local play session (`GameSession`, what F5 / F9 drive in the stock client and generated games):
//! saving is refused online, a load resumes exactly, and a bad load leaves the running game alone.
use std::path::PathBuf;
use vesper3d::viewer::{
    controller::Movement,
    game::{GameDocument, LoadedGame},
    game_session::{GameInput, GameSession},
    net::UdpTransport,
    newgame::scaffold_new_game,
    savestate::{SaveError, SaveSlots, Source},
    server::DedicatedServer,
};

struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "blue-save-session-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        scaffold_new_game(name, &dir, Some(env!("CARGO_MANIFEST_DIR"))).unwrap();
        Self(dir)
    }
    fn load(&self) -> LoadedGame {
        GameDocument::load(&self.0.join("game.json")).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One 60 Hz frame of a scripted player: walk, turn, jump now and then, press the action once.
fn frame(session: &mut GameSession, n: u32) {
    let input = GameInput {
        movement: Movement {
            forward: 1.,
            right: if n % 90 < 45 { 0.5 } else { -0.5 },
            jump: n.is_multiple_of(50),
            ..Default::default()
        },
        look: [if n % 120 < 60 { 4. } else { -4. }, 0.],
        interact: n == 7,
    };
    session.advance(input, 1. / 60., true).unwrap();
}

/// Everything a player can see or feel of the session, as exact bits.
fn fingerprint(session: &GameSession) -> (u64, u64, [u32; 3], [u32; 2], Vec<u32>) {
    let world = session.world();
    let c = session.controller();
    let apple = world.prop_position("apple").unwrap();
    (
        world.tick,
        world.checksum(),
        [
            c.position.0.to_bits(),
            c.position.1.to_bits(),
            c.position.2.to_bits(),
        ],
        [c.yaw.to_bits(), c.pitch.to_bits()],
        vec![apple.0.to_bits(), apple.1.to_bits(), apple.2.to_bits()],
    )
}

#[test]
fn loading_a_quick_save_resumes_the_game_exactly_where_it_was_saved() {
    let fixture = Fixture::new("resume");
    let mut session = GameSession::local(fixture.load()).unwrap();
    // Save while the apple is still falling: that state resumes bit for bit.
    for n in 0..9 {
        frame(&mut session, n);
    }
    let saved = fingerprint(&session);
    let bytes = session.save_bytes("quick save").unwrap();
    let mut played = Vec::new();
    for n in 9..200 {
        frame(&mut session, n);
        played.push(fingerprint(&session));
    }
    assert_ne!(
        saved,
        *played.last().unwrap(),
        "the player must have moved on after the save"
    );
    let header = session.restore_bytes(&bytes).unwrap();
    assert_eq!(
        (header.label.as_str(), header.tick),
        ("quick save", saved.0)
    );
    assert_eq!(
        fingerprint(&session),
        saved,
        "a load returns to the saved moment"
    );
    for (n, expected) in (9..200).zip(&played) {
        frame(&mut session, n);
        assert_eq!(
            &fingerprint(&session),
            expected,
            "replay after a load diverged at frame {n}"
        );
    }
    // Loading into a brand new session of the same game gives the same continuation.
    let mut other = GameSession::local(fixture.load()).unwrap();
    other.restore_bytes(&bytes).unwrap();
    assert_eq!(fingerprint(&other), saved);
    for (n, expected) in (9..200).zip(&played) {
        frame(&mut other, n);
        assert_eq!(
            &fingerprint(&other),
            expected,
            "fresh-session replay diverged at frame {n}"
        );
    }
}

#[test]
fn a_load_drops_input_that_was_pending_and_forgets_partial_frames() {
    let fixture = Fixture::new("pending");
    let mut session = GameSession::local(fixture.load()).unwrap();
    for n in 0..20 {
        frame(&mut session, n);
    }
    let bytes = session.save_bytes("pending").unwrap();
    let saved = fingerprint(&session);
    // A half-finished frame and an unconsumed jump / action edge are all abandoned by the load.
    session
        .advance(
            GameInput {
                movement: Movement {
                    jump: true,
                    ..Default::default()
                },
                interact: true,
                ..Default::default()
            },
            1. / 200.,
            true,
        )
        .unwrap();
    session.restore_bytes(&bytes).unwrap();
    assert_eq!(fingerprint(&session), saved);
    let mut reference = GameSession::local(fixture.load()).unwrap();
    reference.restore_bytes(&bytes).unwrap();
    for n in 20..80 {
        frame(&mut session, n);
        frame(&mut reference, n);
        assert_eq!(
            fingerprint(&session),
            fingerprint(&reference),
            "pending input leaked through the load at frame {n}"
        );
    }
}

#[test]
fn a_bad_load_changes_nothing_and_the_game_keeps_playing() {
    let fixture = Fixture::new("bad-load");
    let mut session = GameSession::local(fixture.load()).unwrap();
    for n in 0..30 {
        frame(&mut session, n);
    }
    let bytes = session.save_bytes("good").unwrap();
    for n in 30..60 {
        frame(&mut session, n);
    }
    let before = fingerprint(&session);
    let mut damaged = bytes.clone();
    let mid = damaged.len() / 2;
    damaged[mid] ^= 0x40;
    assert!(session.restore_bytes(&damaged).is_err());
    assert!(session.restore_bytes(&bytes[..bytes.len() / 2]).is_err());
    assert!(session.restore_bytes(b"not a save at all").is_err());
    assert!(session.restore_bytes(&[]).is_err());
    assert_eq!(
        fingerprint(&session),
        before,
        "refused loads must leave the game exactly as it was"
    );
    // A well-formed save of different content is refused too.
    let other = Fixture::new("other-content");
    let mut foreign = GameSession::local(other.load()).unwrap();
    let error = foreign.restore_bytes(&bytes).unwrap_err();
    assert!(matches!(error, SaveError::WrongContent { .. }), "{error}");
    frame(&mut session, 60);
    assert_eq!(session.world().tick, before.0 + 1);
}

#[test]
fn slots_round_trip_and_fall_back_to_the_previous_save_when_the_latest_is_damaged() {
    let fixture = Fixture::new("slots");
    let mut session = GameSession::local(fixture.load()).unwrap();
    let slots = SaveSlots::new(fixture.0.join("saves"));
    assert!(matches!(
        session.load_from_slot(&slots, "quick"),
        Err(SaveError::NotFound(_))
    ));
    for n in 0..20 {
        frame(&mut session, n);
    }
    session.save_to_slot(&slots, "quick", "first").unwrap();
    let first = fingerprint(&session);
    for n in 20..50 {
        frame(&mut session, n);
    }
    session.save_to_slot(&slots, "quick", "second").unwrap();
    let second = fingerprint(&session);
    for n in 50..80 {
        frame(&mut session, n);
    }
    let (header, source) = session.load_from_slot(&slots, "quick").unwrap();
    assert_eq!(
        (header.label.as_str(), matches!(source, Source::Primary)),
        ("second", true)
    );
    assert_eq!(fingerprint(&session), second);
    let path = slots.path("quick").unwrap();
    let mut file = std::fs::read(&path).unwrap();
    let mid = file.len() / 2;
    file[mid] ^= 0x33;
    std::fs::write(&path, file).unwrap();
    let (header, source) = session.load_from_slot(&slots, "quick").unwrap();
    assert_eq!(header.label, "first");
    assert!(matches!(source, Source::Backup(_)));
    assert_eq!(fingerprint(&session), first);
}

#[test]
fn an_online_client_refuses_to_save_or_load_because_the_server_owns_the_world() {
    let fixture = Fixture::new("online");
    let server =
        DedicatedServer::with_world("127.0.0.1:0", fixture.load().world().unwrap()).unwrap();
    let mut client = GameSession::connect(
        fixture.load(),
        Box::new(UdpTransport::bind("127.0.0.1:0").unwrap()),
        server.local_addr,
        None,
    )
    .unwrap();
    assert!(!client.can_save());
    let local = GameSession::local(fixture.load()).unwrap();
    assert!(local.can_save());
    let bytes = local.save_bytes("local").unwrap();
    let slots = SaveSlots::new(fixture.0.join("saves"));
    assert!(matches!(client.save_bytes("x"), Err(SaveError::Invalid(_))));
    assert!(matches!(
        client.restore_bytes(&bytes),
        Err(SaveError::Invalid(_))
    ));
    assert!(client.save_to_slot(&slots, "quick", "x").is_err());
    assert!(client.load_from_slot(&slots, "quick").is_err());
    assert!(
        !slots.path("quick").unwrap().exists(),
        "a refused save must not create a file"
    );
}
