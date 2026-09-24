//! Validated, rendering-independent locomotion parameters.
use crate::{math::V, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerProfile {
    pub height: f32,
    pub crouched_height: f32,
    pub radius: f32,
    pub eye_height: f32,
    pub walk_speed: f32,
    pub sprint_speed: f32,
    pub crouch_speed: f32,
    pub jump_height: f32,
}
impl Default for ControllerProfile {
    fn default() -> Self {
        Self {
            height: 1.8,
            crouched_height: 1.1,
            radius: 0.23,
            eye_height: 1.68,
            walk_speed: 3.2,
            sprint_speed: 5.6,
            crouch_speed: 1.3,
            jump_height: 0.35,
        }
    }
}
impl ControllerProfile {
    pub fn validate(&self) -> Result<()> {
        let values = [
            self.height,
            self.crouched_height,
            self.radius,
            self.eye_height,
            self.walk_speed,
            self.sprint_speed,
            self.crouch_speed,
            self.jump_height,
        ];
        if values.iter().any(|v| !v.is_finite())
            || !(0.1..=3.).contains(&self.height)
            || !(0.1..=self.height).contains(&self.crouched_height)
            || !(0.05..=1.).contains(&self.radius)
            || !(0.05..self.height).contains(&self.eye_height)
            || self.height - self.eye_height >= self.crouched_height
            || [self.walk_speed, self.sprint_speed, self.crouch_speed]
                .iter()
                .any(|v| !(0.1..=20.).contains(v))
            || !(0.0..=3.).contains(&self.jump_height)
        {
            return Err("Invalid controller profile dimensions, eye clearance or speeds".into());
        }
        Ok(())
    }
    pub fn eye_at(&self, feet: V) -> V {
        feet + V(0., self.eye_height, 0.)
    }
    pub(crate) fn head_margin(&self) -> f32 {
        self.height - self.eye_height
    }
}
