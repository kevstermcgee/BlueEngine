//! Native events become the same public click intentions used by verification.
//! No board changes happen here; Simulation consumes the resulting intentions.
use super::*;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug)]
pub(crate) enum NativeCommand {
    Target(Point),
    Select(usize),
    Cycle,
}
impl NativeCommand {
    pub fn pointer(cells: &[Cell], point: Point) -> Option<Self> {
        if let Some(j) = crate_at(cells, point) {
            // Identify the crate on the displayed board, before queued moves.
            return Some(Self::Select(j));
        }
        // Synthetic targets must be inside the button even if the original
        // physical click was on its final pixel and is dispatched a frame later.
        BUTTONS
            .iter()
            .chain([&UNDO, &HINT, &NEXT])
            .find(|r| r.contains(point))
            .map(|&r| Self::Target(click(r).pointer.unwrap()))
    }
}

#[derive(Default)]
pub(crate) struct NativeInput {
    pending: VecDeque<NativeCommand>,
    issued_at: Option<u32>,
}

impl NativeInput {
    pub fn emit(
        &mut self,
        tick: u32,
        cells: &[Cell],
        selected: usize,
        pointer: Option<Point>,
        commands: impl IntoIterator<Item = NativeCommand>,
    ) -> Intent {
        // The shared client accumulates one action edge and one target. Keep
        // offering the SAME front command over zero-tick display frames; only
        // a changed simulation tick acknowledges its consumption. A multi-tick
        // frame still consumes its action edge just once.
        if self.issued_at.is_some_and(|sent| sent != tick) {
            self.pending.pop_front();
            self.issued_at = None;
        }
        self.pending.extend(commands);
        let idle = Intent {
            pointer,
            ..Intent::default()
        };
        let intent = match self.pending.front() {
            Some(NativeCommand::Target(p)) => keyboard_target(idle, *p),
            Some(NativeCommand::Select(j)) => keyboard_target(idle, center(cells[*j])),
            Some(NativeCommand::Cycle) => {
                keyboard_target(idle, center(cells[(selected + 1) % cells.len()]))
            }
            None => idle,
        };
        if intent.action {
            self.issued_at = Some(tick);
        }
        intent
    }
}

impl Clutch {
    pub(crate) fn native_input(
        &mut self,
        physical: Intent,
        commands: impl IntoIterator<Item = NativeCommand>,
    ) -> Intent {
        self.native.emit(
            self.state.tick,
            &self.state.cells,
            self.state.selected,
            physical.pointer,
            commands,
        )
    }
}
