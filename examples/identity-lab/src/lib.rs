//! One rendering-free rule set; three clients differ only in presentation and device mapping.
use serde::{Deserialize, Serialize};
use vesper3d::two_d::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    pub tick: u32,
    pub lamps: u8,
    pub turns: u8,
    pub selected: u8,
    pub repeat: u8,
}
pub struct Lab<const STYLE: u8> {
    pub state: State,
    events: Vec<&'static str>,
}
impl<const S: u8> GameLogic for Lab<S> {
    const ID: &'static str = "identity-lab";
    const TITLE: &'static str = "Identity Lab";
    const CONTROLS:&'static str="Arrows select · Space or click flips a lamp and its neighbour · Light all four in four turns";
    const VERIFY_TICKS: u32 = 2;
    fn new(_: u64) -> Self {
        Self {
            state: State {
                tick: 0,
                lamps: 0,
                turns: 0,
                selected: 0,
                repeat: 0,
            },
            events: vec![],
        }
    }
    fn tick(&self) -> u32 {
        self.state.tick
    }
    fn outcome(&self) -> &'static str {
        if self.state.lamps == 15 {
            "won"
        } else if self.state.turns == 4 {
            "lost"
        } else {
            "playing"
        }
    }
    fn pointer_target_only_on_press() -> bool {
        true
    }
    fn verification_input(tick: u32) -> Intent {
        Intent {
            action: true,
            pointer: Some(Point::new(if tick == 0 { 0 } else { 2 }, -1)),
            ..Default::default()
        }
    }
    fn probe_input() -> Intent {
        Intent {
            action: true,
            ..Default::default()
        }
    }
    fn probe_success(&self) -> bool {
        self.state.lamps == 3
    }
    fn default_sounds() -> bool {
        false
    }
    fn audio_banks() -> &'static [AudioBankSpec] {
        match S {
            0 => &[AudioBankSpec {
                id: "identity",
                root: "assets/audio/notebook",
                music: false,
            }],
            1 => &[AudioBankSpec {
                id: "identity",
                root: "assets/audio/instrument",
                music: true,
            }],
            _ => &[AudioBankSpec {
                id: "identity",
                root: "assets/audio/arcade",
                music: true,
            }],
        }
    }
    fn audio_bindings() -> &'static [AudioBinding] {
        &[
            AudioBinding {
                event: "select",
                bank: "identity",
                cue: "tick",
                volume: 0.45,
            },
            AudioBinding {
                event: "confirm",
                bank: "identity",
                cue: "gesture",
                volume: 0.6,
            },
            AudioBinding {
                event: "invalid",
                bank: "identity",
                cue: "reject",
                volume: 0.55,
            },
            AudioBinding {
                event: "objective",
                bank: "identity",
                cue: "resolve",
                volume: 0.65,
            },
            AudioBinding {
                event: "ui.start",
                bank: "identity",
                cue: "open",
                volume: 0.55,
            },
            AudioBinding {
                event: "ui.pause",
                bank: "identity",
                cue: "open",
                volume: 0.35,
            },
            AudioBinding {
                event: "ui.resume",
                bank: "identity",
                cue: "open",
                volume: 0.35,
            },
            AudioBinding {
                event: "ui.restart",
                bank: "identity",
                cue: "open",
                volume: 0.45,
            },
        ]
    }
    fn audio_level(&self, _: &str, _: &str) -> f32 {
        if S == 2 {
            0.3
        } else {
            0.12
        }
    }
    fn take_audio_events(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.events)
    }
}
impl<const S: u8> Simulation for Lab<S> {
    type Input = Intent;
    fn step(&mut self, i: &Intent) {
        if self.outcome() != "playing" {
            return;
        }
        self.state.tick += 1;
        self.state.repeat = self.state.repeat.saturating_sub(1);
        let axis = if i.x != 0 { i.x } else { i.y };
        if axis != 0 && self.state.repeat == 0 {
            self.state.selected = (self.state.selected as i32 + axis.signum()).rem_euclid(4) as u8;
            self.state.repeat = 8;
            self.events.push("select");
        }
        if i.action {
            if let Some(p) = i.pointer {
                if p.y != -1 || !(0..4).contains(&p.x) {
                    self.events.push("invalid");
                    return;
                }
                self.state.selected = p.x as u8;
            }
            let n = self.state.selected;
            self.state.lamps ^= (1 << n) | (1 << ((n + 1) % 4));
            self.state.turns += 1;
            self.events.push("confirm");
            if self.outcome() == "won" {
                self.events.push("objective");
            } else if self.outcome() == "lost" {
                self.events.push("invalid");
            }
        }
    }
    fn state_hash(&self) -> u64 {
        hash_json(&self.state)
    }
}
impl<const S: u8> Snapshot for Lab<S> {
    const KIND: &'static str = "identity-lab";
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, state: State) -> Result<(), String> {
        if state.lamps > 15 || state.turns > 4 || state.selected > 3 || state.repeat > 8 {
            return Err("Invalid switch puzzle save".into());
        }
        self.state = state;
        self.events.clear();
        Ok(())
    }
}
#[cfg(feature = "client")]
mod presentation;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_presentations_have_identical_rules_events_and_save_continuation() {
        let route = vec![
            Lab::<0>::verification_input(0),
            Lab::<0>::verification_input(1),
        ];
        vesper3d::runtime::assert_deterministic(|| Lab::<0>::new(7), &route);
        vesper3d::runtime::snapshot::assert_resumes_exactly(|| Lab::<0>::new(7), &route, 1);
        assert_eq!(verify::<Lab<0>>(), verify::<Lab<1>>());
        assert_eq!(verify::<Lab<1>>(), verify::<Lab<2>>());
        assert_eq!(verify::<Lab<0>>().1, "won");
        let mut game = Lab::<0>::new(7);
        game.step(&route[0]);
        assert_eq!(game.take_audio_events(), ["confirm"]);
        game.step(&route[1]);
        assert_eq!(game.take_audio_events(), ["confirm", "objective"]);
    }
    #[test]
    fn public_input_loss_restart_and_keyboard_match_pointer_rules() {
        let mut game = Lab::<0>::new(7);
        for _ in 0..4 {
            game.step(&Intent {
                action: true,
                ..Default::default()
            });
        }
        assert_eq!(game.outcome(), "lost");
        let hash = game.state_hash();
        game.step(&Intent::default());
        assert_eq!(game.state_hash(), hash);
        game.restart();
        assert_eq!(game.outcome(), "playing");
        game.step(&Intent {
            action: true,
            ..Default::default()
        });
        game.step(&Intent {
            x: 1,
            ..Default::default()
        });
        for _ in 0..8 {
            game.step(&Intent::default());
        }
        game.step(&Intent {
            x: 1,
            ..Default::default()
        });
        game.step(&Intent {
            action: true,
            ..Default::default()
        });
        assert_eq!(game.outcome(), "won");
    }
}
