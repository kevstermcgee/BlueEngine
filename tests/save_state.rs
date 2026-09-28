//! Save states of the engine world: save, restore and continue must equal an uninterrupted run, a bad save
//! must be refused without touching the world, and a save for other content must never load.
use std::{path::PathBuf, process::Command};
use vesper3d::{
    math::V,
    viewer::{
        controller::Movement,
        game::GameDocument,
        newgame::scaffold_new_game,
        savestate::{
            self,
            world::{Hold, RestoreOptions, WorldState, KIND, VERSION},
            SaveError, SaveHeader, SaveSlots, Source,
        },
        server::DedicatedServer,
        simulation::HeadlessWorld,
    },
};

struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "blue-save-state-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        scaffold_new_game(name, &dir, Some(env!("CARGO_MANIFEST_DIR"))).unwrap();
        Self(dir)
    }
    fn world(&self) -> HeadlessWorld {
        let mut world = GameDocument::load(&self.0.join("game.json"))
            .unwrap()
            .world()
            .unwrap();
        assert!(world.join(1));
        world
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn apple_index(world: &HeadlessWorld) -> usize {
    world
        .prop_physics
        .as_ref()
        .unwrap()
        .props
        .iter()
        .position(|p| p.id == "apple")
        .unwrap()
}

/// What a scripted playthrough touched, so a test can prove it was not hollow.
#[derive(Default)]
struct Coverage {
    completed: bool,
    restarted: bool,
    zone_visited: bool,
    carried: bool,
    jumped: bool,
}

/// One tick of a scripted player. The script aims at the terminal and completes the objective, restarts the
/// round, picks up the apple, carries it out of the lobby into the courtyard trigger zone while jumping,
/// then drops it (prop physics in flight) and finally stands still until everything comes to rest.
fn drive(world: &mut HeadlessWorld, tick: u64, seen: &mut Coverage) {
    let mut movement = Movement::default();
    let mut yaw = 0.;
    match tick {
        5 => {
            world.game_action(1).unwrap(); // the terminal is straight ahead: objective complete
        }
        40 => {
            world.game_action(1).unwrap(); // any press after completion restarts the round
        }
        62 => {
            let index = apple_index(world);
            world
                .prop_physics
                .as_mut()
                .unwrap()
                .set_held_for_player(1, index);
        }
        170 => world.prop_physics.as_mut().unwrap().drop_held(),
        _ => {}
    }
    if (45..175).contains(&tick) {
        yaw = std::f32::consts::FRAC_PI_2;
        movement.forward = 1.;
        movement.jump = tick.is_multiple_of(40);
    }
    assert!(world.input(1, movement, yaw, 0.));
    world.step();
    let game = world.game.as_ref().unwrap().state();
    seen.completed |= game.completed;
    seen.restarted |= game.round > 0;
    seen.zone_visited |= game.counters[0] > 0;
    seen.jumped |= movement.jump;
    seen.carried |= !world
        .save_state()
        .unwrap()
        .physics
        .unwrap()
        .holds
        .is_empty();
}

fn at_rest(world: &HeadlessWorld) -> bool {
    world
        .prop_physics
        .as_ref()
        .unwrap()
        .active_and_sleeping_counts()
        .0
        == 0
}

const TICKS: u64 = 600;

#[test]
fn saving_and_resuming_matches_an_uninterrupted_run_when_props_are_at_rest_or_falling() {
    let fixture = Fixture::new("resume-proof");
    let mut reference = fixture.world();
    let mut seen = Coverage::default();
    let mut checksums = vec![reference.checksum()];
    let mut saves: Vec<(u64, bool, Vec<u8>)> = Vec::new();
    for tick in 0..TICKS {
        // Exact resumption is promised while props are asleep or in free fall (before the first landing).
        let exact = at_rest(&reference) || tick < 15;
        if tick < 12 || tick % 25 == 0 || tick == TICKS - 1 {
            saves.push((
                tick,
                exact,
                reference.save_bytes(&format!("tick {tick}")).unwrap(),
            ));
        }
        drive(&mut reference, tick, &mut seen);
        checksums.push(reference.checksum());
    }
    assert!(
        seen.completed && seen.restarted && seen.zone_visited && seen.carried && seen.jumped,
        "the script must exercise rules, restart, zones, carrying and jumping"
    );
    let exact_splits = saves.iter().filter(|(_, exact, _)| *exact).count();
    assert!(
        exact_splits >= 12,
        "only {exact_splits} exact split points; the scenario never rests"
    );
    let mut checked_after_rest = 0;
    for (split, exact, bytes) in saves {
        let mut resumed = fixture.world();
        let header = resumed.restore_bytes(&bytes).unwrap();
        assert_eq!((header.kind.as_str(), header.tick), (KIND, split));
        assert_eq!(
            resumed.checksum(),
            checksums[split as usize],
            "restored state at tick {split}"
        );
        let mut ignored = Coverage::default();
        for tick in split..TICKS {
            drive(&mut resumed, tick, &mut ignored);
            if exact {
                assert_eq!(
                    resumed.checksum(),
                    checksums[tick as usize + 1],
                    "resumed run diverged at tick {} after restoring tick {split}",
                    tick + 1
                );
            }
        }
        if exact {
            checked_after_rest += 1;
        } else {
            // Mid-contact saves resume from the exact saved poses and velocities but with a rebuilt contact
            // cache: a valid, not identical, continuation. It must still be a healthy simulation.
            let physics = resumed.prop_physics.as_ref().unwrap();
            assert!(
                (0..physics.props.len()).all(|i| physics
                    .prop_position(i)
                    .is_some_and(|p| p.finite() && p.1 > -1.)),
                "props left the world after restoring tick {split}"
            );
        }
    }
    assert_eq!(checked_after_rest, exact_splits);
}

#[test]
fn a_saved_world_saves_identically_again_and_survives_a_json_round_trip() {
    let fixture = Fixture::new("determinism");
    let mut world = fixture.world();
    let mut seen = Coverage::default();
    for tick in 0..90 {
        drive(&mut world, tick, &mut seen);
    }
    let state = world.save_state().unwrap();
    let again = world.save_state().unwrap();
    assert_eq!(state, again);
    let (a, b) = (
        serde_json::to_vec(&state).unwrap(),
        serde_json::to_vec(&again).unwrap(),
    );
    assert_eq!(a, b, "serialisation is deterministic");
    let back: WorldState = serde_json::from_slice(&a).unwrap();
    assert_eq!(back, state);
    // Restoring a world into itself leaves its saved state unchanged.
    world.restore_state(&state).unwrap();
    assert_eq!(world.save_state().unwrap(), state);
}

/// Damage a valid save's state, re-frame it with a valid checksum, and expect refusal with the world untouched.
fn assert_refused(fixture: &Fixture, damage: impl Fn(&mut WorldState), what: &str) {
    let mut world = fixture.world();
    let mut seen = Coverage::default();
    for tick in 0..120 {
        drive(&mut world, tick, &mut seen);
    }
    let mut state = world.save_state().unwrap();
    damage(&mut state);
    let header = SaveHeader::new(KIND, VERSION, "damaged")
        .with_content(world.content_hash)
        .with_tick(state.tick);
    let before = (world.checksum(), world.save_state().unwrap());
    // Most damage is valid JSON but semantically wrong, caught by restore's checks; a non-finite float
    // (e.g. the NaN case) is instead caught while framing the save itself (it cannot round-trip through
    // JSON at all), so either point of refusal counts, as long as the world is left untouched either way.
    let error = match savestate::save_bytes(&header, &state) {
        Err(error) => error,
        Ok(bytes) => world.restore_bytes(&bytes).expect_err(what),
    };
    assert!(matches!(error, SaveError::Invalid(_)), "{what}: {error}");
    assert_eq!(
        (world.checksum(), world.save_state().unwrap()),
        before,
        "{what}: the world changed"
    );
}

#[test]
fn a_bad_save_is_refused_and_the_world_is_left_exactly_as_it_was() {
    let fixture = Fixture::new("transaction");
    assert_refused(
        &fixture,
        |s| s.physics.as_mut().unwrap().props[0].id = "ghost".into(),
        "unknown prop",
    );
    assert_refused(
        &fixture,
        |s| s.physics.as_mut().unwrap().props.clear(),
        "missing props",
    );
    assert_refused(
        &fixture,
        |s| s.game.as_mut().unwrap().state.counters[0] = 2_000_000,
        "counter out of range",
    );
    assert_refused(
        &fixture,
        |s| s.game.as_mut().unwrap().state.enabled = u64::MAX,
        "flags beyond the rules",
    );
    assert_refused(
        &fixture,
        |s| s.game.as_mut().unwrap().timers.push(5),
        "extra timer",
    );
    assert_refused(&fixture, |s| s.game = None, "rules missing");
    assert_refused(&fixture, |s| s.physics = None, "physics missing");
    assert_refused(
        &fixture,
        |s| s.players[0].controller.position = V(f32::NAN, 0., 0.),
        "nan position",
    );
    assert_refused(
        &fixture,
        |s| s.lifecycle.objects[0].id = "other-object".into(),
        "another map's objects",
    );
    assert_refused(
        &fixture,
        |s| s.checksum ^= 1,
        "the restored world must reproduce the saved checksum",
    );
    assert_refused(
        &fixture,
        |s| {
            let p = s.physics.as_mut().unwrap();
            p.holds.push(Hold {
                player: 9,
                prop: p.props[0].id.clone(),
            });
        },
        "a hold by a player who is not in the save",
    );
}

#[test]
fn a_save_for_other_content_wrong_kind_or_newer_version_never_loads() {
    let (alpha, beta) = (Fixture::new("alpha"), Fixture::new("beta"));
    let mut a = alpha.world();
    let mut b = beta.world();
    let mut seen = Coverage::default();
    for tick in 0..30 {
        drive(&mut a, tick, &mut seen);
        drive(&mut b, tick, &mut seen);
    }
    let save_a = a.save_bytes("alpha").unwrap();
    assert!(
        matches!(
            b.restore_bytes(&save_a),
            Err(SaveError::WrongContent { .. })
        ),
        "another game's save"
    );
    let state = a.save_state().unwrap();
    let hash = a.content_hash;
    let header = |kind: &str, version: u32| SaveHeader::new(kind, version, "x").with_content(hash);
    let wrong_kind = savestate::save_bytes(&header("other", VERSION), &state).unwrap();
    assert!(matches!(
        a.restore_bytes(&wrong_kind),
        Err(SaveError::WrongKind { .. })
    ));
    let newer = savestate::save_bytes(&header(KIND, VERSION + 1), &state).unwrap();
    assert!(matches!(
        a.restore_bytes(&newer),
        Err(SaveError::NewerVersion { .. })
    ));
    let unbound = savestate::save_bytes(&SaveHeader::new(KIND, VERSION, "x"), &state).unwrap();
    assert!(
        matches!(
            a.restore_bytes(&unbound),
            Err(SaveError::WrongContent { .. })
        ),
        "no fingerprint, no load"
    );
    assert!(a.restore_bytes(&save_a).is_ok());
    let mut damaged = save_a.clone();
    damaged[60] ^= 1;
    assert!(matches!(
        a.restore_bytes(&damaged),
        Err(SaveError::Corrupt(_))
    ));
}

#[test]
fn a_server_resuming_a_world_keeps_its_own_players_and_drops_holds() {
    let fixture = Fixture::new("server-load");
    let mut world = fixture.world();
    let mut seen = Coverage::default();
    for tick in 0..100 {
        drive(&mut world, tick, &mut seen);
    }
    assert!(
        seen.carried,
        "the apple is being carried when the save is taken"
    );
    let bytes = world.save_bytes("server").unwrap();
    let mut server = GameDocument::load(&fixture.0.join("game.json"))
        .unwrap()
        .world()
        .unwrap();
    assert!(server.join(7));
    let (header, state) = server.parse_save(&bytes).unwrap();
    assert_eq!(header.tick, 100);
    assert!(!state.physics.as_ref().unwrap().holds.is_empty());
    server
        .restore_state_with(&state, RestoreOptions { players: false })
        .unwrap();
    assert_eq!(server.tick, 100, "the world clock resumes");
    assert!(
        server.player(7).is_some() && server.player(1).is_none(),
        "connected sessions are not replaced"
    );
    assert!(
        server
            .save_state()
            .unwrap()
            .physics
            .unwrap()
            .holds
            .is_empty(),
        "a ghost player holds nothing"
    );
}

#[test]
fn slots_hold_world_saves_with_a_last_good_backup() {
    let fixture = Fixture::new("slots");
    let mut world = fixture.world();
    let mut seen = Coverage::default();
    let slots = SaveSlots::new(fixture.0.join("saves"));
    for tick in 0..60 {
        drive(&mut world, tick, &mut seen);
    }
    slots
        .save_framed("quick", &world.save_bytes("first").unwrap())
        .unwrap();
    let first = world.checksum();
    for tick in 60..120 {
        drive(&mut world, tick, &mut seen);
    }
    slots
        .save_framed("quick", &world.save_bytes("second").unwrap())
        .unwrap();
    let listing = slots.list().unwrap();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0].header.as_ref().unwrap().label, "second");
    assert!(listing[0].has_backup);
    // Ruin the primary file: loading falls back to the first save and says so.
    let path = slots.path("quick").unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0x55;
    std::fs::write(&path, bytes).unwrap();
    let loaded = slots.load("quick").unwrap();
    assert!(matches!(loaded.source, Source::Backup(_)));
    let mut fresh = fixture.world();
    let (header, state) = fresh.parse_loaded(&loaded).unwrap();
    fresh.restore_state(&state).unwrap();
    assert_eq!((header.label.as_str(), fresh.checksum()), ("first", first));
}

#[test]
fn the_test_lab_saves_and_resumes_exactly_once_it_has_settled() {
    let build = || {
        let mut world = HeadlessWorld::new().unwrap();
        assert!(world.join(1) && world.join(2));
        world
    };
    let run = |world: &mut HeadlessWorld, from: u64, to: u64| {
        for tick in from..to {
            let movement = Movement {
                forward: 1.,
                right: f32::from(tick % 50 < 10),
                ..Default::default()
            };
            world.input(1, movement, 0.3 + tick as f32 * 0.002, 0.);
            world.input(
                2,
                Movement {
                    forward: -1.,
                    ..Default::default()
                },
                3.0,
                0.,
            );
            world.step();
        }
    };
    let mut reference = build();
    run(&mut reference, 0, 240);
    let bytes = reference.save_bytes("lab").unwrap();
    run(&mut reference, 240, 480);
    let mut resumed = build();
    resumed.restore_bytes(&bytes).unwrap();
    assert_eq!(resumed.tick, 240);
    run(&mut resumed, 240, 480);
    assert_eq!(resumed.checksum(), reference.checksum());
    assert!(
        bytes.len() < 4_000_000,
        "a full lab save is {} bytes",
        bytes.len()
    );
}

#[test]
fn two_fresh_worlds_given_the_same_inputs_stay_identical() {
    // Prop physics must be deterministic run to run, or no save/resume comparison means anything.
    let fixture = Fixture::new("twins");
    let (mut a, mut b) = (fixture.world(), fixture.world());
    let (mut seen_a, mut seen_b) = (Coverage::default(), Coverage::default());
    for tick in 0..TICKS {
        drive(&mut a, tick, &mut seen_a);
        drive(&mut b, tick, &mut seen_b);
        assert_eq!(
            a.checksum(),
            b.checksum(),
            "the worlds diverged at tick {}",
            tick + 1
        );
    }
}

#[test]
fn what_happens_after_a_load_depends_only_on_the_save_never_on_the_world_that_loaded_it() {
    let fixture = Fixture::new("history-free");
    let mut reference = fixture.world();
    let mut seen = Coverage::default();
    let mut saves = Vec::new();
    for tick in 0..TICKS {
        if tick % 20 == 0 {
            saves.push((tick, reference.save_bytes(&format!("tick {tick}")).unwrap()));
        }
        drive(&mut reference, tick, &mut seen);
    }
    // `reference` has lived through the whole scenario, so its contact caches, islands and sleep timers are
    // as dirty as they get. Loading into it must play out exactly like loading into a world that is brand new,
    // including for saves taken with props in mid-air or mid-bounce.
    let mut moving = 0;
    for (split, bytes) in saves {
        let mut fresh = fixture.world();
        fresh.restore_bytes(&bytes).unwrap();
        reference.restore_bytes(&bytes).unwrap();
        moving += usize::from(!at_rest(&fresh));
        let mut ignored = Coverage::default();
        for tick in split..TICKS.min(split + 150) {
            drive(&mut fresh, tick, &mut ignored);
            drive(&mut reference, tick, &mut ignored);
            assert_eq!(
                fresh.checksum(),
                reference.checksum(),
                "loading the save from tick {split} into a used world diverged at tick {}",
                tick + 1
            );
        }
    }
    assert!(
        moving >= 5,
        "only {moving} saves had props in motion; the scenario is too tame to prove this"
    );
}

fn tool(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_be2-tools"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn save_info_describes_a_save_and_says_whether_it_belongs_to_a_game() {
    let (fixture, other) = (Fixture::new("info"), Fixture::new("info-other"));
    let mut world = fixture.world();
    let mut seen = Coverage::default();
    for tick in 0..50 {
        drive(&mut world, tick, &mut seen);
    }
    let slots = SaveSlots::new(fixture.0.join("saves"));
    slots
        .save_framed("quick", &world.save_bytes("Quick save").unwrap())
        .unwrap();
    let file = slots.path("quick").unwrap();
    let (game, stranger) = (fixture.0.join("game.json"), other.0.join("game.json"));
    let ask = |file: &std::path::Path, game: &std::path::Path| {
        let out = tool(&["save-info", file.to_str().unwrap(), game.to_str().unwrap()]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()
    };
    let report = ask(&file, &game);
    assert_eq!(
        (report["ok"].clone(), report["kind"].clone()),
        (true.into(), "world".into())
    );
    assert_eq!(
        (report["label"].clone(), report["tick"].clone()),
        ("Quick save".into(), 50.into())
    );
    assert_eq!(
        (report["checksum"].clone(), report["source"].clone()),
        ("verified".into(), "primary".into())
    );
    assert_eq!(report["summary"]["players"], serde_json::json!([1]));
    assert!(report["summary"]["props"].as_u64().unwrap() > 0);
    assert_eq!(report["summary"]["valid"], true);
    assert_eq!(report["matches_game"], true, "it is this game's save");
    assert_eq!(
        ask(&file, &stranger)["matches_game"],
        false,
        "another game's content must not match"
    );
    // Without a game document the report still verifies the file and names what it is.
    let plain = tool(&["save-info", file.to_str().unwrap()]);
    assert!(plain.status.success());
    // A damaged file is a failure with a plain sentence, and never a half-read report.
    let mut bytes = std::fs::read(&file).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0x55;
    let broken = fixture.0.join("broken.be2save");
    std::fs::write(&broken, bytes).unwrap();
    let out = tool(&["save-info", broken.to_str().unwrap()]);
    assert!(!out.status.success());
    let error: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(error["ok"], false);
    assert!(
        error["error"].as_str().unwrap().contains("corrupt")
            || error["error"].as_str().unwrap().contains("checksum"),
        "{error}"
    );
}

#[test]
fn a_server_autosaves_on_schedule_keeps_a_ring_and_a_failing_disk_never_stops_it() {
    let fixture = Fixture::new("server-autosave");
    let slots = SaveSlots::new(fixture.0.join("saves"));
    let mut server = DedicatedServer::with_world("127.0.0.1:0", fixture.world())
        .unwrap()
        .with_autosave(slots.clone(), 1., 3);
    for _ in 0..250 {
        server.step();
    }
    // One save each simulated second: ticks 60, 120, 180 and 240; the ring keeps the newest three.
    assert_eq!(
        (server.autosave_stats.written, server.autosave_stats.failed),
        (4, 0)
    );
    let mut names: Vec<String> = slots.list().unwrap().into_iter().map(|s| s.slot).collect();
    names.sort();
    assert_eq!(names, ["auto", "auto-2", "auto-3"]);
    for (slot, tick) in [("auto", 240), ("auto-2", 180), ("auto-3", 120)] {
        assert_eq!(slots.load(slot).unwrap().header.tick, tick, "{slot}");
    }
    // A fresh server world resumes from the newest one, keeping its own players.
    let mut resumed = fixture.world();
    let (header, state) = resumed.parse_loaded(&slots.load("auto").unwrap()).unwrap();
    resumed
        .restore_state_with(&state, RestoreOptions { players: false })
        .unwrap();
    assert_eq!(
        (resumed.tick, header.label.as_str()),
        (240, "Autosave, tick 240")
    );
    resumed.step();

    // A disk that cannot be written to is counted and reported, and the simulation carries on.
    let blocker = fixture.0.join("blocker");
    std::fs::write(&blocker, b"a file where the save folder should be").unwrap();
    let mut broken = DedicatedServer::with_world("127.0.0.1:0", fixture.world())
        .unwrap()
        .with_autosave(SaveSlots::new(blocker.join("saves")), 1., 3);
    for _ in 0..130 {
        broken.step();
    }
    assert_eq!(
        broken.world.tick, 130,
        "a failing disk must not stop the game"
    );
    assert_eq!(
        (broken.autosave_stats.written, broken.autosave_stats.failed),
        (0, 2)
    );
    assert!(broken.autosave_stats.last_error.is_some());
    assert!(!broken.autosave_now(), "still failing, still not fatal");
}

#[test]
fn the_headless_server_saves_at_shutdown_and_a_new_run_resumes_from_it() {
    let fixture = Fixture::new("server-cli");
    let (game, saves) = (fixture.0.join("game.json"), fixture.0.join("saves"));
    let run = |args: &[&str]| {
        let mut all = vec![
            "--game",
            game.to_str().unwrap(),
            "--save-dir",
            saves.to_str().unwrap(),
        ];
        all.extend_from_slice(args);
        Command::new(env!("CARGO_BIN_EXE_be2-headless"))
            .args(all)
            .output()
            .unwrap()
    };
    let out = run(&[
        "--server",
        "127.0.0.1:0",
        "--ticks",
        "90",
        "--autosave",
        "60",
    ]);
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("Final save written at tick 90"), "{text}");
    let info = savestate::describe(&saves.join("auto.be2save")).unwrap();
    assert_eq!(
        (info["kind"].as_str(), info["tick"].as_u64()),
        (Some("world"), Some(90))
    );
    // Resume it (players are new) and run 30 more ticks: the tick counter continues from the save.
    let out = run(&["--load", "auto", "--ticks", "30"]);
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("Resumed \"Autosave, tick 90\" at tick 90"),
        "{text}"
    );
    assert!(text.contains("ticks=120"), "{text}");
    // Naming a save that does not exist is a clear failure, not a fresh world.
    let out = run(&["--load", "nosuch", "--ticks", "5"]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("--load nosuch"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// These content-only maps are meant to be embedded in a game document, whose spawn points override
/// this value, so they carry none of their own; probe a small grid for a point `validate()` accepts
/// (finite, feet at or above ground, and not overlapping a collider at standing height) purely so a
/// bare `HeadlessWorld` can join a player. The map's own layout decides which points are open.
fn find_open_spawn(
    document: &mut vesper3d::viewer::authoring::MapDocument,
) -> vesper3d::viewer::authoring::MapSpawn {
    use vesper3d::viewer::authoring::MapSpawn;
    for y in [0.9, 2., 4., 6.] {
        for x in [0., 2., -2., 4., -4., 6., -6.] {
            for z in [0., 2., -2., 4., -4., 6., -6.] {
                let spawn = MapSpawn {
                    feet: V(x, y, z),
                    yaw: 0.,
                };
                document.default_spawn = Some(spawn);
                if document.validate().is_ok() {
                    return spawn;
                }
            }
        }
    }
    panic!("no open spawn point found for {}", document.name);
}

// These four maps carry many more dynamic props than the Test Lab (65-103, against a handful), and an
// unoptimized `cargo test` build of rapier's contact solver costs hundreds of milliseconds per tick on
// that many bodies; measured driving a player through all four for 40 ticks each took 16-31s per map, a
// debug-build physics cost, not a save-state one. This test compares two INDEPENDENT restores of the same
// save against each other (never against an uninterrupted run), which holds exactly regardless of whether
// the saved props had settled, because a restore rebuilds physics from the same pristine scene either way
// (see `what_happens_after_a_load_depends_only_on_the_save_never_on_the_world_that_loaded_it` for that
// property proven at depth on the Test Lab); this test's job is breadth, so it stays short.
#[test]
fn every_shipped_map_restores_identically_regardless_of_what_loaded_it() {
    use vesper3d::viewer::authoring::MapDocument;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in ["house", "school-wing", "office", "convenience-store"] {
        let path = root.join(format!("assets/maps/starters/{name}.json"));
        let build = || {
            let mut document = MapDocument::load(&path).unwrap();
            document.default_spawn = Some(find_open_spawn(&mut document));
            let room = document.build().unwrap();
            let mut world = HeadlessWorld::try_with_room(room).unwrap();
            assert!(world.join(1), "{name}");
            world
        };
        let run = |world: &mut HeadlessWorld, from: u64, to: u64| {
            for tick in from..to {
                let movement = Movement {
                    forward: 1.,
                    right: f32::from(tick % 14 < 4),
                    jump: tick == 5,
                    ..Default::default()
                };
                world.input(1, movement, 0.4 + tick as f32 * 0.03, 0.);
                world.step();
            }
        };
        // The save comes from a world that has already lived a life (real movement, real prop contact).
        let mut source = build();
        run(&mut source, 0, 12);
        let bytes = source.save_bytes(name).unwrap();
        assert!(
            bytes.len() < 4_000_000,
            "{name}: a save is {} bytes",
            bytes.len()
        );
        // A brand-new world and one that already lived a different life both load it...
        let mut fresh = build();
        let header = fresh.restore_bytes(&bytes).unwrap();
        assert_eq!((header.kind.as_str(), fresh.tick), (KIND, 12), "{name}");
        let mut used = build();
        run(&mut used, 0, 7);
        used.restore_bytes(&bytes).unwrap();
        // ...and from there, identically scripted, they must play out identically.
        for tick in 12..20 {
            run(&mut fresh, tick, tick + 1);
            run(&mut used, tick, tick + 1);
            assert_eq!(
                fresh.checksum(),
                used.checksum(),
                "{name}: diverged at tick {}",
                tick + 1
            );
        }
    }
}
