//! Observability, performance tracking, explainability, and budget validation.
//!
//! Provides structured metrics for AI coding agents and automated CI benchmarks.
use serde::{Deserialize, Serialize};

/// Captured snapshot of engine performance metrics for a simulation frame.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PerformanceSnapshot {
    pub tick: u64,
    pub sim_cpu_time_us: f64,
    pub physics_time_us: f64,
    pub active_dynamic_bodies: usize,
    pub sleeping_bodies: usize,
    pub static_instances: usize,
    pub replicated_entities: usize,
    pub snapshot_bytes: usize,
    pub delta_bytes: usize,
    pub bandwidth_kbps: f64,
}

impl PerformanceSnapshot {
    /// Explain the frame performance in structured, human-readable prose.
    pub fn explain(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("Simulation Tick {}:", self.tick));
        lines.push(format!(
            "  CPU Sim Time: {:.2} µs (Physics: {:.2} µs)",
            self.sim_cpu_time_us, self.physics_time_us
        ));
        lines.push(format!(
            "  Objects: {} static, {} sleeping, {} active dynamic",
            self.static_instances, self.sleeping_bodies, self.active_dynamic_bodies
        ));
        lines.push(format!(
            "  Replication: {} entities, snapshot {} B, delta {} B ({:.1} kbps)",
            self.replicated_entities, self.snapshot_bytes, self.delta_bytes, self.bandwidth_kbps
        ));

        if self.active_dynamic_bodies > 50 {
            lines.push(
                "  Recommendation: >50 active dynamic bodies. Check if resting props can be demoted to static/interactive."
                    .into(),
            );
        }
        if self.snapshot_bytes > 1200 {
            lines.push(
                "  Recommendation: Full snapshot approaches MTU limit. Favor delta compression or spatial interest management."
                    .into(),
            );
        }
        lines.join("\n")
    }
}

/// Explicit performance contract / budget for a scene or game mode.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PerformanceBudget {
    pub max_sim_cpu_time_us: f64,
    pub max_physics_time_us: f64,
    pub max_active_dynamic_bodies: usize,
    pub max_snapshot_bytes: usize,
    pub max_delta_bytes: usize,
}

impl Default for PerformanceBudget {
    fn default() -> Self {
        Self {
            max_sim_cpu_time_us: 4000.0, // 4.0 ms per tick (budget for 60 Hz is 16.6 ms total)
            max_physics_time_us: 2500.0,
            max_active_dynamic_bodies: 64,
            max_snapshot_bytes: 1200,
            max_delta_bytes: 400,
        }
    }
}

/// Result of evaluating a performance snapshot against a budget.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BudgetReport {
    pub passed: bool,
    pub violations: Vec<String>,
}

impl PerformanceBudget {
    pub fn validate(&self, snapshot: &PerformanceSnapshot) -> BudgetReport {
        let mut violations = Vec::new();
        if snapshot.sim_cpu_time_us > self.max_sim_cpu_time_us {
            violations.push(format!(
                "Simulation CPU time ({:.1} µs) exceeded budget ({:.1} µs)",
                snapshot.sim_cpu_time_us, self.max_sim_cpu_time_us
            ));
        }
        if snapshot.physics_time_us > self.max_physics_time_us {
            violations.push(format!(
                "Physics time ({:.1} µs) exceeded budget ({:.1} µs)",
                snapshot.physics_time_us, self.max_physics_time_us
            ));
        }
        if snapshot.active_dynamic_bodies > self.max_active_dynamic_bodies {
            violations.push(format!(
                "Active dynamic bodies ({}) exceeded budget ({})",
                snapshot.active_dynamic_bodies, self.max_active_dynamic_bodies
            ));
        }
        if snapshot.snapshot_bytes > self.max_snapshot_bytes {
            violations.push(format!(
                "Snapshot size ({} B) exceeded budget ({} B)",
                snapshot.snapshot_bytes, self.max_snapshot_bytes
            ));
        }
        if snapshot.delta_bytes > self.max_delta_bytes {
            violations.push(format!(
                "Delta snapshot size ({} B) exceeded budget ({} B)",
                snapshot.delta_bytes, self.max_delta_bytes
            ));
        }

        BudgetReport {
            passed: violations.is_empty(),
            violations,
        }
    }
}

use std::time::{Duration, Instant};

/// Aggregates simulation tick durations and computes running mean/max statistics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickMetrics {
    pub count: u64,
    pub total_us: u128,
    pub max_us: u128,
}

impl TickMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a completed tick execution time in microseconds.
    pub fn record(&mut self, elapsed_us: u128) {
        self.count += 1;
        self.total_us += elapsed_us;
        self.max_us = self.max_us.max(elapsed_us);
    }

    /// Mean execution time per tick in microseconds.
    pub fn mean_us(&self) -> u128 {
        if self.count == 0 {
            0
        } else {
            self.total_us / self.count as u128
        }
    }

    /// Reset counters for the next statistical sampling interval.
    pub fn reset(&mut self) {
        self.count = 0;
        self.total_us = 0;
        self.max_us = 0;
    }
}

/// Accurate fixed-tick execution scheduler preventing clock drift and unbounded catchup bursts.
pub struct FixedTickRunner {
    tick_duration: Duration,
    max_catchup_burst: Duration,
    next_deadline: Instant,
    pub metrics: TickMetrics,
}

impl FixedTickRunner {
    /// Create a runner for a target tick rate (e.g. 60 Hz).
    pub fn new(hz: u64) -> Self {
        let tick_duration = Duration::from_secs_f64(1.0 / hz.max(1) as f64);
        Self {
            tick_duration,
            max_catchup_burst: Duration::from_millis(133), // max ~8 frames burst
            next_deadline: Instant::now(),
            metrics: TickMetrics::new(),
        }
    }

    /// Create a runner with an explicit tick duration.
    pub fn with_duration(tick_duration: Duration) -> Self {
        Self {
            tick_duration,
            max_catchup_burst: Duration::from_millis(133),
            next_deadline: Instant::now(),
            metrics: TickMetrics::new(),
        }
    }

    /// Advance deadline and sleep until the scheduled time.
    ///
    /// Maintains running deadline (`next += tick_duration; sleep(next - now)`) to eliminate
    /// cumulative clock drift, while clamping if the machine was suspended or heavily stalled.
    pub fn sleep_until_next_tick(&mut self, elapsed_us: u128) {
        self.metrics.record(elapsed_us);
        self.next_deadline += self.tick_duration;
        let now = Instant::now();
        if self.next_deadline + self.max_catchup_burst < now {
            self.next_deadline = now;
        }
        let sleep_duration = self.next_deadline.saturating_duration_since(now);
        if !sleep_duration.is_zero() {
            std::thread::sleep(sleep_duration);
        }
    }

    /// Convenience wrapper: executes `step_fn`, records timing, and sleeps until next tick.
    pub fn step<F: FnOnce() -> crate::Result<()>>(&mut self, step_fn: F) -> crate::Result<()> {
        let started = Instant::now();
        step_fn()?;
        let elapsed = started.elapsed().as_micros();
        self.sleep_until_next_tick(elapsed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_validation_detects_violations() {
        let budget = PerformanceBudget::default();
        let mut snap = PerformanceSnapshot {
            sim_cpu_time_us: 1000.0,
            active_dynamic_bodies: 10,
            snapshot_bytes: 300,
            ..Default::default()
        };

        let report = budget.validate(&snap);
        assert!(report.passed);
        assert!(report.violations.is_empty());

        // Exceed budget
        snap.active_dynamic_bodies = 120;
        let fail_report = budget.validate(&snap);
        assert!(!fail_report.passed);
        assert_eq!(fail_report.violations.len(), 1);
        assert!(fail_report.violations[0].contains("Active dynamic bodies"));
    }
}
