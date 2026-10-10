//! Rendering-free lifecycle and interface actions. Appearance belongs to the game.
use super::{Point, Rect};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Screen {
    Start,
    Playing,
    Paused,
    Won,
    Lost,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Start,
    Resume,
    TogglePause,
    Restart,
    Save,
    Load,
    ToggleSound,
    ToggleMusic,
    Quit,
}
#[derive(Default)]
pub struct Layout {
    buttons: Vec<(Rect, Action)>,
}
impl Layout {
    /// Hit regions use the same logical canvas coordinates as Scene; insertion order is navigation order.
    pub fn button(&mut self, bounds: Rect, action: Action) {
        self.buttons.push((bounds, action));
    }
    pub fn hit(&self, pointer: Point) -> Option<Action> {
        self.buttons
            .iter()
            .rev()
            .find(|(r, _)| r.contains(pointer))
            .map(|(_, a)| *a)
    }
    pub fn selected(&self, index: usize) -> Option<Action> {
        self.buttons.get(index).map(|(_, a)| *a)
    }
    pub fn len(&self) -> usize {
        self.buttons.len()
    }
    pub fn is_empty(&self) -> bool {
        self.buttons.is_empty()
    }
}

/// The client is the sole owner of this state. Games receive a read-only frame and return hit regions.
pub struct Lifecycle {
    started: bool,
    paused: bool,
    focused: bool,
}
impl Lifecycle {
    pub fn new(started: bool) -> Self {
        Self {
            started,
            paused: false,
            focused: true,
        }
    }
    pub fn focus(&mut self, focused: bool) {
        if self.focused && !focused && self.started {
            self.paused = true;
        }
        self.focused = focused;
    }
    pub fn screen(&self, outcome: &str) -> Screen {
        if !self.started {
            Screen::Start
        } else if self.paused {
            Screen::Paused
        } else if outcome == "won" {
            Screen::Won
        } else if outcome != "playing" {
            Screen::Lost
        } else {
            Screen::Playing
        }
    }
    pub fn accepting_input(&self, outcome: &str) -> bool {
        self.focused && self.screen(outcome) == Screen::Playing
    }
    pub fn started(&self) -> bool {
        self.started
    }
    /// Returns whether the action is allowed. The client handles storage/settings/quit and clears pending input.
    pub fn apply(&mut self, action: Action, outcome: &str) -> bool {
        if !self.focused {
            return false;
        }
        match action {
            Action::Start if !self.started => {
                self.started = true;
                self.paused = false;
            }
            Action::Resume if self.paused => self.paused = false,
            Action::TogglePause if self.started && outcome == "playing" => {
                self.paused = !self.paused
            }
            Action::Restart => {
                self.started = true;
                self.paused = false;
            }
            Action::Save
            | Action::Load
            | Action::ToggleSound
            | Action::ToggleMusic
            | Action::Quit => {}
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn different_layouts_share_start_pause_focus_restart_and_outcome_behavior() {
        for rect in [Rect::new(35, 75, 200, 40), Rect::new(530, 310, 180, 70)] {
            let mut ui = Lifecycle::new(false);
            let mut layout = Layout::default();
            layout.button(rect, Action::Start);
            assert!(!ui.accepting_input("playing"));
            assert!(ui.apply(
                layout.hit(Point::new(rect.x + 1, rect.y + 1)).unwrap(),
                "playing"
            ));
            assert!(ui.accepting_input("playing"));
            assert!(!ui.apply(Action::Start, "playing"));
            assert!(ui.apply(Action::TogglePause, "playing"));
            assert_eq!(ui.screen("playing"), Screen::Paused);
            assert!(!ui.accepting_input("playing"));
            ui.focus(false);
            assert!(!ui.apply(Action::Resume, "playing"));
            ui.focus(true);
            assert_eq!(ui.screen("playing"), Screen::Paused);
            assert!(ui.apply(Action::Resume, "playing"));
            assert!(!ui.accepting_input("won"));
            assert_eq!(ui.screen("won"), Screen::Won);
            assert!(ui.apply(Action::Restart, "won"));
            assert!(ui.accepting_input("playing"));
            assert_eq!(ui.screen("lost"), Screen::Lost);
            ui.focus(false);
            ui.focus(true);
            assert_eq!(ui.screen("playing"), Screen::Paused);
        }
    }
}
