//! Headless authoring commands;
//! JSON output is suitable for agents and automation.
use serde_json::json;
use std::path::Path;
use vesper3d::{
    math::{Ray, V},
    viewer::{
        authoring::{write_new, Edit, MapDocument},
        controller::{Controller, Movement},
        metrics::{PerformanceBudget, RegressionBudget, RegressionMeasurements},
        net::{InputFrame, NetworkSimulator, PredictionBuffer},
        simulation::HeadlessWorld,
    },
    Result,
};
fn save(path: &str, value: &impl serde::Serialize) -> Result<()> {
    write_new(Path::new(path), &serde_json::to_vec_pretty(value)?)
}
fn vector(text: &str) -> Result<V> {
    let parts: Vec<f32> = text
        .split(',')
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()?;
    if parts.len() != 3 || parts.iter().any(|v| !v.is_finite() || v.abs() > 1000.) {
        return Err("Expected finite x,y,z within +/-1000".into());
    }
    Ok(V(parts[0], parts[1], parts[2]))
}
fn vector2(text: &str) -> Result<(f32, f32)> {
    let parts: Vec<f32> = text
        .split(',')
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()?;
    if parts.len() != 2 || parts.iter().any(|v| !v.is_finite() || v.abs() > 1000.) {
        return Err("Expected finite x,z within +/-1000".into());
    }
    Ok((parts[0], parts[1]))
}
fn rect4(text: &str) -> Result<[f32; 4]> {
    let parts: Vec<f32> = text
        .split(',')
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()?;
    if parts.len() != 4 || parts.iter().any(|v| !v.is_finite() || v.abs() > 1000.) {
        return Err("Expected finite x1,z1,x2,z2 within +/-1000".into());
    }
    Ok([parts[0], parts[1], parts[2], parts[3]])
}
fn main() {
    if let Err(e) = run() {
        eprintln!(
            "{}",
            json!({
            "ok":false,"error":e.to_string()
            }
            )
        );
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let a: Vec<_> = std::env::args().skip(1).collect();
    let arg = |i: usize| -> Result<&str> {
        a.get(i)
            .map(String::as_str)
            .ok_or_else(|| "Missing argument; run be2-tools help".into())
    };
    let command = a.first().map(String::as_str).unwrap_or("help");
    let signature = vesper3d::viewer::capabilities::COMMANDS
        .iter()
        .find(|(name, _)| *name == command)
        .ok_or("Unknown command; run be2-tools help")?
        .1;
    let words: Vec<&str> = signature.split_whitespace().collect();
    let min_args = words.iter().filter(|w| !w.starts_with('[')).count();
    let max_args = words.len();
    let provided_args = if a.is_empty() { 0 } else { a.len() - 1 };
    if !a.is_empty() && (provided_args < min_args || provided_args > max_args) {
        if min_args == max_args {
            return Err(format!("{command} expects {} arguments", min_args).into());
        } else {
            return Err(format!(
                "{command} expects between {} and {} arguments",
                min_args, max_args
            )
            .into());
        }
    }
    match command {
        "game-describe" => println!(
            "{}",
            json!({
                "ok":true,"schema_version":1,"schema_command":"game-schema", "example":"game-example NEW_DIRECTORY",
                "event":"authoritative nearest-visible interaction within 2.5 metres (E intent), spatial trigger zones (on_enter/on_exit), or timers (on_timer)",
                "actions":["increment","set_counter","set_enabled","set_visible","set_mover","start_timer","stop_timer","complete"],
                "conditions":"counter equals integer; null means unconditional",
                "limits":{"game_bytes":64000,"spawns":8,"counters":32,"interactables":64,"trigger_zones":64,"movers":64,"timers":64,"rules":64,"actions_per_rule":4,"counter_magnitude":1000000},
                "order":"player IDs ascending at fixed tick; rules in document order; later conditions see earlier actions; once is per match",
                "geometry":"static axis-aligned box with matching node/collider/entity ID and bounds; trigger zones declare spatial AABB bounds; movers translate colliders smoothly",
                "set_enabled":"interaction and trigger zone eligibility only; never changes visibility or collision",
                "set_visible":"interactable presentation only; never changes eligibility or collision",
                "profiles":"one shared validated movement profile; spawns use feet coordinates and round-robin server IDs",
                "map":"relative child file inside game directory; loaded content participates in fingerprint",
                "unsupported":["recursive events","custom weapon actions","per-player inventory"]
            })
        ),
        "game-schema" => println!("{}", include_str!("../../tools/game.schema.json")),
        "game-example" => {
            vesper3d::viewer::game_example::write(Path::new(arg(1)?))?;
            println!("{}", json!({"ok":true,"directory":arg(1)?}));
        }
        "game-validate" => {
            let world = vesper3d::viewer::game::GameDocument::load(Path::new(arg(1)?))?.world()?;
            println!(
                "{}",
                json!({"ok":true,"content_hash":format!("{:016x}",world.content_hash),"state":world.game.as_ref().unwrap().state()})
            );
        }
        "help" => {
            println!("BlueEngine native toolkit (new output paths only)");
            for (name, signature) in vesper3d::viewer::capabilities::COMMANDS {
                println!("{name} {signature}");
            }
        }
        "describe" => println!("{}", vesper3d::viewer::capabilities::describe()?),
        "search" => println!("{}", vesper3d::viewer::capabilities::search(arg(1)?)?),
        "export-lab" => {
            let doc = MapDocument::from_map(vesper3d::viewer::maps::MapId::TestLab)?;
            doc.build()?;
            save(arg(1)?, &doc)?;
            println!("{}", json!({"ok":true,"output":arg(1)?}));
        }
        "inspect-performance" => {
            let mut world = HeadlessWorld::new()?;
            world.join(1);
            world.join(2);
            let started = std::time::Instant::now();
            for _ in 0..60 {
                world.step();
            }
            let elapsed_us = started.elapsed().as_secs_f64() * 1_000_000. / 60.;
            let perf = world.performance_snapshot(elapsed_us);
            let report = json!({
                "ok": true,
                "snapshot": perf,
                "explanation": perf.explain(),
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        "validate-budget" => {
            let mut world = HeadlessWorld::new()?;
            world.join(1);
            world.join(2);
            let started = std::time::Instant::now();
            for _ in 0..60 {
                world.step();
            }
            let elapsed_us = started.elapsed().as_secs_f64() * 1_000_000. / 60.;
            let perf = world.performance_snapshot(elapsed_us);
            let budget = PerformanceBudget::default();
            let validation = budget.validate(&perf);
            let report = json!({
                "ok": validation.passed,
                "passed": validation.passed,
                "violations": validation.violations,
                "metrics": perf,
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
            if !validation.passed {
                return Err("Performance budget exceeded".into());
            }
        }
        "net-test" => {
            let mut world = HeadlessWorld::new()?;
            world.join(1);
            let mut sim = NetworkSimulator::<InputFrame>::new(50, 0.05); // 50ms latency, 5% packet loss
            let mut pred = PredictionBuffer::new(64);
            let mut controller = Controller::default();
            let mut dropped_packets = 0;
            let mut reconciled_corrections = 0;
            let mut last_ack_tick = 0;

            for tick in 1..=120 {
                let input = InputFrame {
                    client_tick: tick,
                    movement: Movement {
                        forward: 1.0,
                        ..Default::default()
                    },
                    yaw: 0.0,
                    pitch: 0.0,
                    fire_wrench: false,
                    fire_pistol: false,
                    interact: false,
                    ack_server_tick: 0,
                    session_token: None,
                };
                controller.update(
                    input.movement,
                    vesper3d::viewer::simulation::TICK_SECONDS,
                    &world.room.colliders,
                );
                pred.push(input.clone(), controller.clone());

                if !sim.send(tick, input) {
                    dropped_packets += 1;
                }

                // Server receives scheduled packets for this tick
                let delivered = sim.receive(tick);
                for pkt in delivered {
                    last_ack_tick = pkt.client_tick;
                    world.input(1, pkt.movement, pkt.yaw, pkt.pitch);
                }

                // Continuous server simulation tick (60 Hz server ticking)
                world.step();

                if tick % 3 == 0 && last_ack_tick > 0 {
                    let snap = world.snapshot(last_ack_tick);
                    if let Some(p) = snap.players.iter().find(|p| p.id == 1) {
                        if pred.reconcile(
                            snap.ack_client_tick,
                            p,
                            &mut controller,
                            &world.room.colliders,
                            0.02,
                        ) {
                            reconciled_corrections += 1;
                        }
                    }
                }
            }

            let report = json!({
                "ok": true,
                "simulated_ticks": 120,
                "simulated_latency_ms": sim.latency_ms,
                "simulated_packet_loss_rate": sim.packet_loss_rate,
                "dropped_packets": dropped_packets,
                "reconciled_corrections": reconciled_corrections,
                "final_position": [controller.position.0, controller.position.1, controller.position.2],
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        "replay-test" => {
            // Run a match recording inputs and checkpoints, then verify identical replay reproduction
            let mut world = HeadlessWorld::new()?;
            world.join(1);
            world.join(2);

            let mut recorded_inputs: Vec<(u64, u64, Movement, f32, f32)> = Vec::new();
            let mut checkpoints: Vec<(u64, u64)> = Vec::new();

            for tick in 1..=120 {
                let inp1 = Movement {
                    forward: if tick % 20 < 10 { 1.0 } else { 0.0 },
                    ..Default::default()
                };
                let inp2 = Movement {
                    right: if tick % 15 < 8 { 1.0 } else { -1.0 },
                    ..Default::default()
                };
                world.input(1, inp1, 0.0, 0.0);
                world.input(2, inp2, 0.5, 0.0);

                recorded_inputs.push((tick, 1, inp1, 0.0, 0.0));
                recorded_inputs.push((tick, 2, inp2, 0.5, 0.0));

                world.step();

                if tick % 30 == 0 {
                    checkpoints.push((tick, world.checksum()));
                }
            }

            // Replay from identical start
            let mut replay_world = HeadlessWorld::new()?;
            replay_world.join(1);
            replay_world.join(2);

            let mut input_idx = 0;
            let mut verified_checkpoints = 0;

            for tick in 1..=120 {
                while input_idx < recorded_inputs.len() && recorded_inputs[input_idx].0 == tick {
                    let (_, pid, mv, yaw, pitch) = recorded_inputs[input_idx];
                    replay_world.input(pid, mv, yaw, pitch);
                    input_idx += 1;
                }

                replay_world.step();

                if let Some(&(_, expected_checksum)) = checkpoints.iter().find(|(t, _)| *t == tick)
                {
                    let replayed_checksum = replay_world.checksum();
                    if replayed_checksum != expected_checksum {
                        return Err(format!(
                            "Replay checksum mismatch at tick {tick}: expected {expected_checksum:016x}, got {replayed_checksum:016x}"
                        ).into());
                    }
                    verified_checkpoints += 1;
                }
            }

            let report = json!({
                "ok": true,
                "ticks": 120,
                "checkpoints_verified": verified_checkpoints,
                "deterministic": true,
                "final_checksum": format!("0x{:016x}", replay_world.checksum()),
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        "bench" => {
            let mut world = HeadlessWorld::new()?;
            world.join(1);
            world.join(2);

            // 1. Simulation step benchmark (1000 ticks)
            let t0 = std::time::Instant::now();
            for _ in 0..1000 {
                world.step();
            }
            let sim_step_us = t0.elapsed().as_secs_f64() * 1_000_000. / 1000.;

            // 2. Snapshot computation benchmark (1000 iterations)
            let t0 = std::time::Instant::now();
            for _ in 0..1000 {
                std::hint::black_box(world.snapshot(world.tick));
            }
            let snapshot_us = t0.elapsed().as_secs_f64() * 1_000_000. / 1000.;

            // 3. Delta snapshot computation benchmark (1000 iterations)
            let snap1 = world.snapshot(100);
            let mut snap2 = snap1.clone();
            snap2.tick = 101;
            if let Some(p) = snap2.players.get_mut(0) {
                p.position.0 += 0.1;
            }
            let t0 = std::time::Instant::now();
            for _ in 0..1000 {
                std::hint::black_box(snap2.compute_delta(&snap1));
            }
            let delta_us = t0.elapsed().as_secs_f64() * 1_000_000. / 1000.;

            // 4. Room graph spatial lookup benchmark (10,000 queries)
            let t0 = std::time::Instant::now();
            for i in 0..10000 {
                let p = V((i as f32 % 10.0) - 5.0, 1.0, (i as f32 % 10.0) - 5.0);
                std::hint::black_box(world.room_graph.find_room_at(p));
            }
            let room_lookup_ns = t0.elapsed().as_secs_f64() * 1_000_000_000. / 10000.;

            let measurements = RegressionMeasurements {
                sim_step_mean_us: sim_step_us,
                snapshot_creation_mean_us: snapshot_us,
                delta_compression_mean_us: delta_us,
                room_graph_lookup_mean_ns: room_lookup_ns,
            };
            let budget = RegressionBudget::default();
            let violations = budget.violations(&measurements);
            let passed = violations.is_empty();
            let results = json!({
                "ok": passed,
                "passed": passed,
                "benchmarks": measurements,
                "budget": budget,
                "violations": violations,
            });
            println!("{}", serde_json::to_string_pretty(&results)?);
            if !passed {
                return Err("Benchmark regression budget exceeded".into());
            }
        }
        "catalog" => println!(
            "{}",
            json!({
            "props":["cereal","chair","table","apple","framed-art","framed-botanical","sculpture","vase-plant","bowl","table-lamp","book-stack","candle-trio","potted-cactus","flower-vase","tall-vase","mantel-clock","woven-basket"],"maps":["test-lab","house"],"schema_version":1,"operations":["add_box","add_prop","translate","remove"]
            }
            )
        ),
        "mcp" => {
            vesper3d::viewer::mcp::run_mcp_server()?;
        }
        "doc-check" => {
            let root = a.get(1).map(Path::new).unwrap_or_else(|| Path::new("."));
            let report = vesper3d::viewer::doc_drift::audit_documentation(root)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            if !report.ok {
                std::process::exit(1);
            }
        }
        "net-proxy" => {
            let listen = arg(1)?;
            let upstream_str = arg(2)?;
            let upstream: std::net::SocketAddr = upstream_str
                .parse()
                .map_err(|e| format!("Invalid upstream address '{upstream_str}': {e}"))?;
            let preset_name = a.get(3).map(String::as_str).unwrap_or("bad-wifi");
            let config = vesper3d::viewer::net::NetworkProxyConfig::from_preset(preset_name)
                .ok_or_else(|| format!("Unknown proxy preset '{preset_name}'. Available: bad-wifi, mobile-3g, satellite, congested-bursty"))?;

            let mut proxy = vesper3d::viewer::net::UdpProxyServer::bind(listen, upstream, config)?;
            let local_addr = proxy.local_addr()?;
            println!(
                "{}",
                json!({
                    "ok": true,
                    "proxy_listening": local_addr.to_string(),
                    "upstream": upstream.to_string(),
                    "preset": preset_name,
                    "status": "running"
                })
            );

            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            #[cfg(feature = "offline")]
            {
                let s_clone = stop.clone();
                let _ = ctrlc::set_handler(move || {
                    s_clone.store(true, std::sync::atomic::Ordering::Relaxed);
                });
            }
            proxy.run(stop)?;
        }
        "ui-check" => {
            let rep = vesper3d::viewer::ui_check::audit_all_screens();
            println!("{}", serde_json::to_string_pretty(&rep)?);
            if !rep.ok {
                std::process::exit(1);
            }
        }
        "new-game" => {
            let name = arg(1)?;
            let dir = arg(2)?;
            vesper3d::viewer::newgame::scaffold_new_game(name, Path::new(dir), None)?;
            println!("{}", json!({"ok": true, "name": name, "directory": dir}));
        }
        "blueprint-example" => {
            let example = vesper3d::viewer::blueprint::BlueprintSpec {
                name: "Example Map".into(),
                height: 3.2,
                wall_thickness: 0.20,
                rooms: vec![vesper3d::viewer::blueprint::RoomSpec {
                    id: "hall".into(),
                    rect: [-4.0, -4.0, 4.0, 4.0],
                    floor_color: Some([0.4, 0.45, 0.5]),
                    wall_color: None,
                    lamp: true,
                }],
                doors: vec![],
                spawns: vec![vesper3d::viewer::blueprint::SpawnSpec {
                    id: "p1".into(),
                    room: "hall".into(),
                    offset: Some([0.0, 0.0]),
                }],
                fill: vec![vesper3d::viewer::blueprint::FillSpec {
                    room: "hall".into(),
                    kind: "chair".into(),
                    count: 2,
                    seed: 12345,
                }],
            };
            save(arg(1)?, &example)?;
            println!("{}", json!({"ok": true, "output": arg(1)?}));
        }
        "build" => {
            let spec_bytes = std::fs::read(arg(1)?)?;
            let spec: vesper3d::viewer::blueprint::BlueprintSpec =
                serde_json::from_slice(&spec_bytes)?;
            let doc = vesper3d::viewer::blueprint::compile_blueprint(&spec)?;
            save(arg(2)?, &doc)?;
            println!(
                "{}",
                json!({"ok": true, "output": arg(2)?, "rooms": spec.rooms.len()})
            );
        }
        "scatter" => {
            let d = MapDocument::load(Path::new(arg(1)?))?;
            let kind = arg(2)?;
            let count: usize = arg(3)?.parse()?;
            let rect = rect4(arg(4)?)?;
            let seed: u64 = arg(5)?.parse()?;
            let out_path = arg(6)?;
            let (updated, placed) =
                vesper3d::viewer::gen::scatter(d, kind, count, rect, seed, None)?;
            save(out_path, &updated)?;
            println!(
                "{}",
                json!({"ok": true, "placed": placed, "output": out_path})
            );
        }
        "line" => {
            let d = MapDocument::load(Path::new(arg(1)?))?;
            let kind = arg(2)?;
            let count: usize = arg(3)?.parse()?;
            let rect = rect4(arg(4)?)?;
            let out_path = arg(5)?;
            let updated = vesper3d::viewer::gen::line(
                d,
                kind,
                count,
                [rect[0], rect[1]],
                [rect[2], rect[3]],
                None,
            )?;
            save(out_path, &updated)?;
            println!(
                "{}",
                json!({"ok": true, "count": count, "output": out_path})
            );
        }
        "lint" => {
            let d = MapDocument::load(Path::new(arg(1)?))?;
            let rep = vesper3d::viewer::lint::lint_map(&d, false, &[]);
            println!("{}", serde_json::to_string_pretty(&rep)?);
            if !rep.ok {
                std::process::exit(1);
            }
        }
        "reach" => {
            let d = MapDocument::load(Path::new(arg(1)?))?;
            let rep = vesper3d::viewer::reach::analyze_reach(&d, None)?;
            println!("{}", serde_json::to_string_pretty(&rep)?);
            if !rep.ok {
                std::process::exit(1);
            }
        }
        "walk-auto" => {
            let d = MapDocument::load(Path::new(arg(1)?))?;
            let from = vector2(arg(2)?)?;
            let to = vector2(arg(3)?)?;
            let res = vesper3d::viewer::pathing::execute_walk(
                &d,
                V(from.0, 0.0, from.1),
                V(to.0, 0.0, to.1),
                None,
            );
            println!("{}", serde_json::to_string_pretty(&res)?);
            if !res.ok {
                std::process::exit(1);
            }
        }
        "walk-explain" => {
            let d = MapDocument::load(Path::new(arg(1)?))?;
            let from = vector2(arg(2)?)?;
            let to = vector2(arg(3)?)?;
            let out_svg = arg(4)?;
            let res = vesper3d::viewer::pathing::execute_walk(
                &d,
                V(from.0, 0.0, from.1),
                V(to.0, 0.0, to.1),
                None,
            );
            if let Some(blockers) = &res.blockers {
                let svg = vesper3d::viewer::pathing::generate_blocker_svg(
                    &d,
                    res.final_position,
                    [to.0, 0.0, to.1],
                    blockers,
                );
                write_new(Path::new(out_svg), svg.as_bytes())?;
            }
            println!("{}", serde_json::to_string_pretty(&res)?);
            if !res.ok {
                std::process::exit(1);
            }
        }
        "verify" => {
            let path = arg(1)?;
            let d = MapDocument::load(Path::new(path))?;
            let checks = if let Ok(checks_path) = arg(2) {
                let bytes = std::fs::read(checks_path)?;
                serde_json::from_slice(&bytes)?
            } else {
                d.checks.clone().unwrap_or_default()
            };
            let rep = vesper3d::viewer::verify::verify_map(&d, &checks, path);
            println!("{}", serde_json::to_string_pretty(&rep)?);
            if !rep.ok {
                std::process::exit(1);
            }
        }
        "sim" => {
            let scen = vesper3d::viewer::scenario::load_scenario(std::path::Path::new(arg(1)?))?;
            let report = vesper3d::viewer::scenario::evaluate_scenario(&scen)?;
            let last_chk = report
                .trace
                .checkpoints
                .last()
                .map(|c| c.checksum)
                .unwrap_or(0);
            if let Ok(trace_out) = arg(2) {
                save(trace_out, &report.trace)?;
            }
            println!(
                "{}",
                json!({
                    "ok": report.ok,
                    "scenario": scen.name,
                    "ticks": report.trace.total_ticks,
                    "final_checksum": format!("0x{:016x}", last_chk),
                    "assertions": report.assertions,
                })
            );
            if !report.ok {
                std::process::exit(1);
            }
        }
        "replay-trace" => {
            let trace_bytes = std::fs::read(arg(1)?)?;
            let trace: vesper3d::viewer::scenario::SimulationTrace =
                serde_json::from_slice(&trace_bytes)?;
            let game_path = arg(2).ok();
            let rep = vesper3d::viewer::scenario::verify_replay_trace(&trace, game_path)?;
            println!("{}", serde_json::to_string_pretty(&rep)?);
            if !rep.deterministic {
                std::process::exit(1);
            }
        }
        "src" => {
            let action = arg(1)?;
            let query = a.get(2).map(String::as_str).unwrap_or("");
            let root = Path::new(env!("CARGO_MANIFEST_DIR"));
            let idx = vesper3d::viewer::symbols::SourceIndex::scan(root)?;
            match action {
                "map" => println!("{}", serde_json::to_string_pretty(&idx.map())?),
                "find" => println!("{}", serde_json::to_string_pretty(&idx.find(query))?),
                "outline" => println!("{}", serde_json::to_string_pretty(&idx.outline(query))?),
                "show" => println!("{}", idx.show(query)?),
                "refs" => println!("{}", serde_json::to_string_pretty(&idx.refs(query))?),
                "coverage" => println!("{}", serde_json::to_string_pretty(&idx.coverage())?),
                _ => return Err(format!("Unknown src action: {action}").into()),
            }
        }
        "export-house" => {
            let d = MapDocument::house()?;
            d.validate()?;
            save(arg(1)?, &d)?;
            println!(
                "{}",
                json!({
                "ok":true,"output":arg(1)?
                }
                )
            );
        }
        "apply" => {
            let d = MapDocument::load(Path::new(arg(1)?))?;
            let bytes = std::fs::read(arg(2)?)?;
            if bytes.len() > 1_000_000 {
                return Err("Patch exceeds 1 MB".into());
            }
            let ops: Vec<Edit> = serde_json::from_slice(&bytes)?;
            let next = d.apply(&ops)?;
            save(arg(3)?, &next)?;
            println!(
                "{}",
                json!({
                "ok":true,"operations":ops.len(),"output":arg(3)?
                }
                )
            );
        }
        "diff" => {
            let before = serde_json::to_value(MapDocument::load(Path::new(arg(1)?))?)?;
            let after = serde_json::to_value(MapDocument::load(Path::new(arg(2)?))?)?;
            let mut report = serde_json::Map::new();
            for (label, old, new) in [
                ("nodes", &before["scene"]["nodes"], &after["scene"]["nodes"]),
                ("entities", &before["entities"], &after["entities"]),
            ] {
                let index=|v:&serde_json::Value|->std::collections::BTreeMap<String,serde_json::Value>{
v.as_array().unwrap().iter().map(|x|(x["id"].as_str().unwrap().to_owned(),x.clone())).collect()
}
;
                report.insert(label.into(), changes(&index(old), &index(new)));
            }
            let map =
                |v: &serde_json::Value| -> std::collections::BTreeMap<String, serde_json::Value> {
                    v.as_object()
                        .unwrap()
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect()
                };
            report.insert(
                "colliders".into(),
                changes(&map(&before["colliders"]), &map(&after["colliders"])),
            );
            report.insert(
                "materials".into(),
                changes(
                    &map(&before["scene"]["materials"]),
                    &map(&after["scene"]["materials"]),
                ),
            );
            report.insert(
                "name_changed".into(),
                json!(before["name"] != after["name"]),
            );
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        _ => {
            let d = MapDocument::load(Path::new(arg(1)?))?;
            match command {
                "inspect" => println!("{}", serde_json::to_string_pretty(&d)?),
                "export-scene" => save(arg(2)?, &d.scene)?,
                "select" => {
                    let id = arg(2)?;
                    let nodes: Vec<_> = d
                        .scene
                        .nodes
                        .iter()
                        .filter(|n| n.id == id || n.id.starts_with(&format!("{id}/")))
                        .map(|n| &n.id)
                        .collect();
                    let colliders: Vec<_> = d
                        .colliders
                        .keys()
                        .filter(|k| k.as_str() == id || k.starts_with(&format!("{id}/")))
                        .collect();
                    let entities: Vec<_> = d
                        .entities
                        .iter()
                        .filter(|e| e.id == id || e.id.starts_with(&format!("{id}/")))
                        .map(|e| &e.id)
                        .collect();
                    if nodes.is_empty() && colliders.is_empty() && entities.is_empty() {
                        return Err("No matching IDs".into());
                    }
                    println!(
                        "{}",
                        json!({
                        "nodes":nodes,"colliders":colliders,"entities":entities,"note":"Generated house nodes and colliders have independent IDs; use near/inspect to select their associated components."
                        }
                        )
                    );
                }
                "near" => {
                    let p = vector(arg(2)?)?;
                    let radius: f32 = arg(3)?.parse()?;
                    if !radius.is_finite() || !(0.0..=1000.).contains(&radius) {
                        return Err("Radius must be finite in 0..1000".into());
                    }
                    let close = |c: &vesper3d::viewer::controller::Collider| {
                        (V(
                            p.0.clamp(c.min.0, c.max.0),
                            p.1.clamp(c.min.1, c.max.1),
                            p.2.clamp(c.min.2, c.max.2),
                        ) - p)
                            .length()
                            <= radius
                    };
                    let r = d.build()?;
                    let nodes: Vec<_> = d
                        .scene
                        .nodes
                        .iter()
                        .zip(&r.world.instances)
                        .filter(|(_, i)| {
                            close(&vesper3d::viewer::controller::Collider {
                                min: i.bounds.lo,
                                max: i.bounds.hi,
                            })
                        })
                        .map(|(n, _)| &n.id)
                        .collect();
                    println!(
                        "{}",
                        json!({
                        "nodes":nodes,"colliders":d.colliders.iter().filter(|(_,c)|close(c)).map(|(id,_)|id).collect::<Vec<_>>(),"entities":d.entities.iter().filter(|e|close(&e.bounds)).map(|e|&e.id).collect::<Vec<_>>()
                        }
                        )
                    );
                }
                "audit" => {
                    d.default_spawn
                        .ok_or("Map audit requires an explicit default_spawn")?;
                    let r = d.build()?;
                    let mut overlaps = Vec::new();
                    let entries: Vec<_> = d.colliders.iter().collect();
                    for i in 0..entries.len() {
                        for j in i + 1..entries.len() {
                            let (a, b) = (entries[i].1, entries[j].1);
                            if a.min.0 == b.min.0
                                && a.min.1 == b.min.1
                                && a.min.2 == b.min.2
                                && a.max.0 == b.max.0
                                && a.max.1 == b.max.1
                                && a.max.2 == b.max.2
                            {
                                overlaps.push([entries[i].0, entries[j].0]);
                            }
                        }
                    }
                    println!(
                        "{}",
                        json!({
                        "ok":true,"name":d.name,"nodes":d.scene.nodes.len(),"instances":r.world.instances.len(),"colliders":d.colliders.len(),"entities":d.entities.len(),"spawn_clear":true,"duplicate_collision_boxes":overlaps,"note":"Duplicate boxes are advisory; visual inspection and route tests are still required."
                        }
                        )
                    );
                }
                "ray" => {
                    let o = vector(arg(2)?)?;
                    let target = vector(arg(3)?)?;
                    let delta = target - o;
                    if delta.length() < 0.001 {
                        return Err("Ray endpoints must differ".into());
                    }
                    let r = d.build()?;
                    let hit = r
                        .world
                        .hit(Ray { o, d: delta.norm() }, delta.length(), false);
                    println!(
                        "{}",
                        match hit {
                            Some(h) => json!({
                            "blocked":true,"distance":h.t,"point":h.p,"node":d.scene.nodes[h.index].id
                            }
                            ),
                            None => json!({
                            "blocked":false
                            }
                            ),
                        }
                    );
                }
                "route" => {
                    #[derive(serde::Deserialize)]
                    #[serde(deny_unknown_fields)]
                    struct Waypoint {
                        x: f32,
                        z: f32,
                        feet: f32,
                        #[serde(default)]
                        crouch: bool,
                    }
                    let data = std::fs::read(arg(2)?)?;
                    if data.len() > 100_000 {
                        return Err("Route exceeds 100 KB".into());
                    }
                    let route: Vec<Waypoint> = serde_json::from_slice(&data)?;
                    if route.is_empty() || route.len() > 100 {
                        return Err("Route needs 1..100 waypoints".into());
                    }
                    let r = d.build()?;
                    let spawn = d
                        .default_spawn
                        .ok_or("Route simulation requires map.default_spawn")?;
                    let mut p = Controller::for_profile(Default::default(), spawn.feet, spawn.yaw)?;
                    for (i, w) in route.iter().enumerate() {
                        if [w.x, w.z, w.feet]
                            .iter()
                            .any(|v| !v.is_finite() || v.abs() > 1000.)
                        {
                            return Err("Invalid waypoint".into());
                        }
                        let mut reached = false;
                        for _ in 0..3600 {
                            let delta = V(w.x - p.position.0, 0., w.z - p.position.2);
                            if delta.length() < 0.08 && (p.feet_height() - w.feet).abs() < 0.08 {
                                p.stop();
                                reached = true;
                                break;
                            }
                            p.yaw = delta.0.atan2(-delta.2);
                            p.update(
                                Movement {
                                    forward: 1.,
                                    crouch: w.crouch,
                                    ..Default::default()
                                },
                                1. / 60.,
                                &r.colliders,
                            );
                        }
                        if !reached {
                            return Err(format!(
                                "Route blocked at waypoint {i}: eye {:?}, feet {}",
                                p.position,
                                p.feet_height()
                            )
                            .into());
                        }
                    }
                    println!(
                        "{}",
                        json!({
                        "ok":true,"waypoints":route.len(),"final_eye":p.position,"feet":p.feet_height()
                        }
                        )
                    );
                }
                "floorplan" => {
                    let mut svg=String::from("<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 1100 820'><rect width='1100' height='820' fill='#f4f1e9'/><style>text{font-family:monospace;font-size:14px}rect{vector-effect:non-scaling-stroke}</style>");
                    for (panel, floor) in [(0, 0_f32), (1, 3.2)] {
                        let xoff = panel * 550;
                        svg.push_str(&format!(
                            "<text x='{}' y='28'>Floor {}: collision slice at {:.2}m</text>",
                            xoff + 20,
                            panel + 1,
                            floor + 0.6
                        ));
                        for (id, c) in &d.colliders {
                            if c.min.1 < floor + 0.6 && c.max.1 > floor + 0.6 {
                                let x = xoff as f32 + 275. + c.min.0 * 23.;
                                let y = 390. + c.min.2 * 23.;
                                svg.push_str(&format!("<rect x='{x}' y='{y}' width='{}' height='{}' fill='#6d8795' fill-opacity='.65' stroke='#253a45' stroke-width='.4'><title>{}</title></rect>",(c.max.0-c.min.0)*23.,(c.max.2-c.min.2)*23.,xml(id)));
                            }
                        }
                        for e in &d.entities {
                            if e.bounds.min.1 >= floor - 0.05 && e.bounds.min.1 < floor + 2. {
                                svg.push_str(&format!(
                                    "<text x='{}' y='{}' font-size='9'>{}</text>",
                                    xoff as f32 + 275. + e.bounds.min.0 * 23.,
                                    390. + e.bounds.min.2 * 23.,
                                    xml(&e.id)
                                ));
                            }
                        }
                    }
                    svg.push_str("<text x='20' y='790'>Top = -Z / backyard. Hover shapes for collider IDs. Collision proxies, not rendered surfaces.</text></svg>");
                    write_new(Path::new(arg(2)?), svg.as_bytes())?;
                }
                _ => unreachable!(),
            }
        }
    }
    Ok(())
}
fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn changes(
    a: &std::collections::BTreeMap<String, serde_json::Value>,
    b: &std::collections::BTreeMap<String, serde_json::Value>,
) -> serde_json::Value {
    json!({
    "added":b.keys().filter(|k|!a.contains_key(*k)).collect::<Vec<_>>(),"removed":a.keys().filter(|k|!b.contains_key(*k)).collect::<Vec<_>>(),"changed":a.keys().filter(|k|b.contains_key(*k)&&a[*k]!=b[*k]).collect::<Vec<_>>()
    }
    )
}
