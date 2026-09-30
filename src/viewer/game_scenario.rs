//! Turn `game-explore`'s shortest win into a scenario that plays it: walk to each target, look at it,
//! press it, and wait where the rules need time to pass.
//!
//! The explorer abstracts time, but a walk takes ticks and timers run meanwhile, so the plan is not replayed
//! blindly. Before each press the generator asks the model what pressing would do *to the state the game is
//! really in*, and presses only once that does what the plan expects (the same rules fire). That is what a
//! player does when a switch is only safe in one phase of a cycle. Every scenario it writes is run through the
//! normal scenario runner before it is reported as verified.
use super::{
    game::{GameRuntime, LoadedGame, ModelEvent},
    game_explore::Report,
    scenario::{evaluate_scenario, InputDriver, PlayerConfig, Scenario, TimedInput},
};
use crate::Result;
use std::path::Path;

/// Standing distances and directions tried, in order, until one produces a scenario that wins.
const STAND_OFF_M: f32 = 1.5;
/// How long to wait for the state to be ready before giving up, in ticks.
const WAIT_LIMIT_TICKS: u64 = 6000;

#[derive(Clone, Debug)]
pub struct Generated {
    pub scenario: Scenario,
    /// The strategy that worked (which side of each target the player stood on).
    pub strategy: &'static str,
}

/// Build a scenario that plays the report's shortest win. `game_path` is what the scenario will name (relative
/// to where it is saved); `check_path` is the same game as reachable from here, for the verifying run.
pub fn win_scenario(
    loaded: &LoadedGame,
    report: &Report,
    game_path: &str,
    check_path: &str,
) -> Result<Generated> {
    if report.win_events.is_empty() {
        return Err("The game has no winning path to turn into a scenario".into());
    }
    if report
        .win_events
        .iter()
        .any(|e| matches!(e, ModelEvent::EnterZone(_) | ModelEvent::ExitZone(_)))
    {
        return Err("Winning paths through trigger zones are not turned into scenarios yet".into());
    }
    let mut last_error = String::new();
    for (name, side) in [
        ("toward the previous position", None),
        ("south of each target (+z)", Some((0.0, 1.0))),
        ("north of each target (-z)", Some((0.0, -1.0))),
        ("east of each target (+x)", Some((1.0, 0.0))),
        ("west of each target (-x)", Some((-1.0, 0.0))),
    ] {
        match attempt(loaded, report, game_path, check_path, side) {
            Ok(scenario) => {
                return Ok(Generated {
                    scenario,
                    strategy: name,
                })
            }
            Err(error) => last_error = format!("{name}: {error}"),
        }
    }
    Err(format!(
        "No way of standing in front of the targets produced a win. Last attempt, {last_error}"
    )
    .into())
}

fn attempt(
    loaded: &LoadedGame,
    report: &Report,
    game_path: &str,
    check_path: &str,
    side: Option<(f32, f32)>,
) -> Result<Scenario> {
    let document = &loaded.document;
    let mut world = loaded.clone().world()?;
    world.join(1);
    let mut model = GameRuntime::compile(document.clone(), &loaded.map)?;
    model.record_fired_rules();

    // What each planned event does to the rules, replayed from the start state in the model.
    let mut expected: Vec<Vec<usize>> = Vec::new();
    let mut occupied = 0u64;
    for event in &report.win_events {
        model.model_apply(*event, &mut occupied);
        expected.push(model.take_fired_rules());
    }

    let mut driver = InputDriver::new(&[]);
    let mut recorded: Vec<TimedInput> = Vec::new();
    let mut tick = 0u64;
    let mut position = world
        .player(1)
        .map(|c| (c.position.0, c.position.2))
        .unwrap_or((0., 0.));
    let step =
        |driver: &mut InputDriver, world: &mut super::simulation::HeadlessWorld, tick: &mut u64| {
            *tick += 1;
            driver.before_step(world, *tick);
            world.step();
        };

    for (index, event) in report.win_events.iter().enumerate() {
        let ModelEvent::Interact(target) = *event else {
            continue; // time passing: the next press waits until the rules are ready for it
        };
        let name = document.interactables[target].entity.clone();
        let bounds = &loaded
            .map
            .entities
            .iter()
            .find(|e| e.id == name)
            .ok_or_else(|| format!("no entity named {name}"))?
            .bounds;
        let (cx, cz) = (
            (bounds.min.0 + bounds.max.0) / 2.,
            (bounds.min.2 + bounds.max.2) / 2.,
        );
        let (dx, dz) = side.unwrap_or_else(|| {
            let (px, pz) = (position.0 - cx, position.1 - cz);
            let length = px.hypot(pz);
            if length < 0.01 {
                (0., 1.)
            } else {
                (px / length, pz / length)
            }
        });
        let stand = [cx + dx * STAND_OFF_M, cz + dz * STAND_OFF_M];

        let walk = TimedInput {
            tick: tick + 1,
            player: 1,
            walk_to: Some(stand),
            ..Default::default()
        };
        recorded.push(walk.clone());
        driver.push(walk);
        while driver.busy(1) {
            step(&mut driver, &mut world, &mut tick);
            if tick > WAIT_LIMIT_TICKS * 4 {
                return Err(format!("could not walk to {name}").into());
            }
        }
        if let Some(problem) = driver.problems().first() {
            return Err(problem.clone().into());
        }
        position = world
            .player(1)
            .map(|c| (c.position.0, c.position.2))
            .unwrap_or(position);

        // Wait until pressing now does what the plan says, then press.
        let waited_from = tick;
        loop {
            let game = world.game.as_ref().ok_or("the world has no game")?;
            if game.state().finished() {
                return Err(format!("the game ended before {name} could be pressed").into());
            }
            let enabled = game.state().enabled & (1 << target) != 0;
            let mut probe_occupied = 0u64;
            model.model_load(game.state());
            model.model_apply(ModelEvent::Interact(target), &mut probe_occupied);
            let fired = model.take_fired_rules();
            if enabled && fired == expected[index] {
                break;
            }
            if tick - waited_from > WAIT_LIMIT_TICKS {
                return Err(format!(
                    "waited {WAIT_LIMIT_TICKS} ticks for pressing {name} to do what the plan expects"
                )
                .into());
            }
            step(&mut driver, &mut world, &mut tick);
        }
        let press = TimedInput {
            tick: tick + 1,
            player: 1,
            face: Some(name),
            interact: true,
            ..Default::default()
        };
        recorded.push(press.clone());
        driver.push(press);
        step(&mut driver, &mut world, &mut tick);
    }
    step(&mut driver, &mut world, &mut tick);
    if !world.game.as_ref().is_some_and(|g| g.state().completed) {
        return Err("the run did not end with the game completed".into());
    }
    let ticks = tick + 2;
    let scenario = Scenario {
        name: format!(
            "generated: the shortest win ({})",
            report.shortest_win.as_deref().unwrap_or(&[]).join(", ")
        ),
        game_path: Some(game_path.to_string()),
        ticks,
        players: vec![PlayerConfig { id: 1, spawn: None }],
        inputs: recorded,
        assertions: vec![super::scenario::Assertion {
            tick: ticks,
            completed_equals: Some(true),
            failed_equals: Some(false),
            ..super::scenario::Assertion::default()
        }],
    };
    // The recorded inputs must reproduce the run through the ordinary runner.
    let check = evaluate_scenario(&Scenario {
        game_path: Some(check_path.to_string()),
        ..scenario.clone()
    })?;
    if !check.ok {
        let why = check
            .assertions
            .iter()
            .find(|a| !a.ok)
            .map(|a| a.detail.clone())
            .unwrap_or_default();
        return Err(format!("the recorded scenario did not replay: {why}").into());
    }
    Ok(scenario)
}

/// Path for the scenario file to name: the game's file name when they share a directory, else absolute.
pub fn game_path_relative_to(scenario_out: &Path, game: &Path) -> Result<String> {
    let game = game.canonicalize()?;
    let out_dir = scenario_out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()?;
    Ok(if game.parent() == Some(out_dir.as_path()) {
        game.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        game.to_string_lossy().into_owned()
    })
}
