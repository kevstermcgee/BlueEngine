//! Rendering-free mapping of positional controller commands into public intentions.
use vesper3d::two_d::{Intent, Point};
#[derive(Default)]
pub struct Pad {
    pub up: bool,
    pub clockwise: bool,
    pub reverse: bool,
    pub hold: bool,
    pub drop: bool,
    pub mode: bool,
}
pub fn map(mut input: Intent, pad: Pad, mode: u8) -> Intent {
    if pad.up {
        input.y = 0;
    }
    let command = if pad.mode {
        Some(10 + ((mode + 1) % 3) as i32)
    } else if pad.hold {
        Some(4)
    } else if pad.reverse {
        Some(3)
    } else if pad.clockwise {
        Some(2)
    } else {
        None
    };
    if let Some(c) = command {
        input.action = true;
        input.pointer = Some(Point::new(-100, c));
    } else if pad.drop {
        input.action = true;
        input.pointer = None;
    }
    input
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameLogic, Simulation, Tetris};
    #[test]
    fn controller_commands_use_the_same_rules_and_do_not_repeat() {
        let mut g = Tetris::new(7);
        let initial = g.state.active.rotation;
        g.step(&map(
            Intent::default(),
            Pad {
                clockwise: true,
                ..Default::default()
            },
            0,
        ));
        assert_eq!(g.state.active.rotation, (initial + 1) % 4);
        g.step(&map(Intent::default(), Pad::default(), 0));
        assert_eq!(g.state.active.rotation, (initial + 1) % 4);
        g.step(&map(
            Intent::default(),
            Pad {
                reverse: true,
                ..Default::default()
            },
            0,
        ));
        assert_eq!(g.state.active.rotation, initial);
        g.step(&map(
            Intent::default(),
            Pad {
                hold: true,
                ..Default::default()
            },
            0,
        ));
        assert!(g.state.hold.is_some());
        g.step(&map(
            Intent {
                y: -1,
                ..Default::default()
            },
            Pad {
                up: true,
                drop: true,
                ..Default::default()
            },
            0,
        ));
        assert_eq!(g.state.pieces, 1);
        g.step(&map(
            Intent::default(),
            Pad {
                mode: true,
                ..Default::default()
            },
            0,
        ));
        assert_eq!(g.state.mode, 1);
    }
    #[test]
    fn held_dpad_and_keyboard_axes_remain_simulation_inputs() {
        let mut g = Tetris::new(7);
        let x = g.state.active.x;
        for _ in 0..12 {
            g.step(&map(
                Intent {
                    x: -1,
                    y: 1,
                    ..Default::default()
                },
                Pad::default(),
                0,
            ));
        }
        assert!(g.state.active.x < x);
        assert!(g.state.score > 0);
        let i = map(
            Intent {
                y: -1,
                ..Default::default()
            },
            Pad {
                up: true,
                ..Default::default()
            },
            0,
        );
        assert_eq!(i.y, 0);
        assert!(!i.action);
    }
}
