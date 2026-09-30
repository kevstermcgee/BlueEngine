//! An in-memory datagram network with configurable delay, jitter and loss, for tests.
//!
//! [`LoopNet`] hands out [`LoopEnd`] endpoints that implement [`DatagramTransport`]. Virtual time moves only
//! when the test calls [`LoopNet::advance`] (one call per 60 Hz tick), so a whole server and eight clients
//! can race for two minutes of game time in well under a second of wall time, deterministically. The engine's
//! [`NetworkSimulator`](super::NetworkSimulator) only handles its own `Packet` type; this works on raw
//! datagrams, so any protocol can be tested under bad network conditions.
//!
//! ```
//! use vesper3d::viewer::net::{loopback::LoopNet, DatagramTransport};
//! let net = LoopNet::new(2, 0, 0., 1); // two ticks of delay, no loss
//! let (a, mut b) = (net.endpoint("10.0.0.1:1".parse().unwrap()), net.endpoint("10.0.0.2:2".parse().unwrap()));
//! a.send("10.0.0.2:2".parse().unwrap(), b"hi").unwrap();
//! assert!(b.receive().unwrap().is_empty());
//! net.advance();
//! net.advance();
//! assert_eq!(b.receive().unwrap()[0].data, b"hi");
//! ```
use super::transport::{Datagram, DatagramTransport, PeerId};
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

struct Pending {
    deliver_at: u64,
    from: SocketAddr,
    to: SocketAddr,
    data: Vec<u8>,
}

struct NetState {
    now: u64,
    latency: u64,
    jitter: u64,
    /// Chance of dropping a datagram, in tenths of a percent.
    loss: u64,
    rng: u64,
    in_flight: VecDeque<Pending>,
    inboxes: HashMap<SocketAddr, VecDeque<Datagram>>,
    sent: u64,
    dropped: u64,
}

/// The shared network; clone it freely.
#[derive(Clone)]
pub struct LoopNet(Arc<Mutex<NetState>>);

impl LoopNet {
    /// `latency` and `jitter` are in ticks (1/60 s); `loss_percent` is the share of datagrams dropped.
    pub fn new(latency: u64, jitter: u64, loss_percent: f32, seed: u64) -> Self {
        Self(Arc::new(Mutex::new(NetState {
            now: 0,
            latency,
            jitter,
            loss: (loss_percent * 10.) as u64,
            rng: seed | 1,
            in_flight: VecDeque::new(),
            inboxes: HashMap::new(),
            sent: 0,
            dropped: 0,
        })))
    }

    /// A new endpoint with the given address.
    pub fn endpoint(&self, address: SocketAddr) -> LoopEnd {
        self.0.lock().unwrap().inboxes.entry(address).or_default();
        LoopEnd {
            net: self.clone(),
            address,
        }
    }

    /// Move time forward one tick and deliver every datagram that is now due.
    pub fn advance(&self) {
        let mut n = self.0.lock().unwrap();
        n.now += 1;
        let now = n.now;
        let mut later = VecDeque::new();
        while let Some(p) = n.in_flight.pop_front() {
            if p.deliver_at <= now {
                n.inboxes.entry(p.to).or_default().push_back(Datagram {
                    peer: p.from,
                    data: p.data,
                });
            } else {
                later.push_back(p);
            }
        }
        n.in_flight = later;
    }

    /// `(sent, dropped)` datagram counts so far.
    pub fn counts(&self) -> (u64, u64) {
        let n = self.0.lock().unwrap();
        (n.sent, n.dropped)
    }
}

/// One endpoint on a [`LoopNet`].
pub struct LoopEnd {
    net: LoopNet,
    address: SocketAddr,
}

impl DatagramTransport for LoopEnd {
    fn send(&self, peer: PeerId, data: &[u8]) -> crate::Result<usize> {
        let mut n = self.net.0.lock().unwrap();
        n.sent += 1;
        n.rng ^= n.rng << 13;
        n.rng ^= n.rng >> 7;
        n.rng ^= n.rng << 17;
        if n.rng % 1000 < n.loss {
            n.dropped += 1;
            return Ok(data.len());
        }
        let jitter = if n.jitter > 0 {
            (n.rng >> 20) % (n.jitter + 1)
        } else {
            0
        };
        let deliver_at = n.now + n.latency + jitter;
        let from = self.address;
        n.in_flight.push_back(Pending {
            deliver_at,
            from,
            to: peer,
            data: data.to_vec(),
        });
        Ok(data.len())
    }
    fn payload_limit(&self, _: PeerId) -> usize {
        1200
    }
    fn receive(&mut self) -> crate::Result<Vec<Datagram>> {
        let mut n = self.net.0.lock().unwrap();
        Ok(n.inboxes
            .entry(self.address)
            .or_default()
            .drain(..)
            .collect())
    }
    fn local_addr(&self) -> crate::Result<SocketAddr> {
        Ok(self.address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(n: u16) -> SocketAddr {
        format!("10.0.0.{n}:{}", 4000 + n).parse().unwrap()
    }

    #[test]
    fn datagrams_arrive_after_their_delay_and_not_before() {
        let net = LoopNet::new(3, 0, 0., 7);
        let a = net.endpoint(addr(1));
        let mut b = net.endpoint(addr(2));
        a.send(addr(2), b"x").unwrap();
        for _ in 0..2 {
            net.advance();
            assert!(b.receive().unwrap().is_empty());
        }
        net.advance();
        let got = b.receive().unwrap();
        assert_eq!((got.len(), got[0].peer), (1, addr(1)));
    }

    #[test]
    fn loss_drops_about_the_requested_share_deterministically() {
        let run = || {
            let net = LoopNet::new(0, 0, 25., 99);
            let a = net.endpoint(addr(1));
            let _b = net.endpoint(addr(2));
            for _ in 0..4000 {
                a.send(addr(2), b"x").unwrap();
            }
            net.counts()
        };
        let (sent, dropped) = run();
        assert_eq!(sent, 4000);
        assert!((800..1200).contains(&dropped), "about 25%: {dropped}");
        assert_eq!(
            run(),
            (sent, dropped),
            "the same seed drops the same datagrams"
        );
    }

    #[test]
    fn jitter_reorders_but_never_loses() {
        let net = LoopNet::new(1, 5, 0., 3);
        let a = net.endpoint(addr(1));
        let mut b = net.endpoint(addr(2));
        for i in 0..50u8 {
            a.send(addr(2), &[i]).unwrap();
        }
        let mut seen = Vec::new();
        for _ in 0..10 {
            net.advance();
            seen.extend(b.receive().unwrap().into_iter().map(|d| d.data[0]));
        }
        let mut sorted = seen.clone();
        sorted.sort();
        assert_eq!(sorted, (0..50).collect::<Vec<u8>>());
    }
}
