//! Policy for the stock Scientist/Feta demonstration, not a new-game combat foundation.
//! Old public weapon modules and HeadlessWorld attack methods remain narrow protocol adapters.
pub mod weapons;
pub mod wrench;

use crate::viewer::simulation::HeadlessWorld;
impl HeadlessWorld {
    /// Stock-protocol compatibility: pistol range/impulse belong to the demo.
    pub fn fire_pistol(&mut self, id: u64) -> Option<crate::math::V> {
        self.attack_props(id, weapons::PISTOL_RANGE, 6.0, false)
    }
    /// Stock-protocol compatibility: wrench releases held props before applying impulse.
    pub fn fire_wrench(&mut self, id: u64) -> Option<crate::math::V> {
        self.attack_props(id, wrench::REACH, 12.0, true)
    }
}
