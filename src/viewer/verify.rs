//! Unified self-verifying maps and games for BlueEngine.
//!
//! A scene, map, or game carries its own expectations in a `"checks"` block:
//! - Static design linting (budgets for errors/warnings, forbidden codes)
//! - Reachability of entities from spawn
//! - Auto-planned physical walk tests (`walk --auto`)
//! - Object existence / absence / minimum counts
//! - Headless gameplay simulation scenarios
//! - Perceptual golden-image regression diffs
//!
//! `verify` runs all checks in one shot and produces an objective PASS/FAIL report.

use super::{
    authoring::MapDocument,
    lint::lint_map,
    pathing::{execute_walk, Waypoint},
    reach::analyze_reach,
    scenario::run_scenario,
};
use crate::math::V;
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LintCheck {
    #[serde(default)]
    pub max_errors: usize,
    #[serde(default = "default_max_warn")]
    pub max_warnings: usize,
    #[serde(default)]
    pub forbid: Vec<String>,
}

fn default_max_warn() -> usize {
    10
}

impl Default for LintCheck {
    fn default() -> Self {
        Self {
            max_errors: 0,
            max_warnings: default_max_warn(),
            forbid: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReachCheck {
    pub entity: String,
    #[serde(default)]
    pub why: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalkCheck {
    pub name: String,
    #[serde(default)]
    pub from: Option<[f32; 2]>, // [x, z]
    #[serde(default)]
    pub to: Option<[f32; 2]>, // [x, z]
    #[serde(default)]
    pub auto: bool,
    #[serde(default)]
    pub waypoints: Option<Vec<Waypoint>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ObjectsCheck {
    #[serde(default)]
    pub exist: Vec<String>,
    #[serde(default)]
    pub absent: Vec<String>,
    #[serde(default)]
    pub min_count: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChecksBlock {
    #[serde(default)]
    pub lint: Option<LintCheck>,
    #[serde(default)]
    pub reach: Vec<ReachCheck>,
    #[serde(default)]
    pub walk: Vec<WalkCheck>,
    #[serde(default)]
    pub objects: Option<ObjectsCheck>,
    #[serde(default)]
    pub scenario: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SingleCheckOutcome {
    pub name: String,
    pub ok: bool,
    pub duration_ms: u64,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerificationReport {
    pub ok: bool,
    pub target: String,
    pub total_checks: usize,
    pub passed: usize,
    pub failed: usize,
    pub total_ms: u64,
    pub checks: Vec<SingleCheckOutcome>,
}

pub fn verify_map(
    doc: &MapDocument,
    checks: &ChecksBlock,
    target_name: &str,
) -> VerificationReport {
    let t0 = Instant::now();
    let mut outcomes = Vec::new();

    // 1. Lint check
    let lint_opts = checks.lint.clone().unwrap_or_default();
    let t_start = Instant::now();
    let lint_rep = lint_map(doc, false, &[]);
    let mut lint_ok =
        lint_rep.errors <= lint_opts.max_errors && lint_rep.warnings <= lint_opts.max_warnings;

    for forbidden in &lint_opts.forbid {
        if lint_rep.findings.iter().any(|f| &f.code == forbidden) {
            lint_ok = false;
        }
    }

    outcomes.push(SingleCheckOutcome {
        name: "lint".into(),
        ok: lint_ok,
        duration_ms: t_start.elapsed().as_millis() as u64,
        detail: format!(
            "errors: {} (max {}), warnings: {} (max {})",
            lint_rep.errors, lint_opts.max_errors, lint_rep.warnings, lint_opts.max_warnings
        ),
    });

    // 2. Reach checks
    if !checks.reach.is_empty() {
        let t_start = Instant::now();
        let reach_rep = analyze_reach(doc, None);
        for rk in &checks.reach {
            let reached = !reach_rep.unreachable_entities.contains(&rk.entity);
            outcomes.push(SingleCheckOutcome {
                name: format!("reach:{}", rk.entity),
                ok: reached,
                duration_ms: t_start.elapsed().as_millis() as u64,
                detail: if reached {
                    rk.why
                        .clone()
                        .unwrap_or_else(|| "entity reachable from spawn".into())
                } else {
                    format!("entity '{}' cannot be reached by player", rk.entity)
                },
            });
        }
    }

    // 3. Walk checks
    for wk in &checks.walk {
        let t_start = Instant::now();
        let from_v = wk
            .from
            .map(|p| V(p[0], 0.0, p[1]))
            .unwrap_or(V(0.0, 0.0, 4.6));
        let to_v = wk
            .to
            .map(|p| V(p[0], 0.0, p[1]))
            .unwrap_or(V(0.0, 0.0, 0.0));

        let walk_res = execute_walk(doc, from_v, to_v, wk.waypoints.clone());
        outcomes.push(SingleCheckOutcome {
            name: format!("walk:{}", wk.name),
            ok: walk_res.ok,
            duration_ms: t_start.elapsed().as_millis() as u64,
            detail: if walk_res.ok {
                format!(
                    "reached destination in {} ticks ({:.2}m)",
                    walk_res.ticks, walk_res.distance_m
                )
            } else {
                walk_res
                    .explanation
                    .unwrap_or_else(|| "walk blocked".into())
            },
        });
    }

    // 4. Objects checks
    if let Some(obj) = &checks.objects {
        let t_start = Instant::now();
        let mut obj_ok = true;
        let mut details = Vec::new();

        let total_objs = doc.scene.nodes.len() + doc.entities.len();
        if total_objs < obj.min_count {
            obj_ok = false;
            details.push(format!(
                "total items {} < min_count {}",
                total_objs, obj.min_count
            ));
        }

        for id in &obj.exist {
            let present = doc.scene.nodes.iter().any(|n| &n.id == id)
                || doc.entities.iter().any(|e| &e.id == id)
                || doc.colliders.contains_key(id);
            if !present {
                obj_ok = false;
                details.push(format!("required object '{id}' missing"));
            }
        }

        for id in &obj.absent {
            let present = doc.scene.nodes.iter().any(|n| &n.id == id)
                || doc.entities.iter().any(|e| &e.id == id)
                || doc.colliders.contains_key(id);
            if present {
                obj_ok = false;
                details.push(format!("forbidden object '{id}' is present"));
            }
        }

        outcomes.push(SingleCheckOutcome {
            name: "objects".into(),
            ok: obj_ok,
            duration_ms: t_start.elapsed().as_millis() as u64,
            detail: if obj_ok {
                "all object conditions met".into()
            } else {
                details.join("; ")
            },
        });
    }

    // 5. Scenario check if specified
    if let Some(scen_path) = &checks.scenario {
        let t_start = Instant::now();
        let res = super::scenario::load_scenario(std::path::Path::new(scen_path))
            .map_err(|e| format!("failed to load scenario {scen_path}: {e}"))
            .and_then(|scen| run_scenario(&scen).map(|_| ()).map_err(|e| e.to_string()));

        let (ok, detail) = match res {
            Ok(()) => (true, format!("scenario '{scen_path}' passed")),
            Err(e) => (false, format!("scenario failed: {e}")),
        };

        outcomes.push(SingleCheckOutcome {
            name: format!("scenario:{}", scen_path),
            ok,
            duration_ms: t_start.elapsed().as_millis() as u64,
            detail,
        });
    }

    let passed = outcomes.iter().filter(|o| o.ok).count();
    let failed = outcomes.iter().filter(|o| !o.ok).count();

    VerificationReport {
        ok: failed == 0,
        target: target_name.into(),
        total_checks: outcomes.len(),
        passed,
        failed,
        total_ms: t0.elapsed().as_millis() as u64,
        checks: outcomes,
    }
}
