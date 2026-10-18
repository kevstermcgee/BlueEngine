//! Bounded accounting and replay over the real public Workshop authority; no renderer.
use crate::{Command, Envelope, State, Workshop};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vesper3d::runtime::{snapshot, Simulation, Snapshot};
#[derive(Clone, Serialize, Deserialize)]
pub struct Turn {
    pub input: Vec<Envelope>,
    pub reload: bool,
}
#[derive(Serialize, Deserialize)]
pub struct Trace {
    pub seed: u64,
    pub content: String,
    pub turns: Vec<Turn>,
    pub fault: bool,
}
#[derive(Serialize)]
pub struct Report {
    pub ok: bool,
    pub ticks: usize,
    pub hash: String,
    pub failure: Option<String>,
    pub trace: Trace,
}
fn total(state: &State) -> BTreeMap<String, u64> {
    let mut total = BTreeMap::new();
    for map in [state.inventory.counts(), &state.supply, &state.delivered] {
        for (id, n) in map {
            *total.entry(id.clone()).or_default() += n;
        }
    }
    for machine in state.machines.values() {
        for job in &machine.queue {
            for (id, n) in &job.inputs {
                *total.entry(id.clone()).or_default() += n;
            }
        }
    }
    total
}
fn delta(total: &mut BTreeMap<String, u64>, terms: &[(String, u64)], credit: bool) {
    for (id, n) in terms {
        let value = total.entry(id.clone()).or_default();
        if credit {
            *value += n;
        } else {
            *value -= n;
        }
    }
}
fn checked_step(game: &mut Workshop, turn: &Turn, fault: bool) -> Result<(), String> {
    let before = game.capture();
    let mut expected = total(&before);
    // Differential duplicate check uses the same implementation, with one copy per command.
    let mut once = game.clone();
    let mut unique = turn.input.clone();
    unique.dedup();
    once.step(&unique);
    game.step(&turn.input);
    if fault {
        if let Some(Command::Gather { item, amount }) = turn.input.first().map(|e| &e.command) {
            let _ = game.command(&Command::Gather { item: item.clone(), amount: *amount });
        }
    }
    if game.state_hash() != once.state_hash() {
        return Err("duplicate command changed authoritative state".into());
    }
    for outcome in &game.outcomes {
        if !outcome.ok {
            return Err(format!("generated illegal action: {}", outcome.detail));
        }
        if let Some(Command::Upgrade { upgrade, .. }) =
            turn.input.iter().find(|e| e.serial == outcome.serial).map(|e| &e.command)
        {
            delta(&mut expected, &game.definitions.upgrades[upgrade].cost, false);
        }
    }
    // Only completed production changes the source+inventory+delivery+escrow vector.
    // Inspect removed jobs; this is an accounting oracle, not a production simulation.
    for (id, machine) in &before.machines {
        if let Some(job) = machine.queue.first() {
            let queued = turn
                .input
                .iter()
                .filter(|e| matches!(&e.command,Command::Queue{machine,..} if machine==id))
                .map(|e| e.serial)
                .collect::<std::collections::BTreeSet<_>>()
                .len();
            if game.state.machines[id].queue.len() < machine.queue.len() + queued {
                let cancelled = job.cancel_pending
                    || turn.input.iter().any(|e| matches!(&e.command,Command::Cancel{machine} if machine==id));
                if !cancelled {
                    delta(&mut expected, &job.inputs, false);
                    delta(&mut expected, &job.outputs, true);
                }
            } else if !job.cancel_pending && job.remaining_ticks > 0 {
                let after = &game.state.machines[id].queue[0];
                if !after.cancel_pending && after.remaining_ticks + 1 != job.remaining_ticks {
                    return Err("active production stalled".into());
                }
            }
        }
    }
    if expected != total(&game.state) {
        return Err("resource conservation or duplication violation".into());
    }
    game.validate_state(&game.state)?;
    if turn.reload {
        let hash = game.state_hash();
        let bytes = snapshot::save(game, "Parallel playtest checkpoint").map_err(|e| e.to_string())?;
        let mut restored = Workshop::standard();
        snapshot::restore(&mut restored, &bytes).map_err(|e| e.to_string())?;
        if restored.state_hash() != hash {
            return Err("save/reload diverged".into());
        }
        *game = restored;
    }
    Ok(())
}
fn command(kind: &str) -> Command {
    let a = "workshop:machine-a".to_string();
    match kind {
        "queue-a" => Command::Queue { machine: a, recipe: "workshop:smelt".into() },
        "queue-b" => Command::Queue { machine: "workshop:machine-b".into(), recipe: "workshop:smelt".into() },
        "ore" => Command::Gather { item: "workshop:ore".into(), amount: 4 },
        "coal" => Command::Gather { item: "workshop:coal".into(), amount: 2 },
        "cancel" => Command::Cancel { machine: a },
        "deliver" => Command::Deliver { item: "workshop:ore".into(), amount: 6 },
        "upgrade" => Command::Upgrade { machine: a, upgrade: "workshop:wire-tools".into() },
        "wire" => Command::Queue { machine: a, recipe: "workshop:draw-wire".into() },
        "finish" => Command::Deliver { item: "workshop:wire".into(), amount: 3 },
        _ => Command::Wait,
    }
}
fn prefix() -> Vec<Command> {
    [
        "queue-a", "queue-b", "ore", "coal", "wait", "wait", "wait", "wait", "cancel", "deliver", "upgrade", "queue-a",
        "wait", "wait", "wait", "wire", "wait", "wait", "wait", "wait", "wait", "finish",
    ]
    .into_iter()
    .map(command)
    .collect()
}
fn candidate(game: &Workshop, random: &mut u64) -> Command {
    *random = random.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    let mut commands = vec![Command::Wait];
    for item in game.definitions.sources.keys() {
        commands.push(Command::Gather { item: item.clone(), amount: 1 });
    }
    for item in &game.definitions.deliverable {
        commands.push(Command::Deliver { item: item.clone(), amount: 1 });
    }
    for machine in game.state.machines.keys() {
        commands.push(Command::Cancel { machine: machine.clone() });
        for recipe in game.definitions.recipes.keys() {
            commands.push(Command::Queue { machine: machine.clone(), recipe: recipe.clone() });
        }
        for upgrade in game.definitions.upgrades.keys() {
            commands.push(Command::Upgrade { machine: machine.clone(), upgrade: upgrade.clone() });
        }
    }
    let start = (*random as usize) % commands.len();
    for offset in 0..commands.len() {
        let cmd = &commands[(start + offset) % commands.len()];
        // Legal actions are selected by the real command validator on an isolated candidate.
        if game.clone().command(cmd).is_ok() {
            return cmd.clone();
        }
    }
    Command::Wait
}
pub fn run(seed: u64, ticks: usize, fault: bool) -> Report {
    let mut game = Workshop::standard();
    let mut trace = Trace { seed, content: game.definitions.fingerprint(), turns: Vec::new(), fault };
    let mut uninterrupted = game.clone();
    let mut random = seed;
    let prefix = prefix();
    let mut failure = None;
    for tick in 0..ticks {
        let command = prefix.get(tick).cloned().unwrap_or_else(|| candidate(&game, &mut random));
        let envelope = Envelope { seat: 0, serial: tick as u64 + 1, command };
        let input = if tick % 7 == 0 { vec![envelope.clone(), envelope] } else { vec![envelope] };
        let turn = Turn { input, reload: tick % 31 == 4 };
        let result = checked_step(&mut game, &turn, fault);
        uninterrupted.step(&turn.input);
        let result = result.and_then(|()| {
            if uninterrupted.state_hash() == game.state_hash() {
                Ok(())
            } else {
                Err("save/reload continuation diverged".into())
            }
        });
        trace.turns.push(turn);
        if let Err(e) = result {
            failure = Some(e);
            break;
        }
    }
    Report {
        ok: failure.is_none(),
        ticks: trace.turns.len(),
        hash: format!("{:016x}", game.state_hash()),
        failure,
        trace,
    }
}
pub fn replay(trace: Trace) -> Report {
    let mut game = Workshop::standard();
    let mut failure =
        if trace.content != game.definitions.fingerprint() { Some("content identity differs".into()) } else { None };
    if trace.turns.len() > 100000 || trace.turns.iter().any(|t| t.input.len() > crate::MAX_COMMANDS) {
        failure = Some("trace exceeds bounded action/tick limits".into());
    }
    let mut ticks = 0;
    if failure.is_none() {
        for turn in &trace.turns {
            ticks += 1;
            if let Err(e) = checked_step(&mut game, turn, trace.fault) {
                failure = Some(e);
                break;
            }
        }
    }
    Report { ok: failure.is_none(), ticks, hash: format!("{:016x}", game.state_hash()), failure, trace }
}
#[cfg(test)]
mod tests {
    #[test]
    fn real_game_passes_and_deliberate_duplicate_fixture_fails_replayably() {
        for seed in [0, 1, 4, 6, 7, 42] {
            let report = super::run(seed, 10000, false);
            assert!(report.ok, "{:?}", report.failure);
            let replay = super::replay(report.trace);
            assert!(replay.ok);
            assert_eq!(replay.hash, report.hash);
        }
        let broken = super::run(7, 100, true);
        assert!(!broken.ok);
        let replay = super::replay(broken.trace);
        assert_eq!(replay.failure, broken.failure);
        assert_eq!(replay.ticks, broken.ticks);
    }
}
