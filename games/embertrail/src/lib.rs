//! Embertrail's deterministic 60 Hz rules. Rendering never changes gameplay state.
use serde::{Deserialize, Serialize};
use vesper3d::two_d::*;
#[cfg(feature = "client")]
mod art;
pub mod levels;
#[cfg(test)]
mod tests;

pub const SUB: i32 = 256;
pub const COURSES: usize = 32;
pub const FLOOR: i32 = 368;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tile {
    pub r: Rect,
    pub kind: u8,
    pub content: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pickup {
    pub r: Rect,
    pub kind: u8,
    pub active: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Enemy {
    pub r: Rect,
    pub kind: u8,
    pub dir: i32,
    pub vy: i32,
    pub phase: u32,
    pub active: bool,
    pub shell: bool,
    pub rolling: bool,
    pub hp: u8,
    pub immunity: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Platform {
    pub r: Rect,
    pub home: Point,
    pub span: i32,
    pub vertical: bool,
    pub phase: u32,
    pub previous: Rect,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Shot {
    pub r: Rect,
    pub vx: i32,
    pub vy: i32,
    pub life: u32,
    pub hostile: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Level {
    pub width: i32,
    pub tiles: Vec<Tile>,
    pub pickups: Vec<Pickup>,
    pub enemies: Vec<Enemy>,
    pub platforms: Vec<Platform>,
    pub hazards: Vec<Rect>,
    pub exit: Rect,
    pub checkpoint: i32,
    pub secret: Rect,
    pub secret_return: Rect,
    pub secret_used: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State {
    pub tick: u32,
    pub course: usize,
    pub unlocked: usize,
    pub level: Level,
    pub px: i32,
    pub py: i32,
    pub vx: i32,
    pub vy: i32,
    pub grounded: bool,
    pub crouching: bool,
    pub facing: i32,
    pub coyote: u8,
    pub jump_buffer: u8,
    pub jump_age: u8,
    pub up_held: bool,
    pub form: u8,
    pub shield: u32,
    pub invincible: u32,
    pub cooldown: u32,
    pub lives: u8,
    pub sparks: u32,
    pub score: u32,
    pub relics: u8,
    pub best_relics: [u8; COURSES],
    pub cleared: u32,
    pub timer: u32,
    pub checkpoint: bool,
    pub dead: u32,
    pub transition: u32,
    pub won: bool,
    pub secret_inside: bool,
    pub portal_cooldown: u32,
    pub shots: Vec<Shot>,
    pub camera: i32,
    pub atlas: bool,
    pub map_selection: usize,
    pub banner: String,
    pub banner_ticks: u32,
    pub puff: u32,
    pub cue: Point,
}
pub struct Embertrail {
    pub state: State,
    cues: Vec<usize>,
}
impl Embertrail {
    pub fn player(&self) -> Rect {
        Rect::new(
            self.state.px.div_euclid(SUB),
            self.state.py.div_euclid(SUB),
            22,
            if self.state.crouching { 18 } else { 30 },
        )
    }
    fn cue(&mut self, n: usize) {
        self.cues.push(n);
        let r = self.player();
        self.state.cue = Point::new(r.x - self.state.camera + 11, r.y + 10);
    }
    fn banner(&mut self, text: &str) {
        self.state.banner = text.into();
        self.state.banner_ticks = 150;
    }
    pub fn enter_course(&mut self, course: usize) {
        self.state.course = course;
        self.state.level = levels::build(course);
        self.state.px = 80 * SUB;
        self.state.py = 338 * SUB;
        self.state.vx = 0;
        self.state.vy = 0;
        self.state.grounded = true;
        self.state.crouching = false;
        self.state.checkpoint = false;
        self.state.dead = 0;
        self.state.transition = 0;
        self.state.timer = 60 * 240;
        self.state.relics = 0;
        self.state.secret_inside = false;
        self.state.portal_cooldown = 0;
        self.state.shots.clear();
        self.state.camera = 0;
        self.state.atlas = false;
        self.state.jump_buffer = 0;
        self.state.coyote = 6;
        self.state.up_held = false;
        self.state.jump_age = 0;
        self.state.shield = 90;
        self.state.invincible = 0;
        self.state.cooldown = 0;
        self.banner(&format!(
            "{}-{}  {}",
            course / 4 + 1,
            course % 4 + 1,
            levels::NAMES[course]
        ));
    }
    fn hurt(&mut self, fatal: bool) {
        if self.state.dead > 0
            || self.state.transition > 0
            || (!fatal && (self.state.shield > 0 || self.state.invincible > 0))
        {
            return;
        }
        if !fatal && self.state.form > 0 {
            self.state.form -= 1;
            self.state.shield = 120;
            self.state.vy = -4 * SUB;
            self.cue(1);
            self.banner("Lantern cracked! Keep going.");
        } else {
            self.state.lives = self.state.lives.saturating_sub(1);
            self.state.dead = 90;
            self.state.form = 0;
            self.state.invincible = 0;
            self.cue(1);
        }
    }
    fn respawn(&mut self) {
        let x = if self.state.checkpoint {
            self.state.level.checkpoint + 20
        } else {
            80
        };
        self.state.px = x * SUB;
        self.state.py = 338 * SUB;
        self.state.vx = 0;
        self.state.vy = 0;
        self.state.shield = 120;
        self.state.dead = 0;
        self.state.secret_inside = false;
        self.state.timer = 60 * 240;
        self.state.shots.clear();
        self.state.coyote = 6;
        self.state.jump_buffer = 0;
        self.state.grounded = true;
        self.state.crouching = false;
        self.state.camera = (x - 270).clamp(0, self.state.level.width - 800);
        self.banner(if self.state.checkpoint {
            "Checkpoint rekindled"
        } else {
            "Every little light gets another try"
        });
    }
    fn add_sparks(&mut self, amount: u32) {
        let before = self.state.sparks / 100;
        self.state.sparks += amount;
        self.state.score += amount * 100;
        if self.state.sparks / 100 > before {
            self.state.lives = (self.state.lives + 1).min(99);
            self.banner("100 sparks! An extra light.");
        }
        self.cue(0);
    }
    fn map_input(&mut self, i: &Intent) {
        if i.action {
            if let Some(p) = i.pointer {
                if Rect::new(708, 8, 82, 36).contains(p) {
                    self.state.atlas = !self.state.atlas;
                    return;
                }
                if self.state.atlas {
                    for n in 0..COURSES {
                        let r = Rect::new(
                            269 + (n % 4) as i32 * 112,
                            86 + (n / 4) as i32 * 35,
                            102,
                            27,
                        );
                        if r.contains(p) && n <= self.state.unlocked {
                            self.enter_course(n);
                            return;
                        }
                    }
                }
            }
        }
        if self.state.atlas {
            // D-pad up/down edges and action also make the atlas usable without a mouse.
            if i.y < 0 && !self.state.up_held {
                self.state.map_selection = self.state.map_selection.saturating_sub(1);
            }
            if i.y > 0 && !self.state.up_held {
                self.state.map_selection = (self.state.map_selection + 1).min(self.state.unlocked);
            }
            self.state.up_held = i.y != 0;
            if i.action {
                self.enter_course(self.state.map_selection);
            }
        }
    }
    fn bump(&mut self, before: Rect, after: Rect) {
        let mut rewards = Vec::new();
        let strong = self.state.form > 0;
        for tile in &mut self.state.level.tiles {
            if tile.r.w == 0
                || after.y != tile.r.y + tile.r.h
                || before.y < tile.r.y + tile.r.h
                || after.x + after.w <= tile.r.x
                || after.x >= tile.r.x + tile.r.w
            {
                continue;
            }
            if tile.kind == 3 {
                tile.kind = 4;
                rewards.push((tile.r, tile.content));
            } else if tile.kind == 2 && strong {
                tile.r.w = 0;
                self.state.score += 50;
                self.state.puff = 18;
            }
        }
        for (r, kind) in rewards {
            if kind == 0 {
                self.add_sparks(5);
            } else {
                let actual = if kind == 1 && self.state.form > 0 {
                    2
                } else {
                    kind
                };
                self.state.level.pickups.push(Pickup {
                    r: Rect::new(r.x + 4, r.y - 26, 24, 24),
                    kind: actual,
                    active: true,
                });
                self.cue(0);
            }
        }
    }
    fn collect(&mut self) {
        let r = self.player();
        let mut kinds = Vec::new();
        for p in &mut self.state.level.pickups {
            if p.active && p.r.overlaps(r) {
                p.active = false;
                kinds.push(p.kind);
            }
        }
        for kind in kinds {
            match kind {
                0 => self.add_sparks(1),
                1 => {
                    self.state.form = self.state.form.max(1);
                    self.state.score += 500;
                    self.banner("BLOOM LANTERN - one extra hit, break clay blocks");
                    self.cue(0);
                }
                2 => {
                    self.state.form = 2;
                    self.state.score += 800;
                    self.banner("EMBER LANTERN - Down + Space casts a seed");
                    self.cue(0);
                }
                3 => {
                    self.state.invincible = 600;
                    self.banner("DAWN STAR - ten seconds of fearless light");
                    self.cue(0);
                }
                4 => {
                    self.state.lives = (self.state.lives + 1).min(99);
                    self.banner("An extra light!");
                    self.cue(0);
                }
                5 => {
                    self.state.relics += 1;
                    self.state.score += 1500;
                    self.banner("Moon shard found! Three hidden in every course.");
                    self.cue(0);
                }
                _ => (),
            }
        }
    }
    fn move_platforms(&mut self) {
        let player = self.player();
        let mut carry = Point::default();
        for p in &mut self.state.level.platforms {
            p.previous = p.r;
            let t = (self.state.tick + p.phase) % 240;
            let triangle = if t < 120 { t as i32 } else { 240 - t as i32 };
            let offset = triangle * p.span / 120;
            if p.vertical {
                p.r.y = p.home.y - offset;
            } else {
                p.r.x = p.home.x + offset;
            }
            if self.state.grounded
                && player.y + player.h == p.previous.y
                && player.x + player.w > p.previous.x
                && player.x < p.previous.x + p.previous.w
            {
                carry.x = p.r.x - p.previous.x;
                carry.y = p.r.y - p.previous.y;
            }
        }
        self.state.px += carry.x * SUB;
        self.state.py += carry.y * SUB;
    }
    fn enemy_step(&mut self) {
        let walls: Vec<Rect> = self
            .state
            .level
            .tiles
            .iter()
            .filter(|t| t.r.w > 0)
            .map(|t| t.r)
            .collect();
        let tick = self.state.tick;
        let player = self.player();
        let mut projectiles = Vec::new();
        for e in &mut self.state.level.enemies {
            if !e.active {
                continue;
            }
            e.immunity = e.immunity.saturating_sub(1);
            if e.r.x < player.x - 900 || e.r.x > player.x + 1000 {
                continue;
            }
            match e.kind {
                2 => {
                    let t = (tick + e.phase) % 160;
                    e.r.x += e.dir * 2;
                    if e.r.slide(
                        0,
                        if t < 40 || (80..120).contains(&t) {
                            1
                        } else {
                            -1
                        },
                        &walls,
                    )[1]
                    {
                        e.dir = -e.dir;
                    }
                    if (tick + e.phase).is_multiple_of(90) {
                        e.dir = -e.dir;
                    }
                }
                3 => {
                    e.r.y = if (tick + e.phase) % 180 < 100 {
                        FLOOR - 56
                    } else {
                        FLOOR - 8
                    };
                }
                4 => {
                    e.vy = (e.vy + 1).min(12);
                    if (tick + e.phase).is_multiple_of(100) {
                        e.vy = -12;
                        e.dir = (player.x - e.r.x).signum();
                    }
                    let hit = e.r.slide(e.dir * 2, e.vy, &walls);
                    if hit[0] {
                        e.dir = -e.dir;
                    }
                    if hit[1] {
                        e.vy = 0;
                    }
                    if (tick + e.phase).is_multiple_of(85) {
                        projectiles.push(Shot {
                            r: Rect::new(e.r.x + 24, e.r.y + 20, 12, 12),
                            vx: (player.x - e.r.x).signum() * 4,
                            vy: -3,
                            life: 180,
                            hostile: true,
                        });
                    }
                }
                _ => {
                    let speed = if e.rolling {
                        8
                    } else if e.shell {
                        0
                    } else {
                        1
                    };
                    e.vy = (e.vy + 1).min(12);
                    let hit = e.r.slide(e.dir * speed, e.vy, &walls);
                    if hit[0] {
                        e.dir = -e.dir;
                    }
                    if hit[1] {
                        e.vy = 0;
                    }
                    if e.kind == 1
                        && !e.shell
                        && !walls
                            .iter()
                            .any(|w| w.overlaps(Rect::new(e.r.x + e.dir * 25, e.r.y + e.r.h, 2, 3)))
                    {
                        e.dir = -e.dir;
                    }
                    if e.r.y > 520 {
                        e.active = false;
                    }
                }
            }
        }
        self.state.shots.extend(projectiles);
        // Rolling seed shells also defeat other creatures and crack the region guardian.
        let shells: Vec<Rect> = self
            .state
            .level
            .enemies
            .iter()
            .filter(|e| e.active && e.rolling)
            .map(|e| e.r)
            .collect();
        for e in &mut self.state.level.enemies {
            if e.active && !e.rolling && shells.iter().any(|r| r.overlaps(e.r)) {
                if e.kind == 4 {
                    if e.immunity == 0 {
                        e.hp = e.hp.saturating_sub(1);
                        e.immunity = 45;
                        if e.hp == 0 {
                            e.active = false;
                        }
                    }
                } else {
                    e.active = false;
                    self.state.score += 200;
                }
            }
        }
    }
    fn enemy_contacts(&mut self, before: Rect) {
        let player = self.player();
        let mut damage = false;
        let mut reward = false;
        let mut bounce = false;
        for e in &mut self.state.level.enemies {
            if !e.active || !player.overlaps(e.r) {
                continue;
            }
            if self.state.invincible > 0 && e.kind != 4 {
                e.active = false;
                self.state.score += 200;
                reward = true;
                continue;
            }
            let stomp = self.state.vy >= 0 && before.y + before.h <= e.r.y + 12;
            if stomp && e.kind != 3 {
                if e.kind == 4 {
                    if e.immunity == 0 {
                        e.hp = e.hp.saturating_sub(1);
                        e.immunity = 60;
                        if e.hp == 0 {
                            e.active = false;
                            self.state.score += 5000;
                        }
                    }
                } else if e.kind == 1 {
                    if !e.shell {
                        e.shell = true;
                        e.r.y += 12;
                        e.r.h -= 12;
                        e.rolling = false;
                    } else {
                        e.rolling = false;
                    }
                } else {
                    e.active = false;
                }
                bounce = true;
                reward = true;
                self.state.score += 200;
            } else if e.kind == 1 && e.shell && !e.rolling {
                e.rolling = true;
                e.dir = if player.x < e.r.x { 1 } else { -1 };
                e.r.x += e.dir * 12;
                reward = true;
            } else {
                damage = true;
            }
        }
        if bounce {
            self.state.vy = -8 * SUB;
            self.state.py -= 4 * SUB;
            self.state.grounded = false;
            self.state.jump_age = 0;
        }
        if reward {
            self.cue(0);
        }
        if damage {
            self.hurt(false);
        }
    }
    fn shots_step(&mut self) {
        let walls: Vec<Rect> = self
            .state
            .level
            .tiles
            .iter()
            .filter(|t| t.r.w > 0)
            .map(|t| t.r)
            .collect();
        let player = self.player();
        let mut hit_player = false;
        for shot in &mut self.state.shots {
            if shot.life == 0 {
                continue;
            }
            shot.life -= 1;
            shot.vy = (shot.vy + 1).min(7);
            let hit = shot.r.slide(shot.vx, shot.vy, &walls);
            if hit[0] {
                shot.life = 0;
            }
            if hit[1] {
                shot.vy = -6;
            }
            if shot.hostile {
                if shot.r.overlaps(player) {
                    hit_player = true;
                    shot.life = 0;
                }
            } else {
                for e in &mut self.state.level.enemies {
                    if e.active && shot.r.overlaps(e.r) {
                        shot.life = 0;
                        if e.kind == 4 {
                            if e.immunity == 0 {
                                e.hp = e.hp.saturating_sub(1);
                                e.immunity = 30;
                                if e.hp == 0 {
                                    e.active = false;
                                    self.state.score += 5000;
                                }
                            }
                        } else {
                            e.active = false;
                            self.state.score += 200;
                        }
                        break;
                    }
                }
            }
        }
        self.state.shots.retain(|s| s.life > 0 && s.r.y < 500);
        if hit_player {
            self.hurt(false);
        }
    }
    fn portal(&mut self, i: &Intent) {
        if i.y <= 0 || self.state.portal_cooldown > 0 {
            return;
        }
        let p = self.player();
        if !self.state.secret_inside && p.overlaps(self.state.level.secret) {
            self.state.secret_inside = true;
            self.state.level.secret_used = true;
            self.state.px = (self.state.level.width + 176) * SUB;
            self.state.py = 338 * SUB;
            self.banner("The hollow between roots - Down at the far lantern to return");
        } else if self.state.secret_inside && p.overlaps(self.state.level.secret_return) {
            self.state.secret_inside = false;
            self.state.px = (self.state.level.secret.x + 8) * SUB;
            self.state.py = 320 * SUB;
            self.banner("A secret worth keeping");
        } else {
            return;
        }
        self.state.vx = 0;
        self.state.vy = 0;
        self.state.portal_cooldown = 60;
        self.state.crouching = false;
    }
    fn complete(&mut self) {
        self.state.cleared |= 1 << self.state.course;
        self.state.best_relics[self.state.course] =
            self.state.best_relics[self.state.course].max(self.state.relics);
        self.state.unlocked = self
            .state
            .unlocked
            .max((self.state.course + 1).min(COURSES - 1));
        self.state.score += 3000 + self.state.timer / 6;
        self.state.transition = 150;
        self.cue(2);
        self.banner(if self.state.course % 4 == 3 {
            "Dawn beacon rekindled!"
        } else {
            "A little brighter. Course clear!"
        });
    }
}
impl GameLogic for Embertrail {
    const ID: &'static str = "embertrail";
    const TITLE: &'static str = "Embertrail";
    const CONTROLS: &'static str =
        "Arrows/WASD move - Space/Up jump - Shift run - Down + Space cast";
    const VERIFY_TICKS: u32 = 1000;
    fn new(_: u64) -> Self {
        Self {
            state: State {
                tick: 0,
                course: 0,
                unlocked: 0,
                level: levels::build(0),
                px: 80 * SUB,
                py: 338 * SUB,
                vx: 0,
                vy: 0,
                grounded: true,
                crouching: false,
                facing: 1,
                coyote: 6,
                jump_buffer: 0,
                jump_age: 0,
                up_held: false,
                form: 0,
                shield: 0,
                invincible: 0,
                cooldown: 0,
                lives: 5,
                sparks: 0,
                score: 0,
                relics: 0,
                best_relics: [0; COURSES],
                cleared: 0,
                timer: 60 * 240,
                checkpoint: false,
                dead: 0,
                transition: 0,
                won: false,
                secret_inside: false,
                portal_cooldown: 0,
                shots: Vec::new(),
                camera: 0,
                atlas: false,
                map_selection: 0,
                banner: "MOTE, FOLLOW THE LIGHT".into(),
                banner_ticks: 180,
                puff: 0,
                cue: Point::new(80, 338),
            },
            cues: Vec::new(),
        }
    }
    fn tick(&self) -> u32 {
        self.state.tick
    }
    fn outcome(&self) -> &'static str {
        if self.state.won {
            "won"
        } else if self.state.lives == 0 {
            "lost"
        } else {
            "playing"
        }
    }
    fn probe_input() -> Intent {
        Intent {
            x: 1,
            ..Default::default()
        }
    }
    fn probe_success(&self) -> bool {
        self.state.px > 100 * SUB
    }
    fn verification_input(t: u32) -> Intent {
        // Real keyboard-equivalent inputs: run right, jump rhythmically over the introductory course.
        Intent {
            x: 1,
            y: if t % 52 < 28 { -1 } else { 0 },
            sprint: true,
            ..Default::default()
        }
    }
    fn take_cues(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.cues)
    }
    fn cue_point(&self, _: usize) -> Point {
        self.state.cue
    }
    fn audio_banks() -> &'static [AudioBankSpec] {
        &[AudioBankSpec {
            id: "trail",
            root: "assets/audio/trail",
            music: true,
        }]
    }
    fn audio_level(&self, _: &str, layer: &str) -> f32 {
        match layer {
            "melody" => 0.48,
            "bass" => 0.42,
            "glimmer" => {
                if self.state.course % 4 == 3 {
                    0.5
                } else {
                    0.25
                }
            }
            _ => 0.,
        }
    }
    fn restart(&mut self) {
        self.state.lives = self.state.lives.max(5);
        self.state.won = false;
        self.enter_course(self.state.course);
        self.cues.clear();
    }
}
impl Simulation for Embertrail {
    type Input = Intent;
    fn step(&mut self, i: &Intent) {
        if self.outcome() != "playing" {
            return;
        }
        self.state.tick += 1;
        self.map_input(i);
        if self.state.atlas {
            return;
        }
        self.state.banner_ticks = self.state.banner_ticks.saturating_sub(1);
        self.state.shield = self.state.shield.saturating_sub(1);
        self.state.invincible = self.state.invincible.saturating_sub(1);
        self.state.cooldown = self.state.cooldown.saturating_sub(1);
        self.state.portal_cooldown = self.state.portal_cooldown.saturating_sub(1);
        self.state.puff = self.state.puff.saturating_sub(1);
        if self.state.transition > 0 {
            self.state.transition -= 1;
            if self.state.transition == 0 {
                if self.state.course == 31 {
                    self.state.won = true;
                } else {
                    self.enter_course(self.state.course + 1);
                }
            }
            return;
        }
        if self.state.dead > 0 {
            self.state.dead -= 1;
            if self.state.dead == 0 {
                self.respawn();
            }
            return;
        }
        self.state.timer = self.state.timer.saturating_sub(1);
        if self.state.timer == 0 {
            self.hurt(true);
            return;
        }
        self.move_platforms();
        if i.y > 0 && self.state.grounded && !self.state.crouching {
            self.state.crouching = true;
            self.state.py += 12 * SUB;
        } else if i.y <= 0 && self.state.crouching {
            let p = self.player();
            let standing = Rect::new(p.x, p.y - 12, p.w, 30);
            if !self
                .state
                .level
                .tiles
                .iter()
                .any(|t| t.r.w > 0 && t.r.overlaps(standing))
            {
                self.state.crouching = false;
                self.state.py -= 12 * SUB;
            }
        }
        let up = i.y < 0;
        if (up && !self.state.up_held)
            || (i.action && i.y <= 0 && !i.pointer.is_some_and(|p| p.y < 52))
        {
            self.state.jump_buffer = 8;
        }
        self.state.up_held = up;
        if self.state.grounded {
            self.state.coyote = 6;
        } else {
            self.state.coyote = self.state.coyote.saturating_sub(1);
        }
        self.state.jump_buffer = self.state.jump_buffer.saturating_sub(1);
        if self.state.jump_buffer > 0 && self.state.coyote > 0 {
            self.state.vy = -11 * SUB;
            self.state.jump_buffer = 0;
            self.state.coyote = 0;
            self.state.grounded = false;
            self.state.jump_age = 0;
        }
        self.state.jump_age = self.state.jump_age.saturating_add(1);
        let world = self.state.course / 4;
        let acceleration = if world == 3 { 45 } else { 92 };
        let max_speed = if i.sprint { 1470 } else { 950 };
        let target = if i.y > 0 && self.state.grounded {
            0
        } else {
            i.x.clamp(-1, 1) * max_speed
        };
        let friction = if world == 3 { 22 } else { 100 };
        let rate = if target == 0 { friction } else { acceleration };
        self.state.vx += (target - self.state.vx).clamp(-rate, rate);
        if i.x != 0 {
            self.state.facing = i.x.signum();
        }
        let gravity = if up && self.state.vy < 0 && self.state.jump_age < 24 {
            78
        } else {
            142
        };
        self.state.vy = (self.state.vy + gravity).min(12 * SUB);
        if i.action
            && i.y > 0
            && self.state.form == 2
            && self.state.cooldown == 0
            && self.state.shots.len() < 12
        {
            let r = self.player();
            self.state.shots.push(Shot {
                r: Rect::new(r.x + self.state.facing * 16, r.y + 14, 10, 10),
                vx: self.state.facing * 7,
                vy: -2,
                life: 100,
                hostile: false,
            });
            self.state.cooldown = 18;
            self.state.puff = 12;
        }
        let before = self.player();
        let mut walls: Vec<Rect> = self
            .state
            .level
            .tiles
            .iter()
            .filter(|t| t.r.w > 0)
            .map(|t| t.r)
            .collect();
        for p in &self.state.level.platforms {
            if self.state.vy >= 0 && before.y + before.h <= p.r.y + 4 {
                walls.push(p.r);
            }
        }
        if self
            .state
            .level
            .enemies
            .iter()
            .any(|e| e.active && e.kind == 4)
        {
            walls.push(Rect::new(self.state.level.width - 98, 200, 12, 168));
        }
        let mut r = before;
        let nx = (self.state.px + self.state.vx).div_euclid(SUB);
        let hits_x = r.slide(nx - r.x, 0, &walls);
        if hits_x[0] {
            self.state.px = r.x * SUB;
            self.state.vx = 0;
        } else {
            self.state.px += self.state.vx;
        }
        self.state.px = self.state.px.max(0);
        let ny = (self.state.py + self.state.vy).div_euclid(SUB);
        let hits_y = r.slide(0, ny - r.y, &walls);
        self.state.grounded = hits_y[1] && self.state.vy > 0;
        if hits_y[1] {
            if self.state.vy < 0 {
                self.bump(before, r);
            }
            self.state.py = r.y * SUB;
            self.state.vy = 0;
        } else {
            self.state.py += self.state.vy;
        }
        self.portal(i);
        self.collect();
        self.enemy_step();
        self.enemy_contacts(before);
        self.shots_step();
        let r = self.player();
        if self.state.level.hazards.iter().any(|h| r.overlaps(*h)) {
            self.hurt(false);
        }
        if r.y > 480 {
            self.hurt(true);
        }
        if !self.state.secret_inside && !self.state.checkpoint && r.x > self.state.level.checkpoint
        {
            self.state.checkpoint = true;
            self.cue(0);
            self.banner("CHECKPOINT - your light is safe here");
        }
        if self.state.dead == 0
            && !self.state.secret_inside
            && r.overlaps(self.state.level.exit)
            && !self
                .state
                .level
                .enemies
                .iter()
                .any(|e| e.active && e.kind == 4)
        {
            self.complete();
        }
        let target = if self.state.secret_inside {
            (self.state.level.width + 128).clamp(0, i32::MAX - 800)
        } else {
            (r.x - 280).clamp(0, self.state.level.width - 800)
        };
        if self.state.secret_inside || (self.state.camera - target).abs() > 900 {
            self.state.camera = target;
        } else {
            self.state.camera += (target - self.state.camera).clamp(-9, 9);
        }
    }
    fn state_hash(&self) -> u64 {
        hash_json(&self.state)
    }
}
impl Snapshot for Embertrail {
    const KIND: &'static str = "embertrail";
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, s: State) -> Result<(), String> {
        let valid_rect = |r: Rect| {
            r.x >= -100
                && r.x <= 10000
                && r.y >= -600
                && r.y <= 1000
                && r.w >= 0
                && r.w <= 8000
                && r.h >= 0
                && r.h <= 800
        };
        if s.course >= COURSES
            || s.unlocked >= COURSES
            || s.course > s.unlocked
            || s.lives > 99
            || s.form > 2
            || s.relics > 3
            || s.best_relics.iter().any(|n| *n > 3)
            || s.level.width != levels::build(s.course).width
            || s.px < 0
            || s.px > (s.level.width + 1000) * SUB
            || s.py < -500 * SUB
            || s.py > 600 * SUB
            || s.vx.unsigned_abs() > 2000
            || s.vy.unsigned_abs() > 4000
            || s.level.tiles.len() > 1200
            || s.level.enemies.len() > 120
            || s.level.pickups.len() > 300
            || s.level.platforms.len() > 40
            || s.level.hazards.len() > 80
            || s.shots.len() > 100
            || !s
                .level
                .tiles
                .iter()
                .all(|t| valid_rect(t.r) && t.kind <= 4 && t.content <= 5)
            || !s
                .level
                .pickups
                .iter()
                .all(|p| valid_rect(p.r) && p.kind <= 5)
            || !s.level.enemies.iter().all(|e| {
                valid_rect(e.r)
                    && e.kind <= 4
                    && (-1..=1).contains(&e.dir)
                    && e.vy.unsigned_abs() <= 32
            })
            || !s
                .level
                .platforms
                .iter()
                .all(|p| valid_rect(p.r) && valid_rect(p.previous) && (0..=256).contains(&p.span))
            || !s.level.hazards.iter().all(|r| valid_rect(*r))
            || !s.shots.iter().all(|p| {
                valid_rect(p.r)
                    && p.vx.unsigned_abs() <= 12
                    && p.vy.unsigned_abs() <= 16
                    && p.life <= 240
            })
            || s.camera < 0
            || s.camera > s.level.width + 1000
            || s.map_selection >= COURSES
            || s.banner.len() > 160
            || s.timer > 60 * 240
            || s.dead > 90
            || s.transition > 150
        {
            return Err("Invalid Embertrail save bounds".into());
        }
        self.state = s;
        self.cues.clear();
        Ok(())
    }
}
