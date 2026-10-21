use crosshaul::Sim;

/// Presentation evidence captured immediately before an animation's final tick.
/// Endpoint positions already belong to the authority while cargo is in flight.
pub struct DockingArrival {
    chamber: usize,
    used: u8,
    from_docked: usize,
}

impl DockingArrival {
    pub fn finishing(sim: &Sim) -> Option<Self> {
        let motion = sim.motion.as_ref().filter(|motion| motion.remaining == 1)?;
        Some(Self {
            chamber: sim.chamber,
            used: sim.used,
            from_docked: motion.from.iter().zip(&sim.level().docks).filter(|(a, b)| a == b).count(),
        })
    }

    pub fn completed(self, sim: &Sim) -> bool {
        sim.motion.is_none() && sim.chamber == self.chamber && sim.used == self.used && sim.docked() > self.from_docked
    }
}
