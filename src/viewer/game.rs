//! Small declarative interaction rules shared by local play and the authoritative host.
//! No scripts, recursive events, geometry mutation or client-owned results.
use super::{
    authoring::MapDocument, controller::Controller, profile::ControllerProfile, room::Room,
};
use crate::{
    math::V,
    scene::{Shape, Track},
    Result,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Component, Path},
};

pub const MAX_COUNTER: i32 = 1_000_000;
pub const INTERACT_REACH: f32 = 2.5;

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerZone {
    pub id: String,
    pub bounds: super::controller::Collider,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnPoint {
    pub id: String,
    pub feet: V,
    pub yaw: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interactable {
    pub entity: String,
    pub enabled: bool,
    /// Presentation state only. Hidden targets retain collision and interaction eligibility.
    #[serde(default = "default_true")]
    pub visible: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub counter: String,
    pub equals: i32,
}

fn default_mover_duration() -> u32 {
    60
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mover {
    pub id: String,
    pub entity: String,
    pub translation: V,
    #[serde(default = "default_mover_duration")]
    pub duration_ticks: u32,
    #[serde(default)]
    pub initial_open: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimerDefinition {
    pub id: String,
    pub duration_ticks: u32,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub repeats: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum GameAction {
    Increment { counter: String, amount: i32 },
    SetCounter { counter: String, value: i32 },
    SetEnabled { entity: String, enabled: bool },
    SetVisible { entity: String, visible: bool },
    SetMover { mover: String, open: bool },
    StartTimer { timer: String },
    StopTimer { timer: String },
    Complete,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    /// None matches interaction with any declared, enabled target.
    pub on_interact: Option<String>,
    #[serde(default)]
    pub on_enter: Option<String>,
    #[serde(default)]
    pub on_exit: Option<String>,
    #[serde(default)]
    pub on_timer: Option<String>,
    pub condition: Option<Condition>,
    pub once: bool,
    pub actions: Vec<GameAction>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameDocument {
    pub schema_version: u32,
    pub name: String,
    /// Relative map path confined to the game document's directory (including symlinks).
    pub map: String,
    pub player_profile: ControllerProfile,
    pub spawn_points: Vec<SpawnPoint>,
    pub counters: BTreeMap<String, i32>,
    pub interactables: Vec<Interactable>,
    #[serde(default)]
    pub trigger_zones: Vec<TriggerZone>,
    #[serde(default)]
    pub movers: Vec<Mover>,
    #[serde(default)]
    pub timers: Vec<TimerDefinition>,
    pub rules: Vec<Rule>,
}

pub struct LoadedGame {
    pub document: GameDocument,
    pub map: MapDocument,
}

fn id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_/".contains(&b))
}
fn unique<'a>(values: impl Iterator<Item = &'a str>) -> bool {
    let mut ids = BTreeSet::new();
    values.into_iter().all(|s| id(s) && ids.insert(s))
}
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("Document byte limit exceeded".into());
    }
    Ok(bytes)
}

impl GameDocument {
    pub fn load(path: &Path) -> Result<LoadedGame> {
        let document: Self = serde_json::from_slice(&read_bounded(path, 64_000)?)?;
        let root = path
            .canonicalize()?
            .parent()
            .ok_or("Game needs a parent directory")?
            .to_path_buf();
        let relative = Path::new(&document.map);
        if relative.as_os_str().is_empty()
            || !relative
                .components()
                .all(|p| matches!(p, Component::Normal(_)))
        {
            return Err("map must be a relative child path without traversal".into());
        }
        let map_path = root.join(relative).canonicalize()?;
        if !map_path.starts_with(&root) {
            return Err("Map escapes game directory".into());
        }
        let map: MapDocument = serde_json::from_slice(&read_bounded(&map_path, 8_000_000)?)?;
        document.validate(&map)?;
        Ok(LoadedGame { document, map })
    }

    pub fn validate(&self, map: &MapDocument) -> Result<()> {
        map.validate()?;
        self.player_profile.validate()?;
        if self.schema_version != 1
            || self.name.is_empty()
            || self.name.len() > 100
            || self.spawn_points.is_empty()
            || self.spawn_points.len() > 8
            || self.counters.len() > 8
            || (self.interactables.is_empty() && self.trigger_zones.is_empty())
            || self.interactables.len() > 16
            || self.trigger_zones.len() > 16
            || self.movers.len() > 16
            || self.timers.len() > 16
            || self.rules.is_empty()
            || self.rules.len() > 16
        {
            return Err("Game v1: version=1, name 1..100 bytes, spawns 1..8, counters <=8, interactables/zones <=16 (at least 1 total), movers <=16, timers <=16, rules 1..16".into());
        }
        if !unique(self.spawn_points.iter().map(|s| s.id.as_str()))
            || !unique(self.interactables.iter().map(|s| s.entity.as_str()))
            || !unique(self.trigger_zones.iter().map(|s| s.id.as_str()))
            || !unique(self.movers.iter().map(|m| m.id.as_str()))
            || !unique(self.timers.iter().map(|t| t.id.as_str()))
            || !unique(self.rules.iter().map(|s| s.id.as_str()))
            || self
                .counters
                .iter()
                .any(|(key, v)| !id(key) || !(-MAX_COUNTER..=MAX_COUNTER).contains(v))
        {
            return Err("Invalid/duplicate semantic ID or counter outside +/-1000000".into());
        }
        let profile = self.player_profile;
        for spawn in &self.spawn_points {
            if !spawn.feet.finite()
                || [spawn.feet.0, spawn.feet.1, spawn.feet.2]
                    .iter()
                    .any(|v| v.abs() > 1000.)
                || spawn.feet.1 < 0.
                || !spawn.yaw.is_finite()
                || map.colliders.values().any(|c| {
                    c.overlaps_body(spawn.feet, spawn.feet.1, profile.height, profile.radius)
                })
            {
                return Err(format!("Invalid or blocked spawn: {}", spawn.id).into());
            }
        }
        for target in &self.interactables {
            let entity = map
                .entities
                .iter()
                .find(|e| e.id == target.entity)
                .ok_or("Unknown interaction entity")?;
            let node = map
                .scene
                .nodes
                .iter()
                .find(|n| n.id == target.entity)
                .ok_or("Interactables require matching static box node IDs")?;
            let collider = map
                .colliders
                .get(&target.entity)
                .ok_or("Interactables require matching collider IDs")?;
            let (Track::Fixed(center), Track::Fixed(half), Track::Fixed(rotation)) =
                (&node.pos, &node.scale, &node.rot)
            else {
                return Err("Interactables require fixed box transforms".into());
            };
            if !matches!(node.shape, Shape::Box)
                || *rotation != V::ZERO
                || node.material.starts_with("prop-")
                || node.material.starts_with("decor-")
                || collider.min != *center - *half
                || collider.max != *center + *half
                || entity.bounds.min != collider.min
                || entity.bounds.max != collider.max
            {
                return Err("Interactables must be static axis-aligned boxes with matching geometry/collision/entity bounds".into());
            }
        }
        let finite_vec = |v: V| {
            [v.0, v.1, v.2]
                .iter()
                .all(|x| x.is_finite() && x.abs() <= 1000.)
        };
        for zone in &self.trigger_zones {
            let b = &zone.bounds;
            if !finite_vec(b.min)
                || !finite_vec(b.max)
                || b.min.0 >= b.max.0
                || b.min.1 >= b.max.1
                || b.min.2 >= b.max.2
            {
                return Err(format!("Invalid trigger zone bounds: {}", zone.id).into());
            }
        }
        for mover in &self.movers {
            let entity = map
                .entities
                .iter()
                .find(|e| e.id == mover.entity)
                .ok_or("Unknown mover entity")?;
            let node = map
                .scene
                .nodes
                .iter()
                .find(|n| n.id == mover.entity)
                .ok_or("Movers require matching static box node IDs")?;
            let collider = map
                .colliders
                .get(&mover.entity)
                .ok_or("Movers require matching collider IDs")?;
            let (Track::Fixed(center), Track::Fixed(half), Track::Fixed(rotation)) =
                (&node.pos, &node.scale, &node.rot)
            else {
                return Err("Movers require fixed box transforms".into());
            };
            if !matches!(node.shape, Shape::Box)
                || *rotation != V::ZERO
                || node.material.starts_with("prop-")
                || node.material.starts_with("decor-")
                || collider.min != *center - *half
                || collider.max != *center + *half
                || entity.bounds.min != collider.min
                || entity.bounds.max != collider.max
            {
                return Err("Movers must be static axis-aligned boxes with matching geometry/collision/entity bounds".into());
            }
            if mover.duration_ticks == 0 || mover.duration_ticks > 3600 {
                return Err(format!("Invalid mover duration_ticks: {}", mover.id).into());
            }
            if !finite_vec(mover.translation) {
                return Err(format!("Invalid mover translation: {}", mover.id).into());
            }
        }
        for timer in &self.timers {
            if timer.duration_ticks == 0 || timer.duration_ticks > 36000 {
                return Err(format!("Invalid timer duration_ticks: {}", timer.id).into());
            }
        }
        let target_exists = |s: &str| self.interactables.iter().any(|i| i.entity == s);
        let zone_exists = |s: &str| self.trigger_zones.iter().any(|z| z.id == s);
        let mover_exists = |s: &str| self.movers.iter().any(|m| m.id == s);
        let timer_exists = |s: &str| self.timers.iter().any(|t| t.id == s);
        for rule in &self.rules {
            let trigger_count = rule.on_interact.is_some() as usize
                + rule.on_enter.is_some() as usize
                + rule.on_exit.is_some() as usize
                + rule.on_timer.is_some() as usize;
            if trigger_count > 1 {
                return Err(format!("Rule cannot declare multiple triggers: {}", rule.id).into());
            }
            if rule.actions.is_empty()
                || rule.actions.len() > 4
                || rule.on_interact.as_ref().is_some_and(|s| !target_exists(s))
                || rule.on_enter.as_ref().is_some_and(|s| !zone_exists(s))
                || rule.on_exit.as_ref().is_some_and(|s| !zone_exists(s))
                || rule.on_timer.as_ref().is_some_and(|s| !timer_exists(s))
                || rule.condition.as_ref().is_some_and(|c| {
                    !self.counters.contains_key(&c.counter)
                        || !(-MAX_COUNTER..=MAX_COUNTER).contains(&c.equals)
                })
            {
                return Err(format!("Invalid rule references/limits: {}", rule.id).into());
            }
            for action in &rule.actions {
                let valid = match action {
                    GameAction::Increment { counter, amount } => {
                        self.counters.contains_key(counter)
                            && (-MAX_COUNTER..=MAX_COUNTER).contains(amount)
                    }
                    GameAction::SetCounter { counter, value } => {
                        self.counters.contains_key(counter)
                            && (-MAX_COUNTER..=MAX_COUNTER).contains(value)
                    }
                    GameAction::SetEnabled { entity, .. } => {
                        target_exists(entity) || zone_exists(entity)
                    }
                    GameAction::SetVisible { entity, .. } => target_exists(entity),
                    GameAction::SetMover { mover, .. } => mover_exists(mover),
                    GameAction::StartTimer { timer } | GameAction::StopTimer { timer } => {
                        timer_exists(timer)
                    }
                    GameAction::Complete => true,
                };
                if !valid {
                    return Err(format!("Invalid action reference/value in {}", rule.id).into());
                }
            }
        }
        Ok(())
    }
}

/// Small full snapshot; indices are resolved through the fingerprint-matched document.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameState {
    pub counters: Vec<i32>,
    pub enabled: u16,
    /// Visible interactable geometry. This never changes collision or eligibility.
    #[serde(default, rename = "v", alias = "visible")]
    pub visible: u16,
    #[serde(default, rename = "z", alias = "enabled_zones")]
    pub enabled_zones: u16,
    #[serde(default)]
    pub mover_targets: u16,
    #[serde(default)]
    pub active_timers: u16,
    pub fired: u16,
    pub completed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameEvent {
    pub player_id: u64,
    pub entity: String,
}

enum Effect {
    Increment(usize, i32),
    Set(usize, i32),
    EnableInteractable(usize, bool),
    SetVisible(usize, bool),
    EnableZone(usize, bool),
    SetMover(usize, bool),
    StartTimer(usize),
    StopTimer(usize),
    Complete,
}
struct CompiledRule {
    index: usize,
    once: bool,
    condition: Option<(usize, i32)>,
    effects: Vec<Effect>,
}

fn apply_effects(state: &mut GameState, effects: &[Effect]) {
    for action in effects {
        match *action {
            Effect::Increment(i, amount) => {
                state.counters[i] = (state.counters[i] + amount).clamp(-MAX_COUNTER, MAX_COUNTER);
            }
            Effect::Set(i, value) => state.counters[i] = value,
            Effect::EnableInteractable(i, enabled) => {
                if enabled {
                    state.enabled |= 1 << i;
                } else {
                    state.enabled &= !(1 << i);
                }
            }
            Effect::SetVisible(i, visible) => {
                if visible {
                    state.visible |= 1 << i;
                } else {
                    state.visible &= !(1 << i);
                }
            }
            Effect::EnableZone(i, enabled) => {
                if enabled {
                    state.enabled_zones |= 1 << i;
                } else {
                    state.enabled_zones &= !(1 << i);
                }
            }
            Effect::SetMover(i, open) => {
                if open {
                    state.mover_targets |= 1 << i;
                } else {
                    state.mover_targets &= !(1 << i);
                }
            }
            Effect::StartTimer(i) => {
                state.active_timers |= 1 << i;
            }
            Effect::StopTimer(i) => {
                state.active_timers &= !(1 << i);
            }
            Effect::Complete => state.completed = true,
        }
        if state.completed {
            break;
        }
    }
}

#[derive(Clone, Debug)]
pub struct CompiledTimer {
    pub id: String,
    pub duration_ticks: u32,
    pub remaining_ticks: u32,
    pub repeats: bool,
}

#[derive(Clone, Debug)]
pub struct CompiledMover {
    pub entity: String,
    pub base_collider: super::controller::Collider,
    pub translation: V,
    pub duration_ticks: u32,
    pub current_ticks: u32,
    pub collider_index: Option<usize>,
}
impl CompiledMover {
    pub fn current_bounds(&self) -> super::controller::Collider {
        let progress = self.progress();
        let offset = self.translation * progress;
        super::controller::Collider {
            min: self.base_collider.min + offset,
            max: self.base_collider.max + offset,
        }
    }
    pub fn progress(&self) -> f32 {
        if self.duration_ticks == 0 {
            0.
        } else {
            self.current_ticks as f32 / self.duration_ticks as f32
        }
    }
}

pub struct GameRuntime {
    document: GameDocument,
    state: GameState,
    last_snapshot: Option<u64>,
    targets: Vec<super::controller::Collider>,
    trigger_zones: Vec<super::controller::Collider>,
    movers: Vec<CompiledMover>,
    timers: Vec<CompiledTimer>,
    rules: Vec<Vec<CompiledRule>>,
    zone_rules_enter: Vec<Vec<CompiledRule>>,
    zone_rules_exit: Vec<Vec<CompiledRule>>,
    timer_rules: Vec<Vec<CompiledRule>>,
    player_zones: BTreeMap<u64, u16>,
}
impl GameRuntime {
    pub fn compile(document: GameDocument, map: &MapDocument) -> Result<Self> {
        document.validate(map)?;
        let counter_index: BTreeMap<_, _> = document
            .counters
            .keys()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();
        let target_index: BTreeMap<_, _> = document
            .interactables
            .iter()
            .enumerate()
            .map(|(i, t)| (t.entity.as_str(), i))
            .collect();
        let zone_index: BTreeMap<_, _> = document
            .trigger_zones
            .iter()
            .enumerate()
            .map(|(i, z)| (z.id.as_str(), i))
            .collect();
        let mover_index: BTreeMap<_, _> = document
            .movers
            .iter()
            .enumerate()
            .map(|(i, m)| (m.id.as_str(), i))
            .collect();
        let timer_index: BTreeMap<_, _> = document
            .timers
            .iter()
            .enumerate()
            .map(|(i, t)| (t.id.as_str(), i))
            .collect();

        let compile_effects = |actions: &[GameAction]| -> Vec<Effect> {
            actions
                .iter()
                .map(|a| match a {
                    GameAction::Increment { counter, amount } => {
                        Effect::Increment(counter_index[counter.as_str()], *amount)
                    }
                    GameAction::SetCounter { counter, value } => {
                        Effect::Set(counter_index[counter.as_str()], *value)
                    }
                    GameAction::SetEnabled { entity, enabled } => {
                        if let Some(&idx) = target_index.get(entity.as_str()) {
                            Effect::EnableInteractable(idx, *enabled)
                        } else if let Some(&idx) = zone_index.get(entity.as_str()) {
                            Effect::EnableZone(idx, *enabled)
                        } else {
                            panic!("Validated action target missing: {}", entity);
                        }
                    }
                    GameAction::SetVisible { entity, visible } => {
                        Effect::SetVisible(target_index[entity.as_str()], *visible)
                    }
                    GameAction::SetMover { mover, open } => {
                        Effect::SetMover(mover_index[mover.as_str()], *open)
                    }
                    GameAction::StartTimer { timer } => {
                        Effect::StartTimer(timer_index[timer.as_str()])
                    }
                    GameAction::StopTimer { timer } => {
                        Effect::StopTimer(timer_index[timer.as_str()])
                    }
                    GameAction::Complete => Effect::Complete,
                })
                .collect()
        };

        let rules = document
            .interactables
            .iter()
            .map(|target| {
                document
                    .rules
                    .iter()
                    .enumerate()
                    .filter(|(_, rule)| {
                        rule.on_enter.is_none()
                            && rule.on_exit.is_none()
                            && rule.on_timer.is_none()
                            && rule
                                .on_interact
                                .as_ref()
                                .is_none_or(|s| s == &target.entity)
                    })
                    .map(|(index, rule)| CompiledRule {
                        index,
                        once: rule.once,
                        condition: rule
                            .condition
                            .as_ref()
                            .map(|c| (counter_index[c.counter.as_str()], c.equals)),
                        effects: compile_effects(&rule.actions),
                    })
                    .collect()
            })
            .collect();

        let zone_rules_enter = document
            .trigger_zones
            .iter()
            .map(|zone| {
                document
                    .rules
                    .iter()
                    .enumerate()
                    .filter(|(_, rule)| rule.on_enter.as_ref().is_some_and(|s| s == &zone.id))
                    .map(|(index, rule)| CompiledRule {
                        index,
                        once: rule.once,
                        condition: rule
                            .condition
                            .as_ref()
                            .map(|c| (counter_index[c.counter.as_str()], c.equals)),
                        effects: compile_effects(&rule.actions),
                    })
                    .collect()
            })
            .collect();

        let zone_rules_exit = document
            .trigger_zones
            .iter()
            .map(|zone| {
                document
                    .rules
                    .iter()
                    .enumerate()
                    .filter(|(_, rule)| rule.on_exit.as_ref().is_some_and(|s| s == &zone.id))
                    .map(|(index, rule)| CompiledRule {
                        index,
                        once: rule.once,
                        condition: rule
                            .condition
                            .as_ref()
                            .map(|c| (counter_index[c.counter.as_str()], c.equals)),
                        effects: compile_effects(&rule.actions),
                    })
                    .collect()
            })
            .collect();

        let timer_rules = document
            .timers
            .iter()
            .map(|timer| {
                document
                    .rules
                    .iter()
                    .enumerate()
                    .filter(|(_, rule)| rule.on_timer.as_ref().is_some_and(|s| s == &timer.id))
                    .map(|(index, rule)| CompiledRule {
                        index,
                        once: rule.once,
                        condition: rule
                            .condition
                            .as_ref()
                            .map(|c| (counter_index[c.counter.as_str()], c.equals)),
                        effects: compile_effects(&rule.actions),
                    })
                    .collect()
            })
            .collect();

        let mut enabled = 0;
        for (i, target) in document.interactables.iter().enumerate() {
            if target.enabled {
                enabled |= 1 << i;
            }
        }
        let mut enabled_zones = 0;
        for (i, zone) in document.trigger_zones.iter().enumerate() {
            if zone.enabled {
                enabled_zones |= 1 << i;
            }
        }
        let mut visible = 0;
        for (i, target) in document.interactables.iter().enumerate() {
            if target.visible {
                visible |= 1 << i;
            }
        }
        let mut mover_targets = 0;
        for (i, mover) in document.movers.iter().enumerate() {
            if mover.initial_open {
                mover_targets |= 1 << i;
            }
        }
        let mut active_timers = 0;
        for (i, timer) in document.timers.iter().enumerate() {
            if timer.auto_start {
                active_timers |= 1 << i;
            }
        }
        let state = GameState {
            counters: document.counters.values().copied().collect(),
            enabled,
            visible,
            enabled_zones,
            mover_targets,
            active_timers,
            ..Default::default()
        };
        let targets = document
            .interactables
            .iter()
            .map(|t| map.colliders[&t.entity].clone())
            .collect();
        let trigger_zones = document
            .trigger_zones
            .iter()
            .map(|z| z.bounds.clone())
            .collect();
        let movers = document
            .movers
            .iter()
            .map(|m| {
                let current_ticks = if m.initial_open { m.duration_ticks } else { 0 };
                let base_collider = map.colliders[&m.entity].clone();
                CompiledMover {
                    entity: m.entity.clone(),
                    base_collider,
                    translation: m.translation,
                    duration_ticks: m.duration_ticks,
                    current_ticks,
                    collider_index: None,
                }
            })
            .collect();
        let timers = document
            .timers
            .iter()
            .map(|t| CompiledTimer {
                id: t.id.clone(),
                duration_ticks: t.duration_ticks,
                remaining_ticks: t.duration_ticks,
                repeats: t.repeats,
            })
            .collect();
        Ok(Self {
            document,
            state,
            last_snapshot: None,
            targets,
            trigger_zones,
            movers,
            timers,
            rules,
            zone_rules_enter,
            zone_rules_exit,
            timer_rules,
            player_zones: BTreeMap::new(),
        })
    }
    pub fn state(&self) -> &GameState {
        &self.state
    }
    pub fn document(&self) -> &GameDocument {
        &self.document
    }
    pub fn targets(&self) -> &[super::controller::Collider] {
        &self.targets
    }
    pub fn trigger_zones(&self) -> &[super::controller::Collider] {
        &self.trigger_zones
    }
    pub fn movers(&self) -> &[CompiledMover] {
        &self.movers
    }
    pub fn mover_count(&self) -> usize {
        self.movers.len()
    }
    pub fn mover_open(&self, index: usize) -> bool {
        index < self.movers.len() && self.state.mover_targets & (1 << index) != 0
    }
    pub fn mover_progress(&self, index: usize) -> Option<f32> {
        self.movers.get(index).map(|m| m.progress())
    }
    pub fn mover_bounds(&self, index: usize) -> Option<super::controller::Collider> {
        self.movers.get(index).map(|m| m.current_bounds())
    }
    pub fn timers(&self) -> &[CompiledTimer] {
        &self.timers
    }
    pub fn timer_count(&self) -> usize {
        self.timers.len()
    }
    pub fn timer_active(&self, index: usize) -> bool {
        index < self.timers.len() && self.state.active_timers & (1 << index) != 0
    }
    pub fn timer_remaining(&self, index: usize) -> Option<u32> {
        if !self.timer_active(index) {
            return None;
        }
        self.timers.get(index).map(|t| t.remaining_ticks)
    }
    pub fn step_timers(&mut self) {
        if self.state.completed {
            return;
        }
        let mut expired_timers = Vec::new();
        for (i, timer) in self.timers.iter_mut().enumerate() {
            if self.state.active_timers & (1 << i) == 0 {
                timer.remaining_ticks = timer.duration_ticks;
                continue;
            }
            if timer.remaining_ticks == 0 {
                timer.remaining_ticks = timer.duration_ticks;
            }
            if timer.remaining_ticks > 1 {
                timer.remaining_ticks -= 1;
            } else {
                timer.remaining_ticks = 0;
                expired_timers.push(i);
            }
        }
        for i in expired_timers {
            self.timers[i].remaining_ticks = self.timers[i].duration_ticks;
            if !self.timers[i].repeats {
                self.state.active_timers &= !(1 << i);
            }
            self.fire_timer_rules(i);
            if self.state.completed {
                break;
            }
        }
    }
    fn fire_timer_rules(&mut self, timer_index: usize) {
        for rule in &self.timer_rules[timer_index] {
            if rule.once && self.state.fired & (1 << rule.index) != 0 {
                continue;
            }
            if rule
                .condition
                .is_some_and(|(i, v)| self.state.counters[i] != v)
            {
                continue;
            }
            apply_effects(&mut self.state, &rule.effects);
            if rule.once {
                self.state.fired |= 1 << rule.index;
            }
            if self.state.completed {
                break;
            }
        }
    }
    pub fn step_movers(&mut self, room: &mut Room) {
        for (i, mover) in self.movers.iter_mut().enumerate() {
            let target_open = self.state.mover_targets & (1 << i) != 0;
            if target_open && mover.current_ticks < mover.duration_ticks {
                mover.current_ticks += 1;
            } else if !target_open && mover.current_ticks > 0 {
                mover.current_ticks -= 1;
            }
        }
        self.apply_mover_colliders(room);
    }
    pub fn apply_mover_colliders(&mut self, room: &mut Room) {
        for mover in &mut self.movers {
            let progress = mover.progress();
            let offset = mover.translation * progress;
            if mover.collider_index.is_none() {
                mover.collider_index = room.colliders.iter().position(|c| {
                    (c.min - mover.base_collider.min).length() < 0.001
                        && (c.max - mover.base_collider.max).length() < 0.001
                });
            }
            if let Some(idx) = mover.collider_index {
                if idx < room.colliders.len() {
                    room.colliders[idx].min = mover.base_collider.min + offset;
                    room.colliders[idx].max = mover.base_collider.max + offset;
                }
            }
            if let Some(entity) = room.entities.iter_mut().find(|e| e.id == mover.entity) {
                entity.bounds.min = mover.base_collider.min + offset;
                entity.bounds.max = mover.base_collider.max + offset;
            }
            for (t_idx, target) in self.document.interactables.iter().enumerate() {
                if target.entity == mover.entity {
                    self.targets[t_idx].min = mover.base_collider.min + offset;
                    self.targets[t_idx].max = mover.base_collider.max + offset;
                }
            }
        }
    }
    /// Loss/reordering-safe full-state mirror. Invalid or older packets do not mutate it.
    pub fn accept_snapshot(&mut self, tick: u64, state: GameState) -> bool {
        if self.last_snapshot.is_some_and(|last| tick <= last) || !self.accept_state(state) {
            return false;
        }
        self.last_snapshot = Some(tick);
        true
    }
    /// Validate a full authoritative state before replacing the client mirror.
    fn accept_state(&mut self, state: GameState) -> bool {
        let mask = |count: usize| {
            if count == 16 {
                u16::MAX
            } else {
                (1_u16 << count) - 1
            }
        };
        if state.counters.len() != self.document.counters.len()
            || state
                .counters
                .iter()
                .any(|v| !(-MAX_COUNTER..=MAX_COUNTER).contains(v))
            || state.enabled & !mask(self.targets.len()) != 0
            || state.visible & !mask(self.targets.len()) != 0
            || state.enabled_zones & !mask(self.trigger_zones.len()) != 0
            || state.mover_targets & !mask(self.movers.len()) != 0
            || state.active_timers & !mask(self.timers.len()) != 0
            || state.fired & !mask(self.document.rules.len()) != 0
        {
            return false;
        }
        self.state = state;
        true
    }
    /// Resolve the actual closest visible surface; clients never specify an authoritative target.
    pub fn target(&self, room: &Room, controller: &Controller) -> Option<usize> {
        let hit = room.hit(controller.ray(), INTERACT_REACH)?;
        self.targets
            .iter()
            .position(|bounds| bounds.contains(hit.p))
    }
    pub fn enabled(&self, index: usize) -> bool {
        index < self.targets.len() && self.state.enabled & (1 << index) != 0
    }
    pub fn visible(&self, index: usize) -> bool {
        index < self.targets.len() && self.state.visible & (1 << index) != 0
    }
    pub fn enabled_entity(&self, entity: &str) -> Option<bool> {
        self.document
            .interactables
            .iter()
            .position(|target| target.entity == entity)
            .map(|index| self.enabled(index))
    }
    pub fn visible_entity(&self, entity: &str) -> Option<bool> {
        self.document
            .interactables
            .iter()
            .position(|target| target.entity == entity)
            .map(|index| self.visible(index))
    }
    pub fn zone_enabled(&self, index: usize) -> bool {
        index < self.trigger_zones.len() && self.state.enabled_zones & (1 << index) != 0
    }
    /// One bounded event, rules in document order; later rules see earlier actions.
    /// Increment saturates at +/-MAX_COUNTER. Completed games ignore further events.
    pub fn interact(
        &mut self,
        room: &Room,
        controller: &Controller,
        player_id: u64,
    ) -> Option<GameEvent> {
        if self.state.completed {
            return None;
        }
        let target = self.target(room, controller)?;
        if !self.enabled(target) {
            return None;
        }
        for rule in &self.rules[target] {
            if rule.once && self.state.fired & (1 << rule.index) != 0 {
                continue;
            }
            if rule
                .condition
                .is_some_and(|(i, v)| self.state.counters[i] != v)
            {
                continue;
            }
            apply_effects(&mut self.state, &rule.effects);
            if rule.once {
                self.state.fired |= 1 << rule.index;
            }
            if self.state.completed {
                break;
            }
        }
        Some(GameEvent {
            player_id,
            entity: self.document.interactables[target].entity.clone(),
        })
    }
    /// Step player presence across trigger zones and dispatch on_enter / on_exit rules.
    pub fn step_triggers(&mut self, controller: &Controller, player_id: u64) {
        if self.state.completed || self.trigger_zones.is_empty() {
            return;
        }
        let prev_mask = self.player_zones.get(&player_id).copied().unwrap_or(0);
        let mut curr_mask = 0u16;

        for (i, zone) in self.trigger_zones.iter().enumerate() {
            if zone.overlaps_body(
                controller.position,
                controller.feet_height(),
                controller.body_height(),
                self.document.player_profile.radius,
            ) {
                curr_mask |= 1 << i;
            }
        }

        for i in 0..self.trigger_zones.len() {
            let was_in = (prev_mask & (1 << i)) != 0;
            let is_in = (curr_mask & (1 << i)) != 0;
            let enabled = (self.state.enabled_zones & (1 << i)) != 0;

            if !was_in && is_in && enabled {
                self.fire_zone_rules(i, true);
            } else if was_in && !is_in && enabled {
                self.fire_zone_rules(i, false);
            }
            if self.state.completed {
                break;
            }
        }

        self.player_zones.insert(player_id, curr_mask);
    }
    fn fire_zone_rules(&mut self, zone_index: usize, is_enter: bool) {
        let rules_list = if is_enter {
            &self.zone_rules_enter[zone_index]
        } else {
            &self.zone_rules_exit[zone_index]
        };
        for rule in rules_list {
            if rule.once && self.state.fired & (1 << rule.index) != 0 {
                continue;
            }
            if rule
                .condition
                .is_some_and(|(i, v)| self.state.counters[i] != v)
            {
                continue;
            }
            apply_effects(&mut self.state, &rule.effects);
            if rule.once {
                self.state.fired |= 1 << rule.index;
            }
            if self.state.completed {
                break;
            }
        }
    }
    pub fn forget_player(&mut self, player_id: u64) {
        self.player_zones.remove(&player_id);
    }
    /// Round-robin named spawns; IDs are server-issued and one-based.
    pub fn controller(&self, player_id: u64) -> Controller {
        let spawn = &self.document.spawn_points
            [player_id.saturating_sub(1) as usize % self.document.spawn_points.len()];
        Controller::for_profile(self.document.player_profile, spawn.feet, spawn.yaw)
            .expect("Validated game spawn/profile")
    }
    pub fn content_hash(&self, room: &Room) -> u64 {
        let mut semantic = self.document.clone();
        semantic.map.clear(); // File location is not gameplay; loaded map content is hashed instead.
        serde_json::to_vec(&semantic)
            .expect("Serializable game data")
            .iter()
            .fold(super::content::fingerprint(room), |hash, b| {
                (hash ^ u64::from(*b)).wrapping_mul(0x100000001b3)
            })
    }
}

impl LoadedGame {
    pub fn world(self) -> Result<super::simulation::HeadlessWorld> {
        let runtime = GameRuntime::compile(self.document, &self.map)?;
        let room = self.map.build()?;
        let hash = runtime.content_hash(&room);
        let mut world = super::simulation::HeadlessWorld::try_with_room(room)?;
        world.content_hash = hash;
        world.game = Some(runtime);
        Ok(world)
    }
}
