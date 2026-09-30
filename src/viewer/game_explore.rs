//! Static and reachability analysis of a [`GameDocument`](super::game::GameDocument), driven by the engine's own rule runtime.
//!
//! `game-validate` says a game is well formed. This says whether it *works*: can it be won, can it
//! be lost, is any rule, target, timer or counter dead, and can a player get stuck. It searches the
//! space of rule states breadth-first, so the paths it reports are shortest, and it uses
//! [`GameRuntime`] itself to take each step, so the analysis cannot disagree with the game.
//!
//! What is abstracted: time and movement. Any enabled target can be pressed, any running timer can
//! run out, and the player can enter or leave any enabled zone, in any order and at any moment.
//! So "can be won" is an over-approximation of the real game (a physical obstacle or a timing window
//! is not seen), while "can never be won" and "this rule never fires" are exact conclusions.
use super::game::{Condition, GameAction, GameRuntime, GameState, LoadedGame, ModelEvent};
use crate::Result;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

/// States kept before the search stops and says so.
pub const DEFAULT_MAX_STATES: usize = 100_000;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub level: Level,
    pub kind: &'static str,
    pub message: String,
    /// Events that demonstrate it, when there is one.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub states: usize,
    /// The state limit was hit: every "never" below is then only "not within the states seen".
    pub truncated: bool,
    /// `None` when the search was cut short before finding a win.
    pub winnable: Option<bool>,
    pub shortest_win: Option<Vec<String>>,
    pub can_lose: bool,
    pub shortest_loss: Option<Vec<String>>,
    pub findings: Vec<Finding>,
    pub assumptions: Vec<&'static str>,
    /// The shortest win as events, for tooling that turns it into a scenario.
    #[serde(skip)]
    pub win_events: Vec<ModelEvent>,
}

impl Report {
    pub fn has_errors(&self) -> bool {
        self.findings.iter().any(|f| f.level == Level::Error)
    }
}

struct Node {
    state: GameState,
    occupied: u64,
    parent: Option<(usize, ModelEvent)>,
}

/// How a counter is compared across all rules: what a state key may forget about it.
#[derive(Default, Clone)]
struct Reads {
    modulo: Vec<i32>,
    plain: bool,
    constants: Vec<i32>,
}

fn collect_leaf_reads(condition: &Condition, out: &mut BTreeMap<String, Reads>) {
    if let Some(name) = &condition.counter {
        let reads = out.entry(name.clone()).or_default();
        match condition.modulo {
            Some(m) => reads.modulo.push(m),
            None => reads.plain = true,
        }
        reads.constants.extend(
            [
                condition.equals,
                condition.not_equals,
                condition.less_than,
                condition.greater_than,
                condition.at_most,
                condition.at_least,
            ]
            .into_iter()
            .flatten(),
        );
    }
    for child in condition.all.iter().chain(&condition.any).flatten() {
        collect_leaf_reads(child, out);
    }
    if let Some(child) = &condition.not {
        collect_leaf_reads(child, out);
    }
}

/// What the state key keeps of a counter. Each choice is exact: two values with the same
/// abstraction behave identically forever, given how the rules can change that counter.
#[derive(Clone, Copy)]
enum Keep {
    Nothing,
    Exact,
    /// Only the remainder matters (every read is a modulo comparison).
    Remainder(i64),
    /// Never decreases, so every value above the largest constant compared is equivalent.
    AtMost(i32),
    /// Never increases, so every value below the smallest constant compared is equivalent.
    AtLeast(i32),
}

impl Keep {
    fn apply(self, v: i32) -> i64 {
        match self {
            Keep::Nothing => 0,
            Keep::Exact => i64::from(v),
            Keep::Remainder(m) => i64::from(v).rem_euclid(m),
            Keep::AtMost(k) => i64::from(v.min(k)),
            Keep::AtLeast(k) => i64::from(v.max(k)),
        }
    }
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

fn collect_reads(condition: &Condition, out: &mut BTreeSet<String>) {
    out.extend(condition.counter.iter().cloned());
    for child in condition.all.iter().chain(&condition.any).flatten() {
        collect_reads(child, out);
    }
    if let Some(child) = &condition.not {
        collect_reads(child, out);
    }
}

pub fn explore(loaded: &LoadedGame, max_states: usize) -> Result<Report> {
    explore_with(loaded, max_states, true)
}

/// [`explore`] with counter folding optional. Folding is exact, so switching it off must give the same
/// answers (only slower, and possibly truncated); the tests rely on that.
pub fn explore_with(loaded: &LoadedGame, max_states: usize, fold_counters: bool) -> Result<Report> {
    let document = &loaded.document;
    let mut runtime = GameRuntime::compile(document.clone(), &loaded.map)?;
    runtime.record_fired_rules();

    // Static facts about the document.
    let mut reads = BTreeSet::new();
    let mut leaf_reads: BTreeMap<String, Reads> = BTreeMap::new();
    let (mut raises, mut lowers) = (BTreeSet::new(), BTreeSet::new());
    let (mut writes, mut started_timers, mut listened_timers) =
        (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    let (mut listened_zones, mut has_complete, mut has_fail) = (BTreeSet::new(), false, false);
    let mut fail_rules = Vec::new();
    let mut complete_rules = Vec::new();
    for rule in &document.rules {
        rule.condition.iter().for_each(|c| {
            collect_reads(c, &mut reads);
            collect_leaf_reads(c, &mut leaf_reads);
        });
        listened_timers.extend(rule.on_timer.iter().cloned());
        listened_zones.extend(rule.on_enter.iter().chain(&rule.on_exit).cloned());
        for action in &rule.actions {
            match action {
                GameAction::Increment { counter, amount } => {
                    writes.insert(counter.clone());
                    if *amount > 0 {
                        raises.insert(counter.clone());
                    } else if *amount < 0 {
                        lowers.insert(counter.clone());
                    }
                }
                GameAction::SetCounter { counter, .. } => {
                    writes.insert(counter.clone());
                }
                GameAction::StartTimer { timer } => {
                    started_timers.insert(timer.clone());
                }
                GameAction::Complete => {
                    has_complete = true;
                    complete_rules.push(rule.id.clone());
                }
                GameAction::Fail => {
                    has_fail = true;
                    fail_rules.push(rule.id.clone());
                }
                _ => {}
            }
        }
    }
    // What each counter contributes to a state. A counter nobody reads is dropped entirely, which
    // stops a decorative countdown from making the search unbounded; unbounded counters that are
    // only compared by modulo or by threshold fold into a small finite set of equivalent values.
    let keep: Vec<Keep> = document
        .counters
        .keys()
        .map(|name| {
            let Some(r) = leaf_reads.get(name) else {
                return Keep::Nothing;
            };
            if !fold_counters {
                return Keep::Exact;
            }
            let (lo, hi) = (r.constants.iter().min(), r.constants.iter().max());
            if !r.plain && !r.modulo.is_empty() {
                let lcm = r.modulo.iter().fold(1i64, |acc, &m| {
                    let m = i64::from(m);
                    (acc / gcd(acc, m)).saturating_mul(m)
                });
                return if lcm <= 1_000_000 {
                    Keep::Remainder(lcm)
                } else {
                    Keep::Exact
                };
            }
            match (
                r.modulo.is_empty(),
                lowers.contains(name),
                raises.contains(name),
                lo,
                hi,
            ) {
                (true, false, _, _, Some(&hi)) => Keep::AtMost(hi.saturating_add(1)),
                (true, _, false, Some(&lo), _) => Keep::AtLeast(lo.saturating_sub(1)),
                _ => Keep::Exact,
            }
        })
        .collect();
    let key = |state: &GameState, occupied: u64| {
        let counters: Vec<i64> = state
            .counters
            .iter()
            .zip(&keep)
            .map(|(v, keep)| keep.apply(*v))
            .collect();
        (
            counters,
            [
                state.enabled,
                state.enabled_zones,
                state.active_timers,
                state.fired,
            ],
            u8::from(state.completed) | u8::from(state.failed) << 1,
            occupied,
        )
    };

    // Breadth-first search over rule states.
    let mut nodes = vec![Node {
        state: runtime.state().clone(),
        occupied: 0,
        parent: None,
    }];
    let mut seen = HashMap::new();
    seen.insert(key(&nodes[0].state, 0), 0usize);
    let mut queue = VecDeque::from([0usize]);
    let mut edges: Vec<(usize, usize)> = Vec::new();
    let (mut first_win, mut first_loss) = (None, None);
    let mut fired = BTreeSet::new();
    let mut idle_press: BTreeMap<usize, usize> = BTreeMap::new();
    let (mut ever_enabled, mut truncated) = (nodes[0].state.enabled, false);
    while let Some(n) = queue.pop_front() {
        let (state, occupied) = (nodes[n].state.clone(), nodes[n].occupied);
        runtime.model_load(&state);
        for event in runtime.model_events(occupied) {
            runtime.model_load(&state);
            let mut occ = occupied;
            runtime.model_apply(event, &mut occ);
            fired.extend(runtime.take_fired_rules());
            let next = runtime.state().clone();
            let k = key(&next, occ);
            let to = match seen.get(&k) {
                Some(&to) => to,
                None if nodes.len() >= max_states => {
                    truncated = true;
                    continue;
                }
                None => {
                    let to = nodes.len();
                    ever_enabled |= next.enabled;
                    if next.completed && first_win.is_none() {
                        first_win = Some(to);
                    }
                    if next.failed && first_loss.is_none() {
                        first_loss = Some(to);
                    }
                    nodes.push(Node {
                        state: next,
                        occupied: occ,
                        parent: Some((n, event)),
                    });
                    seen.insert(k, to);
                    queue.push_back(to);
                    to
                }
            };
            if to == n {
                if let ModelEvent::Interact(target) = event {
                    idle_press.entry(target).or_insert(n);
                }
            }
            edges.push((n, to));
        }
    }

    let describe = |mut at: usize| -> Vec<String> {
        let mut events = Vec::new();
        while let Some((from, event)) = nodes[at].parent {
            events.push(runtime.model_event_name(event));
            at = from;
        }
        events.reverse();
        let mut out: Vec<(String, usize)> = Vec::new();
        for name in events {
            match out.last_mut() {
                Some((last, count)) if *last == name => *count += 1,
                _ => out.push((name, 1)),
            }
        }
        out.into_iter()
            .map(|(name, n)| if n > 1 { format!("{name} x{n}") } else { name })
            .collect()
    };
    let win_events = first_win
        .map(|mut at| {
            let mut events = Vec::new();
            while let Some((from, event)) = nodes[at].parent {
                events.push(event);
                at = from;
            }
            events.reverse();
            events
        })
        .unwrap_or_default();
    let shortest_win = first_win.map(describe);
    let shortest_loss = first_loss.map(describe);
    let winnable = match (&shortest_win, truncated) {
        (Some(_), _) => Some(true),
        (None, false) => Some(false),
        (None, true) => None,
    };

    let mut findings = Vec::new();
    let mut add = |level, kind, message: String, path: Vec<String>| {
        findings.push(Finding {
            level,
            kind,
            message,
            path,
        });
    };
    let never = if truncated {
        Level::Info
    } else {
        Level::Warning
    };
    if !has_complete {
        add(
            Level::Error,
            "no-win",
            "No rule has a complete action, so the game can never be won.".into(),
            vec![],
        );
    } else if winnable == Some(false) {
        add(
            Level::Error,
            "unwinnable",
            format!(
                "No sequence of events completes the game. The rules that complete it ({}) can never fire.",
                complete_rules.join(", ")
            ),
            vec![],
        );
    } else if winnable.is_none() {
        add(
            Level::Warning,
            "no-win-found",
            format!("No win was found within {} states.", nodes.len()),
            vec![],
        );
    }
    if !has_fail {
        add(
            Level::Info,
            "cannot-lose",
            "No rule uses fail, so the game cannot be lost.".into(),
            vec![],
        );
    } else if first_loss.is_none() {
        add(
            Level::Warning,
            "fail-unreachable",
            format!(
                "The fail action in {} can never happen.",
                fail_rules.join(", ")
            ),
            vec![],
        );
    }
    for (index, rule) in document.rules.iter().enumerate() {
        if !fired.contains(&index) {
            add(
                never,
                "rule-never-fires",
                format!("Rule '{}' never fires in any reachable state.", rule.id),
                vec![],
            );
        }
    }
    for (i, target) in document.interactables.iter().enumerate() {
        if ever_enabled & (1 << i) == 0 {
            add(
                never,
                "target-never-enabled",
                format!(
                    "'{}' starts disabled and nothing ever enables it, so it cannot be pressed.",
                    target.entity
                ),
                vec![],
            );
        }
    }
    for timer in &document.timers {
        if !listened_timers.contains(&timer.id) {
            add(
                Level::Warning,
                "timer-unheard",
                format!(
                    "Timer '{}' runs out but no rule listens to it (on_timer), so it does nothing.",
                    timer.id
                ),
                vec![],
            );
        }
        if !timer.auto_start && !started_timers.contains(&timer.id) {
            add(
                Level::Warning,
                "timer-never-started",
                format!(
                    "Timer '{}' never runs: auto_start is false and no rule starts it.",
                    timer.id
                ),
                vec![],
            );
        }
    }
    let mut no_rule = BTreeSet::new();
    for (i, target) in document.interactables.iter().enumerate() {
        // A rule with no trigger at all reacts to every target; timer and zone rules do not.
        let reacts = document
            .rules
            .iter()
            .any(|r| match r.on_interact.as_deref() {
                Some(t) => t == target.entity,
                None => r.on_enter.is_none() && r.on_exit.is_none() && r.on_timer.is_none(),
            });
        if !reacts {
            no_rule.insert(i);
            add(
                Level::Warning,
                "target-no-rule",
                format!(
                    "No rule reacts to '{}' (on_interact), so pressing it can never do anything.",
                    target.entity
                ),
                vec![],
            );
        }
    }
    for zone in &document.trigger_zones {
        if !listened_zones.contains(&zone.id) {
            add(
                Level::Info,
                "zone-unheard",
                format!(
                    "Trigger zone '{}' has no on_enter or on_exit rule.",
                    zone.id
                ),
                vec![],
            );
        }
    }
    for (name, start) in &document.counters {
        match (reads.contains(name), writes.contains(name)) {
            (false, true) => add(Level::Warning, "counter-unread", format!("Counter '{name}' is changed by rules but no condition reads it, so it cannot affect the game (the HUD still shows it)."), vec![]),
            (true, false) => add(Level::Info, "counter-constant", format!("Counter '{name}' is read by conditions but never changed: it stays at {start}."), vec![]),
            (false, false) => add(Level::Warning, "counter-unused", format!("Counter '{name}' is never read or changed."), vec![]),
            (true, true) => {}
        }
    }
    // A target that is armed but whose press changes nothing the rules can see: usually a target
    // enabled a step too early, or one the player can press before it matters.
    let mut idle: Vec<(usize, usize)> = idle_press
        .into_iter()
        .filter(|(t, _)| !no_rule.contains(t))
        .collect();
    idle.sort_by_key(|&(_, at)| at);
    for &(target, at) in idle.iter().take(5) {
        let name = &document.interactables[target].entity;
        let mut path = describe(at);
        path.push(format!("press {name} (does nothing)"));
        add(
            Level::Info,
            "press-does-nothing",
            format!("'{name}' can be pressed in a reachable state where nothing happens (it may be armed too early)."),
            path,
        );
    }
    if idle.len() > 5 {
        add(
            Level::Info,
            "press-does-nothing",
            format!(
                "{} more targets can also be pressed to no effect.",
                idle.len() - 5
            ),
            vec![],
        );
    }
    // Stuck states: reachable, not finished, and unable to reach a win.
    if winnable == Some(true) && !truncated {
        let mut back = vec![Vec::new(); nodes.len()];
        for &(from, to) in &edges {
            back[to].push(from);
        }
        let mut alive = vec![false; nodes.len()];
        let mut stack: Vec<usize> = (0..nodes.len())
            .filter(|&i| nodes[i].state.completed)
            .collect();
        stack.iter().for_each(|&i| alive[i] = true);
        while let Some(at) = stack.pop() {
            for &from in &back[at] {
                if !alive[from] {
                    alive[from] = true;
                    stack.push(from);
                }
            }
        }
        let stuck: Vec<usize> = (0..nodes.len())
            .filter(|&i| !alive[i] && !nodes[i].state.finished())
            .collect();
        if let Some(&first) = stuck.first() {
            add(
                Level::Warning,
                "can-get-stuck",
                format!("{} reachable state(s) can no longer be won and are not a loss. The shortest way in is shown.", stuck.len()),
                describe(first),
            );
        }
    }
    if truncated {
        add(
            Level::Info,
            "truncated",
            format!("Stopped after {} states. A counter that keeps growing can make the space unbounded, so results are lower bounds.", nodes.len()),
            vec![],
        );
    }
    findings.sort_by_key(|f| f.level);
    Ok(Report {
        states: nodes.len(),
        truncated,
        winnable,
        shortest_win,
        can_lose: first_loss.is_some(),
        shortest_loss,
        findings,
        win_events,
        assumptions: vec![
            "time is abstracted: any running timer can run out at any moment, in any order",
            "movement is abstracted: the player can press any enabled target and enter or leave any enabled zone",
            "so 'can be won' may overstate a physical obstacle, while 'never' findings are exact unless truncated",
        ],
    })
}
