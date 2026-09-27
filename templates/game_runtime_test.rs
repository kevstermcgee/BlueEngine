//! Same public session used by the visible generated client, exercised without graphics.
use vesper3d::viewer::{game::GameDocument, game_session::{GameInput, GameSession}};
#[test]
fn authored_objective_prop_and_replay() {
    let loaded = GameDocument::load(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/game.json"))).unwrap();
    let mut session = GameSession::local(loaded).unwrap();
    let start = session.controller().position;
    let apple = session.world().prop_position("apple").unwrap();
    for _ in 0..12 {
        session.advance(GameInput { movement: vesper3d::viewer::controller::Movement { forward: 1., ..Default::default() }, ..Default::default() }, 1./60., true).unwrap();
    }
    assert!((session.controller().position - start).length() > 0.1);
    assert!((session.world().prop_position("apple").unwrap() - apple).length() > 0.01);
    session.advance(GameInput { interact: true, ..Default::default() }, 1./60., true).unwrap();
    assert!(session.world().game.as_ref().unwrap().state().completed);
    let tick = session.world().tick;
    session.advance(GameInput::default(), 1., false).unwrap();
    assert_eq!(session.world().tick, tick, "local pause must freeze authority");
    session.advance(GameInput { interact: true, ..Default::default() }, 1./60., true).unwrap();
    assert!(!session.world().game.as_ref().unwrap().state().completed);
    assert_eq!(session.world().game.as_ref().unwrap().state().round, 1);
    assert!((session.controller().position - start).length() < 0.01);
    assert!((session.world().prop_position("apple").unwrap() - apple).length() < 0.02);
}
