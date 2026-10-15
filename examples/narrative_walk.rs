//! Public bounded headless execution of the real Relay Yard authority and narrative bindings.
//! cargo run --example narrative_walk --no-default-features --features narrative -- [FILE]
//!   --fixture reactive|scene|storyless --seed N --limit N --route share|keep --quiet
#[path = "narrative/shared.rs"]
mod shared;
use shared::{Fixture, Input, Sim};
use std::path::Path;
use vesper3d::viewer::devkit::{flag_value, has_flag, snapshot, Simulation};

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if has_flag(&args, "--help") {
        println!("Bounded Relay Yard walk\n[FILE] --content FILE --fixture storyless|reactive|scene --seed N --limit N --route share|keep --quiet\nRuns real 60 Hz movement/bindings, reports trace, state and engine snapshot resume.");
        return Ok(());
    }
    let mut index = 1;
    let mut seen = std::collections::BTreeSet::new();
    while index < args.len() {
        let option = args[index].as_str();
        if !seen.insert(option) {
            return Err(format!("duplicate option {option}"));
        }
        if index == 1 && !option.starts_with('-') {
            index += 1;
            continue;
        }
        match option {
            "--quiet" => index += 1,
            "--content" | "--fixture" | "--seed" | "--limit" | "--route" => {
                if args.get(index + 1).is_none_or(|a| a.starts_with("--")) {
                    return Err(format!("{option} needs a value"));
                }
                index += 2;
            }
            _ => return Err(format!("unknown option {option:?}; use --help")),
        }
    }
    let fixture = Fixture::parse(flag_value(&args, "--fixture").unwrap_or("scene"))?;
    let seed = flag_value(&args, "--seed")
        .unwrap_or("7")
        .parse::<u64>()
        .map_err(|e| e.to_string())?;
    let limit = flag_value(&args, "--limit")
        .unwrap_or("340")
        .parse::<usize>()
        .map_err(|e| e.to_string())?;
    if limit > 100_000 {
        return Err("--limit must be <=100000 ticks".into());
    }
    let route = flag_value(&args, "--route").unwrap_or("share");
    if !matches!(route, "share" | "keep") {
        return Err("--route must be share or keep".into());
    }
    if args.get(1).is_some_and(|a| !a.starts_with('-')) && has_flag(&args, "--content") {
        return Err("provide positional FILE or --content FILE once".into());
    }
    let path = flag_value(&args, "--content")
        .map(Path::new)
        .or_else(|| args.get(1).filter(|a| !a.starts_with('-')).map(Path::new));
    let mut sim = if let Some(path) = path {
        Sim::with_source(fixture, seed, Some(path))?
    } else {
        Sim::new(fixture, seed)?
    };
    let quiet = has_flag(&args, "--quiet");
    let mut trace = Vec::new();
    let mut runtime_errors = 0;
    let mut saved = None;
    let mut inputs = Vec::with_capacity(limit);
    for tick in 0..limit {
        let mut input = shared::walk_inputs(tick);
        if let Some(scene) = sim.scene()? {
            input = Input::default();
            if scene.choices.is_empty() {
                input.advance = Some(scene.token);
            } else {
                input.choice = Some((scene.token, route.into()));
            }
            trace.push(serde_json::json!({"tick":tick,"scene":scene}));
        }
        if let Some(line) = sim.response()? {
            trace.push(serde_json::json!({"tick":tick,"response":line,"suppressed":quiet}));
            if quiet {
                input.interrupt = Some(line.token);
            } else {
                let heard = sim
                    .narrative
                    .as_ref()
                    .and_then(|r| r.state().active_response.as_ref())
                    .is_some_and(|r| r.presented);
                if heard {
                    input.complete = Some(line.token);
                } else {
                    input.presented = Some(line.token);
                }
            }
        }
        sim.step(&input);
        inputs.push(input);
        if let Some(error) = &sim.notice {
            runtime_errors += 1;
            trace.push(serde_json::json!({"tick":tick,"diagnostic":error}));
        }
        // Use engine-owned bytes and restore. Never invent a fixture save format.
        if tick == limit / 2 {
            saved =
                Some(snapshot::save(&sim, "bounded headless midpoint").map_err(|e| e.to_string())?);
        }
    }
    let resumed = if let Some(bytes) = saved {
        let mut resume = if let Some(path) = path {
            Sim::with_source(fixture, seed, Some(path))?
        } else {
            Sim::new(fixture, seed)?
        };
        snapshot::restore(&mut resume, &bytes).map_err(|e| e.to_string())?;
        for input in &inputs[limit / 2 + 1..] {
            resume.step(input);
        }
        resume.state_hash() == sim.state_hash()
    } else {
        true
    };
    let report = serde_json::json!({
        "schema_version":1,"execution_errors":runtime_errors,"fixture":format!("{fixture:?}"),"seed":seed,"limit":limit,"route":route,
        "assumptions":["fixed 60 Hz","single player; speakers present","one authored route; no exhaustive exploration","actual movement, collision, typed reads and atomic commands"],
        "world":sim.world.capture(),"state_hash":format!("{:016x}",sim.state_hash()),
        "resume_replays_identically":resumed,"resume_split_after_tick":if limit>0{Some(limit/2)}else{None},"narrative":sim.narrative.as_ref().map(|r|r.state()),"trace":trace,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
    );
    if !resumed {
        return Err("resume determinism failed".into());
    }
    if runtime_errors != 0 {
        return Err(format!(
            "bounded walk encountered {runtime_errors} runtime errors; inspect trace"
        ));
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
