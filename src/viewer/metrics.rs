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
