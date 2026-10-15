//! A bounded, line-oriented frontend. Parsed content is reused by the runtime.
use super::model::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

/// Content is deliberately small enough to validate before activation.
pub const MAX_SOURCE_BYTES: usize = 256 * 1024;

type Parsed<T> = Result<T, Diagnostic>;

struct Parser<'a> {
    file: &'a str,
    lines: Vec<(usize, &'a str)>,
    at: usize,
}

impl<'a> Parser<'a> {
    fn source(&self, node: &str, line: usize) -> Source {
        Source {
            file: self.file.into(),
            node: node.into(),
            line,
        }
    }

    fn take(&mut self, node: &str) -> Parsed<(Source, &'a str)> {
        match self.lines.get(self.at).copied() {
            Some((line, text)) => {
                self.at += 1;
                Ok((self.source(node, line), text))
            }
            None => Err(self
                .source(node, self.lines.last().map_or(1, |(n, _)| n + 1))
                .error("unexpected end of file; close the current block")),
        }
    }

    fn response(&mut self, id: &str, source: Source) -> Parsed<Response> {
        let mut response = Response {
            id: id.into(),
            event: String::new(),
            speaker: String::new(),
            category: String::new(),
            text: String::new(),
            conditions: vec![],
            priority: 0,
            cooldown: 0,
            repeat: Repeat::Always,
            text_source: source.clone(),
            source,
        };
        let mut seen = BTreeSet::new();
        loop {
            let (source, text) = self.take(id)?;
            let (key, value) = field(text);
            if key == "endresponse" {
                no_value(value, &source)?;
                break;
            }
            unique(key, &mut seen, &source)?;
            match key {
                "event" => response.event = identifier(value, &source)?,
                "speaker" => response.speaker = identifier(value, &source)?,
                "category" => response.category = identifier(value, &source)?,
                "text" => {
                    response.text = value.into();
                    response.text_source = source;
                }
                "when" => response.conditions.push(condition(value, source)?),
                "priority" => response.priority = number(value, &source)?,
                "cooldown" => response.cooldown = number(value, &source)?,
                "repeat" => {
                    response.repeat = match value {
                        "always" => Repeat::Always,
                        "once" => Repeat::Once,
                        _ => match value.strip_prefix("after ") {
                            Some(ticks) => Repeat::After(number(ticks.trim(), &source)?),
                            None => {
                                return Err(
                                    source.error("repeat expects always, once, or after TICKS")
                                )
                            }
                        },
                    }
                }
                "note" => {}
                _ => return Err(source.error(format!("unknown response field {key:?}"))),
            }
        }
        for key in ["event", "speaker", "category", "text"] {
            if !seen.contains(key) {
                return Err(response.source.error(format!("response needs {key}")));
            }
        }
        Ok(response)
    }

    fn scene(&mut self, id: &str, source: Source) -> Parsed<Scene> {
        let mut scene = Scene {
            id: id.into(),
            nodes: vec![],
            source,
        };
        loop {
            let (source, text) = self.take(id)?;
            let (key, value) = field(text);
            match key {
                "endscene" => {
                    no_value(value, &source)?;
                    break;
                }
                "node" => {
                    let node_id = identifier(value, &source)?;
                    let source = Source {
                        node: format!("{id}.{node_id}"),
                        ..source
                    };
                    scene.nodes.push(self.node(&node_id, source)?);
                    if scene.nodes.len() > MAX_NODES {
                        return Err(scene.source.error("scene exceeds 128 nodes"));
                    }
                }
                "note" => {}
                _ => return Err(source.error("scene expects node ID or endscene")),
            }
        }
        Ok(scene)
    }

    fn node(&mut self, id: &str, source: Source) -> Parsed<Node> {
        let mut node = Node {
            id: id.into(),
            speaker: String::new(),
            text: String::new(),
            conditions: vec![],
            choices: vec![],
            next: None,
            text_source: source.clone(),
            source,
        };
        let mut seen = BTreeSet::new();
        loop {
            let (source, text) = self.take(&node.source.node)?;
            let (key, value) = field(text);
            if key == "endnode" {
                no_value(value, &source)?;
                break;
            }
            unique(key, &mut seen, &source)?;
            match key {
                "speaker" => node.speaker = identifier(value, &source)?,
                "text" => {
                    node.text = value.into();
                    node.text_source = source;
                }
                "when" => node.conditions.push(condition(value, source)?),
                "choice" => {
                    node.choices.push(self.choice(value, source)?);
                    if node.choices.len() > MAX_CHOICES {
                        return Err(node.source.error("node exceeds 8 choices"));
                    }
                }
                "next" => node.next = Some(transition(value, &source)?),
                "note" => {}
                _ => return Err(source.error(format!("unknown node field {key:?}"))),
            }
        }
        for key in ["speaker", "text"] {
            if !seen.contains(key) {
                return Err(node.source.error(format!("node needs {key}")));
            }
        }
        Ok(node)
    }

    fn choice(&mut self, header: &str, source: Source) -> Parsed<Choice> {
        let (prefix, text) = header
            .split_once(" | ")
            .ok_or_else(|| source.error("choice expects ID KIND -> DESTINATION | TEXT"))?;
        let parts: Vec<_> = prefix.split_whitespace().collect();
        if parts.len() != 4 || parts[2] != "->" {
            return Err(source.error("choice expects ID KIND -> DESTINATION | TEXT"));
        }
        let id = identifier(parts[0], &source)?;
        let source = Source {
            node: format!("{}.{}", source.node, id),
            ..source
        };
        let kind = match parts[1] {
            "consequential" => ChoiceKind::Consequential,
            "informational" => ChoiceKind::Informational,
            "expressive" => ChoiceKind::Expressive,
            _ => {
                return Err(
                    source.error("choice kind expects consequential, informational, or expressive")
                )
            }
        };
        let mut choice = Choice {
            id,
            text: text.into(),
            kind,
            conditions: vec![],
            effects: vec![],
            next: transition(parts[3], &source)?,
            source,
        };
        loop {
            let (source, text) = self.take(&choice.source.node)?;
            let (key, value) = field(text);
            match key {
                "endchoice" => {
                    no_value(value, &source)?;
                    break;
                }
                "when" => choice.conditions.push(condition(value, source)?),
                "do" => {
                    let (id, value) = field(value);
                    choice.effects.push(Command {
                        id: identifier(id, &source)?,
                        value: literal(value, &source)?,
                        source,
                    });
                    if choice.effects.len() > MAX_EFFECTS {
                        return Err(choice.source.error("choice exceeds 16 effects"));
                    }
                }
                "note" => {}
                _ => return Err(source.error("choice expects when, do, note, or endchoice")),
            }
        }
        Ok(choice)
    }
}

fn field(text: &str) -> (&str, &str) {
    text.split_once(char::is_whitespace)
        .map_or((text, ""), |(k, v)| (k, v.trim()))
}

fn no_value(value: &str, source: &Source) -> Parsed<()> {
    if value.is_empty() {
        Ok(())
    } else {
        Err(source.error("unexpected text after block ending"))
    }
}

fn identifier(value: &str, source: &Source) -> Parsed<String> {
    if valid_id(value) {
        Ok(value.into())
    } else {
        Err(source.error(format!("expected stable ID, got {value:?}")))
    }
}

fn number<T: std::str::FromStr>(value: &str, source: &Source) -> Parsed<T> {
    value
        .parse()
        .map_err(|_| source.error(format!("invalid integer {value:?}")))
}

fn unique<'a>(key: &'a str, seen: &mut BTreeSet<&'a str>, source: &Source) -> Parsed<()> {
    if matches!(key, "when" | "choice" | "note") || seen.insert(key) {
        Ok(())
    } else {
        Err(source.error(format!("duplicate field {key}")))
    }
}

fn transition(value: &str, source: &Source) -> Parsed<Transition> {
    if value == "end" {
        Ok(Transition::End)
    } else {
        Ok(Transition::Jump(identifier(value, source)?))
    }
}

fn literal(value: &str, source: &Source) -> Parsed<Value> {
    serde_json::from_str(value)
        .map_err(|_| source.error("expected JSON bool, signed integer, or quoted text"))
}

fn condition(value: &str, source: Source) -> Parsed<Condition> {
    let (query, rest) = field(value);
    let (operator, value) = field(rest);
    let op = match operator {
        "==" => Compare::Eq,
        "!=" => Compare::Ne,
        "<" => Compare::Lt,
        "<=" => Compare::Le,
        ">" => Compare::Gt,
        ">=" => Compare::Ge,
        _ => return Err(source.error("when expects QUERY ==|!=|<|<=|>|>= VALUE")),
    };
    Ok(Condition {
        query: identifier(query, &source)?,
        op,
        value: literal(value, &source)?,
        source,
    })
}

/// Parse text without gameplay bindings. Call `Content::validate` before execution.
pub fn parse(file: &str, text: &str) -> Result<Content, Vec<Diagnostic>> {
    let source = Source {
        file: file.into(),
        node: "content".into(),
        line: 1,
    };
    if text.len() > MAX_SOURCE_BYTES {
        return Err(vec![source.error("source exceeds 256 KiB")]);
    }
    let mut parser = Parser {
        file,
        lines: text
            .trim_start_matches('\u{feff}')
            .lines()
            .enumerate()
            .filter_map(|(i, line)| {
                let line = line.trim();
                (!line.is_empty() && !line.starts_with('#')).then_some((i + 1, line))
            })
            .collect(),
        at: 0,
    };
    let mut read = || -> Parsed<Content> {
        let (source, header) = parser.take("content")?;
        let (key, value) = field(header);
        if key != "poe" || value != "1" {
            return Err(source.error("expected supported header: poe 1"));
        }
        let mut content = Content::default();
        while parser.at < parser.lines.len() {
            let (source, text) = parser.take("content")?;
            let (key, value) = field(text);
            match key {
                "response" => {
                    let id = identifier(value, &source)?;
                    content.responses.push(parser.response(
                        &id,
                        Source {
                            node: id.clone(),
                            ..source
                        },
                    )?);
                }
                "scene" => {
                    let id = identifier(value, &source)?;
                    content.scenes.push(parser.scene(
                        &id,
                        Source {
                            node: id.clone(),
                            ..source
                        },
                    )?);
                }
                "note" => {}
                _ => return Err(source.error("expected response ID or scene ID")),
            }
            if content.responses.len() > MAX_RESPONSES || content.scenes.len() > MAX_SCENES {
                return Err(parser
                    .source("content", 1)
                    .error("content exceeds response/scene limits"));
            }
        }
        Ok(content)
    };
    read().map_err(|error| vec![error])
}

/// Parse and validate against a game's registered live query/command contract.
pub fn parse_validated(
    file: &str,
    text: &str,
    registry: &Registry,
) -> Result<Content, Vec<Diagnostic>> {
    let content = parse(file, text)?;
    content.validate(registry)?;
    Ok(content)
}

/// A bounded content load; games call this explicitly, never every frame.
pub fn load(path: &Path, registry: &Registry) -> Result<Content, Vec<Diagnostic>> {
    use std::io::Read;
    let source = Source {
        file: path.display().to_string(),
        node: "content".into(),
        line: 1,
    };
    let read = || -> std::io::Result<String> {
        let mut text = String::new();
        std::fs::File::open(path)?
            .take((MAX_SOURCE_BYTES + 1) as u64)
            .read_to_string(&mut text)?;
        Ok(text)
    };
    let text = read().map_err(|e| vec![source.error(format!("cannot read content: {e}"))])?;
    parse_validated(&path.display().to_string(), &text, registry)
}

/// Scoped authoring warnings, never a judgment about literary quality.
pub fn lint(content: &Content) -> Vec<Diagnostic> {
    let mut warnings = vec![];
    let mut texts = BTreeMap::new();
    let mut events: BTreeMap<&str, Vec<&Response>> = BTreeMap::new();
    for response in &content.responses {
        if let Some(previous) = texts.insert(&response.text, &response.id) {
            warnings.push(response.source.error(format!(
                "same response text as {previous}; review repetition"
            )));
        }
        events.entry(&response.event).or_default().push(response);
    }
    for (event, pool) in events {
        if pool.iter().all(|r| !r.conditions.is_empty()) {
            warnings.push(pool[0].source.error(format!(
                "event {event} has no unconditional fallback; silence may be intentional"
            )));
        }
    }
    for scene in &content.scenes {
        let Some(first) = scene.nodes.first() else {
            continue;
        };
        let mut reached = BTreeSet::new();
        let mut pending = vec![first.id.as_str()];
        while let Some(id) = pending.pop() {
            if !reached.insert(id) {
                continue;
            }
            if let Some(node) = scene.nodes.iter().find(|n| n.id == id) {
                for next in node.next.iter().chain(node.choices.iter().map(|c| &c.next)) {
                    if let Transition::Jump(id) = next {
                        pending.push(id.as_str());
                    }
                }
            }
        }
        for node in &scene.nodes {
            if !node.choices.is_empty() && node.choices.iter().all(|c| !c.conditions.is_empty()) {
                warnings.push(node.source.error("all choices are conditional; review fallback, interruption, or external recovery"));
            }
            if !reached.contains(node.id.as_str()) {
                warnings.push(node.source.error("node is unreachable from scene entry"));
            }
            for (i, choice) in node.choices.iter().enumerate() {
                for earlier in &node.choices[..i] {
                    // Ignore source locations when comparing semantic conditions/effects.
                    let same_guards = choice
                        .conditions
                        .iter()
                        .map(|c| (&c.query, c.op, &c.value))
                        .eq(earlier
                            .conditions
                            .iter()
                            .map(|c| (&c.query, c.op, &c.value)));
                    let same_effects = choice
                        .effects
                        .iter()
                        .map(|c| (&c.id, &c.value))
                        .eq(earlier.effects.iter().map(|c| (&c.id, &c.value)));
                    if same_guards && same_effects && choice.next == earlier.next {
                        warnings.push(choice.source.error(format!("same bound effects and destination as {}; expressive/informational differences need editorial review", earlier.id)));
                    }
                }
            }
        }
    }
    warnings
}
