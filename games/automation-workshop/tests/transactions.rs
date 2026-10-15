//! Gameplay acceptance uses public commands. Deliberate state forgery is isolated in refusal tests.
use automation_workshop::{content, Command, Definitions, Envelope, Recipe, State, Workshop, MAX_QUEUE};
use std::collections::BTreeMap;
use vesper3d::runtime::{snapshot, Simulation, Snapshot};

const A: &str = "workshop:machine-a";
const B: &str = "workshop:machine-b";
const SMELT: &str = "workshop:smelt";

fn queue(machine: &str) -> Command {
    Command::Queue { machine: machine.into(), recipe: SMELT.into() }
}

fn act(sim: &mut Workshop, command: Command) {
    let serial = sim.state.last_command.get(&0).copied().unwrap_or(0) + 1;
    sim.step(&vec![Envelope { seat: 0, serial, command }]);
    let outcome = sim.outcomes.first().expect("one outcome for a fresh player intention");
    assert!(outcome.ok, "{}", outcome.detail);
    assert_eq!((outcome.seat, outcome.serial), (0, serial));
}

fn wait(sim: &mut Workshop, ticks: u32) {
    for _ in 0..ticks {
        sim.step(&Vec::new());
    }
}

fn recipe(sim: &Workshop) -> Recipe {
    sim.definitions.recipes[SMELT].clone()
}

/// Sources, destinations and escrow are all included. A transfer among them must preserve totals;
/// only an explicitly completed recipe or paid upgrade changes this accounting vector.
fn owned_resources(state: &State) -> BTreeMap<String, u64> {
    let mut result = BTreeMap::new();
    for map in [state.inventory.counts(), &state.supply, &state.delivered] {
        for (id, amount) in map {
            *result.entry(id.clone()).or_default() += amount;
        }
    }
    for machine in state.machines.values() {
        for job in &machine.queue {
            for (id, amount) in &job.inputs {
                *result.entry(id.clone()).or_default() += amount;
            }
        }
    }
    result
}

fn converted(mut before: BTreeMap<String, u64>, rule: &Recipe, count: u64) -> BTreeMap<String, u64> {
    for (id, amount) in &rule.inputs {
        *before.get_mut(id).unwrap() -= amount * count;
    }
    for (id, amount) in &rule.outputs {
        *before.entry(id.clone()).or_default() += amount * count;
    }
    before
}

fn blocked_fixture() -> Workshop {
    let mut sim = Workshop::standard();
    let rule = recipe(&sim);
    assert!(rule.duration_ticks >= 3, "this fixture requires time to fill a running job's destination");
    act(&mut sim, queue(A));
    let free = sim.state.inventory.max_units() - sim.state.inventory.total_units();
    let source = rule.inputs.iter().find(|(id, _)| sim.state.supply.contains_key(id)).unwrap().0.clone();
    act(&mut sim, Command::Gather { item: source, amount: free });
    wait(&mut sim, rule.duration_ticks);
    let machine = &sim.state.machines[A];
    assert_eq!(machine.queue.len(), 1);
    assert_eq!(machine.queue[0].remaining_ticks, 0);
    assert_eq!(machine.blocked.as_ref().unwrap().code, "RESOURCE-BUDGET");
    sim
}

#[test]
fn machines_own_distinct_active_and_queued_jobs_and_cancellation_preserves_resources() {
    let mut sim = Workshop::standard();
    let rule = recipe(&sim);
    let initial = owned_resources(&sim.state);
    act(&mut sim, queue(A));
    act(&mut sim, queue(A));
    act(&mut sim, queue(B));
    assert_eq!(sim.state.machines[A].queue.len(), MAX_QUEUE);
    assert_eq!(sim.state.machines[B].queue.len(), 1);
    assert_eq!(sim.state.machines[A].queue[0].remaining_ticks, rule.duration_ticks - 3);
    assert_eq!(sim.state.machines[A].queue[1].remaining_ticks, rule.duration_ticks);
    assert_eq!(sim.state.machines[B].queue[0].remaining_ticks, rule.duration_ticks - 1);
    assert_eq!(owned_resources(&sim.state), initial);

    act(&mut sim, Command::Cancel { machine: A.into() });
    assert_eq!(sim.state.machines[A].queue.len(), 1);
    assert_eq!(sim.state.machines[A].queue[0].remaining_ticks, rule.duration_ticks);
    assert_eq!(sim.state.machines[B].queue[0].remaining_ticks, rule.duration_ticks - 2);
    assert_eq!(owned_resources(&sim.state), initial);

    wait(&mut sim, rule.duration_ticks * 2);
    assert!(sim.state.machines.values().all(|machine| machine.queue.is_empty()));
    assert_eq!(owned_resources(&sim.state), converted(initial, &rule, 2));
}

#[test]
fn blocked_output_retains_escrow_and_delivering_space_commits_exactly_one_recipe() {
    let initial = owned_resources(&Workshop::standard().state);
    let mut sim = blocked_fixture();
    let rule = recipe(&sim);
    let pending = sim.state.machines[A].queue[0].clone();
    let inventory = sim.state.inventory.clone();
    wait(&mut sim, 3);
    assert_eq!(sim.state.machines[A].queue[0], pending);
    assert_eq!(sim.state.inventory, inventory);
    assert_eq!(owned_resources(&sim.state), initial);

    let needed: u64 = rule.outputs.iter().map(|(_, n)| n).sum();
    let source = rule.inputs.iter().find(|(id, _)| sim.definitions.deliverable.contains(id)).unwrap().0.clone();
    act(&mut sim, Command::Deliver { item: source.clone(), amount: needed });
    assert!(sim.state.machines[A].queue.is_empty());
    assert_eq!(sim.state.delivered[&source], needed);
    assert_eq!(owned_resources(&sim.state), converted(initial, &rule, 1));
    let inventory = sim.state.inventory.clone();
    wait(&mut sim, 3);
    assert_eq!(sim.state.inventory, inventory, "completed output must never apply twice");
}

#[test]
fn full_cancellation_remains_pending_and_recovers_without_losing_or_refunding_twice() {
    let initial = owned_resources(&Workshop::standard().state);
    let mut sim = blocked_fixture();
    let pending = sim.state.machines[A].queue[0].clone();
    let inventory = sim.state.inventory.clone();
    act(&mut sim, Command::Cancel { machine: A.into() });
    wait(&mut sim, 3);
    let job = &sim.state.machines[A].queue[0];
    assert!(job.cancel_pending);
    assert_eq!(job.inputs, pending.inputs);
    assert_eq!(job.outputs, pending.outputs);
    assert_eq!(sim.state.inventory, inventory);
    assert_eq!(sim.state.machines[A].blocked.as_ref().unwrap().code, "RESOURCE-BUDGET");
    assert_eq!(owned_resources(&sim.state), initial);

    let refund: u64 = pending.inputs.iter().map(|(_, n)| n).sum();
    act(&mut sim, Command::Deliver { item: pending.inputs[0].0.clone(), amount: refund });
    assert!(sim.state.machines[A].queue.is_empty());
    assert_eq!(owned_resources(&sim.state), initial);
    let inventory = sim.state.inventory.clone();
    wait(&mut sim, 3);
    assert_eq!(sim.state.inventory, inventory, "one escrow permits only one refund");
}

#[test]
fn blocked_diagnostic_is_derived_after_engine_save_restore_and_traces_authored_rule() {
    let sim = blocked_fixture();
    let saved = snapshot::save(&sim, "blocked production").unwrap();
    let mut restored = Workshop::standard();
    snapshot::restore(&mut restored, &saved).unwrap();
    assert_eq!(restored.state_hash(), sim.state_hash());
    let view = restored.inspect(A).unwrap();
    assert_eq!(view["reason"]["code"], "RESOURCE-BUDGET");
    assert_eq!(view["rule"], SMELT);
    assert_eq!(view["definition"]["id"], SMELT);
    assert!(view["definition"]["pointer"].as_str().unwrap().starts_with("/recipes/"));
    assert!(view["definition"]["source"].as_str().unwrap().ends_with("workshop.json"));
    let serialized = serde_json::to_value(restored.capture()).unwrap();
    assert!(serialized["machines"][A].get("blocked").is_none(), "derived diagnostics are not save authority");
}

#[test]
fn blocked_inspection_uses_final_shared_store_after_later_machine_output() {
    let mut sim = Workshop::standard();
    let smelt = recipe(&sim);
    let gears = sim.definitions.recipes["workshop:gears"].clone();
    act(&mut sim, queue(A));
    wait(&mut sim, smelt.duration_ticks);
    act(&mut sim, Command::Queue { machine: B.into(), recipe: gears.id.clone() });
    // Arrange both real productions to finish on the same fixed tick, with A processed before B.
    let delay = gears.duration_ticks.checked_sub(smelt.duration_ticks).unwrap();
    assert!(delay > 0);
    wait(&mut sim, delay - 1);
    act(&mut sim, queue(A));
    let fill = sim.state.inventory.max_units() - 1 - sim.state.inventory.total_units();
    act(&mut sim, Command::Gather { item: "workshop:ore".into(), amount: fill });
    let remaining = sim.state.machines[A].queue[0].remaining_ticks;
    wait(&mut sim, remaining); // inspect immediately on the shared completion tick
    assert!(sim.state.machines[B].queue.is_empty());
    assert_eq!(sim.state.inventory.total_units(), sim.state.inventory.max_units());
    let required = sim.state.inventory.total_units() + smelt.outputs.iter().map(|(_, n)| n).sum::<u64>();
    assert_eq!(sim.state.machines[A].blocked.as_ref().unwrap().required, Some(required));
    assert_eq!(sim.inspect(A).unwrap()["reason"]["required"], required);
    let bytes = snapshot::save(&sim, "contending machine outputs").unwrap();
    let mut restored = Workshop::standard();
    snapshot::restore(&mut restored, &bytes).unwrap();
    assert_eq!(
        restored.capture(),
        sim.capture(),
        "restore and immediate inspection must explain the same blocked state"
    );
}

#[test]
fn rejected_public_actions_leave_inventory_sources_jobs_and_upgrades_unchanged() {
    let mut sim = Workshop::standard();
    for command in [
        Command::Gather { item: "workshop:ore".into(), amount: 1 }, // full initial inventory
        Command::Gather { item: "workshop:unknown".into(), amount: 1 },
        Command::Deliver { item: "workshop:ore".into(), amount: 100 },
        Command::Queue { machine: "workshop:unknown".into(), recipe: SMELT.into() },
        Command::Queue { machine: A.into(), recipe: "workshop:unknown".into() },
        Command::Upgrade { machine: A.into(), upgrade: "workshop:fast-tools".into() }, // no plates
    ] {
        let before = sim.capture();
        assert!(sim.command(&command).is_err());
        assert_eq!(sim.capture(), before);
    }
    for _ in 0..MAX_QUEUE {
        sim.command(&queue(A)).unwrap();
    }
    let before = sim.capture();
    assert!(sim.command(&queue(A)).unwrap_err().contains("WORKSHOP_QUEUE_FULL"));
    assert_eq!(sim.capture(), before);
}

#[test]
fn conflict_rejects_entire_batch_in_any_order_while_production_time_continues() {
    let mut sim = Workshop::standard();
    act(&mut sim, queue(A));
    let mut expected = sim.clone();
    expected.step(&Vec::new());
    let batch = vec![
        Envelope { seat: 0, serial: 2, command: Command::Gather { item: "workshop:ore".into(), amount: 1 } },
        Envelope { seat: 0, serial: 2, command: Command::Wait },
        Envelope { seat: 1, serial: 1, command: Command::Cancel { machine: A.into() } },
    ];
    let mut reversed = sim.clone();
    let mut reverse_batch = batch.clone();
    reverse_batch.reverse();
    sim.step(&batch);
    reversed.step(&reverse_batch);
    assert_eq!(sim.state, expected.state);
    assert_eq!(reversed.state, expected.state);
    assert_eq!(sim.outcomes, reversed.outcomes);
    assert_eq!(sim.outcomes.len(), 1);
    assert!(!sim.outcomes[0].ok);
    assert!(sim.outcomes[0].detail.contains("WORKSHOP_COMMAND_CONFLICT"));
    assert_eq!(sim.state.last_command.get(&0), Some(&1));
    assert!(!sim.state.last_command.contains_key(&1));
}

#[test]
fn identical_duplicates_and_replays_apply_one_action_and_valid_batches_order_by_identity() {
    let mut sim = Workshop::standard();
    let input = Envelope { seat: 0, serial: 1, command: queue(A) };
    sim.step(&vec![input.clone(), input.clone()]);
    assert_eq!(sim.outcomes.len(), 1);
    assert!(sim.outcomes[0].ok);
    assert_eq!(sim.state.machines[A].queue.len(), 1);
    let inventory = sim.state.inventory.clone();
    sim.step(&vec![input]);
    assert!(sim.outcomes.is_empty());
    assert_eq!(sim.state.inventory, inventory);
    assert_eq!(sim.state.machines[A].queue.len(), 1);

    let mut first = Workshop::standard();
    let mut second = first.clone();
    let batch =
        vec![Envelope { seat: 1, serial: 1, command: queue(B) }, Envelope { seat: 0, serial: 1, command: queue(A) }];
    let mut reversed = batch.clone();
    reversed.reverse();
    first.step(&batch);
    second.step(&reversed);
    assert_eq!(first.state_hash(), second.state_hash());
    assert_eq!(first.outcomes, second.outcomes);
}

#[test]
fn unowned_zero_or_forged_speed_refuses_restore_without_changing_playable_state() {
    let mut sim = Workshop::standard();
    act(&mut sim, queue(A));
    let before = sim.capture();
    let rule = recipe(&sim);
    for speed in [0, 2, 3, 4] {
        let mut forged = before.clone();
        let job = &mut forged.machines.get_mut(A).unwrap().queue[0];
        job.speed_divisor = speed;
        if speed > 0 {
            job.duration_ticks = rule.duration_ticks.div_ceil(speed);
            job.remaining_ticks = job.duration_ticks;
        }
        assert!(sim.restore(forged).unwrap_err().contains("WORKSHOP_JOB_STATE"));
        assert_eq!(sim.capture(), before);
    }
}

#[test]
fn missing_required_upgrade_refuses_forged_job_and_preserves_original_state() {
    let mut original = Workshop::standard();
    act(&mut original, queue(A));
    let forged = original.capture();
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/workshop.json")).unwrap();
    let mut content: serde_json::Value = serde_json::from_str(&text).unwrap();
    let rule = content["recipes"].as_array_mut().unwrap().iter_mut().find(|r| r["id"] == SMELT).unwrap();
    rule["required_upgrade"] = serde_json::json!("workshop:fast-tools");
    let defs =
        Definitions::from_json(&serde_json::to_string(&content).unwrap(), "fixture:required-upgrade.json").unwrap();
    let mut restricted = Workshop::new(defs, 2).unwrap();
    let before = restricted.capture();
    assert!(restricted.restore(forged).unwrap_err().contains("WORKSHOP_JOB_UPGRADE_REQUIRED"));
    assert_eq!(restricted.capture(), before);
}

#[test]
fn later_upgrade_preserves_started_speed_and_new_work_uses_game_owned_tuning() {
    let mut sim = Workshop::standard();
    let rule = recipe(&sim);
    act(&mut sim, queue(A));
    wait(&mut sim, rule.duration_ticks);
    act(&mut sim, queue(A));
    act(&mut sim, Command::Upgrade { machine: A.into(), upgrade: "workshop:fast-tools".into() });
    assert_eq!(sim.state.machines[A].queue[0].speed_divisor, 1, "started production retains its frozen rule");
    let state = sim.capture();
    sim.restore(state).unwrap();
    act(&mut sim, queue(B));
    let plain_duration = sim.state.machines[B].queue[0].duration_ticks;
    wait(&mut sim, rule.duration_ticks * 2);
    // Free inventory through normal delivery before preparing another recipe.
    let output = rule.outputs[0].0.clone();
    let count = sim.state.inventory.quantity(&output);
    if count > 0 {
        act(&mut sim, Command::Deliver { item: output, amount: count });
    }
    for (id, amount) in &rule.inputs {
        let available = sim.state.inventory.quantity(id);
        if available < *amount {
            act(&mut sim, Command::Gather { item: id.clone(), amount: amount - available });
        }
    }
    act(&mut sim, queue(A));
    let job = &sim.state.machines[A].queue[0];
    let divider = sim.definitions.upgrades["workshop:fast-tools"].duration_divisor;
    assert_eq!(job.speed_divisor, divider);
    assert_eq!(job.duration_ticks, rule.duration_ticks.div_ceil(divider));
    assert!(job.duration_ticks < plain_duration);
}

#[test]
fn source_aware_content_addition_replaces_existing_unicode_path_and_drives_real_rules() {
    let directory =
        std::env::temp_dir().join(format!("workshop edit λ-{}-{:?}", std::process::id(), std::thread::current().id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workshop content.json");
    let original = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/workshop.json")).unwrap();
    std::fs::write(&path, original).unwrap();
    let addition = serde_json::json!({
        "upgrades": [{"id":"workshop:fixture-wire-tools", "label":"Wire tools λ", "cost":[["workshop:plate",1]], "duration_divisor":3}],
        "recipes": [{"id":"workshop:fixture-wire", "label":"Wind wire", "duration_ticks":5, "inputs":[["workshop:plate",1]], "outputs":[["workshop:wire",1]], "required_upgrade":"workshop:fixture-wire-tools"}]
    });
    let diff = content::edit(&path, &addition).unwrap();
    assert_ne!(diff["before"], diff["after"]);
    assert_eq!(diff["added_recipes"], serde_json::json!(["workshop:fixture-wire"]));
    assert_eq!(diff["added_upgrades"], serde_json::json!(["workshop:fixture-wire-tools"]));
    let mut sim = Workshop::new(Definitions::load(&path).unwrap(), 2).unwrap();
    let command = Command::Queue { machine: A.into(), recipe: "workshop:fixture-wire".into() };
    let before = sim.capture();
    assert!(sim.command(&command).unwrap_err().contains("WORKSHOP_UPGRADE_REQUIRED"));
    assert_eq!(sim.capture(), before);
    let smelt_ticks = recipe(&sim).duration_ticks;
    act(&mut sim, queue(A));
    wait(&mut sim, smelt_ticks);
    act(&mut sim, Command::Upgrade { machine: A.into(), upgrade: "workshop:fixture-wire-tools".into() });
    act(&mut sim, command);
    let view = sim.inspect(A).unwrap();
    assert_eq!(view["rule"], "workshop:fixture-wire");
    assert_eq!(view["definition"]["source"], path.display().to_string());
    assert_eq!(sim.state.machines[A].queue[0].duration_ticks, 5u32.div_ceil(3));
    wait(&mut sim, 5);
    assert!(sim.state.machines[A].queue.is_empty());
    assert_eq!(sim.state.inventory.quantity("workshop:wire"), 1);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn unresolved_content_addition_reports_exact_source_and_preserves_original_bytes() {
    let directory = std::env::temp_dir().join(format!(
        "workshop unresolved-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workshop content.json");
    let original = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/workshop.json")).unwrap();
    std::fs::write(&path, &original).unwrap();
    let addition = serde_json::json!({"recipes":[{
        "id":"workshop:fixture-unresolved", "label":"Broken reference", "duration_ticks":1,
        "inputs":[["workshop:missing",1]], "outputs":[["workshop:wire",1]], "required_upgrade":null
    }]});
    let error = content::edit(&path, &addition).unwrap_err();
    assert!(error.contains("WORKSHOP_ITEM_REFERENCE"), "{error}");
    assert!(error.contains(&path.display().to_string()), "{error}");
    assert!(error.contains("/inputs/0"), "{error}");
    assert!(error.contains("workshop:missing"), "{error}");
    assert_eq!(std::fs::read(path).unwrap(), original);
    std::fs::remove_dir_all(directory).unwrap();
}
