//! Edit a game in place without hand-keeping its records in step.
//!
//! An interactable lives in four places in `map.json` (material, node, collider, entity) plus one in
//! `game.json`, and `game-validate` rejects any mismatch. [`add_interactable`] creates all five from one
//! description, validates the result as a whole and only then writes anything.
use super::{
    authoring::{Edit, MapDocument},
    game::{GameDocument, Interactable, MAX_GAME_BYTES, MAX_MAP_BYTES},
};
use crate::{math::V, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// What to add. Sizes are half extents in metres, colours linear RGB in 0..1, like `add_box`.
#[derive(Clone, Debug)]
pub struct InteractableSpec {
    pub id: String,
    pub label: Option<String>,
    pub center: V,
    pub half_extents: V,
    pub color: V,
    /// Start eligible for interaction. Rules usually enable a chain of them one at a time.
    pub enabled: bool,
    pub visible: bool,
}

impl InteractableSpec {
    pub fn new(id: &str, center: V) -> Self {
        Self {
            id: id.into(),
            label: None,
            center,
            half_extents: V(0.3, 0.3, 0.3),
            color: V(0.2, 0.5, 0.95),
            enabled: true,
            visible: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AddReport {
    pub game: PathBuf,
    pub map: PathBuf,
    pub written: bool,
    /// The records that were (or, without `write`, would be) created.
    pub records: Value,
    pub warnings: Vec<String>,
    /// A rule to adapt: rules are the one thing this tool cannot guess.
    pub rule_template: Value,
}

/// Add one interactable box to a game and its map.
///
/// Everything is validated first: the map with the new box, then the game against that map. With
/// `write` false nothing is touched. With `write` true both files are replaced atomically (each is
/// written to a sibling temporary file and renamed) and the map is restored if the game cannot be.
pub fn add_interactable(
    game_path: &Path,
    spec: &InteractableSpec,
    write: bool,
) -> Result<AddReport> {
    let game_bytes = read_limited(game_path, MAX_GAME_BYTES)?;
    let document: GameDocument = serde_json::from_slice(&game_bytes)?;
    let map_path = document.map_path(game_path)?;
    let map_bytes = read_limited(&map_path, MAX_MAP_BYTES)?;
    let map: MapDocument = serde_json::from_slice(&map_bytes)?;
    document.validate(&map)?;

    if document.interactables.iter().any(|i| i.entity == spec.id) {
        return Err(format!(
            "The game already declares an interactable named {}",
            spec.id
        )
        .into());
    }
    let label = spec.label.clone().unwrap_or_else(|| spec.id.clone());
    let next_map = map.apply(&[Edit::AddBox {
        id: spec.id.clone(),
        label,
        center: spec.center,
        half_extents: spec.half_extents,
        color: spec.color,
        structural: false,
    }])?;
    let mut next_game = document.clone();
    next_game.interactables.push(Interactable {
        entity: spec.id.clone(),
        enabled: spec.enabled,
        visible: spec.visible,
    });
    next_game.validate(&next_map)?;

    let records = json!({
        "map.scene.materials": next_map.scene.materials.get(&format!("edit-{}", spec.id)),
        "map.scene.nodes": next_map.scene.nodes.iter().find(|n| n.id == spec.id),
        "map.colliders": next_map.colliders.get(&spec.id),
        "map.entities": next_map.entities.iter().find(|e| e.id == spec.id),
        "game.interactables": next_game.interactables.last(),
    });
    let mut warnings = Vec::new();
    // A rule with no trigger at all matches every interactable; timer and zone rules do not.
    let reacts = |r: &super::game::Rule| match r.on_interact.as_deref() {
        Some(target) => target == spec.id,
        None => r.on_enter.is_none() && r.on_exit.is_none() && r.on_timer.is_none(),
    };
    if !document.rules.iter().any(reacts) {
        warnings.push(format!(
            "No rule reacts to {} yet: add one with on_interact \"{}\" (see rule_template).",
            spec.id, spec.id
        ));
    }
    // Two interactable boxes that overlap make "what am I aiming at" ambiguous.
    let bounds = next_map.colliders[&spec.id].clone();
    for other in &map.entities {
        let (a, b) = (&bounds, &other.bounds);
        let overlaps = a.min.0 < b.max.0
            && a.max.0 > b.min.0
            && a.min.1 < b.max.1
            && a.max.1 > b.min.1
            && a.min.2 < b.max.2
            && a.max.2 > b.min.2;
        if overlaps && document.interactables.iter().any(|i| i.entity == other.id) {
            warnings.push(format!(
                "{} overlaps interactable {}; aiming at the overlap is ambiguous.",
                spec.id, other.id
            ));
        }
    }
    let rule_template = json!({
        "id": format!("press-{}", spec.id), "on_interact": spec.id, "condition": null, "once": true,
        "actions": [{"action": "increment", "counter": "COUNTER", "amount": 1}],
    });

    if write {
        let game_out = pretty(&next_game, MAX_GAME_BYTES)?;
        let map_out = pretty(&next_map, MAX_MAP_BYTES)?;
        replace_atomically(&map_path, &map_out)?;
        if let Err(error) = replace_atomically(game_path, &game_out) {
            let restored = replace_atomically(&map_path, &map_bytes);
            return Err(match restored {
                Ok(()) => format!("{error}; the map was left as it was"),
                Err(again) => format!("{error}; restoring the map also failed: {again}"),
            }
            .into());
        }
    }
    Ok(AddReport {
        game: game_path.to_path_buf(),
        map: map_path,
        written: write,
        records,
        warnings,
        rule_template,
    })
}

fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let bytes = std::fs::read(path)?;
    if bytes.len() as u64 > limit {
        return Err(format!("{} exceeds its {limit}-byte limit", path.display()).into());
    }
    Ok(bytes)
}

fn pretty(value: &impl serde::Serialize, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    if bytes.len() as u64 > limit {
        return Err(format!("The result would exceed the {limit}-byte document limit").into());
    }
    Ok(bytes)
}

/// Write to a sibling temporary file, then rename over the target.
fn replace_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let name = path.file_name().ok_or("Path has no file name")?;
    let tmp = path.with_file_name(format!(".{}.tmp", name.to_string_lossy()));
    let outcome = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, path));
    if outcome.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(outcome?)
}
