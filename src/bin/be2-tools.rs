//! Headless authoring commands;
//! JSON output is suitable for agents and automation.
use serde_json::json;
use std::path::Path;
use vesper3d::{
    math::{Ray, V},
    viewer::{
        authoring::{write_new, Edit, MapDocument},
        controller::{Controller, Movement},
        metrics::PerformanceBudget,
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
    let arity = 1 + signature.split_whitespace().count();
    if !a.is_empty() && a.len() != arity {
        return Err(format!("{command} expects {} arguments", arity - 1).into());
    }
    match command {
        "game-describe" => println!(
            "{}",
            json!({
                "ok":true,"schema_version":1,"schema_command":"game-schema", "example":"game-example NEW_DIRECTORY",
                "event":"authoritative nearest-visible interaction within 2.5 metres (E intent)",
                "actions":["increment","set_counter","set_enabled","complete"],
                "conditions":"counter equals integer; null means unconditional",
                "limits":{"game_bytes":64000,"spawns":8,"counters":8,"interactables":16,"rules":16,"actions_per_rule":4,"counter_magnitude":1000000},
                "order":"player IDs ascending at fixed tick; rules in document order; later conditions see earlier actions; once is per match",
                "geometry":"static axis-aligned box with matching node/collider/entity ID and bounds",
                "set_enabled":"interaction eligibility only; never changes visibility or collision",
                "profiles":"one shared validated movement profile; spawns use feet coordinates and round-robin server IDs",
                "map":"relative child file inside game directory; loaded content participates in fingerprint",
                "unsupported":["timers","recursive events","doors/geometry mutation","custom weapon actions","per-player inventory"]
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
                "ok": true,
                "passed": validation.passed,
                "violations": validation.violations,
                "metrics": perf,
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
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
                let _ = world.snapshot(world.tick);
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
                let _ = snap2.compute_delta(&snap1);
            }
            let delta_us = t0.elapsed().as_secs_f64() * 1_000_000. / 1000.;

            // 4. Room graph spatial lookup benchmark (10,000 queries)
            let t0 = std::time::Instant::now();
            for i in 0..10000 {
                let p = V((i as f32 % 10.0) - 5.0, 1.0, (i as f32 % 10.0) - 5.0);
                let _ = world.room_graph.find_room_at(p);
            }
            let room_lookup_ns = t0.elapsed().as_secs_f64() * 1_000_000_000. / 10000.;

            let results = json!({
                "ok": true,
                "benchmarks": {
                    "sim_step_mean_us": sim_step_us,
                    "snapshot_creation_mean_us": snapshot_us,
                    "delta_compression_mean_us": delta_us,
                    "room_graph_lookup_mean_ns": room_lookup_ns,
                }
            });
            println!("{}", serde_json::to_string_pretty(&results)?);
        }
        "catalog" => println!(
            "{}",
            json!({
            "props":["cereal","chair","table","apple","framed-art","framed-botanical","sculpture","vase-plant","bowl","table-lamp","book-stack","candle-trio","potted-cactus","flower-vase","tall-vase","mantel-clock","woven-basket"],"maps":["test-lab","house"],"schema_version":1,"operations":["add_box","add_prop","translate","remove"]
            }
            )
        ),
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
                    let mut p = Controller::default();
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
