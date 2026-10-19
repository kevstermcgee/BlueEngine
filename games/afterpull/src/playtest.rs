//! Game-owned rule oracles over public intentions. Process budgets/evidence live in tools/playtest.py.
use crate::{rooms::ROOMS, Afterpull, Intent};
use serde::{Deserialize, Serialize};
use vesper3d::runtime::{snapshot, Simulation, Snapshot};
use vesper3d::two_d::{hash_json, GameLogic};

#[derive(Clone, Serialize, Deserialize)]
pub struct Turn {
    pub input: Intent,
    pub reload: bool,
}
#[derive(Serialize, Deserialize)]
pub struct Trace {
    pub seed: u64,
    pub content: String,
    pub turns: Vec<Turn>,
    pub fault: bool,
}
#[derive(Default, Serialize)]
pub struct Coverage {
    pub moves: usize,
    pub undos: usize,
    pub retries: usize,
    pub transitions: usize,
    pub losses: usize,
    pub wins: usize,
    pub reloads: usize,
}
#[derive(Serialize)]
pub struct Checkpoint {
    pub tick: usize,
    pub operation: &'static str,
    pub original_hex: String,
}
#[derive(Serialize)]
pub struct Report {
    pub ok: bool,
    pub ticks: usize,
    pub hash: String,
    pub failure: Option<String>,
    pub coverage: Coverage,
    pub checkpoints: Vec<Checkpoint>,
    pub trace: Trace,
}
fn content() -> String {
    // Pin the adapter and real rules/room definitions, including new uncommitted source.
    format!(
        "{:016x}",
        hash_json(&(
            include_str!("lib.rs"),
            include_str!("rooms.rs"),
            include_str!("playtest.rs")
        ))
    )
}
fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn checked_step(
    game: &mut Afterpull,
    turn: &Turn,
    fault: bool,
    coverage: &mut Coverage,
) -> Result<(), String> {
    let before = game.capture();
    let command = turn.input.pointer.filter(|_| turn.input.action);
    let code = command.map(|p| p.x);
    let moving = code == Some(-10) && !game.failed() && !game.cleared() && !before.finished;
    // Inspect the authority's immutable preview rather than implementing another resolver.
    let plan = if moving {
        command.and_then(|p| game.plan(p.y as usize))
    } else {
        None
    };
    let was_failed = game.failed();
    let was_cleared = game.cleared();
    game.step(&turn.input);
    // A deliberately incomplete undo is confined to this developer adapter.
    if fault && code == Some(-11) && !before.history.is_empty() {
        game.state.board.turns = before.board.turns;
    }
    require(
        game.state.tick == before.tick.wrapping_add(1),
        "fixed tick did not advance once",
    )?;
    require(
        Afterpull::validate_board(game.state.room, &game.state.board),
        "crate/player occupancy or crate count invalid",
    )?;
    let room = &ROOMS[game.state.room];
    let occupied = room
        .docks
        .iter()
        .filter(|c| game.state.board.crates.contains(c))
        .count();
    let failed = game
        .state
        .board
        .crates
        .iter()
        .any(|c| room.pits.contains(c));
    require(
        game.filled() == occupied && game.failed() == failed,
        "dock/pit status disagrees with crate occupancy",
    )?;
    require(
        game.cleared() == (!failed && occupied == room.docks.len()),
        "completion disagrees with dock occupancy",
    )?;
    if code == Some(-11) {
        if let Some(previous) = before.history.last() {
            require(
                &game.state.board == previous
                    && game.state.history == before.history[..before.history.len() - 1],
                "whole-turn undo did not restore board/counters/history",
            )?;
            require(
                !game.state.finished && game.state.preview.is_none() && game.state.last.is_none(),
                "undo retained completion/preview state",
            )?;
            coverage.undos += 1;
        } else {
            require(
                game.state.board == before.board && game.state.history.is_empty(),
                "empty undo changed board",
            )?;
        }
    } else if code == Some(-13) {
        require(
            game.state.room == before.room
                && game.state.board == Afterpull::initial(before.room)
                && game.state.history.is_empty()
                && !game.state.finished,
            "retry did not restore current room",
        )?;
        coverage.retries += 1;
    } else if code == Some(-12) && was_cleared && !before.finished {
        if before.room + 1 < ROOMS.len() {
            require(
                game.state.room == before.room + 1
                    && game.state.board == Afterpull::initial(before.room + 1)
                    && game.state.history.is_empty(),
                "cleared room did not transition to fresh next room",
            )?;
            coverage.transitions += 1;
        } else {
            require(
                game.state.finished && game.outcome() == "won",
                "final cleared room did not finish",
            )?;
            coverage.wins += 1;
        }
    } else if let Some(plan) = plan {
        require(
            game.state.board.player == plan.player
                && game.state.history.last() == Some(&before.board)
                && game.state.history.len() == before.history.len() + 1,
            "legal move did not commit once with whole-board history",
        )?;
        for (old, new) in before.board.crates.iter().zip(&game.state.board.crates) {
            let destination = plan
                .pulls
                .iter()
                .find(|p| p.from == *old && !p.jam)
                .map_or(*old, |p| p.to);
            require(
                *new == destination,
                "move disagrees with simultaneous immutable pull plan",
            )?;
        }
        require(
            game.state.board.turns == before.board.turns + 1
                && game.state.board.jams
                    == before.board.jams + u32::from(plan.pulls.iter().any(|p| p.jam))
                && game.state.board.multi
                    == before.board.multi
                        + u32::from(plan.pulls.iter().filter(|p| !p.jam).count() > 1),
            "move counters did not advance once",
        )?;
        coverage.moves += 1;
    } else {
        require(
            game.state.board == before.board
                && game.state.history == before.history
                && game.state.room == before.room,
            "blocked, preview, terminal or idle input changed board/history/room",
        )?;
    }
    if !was_failed && game.failed() {
        coverage.losses += 1;
    }
    require(
        game.outcome()
            == if game.state.finished {
                "won"
            } else if failed {
                "lost"
            } else {
                "playing"
            },
        "outcome disagrees with terminal state",
    )?;
    // Reuse complete candidate validation, including saved history and trace bounds.
    let mut candidate = Afterpull::new(0);
    candidate.restore(game.capture())?;
    Ok(())
}
fn direction(ch: char) -> Intent {
    Afterpull::direction("URDL".find(ch).unwrap())
}
fn next_random(random: &mut u64) -> u64 {
    *random = random
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *random
}
fn room_route(room: usize, random: &mut u64) -> Vec<Intent> {
    let mut route = Vec::new();
    for _ in 0..12 {
        let value = next_random(random);
        route.push(match value % 8 {
            0..=3 => Afterpull::direction((value % 4) as usize),
            4 => Afterpull::preview(((value >> 8) % 4) as usize),
            5 => Afterpull::command(-11, 0),
            // Held axes without a press edge must remain free.
            6 => Intent {
                x: 1,
                ..Intent::default()
            },
            _ => Intent::default(),
        });
    }
    route.push(Afterpull::command(-13, 0));
    if room == 5 {
        route.extend("RLLURUUL".chars().map(direction));
        route.push(Afterpull::preview(1));
        route.push(Afterpull::direction(1)); // real pit loss
        route.push(Afterpull::direction(0)); // terminal movement must stay inert
        route.push(Afterpull::command(-11, 0));
        route.push(Afterpull::command(-13, 0));
    }
    route.extend(ROOMS[room].solution.chars().map(direction));
    route.push(Afterpull::command(-11, 0)); // undo a cleared room
    route.push(direction(ROOMS[room].solution.chars().last().unwrap()));
    route.push(Afterpull::command(-12, 0));
    route
}
fn persist_checkpoint(
    root: &std::path::Path,
    index: usize,
    tick: usize,
    bytes: &[u8],
) -> Result<(), String> {
    use std::io::Write;
    let folder = root.join(index.to_string());
    std::fs::create_dir(&folder).map_err(|e| e.to_string())?;
    // Complete writes and close files before continuing; kernel buffers survive a process kill.
    // Power-loss durability belongs to engine save APIs, not these developer evidence copies.
    for (name, data) in [
        ("original.be2save", bytes.to_vec()),
        (
            "metadata.json",
            serde_json::to_vec(&serde_json::json!({"tick":tick,"operation":"save/reload"}))
                .map_err(|e| e.to_string())?,
        ),
    ] {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(folder.join(name))
            .map_err(|e| e.to_string())?;
        file.write_all(&data).map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub fn run(seed: u64, ticks: usize, fault: bool) -> Report {
    let mut random = seed;
    let mut route: std::collections::VecDeque<_> = (0..ROOMS.len())
        .flat_map(|room| room_route(room, &mut random))
        .collect();
    let mut turns = Vec::new();
    for tick in 0..ticks.min(100000) {
        if route.is_empty() {
            route.extend(room_route(5, &mut random));
        }
        turns.push(Turn {
            input: route.pop_front().unwrap(),
            reload: tick % 31 == 4,
        });
    }
    replay(Trace {
        seed,
        content: content(),
        turns,
        fault,
    })
}
pub fn replay(mut trace: Trace) -> Report {
    let mut game = Afterpull::new(trace.seed);
    let mut uninterrupted = Afterpull::new(trace.seed);
    let mut coverage = Coverage::default();
    let mut checkpoints = Vec::new();
    let mut failure = if trace.content != content() {
        Some("content identity differs".into())
    } else if trace.turns.len() > 100000 {
        Some("trace exceeds bounded tick limit".into())
    } else {
        None
    };
    let mut ticks = 0;
    if failure.is_none() {
        for turn in &trace.turns {
            ticks += 1;
            let result = (|| {
                checked_step(&mut game, turn, trace.fault, &mut coverage)?;
                uninterrupted.step(&turn.input);
                if turn.reload {
                    let bytes = snapshot::save(&game, "Afterpull playtest checkpoint")
                        .map_err(|e| e.to_string())?;
                    // Retain the original framed save even when restore fails.
                    checkpoints.push(Checkpoint {
                        tick: ticks,
                        operation: "save/reload",
                        original_hex: bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
                    });
                    if let Some(root) = std::env::var_os("BE2_PLAYTEST_EVIDENCE_DIR") {
                        persist_checkpoint(
                            std::path::Path::new(&root),
                            checkpoints.len() - 1,
                            ticks,
                            &bytes,
                        )?;
                    }
                    let mut restored = Afterpull::new(trace.seed);
                    snapshot::restore(&mut restored, &bytes).map_err(|e| e.to_string())?;
                    require(
                        restored.state_hash() == game.state_hash(),
                        "save/reload diverged",
                    )?;
                    game = restored;
                    coverage.reloads += 1;
                }
                require(
                    game.state_hash() == uninterrupted.state_hash(),
                    "saved continuation differs from uninterrupted run",
                )
            })();
            if let Err(error) = result {
                failure = Some(error);
                break;
            }
        }
        // Failure evidence records precisely the executed public commands.
        trace.turns.truncate(ticks);
    }
    Report {
        ok: failure.is_none(),
        ticks,
        hash: format!("{:016x}", game.state_hash()),
        failure,
        coverage,
        checkpoints,
        trace,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn completed_original_checkpoint_is_flushed_and_existing_evidence_is_never_overwritten() {
        use vesper3d::two_d::GameLogic;
        let folder = std::env::temp_dir().join(format!(
            "afterpull-checkpoint-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&folder).unwrap();
        let original =
            vesper3d::runtime::snapshot::save(&super::Afterpull::new(0), "retained original")
                .unwrap();
        super::persist_checkpoint(&folder, 0, 4, &original).unwrap();
        assert!(super::persist_checkpoint(&folder, 0, 5, b"replacement").is_err());
        assert_eq!(
            std::fs::read(folder.join("0/original.be2save")).unwrap(),
            original
        );
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn real_actions_cover_campaign_failure_undo_retry_and_repeated_save_continuity() {
        for seed in [0, 7, 42] {
            let report = super::run(seed, 1000, false);
            assert!(report.ok, "{:?}", report.failure);
            assert_eq!(report.coverage.transitions, 5);
            assert!(report.coverage.wins > 5 && report.coverage.losses > 5);
            assert!(
                report.coverage.undos > 20
                    && report.coverage.retries > 20
                    && report.coverage.reloads > 30
            );
            let replay = super::replay(report.trace);
            assert!(replay.ok, "{:?}", replay.failure);
            assert_eq!(replay.hash, report.hash);
        }
    }
    #[test]
    fn deliberately_incomplete_undo_is_detected_and_replays_the_same_failure() {
        let broken = super::run(7, 1000, true);
        assert!(!broken.ok);
        assert!(broken.failure.as_ref().unwrap().contains("whole-turn undo"));
        let replay = super::replay(broken.trace);
        assert_eq!(replay.failure, broken.failure);
        assert_eq!(replay.ticks, broken.ticks);
        assert_eq!(replay.hash, broken.hash);
    }
    #[test]
    fn trace_refuses_other_rule_content_and_excessive_turns() {
        let mut trace = super::run(0, 22, false).trace;
        trace.content = "obsolete".into();
        assert_eq!(
            super::replay(trace).failure.as_deref(),
            Some("content identity differs")
        );
        let mut trace = super::run(0, 22, false).trace;
        trace.turns.resize(
            100001,
            super::Turn {
                input: super::Intent::default(),
                reload: false,
            },
        );
        assert_eq!(
            super::replay(trace).failure.as_deref(),
            Some("trace exceeds bounded tick limit")
        );
    }
}
