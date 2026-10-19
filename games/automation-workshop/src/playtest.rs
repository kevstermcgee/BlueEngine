//! Bounded accounting and replay over the real public Workshop authority; no renderer.
use crate::{Command, Definitions, Envelope, State, Workshop};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vesper3d::runtime::{snapshot, Simulation, Snapshot};
#[derive(Clone, Serialize, Deserialize)]
pub struct Turn {
    pub input: Vec<Envelope>,
    pub reload: bool,
    #[serde(default)]
    pub migrate: bool,
}
#[derive(Serialize, Deserialize)]
pub struct Trace {
    pub seed: u64,
    pub content: String,
    pub turns: Vec<Turn>,
    pub fault: bool,
}
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct Coverage {
    pub completions: usize,
    pub refunds: usize,
    pub blocked_production: usize,
    pub blocked_refunds: usize,
    pub delivery_recoveries: usize,
    pub reloads: usize,
    pub migrations: usize,
}
#[derive(Serialize)]
pub struct Checkpoint {
    pub tick: u64,
    pub operation: &'static str,
    pub source_content: String,
    pub target_content: Option<String>,
    pub original_hex: String,
    pub migrated_hex: Option<String>,
    pub evidence_directory: Option<String>,
}
#[derive(Serialize)]
pub struct Report {
    pub ok: bool,
    pub ticks: usize,
    pub hash: String,
    pub failure: Option<String>,
    pub trace: Trace,
    pub coverage: Coverage,
    pub checkpoints: Vec<Checkpoint>,
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
#[derive(Clone, Copy)]
enum Fault {
    None,
    Duplicate,
    #[cfg(test)]
    StallCompletion,
    #[cfg(test)]
    StallRefund,
}
fn checked_step(
    game: &mut Workshop,
    turn: &Turn,
    fault: Fault,
    coverage: &mut Coverage,
    checkpoints: &mut Vec<Checkpoint>,
) -> Result<(), String> {
    let before = game.capture();
    let mut expected = total(&before);
    let mut commands = game.clone();
    // Differential duplicate check uses the same implementation, with one copy per command.
    let mut once = game.clone();
    let mut unique = turn.input.clone();
    unique.sort_by_key(|e| (e.seat, e.serial));
    unique.dedup();
    once.step(&unique);
    game.step(&turn.input);
    if matches!(fault, Fault::Duplicate) {
        if let Some(Command::Gather { item, amount }) = turn.input.first().map(|e| &e.command) {
            let _ = game.command(&Command::Gather { item: item.clone(), amount: *amount });
        }
    }
    if game.state_hash() != once.state_hash() {
        return Err("duplicate command changed authoritative state".into());
    }
    // Observe the command boundary through public commands, without advancing a second clock or
    // implementing production. This includes newly queued work and command-created capacity.
    for outcome in &game.outcomes {
        if !outcome.ok {
            return Err(format!("generated illegal action: {}", outcome.detail));
        }
        let input = unique
            .iter()
            .find(|e| (e.seat, e.serial) == (outcome.seat, outcome.serial))
            .ok_or("outcome has no player command")?;
        commands.command(&input.command)?;
        if let Command::Upgrade { upgrade, .. } = &input.command {
            delta(&mut expected, &game.definitions.upgrades[upgrade].cost, false);
        }
    }
    #[cfg(test)]
    inject_stall(game, &commands, fault)?;
    // A step visits exactly one head job per machine in stable ID order. Preview prerequisites
    // against the command-boundary inventory plus credits from ACTUAL earlier removals. This is
    // an accounting observation, not another production simulation or an equivalence-only oracle.
    let mut committed = Vec::new();
    for (id, machine) in &commands.state.machines {
        let after = &game.state.machines[id];
        let Some(job) = machine.queue.first() else {
            if !after.queue.is_empty() {
                return Err(format!("unexpected queued work: {id}"));
            }
            continue;
        };
        let removed = after.queue.len() + 1 == machine.queue.len();
        let ready = job.cancel_pending || job.remaining_ticks <= 1;
        let credits = if job.cancel_pending { &job.inputs } else { &job.outputs };
        let mut attempt = committed.clone();
        attempt.extend_from_slice(credits);
        let fits = commands.state.inventory.preview(&[], &attempt).is_ok();
        if removed {
            if !ready || !fits {
                return Err(format!("work completed before its prerequisites: {id}"));
            }
            if after.queue != machine.queue[1..] {
                return Err(format!("queued successor advanced before the next tick: {id}"));
            }
            committed.extend_from_slice(credits);
            if job.cancel_pending {
                coverage.refunds += 1;
            } else {
                coverage.completions += 1;
                delta(&mut expected, &job.inputs, false);
                delta(&mut expected, &job.outputs, true);
            }
            if before.machines[id].blocked.is_some()
                && turn.input.iter().any(|e| matches!(e.command, Command::Deliver { .. }))
            {
                coverage.delivery_recoveries += 1;
            }
        } else {
            if after.queue.len() != machine.queue.len() {
                return Err(format!("queue changed unexpectedly: {id}"));
            }
            if ready && fits {
                return Err(format!(
                    "{} stalled despite available capacity: {id}",
                    if job.cancel_pending { "pending cancellation refund" } else { "ready production" }
                ));
            }
            let mut retained = job.clone();
            if !retained.cancel_pending {
                retained.remaining_ticks = retained.remaining_ticks.saturating_sub(1);
            }
            if after.queue[0] != retained || after.queue[1..] != machine.queue[1..] {
                return Err(format!("active work stalled or blocked escrow changed: {id}"));
            }
            // Diagnostics intentionally describe FINAL shared inventory, including later machines.
            let cause = if ready { game.state.inventory.preview(&[], credits).err() } else { None };
            if after.blocked != cause {
                return Err(format!("incorrect blockage cause: {id}"));
            }
            if ready {
                if job.cancel_pending {
                    coverage.blocked_refunds += 1;
                } else {
                    coverage.blocked_production += 1;
                }
            }
        }
    }
    if expected != total(&game.state) {
        return Err("resource conservation or duplication violation".into());
    }
    game.validate_state(&game.state)?;
    if turn.reload || turn.migrate {
        checkpoint(game, turn.migrate, coverage, checkpoints)?;
    }
    Ok(())
}
#[cfg(test)]
fn inject_stall(game: &mut Workshop, commands: &Workshop, fault: Fault) -> Result<(), String> {
    // Test-only broken authority: reverse one actual completion/refund while retaining all escrow.
    // Accounting and state validation still pass; only the direct progress assertion must fail.
    for (id, machine) in &commands.state.machines {
        let Some(job) = machine.queue.first() else {
            continue;
        };
        let selected = matches!(fault, Fault::StallCompletion) && !job.cancel_pending
            || matches!(fault, Fault::StallRefund) && job.cancel_pending;
        if selected && game.state.machines[id].queue.len() + 1 == machine.queue.len() {
            let credits = if job.cancel_pending { &job.inputs } else { &job.outputs };
            game.state.inventory.transact(credits, &[]).map_err(|e| e.to_string())?;
            let mut retained = job.clone();
            if !retained.cancel_pending {
                retained.remaining_ticks = retained.remaining_ticks.saturating_sub(1);
            }
            game.state.machines.get_mut(id).unwrap().queue.insert(0, retained);
            game.recompute_blocked();
            break;
        }
    }
    Ok(())
}
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(out, "{byte:02x}").expect("write string");
    }
    out
}
fn checkpoint(
    game: &mut Workshop,
    migrate: bool,
    coverage: &mut Coverage,
    checkpoints: &mut Vec<Checkpoint>,
) -> Result<(), String> {
    let hash = game.state_hash();
    let bytes = snapshot::save(game, "Parallel playtest checkpoint").map_err(|e| e.to_string())?;
    checkpoints.push(Checkpoint {
        tick: game.state.tick,
        operation: if migrate { "content-migration" } else { "exact-reload" },
        source_content: game.definitions.fingerprint(),
        target_content: None,
        original_hex: hex(&bytes),
        migrated_hex: None,
        evidence_directory: None,
    });
    let evidence = checkpoints.last_mut().unwrap();
    let mut restored = Workshop::new(game.definitions.clone(), 2)?;
    snapshot::restore(&mut restored, &bytes).map_err(|e| e.to_string())?;
    if restored.state_hash() != hash {
        return Err("save/reload diverged".into());
    }
    coverage.reloads += 1;
    if migrate {
        // A supported explicit label migration changes complete content identity, never paid rules.
        let mut content: serde_json::Value =
            serde_json::from_str(include_str!("../assets/workshop.json")).map_err(|e| e.to_string())?;
        content["recipes"][0]["label"] = format!("Playtest smelt checkpoint {}", game.state.tick).into();
        let target =
            Definitions::from_json(&content.to_string(), "playtest:compatible-labels").map_err(|e| e.to_string())?;
        let mut migrated = Workshop::new(target, 2)?;
        evidence.target_content = Some(migrated.definitions.fingerprint());
        if snapshot::restore(&mut migrated, &bytes).is_ok() {
            return Err("exact restore accepted changed content".into());
        }
        static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nonce =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "workshop-playtest-{}-{nonce}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
        evidence.evidence_directory = Some(directory.display().to_string());
        std::fs::write(directory.join("original.be2save"), &bytes).map_err(|e| e.to_string())?;
        crate::persistence::migrate_content(&mut migrated, &bytes, game.definitions.clone(), &directory, "migrated")?;
        let completed = std::fs::read(directory.join("migrated.be2save")).map_err(|e| e.to_string())?;
        evidence.migrated_hex = Some(hex(&completed));
        if std::fs::read(directory.join("original.be2save")).map_err(|e| e.to_string())? != bytes
            || migrated.state_hash() != hash
            || migrated.capture() != game.capture()
        {
            return Err("migration changed original or paid in-flight state".into());
        }
        // Reusing a completed destination must fail without changing either copy or live state.
        if crate::persistence::migrate_content(&mut migrated, &bytes, game.definitions.clone(), &directory, "migrated")
            .is_ok()
            || std::fs::read(directory.join("migrated.be2save")).map_err(|e| e.to_string())? != completed
        {
            return Err("migration overwrote an existing destination".into());
        }
        let mut reloaded = Workshop::new(migrated.definitions.clone(), 2)?;
        snapshot::restore(&mut reloaded, &completed).map_err(|e| e.to_string())?;
        if reloaded.state_hash() != hash {
            return Err("migrated reload diverged".into());
        }
        restored = reloaded;
        coverage.migrations += 1;
        // Completed bytes remain in the report; failed operations retain this directory as well.
        std::fs::remove_dir_all(&directory).map_err(|e| e.to_string())?;
        evidence.evidence_directory = None;
    }
    *game = restored;
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
#[derive(Default)]
struct Recovery {
    phase: usize,
    refund: bool,
}
impl Recovery {
    fn next(&mut self, game: &Workshop) -> Option<Command> {
        let machine = "workshop:machine-a";
        loop {
            match self.phase {
                0 => return None,
                1 => {
                    if let Some((id, _)) =
                        game.state.machines.iter().find(|(_, m)| m.queue.first().is_some_and(|j| !j.cancel_pending))
                    {
                        return Some(Command::Cancel { machine: id.clone() });
                    }
                    if let Some((item, amount)) = game.state.inventory.counts().iter().find(|(_, n)| **n > 0) {
                        return Some(Command::Deliver { item: item.clone(), amount: *amount });
                    }
                    if game.state.machines.values().any(|m| !m.queue.is_empty()) {
                        return Some(Command::Wait);
                    }
                    self.phase = 2;
                }
                2 => {
                    self.phase += 1;
                    return Some(Command::Gather { item: "workshop:ore".into(), amount: 8 });
                }
                3 => {
                    self.phase += 1;
                    return Some(Command::Gather { item: "workshop:coal".into(), amount: 4 });
                }
                4 => {
                    self.phase += 1;
                    return Some(command("queue-a"));
                }
                5 => {
                    self.phase += 1;
                    return Some(Command::Gather { item: "workshop:ore".into(), amount: 3 });
                }
                6 => {
                    if game.state.machines[machine].queue.first().is_some_and(|j| j.remaining_ticks > 0) {
                        return Some(Command::Wait);
                    }
                    self.phase = if self.refund { 7 } else { 0 };
                    return Some(if self.refund {
                        command("cancel")
                    } else {
                        Command::Deliver { item: "workshop:ore".into(), amount: 2 }
                    });
                }
                7..=9 => {
                    self.phase += 1;
                    return Some(Command::Wait);
                }
                _ => {
                    self.phase = 0;
                    return Some(Command::Deliver { item: "workshop:ore".into(), amount: 3 });
                }
            }
        }
    }
}
pub fn run(seed: u64, ticks: usize, fault: bool) -> Report {
    let mut game = Workshop::standard();
    let mut trace = Trace { seed, content: game.definitions.fingerprint(), turns: Vec::new(), fault };
    let mut uninterrupted = game.clone();
    let mut random = seed;
    let prefix = prefix();
    let mut failure = None;
    let mut coverage = Coverage::default();
    let mut checkpoints = Vec::new();
    let mut recovery = Recovery::default();
    for tick in 0..ticks {
        if tick % 512 == 256 {
            recovery = Recovery { phase: 1, refund: tick / 512 % 2 == 0 };
        }
        let command = prefix
            .get(tick)
            .cloned()
            .unwrap_or_else(|| recovery.next(&game).unwrap_or_else(|| candidate(&game, &mut random)));
        let envelope = Envelope { seat: 0, serial: tick as u64 + 1, command };
        let input = if tick % 7 == 0 { vec![envelope.clone(), envelope] } else { vec![envelope] };
        let turn = Turn { input, reload: tick % 31 == 4, migrate: tick % 997 == 996 };
        let result = checked_step(
            &mut game,
            &turn,
            if fault { Fault::Duplicate } else { Fault::None },
            &mut coverage,
            &mut checkpoints,
        );
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
        coverage,
        checkpoints,
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
    let mut coverage = Coverage::default();
    let mut checkpoints = Vec::new();
    if failure.is_none() {
        for turn in &trace.turns {
            ticks += 1;
            if let Err(e) = checked_step(
                &mut game,
                turn,
                if trace.fault { Fault::Duplicate } else { Fault::None },
                &mut coverage,
                &mut checkpoints,
            ) {
                failure = Some(e);
                break;
            }
        }
    }
    Report {
        ok: failure.is_none(),
        ticks,
        hash: format!("{:016x}", game.state_hash()),
        failure,
        trace,
        coverage,
        checkpoints,
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn real_game_passes_and_deliberate_duplicate_fixture_fails_replayably() {
        for seed in [0, 1, 4, 6, 7, 42] {
            let report = super::run(seed, 10000, false);
            assert!(report.ok, "{:?}", report.failure);
            assert!(report.coverage.completions > 10);
            assert!(report.coverage.refunds > 10);
            assert!(report.coverage.blocked_production > 10);
            assert!(report.coverage.blocked_refunds > 10);
            assert!(report.coverage.delivery_recoveries > 10);
            assert!(report.coverage.reloads > 300);
            assert!(report.coverage.migrations >= 10);
            let replay = super::replay(report.trace);
            assert!(replay.ok);
            assert_eq!(replay.hash, report.hash);
            assert_eq!(replay.coverage, report.coverage);
        }
        let broken = super::run(7, 100, true);
        assert!(!broken.ok);
        let replay = super::replay(broken.trace);
        assert_eq!(replay.failure, broken.failure);
        assert_eq!(replay.ticks, broken.ticks);
    }
}

#[cfg(test)]
mod progression_tests {
    use super::*;
    fn turn(game: &Workshop, command: Command) -> Turn {
        Turn {
            input: vec![Envelope {
                seat: 0,
                serial: game.state.last_command.get(&0).copied().unwrap_or(0) + 1,
                command,
            }],
            reload: false,
            migrate: false,
        }
    }
    fn step(game: &mut Workshop, command: Command, coverage: &mut Coverage, checkpoints: &mut Vec<Checkpoint>) {
        let turn = turn(game, command);
        checked_step(game, &turn, Fault::None, coverage, checkpoints).unwrap();
    }
    fn blocked() -> (Workshop, Coverage, Vec<Checkpoint>) {
        let mut game = Workshop::standard();
        let mut coverage = Coverage::default();
        let mut checkpoints = Vec::new();
        step(&mut game, command("queue-a"), &mut coverage, &mut checkpoints);
        step(&mut game, Command::Gather { item: "workshop:ore".into(), amount: 3 }, &mut coverage, &mut checkpoints);
        for _ in 0..6 {
            step(&mut game, Command::Wait, &mut coverage, &mut checkpoints);
        }
        assert_eq!(game.state.machines["workshop:machine-a"].queue[0].remaining_ticks, 0);
        assert!(coverage.blocked_production > 0);
        (game, coverage, checkpoints)
    }
    #[test]
    fn direct_oracle_detects_accounting_preserving_stalled_completion_and_refund() {
        for (refund, fault, expected) in [
            (false, Fault::StallCompletion, "ready production stalled"),
            (true, Fault::StallRefund, "pending cancellation refund stalled"),
        ] {
            let (mut game, mut coverage, mut checkpoints) = blocked();
            if refund {
                step(&mut game, command("cancel"), &mut coverage, &mut checkpoints);
                assert!(coverage.blocked_refunds > 0);
            }
            // Both authorities preserve accounting and valid frozen escrow. The fitting head job
            // must leave the queue on this delivery tick; differential replay alone cannot prove it.
            let turn =
                turn(&game, Command::Deliver { item: "workshop:ore".into(), amount: if refund { 3 } else { 2 } });
            let before = total(&game.state);
            let failure = checked_step(&mut game, &turn, fault, &mut coverage, &mut checkpoints).unwrap_err();
            assert!(failure.contains(expected), "{failure}");
            assert_eq!(total(&game.state), before);
            game.validate_state(&game.state).unwrap();
        }
    }
    #[test]
    fn blocked_work_and_paid_escrow_survive_repeated_restores_and_compatible_migration() {
        let (mut game, mut coverage, mut checkpoints) = blocked();
        for migrate in [false, true, false] {
            let mut turn = turn(&game, Command::Wait);
            turn.reload = true;
            turn.migrate = migrate;
            checked_step(&mut game, &turn, Fault::None, &mut coverage, &mut checkpoints).unwrap();
        }
        step(&mut game, command("cancel"), &mut coverage, &mut checkpoints);
        let mut turn = turn(&game, Command::Wait);
        turn.migrate = true;
        checked_step(&mut game, &turn, Fault::None, &mut coverage, &mut checkpoints).unwrap();
        step(&mut game, Command::Deliver { item: "workshop:ore".into(), amount: 3 }, &mut coverage, &mut checkpoints);
        assert!(game.state.machines["workshop:machine-a"].queue.is_empty());
        assert_eq!(coverage.refunds, 1);
        assert_eq!(coverage.delivery_recoveries, 1);
        assert_eq!(coverage.migrations, 2);
        assert_eq!(checkpoints.len(), 4);
        for checkpoint in checkpoints {
            assert!(!checkpoint.original_hex.is_empty());
            assert!(checkpoint.evidence_directory.is_none());
            if checkpoint.operation == "content-migration" {
                assert_ne!(Some(checkpoint.source_content), checkpoint.target_content);
                assert!(checkpoint.migrated_hex.is_some());
            }
        }
    }
}
