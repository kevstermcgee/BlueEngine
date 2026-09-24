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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub counter: String,
    pub equals: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum GameAction {
    Increment { counter: String, amount: i32 },
    SetCounter { counter: String, value: i32 },
    SetEnabled { entity: String, enabled: bool },
    Complete,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    /// None matches interaction with any declared, enabled target.
    pub on_interact: Option<String>,
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
            || self.interactables.is_empty()
            || self.interactables.len() > 16
            || self.rules.is_empty()
            || self.rules.len() > 16
        {
            return Err("Game v1: version=1, name 1..100 bytes, spawns 1..8, counters <=8, interactables/rules 1..16".into());
        }
        if !unique(self.spawn_points.iter().map(|s| s.id.as_str()))
            || !unique(self.interactables.iter().map(|s| s.entity.as_str()))
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
        let target_exists = |s: &str| self.interactables.iter().any(|i| i.entity == s);
        for rule in &self.rules {
            if rule.actions.is_empty()
                || rule.actions.len() > 4
                || rule.on_interact.as_ref().is_some_and(|s| !target_exists(s))
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
                    GameAction::SetEnabled { entity, .. } => target_exists(entity),
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
    Enable(usize, bool),
    Complete,
}
struct CompiledRule {
    index: usize,
    once: bool,
    condition: Option<(usize, i32)>,
    effects: Vec<Effect>,
}

pub struct GameRuntime {
    document: GameDocument,
    state: GameState,
    last_snapshot: Option<u64>,
    targets: Vec<super::controller::Collider>,
    rules: Vec<Vec<CompiledRule>>,
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
        let rules = document
            .interactables
            .iter()
            .map(|target| {
                document
                    .rules
                    .iter()
                    .enumerate()
                    .filter(|(_, rule)| {
                        rule.on_interact
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
                        effects: rule
                            .actions
                            .iter()
                            .map(|a| match a {
                                GameAction::Increment { counter, amount } => {
                                    Effect::Increment(counter_index[counter.as_str()], *amount)
                                }
                                GameAction::SetCounter { counter, value } => {
                                    Effect::Set(counter_index[counter.as_str()], *value)
                                }
                                GameAction::SetEnabled { entity, enabled } => {
                                    Effect::Enable(target_index[entity.as_str()], *enabled)
                                }
                                GameAction::Complete => Effect::Complete,
                            })
                            .collect(),
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
        let state = GameState {
            counters: document.counters.values().copied().collect(),
            enabled,
            ..Default::default()
        };
        let targets = document
            .interactables
            .iter()
            .map(|t| map.colliders[&t.entity].clone())
            .collect();
        Ok(Self {
            document,
            state,
            last_snapshot: None,
            targets,
            rules,
        })
    }
    pub fn state(&self) -> &GameState {
        &self.state
    }
    pub fn document(&self) -> &GameDocument {
        &self.document
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
            for action in &rule.effects {
                match *action {
                    Effect::Increment(i, amount) => {
                        self.state.counters[i] =
                            (self.state.counters[i] + amount).clamp(-MAX_COUNTER, MAX_COUNTER)
                    }
                    Effect::Set(i, value) => self.state.counters[i] = value,
                    Effect::Enable(i, enabled) => {
                        if enabled {
                            self.state.enabled |= 1 << i;
                        } else {
                            self.state.enabled &= !(1 << i);
                        }
                    }
                    Effect::Complete => self.state.completed = true,
                }
            }
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
