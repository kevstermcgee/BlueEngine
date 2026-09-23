//! Rendering-free simulation shared by the client and future PulseNet adapter.
use super::{
    controller::{Collider, Controller, Movement},
    room::Room,
};
use std::collections::BTreeMap;

pub const TICK_SECONDS: f32 = 1. / 60.;
const MAX_STEPS: usize = 8;

#[derive(Default)]
pub struct PlayerStepper {
    remainder: f64,
    previous: Controller,
    jump: bool,
}
impl PlayerStepper {
    pub fn reset(&mut self, current: &Controller) {
        self.remainder = 0.;
        self.previous = current.clone();
        self.jump = false;
    }
    pub fn advance(
        &mut self,
        current: &mut Controller,
        mut input: Movement,
        dt: f32,
        colliders: &[Collider],
    ) -> usize {
        if !dt.is_finite() || dt <= 0. {
            return 0;
        }
        self.jump |= input.jump;
        const STEP: f64 = 1. / 60.;
        self.remainder = (self.remainder + f64::from(dt)).min(STEP * MAX_STEPS as f64);
        let mut steps = 0;
        while self.remainder + 1e-9 >= STEP && steps < MAX_STEPS {
            self.previous = current.clone();
            input.jump = std::mem::take(&mut self.jump);
            current.update(input, TICK_SECONDS, colliders);
            self.remainder = (self.remainder - STEP).max(0.);
            steps += 1;
        }
        steps
    }
    pub fn pose(&self, current: &Controller) -> Controller {
        current.interpolated(&self.previous, (self.remainder * 60.) as f32)
    }
}

pub struct Player {
    pub controller: Controller,
    input: Movement,
}
/// Match-local world, bounded to eight players. No socket, renderer or window.
pub struct HeadlessWorld {
    pub room: Room,
    players: BTreeMap<u64, Player>,
    pub tick: u64,
}
impl HeadlessWorld {
    pub fn new() -> crate::Result<Self> {
        Ok(Self::with_room(super::maps::build(
            super::maps::MapId::House,
        )?))
    }
    pub fn with_room(room: Room) -> Self {
        Self {
            room,
            players: BTreeMap::new(),
            tick: 0,
        }
    }
    pub fn join(&mut self, id: u64) -> bool {
        if self.players.len() >= 8 || self.players.contains_key(&id) {
            return false;
        }
        self.players.insert(
            id,
            Player {
                controller: Controller::default(),
                input: Movement::default(),
            },
        );
        true
    }
    pub fn leave(&mut self, id: u64) {
        self.players.remove(&id);
    }
    pub fn player(&self, id: u64) -> Option<&Controller> {
        self.players.get(&id).map(|p| &p.controller)
    }
    /// Only bounded movement intent is accepted; clients cannot assign positions.
    pub fn input(&mut self, id: u64, mut input: Movement, yaw: f32, pitch: f32) -> bool {
        if !input.forward.is_finite()
            || !input.right.is_finite()
            || !yaw.is_finite()
            || !pitch.is_finite()
        {
            return false;
        }
        let Some(player) = self.players.get_mut(&id) else {
            return false;
        };
        input.forward = input.forward.clamp(-1., 1.);
        input.right = input.right.clamp(-1., 1.);
        input.jump |= player.input.jump;
        player.input = input;
        player.controller.yaw = yaw.rem_euclid(std::f32::consts::TAU);
        player.controller.pitch = pitch.clamp(-1.5, 1.5);
        true
    }
    pub fn step(&mut self) {
        for player in self.players.values_mut() {
            player
                .controller
                .update(player.input, TICK_SECONDS, &self.room.colliders);
            player.input.jump = false;
        }
        self.tick += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn render_rates_produce_same_simulation() {
        let mut end = Vec::new();
        for hz in [30, 60, 144, 240] {
            let mut c = Controller::default();
            let mut stepper = PlayerStepper::default();
            let mut count = 0;
            for i in 0..hz {
                count += stepper.advance(
                    &mut c,
                    Movement {
                        forward: 1.,
                        jump: i == 0,
                        ..Default::default()
                    },
                    1. / hz as f32,
                    &[],
                );
            }
            assert_eq!(count, 60);
            end.push(c.position);
        }
        for p in &end {
            assert!((*p - end[0]).length() < 0.00001);
        }
    }
    #[test]
    fn jump_survives_subtick_and_stalls_are_bounded() {
        let mut c = Controller::default();
        let mut s = PlayerStepper::default();
        assert_eq!(
            s.advance(
                &mut c,
                Movement {
                    jump: true,
                    ..Default::default()
                },
                0.001,
                &[]
            ),
            0
        );
        s.advance(&mut c, Movement::default(), 0.02, &[]);
        assert!(!c.is_grounded());
        assert_eq!(s.advance(&mut c, Movement::default(), 10., &[]), MAX_STEPS);
        s.reset(&c);
        assert_eq!(s.pose(&c).position, c.position);
    }
    #[test]
    fn headless_bounds_input_and_matches_client() {
        let mut world = HeadlessWorld::new().unwrap();
        for id in 0..8 {
            assert!(world.join(id));
        }
        assert!(!world.join(8));
        assert!(!world.join(0));
        assert!(!world.input(0, Movement::default(), f32::NAN, 0.));
        let mut client = Controller::default();
        let input = Movement {
            right: 1.,
            ..Default::default()
        };
        world.input(0, input, client.yaw, client.pitch);
        for _ in 0..60 {
            world.step();
            client.update(input, TICK_SECONDS, &world.room.colliders);
        }
        assert!((world.player(0).unwrap().position - client.position).length() < 0.00001);
        world.leave(0);
        assert!(world.join(8));
    }
}
