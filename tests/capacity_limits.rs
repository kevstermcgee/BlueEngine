//! A server configured for more than the default eight players must be able to save and resume its world.
const _: () = assert!(
    vesper3d::viewer::savestate::world::MAX_PLAYERS
        >= vesper3d::viewer::simulation::MAX_PLAYERS_LIMIT,
    "a server must be able to save every player it can admit"
);
use vesper3d::viewer::simulation::{HeadlessWorld, MAX_PLAYERS_LIMIT};

fn crowded(players: usize) -> HeadlessWorld {
    let mut world = HeadlessWorld::new().unwrap();
    world.set_max_players(players);
    for id in 1..=players as u64 {
        assert!(world.join(id), "player {id} should fit");
    }
    for _ in 0..5 {
        world.step();
    }
    world
}

#[test]
fn a_world_with_more_than_eight_players_round_trips_through_a_save() {
    let world = crowded(12);
    let bytes = world.save_bytes("crowd").expect("a 12-player world saves");
    let mut fresh = HeadlessWorld::new().unwrap();
    fresh.set_max_players(12);
    fresh
        .restore_bytes(&bytes)
        .expect("what the server wrote must load again");
    assert_eq!(fresh.checksum(), world.checksum());
}

#[test]
fn the_save_limit_is_the_server_limit() {
    let world = crowded(MAX_PLAYERS_LIMIT);
    let bytes = world
        .save_bytes("full house")
        .expect("saves at the server's own maximum");
    let mut fresh = HeadlessWorld::new().unwrap();
    fresh.set_max_players(MAX_PLAYERS_LIMIT);
    fresh
        .restore_bytes(&bytes)
        .expect("and loads at that maximum");
}

#[test]
fn discovery_reports_limits_read_from_the_code_and_docs_do_not_contradict_them() {
    use vesper3d::viewer::{capabilities, net, savestate, simulation};
    let described = capabilities::describe().unwrap();
    let support = &described["runtime_support"];
    let stock = &support["stock_server"];
    assert_eq!(stock["players"]["default"], simulation::DEFAULT_MAX_PLAYERS);
    assert_eq!(
        stock["players"]["configurable_up_to"],
        simulation::MAX_PLAYERS_LIMIT
    );
    assert_eq!(
        stock["saving"]["players_in_a_save"],
        savestate::world::MAX_PLAYERS
    );
    assert_eq!(stock["datagram_bytes"], net::MAX_PACKET_BYTES);
    assert!(support["custom_sim_netplay"]["graceful_shutdown"]
        .as_str()
        .unwrap()
        .contains("not provided"));

    // The specific contradictions the review found must stay out of the prose.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let read = |path: &str| std::fs::read_to_string(root.join(path)).unwrap();
    assert!(!read("README.md").contains("one JSON packet path"));
    assert!(!read("docs/SAVE_STATE.md").contains("at most 8 players"));
    assert!(!read("docs/SAVE_STATE.md").contains("at most eight players"));
}

#[test]
fn a_server_that_could_not_replicate_a_full_house_refuses_to_start_that_way() {
    use vesper3d::{
        prelude::{SceneBuilder, V},
        viewer::server::DedicatedServer,
    };
    let mut scene = SceneBuilder::new("Crowded props").spawn(V(0., 0., 4.6), 0.0);
    for i in 0..50 {
        scene = scene.box_body(
            format!("crate{i}"),
            V(i as f32 * 0.6 - 15., 0.3, -2.),
            V(0.25, 0.25, 0.25),
            V::ONE,
        );
    }
    let world = scene.world().unwrap();
    let server = DedicatedServer::with_world("127.0.0.1:0", world)
        .unwrap()
        .with_max_players(1020);
    let error = server.check_capacity().unwrap_err().to_string();
    assert!(
        error.contains("--max-players") && error.contains("fewer"),
        "{error}"
    );
    let fits = server.with_max_players(900);
    fits.check_capacity()
        .expect("900 players plus 50 objects fit");
}
