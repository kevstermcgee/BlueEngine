//! A handful of named timing spans and one compact summary, for seeing where a tick goes.
//!
//! ```
//! use vesper3d::viewer::spans as profile;
//! profile::enable(true);
//! {
//!     let _span = profile::span("my_phase");
//!     // ... work ...
//! }
//! let summary = profile::summary();
//! assert_eq!(summary.spans[0].name, "my_phase");
//! assert_eq!(summary.spans[0].count, 1);
//! profile::enable(false);
//! profile::reset();
//! ```
//!
//! Off by default. A span then costs one relaxed atomic load and records nothing. Turned on it costs two clock
//! reads and an uncontended lock on the calling thread's own table (about 50 ns); threads never contend while
//! spans run. Every thread registers its table, and [`summary`] reads all of them, including those of threads that
//! have already ended, so spans on `std::thread::scope` workers are always counted (a scope can return before
//! thread-local destructors run, so nothing here depends on them). There is no timeline and no per-event data:
//! the point is one small table (`name count total mean max`, biggest first) that a person or an agent can read
//! without a viewer. Nested spans are each timed in full, so totals of nested spans overlap; name the phases so
//! that they do not, or read a parent and its children together.
use serde::Serialize;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

static ENABLED: AtomicBool = AtomicBool::new(false);
/// Totals of threads that have ended, merged when a summary is taken or the registry grows.
static FINISHED: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
static REGISTRY: Mutex<Vec<Arc<Table>>> = Mutex::new(Vec::new());
/// Past this many registered tables, ended threads' tables are folded into [`FINISHED`] on registration, so a
/// long run that creates worker threads every tick cannot grow without bound.
const PRUNE_AT: usize = 256;

#[derive(Clone, Copy)]
struct Entry {
    name: &'static str,
    count: u64,
    total_ns: u64,
    max_ns: u64,
}

impl Entry {
    fn add(&mut self, other: &Entry) {
        self.count += other.count;
        self.total_ns += other.total_ns;
        self.max_ns = self.max_ns.max(other.max_ns);
    }
}

fn merge(into: &mut Vec<Entry>, from: &[Entry]) {
    for e in from {
        match into.iter_mut().find(|g| g.name == e.name) {
            Some(g) => g.add(e),
            None => into.push(*e),
        }
    }
}

/// One thread's spans. Only its owner writes it while spans run, so its lock is uncontended.
#[derive(Default)]
struct Table(Mutex<Vec<Entry>>);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Fold the tables of threads that have ended (only the registry still holds them) into [`FINISHED`].
fn prune(registry: &mut Vec<Arc<Table>>) {
    let mut finished = lock(&FINISHED);
    registry.retain(|table| {
        let ended = Arc::strong_count(table) == 1;
        if ended {
            merge(&mut finished, &lock(&table.0));
        }
        !ended
    });
}

fn register() -> Arc<Table> {
    let table = Arc::new(Table::default());
    let mut registry = lock(&REGISTRY);
    if registry.len() >= PRUNE_AT {
        prune(&mut registry);
    }
    registry.push(Arc::clone(&table));
    table
}

thread_local! {
    static MINE: Arc<Table> = register();
}

/// Start or stop recording. Spans created while off record nothing, even if recording starts before they end.
pub fn enable(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Discard everything recorded so far, on every thread.
pub fn reset() {
    let mut registry = lock(&REGISTRY);
    for table in registry.iter() {
        lock(&table.0).clear();
    }
    prune(&mut registry);
    lock(&FINISHED).clear();
}

/// Times until dropped. Create it with [`span`].
#[must_use = "a span measures until it is dropped; bind it to a variable"]
pub struct Span(Option<(&'static str, Instant)>);

/// Time the rest of the current scope under `name`. `name` should be a short stable identifier such as
/// `"world.physics"`; spans with the same name are added together.
pub fn span(name: &'static str) -> Span {
    Span(enabled().then(|| (name, Instant::now())))
}

impl Drop for Span {
    fn drop(&mut self) {
        let Some((name, started)) = self.0 else {
            return;
        };
        let ns = started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        // `try_with`: a span ending during thread teardown is dropped rather than panicking.
        let _ = MINE.try_with(|table| {
            let mut entries = lock(&table.0);
            match entries
                .iter_mut()
                .find(|e| std::ptr::eq(e.name, name) || e.name == name)
            {
                Some(e) => {
                    e.count += 1;
                    e.total_ns += ns;
                    e.max_ns = e.max_ns.max(ns);
                }
                None => entries.push(Entry {
                    name,
                    count: 1,
                    total_ns: ns,
                    max_ns: ns,
                }),
            }
        });
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SpanStats {
    pub name: &'static str,
    pub count: u64,
    pub total_us: f64,
    pub mean_us: f64,
    pub max_us: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Summary {
    /// Biggest total first.
    pub spans: Vec<SpanStats>,
}

impl Summary {
    /// `name count total_ms mean_us max_us`, one line per span, biggest total first.
    pub fn text(&self) -> String {
        let mut out = String::from("span count total_ms mean_us max_us\n");
        for s in &self.spans {
            out.push_str(&format!(
                "{} {} {:.3} {:.1} {:.1}\n",
                s.name,
                s.count,
                s.total_us / 1000.,
                s.mean_us,
                s.max_us
            ));
        }
        out
    }
}

/// Everything recorded since the last [`reset`], on every thread, running or ended.
pub fn summary() -> Summary {
    let mut all: Vec<Entry> = Vec::new();
    {
        let mut registry = lock(&REGISTRY);
        prune(&mut registry);
        for table in registry.iter() {
            merge(&mut all, &lock(&table.0));
        }
    }
    merge(&mut all, &lock(&FINISHED));
    let mut spans: Vec<SpanStats> = all
        .iter()
        .map(|e| SpanStats {
            name: e.name,
            count: e.count,
            total_us: e.total_ns as f64 / 1e3,
            mean_us: e.total_ns as f64 / 1e3 / e.count.max(1) as f64,
            max_us: e.max_ns as f64 / 1e3,
        })
        .collect();
    spans.sort_by(|a, b| b.total_us.total_cmp(&a.total_us).then(a.name.cmp(b.name)));
    Summary { spans }
}
