//! Semantic actions independent of input devices and rendering.
use super::room::Room;
use crate::math::Ray;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    ToggleMonitor,
    CycleCrystal,
    Inspect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feedback {
    pub title: String,
    pub description: &'static str,
}

#[derive(Clone, Debug)]
pub struct Interactions {
    pub monitor_on: bool,
    pub crystal_amber: bool,
    pub feedback: Option<Feedback>,
    pub remaining: f32,
}
impl Default for Interactions {
    fn default() -> Self {
        Self {
            monitor_on: true,
            crystal_amber: false,
            feedback: None,
            remaining: 0.,
        }
    }
}
impl Interactions {
    pub fn prompt(&self, action: Action) -> &'static str {
        match action {
            Action::ToggleMonitor => {
                if self.monitor_on {
                    "Switch monitor off"
                } else {
                    "Switch monitor on"
                }
            }
            Action::CycleCrystal => {
                if self.crystal_amber {
                    "Set crystal to blue"
                } else {
                    "Set crystal to amber"
                }
            }
            Action::Inspect => "Inspect",
        }
    }
    pub fn tick(&mut self, dt: f32) {
        if dt.is_finite() {
            self.remaining = (self.remaining - dt.max(0.)).max(0.);
        }
        if self.remaining == 0. {
            self.feedback = None;
        }
    }
    pub fn dismiss(&mut self) {
        self.feedback = None;
        self.remaining = 0.;
    }
    /// Re-evaluate the ray on activation, so stale UI focus cannot activate an object.
    pub fn activate(&mut self, room: &Room, ray: Ray) -> bool {
        let Some(entity) = room.focus(ray) else {
            return false;
        };
        let description = match entity.action {
            Action::ToggleMonitor => {
                self.monitor_on = !self.monitor_on;
                if self.monitor_on {
                    "Terminal powered on. The studio is ready."
                } else {
                    "Terminal powered off. Use it again to switch it on."
                }
            }
            Action::CycleCrystal => {
                self.crystal_amber = !self.crystal_amber;
                if self.crystal_amber {
                    "Amber finish selected. Use again for blue."
                } else {
                    "Blue finish selected. Use again for amber."
                }
            }
            Action::Inspect => match entity.id.as_str() {
                "composition-01" => "Composition / 01. A study in blue, brass, and balance.",
                "notebook" => "Studio notes: explore, observe, and make something new.",
                "vesper-robot" => "Little explorer. A familiar face from the Vesper3D engine.",
                "entry" => "The studio entrance is closed in this room demo.",
                "lounge" => "The lounge: a quiet corner to take in the studio.",
                "workbench" => "Design workbench. Aim at the monitor to switch it on or off.",
                "table" => "Reading table. The blue notebook contains a studio note.",
                _ => "A piece from the Blue Engine studio collection.",
            },
        };
        self.feedback = Some(Feedback {
            title: entity.label.clone(),
            description,
        });
        self.remaining = 6.;
        true
    }
}

/// E and left-click are equivalent press edges. A resume click is never an action.
pub fn activation_requested(
    active: bool,
    capture_settled: bool,
    e_pressed: bool,
    click_pressed: bool,
) -> bool {
    active && capture_settled && (e_pressed || click_pressed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{math::V, viewer::room};
    fn monitor_ray() -> Ray {
        Ray {
            o: V(-3.3, 1.34, -1.6),
            d: V(-1., 0., 0.),
        }
    }
    #[test]
    fn input_edges_are_equivalent_and_pause_blocks_actions() {
        assert!(activation_requested(true, true, true, false));
        assert!(activation_requested(true, true, false, true));
        assert!(activation_requested(true, true, true, true));
        assert!(!activation_requested(true, true, false, false));
        assert!(!activation_requested(false, true, true, true));
        assert!(!activation_requested(true, false, true, true));
    }
    #[test]
    fn monitor_toggles_and_prompt_tracks_state() {
        let room = room::build().unwrap();
        let mut state = Interactions::default();
        assert_eq!(room.focus(monitor_ray()).unwrap().id, "monitor");
        assert!(state.activate(&room, monitor_ray()));
        assert!(!state.monitor_on);
        assert_eq!(state.prompt(Action::ToggleMonitor), "Switch monitor on");
        state.activate(&room, monitor_ray());
        assert!(state.monitor_on);
    }
    #[test]
    fn crystal_changes_without_changing_other_objects() {
        let room = room::build().unwrap();
        let tags = room.render_tags();
        assert!(
            !tags
                .iter()
                .any(|(bounds, tag)| *tag == 2. && bounds.contains(V(-1.65, 1.0, -2.8))),
            "The pedestal must not change colour with the crystal"
        );
        let mut state = Interactions::default();
        let ray = Ray {
            o: V(-1.65, 1.55, 0.),
            d: V(0., 0., -1.),
        };
        assert_eq!(room.focus(ray).unwrap().id, "vesper-crystal");
        state.activate(&room, ray);
        assert!(state.crystal_amber && state.monitor_on);
        state.activate(&room, ray);
        assert!(!state.crystal_amber);
    }
    #[test]
    fn out_of_range_and_occluded_actions_do_nothing() {
        let room = room::build().unwrap();
        let mut state = Interactions::default();
        for ray in [
            Ray {
                o: V(-1.65, 1.55, 5.),
                d: V(0., 0., -1.),
            },
            Ray {
                o: V(-7., 1.34, -1.6),
                d: V(1., 0., 0.),
            },
            Ray {
                o: V(0., 1.68, 4.6),
                d: V(0., 1., 0.),
            },
        ] {
            assert!(!state.activate(&room, ray));
            assert!(state.monitor_on && !state.crystal_amber);
            assert!(state.feedback.is_none());
        }
    }
    #[test]
    fn inspection_and_feedback_expiry() {
        let room = room::build().unwrap();
        let mut state = Interactions::default();
        let ray = Ray {
            o: V(-1.8, 1.2, 4.3),
            d: V(0., 0., 1.),
        };
        assert!(state.activate(&room, ray));
        assert!(state
            .feedback
            .as_ref()
            .unwrap()
            .description
            .contains("closed"));
        state.tick(3.);
        assert!(state.feedback.is_some());
        state.tick(3.);
        assert!(state.feedback.is_none());
        state.activate(&room, ray);
        state.dismiss();
        assert!(state.feedback.is_none());
    }
}
