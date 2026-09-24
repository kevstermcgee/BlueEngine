//! Room / zone graph and spatial partitioning for indoor maps and interest management.
//!
//! Enables spatial relevance filtering for:
//! - Rendering culling (geometry not relevant from current room)
//! - Low-latency network interest management (replicate high-frequency deltas only to adjacent rooms)
//! - Prop activation / sleep zones
use crate::math::V;
use crate::viewer::controller::Collider;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Stable identifier for a discrete room or outdoor zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RoomId(pub u32);

/// A portal connecting two adjacent rooms (e.g. doorway, archway, window opening).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Portal {
    pub from: RoomId,
    pub to: RoomId,
    pub bounds: Collider,
    pub is_open: bool,
}

/// A node in the spatial zone graph.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoomNode {
    pub id: RoomId,
    pub name: String,
    pub bounds: Collider,
    pub floor_level: u32,
}

/// The spatial room connectivity graph.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RoomGraph {
    pub rooms: HashMap<RoomId, RoomNode>,
    pub portals: Vec<Portal>,
    pub adjacency: HashMap<RoomId, HashSet<RoomId>>,
}

impl RoomGraph {
    pub fn new() -> Self {
        Self {
            rooms: HashMap::new(),
            portals: Vec::new(),
            adjacency: HashMap::new(),
        }
    }

    pub fn add_room(&mut self, id: RoomId, name: &str, bounds: Collider, floor_level: u32) {
        self.rooms.insert(
            id,
            RoomNode {
                id,
                name: name.to_string(),
                bounds,
                floor_level,
            },
        );
        self.adjacency.entry(id).or_default();
    }

    pub fn add_portal(&mut self, from: RoomId, to: RoomId, bounds: Collider) {
        self.portals.push(Portal {
            from,
            to,
            bounds,
            is_open: true,
        });
        self.adjacency.entry(from).or_default().insert(to);
        self.adjacency.entry(to).or_default().insert(from);
    }

    /// Locate which room/zone contains a 3D point (e.g. player position).
    pub fn find_room_at(&self, p: V) -> Option<RoomId> {
        for (id, node) in &self.rooms {
            if node.bounds.contains(p) {
                return Some(*id);
            }
        }
        None
    }

    /// Get all directly connected / adjacent rooms.
    pub fn adjacent_rooms(&self, id: RoomId) -> HashSet<RoomId> {
        self.adjacency.get(&id).cloned().unwrap_or_default()
    }

    /// Check if target room is relevant for interest management (self or 1-hop neighbor).
    pub fn is_relevant_for_interest(&self, observer: RoomId, target: RoomId) -> bool {
        if observer == target {
            return true;
        }
        if let Some(neighbors) = self.adjacency.get(&observer) {
            neighbors.contains(&target)
        } else {
            false
        }
    }

    /// Pre-configured room graph for the default 2-story House map.
    pub fn house() -> Self {
        let mut g = Self::new();
        // Downstairs
        let r_living = RoomId(1);
        let r_kitchen = RoomId(2);
        let r_hall = RoomId(3);
        let r_backyard = RoomId(4);

        // Upstairs
        let r_bed1 = RoomId(5);
        let r_bed2 = RoomId(6);
        let r_bath = RoomId(7);

        g.add_room(
            r_living,
            "Living Room",
            Collider {
                min: V(-6.5, 0.0, -6.5),
                max: V(0.0, 3.2, 0.0),
            },
            1,
        );
        g.add_room(
            r_kitchen,
            "Kitchen",
            Collider {
                min: V(0.0, 0.0, -6.5),
                max: V(6.5, 3.2, 0.0),
            },
            1,
        );
        g.add_room(
            r_hall,
            "Foyer / Hallway",
            Collider {
                min: V(-2.0, 0.0, 0.0),
                max: V(2.0, 3.2, 6.5),
            },
            1,
        );
        g.add_room(
            r_backyard,
            "Backyard",
            Collider {
                min: V(-15.0, 0.0, -20.0),
                max: V(15.0, 10.0, -6.5),
            },
            1,
        );

        // Upstairs rooms
        g.add_room(
            r_bed1,
            "Master Bedroom",
            Collider {
                min: V(-6.5, 3.2, -6.5),
                max: V(0.0, 6.5, 0.0),
            },
            2,
        );
        g.add_room(
            r_bed2,
            "Guest Bedroom",
            Collider {
                min: V(0.0, 3.2, -6.5),
                max: V(6.5, 6.5, 0.0),
            },
            2,
        );
        g.add_room(
            r_bath,
            "Bathroom",
            Collider {
                min: V(-2.0, 3.2, 0.0),
                max: V(2.0, 6.5, 4.0),
            },
            2,
        );

        // Portals / doorways
        let door_bounds = Collider {
            min: V(-0.5, 0.0, -0.5),
            max: V(0.5, 2.2, 0.5),
        };
        g.add_portal(r_living, r_kitchen, door_bounds.clone());
        g.add_portal(r_living, r_hall, door_bounds.clone());
        g.add_portal(r_kitchen, r_backyard, door_bounds.clone());
        g.add_portal(r_hall, r_bed1, door_bounds.clone()); // via stairs
        g.add_portal(r_bed1, r_bed2, door_bounds.clone());
        g.add_portal(r_bed1, r_bath, door_bounds);

        g
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_graph_spatial_lookup_and_interest_management() {
        let g = RoomGraph::house();

        // Point inside Kitchen
        let kitchen_pt = V(3.0, 1.0, -3.0);
        let room = g.find_room_at(kitchen_pt);
        assert_eq!(room, Some(RoomId(2)));

        // Interest check: Kitchen (2) is adjacent to Living Room (1) and Backyard (4)
        assert!(g.is_relevant_for_interest(RoomId(2), RoomId(1)));
        assert!(g.is_relevant_for_interest(RoomId(2), RoomId(4)));
        // But not directly to upstairs Bathroom (7)
        assert!(!g.is_relevant_for_interest(RoomId(2), RoomId(7)));
    }
}
