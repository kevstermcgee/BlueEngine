use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Explicit limits make authoring validation and execution bounded.
pub const CONTENT_VERSION: u32 = 1;
pub const MAX_RESPONSES: usize = 512;
pub const MAX_SCENES: usize = 32;
pub const MAX_NODES: usize = 128;
pub const MAX_CHOICES: usize = 8;
pub const MAX_CONDITIONS: usize = 16;
pub const MAX_EFFECTS: usize = 16;
pub const MAX_TEXT_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    Bool,
    Int,
    Text,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Text(String),
}

impl Value {
    pub fn value_type(&self) -> ValueType {
        match self {
            Self::Bool(_) => ValueType::Bool,
            Self::Int(_) => ValueType::Int,
            Self::Text(_) => ValueType::Text,
        }
    }
    pub fn display(&self) -> String {
        match self {
            Self::Bool(v) => v.to_string(),
            Self::Int(v) => v.to_string(),
            Self::Text(v) => v.clone(),
        }
    }
}

/// A game's public contract, not a copy of its state. Every value is read live.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registry {
    pub queries: BTreeMap<String, ValueType>,
    pub commands: BTreeMap<String, ValueType>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub file: String,
    pub node: String,
    pub line: usize,
}

impl Source {
    pub fn error(&self, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            source: self.clone(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub source: Source,
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{} [{}]: {}",
            self.source.file, self.source.line, self.source.node, self.message
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Compare {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Condition {
    pub query: String,
    pub op: Compare,
    pub value: Value,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub id: String,
    pub value: Value,
    pub source: Source,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Repeat {
    Always,
    Once,
    After(u64),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    pub id: String,
    pub event: String,
    pub speaker: String,
    pub category: String,
    pub text: String,
    pub text_source: Source,
    pub conditions: Vec<Condition>,
    pub priority: i32,
    pub cooldown: u64,
    pub repeat: Repeat,
    pub source: Source,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChoiceKind {
    Consequential,
    Informational,
    Expressive,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transition {
    Jump(String),
    End,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    pub id: String,
    pub text: String,
    pub kind: ChoiceKind,
    pub conditions: Vec<Condition>,
    pub effects: Vec<Command>,
    pub next: Transition,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub speaker: String,
    pub text: String,
    pub text_source: Source,
    pub conditions: Vec<Condition>,
    pub choices: Vec<Choice>,
    /// Explicit continuation for nodes without choices. A choice node has no `next`.
    pub next: Option<Transition>,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scene {
    pub id: String,
    pub nodes: Vec<Node>,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Content {
    pub version: u32,
    pub responses: Vec<Response>,
    pub scenes: Vec<Scene>,
}

impl Default for Content {
    fn default() -> Self {
        Self {
            version: CONTENT_VERSION,
            responses: vec![],
            scenes: vec![],
        }
    }
}

pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
}

impl Content {
    /// Hard validity checks only. Voice, emotional impact, and prose implications need review.
    pub fn validate(&self, registry: &Registry) -> Result<(), Vec<Diagnostic>> {
        let mut errors = Vec::new();
        let global = Source {
            file: "<content>".into(),
            node: "content".into(),
            line: 1,
        };
        if self.version != CONTENT_VERSION {
            errors.push(global.error(format!("unsupported content version {}", self.version)));
        }
        if self.responses.len() > MAX_RESPONSES || self.scenes.len() > MAX_SCENES {
            errors.push(global.error("content exceeds response/scene limits"));
        }
        let mut ids = BTreeSet::new();
        for response in &self.responses {
            check_id(&response.id, &response.source, &mut ids, &mut errors);
            for id in [&response.event, &response.speaker, &response.category] {
                if !valid_id(id) {
                    errors.push(response.source.error(format!("invalid binding ID {id:?}")));
                }
            }
            check_text(&response.text, registry, &response.text_source, &mut errors);
            check_conditions(
                &response.conditions,
                registry,
                &response.source,
                &mut errors,
            );
        }
        for scene in &self.scenes {
            check_id(&scene.id, &scene.source, &mut ids, &mut errors);
            if scene.nodes.is_empty() || scene.nodes.len() > MAX_NODES {
                errors.push(scene.source.error("scene needs 1..128 nodes"));
            }
            let mut node_ids = BTreeSet::new();
            for node in &scene.nodes {
                check_id(&node.id, &node.source, &mut node_ids, &mut errors);
            }
            for node in &scene.nodes {
                if !valid_id(&node.speaker) {
                    errors.push(node.source.error("invalid speaker ID"));
                }
                check_text(&node.text, registry, &node.text_source, &mut errors);
                check_conditions(&node.conditions, registry, &node.source, &mut errors);
                if node.choices.len() > MAX_CHOICES {
                    errors.push(node.source.error("node exceeds 8 choices"));
                }
                if node.choices.is_empty() == node.next.is_none() {
                    errors.push(
                        node.source
                            .error("node needs choices or one explicit next/end"),
                    );
                }
                if let Some(next) = &node.next {
                    check_transition(next, &node_ids, &node.source, &mut errors);
                }
                let mut choices = BTreeSet::new();
                for choice in &node.choices {
                    check_id(&choice.id, &choice.source, &mut choices, &mut errors);
                    check_text(&choice.text, registry, &choice.source, &mut errors);
                    check_conditions(&choice.conditions, registry, &choice.source, &mut errors);
                    check_transition(&choice.next, &node_ids, &choice.source, &mut errors);
                    if choice.effects.len() > MAX_EFFECTS {
                        errors.push(choice.source.error("choice exceeds 16 effects"));
                    }
                    for effect in &choice.effects {
                        if registry.commands.get(&effect.id) != Some(&effect.value.value_type()) {
                            errors.push(effect.source.error(format!(
                                "unknown command or wrong argument type: {}",
                                effect.id
                            )));
                        }
                    }
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

fn check_id(id: &str, source: &Source, ids: &mut BTreeSet<String>, errors: &mut Vec<Diagnostic>) {
    if !valid_id(id) {
        errors.push(source.error(format!("invalid ID {id:?}")));
    }
    if !ids.insert(id.to_owned()) {
        errors.push(source.error(format!("duplicate ID {id}")));
    }
}

fn check_transition(
    next: &Transition,
    ids: &BTreeSet<String>,
    source: &Source,
    errors: &mut Vec<Diagnostic>,
) {
    if let Transition::Jump(id) = next {
        if !ids.contains(id) {
            errors.push(source.error(format!("unknown destination {id}")));
        }
    }
}

fn check_conditions(
    conditions: &[Condition],
    registry: &Registry,
    source: &Source,
    errors: &mut Vec<Diagnostic>,
) {
    if conditions.len() > MAX_CONDITIONS {
        errors.push(source.error("more than 16 conditions"));
    }
    for condition in conditions {
        if registry.queries.get(&condition.query) != Some(&condition.value.value_type()) {
            errors.push(condition.source.error(format!(
                "unknown query or wrong value type: {}",
                condition.query
            )));
        }
        if !matches!(condition.op, Compare::Eq | Compare::Ne)
            && condition.value.value_type() != ValueType::Int
        {
            errors.push(
                condition
                    .source
                    .error("ordered comparisons require integer queries"),
            );
        }
    }
}

/// Find interpolation bindings, keeping syntax deliberately smaller than an expression language.
pub fn interpolation_queries(text: &str) -> Result<Vec<&str>, String> {
    let mut queries = Vec::new();
    let mut remaining = text;
    while let Some(start) = remaining.find(['{', '}']) {
        if remaining.as_bytes()[start] == b'}' {
            return Err("unmatched interpolation closing brace".into());
        }
        remaining = &remaining[start + 1..];
        let end = remaining.find('}').ok_or("unclosed interpolation")?;
        let query = &remaining[..end];
        if !valid_id(query) {
            return Err(format!("invalid interpolation query {query:?}"));
        }
        queries.push(query);
        remaining = &remaining[end + 1..];
    }
    Ok(queries)
}

fn check_text(text: &str, registry: &Registry, source: &Source, errors: &mut Vec<Diagnostic>) {
    if text.is_empty() || text.len() > MAX_TEXT_BYTES {
        errors.push(source.error("text needs 1..4096 bytes"));
    }
    match interpolation_queries(text) {
        Err(message) => errors.push(source.error(message)),
        Ok(queries) => {
            for query in queries {
                if !registry.queries.contains_key(query) {
                    errors.push(source.error(format!("unknown interpolation query {query}")));
                }
            }
        }
    }
}
