//! Monotonically increasing action counters for reliable input edges over unreliable networks.
use serde::{Deserialize, Serialize};

/// Monotonically increasing counters for actions.
///
/// Instead of transmitting fragile one-frame booleans (which are permanently lost if a packet drops),
/// clients increment counters on each action press. The server detects actions whenever
/// `new_counter > last_seen_counter`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionCounters {
    #[serde(default)]
    pub jump: u32,
    #[serde(default)]
    pub primary: u32,
    #[serde(default)]
    pub secondary: u32,
    #[serde(default)]
    pub interact: u32,
}

impl ActionCounters {
    pub const fn new() -> Self {
        Self {
            jump: 0,
            primary: 0,
            secondary: 0,
            interact: 0,
        }
    }
}

/// Resulting discrete action edges detected from comparing incoming counters against server-stored counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActionEdges {
    pub jump: bool,
    pub primary: bool,
    pub secondary: bool,
    pub interact: bool,
}

/// Server-side tracker that detects new action edges and advances stored counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActionCountersTracker {
    pub last_seen: ActionCounters,
}

impl ActionCountersTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Compare incoming counters against the last seen counters.
    /// Detects new action edges and advances the stored counters monotonically.
    pub fn update(&mut self, incoming: &ActionCounters) -> ActionEdges {
        let edges = ActionEdges {
            jump: incoming.jump > self.last_seen.jump,
            primary: incoming.primary > self.last_seen.primary,
            secondary: incoming.secondary > self.last_seen.secondary,
            interact: incoming.interact > self.last_seen.interact,
        };

        if incoming.jump > self.last_seen.jump {
            self.last_seen.jump = incoming.jump;
        }
        if incoming.primary > self.last_seen.primary {
            self.last_seen.primary = incoming.primary;
        }
        if incoming.secondary > self.last_seen.secondary {
            self.last_seen.secondary = incoming.secondary;
        }
        if incoming.interact > self.last_seen.interact {
            self.last_seen.interact = incoming.interact;
        }

        edges
    }
}
