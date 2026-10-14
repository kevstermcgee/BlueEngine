//! Saves use the existing engine frame and atomic slot writer. No custom save file format.
use crate::{State, Workshop};
use vesper3d::runtime::Snapshot;
impl Snapshot for Workshop {
    const KIND: &'static str = "blueengine-workshop";
    const VERSION: u32 = 1;
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, state: State) -> Result<(), String> {
        self.validate_state(&state)?;
        self.state = state;
        self.recompute_blocked();
        Ok(())
    }
    fn content(&self) -> Option<u64> {
        Some(self.definitions.content_hash())
    }
    fn save_tick(&self) -> u64 {
        self.state.tick
    }
}
