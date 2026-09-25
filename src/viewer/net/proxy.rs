//! High-fidelity network impairment proxy and bad network simulation.
//!
//! Features:
//! - 2-State Markov Gilbert-Elliott burst packet loss model
//! - Gaussian latency distribution with Box-Muller jitter
//! - Configurable packet duplication and out-of-order delivery
//! - Bidirectional non-blocking UDP proxy server with live telemetry
//! - Standard presets: bad-wifi, mobile-3g, satellite, congested-bursty

use std::{
    collections::VecDeque,
    f64::consts::PI,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

/// Fast, deterministic, zero-dependency PRNG (XorShift64*).
#[derive(Clone, Debug)]
pub struct FastRng {
    state: u64,
}

impl FastRng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x853c49e6748fea9b } else { seed },
        }
    }

    pub fn from_os() -> Self {
        let mut bytes = [0u8; 8];
        if getrandom::fill(&mut bytes).is_ok() {
            Self::new(u64::from_le_bytes(bytes))
        } else {
            Self::new(1337)
        }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545f4914f6cdd1d)
    }

    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// 2-State Markov chain model for burst packet loss (Gilbert-Elliott).
///
/// In the "Good" state, packets experience low/normal background loss (`loss_good`).
/// In the "Bad" (burst) state, packets experience high burst loss (`loss_bad`).
/// State transitions occur on packet boundaries:
/// - Good -> Bad with probability `p_good_to_bad`
/// - Bad -> Good with probability `p_bad_to_good`
///
/// Average burst length is `1 / p_bad_to_good` packets.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GilbertElliottModel {
    pub p_good_to_bad: f64,
    pub p_bad_to_good: f64,
    pub loss_good: f64,
    pub loss_bad: f64,
    #[serde(skip)]
    pub in_bad_state: bool,
}

impl GilbertElliottModel {
    pub const fn new(
        p_good_to_bad: f64,
        p_bad_to_good: f64,
        loss_good: f64,
        loss_bad: f64,
    ) -> Self {
        Self {
            p_good_to_bad,
            p_bad_to_good,
            loss_good,
            loss_bad,
            in_bad_state: false,
        }
    }

    /// Advance the Markov state and determine if the current packet should be dropped.
    pub fn step(&mut self, rng: &mut FastRng) -> bool {
        // State transition
        if self.in_bad_state {
            if rng.next_f64() < self.p_bad_to_good {
                self.in_bad_state = false;
            }
        } else if rng.next_f64() < self.p_good_to_bad {
            self.in_bad_state = true;
        }

        // Loss determination based on current state
        let loss_prob = if self.in_bad_state {
            self.loss_bad
        } else {
            self.loss_good
        };
        rng.next_f64() < loss_prob
    }

    /// Expected steady-state loss rate.
    pub fn steady_state_loss(&self) -> f64 {
        let total = self.p_good_to_bad + self.p_bad_to_good;
        if total <= 0.0 {
            return 0.0;
        }
        let p_bad = self.p_good_to_bad / total;
        let p_good = self.p_bad_to_good / total;
        (p_good * self.loss_good) + (p_bad * self.loss_bad)
    }

    /// Average burst length in packets when in bad state.
    pub fn average_burst_length(&self) -> f64 {
        if self.p_bad_to_good > 0.0 {
            1.0 / self.p_bad_to_good
        } else {
            1.0
        }
    }
}

/// Gaussian latency model with Box-Muller transform for realistic network jitter.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GaussianJitter {
    pub base_latency_ms: f64,
    pub jitter_std_dev_ms: f64,
}

impl GaussianJitter {
    pub const fn new(base_latency_ms: f64, jitter_std_dev_ms: f64) -> Self {
        Self {
            base_latency_ms,
            jitter_std_dev_ms,
        }
    }

    /// Sample a random latency in milliseconds using Box-Muller normal distribution.
    pub fn sample_ms(&self, rng: &mut FastRng) -> f64 {
        if self.jitter_std_dev_ms <= 0.0 {
            return self.base_latency_ms.max(0.0);
        }

        let u1 = rng.next_f64().max(1e-12);
        let u2 = rng.next_f64();
        let z = (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos();
        let sample = self.base_latency_ms + self.jitter_std_dev_ms * z;
        sample.max(0.0)
    }

    pub fn sample_duration(&self, rng: &mut FastRng) -> Duration {
        Duration::from_secs_f64(self.sample_ms(rng) / 1000.0)
    }
}

/// Comprehensive network impairment profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NetworkProxyConfig {
    pub name: String,
    pub burst_loss: GilbertElliottModel,
    pub jitter: GaussianJitter,
    pub duplication_rate: f64,
    pub duplication_offset_ms: f64,
}

impl NetworkProxyConfig {
    /// Bad Wi-Fi preset: moderate latency, high jitter, burst loss when microwave or interference fires.
    pub fn bad_wifi() -> Self {
        Self {
            name: "bad-wifi".into(),
            burst_loss: GilbertElliottModel::new(0.03, 0.25, 0.01, 0.70),
            jitter: GaussianJitter::new(30.0, 20.0),
            duplication_rate: 0.015,
            duplication_offset_ms: 10.0,
        }
    }

    /// Mobile 3G/LTE preset: higher latency, periodic cellular handoff bursts.
    pub fn mobile_3g() -> Self {
        Self {
            name: "mobile-3g".into(),
            burst_loss: GilbertElliottModel::new(0.05, 0.15, 0.02, 0.85),
            jitter: GaussianJitter::new(110.0, 45.0),
            duplication_rate: 0.02,
            duplication_offset_ms: 15.0,
        }
    }

    /// Satellite connection preset: heavy roundtrip latency, atmospheric bursts.
    pub fn satellite() -> Self {
        Self {
            name: "satellite".into(),
            burst_loss: GilbertElliottModel::new(0.02, 0.20, 0.01, 0.80),
            jitter: GaussianJitter::new(550.0, 50.0),
            duplication_rate: 0.03,
            duplication_offset_ms: 25.0,
        }
    }

    /// Heavily congested router buffer preset: bufferbloat bursts and frequent drops.
    pub fn congested_bursty() -> Self {
        Self {
            name: "congested-bursty".into(),
            burst_loss: GilbertElliottModel::new(0.08, 0.18, 0.03, 0.90),
            jitter: GaussianJitter::new(65.0, 35.0),
            duplication_rate: 0.04,
            duplication_offset_ms: 12.0,
        }
    }

    /// Clean, deterministic delay profile for testing.
    pub fn clean_delay(delay_ms: f64) -> Self {
        Self {
            name: "clean-delay".into(),
            burst_loss: GilbertElliottModel::new(0.0, 1.0, 0.0, 0.0),
            jitter: GaussianJitter::new(delay_ms, 0.0),
            duplication_rate: 0.0,
            duplication_offset_ms: 0.0,
        }
    }

    /// Resolve preset name or parse configuration.
    pub fn from_preset(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "bad-wifi" | "wifi" => Some(Self::bad_wifi()),
            "mobile-3g" | "mobile" | "3g" => Some(Self::mobile_3g()),
            "satellite" => Some(Self::satellite()),
            "congested-bursty" | "congested" | "bursty" => Some(Self::congested_bursty()),
            _ => None,
        }
    }
}

impl Default for NetworkProxyConfig {
    fn default() -> Self {
        Self::bad_wifi()
    }
}

/// Telemetry metrics tracked during proxy operation.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProxyStats {
    pub packets_received: u64,
    pub packets_forwarded: u64,
    pub packets_dropped: u64,
    pub packets_duplicated: u64,
    pub bytes_forwarded: u64,
    pub burst_episodes: u64,
    pub total_simulated_latency_ms: f64,
}

impl ProxyStats {
    pub fn drop_rate(&self) -> f64 {
        if self.packets_received == 0 {
            0.0
        } else {
            self.packets_dropped as f64 / self.packets_received as f64
        }
    }

    pub fn average_latency_ms(&self) -> f64 {
        if self.packets_forwarded == 0 {
            0.0
        } else {
            self.total_simulated_latency_ms / self.packets_forwarded as f64
        }
    }
}

#[derive(Clone, Debug)]
struct DelayedPacket {
    deliver_at: Instant,
    payload: Vec<u8>,
    target: SocketAddr,
    latency_ms: f64,
}

/// Simulated impairment pipeline operating on arbitrary packet payloads.
pub struct ImpairmentPipeline {
    pub config: NetworkProxyConfig,
    pub rng: FastRng,
    pub stats: ProxyStats,
    queue: VecDeque<DelayedPacket>,
    was_in_burst: bool,
}

impl ImpairmentPipeline {
    pub fn new(config: NetworkProxyConfig, seed: u64) -> Self {
        Self {
            config,
            rng: FastRng::new(seed),
            stats: ProxyStats::default(),
            queue: VecDeque::new(),
            was_in_burst: false,
        }
    }

    /// Submit a packet to the impairment pipeline.
    ///
    /// Evaluates burst drop, Gaussian delay, and packet duplication.
    /// Returns true if the packet was enqueued (not dropped).
    pub fn process_packet(&mut self, payload: Vec<u8>, target: SocketAddr, now: Instant) -> bool {
        self.stats.packets_received += 1;

        // 1. Gilbert-Elliott Burst Loss Check
        let dropped = self.config.burst_loss.step(&mut self.rng);
        if self.config.burst_loss.in_bad_state && !self.was_in_burst {
            self.stats.burst_episodes += 1;
        }
        self.was_in_burst = self.config.burst_loss.in_bad_state;

        if dropped {
            self.stats.packets_dropped += 1;
            return false;
        }

        // 2. Gaussian Latency / Jitter Calculation
        let latency_ms = self.config.jitter.sample_ms(&mut self.rng);
        let deliver_at = now + Duration::from_secs_f64(latency_ms / 1000.0);

        self.queue.push_back(DelayedPacket {
            deliver_at,
            payload: payload.clone(),
            target,
            latency_ms,
        });

        // 3. Packet Duplication Check
        if self.config.duplication_rate > 0.0 && self.rng.next_f64() < self.config.duplication_rate
        {
            self.stats.packets_duplicated += 1;
            let dup_delay = latency_ms + self.config.duplication_offset_ms;
            let dup_deliver_at = now + Duration::from_secs_f64(dup_delay / 1000.0);
            self.queue.push_back(DelayedPacket {
                deliver_at: dup_deliver_at,
                payload,
                target,
                latency_ms: dup_delay,
            });
        }

        true
    }

    /// Drain all packets ready to be delivered at timestamp `now`.
    pub fn drain_ready(&mut self, now: Instant) -> Vec<(Vec<u8>, SocketAddr)> {
        let mut ready = Vec::new();
        let mut idx = 0;
        while idx < self.queue.len() {
            if self.queue[idx].deliver_at <= now {
                let pkt = self.queue.remove(idx).unwrap();
                self.stats.packets_forwarded += 1;
                self.stats.bytes_forwarded += pkt.payload.len() as u64;
                self.stats.total_simulated_latency_ms += pkt.latency_ms;
                ready.push((pkt.payload, pkt.target));
            } else {
                idx += 1;
            }
        }
        ready
    }

    /// Pending packet count currently queued in simulation pipeline.
    pub fn pending_count(&self) -> usize {
        self.queue.len()
    }
}

/// Bidirectional UDP proxy server that applies network impairments between clients and an upstream server.
pub struct UdpProxyServer {
    pub socket: std::net::UdpSocket,
    pub upstream_addr: SocketAddr,
    pub client_addr: Option<SocketAddr>,
    pub upstream_pipeline: ImpairmentPipeline,
    pub downstream_pipeline: ImpairmentPipeline,
}

impl UdpProxyServer {
    pub fn bind(
        listen_addr: &str,
        upstream_addr: SocketAddr,
        config: NetworkProxyConfig,
    ) -> crate::Result<Self> {
        let socket = std::net::UdpSocket::bind(listen_addr)
            .map_err(|e| format!("Proxy failed to bind on {listen_addr}: {e}"))?;
        socket
            .set_nonblocking(true)
            .map_err(|e| format!("Proxy non-blocking failed: {e}"))?;

        let upstream_pipeline = ImpairmentPipeline::new(config.clone(), 42);
        let downstream_pipeline = ImpairmentPipeline::new(config, 84);

        Ok(Self {
            socket,
            upstream_addr,
            client_addr: None,
            upstream_pipeline,
            downstream_pipeline,
        })
    }

    pub fn local_addr(&self) -> crate::Result<SocketAddr> {
        self.socket
            .local_addr()
            .map_err(|e| format!("Proxy local_addr error: {e}").into())
    }

    /// Poll incoming network traffic, advance pipelines, and forward ready packets.
    pub fn poll(&mut self, now: Instant) -> crate::Result<usize> {
        let mut buf = [0u8; 2048];
        let mut activity = 0;

        // 1. Read incoming packets
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((len, src)) => {
                    activity += 1;
                    let payload = buf[..len].to_vec();
                    if src == self.upstream_addr {
                        // Traffic from server to client
                        if let Some(client) = self.client_addr {
                            self.downstream_pipeline
                                .process_packet(payload, client, now);
                        }
                    } else {
                        // Traffic from client to server
                        self.client_addr = Some(src);
                        self.upstream_pipeline
                            .process_packet(payload, self.upstream_addr, now);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(format!("Proxy socket error: {e}").into()),
            }
        }

        // 2. Deliver ready packets downstream (to client)
        for (payload, target) in self.downstream_pipeline.drain_ready(now) {
            let _ = self.socket.send_to(&payload, target);
            activity += 1;
        }

        // 3. Deliver ready packets upstream (to server)
        for (payload, target) in self.upstream_pipeline.drain_ready(now) {
            let _ = self.socket.send_to(&payload, target);
            activity += 1;
        }

        Ok(activity)
    }

    /// Run the proxy loop continuously until `stop_signal` is asserted.
    pub fn run(&mut self, stop_signal: Arc<AtomicBool>) -> crate::Result<()> {
        let tick = Duration::from_millis(1);
        while !stop_signal.load(Ordering::Relaxed) {
            let now = Instant::now();
            self.poll(now)?;
            std::thread::sleep(tick);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fast_rng_distribution() {
        let mut rng = FastRng::new(12345);
        let samples = 10_000;
        let mut sum = 0.0;
        for _ in 0..samples {
            let val = rng.next_f64();
            assert!((0.0..1.0).contains(&val));
            sum += val;
        }
        let avg = sum / samples as f64;
        assert!((avg - 0.5).abs() < 0.02, "Uniform average ~0.5, got {avg}");
    }

    #[test]
    fn test_gilbert_elliott_burst_characteristics() {
        let mut model = GilbertElliottModel::new(0.05, 0.20, 0.0, 1.0);
        let mut rng = FastRng::new(999);
        let mut bursts = 0;
        let mut total_dropped = 0;
        let packets = 20_000;
        let mut was_bad = false;

        for _ in 0..packets {
            let dropped = model.step(&mut rng);
            if model.in_bad_state && !was_bad {
                bursts += 1;
            }
            was_bad = model.in_bad_state;
            if dropped {
                total_dropped += 1;
            }
        }

        let steady = model.steady_state_loss();
        let measured_loss = total_dropped as f64 / packets as f64;
        assert!(
            (measured_loss - steady).abs() < 0.05,
            "Expected steady {steady}, got {measured_loss}"
        );
        assert!(bursts > 100, "Should observe multiple burst episodes");
    }

    #[test]
    fn test_gaussian_jitter_box_muller() {
        let jitter = GaussianJitter::new(50.0, 10.0);
        let mut rng = FastRng::new(42);
        let samples = 10_000;
        let mut sum = 0.0;
        let mut sum_sq = 0.0;

        for _ in 0..samples {
            let ms = jitter.sample_ms(&mut rng);
            sum += ms;
            sum_sq += ms * ms;
        }

        let mean = sum / samples as f64;
        let variance = (sum_sq / samples as f64) - (mean * mean);
        let std_dev = variance.sqrt();

        assert!((mean - 50.0).abs() < 1.0, "Mean should be ~50, got {mean}");
        assert!(
            (std_dev - 10.0).abs() < 1.0,
            "StdDev should be ~10, got {std_dev}"
        );
    }

    #[test]
    fn test_pipeline_duplication_and_latency() {
        let config = NetworkProxyConfig {
            name: "test".into(),
            burst_loss: GilbertElliottModel::new(0.0, 1.0, 0.0, 0.0), // No loss
            jitter: GaussianJitter::new(20.0, 0.0),                   // Fixed 20ms
            duplication_rate: 1.0,                                    // 100% duplication
            duplication_offset_ms: 10.0,
        };
        let mut pipeline = ImpairmentPipeline::new(config, 77);
        let target: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let start = Instant::now();

        pipeline.process_packet(b"ping".to_vec(), target, start);
        assert_eq!(pipeline.pending_count(), 2, "Original + duplicate queued");

        // After 10ms: neither ready
        let ready = pipeline.drain_ready(start + Duration::from_millis(10));
        assert_eq!(ready.len(), 0);

        // After 21ms: original ready
        let ready = pipeline.drain_ready(start + Duration::from_millis(21));
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].0, b"ping");

        // After 31ms: duplicate ready
        let ready = pipeline.drain_ready(start + Duration::from_millis(31));
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].0, b"ping");
    }
}
