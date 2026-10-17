//! Native lifecycle policy. Device observations stay outside puzzle authority.
#[derive(Default)]
pub struct FocusGate {
    blocked: bool,
    drained: bool,
}
impl FocusGate {
    /// `lost` includes a focus-out event followed by focus-in between frames.
    pub fn observe(&mut self, focused: bool, lost: bool, neutral: bool) {
        if !focused || lost {
            self.blocked = true;
            self.drained = false;
        } else if self.blocked && self.drained && neutral {
            self.blocked = false;
        }
    }
    pub fn active(&self) -> bool {
        !self.blocked
    }
    /// Consume a pending command without advancing puzzle time while blocked.
    pub fn step(&mut self, game: &mut crate::Afterpull, input: &crate::Intent) {
        if self.accept_tick() {
            crate::Simulation::step(game, input);
        } else {
            game.state.preview = None;
        }
    }
    /// Consume at least one accumulator tick before accepting fresh edges.
    fn accept_tick(&mut self) -> bool {
        if self.blocked {
            self.drained = true;
            false
        } else {
            true
        }
    }
}
