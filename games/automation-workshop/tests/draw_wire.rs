//! Author acceptance exercises packaged content and player intentions on the existing authority.
use automation_workshop::{Command, Envelope, State, Workshop};
use vesper3d::runtime::{snapshot, SavePolicy, Simulation, Snapshot};

const A: &str = "workshop:machine-a";
const B: &str = "workshop:machine-b";
const DRAW: &str = "workshop:draw-wire";
const TOOLS: &str = "workshop:wire-tools";
const ITEMS: [&str; 5] = ["workshop:ore", "workshop:coal", "workshop:plate", "workshop:gear", "workshop:wire"];

fn queue(machine: &str, recipe: &str) -> Command {
    Command::Queue { machine: machine.into(), recipe: recipe.into() }
}

fn act(game: &mut Workshop, command: Command, ok: bool) -> Envelope {
    let serial = game.state.last_command.get(&0).copied().unwrap_or(0) + 1;
    let input = Envelope { seat: 0, serial, command };
    let tick = game.state.tick;
    game.step(&vec![input.clone()]);
    assert_eq!(game.state.tick, tick + 1);
    assert_eq!(game.state.last_command.get(&0), Some(&serial));
    assert_eq!(game.outcomes.len(), 1);
    let outcome = &game.outcomes[0];
    assert_eq!((outcome.seat, outcome.serial), (0, serial));
    assert_eq!(outcome.ok, ok, "{}", outcome.detail);
    input
}

fn balances(game: &Workshop, expected: [u64; 5]) {
    for (item, amount) in ITEMS.into_iter().zip(expected) {
        assert_eq!(game.state.inventory.counts().get(item).copied().unwrap_or(0), amount, "{item}");
    }
    assert_eq!(game.state.inventory.total_units(), expected.iter().sum::<u64>());
}

// Gather/deliver and queue escrow transfer ownership. Only paid upgrades and completed
// recipes may change these totals, including resources outside the shared inventory.
fn accounted(state: &State) -> [u64; 5] {
    ITEMS.map(|item| {
        let stored = [state.inventory.counts(), &state.supply, &state.delivered]
            .into_iter()
            .map(|map| map.get(item).copied().unwrap_or(0))
            .sum::<u64>();
        stored
            + state
                .machines
                .values()
                .flat_map(|machine| &machine.queue)
                .flat_map(|job| &job.inputs)
                .filter(|(id, _)| id == item)
                .map(|(_, amount)| amount)
                .sum::<u64>()
    })
}

fn reload(game: &Workshop) -> Workshop {
    assert_eq!(Workshop::POLICY, SavePolicy::Exact);
    let bytes = snapshot::save(game, "Draw wire production").expect("engine Snapshot save");
    let mut restored = Workshop::standard();
    snapshot::restore(&mut restored, &bytes).expect("engine Snapshot load");
    assert_eq!(restored.capture(), game.capture());
    assert_eq!(restored.state_hash(), game.state_hash());
    restored
}

fn same(uninterrupted: &Workshop, restored: &Workshop) {
    assert_eq!(restored.state, uninterrupted.state);
    assert_eq!(restored.state_hash(), uninterrupted.state_hash());
    assert_eq!(restored.outcomes, uninterrupted.outcomes);
}

fn advance_pair(uninterrupted: &mut Workshop, restored: &mut Workshop, input: Vec<Envelope>) {
    uninterrupted.step(&input);
    restored.step(&input);
    same(uninterrupted, restored);
}

#[test]
fn draw_wire_requires_paid_tools_preserves_blocked_escrow_and_resumes_exactly() {
    let mut game = Workshop::standard();
    let rule = game.definitions.recipes[DRAW].clone();
    let tools = game.definitions.upgrades[TOOLS].clone();
    assert_eq!(game.definitions.content_version, 2);
    assert_eq!(rule.inputs, vec![("workshop:plate".into(), 2), ("workshop:coal".into(), 1)]);
    assert_eq!(rule.outputs, vec![("workshop:wire".into(), 3)]);
    assert_eq!(rule.required_upgrade.as_deref(), Some(TOOLS));
    assert_eq!(rule.duration_ticks, 10);
    assert_eq!(tools.cost, vec![("workshop:plate".into(), 1), ("workshop:coal".into(), 1)]);
    assert_eq!(tools.duration_divisor, 2);
    balances(&game, [8, 4, 0, 0, 0]);
    assert_eq!(accounted(&game.state), [1_000_008, 1_000_004, 0, 0, 0]);

    // Produce all plates through normal play on the two machines sharing inventory.
    act(&mut game, queue(A, "workshop:smelt"), true);
    act(&mut game, queue(B, "workshop:smelt"), true);
    balances(&game, [4, 2, 0, 0, 0]);
    assert_eq!(game.state.machines[A].queue[0].remaining_ticks, 4);
    assert_eq!(game.state.machines[B].queue[0].remaining_ticks, 5);
    assert_eq!(accounted(&game.state), [1_000_008, 1_000_004, 0, 0, 0]);
    for _ in 0..5 {
        game.step(&vec![]);
    }
    balances(&game, [4, 2, 4, 0, 0]);
    assert!(game.state.machines.values().all(|machine| machine.queue.is_empty()));
    assert_eq!(accounted(&game.state), [1_000_004, 1_000_002, 4, 0, 0]);

    // Inputs are available: the missing upgrade, rather than poverty, must reject this.
    let before = game.capture();
    act(&mut game, queue(A, DRAW), false);
    assert!(game.outcomes[0].detail.contains("WORKSHOP_UPGRADE_REQUIRED"));
    assert!(game.outcomes[0].detail.contains(TOOLS));
    assert_eq!(game.state.inventory, before.inventory);
    assert_eq!(game.state.supply, before.supply);
    assert_eq!(game.state.delivered, before.delivered);
    assert_eq!(game.state.machines, before.machines);

    act(&mut game, Command::Upgrade { machine: A.into(), upgrade: TOOLS.into() }, true);
    balances(&game, [4, 1, 3, 0, 0]);
    assert_eq!(game.state.machines[A].upgrades.iter().map(String::as_str).collect::<Vec<_>>(), vec![TOOLS]);
    assert!(game.state.machines[B].upgrades.is_empty());
    assert_eq!(accounted(&game.state), [1_000_004, 1_000_001, 3, 0, 0]);
    let paid_inventory = game.state.inventory.clone();
    act(&mut game, queue(B, DRAW), false);
    assert!(game.outcomes[0].detail.contains("WORKSHOP_UPGRADE_REQUIRED"));
    assert_eq!(game.state.inventory, paid_inventory);
    assert!(game.state.machines[B].queue.is_empty());

    act(&mut game, queue(A, DRAW), true);
    balances(&game, [4, 0, 1, 0, 0]);
    let job = &game.state.machines[A].queue[0];
    assert_eq!(game.state.machines[A].queue.len(), 1);
    assert_eq!(job.recipe, DRAW);
    assert_eq!(job.rule_version, 2);
    assert_eq!(job.speed_divisor, 2);
    assert_eq!(job.duration_ticks, 5);
    assert_eq!(job.remaining_ticks, 4);
    assert_eq!(job.inputs, rule.inputs);
    assert_eq!(job.outputs, rule.outputs);
    assert!(!job.cancel_pending);
    assert_eq!(accounted(&game.state), [1_000_004, 1_000_001, 3, 0, 0]);

    let fill = act(&mut game, Command::Gather { item: "workshop:ore".into(), amount: 7 }, true);
    balances(&game, [11, 0, 1, 0, 0]);
    assert_eq!(game.state.inventory.total_units(), game.state.inventory.max_units());
    assert_eq!(game.state.supply["workshop:ore"], 999_993);
    assert_eq!(game.state.supply["workshop:coal"], 1_000_000);
    assert_eq!(game.state.machines[A].queue[0].remaining_ticks, 3);

    // Load a fresh authority mid-production, then compare every continuing tick against
    // the uninterrupted run. Saved serials prevent a replayed gather from charging twice.
    let mut restored = reload(&game);
    advance_pair(&mut game, &mut restored, vec![]);
    assert_eq!(game.state.machines[A].queue[0].remaining_ticks, 2);
    advance_pair(&mut game, &mut restored, vec![fill]);
    assert!(game.outcomes.is_empty());
    balances(&game, [11, 0, 1, 0, 0]);
    assert_eq!(game.state.supply["workshop:ore"], 999_993);
    assert_eq!(game.state.machines[A].queue[0].remaining_ticks, 1);
    advance_pair(&mut game, &mut restored, vec![]);
    assert_eq!(game.state.machines[A].queue[0].remaining_ticks, 0);
    assert_eq!(game.state.machines[A].blocked.as_ref().unwrap().code, "RESOURCE-BUDGET");
    balances(&game, [11, 0, 1, 0, 0]);
    assert_eq!(accounted(&game.state), [1_000_004, 1_000_001, 3, 0, 0]);

    // A second engine save/load at blockage must retain escrow and rederive the cause.
    restored = reload(&restored);
    assert_eq!(restored.state, game.state);
    let pending = game.state.machines[A].queue.clone();
    let full_inventory = game.state.inventory.clone();
    for _ in 0..3 {
        advance_pair(&mut game, &mut restored, vec![]);
        assert_eq!(game.state.machines[A].queue, pending);
        assert_eq!(game.state.inventory, full_inventory);
        assert_eq!(accounted(&game.state), [1_000_004, 1_000_001, 3, 0, 0]);
    }

    // The next public intention resumes the saved serial and frees exactly the output space.
    let recovery = act(&mut game, Command::Deliver { item: "workshop:ore".into(), amount: 3 }, true);
    restored.step(&vec![recovery.clone()]);
    same(&game, &restored);
    balances(&game, [8, 0, 1, 0, 3]);
    assert_eq!(game.state.delivered["workshop:ore"], 3);
    assert_eq!(game.state.delivered.len(), 1);
    assert!(game.state.machines.values().all(|machine| machine.queue.is_empty() && machine.blocked.is_none()));
    assert_eq!(accounted(&game.state), [1_000_004, 1_000_000, 1, 0, 3]);

    advance_pair(&mut game, &mut restored, vec![recovery]);
    assert!(game.outcomes.is_empty());
    for _ in 0..3 {
        advance_pair(&mut game, &mut restored, vec![]);
    }
    balances(&game, [8, 0, 1, 0, 3]);
    assert_eq!(game.state.delivered["workshop:ore"], 3);
    assert_eq!(accounted(&game.state), [1_000_004, 1_000_000, 1, 0, 3]);
}
