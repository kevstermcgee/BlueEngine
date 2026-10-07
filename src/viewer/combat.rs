//! Shared rendering-independent fixed-tick attack timing. Games own weapon policy.
/// Deterministic countdown in whole simulation ticks (inspired by RedEngine).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TickCooldown(pub u32);

impl TickCooldown {
    pub const fn new() -> Self {
        Self(0)
    }

    pub fn start(&mut self, ticks: u32) {
        self.0 = ticks;
    }

    pub fn tick(&mut self) {
        self.0 = self.0.saturating_sub(1);
    }

    pub fn ready(&self) -> bool {
        self.0 == 0
    }
}

/// Deterministic melee swing state machine in fixed ticks: windup -> strike -> recover.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MeleeSwingState {
    ticks: Option<u32>,
    struck: bool,
    windup_ticks: u32,
    total_ticks: u32,
}

impl MeleeSwingState {
    pub fn new(windup_ticks: u32, total_ticks: u32) -> Self {
        Self {
            ticks: None,
            struck: false,
            windup_ticks,
            total_ticks,
        }
    }

    pub fn is_idle(&self) -> bool {
        self.ticks.is_none()
    }

    pub fn start(&mut self) -> bool {
        if self.ticks.is_some() {
            return false;
        }
        self.ticks = Some(0);
        self.struck = false;
        true
    }

    pub fn cancel(&mut self) {
        self.ticks = None;
    }

    /// Advance one tick. Returns true on the exact tick where the strike lands.
    pub fn tick(&mut self) -> bool {
        let Some(t) = self.ticks else { return false };
        let t = t + 1;
        let strike = t >= self.windup_ticks && !self.struck;
        self.struck |= strike;
        self.ticks = if t >= self.total_ticks { None } else { Some(t) };
        strike
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tick_combat_timing_and_melee_windup_strike() {
        let mut cooldown = TickCooldown::new();
        assert!(cooldown.ready());
        cooldown.start(3);
        assert!(!cooldown.ready());
        cooldown.tick();
        assert!(!cooldown.ready());
        cooldown.tick();
        assert!(!cooldown.ready());
        cooldown.tick();
        assert!(cooldown.ready());

        let mut swing = MeleeSwingState::new(2, 5);
        assert!(swing.is_idle());
        assert!(swing.start());
        assert!(!swing.is_idle());
        assert!(!swing.tick()); // tick 1: windup
        assert!(swing.tick()); // tick 2: strike lands!
        assert!(!swing.tick()); // tick 3: recover
        assert!(!swing.tick()); // tick 4: recover
        assert!(!swing.tick()); // tick 5: end of swing
        assert!(swing.is_idle());
    }
}
