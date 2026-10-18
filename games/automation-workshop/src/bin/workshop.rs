//! Bounded public-action probes and explicit content/save migration; no privileged debug mutation.
use automation_workshop::{content, persistence, Command, Definitions, Envelope, Workshop, MAX_STATE_BYTES};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
use vesper3d::runtime::{snapshot, Simulation};

struct MeasuredAllocator;
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
fn allocated(size: usize) {
    CALLS.fetch_add(1, Ordering::Relaxed);
    BYTES.fetch_add(size, Ordering::Relaxed);
    let live = LIVE.fetch_add(size, Ordering::Relaxed) + size;
    PEAK.fetch_max(live, Ordering::Relaxed);
}
// Instrument requested allocation sizes, including reallocations; allocator metadata is excluded.
unsafe impl GlobalAlloc for MeasuredAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = System.alloc(layout);
        if !pointer.is_null() {
            allocated(layout.size());
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = System.alloc_zeroed(layout);
        if !pointer.is_null() {
            allocated(layout.size());
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        System.dealloc(pointer, layout);
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let next = System.realloc(pointer, layout, size);
        if !next.is_null() {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
            allocated(size);
        }
        next
    }
}
#[global_allocator]
static ALLOCATOR: MeasuredAllocator = MeasuredAllocator;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn argument<'a>(args: &'a [String], index: usize, help: &str) -> Result<&'a str, String> {
    args.get(index).map(String::as_str).ok_or_else(|| help.to_owned())
}
fn save_bytes(path: &str) -> Result<Vec<u8>, String> {
    let file = Path::new(path);
    if file.metadata().map_err(|e| format!("{path}: {e}"))?.len() > MAX_STATE_BYTES as u64 {
        return Err(format!("WORKSHOP_SAVE_LIMIT: {path}: max {MAX_STATE_BYTES} bytes"));
    }
    std::fs::read(file).map_err(|e| format!("{path}: {e}"))
}
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| matches!(s.as_str(), "--help" | "-h" | "help")) {
        println!("workshop: validate [CONTENT] | inspect [MACHINE] | edit CONTENT ADDITIONS | play | probe | playtest SEED TICKS [fault] | replay-playtest TRACE | migrate-v1 SAVE OUTPUT | migrate-content SOURCE SAVE OUTPUT [TARGET]");
        return Ok(());
    }
    let mut game = Workshop::standard();
    let output = match args.first().map(String::as_str).unwrap_or("inspect") {
        "validate" => {
            if let Some(path) = args.get(1) { game = Workshop::new(Definitions::load(Path::new(path)).map_err(|e| e.to_string())?, 2)?; }
            serde_json::json!({"ok":true,"content":game.definitions.fingerprint(),"version":game.definitions.content_version})
        }
        "inspect" => game.inspect(args.get(1).map(String::as_str).unwrap_or("workshop:machine-a"))?,
        "edit" => {
            let path = argument(&args, 1, "edit CONTENT_JSON ADDITIONS_JSON")?;
            let additions = argument(&args, 2, "edit CONTENT_JSON ADDITIONS_JSON")?;
            if Path::new(additions).metadata().map_err(|e| e.to_string())?.len() > 65536 { return Err("WORKSHOP_CONTENT_LIMIT: additions exceed 64 KiB".into()); }
            let addition = serde_json::from_slice(&std::fs::read(additions).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            content::edit(Path::new(path), &addition)?
        }
        "migrate-v1" => {
            let original = save_bytes(argument(&args, 1, "migrate-v1 SAVE OUTPUT_DIRECTORY")?)?;
            let destination = Path::new(argument(&args, 2, "migrate-v1 SAVE OUTPUT_DIRECTORY")?);
            persistence::migrate_v1(&mut game, &original, destination)?;
            serde_json::json!({"ok":true,"output":destination.join("migrated-v2.be2save"),"tick":game.state.tick,"content":game.definitions.fingerprint()})
        }
        "migrate-content" => {
            let source = Definitions::load(Path::new(argument(&args, 1, "migrate-content SOURCE_CONTENT SAVE OUTPUT_DIRECTORY [TARGET_CONTENT]")?)).map_err(|e| e.to_string())?;
            let original = save_bytes(argument(&args, 2, "migrate-content SOURCE_CONTENT SAVE OUTPUT_DIRECTORY [TARGET_CONTENT]")?)?;
            let destination = Path::new(argument(&args, 3, "migrate-content SOURCE_CONTENT SAVE OUTPUT_DIRECTORY [TARGET_CONTENT]")?);
            if let Some(target) = args.get(4) { game = Workshop::new(Definitions::load(Path::new(target)).map_err(|e| e.to_string())?, 2)?; }
            persistence::migrate_content(&mut game, &original, source, destination, "migrated-content")?;
            serde_json::json!({"ok":true,"output":destination.join("migrated-content.be2save"),"tick":game.state.tick,"content":game.definitions.fingerprint()})
        }
        "playtest" => {
            let seed = argument(&args, 1, "playtest SEED TICKS [fault]")?.parse().map_err(|_|"invalid seed")?;
            let ticks: usize = argument(&args, 2, "playtest SEED TICKS [fault]")?.parse().map_err(|_|"invalid ticks")?;
            if !(22..=100000).contains(&ticks) { return Err("ticks must be 22..100000".into()); }
            let report = automation_workshop::playtest::run(seed,ticks,args.get(3).is_some_and(|s|s=="fault"));
            serde_json::to_value(report).map_err(|e|e.to_string())?
        }
        "replay-playtest" => {
            let path = argument(&args,1,"replay-playtest TRACE_JSON")?;
            let bytes = std::fs::read(path).map_err(|e|e.to_string())?;
            if bytes.len()>64*1024*1024 { return Err("trace exceeds 64 MiB".into()); }
            let trace=serde_json::from_slice(&bytes).map_err(|e|e.to_string())?;
            serde_json::to_value(automation_workshop::playtest::replay(trace)).map_err(|e|e.to_string())?
        }
        "probe" => probe()?,
        "play" => {
            let mut serial = 0;
            actions(&mut game, &mut serial, vec![queue("workshop:machine-a"), queue("workshop:machine-b")])?;
            actions(&mut game, &mut serial, vec![Command::Gather{item:"workshop:ore".into(),amount:4},Command::Gather{item:"workshop:coal".into(),amount:2}])?;
            for _ in 0..8 { game.step(&vec![]); }
            let blocked = game.inspect("workshop:machine-a")?;
            actions(&mut game, &mut serial, vec![Command::Cancel{machine:"workshop:machine-a".into()}])?;
            let refund = game.inspect("workshop:machine-a")?;
            actions(&mut game, &mut serial, vec![Command::Deliver{item:"workshop:ore".into(),amount:6}])?;
            let saved = snapshot::save(&game, "Recovered workshop").map_err(|e| e.to_string())?;
            let mut restored = Workshop::standard();
            snapshot::restore(&mut restored, &saved).map_err(|e| e.to_string())?;
            if restored.state_hash() != game.state_hash() { return Err("WORKSHOP_RECOVERY: hash mismatch".into()); }
            serde_json::json!({"blocked":blocked,"pending_refund":refund,"recovered":game.inspect("workshop:machine-a")?,"other_machine":game.inspect("workshop:machine-b")?,"save_bytes":saved.len(),"restored_hash":restored.state_hash()})
        }
        _ => return Err("Commands: validate [CONTENT], inspect [MACHINE-ID], play, probe, edit CONTENT_JSON ADDITIONS_JSON, migrate-v1 SAVE OUTPUT_DIRECTORY, migrate-content SOURCE_CONTENT SAVE OUTPUT_DIRECTORY [TARGET_CONTENT]. V1 fixture generation lives at commit d7e4188.".into()),
    };
    println!("{}", serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?);
    if output.get("ok") == Some(&serde_json::Value::Bool(false)) {
        std::process::exit(1);
    }
    Ok(())
}
fn queue(machine: &str) -> Command {
    Command::Queue { machine: machine.into(), recipe: "workshop:smelt".into() }
}
fn actions(game: &mut Workshop, serial: &mut u64, commands: Vec<Command>) -> Result<(), String> {
    let input = commands
        .into_iter()
        .map(|command| {
            *serial += 1;
            Envelope { seat: 0, serial: *serial, command }
        })
        .collect();
    game.step(&input);
    if let Some(failed) = game.outcomes.iter().find(|outcome| !outcome.ok) {
        return Err(failed.detail.clone());
    }
    Ok(())
}
fn percentile(sorted: &[u128], percentage: usize) -> u128 {
    sorted[(sorted.len() * percentage).div_ceil(100).saturating_sub(1)]
}
fn resident_bytes() -> Option<u64> {
    // Linux reports resident KiB here. Other platforms are explicitly unavailable, not zero.
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:")?.split_whitespace().next()?.parse::<u64>().ok()?.checked_mul(1024))
}
fn probe() -> Result<serde_json::Value, String> {
    const TICK_BUDGET_US: u128 = 16667;
    let mut results = Vec::new();
    for count in [10, 100, 1000] {
        let setup = Instant::now();
        let mut game = Workshop::with_machine_count(count)?;
        let mut serial = 0;
        let machines: Vec<_> = game.state.machines.keys().cloned().collect();
        for machine in &machines {
            actions(
                &mut game,
                &mut serial,
                vec![
                    queue(machine),
                    Command::Gather { item: "workshop:ore".into(), amount: 2 },
                    Command::Gather { item: "workshop:coal".into(), amount: 1 },
                ],
            )?;
        }
        for _ in 0..8 {
            game.step(&vec![]);
        }
        let setup_us = setup.elapsed().as_micros();
        if game.state.machines.values().any(|machine| machine.blocked.is_none()) {
            return Err("WORKSHOP_PROBE: expected every full-destination job to remain blocked".into());
        }
        let calls = CALLS.load(Ordering::Relaxed);
        let bytes = BYTES.load(Ordering::Relaxed);
        let baseline = LIVE.load(Ordering::Relaxed);
        PEAK.store(baseline, Ordering::Relaxed);
        let mut times = Vec::with_capacity(51);
        for _ in 0..51 {
            let start = Instant::now();
            game.step(&vec![]);
            times.push(start.elapsed().as_nanos());
        }
        let allocation_calls = CALLS.load(Ordering::Relaxed) - calls;
        let allocated_bytes = BYTES.load(Ordering::Relaxed) - bytes;
        let peak = PEAK.load(Ordering::Relaxed);
        times.sort_unstable();
        let start = Instant::now();
        let saved = snapshot::save(&game, "Load probe").map_err(|e| e.to_string())?;
        let save_us = start.elapsed().as_micros();
        let mut restored = Workshop::standard();
        let start = Instant::now();
        snapshot::restore(&mut restored, &saved).map_err(|e| e.to_string())?;
        let load_us = start.elapsed().as_micros();
        if game.state_hash() != restored.state_hash() {
            return Err("WORKSHOP_PROBE: save continuation hash differs".into());
        }
        let over_budget = saved.len() > MAX_STATE_BYTES || percentile(&times, 99) / 1000 > TICK_BUDGET_US;
        let mut row = serde_json::json!({"machines":count,"workload":"all machines have real paid smelt escrow; completed outputs blocked by a full shared destination","samples":51,"setup_us":setup_us,"tick_ns":{"p50":percentile(&times,50),"p95":percentile(&times,95),"p99":percentile(&times,99)},"allocation_calls_51_ticks":allocation_calls,"requested_allocation_bytes_51_ticks":allocated_bytes,"requested_live_bytes_before_ticks":baseline,"requested_peak_bytes_during_ticks":peak,"linux_resident_bytes":resident_bytes(),"save_bytes":saved.len(),"save_us":save_us,"load_us":load_us,"budget_exceeded":over_budget});
        #[cfg(feature = "online")]
        {
            let size = automation_workshop::net::snapshot_datagram_bytes(&game);
            row["online_snapshot_bytes"] = serde_json::json!(size);
            row["online_snapshot_fits_1100_bytes"] = serde_json::json!(size <= 1100);
        }
        #[cfg(not(feature = "online"))]
        {
            row["online_snapshot_bytes"] = serde_json::Value::Null;
        }
        results.push(row);
        if over_budget {
            break;
        }
    }
    Ok(
        serde_json::json!({"authority":"same Workshop::step as offline and custom netplay","runtime":"offline load ladder; online sizes are encoder-only diagnostics, with no network sends or supported-capacity claim","profile":if cfg!(debug_assertions){"debug assertions enabled"}else{"release"},"budgets":{"machines":1024,"jobs_per_machine":2,"commands_per_tick":32,"save_bytes":MAX_STATE_BYTES,"p99_tick_us":TICK_BUDGET_US},"allocation_measurement":"System allocator requested sizes including reallocations; excludes allocator metadata and GPU; RSS is Linux process resident size","capacity_claim":false,"results":results}),
    )
}
