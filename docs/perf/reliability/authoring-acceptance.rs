//! Fresh-author acceptance: only player intentions change the authority.
use automation_workshop::{Command, Envelope, Workshop};
use vesper3d::runtime::{snapshot, Simulation, Snapshot};

const A: &str = "workshop:machine-a";
const B: &str = "workshop:machine-b";
const RECIPE: &str = "workshop:recycle-plate";
const UPGRADE: &str = "workshop:recycling-tools";

fn queue(machine: &str, recipe: &str) -> Command {
    Command::Queue { machine: machine.into(), recipe: recipe.into() }
}

fn act(game: &mut Workshop, command: Command, expected_ok: bool) -> Envelope {
    let serial = game.state.last_command.get(&0).copied().unwrap_or(0) + 1;
    let input = Envelope { seat: 0, serial, command };
    game.step(&vec![input.clone()]);
    assert_eq!(game.outcomes.len(), 1);
    assert_eq!(game.outcomes[0].ok, expected_ok, "{}", game.outcomes[0].detail);
    input
}

fn reload(game: &Workshop) -> Workshop {
    let bytes = snapshot::save(game, "Plate recycling acceptance").unwrap();
    let mut restored = Workshop::standard();
    snapshot::restore(&mut restored, &bytes).unwrap();
    assert_eq!(restored.capture(), game.capture());
    assert_eq!(restored.state_hash(), game.state_hash());
    restored
}

fn together(game: &mut Workshop, restored: &mut Workshop, input: Vec<Envelope>) {
    game.step(&input);
    restored.step(&input);
    assert_eq!(restored.capture(), game.capture());
    assert_eq!(restored.state_hash(), game.state_hash());
    assert_eq!(restored.outcomes, game.outcomes);
}

#[test]
fn recycling_requires_paid_upgrade_and_resumes_blocked_production_exactly() {
    let mut game = Workshop::standard();
    act(&mut game, queue(A, "workshop:smelt"), true);
    act(&mut game, queue(B, "workshop:smelt"), true);
    for _ in 0..5 { game.step(&vec![]); }
    assert_eq!(game.state.inventory.quantity("workshop:plate"), 4);

    let before = game.capture();
    act(&mut game, queue(A, RECIPE), false);
    assert!(game.outcomes[0].detail.contains("WORKSHOP_UPGRADE_REQUIRED"));
    assert_eq!(game.state.inventory, before.inventory);
    assert!(game.state.machines[A].queue.is_empty());

    act(&mut game, Command::Upgrade { machine: A.into(), upgrade: UPGRADE.into() }, true);
    assert_eq!(game.state.inventory.quantity("workshop:plate"), 3);
    assert_eq!(game.state.inventory.quantity("workshop:coal"), 1);
    assert!(game.state.machines[A].upgrades.contains(UPGRADE));
    assert!(game.state.machines[B].upgrades.is_empty());
    act(&mut game, queue(A, RECIPE), true);
    assert_eq!(game.state.inventory.quantity("workshop:plate"), 2);
    let job = &game.state.machines[A].queue[0];
    assert_eq!(job.duration_ticks, 4);
    assert_eq!(job.remaining_ticks, 3);
    assert_eq!(job.inputs, vec![("workshop:plate".into(), 1)]);
    assert_eq!(job.outputs, vec![("workshop:ore".into(), 1)]);
    let inspection = game.inspect(A).unwrap();
    assert_eq!(inspection["rule"], RECIPE);
    assert_eq!(inspection["definition"]["id"], RECIPE);

    let fill = act(&mut game, Command::Gather { item: "workshop:ore".into(), amount: 5 }, true);
    assert_eq!(game.state.inventory.total_units(), 12);
    assert_eq!(game.state.machines[A].queue[0].remaining_ticks, 2);
    let mut restored = reload(&game);
    // A saved serial prevents the fill action from applying twice after load.
    together(&mut game, &mut restored, vec![fill]);
    assert!(game.outcomes.is_empty());
    together(&mut game, &mut restored, vec![]);
    assert_eq!(game.state.machines[A].queue[0].remaining_ticks, 0);
    assert_eq!(game.state.machines[A].blocked.as_ref().unwrap().code, "RESOURCE-BUDGET");
    assert_eq!(game.state.inventory.quantity("workshop:ore"), 9);
    assert_eq!(game.state.inventory.quantity("workshop:plate"), 2);

    restored = reload(&game);
    let pending = game.state.machines[A].queue.clone();
    together(&mut game, &mut restored, vec![]);
    assert_eq!(game.state.machines[A].queue, pending);
    let serial = game.state.last_command[&0] + 1;
    let recovery = Envelope { seat: 0, serial, command: Command::Deliver { item: "workshop:ore".into(), amount: 1 } };
    together(&mut game, &mut restored, vec![recovery.clone()]);
    assert!(game.outcomes[0].ok);
    assert!(game.state.machines[A].queue.is_empty());
    assert!(game.state.machines[A].blocked.is_none());
    assert_eq!(game.state.inventory.quantity("workshop:ore"), 9);
    assert_eq!(game.state.inventory.quantity("workshop:plate"), 2);
    assert_eq!(game.state.inventory.quantity("workshop:coal"), 1);
    assert_eq!(game.state.inventory.total_units(), 12);
    assert_eq!(game.state.delivered["workshop:ore"], 1);
    together(&mut game, &mut restored, vec![recovery]);
    assert!(game.outcomes.is_empty());
    together(&mut game, &mut restored, vec![]);
    assert_eq!(game.state.inventory.quantity("workshop:ore"), 9);
    assert_eq!(game.state.delivered["workshop:ore"], 1);
}
