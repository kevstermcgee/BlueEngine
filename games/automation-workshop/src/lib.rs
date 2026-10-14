//! One game-owned fixed-tick authority. Rendering/network clients only submit commands.
pub mod content;
#[cfg(feature = "online")]
pub mod net;
pub mod persistence;
pub use content::{Definitions, Item, Recipe, Upgrade};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vesper3d::runtime::resources::{Inventory, TransactionError};
use vesper3d::runtime::{Simulation, StateHasher};
pub const GAME_ID: &str = "blueengine:automation-workshop";
pub const GAME_RELEASE: &str = "0.1.0";
pub const MAX_MACHINES: usize = 1024;
pub const MAX_QUEUE: usize = 2;
pub const MAX_COMMANDS: usize = 32;
pub const MAX_STATE_BYTES: usize = 2 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Queue { machine: String, recipe: String },
    Cancel { machine: String },
    Gather { item: String, amount: u64 },
    Deliver { item: String, amount: u64 },
    Upgrade { machine: String, upgrade: String },
    Wait,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    pub seat: u8,
    pub serial: u64,
    pub command: Command,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub recipe: String,
    pub rule_version: u32,
    pub speed_divisor: u32,
    pub duration_ticks: u32,
    pub remaining_ticks: u32,
    pub inputs: Vec<(String, u64)>,
    pub outputs: Vec<(String, u64)>,
    pub cancel_pending: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Machine {
    pub id: String,
    pub queue: Vec<Job>,
    pub upgrades: BTreeSet<String>,
    #[serde(skip)]
    pub blocked: Option<TransactionError>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub tick: u64,
    pub instance_id: String,
    pub inventory: Inventory,
    pub supply: BTreeMap<String, u64>,
    #[serde(rename = "shipped")]
    pub delivered: BTreeMap<String, u64>,
    pub machines: BTreeMap<String, Machine>,
    pub last_command: BTreeMap<u8, u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Outcome {
    pub seat: u8,
    pub serial: u64,
    pub ok: bool,
    pub detail: String,
}
#[derive(Clone)]
pub struct Workshop {
    pub state: State,
    pub definitions: Definitions,
    pub outcomes: Vec<Outcome>,
}
impl Workshop {
    pub fn standard() -> Self {
        Self::new(Definitions::standard(), 2).expect("validated bundled workshop")
    }
    pub fn with_machine_count(count: usize) -> Result<Self, String> {
        Self::new(Definitions::standard(), count)
    }
    pub fn new(definitions: Definitions, count: usize) -> Result<Self, String> {
        if !(1..=MAX_MACHINES).contains(&count) {
            return Err(format!("WORKSHOP_MACHINE_LIMIT: requested {count}; allowed 1..={MAX_MACHINES}"));
        }
        let capacities = definitions.items.iter().map(|(id, item)| (id.clone(), item.capacity)).collect();
        let inventory = Inventory::from_counts(definitions.inventory_capacity, capacities, definitions.initial.clone())
            .map_err(|e| e.to_string())?;
        let machines = (0..count)
            .map(|i| {
                let id: String = match i {
                    0 => "workshop:machine-a".into(),
                    1 => "workshop:machine-b".into(),
                    _ => format!("workshop:machine-{i:04}"),
                };
                (id.clone(), Machine { id, queue: Vec::new(), upgrades: BTreeSet::new(), blocked: None })
            })
            .collect();
        let state = State {
            tick: 0,
            instance_id: "workshop:world-0001".into(),
            inventory,
            supply: definitions.sources.clone(),
            delivered: BTreeMap::new(),
            machines,
            last_command: BTreeMap::new(),
        };
        let sim = Self { state, definitions, outcomes: Vec::new() };
        sim.validate_state(&sim.state)?;
        Ok(sim)
    }
    /// Player action: validate a complete candidate, then activate it once.
    pub fn command(&mut self, command: &Command) -> Result<(), String> {
        let mut next = self.state.clone();
        match command {
            Command::Wait => return Ok(()),
            Command::Gather { item, amount } => {
                let stock = next
                    .supply
                    .get_mut(item)
                    .ok_or_else(|| format!("WORKSHOP_SOURCE: no declared source for {item}"))?;
                if *amount == 0 || *stock < *amount {
                    return Err(format!("WORKSHOP_SOURCE: {item}, available {stock}, requested {amount}"));
                }
                next.inventory.transact(&[], &[(item.clone(), *amount)]).map_err(|e| e.to_string())?;
                *stock -= amount;
            }
            Command::Deliver { item, amount } => {
                if *amount == 0 {
                    return Err("WORKSHOP_DELIVER: quantity must be positive".into());
                }
                if !self.definitions.deliverable.contains(item) {
                    return Err(format!("WORKSHOP_SINK: {item} is not a declared delivery item"));
                }
                let total = next
                    .delivered
                    .get(item)
                    .copied()
                    .unwrap_or(0)
                    .checked_add(*amount)
                    .ok_or("WORKSHOP_OVERFLOW: delivery count")?;
                next.inventory.transact(&[(item.clone(), *amount)], &[]).map_err(|e| e.to_string())?;
                next.delivered.insert(item.clone(), total);
            }
            Command::Queue { machine, recipe } => {
                let recipe =
                    self.definitions.recipes.get(recipe).ok_or_else(|| format!("WORKSHOP_RECIPE: unknown {recipe}"))?;
                let machine =
                    next.machines.get_mut(machine).ok_or_else(|| format!("WORKSHOP_MACHINE: unknown {machine}"))?;
                if machine.queue.len() >= MAX_QUEUE {
                    return Err(format!("WORKSHOP_QUEUE_FULL: {} max {MAX_QUEUE}", machine.id));
                }
                if let Some(required) = &recipe.required_upgrade {
                    if !machine.upgrades.contains(required) {
                        return Err(format!("WORKSHOP_UPGRADE_REQUIRED: {} needs {required}", recipe.id));
                    }
                }
                let speed = machine
                    .upgrades
                    .iter()
                    .filter_map(|id| self.definitions.upgrades.get(id))
                    .map(|u| u.duration_divisor)
                    .max()
                    .unwrap_or(1);
                let duration = recipe.duration_ticks.div_ceil(speed);
                next.inventory.transact(&recipe.inputs, &[]).map_err(|e| e.to_string())?;
                machine.queue.push(Job {
                    recipe: recipe.id.clone(),
                    rule_version: self.definitions.content_version,
                    speed_divisor: speed,
                    duration_ticks: duration,
                    remaining_ticks: duration,
                    inputs: recipe.inputs.clone(),
                    outputs: recipe.outputs.clone(),
                    cancel_pending: false,
                });
                machine.blocked = None;
            }
            Command::Cancel { machine } => {
                let machine =
                    next.machines.get_mut(machine).ok_or_else(|| format!("WORKSHOP_MACHINE: unknown {machine}"))?;
                machine.queue.first_mut().ok_or("WORKSHOP_IDLE: no work to cancel")?.cancel_pending = true;
            }
            Command::Upgrade { machine, upgrade } => {
                let upgrade = self
                    .definitions
                    .upgrades
                    .get(upgrade)
                    .ok_or_else(|| format!("WORKSHOP_UPGRADE: unknown {upgrade}"))?;
                let machine =
                    next.machines.get_mut(machine).ok_or_else(|| format!("WORKSHOP_MACHINE: unknown {machine}"))?;
                if machine.upgrades.contains(&upgrade.id) {
                    return Err("WORKSHOP_ALREADY_UPGRADED".into());
                }
                next.inventory.transact(&upgrade.cost, &[]).map_err(|e| e.to_string())?;
                machine.upgrades.insert(upgrade.id.clone());
            }
        }
        self.validate_state(&next)?;
        self.state = next;
        Ok(())
    }
    pub fn validate_state(&self, state: &State) -> Result<(), String> {
        if state.machines.is_empty()
            || state.machines.len() > MAX_MACHINES
            || !content::valid_id(&state.instance_id)
            || state.last_command.len() > 2
            || state.last_command.keys().any(|seat| *seat >= 2)
        {
            return Err("WORKSHOP_STATE_BOUNDS: invalid instances, identity or seats".into());
        }
        state.inventory.validate().map_err(|e| e.to_string())?;
        let capacities: BTreeMap<_, _> =
            self.definitions.items.iter().map(|(id, i)| (id.clone(), i.capacity)).collect();
        if state.inventory.capacities() != &capacities
            || state.inventory.max_units() != self.definitions.inventory_capacity
        {
            return Err("WORKSHOP_STATE_CONTENT: inventory definition differs".into());
        }
        for (id, quantity) in &state.supply {
            if *quantity > self.definitions.sources.get(id).copied().ok_or("WORKSHOP_STATE_SOURCE: unknown source")? {
                return Err("WORKSHOP_STATE_SOURCE: exceeds declared source".into());
            }
        }
        if state.supply.len() != self.definitions.sources.len()
            || state.delivered.keys().any(|id| !self.definitions.deliverable.contains(id))
        {
            return Err("WORKSHOP_STATE_CONTENT: invalid source/sink".into());
        }
        for (id, machine) in &state.machines {
            if id != &machine.id
                || !content::valid_id(id)
                || machine.queue.len() > MAX_QUEUE
                || machine.upgrades.iter().any(|id| !self.definitions.upgrades.contains_key(id))
            {
                return Err(format!("WORKSHOP_MACHINE_STATE: invalid {id}"));
            }
            for job in &machine.queue {
                let rules = self.definitions.rules_for_version(job.rule_version)?;
                let upgrades = self.definitions.upgrades_for_version(job.rule_version)?;
                let recipe = rules.get(&job.recipe).ok_or_else(|| {
                    format!("WORKSHOP_JOB_RECIPE: {} not in content {}", job.recipe, job.rule_version)
                })?;
                if let Some(required) = &recipe.required_upgrade {
                    if !machine.upgrades.contains(required) {
                        return Err(format!("WORKSHOP_JOB_UPGRADE_REQUIRED: {} requires owned {required}", job.recipe));
                    }
                }
                let authorized_speed = job.speed_divisor == 1
                    || machine.upgrades.iter().any(|id| {
                        upgrades.get(id).is_some_and(|upgrade| upgrade.duration_divisor == job.speed_divisor)
                    });
                if !authorized_speed
                    || job.duration_ticks != recipe.duration_ticks.div_ceil(job.speed_divisor)
                    || job.remaining_ticks > job.duration_ticks
                    || job.inputs != recipe.inputs
                    || job.outputs != recipe.outputs
                {
                    return Err(format!("WORKSHOP_JOB_STATE: frozen rule differs or unowned speed for {}", job.recipe));
                }
            }
        }
        Ok(())
    }
    /// Rebuild derived blockage diagnostics after restore without advancing logical time or committing
    /// inventory changes. Each preview uses the current shared inventory, not cached presentation state.
    pub fn recompute_blocked(&mut self) {
        let inventory = &self.state.inventory;
        for machine in self.state.machines.values_mut() {
            machine.blocked =
                machine.queue.first().filter(|job| job.cancel_pending || job.remaining_ticks == 0).and_then(|job| {
                    let credits = if job.cancel_pending { &job.inputs } else { &job.outputs };
                    inventory.clone().transact(&[], credits).err()
                });
        }
    }
    pub fn inspect(&self, id: &str) -> Result<serde_json::Value, String> {
        let m = self.state.machines.get(id).ok_or_else(|| format!("WORKSHOP_MACHINE: unknown {id}"))?;
        let job = m.queue.first();
        Ok(
            serde_json::json!({"game":GAME_ID,"release":GAME_RELEASE,"instance":self.state.instance_id,"tick":self.state.tick,"machine":m,"reason":m.blocked,"rule":job.map(|j|&j.recipe),"definition":job.and_then(|j|self.definitions.location(&j.recipe,j.rule_version)),"inventory":self.state.inventory.counts(),"total_units":self.state.inventory.total_units(),"capacity":self.state.inventory.max_units(),"queued_work":m.queue.len(),"authority":"fixed-tick game-owned Workshop::step"}),
        )
    }
}
impl Simulation for Workshop {
    type Input = Vec<Envelope>;
    fn step(&mut self, inputs: &Self::Input) {
        self.outcomes.clear();
        if self.state.tick == u64::MAX {
            return;
        }
        self.state.tick += 1;
        if inputs.len() <= MAX_COMMANDS {
            let mut ordered: Vec<_> = inputs.iter().collect();
            ordered.sort_by_key(|input| (input.seat, input.serial));
            if let Some(conflict) = ordered.windows(2).find(|pair| {
                pair[0].seat == pair[1].seat && pair[0].serial == pair[1].serial && pair[0].command != pair[1].command
            }) {
                self.outcomes.push(Outcome {
                    seat: conflict[0].seat,
                    serial: conflict[0].serial,
                    ok: false,
                    detail: format!(
                        "WORKSHOP_COMMAND_CONFLICT: seat {} serial {} names different commands; entire batch rejected",
                        conflict[0].seat, conflict[0].serial
                    ),
                });
            } else {
                for input in ordered {
                    if input.seat >= 2
                        || input.serial == 0
                        || input.serial <= self.state.last_command.get(&input.seat).copied().unwrap_or(0)
                    {
                        continue;
                    }
                    let result = self.command(&input.command);
                    self.state.last_command.insert(input.seat, input.serial);
                    self.outcomes.push(Outcome {
                        seat: input.seat,
                        serial: input.serial,
                        ok: result.is_ok(),
                        detail: result.err().unwrap_or_else(|| "committed".into()),
                    });
                }
            }
        } else {
            self.outcomes.push(Outcome {
                seat: 0,
                serial: 0,
                ok: false,
                detail: format!("WORKSHOP_COMMAND_LIMIT: max {MAX_COMMANDS}"),
            });
        }
        for machine in self.state.machines.values_mut() {
            let Some(job) = machine.queue.first_mut() else {
                machine.blocked = None;
                continue;
            };
            if !job.cancel_pending && job.remaining_ticks > 0 {
                job.remaining_ticks -= 1;
            }
            if job.cancel_pending || job.remaining_ticks == 0 {
                let credits = if job.cancel_pending { &job.inputs } else { &job.outputs };
                match self.state.inventory.transact(&[], credits) {
                    Ok(_) => {
                        machine.queue.remove(0);
                        machine.blocked = None;
                    }
                    Err(error) => machine.blocked = Some(error),
                }
            }
        }
    }
    fn state_hash(&self) -> u64 {
        let mut h = StateHasher::new();
        h.u64(self.state.tick).str(&self.state.instance_id);
        for (id, n) in self.state.inventory.counts() {
            h.str(id).u64(*n);
        }
        for map in [&self.state.supply, &self.state.delivered] {
            for (id, n) in map {
                h.str(id).u64(*n);
            }
            h.u64(map.len() as u64);
        }
        for (id, m) in &self.state.machines {
            h.str(id);
            for u in &m.upgrades {
                h.str(u);
            }
            h.u64(m.upgrades.len() as u64).u64(m.queue.len() as u64);
            for j in &m.queue {
                h.str(&j.recipe)
                    .u64(j.rule_version as u64)
                    .u64(j.speed_divisor as u64)
                    .u64(j.duration_ticks as u64)
                    .u64(j.remaining_ticks as u64)
                    .bool(j.cancel_pending);
                for terms in [&j.inputs, &j.outputs] {
                    for (id, n) in terms {
                        h.str(id).u64(*n);
                    }
                    h.u64(terms.len() as u64);
                }
            }
        }
        for (seat, serial) in &self.state.last_command {
            h.u64(*seat as u64).u64(*serial);
        }
        h.u64(self.state.last_command.len() as u64);
        h.finish()
    }
}
