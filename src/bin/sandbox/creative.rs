//! Rendering-free creative placement and per-map persistence.
use std::collections::HashSet;
use std::path::Path;
use vesper3d::{math::Mat, prelude::*, scene::Track, viewer::controller::Collider};

pub fn rotate(p: V, turns: u8) -> V {
    match turns % 4 {
        0 => p,
        1 => V(p.2, p.1, -p.0),
        2 => V(-p.0, p.1, -p.2),
        _ => V(-p.2, p.1, p.0),
    }
}
fn rotated_angles(r: V, turns: u8) -> V {
    let m = Mat::trs(V::ZERO, r, V::ONE);
    let x = rotate(m.x, turns);
    let y = rotate(m.y, turns);
    let z = rotate(m.z, turns);
    let pitch = (-x.2).clamp(-1., 1.).asin();
    let (roll, yaw) = if pitch.cos().abs() > 0.00001 {
        (y.2.atan2(z.2), x.1.atan2(x.0))
    } else {
        ((-z.1).atan2(y.1), 0.)
    };
    V(roll.to_degrees(), pitch.to_degrees(), yaw.to_degrees())
}
pub fn bounds(b: &Collider, at: V, turns: u8) -> Collider {
    let a = rotate(b.min, turns) + at;
    let z = rotate(b.max, turns) + at;
    Collider {
        min: V(a.0.min(z.0), a.1.min(z.1), a.2.min(z.2)),
        max: V(a.0.max(z.0), a.1.max(z.1), a.2.max(z.2)),
    }
}
/// Extract only the catalog specimen, never its studio floor, walls or grid.
pub fn specimen(mut doc: MapDocument) -> Result<MapDocument> {
    doc.scene.nodes.retain(|n| n.id.starts_with("specimen/"));
    doc.colliders
        .retain(|id, _| id == "specimen" || id.starts_with("specimen/"));
    doc.entities.retain(|e| e.id == "specimen");
    let used: HashSet<_> = doc.scene.nodes.iter().map(|n| n.material.clone()).collect();
    doc.scene.materials.retain(|id, _| used.contains(id));
    doc.default_spawn = None;
    doc.spatial = None;
    doc.checks = None;
    if doc.scene.nodes.is_empty() || doc.entities.len() != 1 {
        return Err("This catalog item has no placeable specimen".into());
    }
    doc.validate()?;
    Ok(doc)
}
pub fn next_id(doc: &MapDocument) -> String {
    let mut n = 1;
    loop {
        let id = format!("creative-{n}");
        if !doc.entities.iter().any(|e| e.id == id) {
            return id;
        }
        n += 1;
    }
}
pub fn place(
    doc: &MapDocument,
    asset: &MapDocument,
    at: V,
    turns: u8,
    player: &Controller,
) -> Result<MapDocument> {
    if !at.finite() || at.1 < 0. {
        return Err("Choose a position above the ground".into());
    }
    if doc
        .entities
        .iter()
        .filter(|e| e.id.starts_with("creative-"))
        .count()
        >= 256
    {
        return Err("This world has reached its 256 placed-object limit".into());
    }
    let mut result = doc.clone();
    let id = next_id(doc);
    let rename = |name: &str| name.replacen("specimen", &id, 1);
    for (name, mat) in &asset.scene.materials {
        result
            .scene
            .materials
            .insert(format!("{id}/{name}"), mat.clone());
    }
    for source in &asset.scene.nodes {
        let mut n = source.clone();
        n.id = rename(&n.id);
        n.material = format!("{id}/{}", n.material);
        if let Track::Fixed(p) = n.pos {
            n.pos = Track::Fixed(rotate(p, turns) + at);
        }
        if let Track::Fixed(r) = n.rot {
            n.rot = Track::Fixed(rotated_angles(r, turns));
        }
        result.scene.nodes.push(n);
    }
    for (name, b) in &asset.colliders {
        let b = bounds(b, at, turns);
        if b.max.1 > player.feet_height() + 0.001
            && b.min.1 < player.feet_height() + player.body_height() - 0.001
            && b.overlaps_xz(player.position, player.character_kind().radius())
        {
            return Err("Move the preview away from your character".into());
        }
        if let Some(spawn) = doc.default_spawn {
            if b.max.1 > spawn.feet.1 + 0.001
                && b.min.1 < spawn.feet.1 + 1.80 - 0.001
                && b.overlaps_xz(spawn.feet, 0.23)
            {
                return Err("Keep the map's starting area clear".into());
            }
        }
        result.colliders.insert(rename(name), b);
    }
    for e in &asset.entities {
        let mut e = e.clone();
        e.id = rename(&e.id);
        e.bounds = bounds(&e.bounds, at, turns);
        result.entities.push(e);
    }
    result.validate()?; // Includes default spawn and scene capacity protection.
    Ok(result)
}
/// Only player-created objects may be deleted; base map content is preserved.
pub fn remove(doc: &MapDocument, id: &str) -> Result<MapDocument> {
    if !id.starts_with("creative-") || !doc.entities.iter().any(|e| e.id == id) {
        return Err("Aim at an object you placed to remove it".into());
    }
    let prefix = format!("{id}/");
    let mut next = doc.clone();
    next.scene.nodes.retain(|n| !n.id.starts_with(&prefix));
    next.scene
        .materials
        .retain(|name, _| !name.starts_with(&prefix));
    next.colliders
        .retain(|name, _| name != id && !name.starts_with(&prefix));
    next.entities.retain(|e| e.id != id);
    next.validate()?;
    Ok(next)
}
/// Write a complete valid replacement before renaming; a failed write preserves the old save.
pub fn save(doc: &MapDocument, path: &Path) -> Result<()> {
    use std::io::Write;
    doc.validate()?;
    let bytes = serde_json::to_vec(doc)?;
    if bytes.len() > 8_000_000 {
        return Err("World exceeds the 8 MB save limit".into());
    }
    std::fs::create_dir_all(path.parent().ok_or("Missing save directory")?)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let temporary = path.with_extension(format!("{stamp}.tmp"));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(temporary, path)?;
    Ok(())
}
