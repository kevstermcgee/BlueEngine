//! Comparable release workloads; requested allocations are instrumented, not allocator/GPU totals.
use automation_workshop::{Command, Envelope, Workshop};
use std::{
    alloc::{GlobalAlloc, Layout, System},
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
fn rss() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:")?.split_whitespace().next()?.parse::<u64>().ok()?.checked_mul(1024))
}
fn act(game: &mut Workshop, serial: &mut u64, commands: Vec<Command>) {
    let input = commands
        .into_iter()
        .map(|command| {
            *serial += 1;
            Envelope { seat: 0, serial: *serial, command }
        })
        .collect();
    game.step(&input);
    assert!(game.outcomes.iter().all(|r| r.ok), "{:?}", game.outcomes);
}
fn percentile(values: &mut [u64]) -> serde_json::Value {
    values.sort_unstable();
    let n = values.len();
    serde_json::json!({"p50":values[n/2],"p95":values[n*95/100],"p99":values[n*99/100],"max":values[n-1]})
}
fn main() {
    let ticks: usize = std::env::args().nth(1).unwrap_or_else(|| "50000".into()).parse().expect("tick count");
    assert!((1000..=1000000).contains(&ticks));
    let mut rows = Vec::new();
    for (name, count) in [("blocked", 1000), ("active", 2), ("mixed", 1000)] {
        let mut game = Workshop::with_machine_count(count).unwrap();
        let mut serial = 0;
        let machines: Vec<_> = game.state.machines.keys().cloned().collect();
        if name != "active" {
            for machine in &machines {
                act(
                    &mut game,
                    &mut serial,
                    vec![
                        Command::Queue { machine: machine.clone(), recipe: "workshop:smelt".into() },
                        Command::Gather { item: "workshop:ore".into(), amount: 2 },
                        Command::Gather { item: "workshop:coal".into(), amount: 1 },
                    ],
                );
            }
            for _ in 0..8 {
                game.step(&vec![]);
            }
            assert!(game.state.machines.values().all(|m| m.blocked.is_some()));
        }
        let mut times = Vec::with_capacity(ticks);
        let mut save_times = Vec::new();
        let mut load_times = Vec::new();
        let mut rss_series = Vec::new();
        let mut calls = 0;
        let mut bytes = 0;
        let mut accepted = 0;
        let mut refused = 0;
        let mut save_bytes = 0;
        let start = Instant::now();
        for t in 0..ticks + 2000 {
            let input = if name == "blocked" {
                vec![]
            } else {
                let command = if name == "mixed" && t % 17 == 0 {
                    Command::Cancel { machine: machines[t % count].clone() }
                } else if game.state.inventory.total_units() > 8 {
                    let (id, _) = game.state.inventory.counts().iter().max_by_key(|(_, v)| **v).unwrap();
                    Command::Deliver { item: id.clone(), amount: 1 }
                } else if game.state.inventory.quantity("workshop:ore") < 2 {
                    Command::Gather { item: "workshop:ore".into(), amount: 2 }
                } else if game.state.inventory.quantity("workshop:coal") < 1 {
                    Command::Gather { item: "workshop:coal".into(), amount: 1 }
                } else {
                    Command::Queue { machine: machines[t % count].clone(), recipe: "workshop:smelt".into() }
                };
                serial += 1;
                vec![Envelope { seat: 0, serial, command }]
            };
            let c = CALLS.load(Ordering::Relaxed);
            let b = BYTES.load(Ordering::Relaxed);
            let tick_start = Instant::now();
            game.step(&input);
            let elapsed = tick_start.elapsed().as_nanos() as u64;
            if t >= 2000 {
                times.push(elapsed);
                calls += CALLS.load(Ordering::Relaxed) - c;
                bytes += BYTES.load(Ordering::Relaxed) - b;
                accepted += game.outcomes.iter().filter(|o| o.ok).count();
                refused += game.outcomes.iter().filter(|o| !o.ok).count();
            }
            if t % 2500 == 0 && t >= 2000 {
                let hash = game.state_hash();
                let at = Instant::now();
                let saved = snapshot::save(&game, "Sustained workload").unwrap();
                save_times.push(at.elapsed().as_nanos() as u64);
                save_bytes = saved.len();
                let at = Instant::now();
                snapshot::restore(&mut game, &saved).unwrap();
                load_times.push(at.elapsed().as_nanos() as u64);
                assert_eq!(hash, game.state_hash());
                rss_series.push(rss());
            }
        }
        rows.push(serde_json::json!({"workload":name,"machines":count,"warmup":2000,"samples":ticks,"tick_ns":percentile(&mut times),"save_ns":percentile(&mut save_times),"load_ns":percentile(&mut load_times),"save_load_samples":save_times.len(),"save_bytes":save_bytes,"tick_allocation_calls":calls,"tick_requested_bytes":bytes,"requested_live_bytes":LIVE.load(Ordering::Relaxed),"linux_rss_series":rss_series,"accepted_commands":accepted,"refused_commands":refused,"elapsed_seconds":start.elapsed().as_secs_f64(),"final_hash":format!("{:016x}",game.state_hash())}));
    }
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"profile":"release","instrumentation":"System allocator requested sizes; per-tick allocations exclude player input construction and save/load; no GPU","workloads":rows})).unwrap());
}
