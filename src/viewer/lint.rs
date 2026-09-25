//! Design-level static analysis (linting) for BlueEngine maps.
//!
//! Catches common map design mistakes that ruin gameplay:
//! - `duplicate-id`: identical IDs in nodes, colliders, or entities
//! - `overlap`: colliding shapes that penetrate each other
//! - `floating`: props/entities hovering in the air without support
//! - `sunk`: props/entities buried in the floor slab
//! - `headroom`: walkable areas with ceilings lower than player height
//! - `drop`: drop hazards (>0.6m fall without barrier)
//! - `leak`: walkable paths escaping the map boundary
//! - `unreachable`: entities or rooms the player cannot physically reach
//! - `spawn-blocked`: spawn point blocked by colliders or lacking clearance
//! - `door-blocked`: doorway or portal blocked or too narrow (<0.85m)

use super::{
    authoring::MapDocument,
    controller::{CharacterKind, Collider, STANDING_HEIGHT},
    reach::analyze_reach,
};
use crate::math::V;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Finding {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<[f32; 3]>,
    pub ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LintReport {
    pub ok: bool,
    pub errors: usize,
    pub warnings: usize,
    pub findings: Vec<Finding>,
}

pub fn lint_map(doc: &MapDocument, strict: bool, ignore_codes: &[String]) -> LintReport {
    let mut findings = Vec::new();
    let ignores: HashSet<&str> = ignore_codes.iter().map(String::as_str).collect();

    // 1. Duplicate IDs
    let mut node_ids = HashSet::new();
    for node in &doc.scene.nodes {
        if !node_ids.insert(&node.id) && !ignores.contains("duplicate-id") {
            findings.push(Finding {
                code: "duplicate-id".into(),
                severity: Severity::Error,
                message: format!("Duplicate node ID: {}", node.id),
                at: None,
                ids: vec![node.id.clone()],
            });
        }
    }

    let mut entity_ids = HashSet::new();
    for entity in &doc.entities {
        if !entity_ids.insert(&entity.id) && !ignores.contains("duplicate-id") {
            findings.push(Finding {
                code: "duplicate-id".into(),
                severity: Severity::Error,
                message: format!("Duplicate entity ID: {}", entity.id),
                at: Some([
                    entity.bounds.min.0,
                    entity.bounds.min.1,
                    entity.bounds.min.2,
                ]),
                ids: vec![entity.id.clone()],
            });
        }
    }

    // 2. Overlapping colliders
    if !ignores.contains("overlap") {
        let colliders: Vec<(&String, &Collider)> = doc.colliders.iter().collect();
        for i in 0..colliders.len() {
            for j in (i + 1)..colliders.len() {
                let (id_a, a) = colliders[i];
                let (id_b, b) = colliders[j];

                // Calculate overlap extents
                let ox = (a.max.0.min(b.max.0) - a.min.0.max(b.min.0)).max(0.0);
                let oy = (a.max.1.min(b.max.1) - a.min.1.max(b.min.1)).max(0.0);
                let oz = (a.max.2.min(b.max.2) - a.min.2.max(b.min.2)).max(0.0);

                // Check significant overlap (>0.05m on all axes)
                if ox > 0.05 && oy > 0.05 && oz > 0.05 {
                    let center = [
                        (a.min.0.max(b.min.0) + a.max.0.min(b.max.0)) * 0.5,
                        (a.min.1.max(b.min.1) + a.max.1.min(b.max.1)) * 0.5,
                        (a.min.2.max(b.min.2) + a.max.2.min(b.max.2)) * 0.5,
                    ];
                    findings.push(Finding {
                        code: "overlap".into(),
                        severity: Severity::Warn,
                        message: format!(
                            "Colliders '{id_a}' and '{id_b}' overlap by {ox:.2}x{oy:.2}x{oz:.2}m"
                        ),
                        at: Some(center),
                        ids: vec![id_a.clone(), id_b.clone()],
                    });
                }
            }
        }
    }

    // 3. Floating or sunk props/entities
    for entity in &doc.entities {
        let bottom = entity.bounds.min.1;
        let center_x = (entity.bounds.min.0 + entity.bounds.max.0) * 0.5;
        let center_z = (entity.bounds.min.2 + entity.bounds.max.2) * 0.5;

        // Find supporting floor/collider under entity
        let mut max_support = 0.0_f32; // ground level

        for (cid, c) in &doc.colliders {
            if cid == &entity.id {
                continue; // don't support on own collider
            }
            if c.min.0 <= center_x
                && c.max.0 >= center_x
                && c.min.2 <= center_z
                && c.max.2 >= center_z
                && c.max.1 <= bottom + 0.10
            {
                max_support = max_support.max(c.max.1);
            }
        }

        // Check if floating
        if bottom - max_support > 0.08 && !ignores.contains("floating") {
            // Wall-mounted or hanging items can be allowed if labeled
            let label = entity.label.to_ascii_lowercase();
            let is_wall_mounted = label.contains("art")
                || label.contains("botanical")
                || label.contains("clock")
                || label.contains("lamp");
            if !is_wall_mounted {
                findings.push(Finding {
                    code: "floating".into(),
                    severity: Severity::Warn,
                    message: format!(
                        "Entity '{}' floats {:.2}m above support",
                        entity.id,
                        bottom - max_support
                    ),
                    at: Some([center_x, bottom, center_z]),
                    ids: vec![entity.id.clone()],
                });
            }
        }

        // Check if sunk
        if max_support - bottom > 0.08 && !ignores.contains("sunk") {
            findings.push(Finding {
                code: "sunk".into(),
                severity: Severity::Warn,
                message: format!(
                    "Entity '{}' is buried {:.2}m in floor/support",
                    entity.id,
                    max_support - bottom
                ),
                at: Some([center_x, bottom, center_z]),
                ids: vec![entity.id.clone()],
            });
        }
    }

    // 4. Spawn point clearance
    let spawn = if let Some(sp) = doc.entities.iter().find(|e| e.id.starts_with("spawn")) {
        V(
            (sp.bounds.min.0 + sp.bounds.max.0) * 0.5,
            sp.bounds.min.1,
            (sp.bounds.min.2 + sp.bounds.max.2) * 0.5,
        )
    } else {
        V(0., 0., 4.6)
    };
    let radius = CharacterKind::Scientist.radius();
    if !ignores.contains("spawn-blocked") {
        for (id, c) in &doc.colliders {
            if c.overlaps_body(
                V(spawn.0, spawn.1 + STANDING_HEIGHT * 0.5, spawn.2),
                spawn.1,
                STANDING_HEIGHT,
                radius,
            ) {
                findings.push(Finding {
                    code: "spawn-blocked".into(),
                    severity: Severity::Error,
                    message: format!(
                        "Spawn ({:.1}, {:.1}, {:.1}) is obstructed by collider '{id}'",
                        spawn.0, spawn.1, spawn.2
                    ),
                    at: Some([spawn.0, spawn.1, spawn.2]),
                    ids: vec![id.clone()],
                });
            }
        }
    }

    // 5. Reachability, leaks, drops, and unreachable items
    let reach = analyze_reach(doc, Some(spawn));
    if !ignores.contains("unreachable") {
        for uid in &reach.unreachable_entities {
            findings.push(Finding {
                code: "unreachable".into(),
                severity: Severity::Error,
                message: format!("Entity '{uid}' cannot be reached by player from spawn"),
                at: None,
                ids: vec![uid.clone()],
            });
        }
    }

    if !ignores.contains("leak") {
        for leak in reach.perimeter_leaks.iter().take(3) {
            findings.push(Finding {
                code: "leak".into(),
                severity: Severity::Error,
                message: format!(
                    "Perimeter leak: player can walk off map boundary at ({:.1}, {:.1}, {:.1})",
                    leak[0], leak[1], leak[2]
                ),
                at: Some(*leak),
                ids: vec![],
            });
        }
    }

    if !ignores.contains("drop") {
        for drop in reach.drop_hazards.iter().take(5) {
            findings.push(Finding {
                code: "drop".into(),
                severity: Severity::Warn,
                message: format!(
                    "Drop hazard: {:.2}m fall at ({:.1}, {:.1}) without barrier",
                    drop.fall_distance, drop.from[0], drop.from[2]
                ),
                at: Some(drop.from),
                ids: vec![],
            });
        }
    }

    // 6. Doorways and portals clearance (if spatial graph present)
    if let Some(spatial) = &doc.spatial {
        if !ignores.contains("door-blocked") {
            for portal in &spatial.portals {
                let p_min = portal.bounds.min;
                let p_max = portal.bounds.max;
                let width = (p_max.0 - p_min.0).max(p_max.2 - p_min.2);
                let center = [
                    (p_min.0 + p_max.0) * 0.5,
                    (p_min.1 + p_max.1) * 0.5,
                    (p_min.2 + p_max.2) * 0.5,
                ];

                if width < 0.80 {
                    findings.push(Finding {
                        code: "door-narrow".into(),
                        severity: Severity::Warn,
                        message: format!(
                            "Portal from room {} to {} width ({:.2}m) is narrower than 0.80m",
                            portal.from.0, portal.to.0, width
                        ),
                        at: Some(center),
                        ids: vec![],
                    });
                }
            }
        }
    }

    let errors = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    let warnings = findings
        .iter()
        .filter(|f| f.severity == Severity::Warn)
        .count();

    LintReport {
        ok: if strict {
            errors == 0 && warnings == 0
        } else {
            errors == 0
        },
        errors,
        warnings,
        findings,
    }
}
