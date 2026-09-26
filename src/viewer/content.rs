//! Versioned content fingerprint for accidental map mismatch detection, not authentication.
use super::room::Room;
use std::collections::{BTreeMap, BTreeSet};

/// Hash the loaded scene, collision, semantic entities and spatial graph before physics extraction.
/// Ordered serialization and FNV-1a make this stable across processes for identical content.
/// This is a non-cryptographic compatibility check; hostile peers can forge it.
pub fn fingerprint(room: &Room) -> u64 {
    // Runtime graph tables use randomized HashMap/HashSet iteration. Canonicalize
    // both levels before hashing, including adjacency lists represented as arrays.
    let spatial = room.spatial.as_ref().map(|graph| {
        (
            graph.rooms.iter().collect::<BTreeMap<_, _>>(),
            &graph.portals,
            graph
                .adjacency
                .iter()
                .map(|(id, neighbors)| (id, neighbors.iter().collect::<BTreeSet<_>>()))
                .collect::<BTreeMap<_, _>>(),
        )
    });
    let bytes = serde_json::to_vec(&(
        2_u32,
        &room.name,
        &room.compiled.scene,
        &room.colliders,
        &room.entities,
        &room.default_spawn,
        spatial,
    ))
    .expect("Room content consists only of serializable owned data");
    bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
