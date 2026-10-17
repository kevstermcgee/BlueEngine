//! Render-frame controls carried to the next authoritative fixed tick.
use crate::{Intent, Pulse};
use vesper3d::viewer::devkit::{Lifecycle, Tick};

pub const TAB: u32 = 1;
pub const SPREAD: u32 = 2;
pub const GATHER: u32 = 4;
pub const EXEC: u32 = 8;
pub const UNDO: u32 = 16;
pub const RESET: u32 = 32;
const SELECT: u32 = 64;

#[derive(Default)]
pub struct Controls {
    beacon: usize,
}

impl Controls {
    pub fn feed(&mut self, life: &mut Lifecycle<usize>, edges: u32, beacon: Option<usize>) {
        // Lifecycle retains edges across zero-tick frames, but replaces held data every frame.
        // Keep the selection payload until another selection replaces it; only SELECT applies it.
        if let Some(beacon) = beacon {
            self.beacon = beacon;
        }
        life.feed(self.beacon, edges | if beacon.is_some() { SELECT } else { 0 }, [0.; 2]);
    }
}

pub fn intent(tick: Tick<usize>) -> Intent {
    Intent {
        beacon: if tick.pressed(SELECT) { Some(tick.held) } else { None },
        cycle: tick.pressed(TAB),
        mode: if tick.pressed(SPREAD) {
            Some(Pulse::Spread)
        } else if tick.pressed(GATHER) {
            Some(Pulse::Gather)
        } else {
            None
        },
        execute: tick.pressed(EXEC),
        next: tick.pressed(EXEC),
        undo: tick.pressed(UNDO),
        restart: tick.pressed(RESET),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sim;

    fn lifecycle() -> Lifecycle<usize> {
        Lifecycle::start(&["Scatterlock".into(), "--seed".into(), "3".into()], &[]).unwrap()
    }

    #[test]
    fn beacon_selection_survives_rendered_frames_without_a_tick() {
        for beacon in 0..5 {
            let (mut controls, mut life, mut sim) = (Controls::default(), lifecycle(), Sim::at_level(5));
            // A click, number key, or script cue arrives on a 240 Hz frame.
            controls.feed(&mut life, 0, Some(beacon));
            assert_eq!(life.ticks(1. / 240., 1., true), 0);
            // The release and subsequent quiet frames must not replace its payload with A.
            for _ in 0..2 {
                controls.feed(&mut life, 0, None);
                assert_eq!(life.ticks(1. / 240., 1., true), 0);
            }
            controls.feed(&mut life, GATHER, None);
            assert_eq!(life.ticks(1. / 240., 1., true), 1);
            sim.step(&intent(life.take_tick()));
            assert_eq!(sim.state.selected, beacon);
            assert_eq!(sim.state.mode, Pulse::Gather);
            assert_eq!(intent(life.take_tick()), Intent::default(), "consume selection exactly once");
        }
    }

    #[test]
    fn most_recent_selection_wins_and_catch_up_does_not_repeat_pulses() {
        let (mut controls, mut life, mut sim) = (Controls::default(), lifecycle(), Sim::new(3));
        controls.feed(&mut life, 0, Some(0));
        controls.feed(&mut life, 0, Some(1));
        controls.feed(&mut life, GATHER | EXEC, None);
        assert_eq!(life.ticks(1. / 30., 1., true), 2);
        let first = intent(life.take_tick());
        assert_eq!(first.beacon, Some(1));
        sim.step(&first);
        let after_pulse = sim.state.config.clone();
        let second = intent(life.take_tick());
        assert_eq!(second, Intent::default());
        sim.step(&second);
        assert_eq!(sim.state.config, after_pulse);
    }
}
