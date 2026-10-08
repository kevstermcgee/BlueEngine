//! Corkscrew Key: fixed-step authoritative spatial puzzle; drawing reads its state.
pub mod rules;
mod sweep_table;
#[cfg(feature = "client")]
mod view;
use rules::*;
use serde::{Deserialize, Serialize};
use vesper3d::portable::*;

pub const TURN_TICKS: u32 = 18;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub tick: u32,
    pub chamber: usize,
    pub pose: Pose,
    pub axis: usize,
    pub sign: i32,
    pub history: Vec<Pose>,
    pub from: Pose,
    pub turning: u32,
    pub last_axis: usize,
    pub last_sign: i32,
    pub docked: bool,
    pub won: bool,
    pub rejected: u32,
    pub flash: u32,
    pub hint: bool,
}
pub struct Corkscrew {
    pub state: State,
    cues: Vec<usize>,
    #[cfg(feature = "client")]
    camera: view::Orbit,
}
// Public commands share one route for native keyboard, clickable controls and verification.
pub fn command(code: i32) -> Intent {
    Intent {
        action: true,
        pointer: Some(Point::new(-1, code)),
        ..Default::default()
    }
}
pub fn button_at(p: Point) -> Option<i32> {
    if (74..=104).contains(&p.y) && (24..=332).contains(&p.x) {
        let n = (p.x - 24) / 104;
        if (0..3).contains(&n) {
            return Some(n + 1);
        }
    }
    for (rect, code) in [
        (Rect::new(24, 119, 98, 29), 5),
        (Rect::new(130, 119, 98, 29), 4),
        (Rect::new(24, 166, 204, 38), 6),
        (Rect::new(24, 216, 98, 29), 7),
        (Rect::new(130, 216, 98, 29), 8),
    ] {
        if rect.contains(p) {
            return Some(code);
        }
    }
    None
}
impl Corkscrew {
    pub fn preview(&self) -> Option<Obstruction> {
        sweep(
            self.state.pose,
            self.state.axis,
            self.state.sign,
            &blocks(self.state.chamber),
        )
    }
    pub fn moves(&self) -> usize {
        self.state.history.len()
    }
    fn load_chamber(&mut self, n: usize) {
        let tick = self.state.tick;
        let hint = self.state.hint;
        self.state = State {
            tick,
            chamber: n,
            pose: Pose::default(),
            axis: 0,
            sign: 1,
            history: vec![],
            from: Pose::default(),
            turning: 0,
            last_axis: 0,
            last_sign: 1,
            docked: false,
            won: false,
            rejected: 0,
            flash: 0,
            hint,
        };
    }
    pub fn hint_move(&self) -> Option<(usize, i32)> {
        let mut p = Pose::default();
        for &(a, s) in ROUTES[self.state.chamber] {
            if p == self.state.pose {
                return Some((a, s));
            }
            p = p.screw(a, s);
        }
        None
    }
}
impl GameLogic for Corkscrew {
    const ID: &'static str = "corkscrew-key";
    const TITLE: &'static str = "Corkscrew Key";
    const CONTROLS: &'static str = "1/2/3 axis   Left/Right sign   Space step   Z undo   R retry";
    const VERIFY_TICKS: u32 = 672;
    fn new(_: u64) -> Self {
        Self {
            state: State {
                tick: 0,
                chamber: 0,
                pose: Pose::default(),
                axis: 0,
                sign: 1,
                history: vec![],
                from: Pose::default(),
                turning: 0,
                last_axis: 0,
                last_sign: 1,
                docked: false,
                won: false,
                rejected: 0,
                flash: 0,
                hint: false,
            },
            cues: vec![],
            #[cfg(feature = "client")]
            camera: Default::default(),
        }
    }
    fn restart(&mut self) {
        let n = if self.state.won {
            0
        } else {
            self.state.chamber
        };
        self.load_chamber(n);
        self.cues.clear();
    }
    fn pointer_target_only_on_press() -> bool {
        true
    }
    fn tick(&self) -> u32 {
        self.state.tick
    }
    fn outcome(&self) -> &'static str {
        if self.state.won {
            "won"
        } else {
            "playing"
        }
    }
    fn probe_input() -> Intent {
        command(6)
    }
    fn probe_success(&self) -> bool {
        self.state.chamber > 0 || self.state.pose != Pose::default()
    }
    fn verification_input(tick: u32) -> Intent {
        let mut start = 0;
        for route in ROUTES {
            for &(axis, sign) in route {
                match tick.checked_sub(start) {
                    Some(0) => return command(axis as i32 + 1),
                    Some(1) => return command(if sign > 0 { 4 } else { 5 }),
                    Some(2) => return command(6),
                    _ => {}
                }
                start += 24;
            }
            if tick == start {
                return command(6);
            }
            start += 24;
        }
        Intent::default()
    }
    fn take_cues(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.cues)
    }
    fn cue_point(&self, _cue: usize) -> Point {
        #[cfg(feature = "client")]
        {
            self.camera
                .project(self.state.pose.center.map(|v| v as f32))
        }
        #[cfg(not(feature = "client"))]
        {
            Point::new(550, 230)
        }
    }
}
impl Simulation for Corkscrew {
    type Input = Intent;
    fn step(&mut self, i: &Intent) {
        if self.state.won {
            return;
        }
        self.state.tick = self.state.tick.saturating_add(1);
        self.state.turning = self.state.turning.saturating_sub(1);
        self.state.flash = self.state.flash.saturating_sub(1);
        if !i.action {
            return;
        }
        let Some(p) = i.pointer else {
            return;
        };
        let code = if p.x == -1 { Some(p.y) } else { button_at(p) };
        match code {
            Some(1..=3) => self.state.axis = (code.unwrap() - 1) as usize,
            Some(4) => self.state.sign = 1,
            Some(5) => self.state.sign = -1,
            Some(7) => {
                if let Some(pose) = self.state.history.pop() {
                    self.state.pose = pose;
                    self.state.from = pose;
                    self.state.turning = 0;
                    self.state.docked = false;
                    self.state.flash = 0;
                    self.cues.push(0);
                }
            }
            Some(8) => self.state.hint = !self.state.hint,
            Some(6) => {
                if self.state.turning > 0 {
                    return;
                }
                if self.state.docked {
                    if self.state.chamber == 5 {
                        self.state.won = true;
                    } else {
                        self.load_chamber(self.state.chamber + 1);
                    }
                    self.cues.push(2);
                    return;
                }
                if self.preview().is_some() {
                    self.state.rejected = self.state.rejected.saturating_add(1);
                    self.state.flash = 50;
                    self.cues.push(1);
                    return;
                }
                self.state.history.push(self.state.pose);
                self.state.from = self.state.pose;
                self.state.last_axis = self.state.axis;
                self.state.last_sign = self.state.sign;
                self.state.pose = self.state.pose.screw(self.state.axis, self.state.sign);
                self.state.turning = TURN_TICKS;
                self.state.flash = 0;
                self.state.docked = self.state.pose == goal(self.state.chamber);
                self.cues.push(if self.state.docked { 2 } else { 0 });
            }
            _ => {}
        }
    }
    fn state_hash(&self) -> u64 {
        hash_json(&self.state)
    }
}
impl Snapshot for Corkscrew {
    const KIND: &'static str = "corkscrew-key";
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, s: State) -> Result<(), String> {
        let valid = s.chamber < 6
            && s.axis < 3
            && s.last_axis < 3
            && [-1, 1].contains(&s.sign)
            && [-1, 1].contains(&s.last_sign)
            && s.turning <= TURN_TICKS
            && s.flash <= 50
            && s.pose.valid()
            && s.from.valid()
            && s.history.iter().all(|p| p.valid());
        if !valid {
            return Err("Invalid Corkscrew Key snapshot".into());
        }
        let obstacles = blocks(s.chamber);
        if !pose_clear(s.pose, &obstacles)
            || s.history.iter().any(|p| !pose_clear(*p, &obstacles))
            || s.docked != (s.pose == goal(s.chamber))
            || (s.won && (s.chamber != 5 || !s.docked))
            || (s.turning > 0
                && (s.from.screw(s.last_axis, s.last_sign) != s.pose
                    || sweep(s.from, s.last_axis, s.last_sign, &obstacles).is_some()))
        {
            return Err("Snapshot pose, docking or animation is inconsistent".into());
        }
        self.state = s;
        self.cues.clear();
        Ok(())
    }
}
#[cfg(test)]
mod tests;
