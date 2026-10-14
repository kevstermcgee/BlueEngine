//! Harvest Hollow: deterministic farm rules. The engine owns input, audio and saves.
use serde::{Deserialize, Serialize};
use vesper3d::two_d::*;
pub const W: i32 = 22;
pub const H: i32 = 13;
pub const TILE: i32 = 26;
pub const DAY_TICKS: u32 = 7200;
pub fn rainy(day: u32) -> bool {
    day % 4 == 3
}
pub fn crop_days(kind: u8) -> u8 {
    match kind {
        1 => 2,
        2 => 3,
        3 => 4,
        _ => 0,
    }
}
pub fn tile_center(x: i32, y: i32) -> Point {
    Point::new(x * TILE + 13, y * TILE + 13)
}
pub const CROP_NAMES: [&str; 3] = ["Radish", "Carrot", "Pumpkin"];
pub const PRICES: [u32; 3] = [8, 14, 24];
pub const SALES: [u32; 3] = [24, 45, 88];
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Tile {
    pub obstacle: u8,
    pub hp: u8,
    pub soil: bool,
    pub wet: bool,
    pub crop: u8,
    pub growth: u8,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    pub tick: u32,
    pub day: u32,
    pub clock: u32,
    pub player: Rect,
    pub facing: Point,
    pub tiles: Vec<Tile>,
    pub tool: u8,
    pub seed_kind: u8,
    pub seeds: [u32; 3],
    pub gold: u32,
    pub energy: u32,
    pub water: u32,
    pub wood: u32,
    pub stone: u32,
    pub harvest: [u32; 3],
    pub shipped: u32,
    pub cleared: u32,
    pub sprinkler: bool,
    pub bridge: bool,
    pub notice: String,
    pub notice_until: u32,
    pub action_at: Point,
    pub action_tick: u32,
    pub walk_target: Option<Point>,
    pub pending_action: bool,
    pub cooldown: u32,
}
pub struct Garden {
    pub state: State,
    cues: Vec<usize>,
    #[cfg(feature = "client")]
    pub panel_open: bool,
}
fn idx(x: i32, y: i32) -> usize {
    (y * W + x) as usize
}
fn inside(x: i32, y: i32) -> bool {
    (0..W).contains(&x) && (0..H).contains(&y)
}
pub fn reserved(x: i32, y: i32) -> bool {
    (x <= 4 && y <= 2)
        || ((17..=20).contains(&x) && (1..=3).contains(&y))
        || (y == 1 && (x == 8 || x == 12))
}
fn within_reach(actor: Point, target: Point) -> bool {
    let x = target.x / TILE;
    let y = target.y / TILE;
    let nearest = if (17..=20).contains(&x) && (1..=3).contains(&y) {
        Point::new(
            actor.x.clamp(17 * TILE + 13, 20 * TILE + 13),
            actor.y.clamp(TILE + 13, 3 * TILE + 13),
        )
    } else if x <= 4 && y <= 2 {
        Point::new(
            actor.x.clamp(13, 4 * TILE + 13),
            actor.y.clamp(13, 2 * TILE + 13),
        )
    } else {
        target
    };
    (actor.x - nearest.x).abs() <= 31 && (actor.y - nearest.y).abs() <= 31
}
impl Garden {
    // Route a click around solid farm clutter, stopping within tool reach.
    fn approach(&self, target: Point) -> Option<Point> {
        let cx = self.state.player.x + 8;
        let cy = self.state.player.y + 10;
        let start = Point::new(cx / TILE, cy / TILE);
        let mut previous = vec![None; (W * H) as usize];
        let mut queue = std::collections::VecDeque::from([start]);
        previous[idx(start.x, start.y)] = Some(start);
        while let Some(at) = queue.pop_front() {
            let center = tile_center(at.x, at.y);
            if within_reach(center, target) {
                let mut next = at;
                while let Some(parent) = previous[idx(next.x, next.y)] {
                    if parent == start {
                        break;
                    }
                    next = parent;
                }
                let dest = tile_center(next.x, next.y);
                let here = tile_center(start.x, start.y);
                let waypoint = if next.x != start.x && (cy - here.y).abs() > 2 {
                    Point::new(cx, here.y)
                } else if next.y != start.y && (cx - here.x).abs() > 2 {
                    Point::new(here.x, cy)
                } else {
                    dest
                };
                return Some(Point::new(
                    (waypoint.x - cx).signum(),
                    (waypoint.y - cy).signum(),
                ));
            }
            for delta in [
                Point::new(1, 0),
                Point::new(0, 1),
                Point::new(-1, 0),
                Point::new(0, -1),
            ] {
                let next = Point::new(at.x + delta.x, at.y + delta.y);
                if !inside(next.x, next.y)
                    || reserved(next.x, next.y)
                    || previous[idx(next.x, next.y)].is_some()
                {
                    continue;
                }
                if matches!(self.state.tiles[idx(next.x, next.y)].obstacle, 1 | 2) {
                    continue;
                }
                previous[idx(next.x, next.y)] = Some(at);
                queue.push_back(next);
            }
        }
        None
    }
    fn say(&mut self, text: impl Into<String>) {
        self.state.notice = text.into();
        self.state.notice_until = self.state.tick + 240;
    }
    fn cue(&mut self, cue: usize, x: i32, y: i32) {
        self.cues.push(cue);
        self.state.action_at = Point::new(x, y);
        self.state.action_tick = self.state.tick;
    }
    fn new_day(&mut self) {
        let mut grown = 0;
        for tile in &mut self.state.tiles {
            if tile.crop > 0 && tile.wet && tile.growth < crop_days(tile.crop) {
                tile.growth += 1;
                grown += 1;
            }
            tile.wet = false;
        }
        self.state.day += 1;
        self.state.clock = 0;
        self.state.energy = 100;
        self.state.water = 40;
        for y in 0..H {
            for x in 0..W {
                if rainy(self.state.day)
                    || (self.state.sprinkler && (7..=12).contains(&x) && (6..=9).contains(&y))
                {
                    self.state.tiles[idx(x, y)].wet = true;
                }
            }
        }
        self.state.walk_target = None;
        self.state.pending_action = false;
        self.say(if rainy(self.state.day) {
            format!("Day {}: rain waters your crops!", self.state.day)
        } else {
            format!("Good morning! {grown} crops grew.")
        });
        self.cues.push(2);
    }
    fn sell(&mut self) {
        let count: u32 = self.state.harvest.iter().sum();
        if count == 0 {
            self.say("Harvest ripe crops, then ship them here.");
            return;
        }
        let value: u32 = self
            .state
            .harvest
            .iter()
            .zip(SALES)
            .map(|(n, p)| n * p)
            .sum();
        self.state.gold += value;
        self.state.shipped += count;
        self.state.harvest = [0; 3];
        self.say(format!("Shipped {count} crops. +{value} gold!"));
        self.cues.push(2);
    }
    fn buy(&mut self) {
        let k = self.state.seed_kind as usize;
        let cost = PRICES[k] * 5;
        if self.state.gold < cost {
            self.say(format!("Five {} seeds cost {cost} gold.", CROP_NAMES[k]));
            return;
        }
        self.state.gold -= cost;
        self.state.seeds[k] += 5;
        self.say(format!("Five {} seeds added to your pouch.", CROP_NAMES[k]));
        self.cues.push(0);
    }
    fn build(&mut self) {
        if !self.state.sprinkler {
            if self.state.wood < 20 || self.state.stone < 8 || self.state.gold < 90 {
                self.say("Sprinkler: 20 wood + 8 stone + 90 gold.");
                return;
            }
            self.state.wood -= 20;
            self.state.stone -= 8;
            self.state.gold -= 90;
            self.state.sprinkler = true;
            self.say("Sprinkler built! Main beds water every morning.");
        } else if !self.state.bridge {
            if self.state.wood < 18 || self.state.stone < 5 {
                self.say("Pond deck: 18 wood + 5 stone.");
                return;
            }
            self.state.wood -= 18;
            self.state.stone -= 5;
            self.state.bridge = true;
            self.say("Pond deck built. A lovely place to unwind!");
        } else {
            self.say("Farm improvements complete. Keep growing!");
            return;
        }
        self.cues.push(2);
    }
    fn ui(&mut self, p: Point) -> bool {
        if p.y >= 402 && p.y < 442 && (16..576).contains(&p.x) {
            self.state.tool = ((p.x - 16) / 80).min(6) as u8;
            self.say(
                [
                    "Smart: clear, till, plant, water or harvest.",
                    "Hoe: turn grass into a crop bed.",
                    "Seeds: plant your selected crop.",
                    "Water: give each crop a drink daily.",
                    "Axe: chop trees, then remove stumps.",
                    "Pick: break rocks for stone.",
                    "Build: improve the farm from the right panel.",
                ][self.state.tool as usize],
            );
            return true;
        }
        if !(610..778).contains(&p.x) {
            return false;
        }
        match p.y {
            156..=187 => {
                self.state.seed_kind = (self.state.seed_kind + 1) % 3;
                self.say(format!(
                    "{}: {} watered days to harvest.",
                    CROP_NAMES[self.state.seed_kind as usize],
                    crop_days(self.state.seed_kind + 1)
                ));
            }
            192..=219 => self.buy(),
            226..=253 => self.sell(),
            264..=291 => self.build(),
            302..=329 => self.new_day(),
            340..=367 => self.say("Click land to walk + work. 1-7 tools, Tab crop, E work."),
            _ => return false,
        }
        true
    }
    fn work(&mut self, x: i32, y: i32) {
        if !inside(x, y) {
            return;
        }
        if y <= 2 && x <= 4 {
            self.new_day();
            return;
        }
        if x == 8 && y == 1 {
            self.buy();
            return;
        }
        if x == 12 && y == 1 {
            self.sell();
            return;
        }
        if (17..=20).contains(&x) && (1..=3).contains(&y) {
            self.state.water = 40;
            self.state.energy = (self.state.energy + 15).min(100);
            self.say("Watering can refilled. A peaceful moment by the pond.");
            self.cue(0, x, y);
            return;
        }
        if self.state.energy == 0 {
            self.say("Time to rest! Sleep restores all your energy.");
            return;
        }
        let n = idx(x, y);
        let tile = &self.state.tiles[n];
        let tool = self.state.tool;
        if tile.crop > 0 && tile.growth >= crop_days(tile.crop) {
            let k = tile.crop as usize - 1;
            self.state.harvest[k] += 1;
            self.state.tiles[n].crop = 0;
            self.state.tiles[n].growth = 0;
            self.state.energy -= 1;
            self.say(format!(
                "Fresh {}! Ship it for {} gold.",
                CROP_NAMES[k], SALES[k]
            ));
            self.cue(0, x, y);
            return;
        }
        let obstacle = tile.obstacle;
        if obstacle > 0 {
            let ok = tool == 0
                || (tool == 4 && (obstacle == 1 || obstacle == 4))
                || (tool == 5 && obstacle == 2);
            if !ok {
                self.say("Use Smart, the axe for trees, or pick for rocks.");
                return;
            }
            self.state.energy = self.state.energy.saturating_sub(2);
            self.state.tiles[n].hp = self.state.tiles[n].hp.saturating_sub(1);
            if self.state.tiles[n].hp == 0 {
                match obstacle {
                    1 => {
                        self.state.wood += 6;
                        self.state.tiles[n].obstacle = 4;
                        self.state.tiles[n].hp = 1;
                        self.say("Timber! +6 wood. One more swing clears the stump.");
                    }
                    4 => {
                        self.state.wood += 2;
                        self.state.tiles[n].obstacle = 0;
                        self.state.cleared += 1;
                        self.say("Stump cleared. +2 wood. Room to grow!");
                    }
                    2 => {
                        self.state.stone += 3;
                        self.state.tiles[n].obstacle = 0;
                        self.state.cleared += 1;
                        self.say("Rock cleared. +3 stone.");
                    }
                    _ => {
                        self.state.tiles[n].obstacle = 0;
                        self.state.cleared += 1;
                        self.state.seeds[self.state.seed_kind as usize] += 1;
                        self.say("Weeds cleared. Found a seed!");
                    }
                }
                self.cue(0, x, y);
            } else {
                self.say(if obstacle == 1 {
                    "Chop! Keep swinging."
                } else {
                    "Crack! One more hit."
                });
                self.cue(1, x, y);
            }
            return;
        }
        if tool == 6 {
            self.build();
            return;
        }
        if tile.crop > 0 {
            if tile.wet {
                self.say(format!(
                    "Growing: {}/{} days. Already watered.",
                    tile.growth,
                    crop_days(tile.crop)
                ));
                return;
            }
            if tool != 0 && tool != 3 {
                self.say("This crop needs water. Select Smart or Water.");
                return;
            }
            if self.state.water == 0 {
                self.say("Empty can! Click the pond to refill.");
                return;
            }
            self.state.water -= 1;
            self.state.energy -= 1;
            self.state.tiles[n].wet = true;
            self.say("Watered! Crops grow overnight when they have a drink.");
            self.cue(0, x, y);
            return;
        }
        if tool == 3 {
            if !tile.soil {
                self.say("Till this patch first.");
                return;
            }
            if tile.wet {
                self.say("This soil is already watered.");
                return;
            }
            if self.state.water == 0 {
                self.say("Refill your can at the pond.");
                return;
            }
            self.state.water -= 1;
            self.state.energy -= 1;
            self.state.tiles[n].wet = true;
            self.cue(0, x, y);
            return;
        }
        if !tile.soil {
            if tool != 0 && tool != 1 {
                self.say("Select Smart or Hoe to till the ground.");
                return;
            }
            self.state.tiles[n].soil = true;
            self.state.energy -= 1;
            self.say("A fresh crop bed! Click again to plant.");
            self.cue(0, x, y);
            return;
        }
        if tool == 0 || tool == 2 {
            let k = self.state.seed_kind as usize;
            if self.state.seeds[k] == 0 {
                self.say("Your seed pouch is empty. Buy seeds at the stand.");
                return;
            }
            self.state.seeds[k] -= 1;
            self.state.tiles[n].crop = k as u8 + 1;
            self.state.tiles[n].growth = 0;
            self.state.energy -= 1;
            self.say(format!("{} planted! Water it, then sleep.", CROP_NAMES[k]));
            self.cue(0, x, y);
        } else {
            self.say("Select Seeds or Smart to plant this bed.");
        }
    }
}
impl GameLogic for Garden {
    const ID: &'static str = "harvest-hollow";
    const TITLE: &'static str = "Harvest Hollow";
    const CONTROLS: &'static str =
        "WASD move | Click walk/work | E work | 1-7 tools | Tab crop | K/L save/load | Esc pause";
    const VERIFY_TICKS: u32 = 1400;
    fn new(_: u64) -> Self {
        let mut tiles = vec![Tile::default(); (W * H) as usize];
        for y in 0..H {
            for x in 0..W {
                if reserved(x, y) || ((5..=14).contains(&x) && (3..=10).contains(&y)) || y == 4 {
                    continue;
                }
                let v = (x * 37 + y * 71 + 13) % 17;
                let obstacle = if v < 3 {
                    1
                } else if v < 5 {
                    2
                } else if v < 9 {
                    3
                } else {
                    0
                };
                tiles[idx(x, y)].obstacle = obstacle;
                tiles[idx(x, y)].hp = match obstacle {
                    1 => 3,
                    2 => 2,
                    3 => 1,
                    _ => 0,
                };
            }
        }
        // Leave the shipping area open so forest canopies never hide the crate.
        for y in 0..=3 {
            for x in 10..=14 {
                tiles[idx(x, y)] = Tile::default();
            }
        }
        for y in 6..=9 {
            for x in 7..=12 {
                tiles[idx(x, y)].soil = true;
            }
        }
        for x in 7..=9 {
            tiles[idx(x, 6)].crop = 1;
            tiles[idx(x, 6)].growth = 1;
            tiles[idx(x, 6)].wet = true;
        }
        for (x, y, o) in [(15, 8, 1), (16, 8, 2), (5, 10, 3), (6, 10, 2), (4, 8, 1)] {
            tiles[idx(x, y)].obstacle = o;
            tiles[idx(x, y)].hp = if o == 1 {
                3
            } else if o == 2 {
                2
            } else {
                1
            };
        }
        Self {
            state: State {
                tick: 0,
                day: 1,
                clock: 0,
                player: Rect::new(7 * TILE + 5, 4 * TILE + 3, 16, 20),
                facing: Point::new(0, 1),
                tiles,
                tool: 0,
                seed_kind: 0,
                seeds: [9, 4, 2],
                gold: 160,
                energy: 100,
                water: 40,
                wood: 0,
                stone: 0,
                harvest: [0; 3],
                shipped: 0,
                cleared: 0,
                sprinkler: false,
                bridge: false,
                notice: "Welcome home! Three radishes will ripen after your first sleep.".into(),
                notice_until: 360,
                action_at: Point::new(7, 6),
                action_tick: 0,
                walk_target: None,
                pending_action: false,
                cooldown: 0,
            },
            cues: vec![],
            #[cfg(feature = "client")]
            panel_open: false,
        }
    }
    fn pointer_target_only_on_press() -> bool {
        true
    }
    fn tick(&self) -> u32 {
        self.state.tick
    }
    fn outcome(&self) -> &'static str {
        "playing"
    }
    fn probe_input() -> Intent {
        Intent {
            x: 1,
            ..Default::default()
        }
    }
    fn probe_success(&self) -> bool {
        self.state.player.x > 7 * TILE + 5
    }
    fn verification_input(t: u32) -> Intent {
        let p = match t {
            0 | 180 | 300 | 480 => Some(Point::new(16 + 10 * TILE + 13, 54 + 6 * TILE + 13)),
            240 | 420 => Some(Point::new(688, 315)),
            660 => Some(Point::new(688, 240)),
            740 | 880 | 920 | 960 => Some(Point::new(16 + 15 * TILE + 13, 54 + 8 * TILE + 13)),
            _ => None,
        };
        Intent {
            pointer: p,
            action: p.is_some(),
            ..Default::default()
        }
    }
    fn take_cues(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.cues)
    }
    fn cue_point(&self, _: usize) -> Point {
        let p = tile_center(self.state.action_at.x, self.state.action_at.y);
        Point::new(
            48 + (p.x as f32 * 1.23) as i32,
            24 + (p.y as f32 * 1.23) as i32,
        )
    }
    fn audio_banks() -> &'static [AudioBankSpec] {
        &[
            AudioBankSpec {
                id: "morning",
                root: "assets/audio/morning",
                music: true,
            },
            AudioBankSpec {
                id: "afternoon",
                root: "assets/audio/afternoon",
                music: true,
            },
            AudioBankSpec {
                id: "rain",
                root: "assets/audio/rain",
                music: true,
            },
            AudioBankSpec {
                id: "evening",
                root: "assets/audio/evening",
                music: true,
            },
        ]
    }
    fn audio_level(&self, bank: &str, _: &str) -> f32 {
        let active = if rainy(self.state.day) {
            "rain"
        } else if self.state.clock < 2400 {
            "morning"
        } else if self.state.clock < 4800 {
            "afternoon"
        } else {
            "evening"
        };
        if bank == active {
            0.62
        } else {
            0.0
        }
    }
}
impl Simulation for Garden {
    type Input = Intent;
    fn step(&mut self, i: &Intent) {
        self.state.tick += 1;
        self.state.clock += 1;
        self.state.cooldown = self.state.cooldown.saturating_sub(1);
        if self.state.clock >= DAY_TICKS {
            self.new_day();
        }
        if i.action {
            if let Some(p) = i.pointer {
                if self.ui(p) {
                    return;
                }
                let x = (p.x - 16).div_euclid(TILE);
                let y = (p.y - 54).div_euclid(TILE);
                if inside(x, y) {
                    self.state.walk_target = Some(tile_center(x, y));
                    self.state.pending_action = true;
                }
            } else if self.state.cooldown == 0 {
                let cx = (self.state.player.x + 8) / TILE;
                let cy = (self.state.player.y + 10) / TILE;
                self.work(cx + self.state.facing.x, cy + self.state.facing.y);
                self.state.cooldown = 14;
            }
        }
        let mut dx = i.x.clamp(-1, 1);
        let mut dy = i.y.clamp(-1, 1);
        if dx != 0 || dy != 0 {
            self.state.walk_target = None;
            self.state.pending_action = false;
        }
        if let Some(p) = self.state.walk_target {
            let cx = self.state.player.x + 8;
            let cy = self.state.player.y + 10;
            if within_reach(Point::new(cx, cy), p) {
                if self.state.pending_action && self.state.cooldown == 0 {
                    self.work(p.x / TILE, p.y / TILE);
                    self.state.cooldown = 14;
                    self.state.pending_action = false;
                    self.state.walk_target = None;
                }
            } else {
                if let Some(direction) = self.approach(p) {
                    dx = direction.x;
                    dy = direction.y;
                } else {
                    self.state.walk_target = None;
                    self.state.pending_action = false;
                    self.say("That patch is blocked. Clear a path or choose another tile.");
                }
            }
        }
        if dx != 0 || dy != 0 {
            self.state.facing = if dx != 0 {
                Point::new(dx, 0)
            } else {
                Point::new(0, dy)
            };
        }
        let mut walls = vec![
            Rect::new(-30, -30, W * TILE + 60, 30),
            Rect::new(-30, H * TILE, W * TILE + 60, 30),
            Rect::new(-30, 0, 30, H * TILE),
            Rect::new(W * TILE, 0, 30, H * TILE),
            Rect::new(0, 0, 5 * TILE, 3 * TILE),
            Rect::new(17 * TILE, TILE, 4 * TILE, 3 * TILE),
        ];
        for y in 0..H {
            for x in 0..W {
                let o = self.state.tiles[idx(x, y)].obstacle;
                if o == 1 || o == 2 {
                    walls.push(Rect::new(x * TILE + 5, y * TILE + 8, 16, 16));
                }
            }
        }
        self.state.player.slide(dx * 2, dy * 2, &walls);
    }
    fn state_hash(&self) -> u64 {
        hash_json(&self.state)
    }
}
impl Snapshot for Garden {
    const KIND: &'static str = "harvest-hollow";
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, s: State) -> Result<(), String> {
        if s.tiles.len() != (W * H) as usize
            || s.day == 0
            || s.clock >= DAY_TICKS
            || s.tool > 6
            || s.seed_kind > 2
            || s.energy > 100
            || s.water > 40
            || s.player.w != 16
            || s.player.h != 20
            || s.player.x < 0
            || s.player.y < 0
            || s.player.x + 16 > W * TILE
            || s.player.y + 20 > H * TILE
            || s.notice.len() > 256
            || s.tiles
                .iter()
                .any(|t| t.obstacle > 4 || t.hp > 3 || t.crop > 3 || t.growth > crop_days(t.crop))
        {
            return Err("Invalid farm save bounds".into());
        }
        self.state = s;
        self.cues.clear();
        Ok(())
    }
}
#[cfg(feature = "client")]
mod presentation;
#[cfg(test)]
mod tests {
    use super::*;
    fn click(g: &mut Garden, x: i32, y: i32) {
        g.step(&Intent {
            pointer: Some(Point::new(x, y)),
            action: true,
            ..Default::default()
        });
    }
    fn work(g: &mut Garden, x: i32, y: i32) {
        g.work(x, y);
    }
    #[test]
    fn clicking_a_bed_routes_around_solid_clutter() {
        let mut g = Garden::new(7);
        for y in 4..=6 {
            g.state.tiles[idx(9, y)].obstacle = 2;
            g.state.tiles[idx(9, y)].hp = 2;
        }
        click(&mut g, 16 + 12 * TILE + 13, 54 + 6 * TILE + 13);
        for _ in 0..300 {
            g.step(&Intent::default());
        }
        assert_eq!(g.state.tiles[idx(12, 6)].crop, 1);
        assert!(g.state.walk_target.is_none());
    }
    #[test]
    fn pond_and_cabin_interior_clicks_use_reachable_edges() {
        let mut g = Garden::new(7);
        g.state.water = 1;
        click(&mut g, 16 + 18 * TILE + 13, 54 + 2 * TILE + 13);
        for _ in 0..400 {
            g.step(&Intent::default());
        }
        assert_eq!(g.state.water, 40);
        assert!(g.state.walk_target.is_none());
        click(&mut g, 16 + 2 * TILE + 13, 54 + TILE + 13);
        for _ in 0..400 {
            g.step(&Intent::default());
        }
        assert_eq!(g.state.day, 2);
        assert!(g.state.walk_target.is_none());
    }
    #[test]
    fn public_route_farms_chops_ships_and_resumes_exactly() {
        let inputs: Vec<_> = (0..Garden::VERIFY_TICKS)
            .map(Garden::verification_input)
            .collect();
        vesper3d::runtime::assert_deterministic(|| Garden::new(7), &inputs);
        vesper3d::runtime::snapshot::assert_resumes_exactly(|| Garden::new(7), &inputs, 360);
        let mut g = Garden::new(7);
        for i in inputs {
            g.step(&i);
        }
        assert!(
            g.state.shipped >= 1,
            "public route must grow, harvest and ship a crop"
        );
        assert!(g.state.wood >= 6, "public route must chop a tree");
        assert_eq!(g.outcome(), "playing");
        if let Ok(path) = std::env::var("BE2_VERIFY_REPORT") {
            std::fs::write(path,serde_json::json!({"hash":format!("{:016x}",g.state_hash()),"outcome":"playing","purpose":"plant, water, grow, harvest, ship and chop through public intentions","ticks":Garden::VERIFY_TICKS}).to_string()).unwrap();
        }
    }
    #[test]
    fn dry_crops_wait_rain_and_sprinklers_water_without_dying() {
        let mut g = Garden::new(7);
        work(&mut g, 10, 7);
        g.new_day();
        assert_eq!(g.state.tiles[idx(10, 7)].growth, 0);
        work(&mut g, 10, 7);
        g.new_day();
        assert_eq!(g.state.tiles[idx(10, 7)].growth, 1);
        assert!(g.state.tiles[idx(10, 7)].wet);
        g.new_day();
        assert_eq!(g.state.tiles[idx(10, 7)].growth, 2);
        g.state.sprinkler = true;
        g.new_day();
        assert!(g.state.tiles[idx(10, 7)].wet);
    }
    #[test]
    fn tools_resources_building_and_refill_have_real_costs() {
        let mut g = Garden::new(7);
        g.state.tool = 3;
        work(&mut g, 6, 6);
        assert!(!g.state.tiles[idx(6, 6)].soil);
        g.state.tool = 0;
        for _ in 0..4 {
            work(&mut g, 15, 8);
        }
        assert_eq!(g.state.wood, 8);
        assert_eq!(g.state.tiles[idx(15, 8)].obstacle, 0);
        for _ in 0..2 {
            work(&mut g, 16, 8);
        }
        assert_eq!(g.state.stone, 3);
        g.build();
        assert!(!g.state.sprinkler);
        g.state.wood = 38;
        g.state.stone = 13;
        g.build();
        assert!(g.state.sprinkler);
        g.build();
        assert!(g.state.bridge);
        g.state.water = 0;
        work(&mut g, 18, 2);
        assert_eq!(g.state.water, 40);
        g.state.gold = 0;
        g.buy();
        assert_eq!(g.state.seeds, [9, 4, 2]);
    }
    #[test]
    fn ui_inventory_saves_and_collision_are_checked() {
        let mut g = Garden::new(7);
        click(&mut g, 688, 172);
        assert_eq!(g.state.seed_kind, 1);
        click(&mut g, 688, 204);
        assert_eq!(g.state.seeds[1], 9);
        assert_eq!(g.state.gold, 90);
        let old = g.state_hash();
        let mut s = g.capture();
        s.water = 41;
        assert!(g.restore(s).is_err());
        assert_eq!(old, g.state_hash());
        g.state.player = Rect::new(130, 20, 16, 20);
        for _ in 0..20 {
            g.step(&Intent {
                x: -1,
                ..Default::default()
            });
        }
        assert_eq!(g.state.player.x, 130);
        assert!(g.state.player.x >= 0);
        let mut fresh = Garden::new(7);
        fresh.step(&Garden::probe_input());
        assert!(fresh.probe_success());
    }
}
