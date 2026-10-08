//! Busy combat must not make state snapshots permanently exceed the datagram budget.
use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Instant;
use vesper3d::viewer::net::codec::{Reader, WireResult, Writer};
use vesper3d::viewer::net::loopback::{LoopEnd, LoopNet};
use vesper3d::viewer::net::{Datagram, DatagramTransport};
use vesper3d::viewer::netplay::{
    ClientConfig, ClientState, ClientView, NetClient, NetGame, NetServer, PredictionStats, Seat,
    ServerConfig,
};

struct CombatGame<
    const MIN: usize = 1,
    const LARGE: bool = false,
    const DURATION: u32 = 180,
    const CAPACITY: usize = 2,
>;
struct CombatView(u32);

struct QueuedDatagram {
    from: SocketAddr,
    to: SocketAddr,
    bytes: Vec<u8>,
}
struct OrderedEnd {
    inner: LoopEnd,
    outgoing: Rc<RefCell<Vec<QueuedDatagram>>>,
}
impl DatagramTransport for OrderedEnd {
    fn send(&self, peer: SocketAddr, data: &[u8]) -> vesper3d::Result<usize> {
        assert!(data.len() <= 1100, "actual transport payload ceiling");
        self.outgoing.borrow_mut().push(QueuedDatagram {
            from: self.inner.local_addr()?,
            to: peer,
            bytes: data.to_vec(),
        });
        Ok(data.len())
    }
    fn payload_limit(&self, _: SocketAddr) -> usize {
        1100
    }
    fn receive(&mut self) -> vesper3d::Result<Vec<Datagram>> {
        self.inner.receive()
    }
    fn local_addr(&self) -> vesper3d::Result<SocketAddr> {
        self.inner.local_addr()
    }
}

#[test]
fn previous_reliable_event_protocol_is_rejected_at_handshake() {
    use vesper3d::viewer::net::DatagramTransport;
    use vesper3d::viewer::netplay::wire::{decode_server, encode_client, ClientMsg, ServerMsg};
    let net = LoopNet::new(0, 0, 0., 81);
    let addr = |n: u16| SocketAddr::from(([127, 0, 0, 1], 35_000 + n));
    let mut server =
        NetServer::<CombatGame, _>::new(net.endpoint(addr(0)), ServerConfig::default()).unwrap();
    let mut client = net.endpoint(addr(1));
    let hello = encode_client::<CombatGame>(&ClientMsg::Hello {
        key: String::new(),
        name: "Old reliable client".into(),
        choice: 0,
        nonce: [1, 2],
        // Pinned NEV1 fingerprint for this game, before retention-aware gap framing.
        fingerprint: 0x8acf_92d1,
    });
    client.send(addr(0), &hello).unwrap();
    net.advance();
    server.poll(Instant::now());
    net.advance();
    let replies = client.receive().unwrap();
    assert_eq!(replies.len(), 1);
    assert!(matches!(
        decode_server::<CombatGame>(&replies[0].data).unwrap(),
        ServerMsg::Rejected { .. }
    ));
    assert_eq!(server.players(), 0);
}

impl<const MIN: usize, const LARGE: bool, const DURATION: u32, const CAPACITY: usize> NetGame
    for CombatGame<MIN, LARGE, DURATION, CAPACITY>
{
    type Input = u8;
    type Match = u32;
    type View = CombatView;
    type Snapshot = u32;
    type Event = u32;
    const NAME: &'static str = "event-budget-test";
    const MAX_SEATS: usize = 4;
    fn lobby_capacity() -> usize {
        CAPACITY
    }
    fn minimum_players() -> usize {
        MIN
    }
    const CHOICES: u8 = 1;
    const UNIQUE_CHOICES: bool = false;
    const RELIABLE_EVENTS: bool = true;
    fn fingerprint() -> u32 {
        1234
    }
    fn write_input(i: &u8, w: &mut Writer) {
        w.u8(*i);
    }
    fn read_input(r: &mut Reader) -> WireResult<u8> {
        r.u8()
    }
    fn write_snapshot(s: &u32, w: &mut Writer) {
        w.u32(*s);
        w.raw(&[0; 400]);
    }
    fn read_snapshot(r: &mut Reader) -> WireResult<u32> {
        let tick = r.u32()?;
        for _ in 0..400 {
            r.u8()?;
        }
        Ok(tick)
    }
    fn write_event(e: &u32, w: &mut Writer) {
        w.u32(*e);
        if LARGE && *e == 100 {
            w.raw(&[0; 1080]);
        } else {
            w.raw(&[0; 60]);
        }
    }
    fn read_event(r: &mut Reader) -> WireResult<u32> {
        let id = r.u32()?;
        for _ in 0..if LARGE && id == 100 { 1080 } else { 60 } {
            r.u8()?;
        }
        Ok(id)
    }
    fn start(_: u64, seats: &[Seat], _: usize) -> (u32, Vec<usize>) {
        (0, (0..seats.len()).collect())
    }
    fn participants(_: &u32) -> usize {
        CAPACITY
    }
    fn step(m: &mut u32, _: &[Option<u8>]) -> Vec<u32> {
        let first = *m * 8;
        *m += 1;
        (first..first + 8).collect()
    }
    fn release(_: &mut u32, _: usize) {}
    fn snapshot(m: &u32, _: Option<usize>) -> u32 {
        *m
    }
    fn is_over(m: &u32) -> bool {
        *m >= DURATION
    }
    fn report(m: &u32) -> serde_json::Value {
        serde_json::json!({"ticks": m})
    }
}

impl<const MIN: usize, const LARGE: bool, const DURATION: u32, const CAPACITY: usize>
    ClientView<CombatGame<MIN, LARGE, DURATION, CAPACITY>> for CombatView
{
    fn new() -> Self {
        Self(0)
    }
    fn on_snapshot(&mut self, s: &u32, _: Option<usize>, _: &[(u32, u8)], _: f64) {
        self.0 = *s;
    }
    fn on_input(&mut self, _: &u8) {}
    fn frame(&mut self, _: f64, _: f32) {}
    fn reset(&mut self) {
        self.0 = 0;
    }
    fn prediction(&self) -> PredictionStats {
        PredictionStats::default()
    }
}

#[test]
fn two_minutes_of_combat_delivers_promptly_to_four_lossy_clients() {
    type SustainedGame = CombatGame<4, false, 7200, 4>;
    let net = LoopNet::new(4, 3, 30., 81);
    let outgoing = Rc::new(RefCell::new(Vec::<QueuedDatagram>::new()));
    let endpoint = |address| OrderedEnd {
        inner: net.endpoint(address),
        outgoing: outgoing.clone(),
    };
    let addr = |n: u16| SocketAddr::from(([127, 0, 0, 1], 34_000 + n));
    let mut server = NetServer::<SustainedGame, _>::new(
        endpoint(addr(0)),
        ServerConfig {
            participants: 4,
            countdown_seconds: 0,
            auto_start_seconds: 0,
            results_seconds: 3,
            seed: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    let mut clients: Vec<_> = (1..=4)
        .map(|n| {
            NetClient::<SustainedGame, _>::new(
                endpoint(addr(n)),
                addr(0),
                ClientConfig {
                    name: format!("Sustained {n}"),
                    choice: 0,
                    key: String::new(),
                },
            )
            .unwrap()
        })
        .collect();
    let mut delivered = [0; 4];
    let mut event_age = [const { Vec::new() }; 4];
    let mut state_age = [const { Vec::new() }; 4];
    let mut started_at = None;
    let mut max_retained = 0;
    let mut max_bytes = 0;
    for tick in 0u32..7500 {
        // Session HashMaps use random iteration order. Sort streams before assigning seeded
        // network impairments, preserving order within each stream without changing the engine.
        let mut queued = std::mem::take(&mut *outgoing.borrow_mut());
        queued.sort_by_key(|d| (d.from, d.to));
        for d in queued {
            net.endpoint(d.from).send(d.to, &d.bytes).unwrap();
        }
        net.advance();
        server.poll(Instant::now());
        server.step(Instant::now());
        if server.stage() == vesper3d::viewer::netplay::Stage::Match {
            started_at.get_or_insert(tick);
        }
        let stats = server.event_stats();
        max_retained = max_retained.max(stats.retained);
        max_bytes = max_bytes.max(stats.retained_bytes);
        for (i, client) in clients.iter_mut().enumerate() {
            client.poll(tick as f64 / 60.);
            match client.state() {
                ClientState::Lobby if started_at.is_none() => client.ready(true),
                ClientState::Playing => client.tick(0),
                _ => {}
            }
            if let Some(start) = started_at {
                let elapsed = tick - start + 1;
                if elapsed <= 7200 && client.view().0 > 0 {
                    // Sample every playing tick, including ticks without a new snapshot.
                    state_age[i].push((tick + 1).saturating_sub(client.server_tick()));
                }
                for (original_tick, event) in client.drain_timed_events() {
                    assert_eq!(
                        event, delivered[i],
                        "client {i}: lost/duplicate/out-of-order event at tick {tick}, original={original_tick}, gaps={}, retained={stats:?}, state_tick={}",
                        client.event_gaps(), client.view().0
                    );
                    delivered[i] += 1;
                    event_age[i].push((tick + 1).saturating_sub(original_tick));
                }
            }
        }
    }
    fn ages(samples: &mut [u32]) -> (u32, u32, u32) {
        samples.sort_unstable();
        (
            samples[samples.len() / 2],
            samples[samples.len() * 99 / 100],
            *samples.last().unwrap(),
        )
    }
    for (i, client) in clients.iter().enumerate() {
        let event = ages(&mut event_age[i]);
        let state = ages(&mut state_age[i]);
        eprintln!("client={i} event_age_ticks(p50,p99,max)={event:?} state_age_ticks(p50,p99,max)={state:?} events={} gaps={}", delivered[i], client.event_gaps());
        assert_eq!(delivered[i], 7200 * 8);
        assert_eq!(client.event_gaps(), 0);
        // Ordered events must repair lost predecessors across several ACK round trips.
        // At 60 Hz allow p99 <= 0.75 s / max <= 1.5 s; independent state uses 0.5 s / 1 s.
        assert!(event.1 <= 45 && event.2 <= 90, "event age {event:?}");
        assert!(state.1 <= 30 && state.2 <= 60, "state age {state:?}");
        assert!(
            state_age[i].len() > 7000,
            "measure sustained play, not only results"
        );
    }
    eprintln!(
        "retained_events_max={max_retained} retained_bytes_max={max_bytes} send={:?}",
        server.send_stats()
    );
    assert!(max_retained <= 4096);
    assert!(max_bytes <= 1024 * 1024);
    assert_eq!(server.send_stats().oversized, 0);
}

#[test]
fn combat_event_bursts_keep_state_progressing_and_deliver_every_event_once() {
    combat_probe::<false>(0, 0, 0.);
}
#[test]
fn combat_events_survive_delay_jitter_and_thirty_percent_loss() {
    combat_probe::<false>(4, 3, 30.);
}
fn combat_probe<const LARGE: bool>(delay: u64, jitter: u64, loss: f32) {
    let net = LoopNet::new(delay, jitter, loss, 81);
    let address = |n: u16| SocketAddr::from(([127, 0, 0, 1], 31_000 + n));
    let mut server = NetServer::<CombatGame<1, LARGE>, _>::new(
        net.endpoint(address(0)),
        ServerConfig {
            participants: 2,
            countdown_seconds: 0,
            auto_start_seconds: 0,
            results_seconds: 30,
            seed: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    let mut client = NetClient::<CombatGame<1, LARGE>, _>::new(
        net.endpoint(address(1)),
        address(0),
        ClientConfig {
            name: "Combat probe".into(),
            key: String::new(),
            choice: 0,
        },
    )
    .unwrap();
    let mut events = Vec::new();
    for tick in 0..600 {
        net.advance();
        server.poll(Instant::now());
        server.step(Instant::now());
        client.poll(tick as f64 / 60.);
        match client.state() {
            ClientState::Lobby => client.ready(true),
            ClientState::Playing => client.tick(0),
            _ => {}
        }
        events.extend(client.drain_events());
    }
    eprintln!(
        "state_tick={} received_events={} transport={:?}",
        client.view().0,
        events.len(),
        server.send_stats()
    );
    assert_eq!(
        client.view().0,
        180,
        "combat events must not starve state snapshots"
    );
    assert_eq!(
        events,
        (0..180 * 8)
            .filter(|id| !LARGE || *id != 100)
            .collect::<Vec<_>>(),
        "no lost or duplicated combat events"
    );
    assert_eq!(server.send_stats().oversized, 0);
    assert_eq!(client.event_gaps(), u64::from(LARGE));
    assert_eq!(server.event_stats().oversized, u64::from(LARGE));
}

#[test]
fn a_two_player_room_waits_for_the_friend_and_refuses_a_third_human() {
    use vesper3d::viewer::netplay::Stage;
    let net = LoopNet::new(0, 0, 0., 16);
    let addr = |n: u16| SocketAddr::from(([127, 0, 0, 1], 32000 + n));
    let mut server = NetServer::<CombatGame<2>, _>::new(
        net.endpoint(addr(0)),
        ServerConfig {
            participants: 4,
            auto_start_seconds: 1,
            countdown_seconds: 1,
            seed: Some(8),
            ..Default::default()
        },
    )
    .unwrap();
    let client = |n: u16| {
        NetClient::<CombatGame<2>, _>::new(
            net.endpoint(addr(n)),
            addr(0),
            ClientConfig {
                name: format!("Friend {n}"),
                choice: 0,
                key: String::new(),
            },
        )
        .unwrap()
    };
    let mut first = client(1);
    for t in 0..240 {
        net.advance();
        server.poll(Instant::now());
        server.step(Instant::now());
        first.poll(t as f64 / 60.);
        first.ready(true);
    }
    assert_eq!(
        server.stage(),
        Stage::Lobby,
        "auto-start cannot close the room on a lone ready player"
    );
    assert_eq!(server.status_snapshot().participants, 2);
    let mut second = client(2);
    let mut third = client(3);
    for t in 240..280 {
        net.advance();
        first.poll(t as f64 / 60.);
        second.poll(t as f64 / 60.);
        server.poll(Instant::now());
        server.step(Instant::now());
    }
    for t in 280..300 {
        net.advance();
        third.poll(t as f64 / 60.);
        server.poll(Instant::now());
        server.step(Instant::now());
    }
    assert_eq!(server.players(), 2);
    assert!(matches!(third.state(), ClientState::Rejected(_)));
    second.leave();
    net.advance();
    server.poll(Instant::now());
    server.step(Instant::now());
    for t in 300..420 {
        net.advance();
        server.poll(Instant::now());
        server.step(Instant::now());
        first.poll(t as f64 / 60.);
    }
    assert_eq!(
        server.stage(),
        Stage::Lobby,
        "losing the friend cancels countdown"
    );
}

#[test]
fn an_oversized_event_is_reported_without_blocking_later_events_or_state() {
    combat_probe::<true>(4, 3, 30.);
}

#[test]
fn events_reset_cleanly_across_matches_despite_reordering_and_loss() {
    let net = LoopNet::new(4, 4, 25., 18);
    let addr = |n: u16| SocketAddr::from(([127, 0, 0, 1], 33000 + n));
    let mut server = NetServer::<CombatGame, _>::new(
        net.endpoint(addr(0)),
        ServerConfig {
            countdown_seconds: 0,
            results_seconds: 3,
            seed: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    let mut client = NetClient::<CombatGame, _>::new(
        net.endpoint(addr(1)),
        addr(0),
        ClientConfig {
            name: "Repeat".into(),
            choice: 0,
            key: String::new(),
        },
    )
    .unwrap();
    let mut matches = Vec::<Vec<u32>>::new();
    for tick in 0..1600 {
        net.advance();
        server.poll(Instant::now());
        server.step(Instant::now());
        client.poll(tick as f64 / 60.);
        match client.state() {
            ClientState::Lobby => client.ready(true),
            ClientState::Playing => client.tick(0),
            _ => {}
        }
        for e in client.drain_events() {
            if e == 0 {
                matches.push(Vec::new());
            }
            matches
                .last_mut()
                .expect("first event starts a match")
                .push(e);
        }
    }
    assert!(
        matches.len() >= 3,
        "completed event streams: {:?}",
        matches.iter().map(Vec::len).collect::<Vec<_>>()
    );
    for events in matches.iter().take(3) {
        assert_eq!(
            events.len(),
            1440,
            "completed event streams: {:?}; last {:?}",
            matches.iter().map(Vec::len).collect::<Vec<_>>(),
            events.last()
        );
        assert!(events.iter().copied().eq(0..1440));
    }
    assert_eq!(client.event_gaps(), 0);
}
