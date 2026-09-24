//! Object lifecycle and generalized change tracking.
//!
//! "Static until proven otherwise": Objects begin as low-cost static instances
//! and only graduate into dynamic simulation or replicated networking when
//! players touch, shoot, carry, or interact with them.
use crate::math::V;
use serde::{Deserialize, Serialize};

/// Monotonically increasing generation number for change tracking.
/// Systems store the last generation they processed to avoid re-evaluating
/// unchanged objects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Generation(pub u64);

impl Generation {
    pub const ZERO: Self = Self(0);

    pub fn next(&mut self) -> Self {
        self.0 += 1;
        *self
    }
}

/// The four-tier lifecycle for objects in Blue Engine V2.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum LifecycleState {
    /// Render instance + static collision. No ECS entity, no dynamic physics, no replication.
    #[default]
    StaticInstance,
    /// Static collision + interactive metadata (can be targeted, inspected, or transformed).
    InteractiveStatic,
    /// Active dynamic rigid body in local simulation (has velocity, physics, contacts).
    DynamicEntity,
    /// Active simulation entity replicated over low-latency multiplayer snapshots.
    ReplicatedEntity,
}

impl LifecycleState {
    /// Estimated relative cost tier for AI construction conscience and performance budgets.
    pub fn cost_tier(&self) -> &'static str {
        match self {
            Self::StaticInstance => "CHEAP",
            Self::InteractiveStatic => "LOW",
            Self::DynamicEntity => "MODERATE",
            Self::ReplicatedEntity => "EXPENSIVE",
        }
    }

    pub fn requires_rigid_body(&self) -> bool {
        matches!(self, Self::DynamicEntity | Self::ReplicatedEntity)
    }

    pub fn requires_networking(&self) -> bool {
        matches!(self, Self::ReplicatedEntity)
    }
}

/// A tracked prop or scene object managed under the lifecycle system.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LifecycleObject {
    pub id: String,
    pub label: String,
    pub state: LifecycleState,
    pub generation: Generation,
    pub position: V,
    pub rest_duration: f32,
}

impl LifecycleObject {
    pub fn new(id: String, label: String, position: V) -> Self {
        Self {
            id,
            label,
            state: LifecycleState::StaticInstance,
            generation: Generation(1),
            position,
            rest_duration: 0.0,
        }
    }

    /// Promote to a higher lifecycle tier when interacted with, shot, or picked up.
    /// Returns true if the state changed.
    pub fn promote(&mut self, target: LifecycleState, gen: &mut Generation) -> bool {
        if target > self.state {
            self.state = target;
            self.generation = gen.next();
            self.rest_duration = 0.0;
            true
        } else {
            false
        }
    }

    /// Attempt to demote a settled dynamic object back toward static/interactive after rest.
    /// Returns true if demoted.
    pub fn update_rest(&mut self, dt: f32, speed: f32, gen: &mut Generation) -> bool {
        if !self.state.requires_rigid_body() {
            return false;
        }

        if speed < 0.02 {
            self.rest_duration += dt;
            // After resting stationary for 3.0 seconds, demote from dynamic back to interactive
            if self.rest_duration >= 3.0 {
                self.state = LifecycleState::InteractiveStatic;
                self.generation = gen.next();
                self.rest_duration = 0.0;
                return true;
            }
        } else {
            self.rest_duration = 0.0;
        }
        false
    }
}

/// Lifecycle tracking registry for scenes.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LifecycleRegistry {
    pub current_generation: Generation,
    pub objects: Vec<LifecycleObject>,
}

impl LifecycleRegistry {
    pub fn new() -> Self {
        Self {
            current_generation: Generation(1),
            objects: Vec::new(),
        }
    }

    pub fn register(&mut self, id: String, label: String, position: V) -> usize {
        let index = self.objects.len();
        self.objects.push(LifecycleObject::new(id, label, position));
        index
    }

    pub fn promote(&mut self, index: usize, target: LifecycleState) -> bool {
        if let Some(obj) = self.objects.get_mut(index) {
            obj.promote(target, &mut self.current_generation)
        } else {
            false
        }
    }

    pub fn promote_by_id(&mut self, id: &str, target: LifecycleState) -> bool {
        if let Some(obj) = self.objects.iter_mut().find(|o| o.id == id) {
            obj.promote(target, &mut self.current_generation)
        } else {
            false
        }
    }

    /// Count objects in each lifecycle tier.
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let mut static_inst = 0;
        let mut interactive = 0;
        let mut dynamic = 0;
        let mut replicated = 0;
        for o in &self.objects {
            match o.state {
                LifecycleState::StaticInstance => static_inst += 1,
                LifecycleState::InteractiveStatic => interactive += 1,
                LifecycleState::DynamicEntity => dynamic += 1,
                LifecycleState::ReplicatedEntity => replicated += 1,
            }
        }
        (static_inst, interactive, dynamic, replicated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_by_default_and_promotes_on_demand() {
        let mut registry = LifecycleRegistry::new();
        let idx = registry.register("can-1".into(), "Soda Can".into(), V(1.0, 1.0, 1.0));
        assert_eq!(registry.objects[idx].state, LifecycleState::StaticInstance);
        assert_eq!(registry.objects[idx].state.cost_tier(), "CHEAP");

        let gen_before = registry.current_generation;
        assert!(registry.promote(idx, LifecycleState::DynamicEntity));
        assert_eq!(registry.objects[idx].state, LifecycleState::DynamicEntity);
        assert!(registry.current_generation > gen_before);
    }

    #[test]
    fn settles_and_demotes_after_stationary_rest() {
        let mut reg = LifecycleRegistry::new();
        let idx = reg.register("chair-1".into(), "Chair".into(), V::ZERO);
        reg.promote(idx, LifecycleState::DynamicEntity);
        assert_eq!(reg.objects[idx].state, LifecycleState::DynamicEntity);

        let mut gen = reg.current_generation;
        let obj = &mut reg.objects[idx];
        assert!(!obj.update_rest(1.0, 0.0, &mut gen));
        assert!(!obj.update_rest(1.5, 0.0, &mut gen));
        assert!(obj.update_rest(1.0, 0.0, &mut gen)); // 3.5s total > 3.0s threshold
        assert_eq!(obj.state, LifecycleState::InteractiveStatic);
    }
}
