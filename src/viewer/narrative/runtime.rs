use super::model::*;
use crate::viewer::devkit::StateHasher;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_QUEUE: usize = 8;
pub const STATE_VERSION: u32 = 1;

/// Pure typed reads of the authoritative game. Implementations must not use interior
/// mutation, randomness, a wall clock, or change any game fact while answering.
pub trait View {
    fn query(&self, id: &str) -> Option<Value>;
    fn speaker_present(&self, id: &str) -> bool;
}

/// Game-owned transaction boundary. Validate every command against one current
/// state, then apply in source order, all or none. An `Err` must leave the game
/// unchanged. A small game can stage a clone and replace it after validation.
pub trait Commands: View {
    fn apply_atomic(&mut self, commands: &[Command]) -> Result<(), String>;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    pub id: u64,
    pub event: String,
    pub expires_at: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseHistory {
    pub selected: u32,
    pub presented: u32,
    pub completed: u32,
    pub last_selected: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveResponse {
    pub id: String,
    pub token: u64,
    pub expires_at: u64,
    pub presented: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneProgress {
    pub scene: String,
    pub node: String,
    pub token: u64,
}

/// Run-lifetime authoritative state. Embed this in the game's existing Snapshot
/// payload, version that payload, and include `state_hash()` in its simulation hash.
/// Presentation's reveal cursor/audio playback are deliberately game-owned and transient.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeState {
    pub version: u32,
    pub history: BTreeMap<String, ResponseHistory>,
    pub speaker_ready: BTreeMap<String, u64>,
    pub category_ready: BTreeMap<String, u64>,
    pub queue: Vec<Context>,
    pub active_response: Option<ActiveResponse>,
    pub scene: Option<SceneProgress>,
    /// Counts of committed choices, keyed by scene/node/choice IDs. Not gameplay flags.
    pub decisions: BTreeMap<String, u32>,
    pub completed_scenes: BTreeMap<String, u32>,
    pub last_context_id: u64,
    pub next_token: u64,
    pub generation: u64,
}

impl Default for NarrativeState {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            history: BTreeMap::new(),
            speaker_ready: BTreeMap::new(),
            category_ready: BTreeMap::new(),
            queue: vec![],
            active_response: None,
            scene: None,
            decisions: BTreeMap::new(),
            completed_scenes: BTreeMap::new(),
            last_context_id: 0,
            next_token: 1,
            generation: 0,
        }
    }
}

impl NarrativeState {
    pub fn state_hash(&self) -> u64 {
        // Only finite integers/strings/bools and ordered maps occur in this state.
        // serde's representation is also the one embedded in the engine Snapshot.
        let bytes = serde_json::to_vec(self).expect("narrative state contains serializable values");
        StateHasher::new().bytes(&bytes).finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CandidateDiagnostic {
    pub id: String,
    pub eligible: bool,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SelectionReport {
    pub selected: Option<String>,
    pub candidates: Vec<CandidateDiagnostic>,
}

/// An optimistic selection from one immutable view. Committing rechecks eligibility.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub response: String,
    pub context: Context,
    generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Presentation {
    pub token: u64,
    pub id: String,
    pub speaker: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PresentedChoice {
    pub id: String,
    pub text: String,
    pub kind: ChoiceKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SceneView {
    pub token: u64,
    pub scene: String,
    pub node: String,
    pub speaker: String,
    pub text: String,
    pub choices: Vec<PresentedChoice>,
}

/// Parsed/validated content and event indices, plus only narrative-owned state.
/// There is no tick method: games call this on relevant events or input intentions.
pub struct Runtime {
    content: Content,
    registry: Registry,
    events: BTreeMap<String, Vec<usize>>,
    templates: BTreeMap<String, TextTemplate>,
    state: NarrativeState,
}

impl Runtime {
    pub fn new(content: Content, registry: Registry) -> Result<Self, Vec<Diagnostic>> {
        content.validate(&registry)?;
        let events = event_index(&content);
        let templates = compile_templates(&content, &registry);
        Ok(Self {
            content,
            registry,
            events,
            templates,
            state: NarrativeState::default(),
        })
    }
    pub fn content(&self) -> &Content {
        &self.content
    }
    pub fn state(&self) -> &NarrativeState {
        &self.state
    }

    /// Restore validation occurs before mutation. Removing an in-use capability or
    /// referenced content requires an explicit game Snapshot migration/restart.
    pub fn restore_state(&mut self, state: NarrativeState) -> Result<(), String> {
        validate_state(&state, &self.content)?;
        self.state = state;
        Ok(())
    }

    /// Content loading/watching belongs to the developer tool or game. Call only
    /// between authoritative ticks/input commits; failure keeps content and state.
    /// Text-only edits preserve active progress. Changed active scene semantics or
    /// removed historical IDs require a deliberate restart/migration outside here.
    pub fn reload(
        &mut self,
        content: Content,
        registry: &Registry,
        safe_boundary: bool,
    ) -> Result<(), String> {
        if !safe_boundary {
            return Err("reload requires an authoritative safe boundary".into());
        }
        content.validate(registry).map_err(|errors| {
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        if registry != &self.registry {
            return Err("binding changes require a game restart".into());
        }
        validate_state(&self.state, &content)?;
        if let Some(active) = &self.state.scene {
            let old = self.scene(&active.scene).expect("validated state");
            let new = content
                .scenes
                .iter()
                .find(|scene| scene.id == active.scene)
                .ok_or("active scene removed")?;
            if scene_semantics(old) != scene_semantics(new) {
                return Err(
                    "active scene semantics changed; finish or deliberately restart the scene"
                        .into(),
                );
            }
        }
        if let Some(active) = &self.state.active_response {
            let old = self.response(&active.id).expect("validated state");
            let new = content
                .responses
                .iter()
                .find(|response| response.id == active.id)
                .ok_or("active response removed")?;
            let mut old = old.clone();
            let mut new = new.clone();
            old.text.clear();
            new.text.clear();
            old.source = Source::default();
            new.source = Source::default();
            old.text_source = Source::default();
            new.text_source = Source::default();
            for condition in old.conditions.iter_mut().chain(new.conditions.iter_mut()) {
                condition.source = Source::default();
            }
            if old != new {
                return Err("active response semantics changed; interrupt it before reload".into());
            }
        }
        self.state.generation = self
            .state
            .generation
            .checked_add(1)
            .ok_or("narrative generation exhausted")?;
        self.events = event_index(&content);
        self.templates = compile_templates(&content, registry);
        self.content = content;
        Ok(())
    }

    /// IDs are game-issued, strictly increasing per run; retries cannot enqueue an
    /// already accepted event. On full queues the caller may drop or retry the same
    /// ID after making room; rejection does not consume that ID.
    pub fn enqueue(&mut self, context: Context) -> Result<(), String> {
        if !valid_id(&context.event) {
            return Err("invalid event ID".into());
        }
        if context.id <= self.state.last_context_id {
            return Err("context ID was already accepted or is out of order".into());
        }
        if self.state.queue.len() >= MAX_QUEUE {
            return Err("context queue is full".into());
        }
        let generation = self
            .state
            .generation
            .checked_add(1)
            .ok_or("narrative generation exhausted")?;
        self.state.last_context_id = context.id;
        self.state.queue.push(context);
        self.state.generation = generation;
        Ok(())
    }

    /// Full diagnostics are opt-in and allocate only when requested.
    pub fn inspect(&self, event: &str, now: u64, view: &impl View) -> SelectionReport {
        let mut selected = None;
        let candidates = self
            .events
            .get(event)
            .into_iter()
            .flatten()
            .map(|index| {
                let response = &self.content.responses[*index];
                let reason = self.rejection(response, now, view);
                if reason.is_none() && selected.is_none() {
                    selected = Some(response.id.clone());
                }
                CandidateDiagnostic {
                    id: response.id.clone(),
                    eligible: reason.is_none(),
                    reason: reason.unwrap_or("eligible").to_owned(),
                }
            })
            .collect();
        SelectionReport {
            selected,
            candidates,
        }
    }

    /// Pure eligibility: no history, cooldown, queue, RNG or game changes.
    pub fn select(&self, context: &Context, now: u64, view: &impl View) -> Option<Selection> {
        if now > context.expires_at {
            return None;
        }
        let response = self
            .events
            .get(&context.event)?
            .iter()
            .map(|index| &self.content.responses[*index])
            .find(|response| self.rejection(response, now, view).is_none())?;
        Some(Selection {
            response: response.id.clone(),
            context: context.clone(),
            generation: self.state.generation,
        })
    }

    pub fn commit_response(
        &mut self,
        selection: &Selection,
        now: u64,
        view: &impl View,
    ) -> Result<Presentation, String> {
        if selection.generation != self.state.generation {
            return Err("stale response selection".into());
        }
        let queued = self
            .state
            .queue
            .iter()
            .position(|context| context == &selection.context)
            .ok_or("response context is no longer queued")?;
        let current = self
            .select(&selection.context, now, view)
            .ok_or("response is no longer eligible")?;
        if current.response != selection.response {
            return Err("response selection changed".into());
        }
        let response = self
            .response(&selection.response)
            .ok_or("unknown response")?;
        if let Some(active) = &self.state.active_response {
            let priority = self
                .response(&active.id)
                .ok_or("unknown active response")?
                .priority;
            if response.priority <= priority {
                return Err("active response has equal or greater interruption priority".into());
            }
        }
        let token = self.state.next_token;
        let next_token = token.checked_add(1).ok_or("narrative token exhausted")?;
        let generation = self
            .state
            .generation
            .checked_add(1)
            .ok_or("narrative generation exhausted")?;
        let presentation = Presentation {
            token,
            id: response.id.clone(),
            speaker: response.speaker.clone(),
            text: self.render(&response.text, view)?,
        };
        let speaker = response.speaker.clone();
        let category = response.category.clone();
        let cooldown = response.cooldown;
        let history = self
            .state
            .history
            .entry(selection.response.clone())
            .or_default();
        history.selected = history.selected.saturating_add(1);
        history.last_selected = Some(now);
        self.state
            .speaker_ready
            .insert(speaker, now.saturating_add(cooldown));
        self.state
            .category_ready
            .insert(category, now.saturating_add(cooldown));
        self.state.active_response = Some(ActiveResponse {
            id: selection.response.clone(),
            token,
            expires_at: selection.context.expires_at,
            presented: false,
        });
        self.state.queue.remove(queued);
        self.state.next_token = next_token;
        self.state.generation = generation;
        Ok(presentation)
    }

    /// Bounded oldest-context dispatch. Lines inside an event use priority desc,
    /// specificity (guard count) desc, then stable ID asc. No RNG is consumed.
    /// Silence is valid. Expired/absent active speakers are interrupted without
    /// becoming heard; lower-priority contexts wait until completion or expiry.
    pub fn dispatch(&mut self, now: u64, view: &impl View) -> Result<Option<Presentation>, String> {
        let before = self.state.queue.len();
        self.state.queue.retain(|context| now <= context.expires_at);
        let absent = self.state.active_response.as_ref().is_some_and(|active| {
            now > active.expires_at
                || self
                    .response(&active.id)
                    .is_none_or(|response| !view.speaker_present(&response.speaker))
        });
        if absent {
            self.state.active_response = None;
        }
        if before != self.state.queue.len() || absent {
            self.bump_generation()?;
        }
        for context in &self.state.queue {
            if let Some(selection) = self.select(context, now, view) {
                let priority = self
                    .response(&selection.response)
                    .expect("indexed response")
                    .priority;
                if self.state.active_response.as_ref().is_some_and(|active| {
                    self.response(&active.id)
                        .is_some_and(|response| response.priority >= priority)
                }) {
                    continue;
                }
                return self.commit_response(&selection, now, view).map(Some);
            }
        }
        Ok(None)
    }

    pub fn current_response(&self, view: &impl View) -> Result<Option<Presentation>, String> {
        let Some(active) = &self.state.active_response else {
            return Ok(None);
        };
        let response = self.response(&active.id).ok_or("unknown active response")?;
        if !view.speaker_present(&response.speaker) {
            return Ok(None);
        }
        Ok(Some(Presentation {
            token: active.token,
            id: response.id.clone(),
            speaker: response.speaker.clone(),
            text: self.render(&response.text, view)?,
        }))
    }
    pub fn presented_response(&mut self, token: u64) -> Result<(), String> {
        let active = self
            .state
            .active_response
            .as_mut()
            .filter(|active| active.token == token)
            .ok_or("stale response token")?;
        if !active.presented {
            active.presented = true;
            let history = self
                .state
                .history
                .get_mut(&active.id)
                .expect("selection owns history");
            history.presented = history.presented.saturating_add(1);
        }
        Ok(())
    }
    pub fn complete_response(&mut self, token: u64) -> Result<(), String> {
        let active = self
            .state
            .active_response
            .as_ref()
            .filter(|active| active.token == token)
            .ok_or("stale response token")?;
        if !active.presented {
            return Err("response was never presented".into());
        }
        let id = active.id.clone();
        self.bump_generation()?;
        let history = self
            .state
            .history
            .get_mut(&id)
            .expect("selection owns history");
        history.completed = history.completed.saturating_add(1);
        self.state.active_response = None;
        Ok(())
    }
    pub fn interrupt_response(&mut self, token: u64) -> Result<(), String> {
        if self
            .state
            .active_response
            .as_ref()
            .is_none_or(|active| active.token != token)
        {
            return Err("stale response token".into());
        }
        self.bump_generation()?;
        self.state.active_response = None;
        Ok(())
    }

    pub fn start_scene(&mut self, id: &str, view: &impl View) -> Result<(), String> {
        if self.state.scene.is_some() {
            return Err("a scene is already active".into());
        }
        let node = self
            .scene(id)
            .and_then(|scene| scene.nodes.first())
            .ok_or("unknown/empty scene")?;
        require_node(node, view)?;
        let node = node.id.clone();
        let token = self.take_token()?;
        self.state.scene = Some(SceneProgress {
            scene: id.to_owned(),
            node,
            token,
        });
        Ok(())
    }
    pub fn scene_view(&self, view: &impl View) -> Result<Option<SceneView>, String> {
        let Some(progress) = &self.state.scene else {
            return Ok(None);
        };
        let node = self.node(progress)?;
        require_node(node, view)?;
        let choices = node
            .choices
            .iter()
            .filter(|choice| conditions_hold(&choice.conditions, view))
            .map(|choice| {
                Ok(PresentedChoice {
                    id: choice.id.clone(),
                    text: self.render(&choice.text, view)?,
                    kind: choice.kind,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Some(SceneView {
            token: progress.token,
            scene: progress.scene.clone(),
            node: progress.node.clone(),
            speaker: node.speaker.clone(),
            text: self.render(&node.text, view)?,
            choices,
        }))
    }

    /// Revalidate the current node and choice, stage the next narrative state,
    /// then run the game's atomic batch. Nothing fallible remains after success.
    /// Retries, repeated input and restored old UI tokens cannot repeat rewards.
    pub fn choose(
        &mut self,
        token: u64,
        choice_id: &str,
        game: &mut impl Commands,
    ) -> Result<(), String> {
        let progress = self
            .state
            .scene
            .as_ref()
            .filter(|progress| progress.token == token)
            .ok_or("stale scene token")?;
        let node = self.node(progress)?;
        require_node(node, game)?;
        let choice = node
            .choices
            .iter()
            .find(|choice| choice.id == choice_id)
            .ok_or("unknown choice")?;
        if !conditions_hold(&choice.conditions, game) {
            return Err("choice is no longer eligible".into());
        }
        let mut staged = self.state.clone();
        let key = format!("{}/{}/{}", progress.scene, progress.node, choice.id);
        let count = staged.decisions.entry(key).or_default();
        *count = count.saturating_add(1);
        apply_transition(&mut staged, &choice.next)?;
        game.apply_atomic(&choice.effects)?;
        self.state = staged;
        Ok(())
    }
    pub fn advance_scene(&mut self, token: u64, view: &impl View) -> Result<(), String> {
        let progress = self
            .state
            .scene
            .as_ref()
            .filter(|progress| progress.token == token)
            .ok_or("stale scene token")?;
        let node = self.node(progress)?;
        require_node(node, view)?;
        let next = node
            .next
            .as_ref()
            .ok_or("choose a response before advancing")?
            .clone();
        let mut staged = self.state.clone();
        apply_transition(&mut staged, &next)?;
        self.state = staged;
        Ok(())
    }
    /// Explicit interruption keeps committed decisions but exits the encounter.
    pub fn interrupt_scene(&mut self, token: u64) -> Result<(), String> {
        if self
            .state
            .scene
            .as_ref()
            .is_none_or(|progress| progress.token != token)
        {
            return Err("stale scene token".into());
        }
        self.bump_generation()?;
        self.state.scene = None;
        Ok(())
    }

    fn rejection(&self, response: &Response, now: u64, view: &impl View) -> Option<&'static str> {
        if !view.speaker_present(&response.speaker) {
            return Some("speaker absent");
        }
        if !conditions_hold(&response.conditions, view) {
            return Some("conditions false or binding unavailable");
        }
        if !self.templates[&response.text].available(view) {
            return Some("interpolation unavailable, wrong registered type, or too large");
        }
        if self
            .state
            .speaker_ready
            .get(&response.speaker)
            .is_some_and(|ready| now < *ready)
        {
            return Some("speaker cooldown");
        }
        if self
            .state
            .category_ready
            .get(&response.category)
            .is_some_and(|ready| now < *ready)
        {
            return Some("category cooldown");
        }
        if let Some(history) = self.state.history.get(&response.id) {
            match response.repeat {
                Repeat::Once if history.selected != 0 => {
                    return Some("once response already selected")
                }
                Repeat::After(delay)
                    if history
                        .last_selected
                        .is_some_and(|last| now < last.saturating_add(delay)) =>
                {
                    return Some("response repetition cooldown")
                }
                _ => {}
            }
        }
        None
    }
    fn response(&self, id: &str) -> Option<&Response> {
        self.content
            .responses
            .iter()
            .find(|response| response.id == id)
    }
    fn render(&self, text: &str, view: &impl View) -> Result<String, String> {
        self.templates
            .get(text)
            .expect("validated content template")
            .render(view)
    }
    fn scene(&self, id: &str) -> Option<&Scene> {
        self.content.scenes.iter().find(|scene| scene.id == id)
    }
    fn node(&self, progress: &SceneProgress) -> Result<&Node, String> {
        self.scene(&progress.scene)
            .and_then(|scene| scene.nodes.iter().find(|node| node.id == progress.node))
            .ok_or_else(|| "unknown active scene/node".into())
    }
    fn bump_generation(&mut self) -> Result<(), String> {
        self.state.generation = self
            .state
            .generation
            .checked_add(1)
            .ok_or("narrative generation exhausted")?;
        Ok(())
    }
    fn take_token(&mut self) -> Result<u64, String> {
        let token = self.state.next_token;
        let next = token.checked_add(1).ok_or("narrative token exhausted")?;
        self.bump_generation()?;
        self.state.next_token = next;
        Ok(token)
    }
}

fn event_index(content: &Content) -> BTreeMap<String, Vec<usize>> {
    let mut events: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, response) in content.responses.iter().enumerate() {
        events
            .entry(response.event.clone())
            .or_default()
            .push(index);
    }
    for indices in events.values_mut() {
        indices.sort_by(|a, b| {
            let a = &content.responses[*a];
            let b = &content.responses[*b];
            b.priority
                .cmp(&a.priority)
                .then_with(|| b.conditions.len().cmp(&a.conditions.len()))
                .then_with(|| a.id.cmp(&b.id))
        });
    }
    events
}

pub fn conditions_hold(conditions: &[Condition], view: &impl View) -> bool {
    conditions.iter().all(|condition| {
        let Some(actual) = view.query(&condition.query) else {
            return false;
        };
        if actual.value_type() != condition.value.value_type() {
            return false;
        }
        match condition.op {
            Compare::Eq => actual == condition.value,
            Compare::Ne => actual != condition.value,
            op => match (actual, &condition.value) {
                (Value::Int(a), Value::Int(b)) => match op {
                    Compare::Lt => a < *b,
                    Compare::Le => a <= *b,
                    Compare::Gt => a > *b,
                    Compare::Ge => a >= *b,
                    _ => false,
                },
                _ => false,
            },
        }
    })
}

/// One-shot helper. Runtime content templates compile at activation/reload, once.
pub fn interpolate(text: &str, registry: &Registry, view: &impl View) -> Result<String, String> {
    TextTemplate::compile(text, registry)?.render(view)
}

enum TextPart {
    Literal(String),
    Query(String, ValueType),
}
struct TextTemplate {
    parts: Vec<TextPart>,
    capacity: usize,
}

impl TextTemplate {
    fn compile(text: &str, registry: &Registry) -> Result<Self, String> {
        interpolation_queries(text)?;
        let mut parts = Vec::new();
        let mut remaining = text;
        while let Some(start) = remaining.find('{') {
            parts.push(TextPart::Literal(remaining[..start].to_owned()));
            remaining = &remaining[start + 1..];
            let end = remaining.find('}').expect("validated interpolation");
            let id = &remaining[..end];
            let kind = registry
                .queries
                .get(id)
                .ok_or_else(|| format!("unknown interpolation query {id}"))?;
            parts.push(TextPart::Query(id.to_owned(), *kind));
            remaining = &remaining[end + 1..];
        }
        parts.push(TextPart::Literal(remaining.to_owned()));
        Ok(Self {
            parts,
            capacity: text.len(),
        })
    }
    fn available(&self, view: &impl View) -> bool {
        let mut bytes = 0usize;
        for part in &self.parts {
            let length = match part {
                TextPart::Literal(literal) => literal.len(),
                TextPart::Query(id, kind) => {
                    let Some(value) = view.query(id) else {
                        return false;
                    };
                    if value.value_type() != *kind {
                        return false;
                    }
                    let length = match value {
                        Value::Text(text) => text.len(),
                        Value::Bool(value) => {
                            if value {
                                4
                            } else {
                                5
                            }
                        }
                        Value::Int(value) => {
                            let magnitude = value.unsigned_abs();
                            let digits = if magnitude == 0 {
                                1
                            } else {
                                magnitude.ilog10() as usize + 1
                            };
                            digits + usize::from(value < 0)
                        }
                    };
                    if length > MAX_TEXT_BYTES {
                        return false;
                    }
                    length
                }
            };
            let Some(total) = bytes.checked_add(length) else {
                return false;
            };
            if total > MAX_TEXT_BYTES * 4 {
                return false;
            }
            bytes = total;
        }
        true
    }
    fn render(&self, view: &impl View) -> Result<String, String> {
        let mut text = String::with_capacity(self.capacity);
        for part in &self.parts {
            match part {
                TextPart::Literal(literal) => text.push_str(literal),
                TextPart::Query(id, kind) => {
                    let value = view
                        .query(id)
                        .ok_or_else(|| format!("interpolation query {id} unavailable"))?;
                    if value.value_type() != *kind {
                        return Err(format!(
                            "interpolation query {id} returned the wrong registered type"
                        ));
                    }
                    let value = value.display();
                    if value.len() > MAX_TEXT_BYTES {
                        return Err("interpolated value exceeds 4096 bytes".into());
                    }
                    text.push_str(&value);
                }
            }
            if text.len() > MAX_TEXT_BYTES * 4 {
                return Err("interpolated text exceeds 16384 bytes".into());
            }
        }
        Ok(text)
    }
}

fn compile_templates(content: &Content, registry: &Registry) -> BTreeMap<String, TextTemplate> {
    let response_text = content
        .responses
        .iter()
        .map(|response| response.text.as_str());
    let node_text = content
        .scenes
        .iter()
        .flat_map(|scene| scene.nodes.iter().map(|node| node.text.as_str()));
    let choice_text = content.scenes.iter().flat_map(|scene| {
        scene
            .nodes
            .iter()
            .flat_map(|node| node.choices.iter().map(|choice| choice.text.as_str()))
    });
    let mut templates = BTreeMap::new();
    for text in response_text.chain(node_text).chain(choice_text) {
        templates.entry(text.to_owned()).or_insert_with(|| {
            TextTemplate::compile(text, registry).expect("validated content interpolation")
        });
    }
    templates
}

fn require_node(node: &Node, view: &impl View) -> Result<(), String> {
    if !view.speaker_present(&node.speaker) {
        return Err("scene speaker absent".into());
    }
    if !conditions_hold(&node.conditions, view) {
        return Err("scene node is no longer eligible".into());
    }
    Ok(())
}

fn apply_transition(state: &mut NarrativeState, next: &Transition) -> Result<(), String> {
    let next_token = state
        .next_token
        .checked_add(1)
        .ok_or("narrative token exhausted")?;
    let generation = state
        .generation
        .checked_add(1)
        .ok_or("narrative generation exhausted")?;
    let progress = state.scene.as_mut().ok_or("no active scene")?;
    match next {
        Transition::Jump(node) => {
            progress.node = node.clone();
            progress.token = state.next_token;
        }
        Transition::End => {
            let count = state
                .completed_scenes
                .entry(progress.scene.clone())
                .or_default();
            *count = count.saturating_add(1);
            state.scene = None;
        }
    }
    state.next_token = next_token;
    state.generation = generation;
    Ok(())
}

fn scene_semantics(scene: &Scene) -> Scene {
    let mut scene = scene.clone();
    scene.source = Source::default();
    for node in &mut scene.nodes {
        node.text.clear();
        node.source = Source::default();
        node.text_source = Source::default();
        for condition in &mut node.conditions {
            condition.source = Source::default();
        }
        for choice in &mut node.choices {
            choice.text.clear();
            choice.source = Source::default();
            for condition in &mut choice.conditions {
                condition.source = Source::default();
            }
            for command in &mut choice.effects {
                command.source = Source::default();
            }
        }
    }
    scene
}

fn validate_state(state: &NarrativeState, content: &Content) -> Result<(), String> {
    if state.version != STATE_VERSION {
        return Err("unsupported narrative state version".into());
    }
    if state.queue.len() > MAX_QUEUE
        || state.next_token == 0
        || state.generation == u64::MAX
        || state.next_token == u64::MAX
    {
        return Err("invalid narrative state bounds".into());
    }
    let responses: BTreeMap<_, _> = content
        .responses
        .iter()
        .map(|response| (response.id.as_str(), response))
        .collect();
    for (id, history) in &state.history {
        if !responses.contains_key(id.as_str())
            || history.completed > history.presented
            || history.presented > history.selected
            || (history.selected != 0) != history.last_selected.is_some()
        {
            return Err(format!("invalid or removed response history {id}"));
        }
    }
    if state.speaker_ready.keys().any(|id| {
        !content
            .responses
            .iter()
            .any(|response| response.speaker == *id)
    }) || state.category_ready.keys().any(|id| {
        !content
            .responses
            .iter()
            .any(|response| response.category == *id)
    }) {
        return Err("removed cooldown binding requires a migration".into());
    }
    let mut last = 0;
    for context in &state.queue {
        if !valid_id(&context.event) || context.id <= last || context.id > state.last_context_id {
            return Err("invalid queued context sequence".into());
        }
        last = context.id;
    }
    if let Some(active) = &state.active_response {
        let history = state
            .history
            .get(&active.id)
            .ok_or("active response lacks history")?;
        if !responses.contains_key(active.id.as_str())
            || active.token == 0
            || active.token >= state.next_token
            || history.selected == 0
            || (active.presented && history.presented == 0)
        {
            return Err("invalid active response".into());
        }
    }
    if let Some(progress) = &state.scene {
        if progress.token == 0
            || progress.token >= state.next_token
            || !content.scenes.iter().any(|scene| {
                scene.id == progress.scene
                    && scene.nodes.iter().any(|node| node.id == progress.node)
            })
        {
            return Err("invalid or removed active scene/node".into());
        }
    }
    for key in state.decisions.keys() {
        if !content.scenes.iter().any(|scene| {
            scene.nodes.iter().any(|node| {
                node.choices
                    .iter()
                    .any(|choice| *key == format!("{}/{}/{}", scene.id, node.id, choice.id))
            })
        }) {
            return Err(format!("removed choice history {key} requires migration"));
        }
    }
    if state
        .completed_scenes
        .keys()
        .any(|id| !content.scenes.iter().any(|scene| scene.id == *id))
    {
        return Err("removed completed scene requires migration".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Game {
        coins: i64,
        gate: bool,
        present: bool,
        rewards: u32,
    }
    impl Default for Game {
        fn default() -> Self {
            Self {
                coins: 2,
                gate: false,
                present: true,
                rewards: 0,
            }
        }
    }
    impl View for Game {
        fn query(&self, id: &str) -> Option<Value> {
            match id {
                "coins" => Some(Value::Int(self.coins)),
                "gate" => Some(Value::Bool(self.gate)),
                "name" => Some(Value::Text("Iris".into())),
                _ => None,
            }
        }
        fn speaker_present(&self, _: &str) -> bool {
            self.present
        }
    }
    impl Commands for Game {
        fn apply_atomic(&mut self, commands: &[Command]) -> Result<(), String> {
            let mut staged = self.clone();
            for command in commands {
                match (&*command.id, &command.value) {
                    ("spend", Value::Int(amount)) if *amount > 0 && staged.coins >= *amount => {
                        staged.coins -= amount;
                        staged.rewards += 1;
                    }
                    ("gate", Value::Bool(open)) => staged.gate = *open,
                    _ => return Err("command rejected by authoritative rules".into()),
                }
            }
            *self = staged;
            Ok(())
        }
    }
    fn registry() -> Registry {
        Registry {
            queries: [
                ("coins".into(), ValueType::Int),
                ("gate".into(), ValueType::Bool),
                ("name".into(), ValueType::Text),
            ]
            .into(),
            commands: [
                ("spend".into(), ValueType::Int),
                ("gate".into(), ValueType::Bool),
            ]
            .into(),
        }
    }
    fn guard(query: &str, op: Compare, value: Value) -> Condition {
        Condition {
            query: query.into(),
            op,
            value,
            source: Source::default(),
        }
    }
    fn response(id: &str, priority: i32, conditions: Vec<Condition>) -> Response {
        Response {
            id: id.into(),
            event: "arrived".into(),
            speaker: id.into(),
            category: id.into(),
            text: "Hello {name}, {coins} coins.".into(),
            text_source: Source::default(),
            conditions,
            priority,
            cooldown: 0,
            repeat: Repeat::Always,
            source: Source::default(),
        }
    }
    fn responses(responses: Vec<Response>) -> Runtime {
        Runtime::new(
            Content {
                responses,
                ..Content::default()
            },
            registry(),
        )
        .unwrap()
    }
    fn context(id: u64, expires_at: u64) -> Context {
        Context {
            id,
            event: "arrived".into(),
            expires_at,
        }
    }
    fn interaction() -> Runtime {
        let choice = Choice {
            id: "pay".into(),
            text: "Open the gate".into(),
            kind: ChoiceKind::Consequential,
            conditions: vec![guard("coins", Compare::Ge, Value::Int(2))],
            effects: vec![
                Command {
                    id: "spend".into(),
                    value: Value::Int(2),
                    source: Source::default(),
                },
                Command {
                    id: "gate".into(),
                    value: Value::Bool(true),
                    source: Source::default(),
                },
            ],
            next: Transition::Jump("callback".into()),
            source: Source::default(),
        };
        let scene = Scene {
            id: "toll".into(),
            source: Source::default(),
            nodes: vec![
                Node {
                    id: "offer".into(),
                    speaker: "iris".into(),
                    text: "Two coins?".into(),
                    text_source: Source::default(),
                    conditions: vec![],
                    choices: vec![choice],
                    next: None,
                    source: Source::default(),
                },
                Node {
                    id: "callback".into(),
                    speaker: "iris".into(),
                    text: "The gate is {gate}.".into(),
                    text_source: Source::default(),
                    conditions: vec![guard("gate", Compare::Eq, Value::Bool(true))],
                    choices: vec![],
                    next: Some(Transition::End),
                    source: Source::default(),
                },
            ],
        };
        Runtime::new(
            Content {
                scenes: vec![scene],
                ..Content::default()
            },
            registry(),
        )
        .unwrap()
    }

    #[test]
    fn eligibility_is_pure_and_order_is_priority_specificity_then_stable_id() {
        let game = Game::default();
        let mut runtime = responses(vec![
            response("z", 1, vec![]),
            response("b", 1, vec![guard("coins", Compare::Ge, Value::Int(1))]),
            response("a", 1, vec![guard("coins", Compare::Ge, Value::Int(1))]),
            response("low", 0, vec![]),
        ]);
        runtime.enqueue(context(1, 100)).unwrap();
        let before = runtime.state().clone();
        for _ in 0..10 {
            assert_eq!(
                runtime.inspect("arrived", 0, &game).selected.as_deref(),
                Some("a")
            );
            assert_eq!(
                runtime.select(&context(1, 100), 0, &game).unwrap().response,
                "a"
            );
        }
        assert_eq!(runtime.state(), &before);
        let selected = runtime.select(&context(1, 100), 0, &game).unwrap();
        let shown = runtime.commit_response(&selected, 0, &game).unwrap();
        assert_eq!(shown.text, "Hello Iris, 2 coins.");
        assert!(runtime.commit_response(&selected, 0, &game).is_err());
    }

    #[test]
    fn selected_presented_completed_and_interrupted_are_distinct() {
        let mut runtime = responses(vec![response("quiet", 1, vec![])]);
        let game = Game::default();
        runtime.enqueue(context(1, 100)).unwrap();
        let selected = runtime.dispatch(0, &game).unwrap().unwrap();
        assert!(runtime.complete_response(selected.token).is_err());
        runtime.presented_response(selected.token).unwrap();
        runtime.presented_response(selected.token).unwrap();
        runtime.interrupt_response(selected.token).unwrap();
        assert_eq!(
            runtime.state().history["quiet"],
            ResponseHistory {
                selected: 1,
                presented: 1,
                completed: 0,
                last_selected: Some(0)
            }
        );
        runtime.enqueue(context(2, 100)).unwrap();
        let selected = runtime.dispatch(1, &game).unwrap().unwrap();
        runtime.presented_response(selected.token).unwrap();
        runtime.complete_response(selected.token).unwrap();
        assert!(runtime.complete_response(selected.token).is_err());
        assert_eq!(runtime.state().history["quiet"].completed, 1);
    }

    #[test]
    fn higher_priority_interrupts_and_old_presentation_tokens_fail() {
        let mut urgent = response("urgent", 10, vec![]);
        urgent.event = "danger".into();
        let mut runtime = responses(vec![response("quiet", 1, vec![]), urgent]);
        let game = Game::default();
        runtime.enqueue(context(1, 100)).unwrap();
        let quiet = runtime.dispatch(0, &game).unwrap().unwrap();
        runtime.presented_response(quiet.token).unwrap();
        runtime
            .enqueue(Context {
                id: 2,
                event: "danger".into(),
                expires_at: 100,
            })
            .unwrap();
        let urgent = runtime.dispatch(1, &game).unwrap().unwrap();
        assert_eq!(urgent.id, "urgent");
        assert!(runtime.complete_response(quiet.token).is_err());
        assert_eq!(runtime.state().history["quiet"].completed, 0);
        runtime.enqueue(context(3, 100)).unwrap();
        assert!(runtime.dispatch(2, &game).unwrap().is_none());
    }

    #[test]
    fn absent_speakers_expired_contexts_and_unknown_events_produce_silence() {
        let mut runtime = responses(vec![response("quiet", 1, vec![])]);
        let mut game = Game {
            present: false,
            ..Game::default()
        };
        runtime.enqueue(context(1, 10)).unwrap();
        assert!(runtime.dispatch(0, &game).unwrap().is_none());
        assert_eq!(
            runtime.inspect("arrived", 0, &game).candidates[0].reason,
            "speaker absent"
        );
        game.present = true;
        let line = runtime.dispatch(10, &game).unwrap().unwrap();
        assert_eq!(line.id, "quiet");
        game.present = false;
        assert!(runtime.dispatch(11, &game).unwrap().is_none());
        assert!(runtime.state().active_response.is_none());
        runtime.enqueue(context(2, 12)).unwrap();
        assert!(runtime.dispatch(13, &game).unwrap().is_none());
        assert!(runtime.state().queue.is_empty());
        assert!(runtime.inspect("missing", 14, &game).candidates.is_empty());
    }

    #[test]
    fn speaker_and_category_cooldowns_and_once_policy_start_on_selection() {
        let mut first = response("first", 2, vec![]);
        first.speaker = "same".into();
        first.category = "same-category".into();
        first.cooldown = 10;
        first.repeat = Repeat::Once;
        let mut second = response("second", 1, vec![]);
        second.speaker = "same".into();
        let mut third = response("third", 0, vec![]);
        third.category = "same-category".into();
        let mut runtime = responses(vec![first, second, third]);
        let game = Game::default();
        runtime.enqueue(context(1, 100)).unwrap();
        let line = runtime.dispatch(0, &game).unwrap().unwrap();
        runtime.interrupt_response(line.token).unwrap();
        let report = runtime.inspect("arrived", 1, &game);
        assert!(report.selected.is_none());
        assert!(report
            .candidates
            .iter()
            .any(|candidate| candidate.reason == "speaker cooldown"));
        assert!(report
            .candidates
            .iter()
            .any(|candidate| candidate.reason == "category cooldown"));
        assert_eq!(
            runtime.inspect("arrived", 10, &game).selected.as_deref(),
            Some("second")
        );
        assert_eq!(
            runtime.inspect("arrived", 10, &game).candidates[0].reason,
            "once response already selected"
        );
    }

    #[test]
    fn bounded_queue_and_retried_event_ids_are_explicit() {
        let mut runtime = responses(vec![]);
        let game = Game::default();
        for id in 1..=MAX_QUEUE as u64 {
            runtime.enqueue(context(id, 10)).unwrap();
        }
        assert!(runtime.enqueue(context(9, 10)).is_err());
        assert_eq!(runtime.state().last_context_id, 8);
        assert!(runtime.enqueue(context(8, 10)).is_err());
        runtime.dispatch(11, &game).unwrap();
        runtime.enqueue(context(9, 20)).unwrap();
    }

    #[test]
    fn stale_response_conditions_revalidate_without_committing_history() {
        let mut runtime = responses(vec![
            response("rich", 2, vec![guard("coins", Compare::Ge, Value::Int(2))]),
            response("fallback", 0, vec![]),
        ]);
        let mut game = Game::default();
        runtime.enqueue(context(1, 100)).unwrap();
        let selected = runtime.select(&context(1, 100), 0, &game).unwrap();
        game.coins = 0;
        assert!(runtime.commit_response(&selected, 0, &game).is_err());
        assert!(runtime.state().history.is_empty());
        assert_eq!(runtime.dispatch(0, &game).unwrap().unwrap().id, "fallback");
    }

    #[test]
    fn stale_choice_and_failed_atomic_batch_leave_both_states_unchanged() {
        let mut runtime = interaction();
        let mut game = Game::default();
        runtime.start_scene("toll", &game).unwrap();
        let token = runtime.scene_view(&game).unwrap().unwrap().token;
        game.coins = 1;
        let before = runtime.state().clone();
        let game_before = game.clone();
        assert!(runtime.choose(token, "pay", &mut game).is_err());
        assert_eq!(runtime.state(), &before);
        assert_eq!(game, game_before);
        game.coins = 2;
        let mut content = runtime.content().clone();
        content.scenes[0].nodes[0].choices[0].effects.insert(
            1,
            Command {
                id: "spend".into(),
                value: Value::Int(1),
                source: Source::default(),
            },
        );
        let mut failed = Runtime::new(content, registry()).unwrap();
        failed.start_scene("toll", &game).unwrap();
        let token = failed.scene_view(&game).unwrap().unwrap().token;
        let before = failed.state().clone();
        let game_before = game.clone();
        assert!(failed.choose(token, "pay", &mut game).is_err());
        assert_eq!(failed.state(), &before);
        assert_eq!(game, game_before);
    }

    #[test]
    fn choice_effects_apply_in_order_once_and_callback_reads_real_game_state() {
        let mut runtime = interaction();
        let mut game = Game::default();
        runtime.start_scene("toll", &game).unwrap();
        let offer = runtime.scene_view(&game).unwrap().unwrap();
        runtime.choose(offer.token, "pay", &mut game).unwrap();
        assert_eq!((game.coins, game.gate, game.rewards), (0, true, 1));
        assert!(runtime.choose(offer.token, "pay", &mut game).is_err());
        assert_eq!(game.rewards, 1);
        let callback = runtime.scene_view(&game).unwrap().unwrap();
        assert_eq!(callback.text, "The gate is true.");
        runtime.advance_scene(callback.token, &game).unwrap();
        assert!(runtime.scene_view(&game).unwrap().is_none());
        assert_eq!(runtime.state().completed_scenes["toll"], 1);
    }

    #[test]
    fn snapshot_round_trip_resumes_progress_without_duplicate_rewards() {
        let mut first = interaction();
        let mut first_game = Game::default();
        first.start_scene("toll", &first_game).unwrap();
        let offer = first.scene_view(&first_game).unwrap().unwrap();
        first.choose(offer.token, "pay", &mut first_game).unwrap();
        let bytes = serde_json::to_vec(first.state()).unwrap();
        let mut resumed = interaction();
        resumed
            .restore_state(serde_json::from_slice(&bytes).unwrap())
            .unwrap();
        let mut resumed_game = first_game.clone();
        assert_eq!(first.state().state_hash(), resumed.state().state_hash());
        assert!(resumed
            .choose(offer.token, "pay", &mut resumed_game)
            .is_err());
        let token = first.scene_view(&first_game).unwrap().unwrap().token;
        first.advance_scene(token, &first_game).unwrap();
        resumed.advance_scene(token, &resumed_game).unwrap();
        assert_eq!(first.state(), resumed.state());
        assert_eq!(resumed_game.rewards, 1);
    }

    #[test]
    fn reload_is_validate_then_activate_and_preserves_only_compatible_progress() {
        let mut runtime = interaction();
        let mut game = Game::default();
        runtime.start_scene("toll", &game).unwrap();
        let before = runtime.state().clone();
        let mut prose = runtime.content().clone();
        prose.scenes[0].nodes[0].text = "May I have two coins?".into();
        assert!(runtime.reload(prose.clone(), &registry(), false).is_err());
        assert_eq!(runtime.state(), &before);
        runtime.reload(prose, &registry(), true).unwrap();
        assert_eq!(
            runtime.scene_view(&game).unwrap().unwrap().text,
            "May I have two coins?"
        );
        let before = runtime.state().clone();
        let content_before = runtime.content().clone();
        let mut invalid = content_before.clone();
        invalid.version = 99;
        assert!(runtime.reload(invalid, &registry(), true).is_err());
        assert_eq!(runtime.content(), &content_before);
        assert_eq!(runtime.state(), &before);
        let mut semantic = content_before.clone();
        semantic.scenes[0].nodes[0].choices[0].effects[0].value = Value::Int(1);
        assert!(runtime.reload(semantic, &registry(), true).is_err());
        assert_eq!(runtime.state(), &before);
        let token = runtime.scene_view(&game).unwrap().unwrap().token;
        runtime.choose(token, "pay", &mut game).unwrap();
        assert_eq!(game.rewards, 1);
    }

    #[test]
    fn malformed_saved_state_is_refused_before_mutation() {
        let mut runtime = interaction();
        let before = runtime.state().clone();
        let mut invalid = before.clone();
        invalid.scene = Some(SceneProgress {
            scene: "missing".into(),
            node: "offer".into(),
            token: 1,
        });
        assert!(runtime.restore_state(invalid).is_err());
        assert_eq!(runtime.state(), &before);
        let mut invalid = before.clone();
        invalid.version += 1;
        assert!(runtime.restore_state(invalid).is_err());
        let mut invalid = before.clone();
        invalid.decisions.insert("toll/offer/missing".into(), 1);
        assert!(runtime.restore_state(invalid).is_err());
    }

    #[test]
    fn intentional_scene_loops_yield_and_require_a_fresh_input_token() {
        let mut runtime = interaction();
        let mut content = runtime.content().clone();
        content.scenes[0].nodes[0].choices[0].next = Transition::Jump("offer".into());
        content.scenes[0].nodes[0].choices[0].effects.clear();
        runtime = Runtime::new(content, registry()).unwrap();
        let mut game = Game::default();
        runtime.start_scene("toll", &game).unwrap();
        for _ in 0..20 {
            let token = runtime.scene_view(&game).unwrap().unwrap().token;
            runtime.choose(token, "pay", &mut game).unwrap();
            assert!(runtime.choose(token, "pay", &mut game).is_err());
        }
        assert_eq!(runtime.state().decisions.len(), 1);
        assert_eq!(runtime.state().decisions["toll/offer/pay"], 20);
        assert_eq!(game.rewards, 0);
    }

    #[test]
    fn content_validation_reports_actual_binding_locations_and_types() {
        let mut content = interaction().content().clone();
        let choice = &mut content.scenes[0].nodes[0].choices[0];
        choice.effects[0].source = Source {
            file: "toll.poe".into(),
            node: "offer/pay".into(),
            line: 14,
        };
        choice.effects[0].value = Value::Text("two".into());
        let errors = content.validate(&registry()).unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.to_string().contains("toll.poe:14 [offer/pay]")));
        let mut content = Content::default();
        let mut line = response(
            "bad",
            0,
            vec![guard("gate", Compare::Gt, Value::Bool(false))],
        );
        line.text = "{unknown}".into();
        content.responses.push(line);
        let errors = content.validate(&registry()).unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.message.contains("ordered comparisons")));
        assert!(errors
            .iter()
            .any(|error| error.message.contains("unknown interpolation")));
    }

    #[test]
    fn interpolation_rejects_runtime_type_mismatch_and_final_size_overflow() {
        struct BadType;
        impl View for BadType {
            fn query(&self, _: &str) -> Option<Value> {
                Some(Value::Bool(true))
            }
            fn speaker_present(&self, _: &str) -> bool {
                true
            }
        }
        assert!(interpolate("{coins}", &registry(), &BadType)
            .unwrap_err()
            .contains("wrong registered type"));
        struct Long;
        impl View for Long {
            fn query(&self, _: &str) -> Option<Value> {
                Some(Value::Text("x".repeat(MAX_TEXT_BYTES)))
            }
            fn speaker_present(&self, _: &str) -> bool {
                true
            }
        }
        assert!(interpolate("{name}{name}{name}{name}trailing", &registry(), &Long).is_err());
    }

    #[test]
    fn oversized_interpolation_selects_fallback_without_mutating_queued_context() {
        struct Name {
            bytes: usize,
        }
        impl View for Name {
            fn query(&self, id: &str) -> Option<Value> {
                (id == "name").then(|| Value::Text("x".repeat(self.bytes)))
            }
            fn speaker_present(&self, _: &str) -> bool {
                true
            }
        }
        for (bytes, text, expected) in [
            (MAX_TEXT_BYTES + 1, "{name}", "fallback"),
            (
                MAX_TEXT_BYTES,
                "{name}{name}{name}{name}trailing",
                "fallback",
            ),
            (MAX_TEXT_BYTES, "{name}{name}{name}{name}", "large"),
        ] {
            let mut large = response("large", 10, vec![]);
            large.text = text.into();
            let mut fallback = response("fallback", 0, vec![]);
            fallback.text = "Quiet is fine.".into();
            let mut runtime = responses(vec![large, fallback]);
            let view = Name { bytes };
            let context = context(1, 100);
            runtime.enqueue(context.clone()).unwrap();
            let before = runtime.state().clone();
            assert_eq!(
                runtime.select(&context, 0, &view).unwrap().response,
                expected
            );
            assert_eq!(runtime.state(), &before);
            assert_eq!(runtime.dispatch(0, &view).unwrap().unwrap().id, expected);
        }
    }

    /// Opt-in measurement, separate from correctness checks. Report raw duration,
    /// pool sizes and iteration counts; these numbers do not measure story quality.
    #[test]
    #[ignore = "explicit microbenchmark; use --ignored --nocapture"]
    fn measure_indexed_selection_and_validated_reload() {
        use std::time::Instant;
        let mut lines = Vec::new();
        for i in 0..64 {
            let mut line = response(
                &format!("line-{i:03}"),
                i,
                vec![guard("coins", Compare::Ge, Value::Int(1))],
            );
            line.event = format!("event-{}", i % 4);
            lines.push(line);
        }
        let mut runtime = responses(lines);
        let game = Game::default();
        let context = Context {
            id: 1,
            event: "event-0".into(),
            expires_at: 100,
        };
        let selection_start = Instant::now();
        for _ in 0..100_000 {
            std::hint::black_box(runtime.select(std::hint::black_box(&context), 0, &game));
        }
        let selection_ns = selection_start.elapsed().as_nanos();
        let content = runtime.content().clone();
        let reload_start = Instant::now();
        for _ in 0..1_000 {
            runtime.reload(content.clone(), &registry(), true).unwrap();
        }
        let reload_ns = reload_start.elapsed().as_nanos();
        println!("{{\"profile\":\"itest\",\"responses\":64,\"event_pools\":4,\"candidates_per_event\":16,\"select_iterations\":100000,\"select_total_ns\":{selection_ns},\"reload_iterations\":1000,\"reload_total_ns\":{reload_ns},\"state_bytes\":{}}}", serde_json::to_vec(runtime.state()).unwrap().len());
    }
}
