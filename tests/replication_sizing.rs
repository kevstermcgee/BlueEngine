//! Sizing a delta candidate by arithmetic must produce exactly the packets that serializing the whole
//! delta for every candidate produced (the original method), for any world, budget and loss pattern.
use std::{cell::RefCell, net::SocketAddr};
use vesper3d::viewer::{
    controller::Movement,
    net::{replication::ReplicationSender, Datagram, DatagramTransport},
    simulation::HeadlessWorld,
};

/// Keeps every datagram it is asked to send, and reports a chosen payload budget.
struct Recorder {
    limit: usize,
    sent: RefCell<Vec<Vec<u8>>>,
}
impl DatagramTransport for Recorder {
    fn send(&self, _peer: SocketAddr, data: &[u8]) -> vesper3d::Result<usize> {
        self.sent.borrow_mut().push(data.to_vec());
        Ok(data.len())
    }
    fn payload_limit(&self, _peer: SocketAddr) -> usize {
        self.limit
    }
    fn receive(&mut self) -> vesper3d::Result<Vec<Datagram>> {
        Ok(Vec::new())
    }
    fn local_addr(&self) -> vesper3d::Result<SocketAddr> {
        Ok("127.0.0.1:1".parse().unwrap())
    }
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

fn run(players: u64, limit: usize, seed: u64, drop_acks_percent: u64) -> usize {
    let mut world = HeadlessWorld::new().unwrap();
    world.set_max_players(players as usize);
    for id in 1..=players {
        world.join(id);
    }
    let peer: SocketAddr = "10.0.0.1:1".parse().unwrap();
    let (fast_net, exact_net) = (
        Recorder {
            limit,
            sent: RefCell::default(),
        },
        Recorder {
            limit,
            sent: RefCell::default(),
        },
    );
    let (mut fast, mut exact) = (
        ReplicationSender::default(),
        ReplicationSender::default().with_exact_sizing(),
    );
    let mut rng = Lcg(seed);
    let owner = 1 + seed % players;
    let mut compared = 0;
    for round in 0..80 {
        for id in 1..=players {
            let (a, b) = (rng.next() % 2000, rng.next() % 2000);
            let mv = Movement {
                forward: 1.0,
                right: (a as f32 / 1000.0) - 1.0,
                ..Movement::default()
            };
            world.input(id, mv, (b as f32) / 300.0, 0.0);
        }
        for _ in 0..3 {
            world.step();
        }
        let ack = round as u64;
        let snap = world.snapshot_for_player(owner, ack);
        let (rf, re) = (
            fast.send(&fast_net, peer, &snap, owner),
            exact.send(&exact_net, peer, &snap, owner),
        );
        assert_eq!(
            rf.is_ok(),
            re.is_ok(),
            "seed {seed} round {round}: {rf:?} vs {re:?}"
        );
        if rf.is_err() {
            return compared; // a budget too small for one record is refused the same way by both
        }
        let (a, b) = (
            fast_net.sent.borrow().last().cloned(),
            exact_net.sent.borrow().last().cloned(),
        );
        assert_eq!(
            a, b,
            "seed {seed} limit {limit} round {round}: packets differ"
        );
        compared += 1;
        if rng.next() % 100 >= drop_acks_percent {
            let target = fast.pending_target();
            assert_eq!(target, exact.pending_target());
            if let Some(t) = target {
                assert_eq!(fast.acknowledge(t), exact.acknowledge(t));
            }
        } else if rng.next().is_multiple_of(4) {
            fast.resynchronize();
            exact.resynchronize();
        }
    }
    compared
}

#[test]
fn arithmetic_sizing_emits_the_same_packets_as_serializing_every_candidate() {
    let mut rounds = 0;
    for (players, limit) in [
        (1, 1100),
        (2, 1100),
        (6, 400),
        (8, 1100),
        (8, 250),
        (40, 1100),
        (40, 600),
        (120, 1100),
        (120, 900),
    ] {
        for seed in 0..3 {
            for loss in [0, 30] {
                rounds += run(players, limit, seed * 31 + players, loss);
            }
        }
    }
    assert!(
        rounds > 1000,
        "the comparison should cover thousands of packets, covered {rounds}"
    );
}
