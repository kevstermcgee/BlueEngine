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
    path::{Component, Path, PathBuf},
};

pub const MAX_COUNTER: i32 = 1_000_000;
/// Byte limits for a game document and its map when loaded from disk.
pub const MAX_GAME_BYTES: u64 = 64_000;
pub const MAX_MAP_BYTES: u64 = 8_000_000;
pub const INTERACT_REACH: f32 = 2.5;
pub const MAX_GAME_COUNTERS: usize = 32;
pub const MAX_GAME_FLAGS: usize = 64;

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema-generation", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema-generation", schemars(transform = crate::authoring_schemas::trigger_zone))]
pub struct TriggerZone {
    pub id: String,
    pub bounds: super::controller::Collider,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema-generation", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema-generation", schemars(transform = crate::authoring_schemas::spawn_point))]
pub struct SpawnPoint {
    pub id: String,
    pub feet: V,
    pub yaw: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema-generation", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema-generation", schemars(transform = crate::authoring_schemas::interactable))]
pub struct Interactable {
    pub entity: String,
    pub enabled: bool,
    /// Presentation state only. Hidden targets retain collision and interaction eligibility.
    #[serde(default = "default_true")]
    pub visible: bool,
}

/// A rule guard over counters. Exactly one form is used per node:
/// a leaf (`counter` plus one or more comparisons, all of which must hold), or a compound
/// (`all`, `any`, `not`). `{"counter": "x", "equals": 1}` is the original form and still works.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema-generation", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema-generation", schemars(transform = crate::authoring_schemas::condition))]
#[cfg_attr(feature = "schema-generation", schemars(rename = "condition"))]
pub struct Condition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counter: Option<String>,
    /// Compare `counter mod modulo` (always 0..modulo) instead of the raw value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modulo: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equals: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_equals: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub less_than: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub greater_than: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_most: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_least: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all: Option<Vec<Condition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub any: Option<Vec<Condition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not: Option<Box<Condition>>,
}

/// Bounds that keep a condition cheap to evaluate every tick and small in the document.
pub const MAX_CONDITION_DEPTH: usize = 4;
pub const MAX_CONDITION_NODES: usize = 16;

impl Condition {
    /// The original single-comparison guard.
    pub fn counter_equals(counter: &str, value: i32) -> Self {
        Self {
            counter: Some(counter.into()),
            equals: Some(value),
            ..Self::default()
        }
    }

    fn comparisons(&self) -> [(Cmp, Option<i32>); 6] {
        [
            (Cmp::Eq, self.equals),
            (Cmp::Ne, self.not_equals),
            (Cmp::Lt, self.less_than),
            (Cmp::Gt, self.greater_than),
            (Cmp::Le, self.at_most),
            (Cmp::Ge, self.at_least),
        ]
    }

    /// Check shape, references and limits; `nodes` counts every node visited so far.
    fn validate(
        &self,
        counters: &BTreeMap<String, i32>,
        depth: usize,
        nodes: &mut usize,
    ) -> std::result::Result<(), String> {
        *nodes += 1;
        if depth > MAX_CONDITION_DEPTH || *nodes > MAX_CONDITION_NODES {
            return Err(format!(
                "condition nests deeper than {MAX_CONDITION_DEPTH} or has more than {MAX_CONDITION_NODES} parts"
            ));
        }
        let in_range = |v: i32| (-MAX_COUNTER..=MAX_COUNTER).contains(&v);
        let leaf_fields = self.counter.is_some()
            || self.modulo.is_some()
            || self.comparisons().iter().any(|(_, v)| v.is_some());
        let forms = leaf_fields as usize
            + self.all.is_some() as usize
            + self.any.is_some() as usize
            + self.not.is_some() as usize;
        if forms != 1 {
            return Err(
                "a condition uses exactly one form: counter with comparisons, all, any or not"
                    .into(),
            );
        }
        if leaf_fields {
            let counter = self
                .counter
                .as_ref()
                .ok_or("comparisons and modulo need a counter")?;
            if !counters.contains_key(counter) {
                return Err(format!("condition references unknown counter {counter}"));
            }
            let tests = self.comparisons();
            if tests.iter().all(|(_, v)| v.is_none()) {
                return Err(format!("condition on {counter} has no comparison"));
            }
            if tests.iter().any(|(_, v)| v.is_some_and(|v| !in_range(v)))
                || self.modulo.is_some_and(|m| !(1..=MAX_COUNTER).contains(&m))
            {
                return Err(format!("condition on {counter} has a value out of range"));
            }
        }
        for group in [&self.all, &self.any].into_iter().flatten() {
            if group.is_empty() || group.len() > MAX_CONDITION_NODES {
                return Err("all/any need at least one condition".into());
            }
            for child in group {
                child.validate(counters, depth + 1, nodes)?;
            }
        }
        if let Some(child) = &self.not {
            child.validate(counters, depth + 1, nodes)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
enum Cmp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

/// A validated condition with counter names resolved to indices.
#[derive(Clone, Debug)]
enum CompiledCondition {
    Leaf {
        counter: usize,
        modulo: Option<i32>,
        tests: Vec<(Cmp, i32)>,
    },
    All(Vec<CompiledCondition>),
    Any(Vec<CompiledCondition>),
    Not(Box<CompiledCondition>),
}

impl CompiledCondition {
    /// The document must already have passed [`GameDocument::validate`].
    fn compile(c: &Condition, counter_index: &BTreeMap<&str, usize>) -> Self {
        if let Some(name) = &c.counter {
            return Self::Leaf {
                counter: counter_index[name.as_str()],
                modulo: c.modulo,
                tests: c
                    .comparisons()
                    .into_iter()
                    .filter_map(|(cmp, v)| v.map(|v| (cmp, v)))
                    .collect(),
            };
        }
        let list = |v: &[Condition]| v.iter().map(|c| Self::compile(c, counter_index)).collect();
        if let Some(all) = &c.all {
            Self::All(list(all))
        } else if let Some(any) = &c.any {
            Self::Any(list(any))
        } else {
            Self::Not(Box::new(Self::compile(
                c.not.as_deref().expect("validated condition form"),
                counter_index,
            )))
        }
    }

    fn holds(&self, counters: &[i32]) -> bool {
        match self {
            Self::Leaf {
                counter,
                modulo,
                tests,
            } => {
                let raw = counters[*counter];
                let value = modulo.map_or(raw, |m| raw.rem_euclid(m));
                tests.iter().all(|&(cmp, rhs)| match cmp {
                    Cmp::Eq => value == rhs,
                    Cmp::Ne => value != rhs,
                    Cmp::Lt => value < rhs,
                    Cmp::Gt => value > rhs,
                    Cmp::Le => value <= rhs,
                    Cmp::Ge => value >= rhs,
                })
            }
            Self::All(list) => list.iter().all(|c| c.holds(counters)),
            Self::Any(list) => list.iter().any(|c| c.holds(counters)),
            Self::Not(c) => !c.holds(counters),
        }
    }
}

fn default_mover_duration() -> u32 {
    60
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema-generation", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema-generation", schemars(transform = crate::authoring_schemas::mover))]
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
#[cfg_attr(feature = "schema-generation", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema-generation", schemars(transform = crate::authoring_schemas::timer))]
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
#[cfg_attr(feature = "schema-generation", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema-generation", schemars(transform = crate::authoring_schemas::game_action))]
pub enum GameAction {
    Increment {
        counter: String,
        amount: i32,
    },
    SetCounter {
        counter: String,
        value: i32,
    },
    SetEnabled {
        entity: String,
        enabled: bool,
    },
    SetVisible {
        entity: String,
        visible: bool,
    },
    SetMover {
        mover: String,
        open: bool,
    },
    StartTimer {
        timer: String,
    },
    StopTimer {
        timer: String,
    },
    /// End the match as won.
    Complete,
    /// End the match as lost. Like `complete`, later events are ignored until the game restarts.
    Fail,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema-generation", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema-generation", schemars(transform = crate::authoring_schemas::rule))]
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
#[cfg_attr(feature = "schema-generation", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema-generation", schemars(transform = crate::authoring_schemas::game_document))]
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
    /// Optional stock HUD configuration. Missing preserves legacy presentation and serialization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<super::stock_presentation::StockPresentation>,
}

#[derive(Clone)]
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

/// A game box (an interactable or a mover) is described by three records in the map that must all be named
/// after it and agree: a scene node, a collider and an entity. Say exactly what is missing or differs.
fn check_game_box(map: &MapDocument, id: &str, kind: &str) -> Result<()> {
    let entity = map.entities.iter().find(|e| e.id == id);
    let node = map.scene.nodes.iter().find(|n| n.id == id);
    let collider = map.colliders.get(id);
    let (Some(entity), Some(node), Some(collider)) = (entity, node, collider) else {
        let state = |present: bool| if present { "found" } else { "MISSING" };
        let fix = if kind == "Interactable" {
            format!("`be2-tools add-interactable GAME.json {id} --at=X,Y,Z` creates all of them and the game.json entry together")
        } else {
            "add the box with a non-structural add_box patch, which creates all three".to_string()
        };
        return Err(format!(
            "{kind} '{id}' needs three matching records in the map, each named '{id}': scene node ({}), collider ({}), entity ({}). {fix}.",
            state(node.is_some()),
            state(collider.is_some()),
            state(entity.is_some())
        )
        .into());
    };
    let (Track::Fixed(center), Track::Fixed(half), Track::Fixed(rotation)) =
        (&node.pos, &node.scale, &node.rot)
    else {
        return Err(format!(
            "{kind} '{id}' has an animated position, scale or rotation; game boxes must be fixed."
        )
        .into());
    };
    if !matches!(node.shape, Shape::Box) {
        return Err(format!(
            "{kind} '{id}' is not a box node; game boxes must be axis-aligned boxes."
        )
        .into());
    }
    if *rotation != V::ZERO {
        return Err(format!("{kind} '{id}' is rotated; game boxes must be axis-aligned.").into());
    }
    if node.material.starts_with("prop-") || node.material.starts_with("decor-") {
        return Err(format!(
            "{kind} '{id}' uses the prop/decor material '{}'; game boxes must be plain boxes.",
            node.material
        )
        .into());
    }
    let (lo, hi) = (*center - *half, *center + *half);
    if collider.min != lo || collider.max != hi {
        return Err(format!(
            "{kind} '{id}': the collider {:?}..{:?} does not match the node box {lo:?}..{hi:?}.",
            collider.min, collider.max
        )
        .into());
    }
    if entity.bounds.min != collider.min || entity.bounds.max != collider.max {
        return Err(format!(
            "{kind} '{id}': the entity bounds {:?}..{:?} do not match the collider {:?}..{:?}.",
            entity.bounds.min, entity.bounds.max, collider.min, collider.max
        )
        .into());
    }
    Ok(())
}

impl GameDocument {
    /// The confined map path a game document at `path` refers to.
    pub fn map_path(&self, path: &Path) -> Result<PathBuf> {
        let root = path
            .canonicalize()?
            .parent()
            .ok_or("Game needs a parent directory")?
            .to_path_buf();
        let relative = Path::new(&self.map);
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
        Ok(map_path)
    }

    pub fn load(path: &Path) -> Result<LoadedGame> {
        let mut document: Self = serde_json::from_slice(&read_bounded(path, MAX_GAME_BYTES)?)?;
        let map_path = document.map_path(path)?;
        let map: MapDocument = serde_json::from_slice(&read_bounded(&map_path, MAX_MAP_BYTES)?)?;
        document.validate(&map)?;
        if let Some(audio) = document
            .presentation
            .as_mut()
            .and_then(|p| p.audio.as_mut())
        {
            audio.root = path.canonicalize()?.parent().map(Path::to_path_buf);
        }
        Ok(LoadedGame { document, map })
    }

    pub fn validate(&self, map: &MapDocument) -> Result<()> {
        map.validate()?;
        self.player_profile.validate()?;
        if let Some(presentation) = &self.presentation {
            presentation.validate(&self.counters)?;
        }
        if self.schema_version != 1
            || self.name.is_empty()
            || self.name.len() > 100
            || self.spawn_points.is_empty()
            || self.spawn_points.len() > 8
            || self.counters.len() > MAX_GAME_COUNTERS
            || (self.interactables.is_empty() && self.trigger_zones.is_empty())
            || self.interactables.len() > MAX_GAME_FLAGS
            || self.trigger_zones.len() > MAX_GAME_FLAGS
            || self.movers.len() > MAX_GAME_FLAGS
            || self.timers.len() > MAX_GAME_FLAGS
            || self.rules.is_empty()
            || self.rules.len() > MAX_GAME_FLAGS
        {
            return Err("Game v1: version=1, name 1..100 bytes, spawns 1..8, counters <=32, interactables/zones <=64 (at least 1 total), movers <=64, timers <=64, rules 1..64".into());
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
            check_game_box(map, &target.entity, "Interactable")?;
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
            check_game_box(map, &mover.entity, "Mover")?;
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
            {
                return Err(format!("Invalid rule references/limits: {}", rule.id).into());
            }
            if let Some(condition) = &rule.condition {
                condition
                    .validate(&self.counters, 0, &mut 0)
                    .map_err(|e| format!("Invalid condition in {}: {e}", rule.id))?;
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
                    GameAction::Complete | GameAction::Fail => true,
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
    /// Authoritative restart generation; clients discard prediction across rounds.
    #[serde(default)]
    pub round: u64,
    /// Authoritative mover positions in fixed ticks. Mirrors never advance rules.
    #[serde(default)]
    pub mover_ticks: Vec<u32>,
    pub counters: Vec<i32>,
    pub enabled: u64,
    /// Visible interactable geometry. This never changes collision or eligibility.
    #[serde(default, rename = "v", alias = "visible")]
    pub visible: u64,
    #[serde(default, rename = "z", alias = "enabled_zones")]
    pub enabled_zones: u64,
    #[serde(default)]
    pub mover_targets: u64,
    #[serde(default)]
    pub active_timers: u64,
    pub fired: u64,
    pub completed: bool,
    /// The match ended in a loss. Omitted from JSON while false, so saves, packets and hashes of
    /// games that never fail are unchanged.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub failed: bool,
}

impl GameState {
    /// Won or lost: the match accepts no further events until it restarts.
    pub fn finished(&self) -> bool {
        self.completed || self.failed
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameEvent {
    pub player_id: u64,
    pub entity: String,
}

/// A mover's box before and after one tick of motion. It is a pure translation.
#[derive(Clone, Debug)]
pub struct MoverMotion {
    pub from: super::controller::Collider,
    pub to: super::controller::Collider,
}

/// One thing that can happen to a game, at the granularity the rules can tell apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelEvent {
    Interact(usize),
    TimerExpires(usize),
    EnterZone(usize),
    ExitZone(usize),
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
    Fail,
}
struct CompiledRule {
    index: usize,
    once: bool,
    condition: Option<CompiledCondition>,
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
            Effect::Fail => state.failed = true,
        }
        if state.finished() {
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

fn flag_mask(count: usize) -> u64 {
    if count >= MAX_GAME_FLAGS {
        u64::MAX
    } else {
        (1_u64 << count) - 1
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
    player_zones: BTreeMap<u64, u64>,
    /// When set, the document index of every rule that fires is appended (model checking only).
    fired_log: Option<Vec<usize>>,
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
                    GameAction::Fail => Effect::Fail,
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
                            .map(|c| CompiledCondition::compile(c, &counter_index)),
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
                            .map(|c| CompiledCondition::compile(c, &counter_index)),
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
                            .map(|c| CompiledCondition::compile(c, &counter_index)),
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
                            .map(|c| CompiledCondition::compile(c, &counter_index)),
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
            round: 0,
            mover_ticks: document
                .movers
                .iter()
                .map(|m| if m.initial_open { m.duration_ticks } else { 0 })
                .collect(),
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
            fired_log: None,
        })
    }
    pub(crate) fn set_round(&mut self, round: u64) {
        self.state.round = round;
    }
    pub fn state(&self) -> &GameState {
        &self.state
    }
    /// Start recording which rules fire (see [`Self::model_apply`]).
    pub fn record_fired_rules(&mut self) {
        self.fired_log = Some(Vec::new());
    }
    /// The events that can happen next, as a model checker sees them. Time and movement are
    /// abstracted away: any enabled target can be pressed, any running timer can run out, and the
    /// player can enter or leave any enabled zone, in any order. `occupied` is the bit set of zones
    /// the player is currently inside.
    pub fn model_events(&self, occupied: u64) -> Vec<ModelEvent> {
        if self.state.finished() {
            return Vec::new();
        }
        let mut events = Vec::new();
        events.extend(
            (0..self.targets.len())
                .filter(|&i| self.state.enabled & (1 << i) != 0)
                .map(ModelEvent::Interact),
        );
        events.extend(
            (0..self.timers.len())
                .filter(|&i| self.state.active_timers & (1 << i) != 0)
                .map(ModelEvent::TimerExpires),
        );
        for i in 0..self.trigger_zones.len() {
            let enabled = self.state.enabled_zones & (1 << i) != 0;
            match (occupied & (1 << i) != 0, enabled) {
                (false, true) => events.push(ModelEvent::EnterZone(i)),
                (true, true) => events.push(ModelEvent::ExitZone(i)),
                _ => {}
            }
        }
        events
    }
    /// Apply one event from [`Self::model_events`] to the current state, updating `occupied`.
    pub fn model_apply(&mut self, event: ModelEvent, occupied: &mut u64) {
        match event {
            ModelEvent::Interact(i) => self.fire_target_rules(i),
            ModelEvent::TimerExpires(i) => self.expire_timer(i),
            ModelEvent::EnterZone(i) => {
                *occupied |= 1 << i;
                self.fire_zone_rules(i, true);
            }
            ModelEvent::ExitZone(i) => {
                *occupied &= !(1 << i);
                self.fire_zone_rules(i, false);
            }
        }
    }
    /// Replace the rule state (the model checker jumps between states of one runtime).
    pub fn model_load(&mut self, state: &GameState) {
        self.state = state.clone();
    }
    /// Take the document indices of the rules that fired since the last call.
    pub fn take_fired_rules(&mut self) -> Vec<usize> {
        self.fired_log
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }
    /// A human name for an event, for reports.
    pub fn model_event_name(&self, event: ModelEvent) -> String {
        match event {
            ModelEvent::Interact(i) => format!("press {}", self.document.interactables[i].entity),
            ModelEvent::TimerExpires(i) => format!("timer {} runs out", self.document.timers[i].id),
            ModelEvent::EnterZone(i) => format!("enter {}", self.document.trigger_zones[i].id),
            ModelEvent::ExitZone(i) => format!("leave {}", self.document.trigger_zones[i].id),
        }
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
        if self.state.finished() {
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
            self.expire_timer(i);
            if self.state.finished() {
                break;
            }
        }
    }
    /// A timer runs out: it restarts (or stops, if it does not repeat) and its rules fire.
    fn expire_timer(&mut self, index: usize) {
        self.timers[index].remaining_ticks = self.timers[index].duration_ticks;
        if !self.timers[index].repeats {
            self.state.active_timers &= !(1 << index);
        }
        self.fire_timer_rules(index);
    }
    fn fire_timer_rules(&mut self, timer_index: usize) {
        for rule in &self.timer_rules[timer_index] {
            if rule.once && self.state.fired & (1 << rule.index) != 0 {
                continue;
            }
            if rule
                .condition
                .as_ref()
                .is_some_and(|c| !c.holds(&self.state.counters))
            {
                continue;
            }
            apply_effects(&mut self.state, &rule.effects);
            if let Some(log) = &mut self.fired_log {
                log.push(rule.index);
            }
            if rule.once {
                self.state.fired |= 1 << rule.index;
            }
            if self.state.finished() {
                break;
            }
        }
    }
    /// Advance every mover one tick and return the ones that moved, so the caller can carry or push the
    /// players they touch (see `Controller::ride`). Callers that own no players may ignore the result.
    pub fn step_movers(&mut self, room: &mut Room) -> Vec<MoverMotion> {
        let mut moved = Vec::new();
        for (i, mover) in self.movers.iter_mut().enumerate() {
            let target_open = self.state.mover_targets & (1 << i) != 0;
            let from = mover.current_bounds();
            if target_open && mover.current_ticks < mover.duration_ticks {
                mover.current_ticks += 1;
            } else if !target_open && mover.current_ticks > 0 {
                mover.current_ticks -= 1;
            }
            let to = mover.current_bounds();
            if from.min != to.min {
                moved.push(MoverMotion { from, to });
            }
        }
        self.state.mover_ticks = self.movers.iter().map(|m| m.current_ticks).collect();
        self.apply_mover_colliders(room);
        moved
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
    /// True when `state` fits this game: right counts, in-range values, no flag beyond the rules.
    fn state_is_valid(&self, state: &GameState) -> bool {
        state.mover_ticks.len() == self.movers.len()
            && state
                .mover_ticks
                .iter()
                .zip(&self.movers)
                .all(|(t, m)| *t <= m.duration_ticks)
            && state.counters.len() == self.document.counters.len()
            && state
                .counters
                .iter()
                .all(|v| (-MAX_COUNTER..=MAX_COUNTER).contains(v))
            && state.enabled & !flag_mask(self.targets.len()) == 0
            && state.visible & !flag_mask(self.targets.len()) == 0
            && state.enabled_zones & !flag_mask(self.trigger_zones.len()) == 0
            && state.mover_targets & !flag_mask(self.movers.len()) == 0
            && state.active_timers & !flag_mask(self.timers.len()) == 0
            && state.fired & !flag_mask(self.document.rules.len()) == 0
    }
    /// Complete rule state for a save: the public state plus timer countdowns and zone occupancy.
    pub(crate) fn capture(&self) -> super::savestate::world::GameSave {
        super::savestate::world::GameSave {
            state: self.state.clone(),
            timers: self.timers.iter().map(|t| t.remaining_ticks).collect(),
            zones: self.player_zones.iter().map(|(&p, &m)| (p, m)).collect(),
        }
    }
    /// Check that a saved rule state belongs to this game. Nothing is changed.
    pub(crate) fn check_save(
        &self,
        save: &super::savestate::world::GameSave,
    ) -> std::result::Result<(), String> {
        if !self.state_is_valid(&save.state) {
            return Err(
                "the saved rule state does not fit this game's counters, movers and flags".into(),
            );
        }
        if save.timers.len() != self.timers.len() {
            return Err(format!(
                "the save has {} timers, this game has {}",
                save.timers.len(),
                self.timers.len()
            ));
        }
        if save
            .timers
            .iter()
            .zip(&self.timers)
            .any(|(left, t)| *left > t.duration_ticks)
        {
            return Err("a saved timer has more time left than its duration".into());
        }
        if save
            .zones
            .iter()
            .any(|(_, mask)| mask & !flag_mask(self.trigger_zones.len()) != 0)
        {
            return Err("a saved player is inside a trigger zone this game does not have".into());
        }
        Ok(())
    }
    /// Apply a save that [`Self::check_save`] accepted.
    pub(crate) fn restore(&mut self, save: &super::savestate::world::GameSave, room: &mut Room) {
        for (mover, ticks) in self.movers.iter_mut().zip(&save.state.mover_ticks) {
            mover.current_ticks = *ticks;
        }
        self.state = save.state.clone();
        for (timer, left) in self.timers.iter_mut().zip(&save.timers) {
            timer.remaining_ticks = *left;
        }
        self.player_zones = save.zones.iter().copied().collect();
        self.last_snapshot = None;
        self.apply_mover_colliders(room);
    }
    /// Validate a full authoritative state before replacing the client mirror.
    fn accept_state(&mut self, state: GameState) -> bool {
        if !self.state_is_valid(&state) {
            return false;
        }
        for (mover, ticks) in self.movers.iter_mut().zip(&state.mover_ticks) {
            mover.current_ticks = *ticks;
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
        if self.state.finished() {
            return None;
        }
        let target = self.target(room, controller)?;
        if !self.enabled(target) {
            return None;
        }
        self.fire_target_rules(target);
        Some(GameEvent {
            player_id,
            entity: self.document.interactables[target].entity.clone(),
        })
    }
    fn fire_target_rules(&mut self, target: usize) {
        for rule in &self.rules[target] {
            if rule.once && self.state.fired & (1 << rule.index) != 0 {
                continue;
            }
            if rule
                .condition
                .as_ref()
                .is_some_and(|c| !c.holds(&self.state.counters))
            {
                continue;
            }
            apply_effects(&mut self.state, &rule.effects);
            if let Some(log) = &mut self.fired_log {
                log.push(rule.index);
            }
            if rule.once {
                self.state.fired |= 1 << rule.index;
            }
            if self.state.finished() {
                break;
            }
        }
    }
    /// Step player presence across trigger zones and dispatch on_enter / on_exit rules.
    pub fn step_triggers(&mut self, controller: &Controller, player_id: u64) {
        if self.state.finished() || self.trigger_zones.is_empty() {
            return;
        }
        let prev_mask = self.player_zones.get(&player_id).copied().unwrap_or(0);
        let mut curr_mask = 0u64;

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
            if self.state.finished() {
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
                .as_ref()
                .is_some_and(|c| !c.holds(&self.state.counters))
            {
                continue;
            }
            apply_effects(&mut self.state, &rule.effects);
            if let Some(log) = &mut self.fired_log {
                log.push(rule.index);
            }
            if rule.once {
                self.state.fired |= 1 << rule.index;
            }
            if self.state.finished() {
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
        let original = self.clone();
        let runtime = GameRuntime::compile(self.document, &self.map)?;
        let room = self.map.build()?;
        let hash = runtime.content_hash(&room);
        let mut world = super::simulation::HeadlessWorld::try_with_room(room)?;
        world.content_hash = hash;
        world.game = Some(runtime);
        world.initial_game = Some(original);
        Ok(world)
    }
}
