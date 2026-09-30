//! Span profiling: off by default, cheap, aggregated by name, and complete across worker threads.
use std::{net::SocketAddr, sync::Mutex, time::Duration};
use vesper3d::viewer::{
    net::{Datagram, DatagramTransport, PROTOCOL_VERSION},
    server::DedicatedServer,
    simulation::HeadlessWorld,
    spans,
};

/// The recorder is global, so these tests take turns.
static TURN: Mutex<()> = Mutex::new(());
fn turn() -> std::sync::MutexGuard<'static, ()> {
    let guard = TURN.lock().unwrap_or_else(|p| p.into_inner());
    spans::enable(false);
    spans::reset();
    guard
}
fn find<'a>(s: &'a spans::Summary, name: &str) -> Option<&'a spans::SpanStats> {
    s.spans.iter().find(|x| x.name == name)
}

#[test]
fn nothing_is_recorded_while_off_and_a_span_begun_off_stays_unrecorded() {
    let _turn = turn();
    {
        let _a = spans::span("off");
    }
    let begun_off = spans::span("begun_off");
    spans::enable(true);
    drop(begun_off);
    assert!(spans::summary().spans.is_empty());
}

#[test]
fn spans_with_the_same_name_add_up_and_the_summary_is_biggest_first() {
    let _turn = turn();
    spans::enable(true);
    for _ in 0..3 {
        let _s = spans::span("slow");
        std::thread::sleep(Duration::from_millis(3));
    }
    {
        let _s = spans::span("quick");
    }
    let summary = spans::summary();
    let slow = find(&summary, "slow").unwrap();
    assert_eq!(slow.count, 3);
    assert!(slow.total_us >= 9_000.0 && slow.max_us >= 3_000.0 && slow.max_us <= slow.total_us);
    assert!((slow.mean_us - slow.total_us / 3.0).abs() < 1.0);
    assert_eq!(
        summary.spans.iter().map(|s| s.name).collect::<Vec<_>>(),
        ["slow", "quick"]
    );
    let text = summary.text();
    assert!(
        text.starts_with("span count total_ms mean_us max_us\nslow 3 "),
        "{text}"
    );
}

#[test]
fn nested_spans_are_each_timed_in_full() {
    let _turn = turn();
    spans::enable(true);
    {
        let _outer = spans::span("outer");
        let _inner = spans::span("inner");
        std::thread::sleep(Duration::from_millis(2));
    }
    let summary = spans::summary();
    assert!(find(&summary, "outer").unwrap().total_us >= 2_000.0);
    assert!(find(&summary, "inner").unwrap().total_us >= 2_000.0);
}

#[test]
fn worker_thread_spans_are_counted_after_the_workers_finish() {
    let _turn = turn();
    spans::enable(true);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                for _ in 0..25 {
                    let _s = spans::span("worker");
                }
            });
        }
    });
    let _s = spans::span("main");
    drop(_s);
    let summary = spans::summary();
    assert_eq!(find(&summary, "worker").unwrap().count, 100);
    assert_eq!(find(&summary, "main").unwrap().count, 1);
}

#[test]
fn reset_clears_everything() {
    let _turn = turn();
    spans::enable(true);
    drop(spans::span("x"));
    spans::reset();
    assert!(spans::summary().spans.is_empty());
}

#[test]
fn a_world_step_reports_its_phases() {
    let _turn = turn();
    spans::enable(true);
    let mut world = HeadlessWorld::new().unwrap();
    world.join(1);
    for _ in 0..10 {
        world.step();
    }
    let summary = spans::summary();
    for name in [
        "world.step",
        "world.rules",
        "world.players",
        "world.physics",
    ] {
        assert_eq!(
            find(&summary, name).map(|s| s.count),
            Some(10),
            "{name}: {:?}",
            summary.spans
        );
    }
}

#[derive(Default)]
struct Sink;
impl DatagramTransport for Sink {
    fn send(&self, _: SocketAddr, data: &[u8]) -> vesper3d::Result<usize> {
        Ok(data.len())
    }
    fn receive(&mut self) -> vesper3d::Result<Vec<Datagram>> {
        Ok(Vec::new())
    }
    fn local_addr(&self) -> vesper3d::Result<SocketAddr> {
        Ok("127.0.0.1:1".parse().unwrap())
    }
}

/// With peers prepared on worker threads every peer must still be counted: this is the path where a
/// thread-local recorder would silently lose the expensive part.
#[test]
fn every_peer_prepared_on_a_worker_thread_is_counted() {
    let _turn = turn();
    let peers = 48;
    for threads in [1, 4] {
        spans::reset();
        spans::enable(true);
        let mut server = DedicatedServer::with_transport(Sink, HeadlessWorld::new().unwrap())
            .unwrap()
            .with_max_players(peers)
            .with_network_threads(threads);
        let hash = server.world.content_hash;
        for i in 0..peers {
            let addr: SocketAddr = format!("10.0.0.{}:{}", i + 1, 4000 + i).parse().unwrap();
            server.handle_hello(addr, PROTOCOL_VERSION, i as u64 + 1, hash);
        }
        for _ in 0..5 {
            server.world.step();
            server.try_broadcast_snapshots().unwrap();
        }
        let summary = spans::summary();
        assert_eq!(
            find(&summary, "replication.stage").unwrap().count,
            (peers * 5) as u64,
            "{threads} threads"
        );
        assert_eq!(find(&summary, "server.broadcast.stage").unwrap().count, 5);
        assert_eq!(
            find(&summary, "server.broadcast.transmit").unwrap().count,
            5
        );
    }
}

/// A server that prepares peers on fresh scoped threads every broadcast creates thousands of threads; none of
/// their spans may be lost, and their tables must not pile up (they are folded away as they end).
#[test]
fn thousands_of_short_lived_threads_are_all_counted() {
    let _turn = turn();
    spans::enable(true);
    for _ in 0..50 {
        std::thread::scope(|scope| {
            for _ in 0..40 {
                scope.spawn(|| drop(spans::span("short")));
            }
        });
    }
    assert_eq!(find(&spans::summary(), "short").unwrap().count, 2000);
}
