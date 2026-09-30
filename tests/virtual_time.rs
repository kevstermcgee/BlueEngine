//! Timeouts, the reconnect reservation and the handshake rate window, tested by moving a clock, not by sleeping.
use std::{net::SocketAddr, sync::Mutex, time::Duration};
use vesper3d::viewer::{
    net::{Clock, Datagram, DatagramTransport, InputFrame, Packet, PROTOCOL_VERSION},
    server::DedicatedServer,
    simulation::HeadlessWorld,
};

/// Delivers whatever a test queues, and remembers what the server sent.
#[derive(Default)]
struct Wire {
    inbox: Mutex<Vec<Datagram>>,
    sent: Mutex<Vec<(SocketAddr, Packet)>>,
}
impl DatagramTransport for Wire {
    fn send(&self, peer: SocketAddr, data: &[u8]) -> vesper3d::Result<usize> {
        if let Ok(packet) = Packet::decode(data) {
            self.sent.lock().unwrap().push((peer, packet));
        }
        Ok(data.len())
    }
    fn receive(&mut self) -> vesper3d::Result<Vec<Datagram>> {
        Ok(std::mem::take(&mut *self.inbox.lock().unwrap()))
    }
    fn local_addr(&self) -> vesper3d::Result<SocketAddr> {
        Ok("127.0.0.1:1".parse().unwrap())
    }
}

fn addr(i: u16) -> SocketAddr {
    format!("10.0.0.{}:{}", i % 200 + 1, 4000 + i)
        .parse()
        .unwrap()
}
fn server() -> (DedicatedServer<Wire>, Clock) {
    let clock = Clock::manual();
    let server = DedicatedServer::with_transport(Wire::default(), HeadlessWorld::new().unwrap())
        .unwrap()
        .with_clock(clock.clone());
    (server, clock)
}
fn hello(server: &mut DedicatedServer<Wire>, from: SocketAddr, reserved_id: u64) {
    let hash = server.world.content_hash;
    server.handle_hello(from, PROTOCOL_VERSION, reserved_id, hash);
}
fn welcomed_id(server: &DedicatedServer<Wire>, from: SocketAddr) -> Option<u64> {
    server
        .transport
        .sent
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find_map(|(to, p)| match p {
            Packet::Welcome { player_id, .. } if *to == from => Some(*player_id),
            _ => None,
        })
}
fn send_input(server: &mut DedicatedServer<Wire>, from: SocketAddr, tick: u64) {
    let frame = InputFrame {
        client_tick: tick,
        ..InputFrame::default()
    };
    server.transport.inbox.lock().unwrap().push(Datagram {
        peer: from,
        data: Packet::Input(frame).encode().unwrap(),
    });
    server.poll_network().unwrap();
}
const SECOND: Duration = Duration::from_secs(1);

#[test]
fn a_silent_client_times_out_at_the_configured_moment_and_not_before() {
    let (mut server, clock) = server();
    hello(&mut server, addr(1), 0);
    assert_eq!(server.sessions.len(), 1);
    assert_eq!(server.client_timeout, Duration::from_secs(5));
    clock.advance(Duration::from_millis(4900));
    assert!(
        server.check_timeouts().is_empty(),
        "4.9 s of silence is still within 5 s"
    );
    clock.advance(Duration::from_millis(200));
    assert_eq!(server.check_timeouts(), [1]);
    assert_eq!(server.sessions.len(), 0);
    assert_eq!(server.session_registry.count(), 0);
}

#[test]
fn input_keeps_a_session_alive_and_silence_after_it_is_timed_from_the_last_input() {
    let (mut server, clock) = server();
    hello(&mut server, addr(1), 0);
    clock.advance(3 * SECOND);
    send_input(&mut server, addr(1), 1);
    clock.advance(3 * SECOND); // 6 s since the hello, 3 s since the input
    assert!(server.check_timeouts().is_empty());
    clock.advance(3 * SECOND); // 6 s since the input
    assert_eq!(server.check_timeouts(), [1]);
}

#[test]
fn a_stale_input_frame_does_not_count_as_activity() {
    let (mut server, clock) = server();
    hello(&mut server, addr(1), 0);
    send_input(&mut server, addr(1), 5);
    clock.advance(4 * SECOND);
    send_input(&mut server, addr(1), 5); // same tick again: a duplicate, ignored
    clock.advance(2 * SECOND);
    assert_eq!(
        server.check_timeouts(),
        [1],
        "the duplicate did not refresh the session"
    );
}

#[test]
fn a_returning_client_keeps_its_player_inside_the_reservation_and_loses_it_outside() {
    let (mut server, clock) = server();
    hello(&mut server, addr(1), 0);
    assert_eq!(welcomed_id(&server, addr(1)), Some(1));
    clock.advance(6 * SECOND);
    assert_eq!(server.check_timeouts(), [1]);

    // 59 s after it dropped, the reservation still holds: asking for player 1 again gets player 1.
    clock.advance(59 * SECOND);
    hello(&mut server, addr(1), 1);
    assert_eq!(
        welcomed_id(&server, addr(1)),
        Some(1),
        "reconnected as the same player"
    );

    // It drops again; this time it is gone for 61 s, so the reservation has expired.
    clock.advance(6 * SECOND);
    assert_eq!(server.check_timeouts(), [1]);
    clock.advance(61 * SECOND);
    hello(&mut server, addr(1), 1);
    let id = welcomed_id(&server, addr(1)).unwrap();
    assert_ne!(
        id, 1,
        "after the window a returning client is a new player ({id})"
    );
}

#[test]
fn another_address_cannot_claim_a_reserved_player() {
    let (mut server, clock) = server();
    hello(&mut server, addr(1), 0);
    clock.advance(6 * SECOND);
    server.check_timeouts();
    hello(&mut server, addr(2), 1); // a different address asks for player 1
    assert_ne!(welcomed_id(&server, addr(2)), Some(1));
}

#[test]
fn the_handshake_rate_limit_follows_the_clock() {
    let (mut server, clock) = server();
    let bad_hash = |server: &mut DedicatedServer<Wire>, n: u16| {
        let before = server.transport.sent.lock().unwrap().len();
        server.handle_hello(addr(n), PROTOCOL_VERSION, 0, 0xBAD);
        server.transport.sent.lock().unwrap().len() > before // a refusal is answered; a rate-limited hello is not
    };
    let answered = (0..64).filter(|n| bad_hash(&mut server, *n)).count();
    assert_eq!(answered, 64, "64 handshakes in one second are answered");
    assert!(
        !bad_hash(&mut server, 64),
        "the 65th within the same second is dropped without a reply"
    );
    clock.advance(SECOND);
    assert!(
        bad_hash(&mut server, 65),
        "a second later the window has reopened"
    );
}

#[test]
fn the_default_clock_is_the_real_one() {
    let server =
        DedicatedServer::with_transport(Wire::default(), HeadlessWorld::new().unwrap()).unwrap();
    assert!(!server.clock().is_manual());
    let a = server.clock().now();
    std::thread::sleep(Duration::from_millis(2));
    assert!(server.clock().now() > a);
}

#[test]
fn a_manual_clock_is_shared_by_its_clones_and_a_real_clock_ignores_advance() {
    let clock = Clock::manual();
    let copy = clock.clone();
    let start = clock.now();
    copy.advance(10 * SECOND);
    assert_eq!(clock.now() - start, 10 * SECOND);
    let real = Clock::real();
    let before = real.now();
    real.advance(3600 * SECOND);
    assert!(
        real.now() - before < SECOND,
        "advance does nothing to a real clock"
    );
}
