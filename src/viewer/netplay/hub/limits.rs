//! The hub's abuse limits: token buckets per source, for creating, and over everyone; caps on rooms.
//!
//! A hub on a home connection answers anyone on the internet, so every limit here is about what it costs the box:
//! a source that floods gets its burst and then silence (silence costs the hub nothing and gives a flooder
//! nothing), creating a room (which starts a process) has its own slower bucket, and one global bucket bounds
//! the hub's total output when sources are spoofed. Time is passed in as milliseconds, so tests are deterministic.
use std::collections::HashMap;
use std::net::IpAddr;

#[derive(Clone, Copy, Debug)]
pub struct Bucket {
    tokens: f64,
    last_ms: u64,
}

impl Bucket {
    /// A bucket that starts full (`tokens` is its capacity).
    pub fn new(tokens: f64) -> Self {
        Self { tokens, last_ms: 0 }
    }

    fn refill(&mut self, now_ms: u64, capacity: f64, per_sec: f64) {
        let dt = now_ms.saturating_sub(self.last_ms) as f64 / 1000.;
        self.tokens = (self.tokens + dt * per_sec).min(capacity);
        self.last_ms = self.last_ms.max(now_ms);
    }

    /// Spend `cost` tokens at `now_ms`; false if there are not enough. A clock that goes backwards gives nothing back.
    pub fn take(&mut self, now_ms: u64, capacity: f64, per_sec: f64, cost: f64) -> bool {
        self.refill(now_ms, capacity, per_sec);
        if self.tokens >= cost {
            self.tokens -= cost;
            true
        } else {
            false
        }
    }
}

/// A token bucket per source IP address (a source that hops ports still shares one bucket), with a hard cap on how
/// many sources are remembered.
#[derive(Clone, Debug)]
pub struct RateLimiter {
    /// Burst size, in requests.
    pub capacity: f64,
    /// Sustained rate, requests per second.
    pub per_sec: f64,
    /// At most this many sources are tracked; a new source beyond it is refused until idle ones age out.
    pub max_sources: usize,
    buckets: HashMap<IpAddr, Bucket>,
    last_sweep_ms: u64,
}

impl RateLimiter {
    pub fn new(capacity: f64, per_sec: f64, max_sources: usize) -> Self {
        Self {
            capacity,
            per_sec,
            max_sources,
            buckets: HashMap::new(),
            last_sweep_ms: 0,
        }
    }

    /// Spend one token for `ip` at `now_ms`; false if the source is over its rate (or the table is full of busy sources).
    pub fn allow(&mut self, ip: IpAddr, now_ms: u64) -> bool {
        let (capacity, per_sec) = (self.capacity, self.per_sec);
        if let Some(b) = self.buckets.get_mut(&ip) {
            return b.take(now_ms, capacity, per_sec, 1.);
        }
        if self.buckets.len() >= self.max_sources {
            self.sweep(now_ms);
            if self.buckets.len() >= self.max_sources {
                return false;
            }
        }
        let mut b = Bucket {
            tokens: capacity,
            last_ms: now_ms,
        };
        let ok = b.take(now_ms, capacity, per_sec, 1.);
        self.buckets.insert(ip, b);
        ok
    }

    /// Forget sources whose bucket has refilled completely (they look exactly like a source never seen). At most once a second.
    fn sweep(&mut self, now_ms: u64) {
        if now_ms.saturating_sub(self.last_sweep_ms) < 1000 && self.last_sweep_ms != 0 {
            return;
        }
        self.last_sweep_ms = now_ms.max(1);
        let (capacity, per_sec) = (self.capacity, self.per_sec);
        self.buckets.retain(|_, b| {
            b.refill(now_ms, capacity, per_sec);
            b.tokens < capacity
        });
    }

    pub fn tracked(&self) -> usize {
        self.buckets.len()
    }
}

/// The hub's abuse limits.
#[derive(Clone, Debug)]
pub struct Limits {
    /// Per source address: burst and sustained requests per second.
    pub burst: f64,
    pub per_sec: f64,
    /// Creating a room is much costlier than listing: its own, slower bucket per source.
    pub create_burst: f64,
    pub create_per_sec: f64,
    /// Over all sources together (spoofed sources defeat per-source limits, this bounds the hub's total output).
    pub global_burst: f64,
    pub global_per_sec: f64,
    pub max_sources: usize,
    /// Creates over the legacy `DFHB` protocol (which has no cookie, so the source could be forged) share this one
    /// bucket across every source.
    pub legacy_create_burst: f64,
    pub legacy_create_per_sec: f64,
    /// Rooms one creator IP may have open at once (all games together).
    pub max_rooms_per_ip: usize,
    /// Room server processes the hub runs at once, all games together, the permanent Public rooms included.
    pub max_processes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            burst: 10.,
            per_sec: 2.,
            create_burst: 3.,
            create_per_sec: 1. / 30.,
            global_burst: 300.,
            global_per_sec: 150.,
            max_sources: 4096,
            legacy_create_burst: 3.,
            legacy_create_per_sec: 1. / 20.,
            max_rooms_per_ip: 2,
            max_processes: 16,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rate_limiter_refills_with_time_and_never_trusts_a_clock_that_goes_backwards() {
        let mut l = RateLimiter::new(3., 1., 100);
        let ip: IpAddr = [1, 2, 3, 4].into();
        assert!([0, 0, 0].iter().all(|t| l.allow(ip, *t)));
        assert!(!l.allow(ip, 0), "burst of three spent");
        assert!(!l.allow(ip, 500), "half a token is not enough");
        assert!(l.allow(ip, 1000), "one token per second");
        assert!(!l.allow(ip, 1000));
        assert!(!l.allow(ip, 10), "time going backwards gives nothing back");
        assert!(
            l.allow([1, 2, 3, 5].into(), 0),
            "another source has its own bucket"
        );
        assert!(
            l.allow(ip, 100_000) && l.allow(ip, 100_001) && l.allow(ip, 100_002),
            "long idle refills to the burst"
        );
        assert!(!l.allow(ip, 100_003), "and no further");
    }

    #[test]
    fn the_source_table_is_capped_and_ages_out_idle_sources() {
        let mut l = RateLimiter::new(2., 1., 4);
        let ip = |n: u8| IpAddr::from([9, 9, 9, n]);
        for n in 0..4 {
            assert!(l.allow(ip(n), 0));
        }
        assert_eq!(l.tracked(), 4);
        assert!(
            !l.allow(ip(99), 100),
            "full of recent sources: a fifth is refused"
        );
        assert_eq!(l.tracked(), 4, "memory does not grow");
        assert!(
            l.allow(ip(99), 5_000),
            "after the others refill they are forgotten and the newcomer fits"
        );
        assert!(l.tracked() <= 4);
    }

    #[test]
    fn the_defaults_are_the_deadfall_hubs_numbers_plus_the_new_caps() {
        let l = Limits::default();
        assert_eq!((l.burst, l.per_sec), (10., 2.));
        assert_eq!((l.create_burst, l.create_per_sec), (3., 1. / 30.));
        assert_eq!(
            (l.global_burst, l.global_per_sec, l.max_sources),
            (300., 150., 4096)
        );
        assert_eq!(l.max_rooms_per_ip, 2);
        assert!(l.legacy_create_per_sec < l.create_per_sec * 2. && l.legacy_create_burst >= 1.);
    }

    #[test]
    fn a_bucket_takes_costs() {
        let mut b = Bucket::new(2.);
        assert!(b.take(0, 2., 1., 2.));
        assert!(!b.take(0, 2., 1., 1.));
        assert!(b.take(1000, 2., 1., 1.));
    }
}
