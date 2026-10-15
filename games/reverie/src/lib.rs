//! Deterministic museum rules. Rendering never chooses key locations or opens doors.
pub mod collection;
use collection::{exhibit, room_seed, theme, Exhibit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use vesper3d::{
    math::V,
    viewer::{
        controller::{Collider, Controller, ControllerState, Movement},
        devkit::{path, SavePolicy, Simulation, Snapshot, StateHasher, TICK},
        profile::ControllerProfile,
    },
};
pub const ART_COUNT: usize = 6;
pub const ROOM_HALF: f32 = 6.;
pub const ROOM_LENGTH: f32 = 9.;
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Input {
    pub forward: f32,
    pub right: f32,
    pub look: [f32; 2],
    pub sprint: bool,
    pub interact: bool,
    pub inspect: bool,
    pub leave: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Footstep,
    Entered,
    Returned,
    KeyFound,
    DoorOpened,
    Locked,
    Awakened(usize),
    Inspected(Exhibit),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Art(usize),
    Door(u8),
    Parent,
    Nested,
    Return,
    Key,
}
/// Only exploration data accumulates. The client regenerates and caches just the current scene.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Archive {
    pub seed: u64,
    pub tick: u64,
    pub room: Vec<u8>,
    pub inside: Vec<u8>,
    pub returns: Vec<ControllerState>,
    pub visited: BTreeSet<Vec<u8>>,
    pub keys: BTreeSet<Vec<u8>>,
    pub awakened: BTreeSet<String>,
    pub worlds: BTreeSet<String>,
    pub footsteps: f32,
}
pub struct Sim {
    pub player: Controller,
    pub archive: Archive,
    events: Vec<Event>,
}
pub fn art_position(slot: usize) -> V {
    match slot {
        0 => V(0., 2.8, -8.7),
        1 => V(-5.72, 2.5, -2.8),
        2 => V(5.72, 2.5, -2.8),
        3 => V(-2.35, 1.6, 1.1),
        4 => V(2.35, 1.7, 1.1),
        _ => V(-2.4, 1.9, 6.3),
    }
}
pub fn door_position(branch: u8) -> V {
    V(if branch == 0 { -4.5 } else { 4.5 }, 1.65, -8.65)
}
pub fn route_name(route: &[u8]) -> String {
    if route.is_empty() {
        "Atrium".into()
    } else {
        route.iter().map(|b| if *b == 0 { 'L' } else { 'R' }).collect()
    }
}
impl Sim {
    pub fn new(seed: u64) -> Self {
        Self {
            player: Self::controller(V(0., 0., 7.5), 0.),
            archive: Archive {
                seed,
                tick: 0,
                room: vec![],
                inside: vec![],
                returns: vec![],
                visited: BTreeSet::from([vec![]]),
                keys: BTreeSet::new(),
                awakened: BTreeSet::new(),
                worlds: BTreeSet::new(),
                footsteps: 0.,
            },
            events: vec![],
        }
    }
    fn controller(feet: V, yaw: f32) -> Controller {
        Controller::for_profile(
            ControllerProfile { walk_speed: 2.7, sprint_speed: 4.6, jump_height: 0., ..Default::default() },
            feet,
            yaw,
        )
        .expect("museum profile")
    }
    pub fn seed(&self) -> u64 {
        room_seed(self.archive.seed, &self.archive.room)
    }
    pub fn world_seed(&self) -> u64 {
        room_seed(self.seed() ^ 0x8cb92baa3f3d8dd7, &self.archive.inside)
    }
    pub fn emotion(&self) -> usize {
        theme(self.seed())
    }
    pub fn world_emotion(&self) -> usize {
        if self.depth() <= 1 {
            self.emotion()
        } else {
            theme(room_seed(self.seed() ^ 0x8cb92baa3f3d8dd7, &self.archive.inside[..self.depth() - 1]))
        }
    }
    pub fn depth(&self) -> usize {
        self.archive.inside.len()
    }
    pub fn key_host(&self) -> usize {
        if self.archive.room.is_empty() {
            0
        } else {
            (self.seed() % ART_COUNT as u64) as usize
        }
    }
    pub fn key_depth(&self) -> usize {
        if self.archive.room.is_empty() {
            2
        } else {
            1 + (self.seed().rotate_left(13) % 3) as usize
        }
    }
    pub fn has_key(&self) -> bool {
        self.archive.keys.contains(&self.archive.room)
    }
    pub fn art(&self, slot: usize) -> Exhibit {
        exhibit(self.seed(), slot, 0)
    }
    pub fn entered_art(&self) -> Exhibit {
        if self.depth() <= 1 {
            self.art(self.archive.inside.first().copied().unwrap_or(0) as usize)
        } else {
            exhibit(
                room_seed(self.seed() ^ 0x8cb92baa3f3d8dd7, &self.archive.inside[..self.depth() - 1]),
                0,
                self.depth() - 1,
            )
        }
    }
    pub fn inner_art(&self) -> Exhibit {
        exhibit(self.world_seed(), 0, self.depth())
    }
    pub fn art_id(&self, slot: usize) -> String {
        format!("{}:{slot}", route_name(&self.archive.room))
    }
    pub fn world_id(&self) -> String {
        format!("{}:{:?}", route_name(&self.archive.room), self.archive.inside)
    }
    pub fn key_visible(&self) -> bool {
        if self.has_key() {
            return false;
        }
        if self.depth() == 0 {
            self.key_host() >= 3 && self.archive.awakened.contains(&self.art_id(self.key_host()))
        } else {
            self.archive.inside[0] as usize == self.key_host() && self.depth() == self.key_depth()
        }
    }
    pub fn key_position(&self) -> V {
        if self.depth() == 0 {
            let p = art_position(self.key_host());
            V(p.0, 2.4, p.2)
        } else {
            V(if self.world_seed() & 1 == 0 { -5.5 } else { 5.5 }, 1.35, -5.)
        }
    }
    pub fn targets(&self) -> Vec<(Target, V)> {
        let mut list = if self.depth() == 0 {
            let mut a: Vec<_> = (0..ART_COUNT).map(|s| (Target::Art(s), art_position(s))).collect();
            a.push((Target::Door(0), door_position(0)));
            a.push((Target::Door(1), door_position(1)));
            if !self.archive.room.is_empty() {
                a.push((Target::Parent, V(0., 1.65, 8.65)));
            }
            a
        } else {
            vec![(Target::Nested, V(0., 2.4, -11.)), (Target::Return, V(0., 1.7, 13.))]
        };
        if self.key_visible() {
            list.insert(0, (Target::Key, self.key_position()));
        }
        list
    }
    pub fn target(&self) -> Option<Target> {
        let cp = self.player.pitch.cos();
        let dir = V(self.player.yaw.sin() * cp, self.player.pitch.sin(), -self.player.yaw.cos() * cp);
        self.targets()
            .into_iter()
            .filter_map(|(t, p)| {
                let d = p - self.player.position;
                let len = d.length();
                let reach = if matches!(t, Target::Art(0)) { 4.5 } else { 3.6 };
                if len < reach && (len < 0.7 || dir.dot(d.norm()) > 0.76) {
                    Some((t, if t == Target::Key { len * 0.6 } else { len }))
                } else {
                    None
                }
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|v| v.0)
    }
    pub fn colliders(&self) -> Vec<Collider> {
        let (x, z) = if self.depth() == 0 { (ROOM_HALF, ROOM_LENGTH) } else { (23., 24.) };
        let mut a = vec![
            Collider { min: V(-x - 0.3, 0., -z), max: V(-x, 6., z) },
            Collider { min: V(x, 0., -z), max: V(x + 0.3, 6., z) },
            Collider { min: V(-x, 0., -z - 0.3), max: V(x, 6., -z) },
            Collider { min: V(-x, 0., z), max: V(x, 6., z + 0.3) },
        ];
        if self.depth() == 0 {
            for s in 3..6 {
                let p = art_position(s);
                a.push(Collider { min: V(p.0 - 0.65, 0., p.2 - 0.65), max: V(p.0 + 0.65, 1.1, p.2 + 0.65) });
            }
        }
        a
    }
    fn enter(&mut self, slot: u8) {
        self.archive.returns.push(self.player.network_state());
        self.archive.inside.push(slot);
        self.player = Self::controller(V(0., 0., 10.), 0.);
        self.archive.worlds.insert(self.world_id());
        self.archive.footsteps = 0.;
        self.events.push(Event::Entered);
    }
    fn leave(&mut self) {
        if let Some(p) = self.archive.returns.pop() {
            self.archive.inside.pop();
            self.player.restore_network_state(&p);
            self.archive.footsteps = 0.;
            self.events.push(Event::Returned);
        }
    }
    fn go_room(&mut self, room: Vec<u8>, back: bool) {
        self.archive.room = room;
        self.archive.visited.insert(self.archive.room.clone());
        self.player =
            Self::controller(V(0., 0., if back { -6.8 } else { 7.5 }), if back { std::f32::consts::PI } else { 0. });
        self.archive.footsteps = 0.;
        self.events.push(Event::DoorOpened);
    }
    fn interact(&mut self, t: Target) {
        match t {
            Target::Key => {
                if self.key_visible() {
                    self.archive.keys.insert(self.archive.room.clone());
                    self.events.push(Event::KeyFound);
                }
            }
            Target::Art(slot) if slot < 3 => self.enter(slot as u8),
            Target::Art(slot) => {
                self.archive.awakened.insert(self.art_id(slot));
                self.events.push(Event::Awakened(slot));
            }
            Target::Nested => self.enter(0),
            Target::Return => self.leave(),
            Target::Door(branch) => {
                if self.has_key() {
                    let mut room = self.archive.room.clone();
                    room.push(branch);
                    self.go_room(room, false);
                } else {
                    self.events.push(Event::Locked);
                }
            }
            Target::Parent => {
                let mut room = self.archive.room.clone();
                room.pop();
                self.go_room(room, true);
            }
        }
    }
    pub fn step(&mut self, input: &Input) {
        self.archive.tick += 1;
        let finite = |v: f32| if v.is_finite() { v } else { 0. };
        self.player.look(finite(input.look[0]), finite(input.look[1]), 1., false);
        let old = self.player.position;
        self.player.update(
            Movement {
                forward: finite(input.forward).clamp(-1., 1.),
                right: finite(input.right).clamp(-1., 1.),
                sprint: input.sprint,
                ..Default::default()
            },
            TICK,
            &self.colliders(),
        );
        self.archive.footsteps += (self.player.position - old).length();
        if self.archive.footsteps > 1.6 {
            self.archive.footsteps -= 1.6;
            self.events.push(Event::Footstep);
        }
        if input.leave {
            self.leave();
            return;
        }
        if let Some(t) = self.target() {
            if input.inspect {
                let art = match t {
                    Target::Art(s) => Some(self.art(s)),
                    Target::Nested => Some(self.inner_art()),
                    _ => None,
                };
                if let Some(a) = art {
                    self.events.push(Event::Inspected(a));
                }
            } else if input.interact {
                self.interact(t);
            }
        }
    }
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
    pub fn hint(&self) -> &'static str {
        if self.has_key() {
            "The key remembers you. Either gold door will open."
        } else if self.depth() > 0 && self.archive.inside[0] as usize == self.key_host() {
            if self.depth() < self.key_depth() {
                "A second frame is breathing. The key lies deeper."
            } else if self.depth() == self.key_depth() {
                "Listen for a golden note beyond the path."
            } else {
                "You have passed the golden note. Return one layer."
            }
        } else if self.key_host() < 3 {
            "One canvas holds a golden thread. Follow its smaller frames."
        } else {
            "The key sleeps in a solid work. Wake the sculptures and listen."
        }
    }
    pub fn face(&mut self, at: V) {
        self.player.yaw = path::yaw_facing(self.player.position, at);
        let d = at - self.player.position;
        self.player.pitch = (d.1 / d.length().max(0.001)).asin();
    }
}
impl Simulation for Sim {
    type Input = Input;
    fn step(&mut self, i: &Input) {
        Sim::step(self, i);
    }
    fn state_hash(&self) -> u64 {
        let mut h = StateHasher::new();
        for b in serde_json::to_vec(&self.capture()).expect("finite state") {
            h.u32(b as u32);
        }
        h.finish()
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct SimState {
    pub archive: Archive,
    pub player: ControllerState,
}
impl Snapshot for Sim {
    const KIND: &'static str = "reverie";
    const POLICY: SavePolicy = SavePolicy::Exact;
    type State = SimState;
    fn capture(&self) -> SimState {
        SimState { archive: self.archive.clone(), player: self.player.network_state() }
    }
    fn restore(&mut self, s: SimState) -> Result<(), String> {
        let a = &s.archive;
        let valid = |p: &ControllerState| {
            p.position.finite()
                && p.yaw.is_finite()
                && p.pitch.is_finite()
                && p.velocity.finite()
                && p.push.finite()
                && p.feet.is_finite()
                && p.body_height.is_finite()
                && p.vertical_velocity.is_finite()
                && p.velocity.length() < 20.
                && p.push.length() < 20.
                && p.feet.abs() < 0.01
                && (p.body_height - 1.8).abs() < 0.01
                && p.position.0.abs() <= 24.
                && p.position.2.abs() <= 25.
                && (0. ..=8.).contains(&p.position.1)
        };
        if !valid(&s.player)
            || !a.returns.iter().all(valid)
            || a.inside.len() != a.returns.len()
            || a.room.iter().any(|b| *b > 1)
            || a.inside.iter().any(|s| *s > 2)
            || a.visited.iter().any(|p| p.iter().any(|b| *b > 1))
            || !a.visited.contains(&a.room)
            || !a.keys.is_subset(&a.visited)
            || !a.footsteps.is_finite()
            || !(0. ..=1.7).contains(&a.footsteps)
        {
            return Err("This museum archive contains an invalid room, portal or pose.".into());
        }
        self.archive = s.archive;
        self.player.restore_network_state(&s.player);
        self.events.clear();
        Ok(())
    }
    fn save_tick(&self) -> u64 {
        self.archive.tick
    }
}
