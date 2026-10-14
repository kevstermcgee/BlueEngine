//! The small workshop on the existing native custom-netplay path.
//!
//! The server calls the same `Workshop::step` as offline play. Clients keep an observational replica;
//! they never commit inventory mutations or run a second authority. The pinned content fingerprint
//! supplies the wire dictionary. Full snapshots recover a brief packet interruption, but this
//! match-oriented adapter cannot reclaim a seat after timeout or admit a new peer during a match.
use crate::{Command, Definitions, Envelope, Job, Workshop};
use std::collections::BTreeMap;
use std::sync::LazyLock;
use vesper3d::runtime::resources::Inventory;
use vesper3d::runtime::{Simulation, StateHasher};
use vesper3d::viewer::net::codec::{Reader, WireError, WireResult, Writer};
use vesper3d::viewer::netplay::wire::{encode_server, ServerMsg, SnapshotMsg, MAX_DATAGRAM};
use vesper3d::viewer::netplay::{ClientView, NetGame, PredictionStats, Seat};

/// The game's wire layout, versioned independently from its save schema and release.
pub const PROTOCOL_VERSION: u32 = 1;
/// This fixture exposes two cooperative player seats; it is not a measured capacity promise.
pub const MAX_PLAYERS: usize = 2;

static CONTENT: LazyLock<(Definitions, Vec<String>)> = LazyLock::new(|| {
    let workshop = Workshop::standard();
    let ids = workshop.state.machines.keys().cloned().collect();
    (workshop.definitions, ids)
});

fn definition_index<T>(map: &BTreeMap<String, T>, id: &str) -> Result<u16, String> {
    map.keys()
        .position(|key| key == id)
        .and_then(|index| u16::try_from(index).ok())
        .ok_or_else(|| format!("online content has no definition '{id}'"))
}

fn definition_id<T>(map: &BTreeMap<String, T>, index: u16) -> WireResult<String> {
    map.keys().nth(index as usize).cloned().ok_or(WireError("unknown workshop definition index"))
}

/// One player intention. The serial is an application command ID, separate from transport sequence.
/// Repeating it under packet loss cannot charge, enqueue or cancel twice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetInput {
    pub serial: u64,
    action: u8,
    machine: u16,
    definition: u16,
    amount: u64,
}

impl NetInput {
    /// Resolve game-owned semantic IDs against the content pinned into this client and server.
    /// `Wait` has no serial and is the default; real player actions require a nonzero serial.
    pub fn from_command(serial: u64, command: &Command) -> Result<Self, String> {
        let machine_index = |id: &str| {
            CONTENT
                .1
                .iter()
                .position(|name| name == id)
                .and_then(|index| u16::try_from(index).ok())
                .ok_or_else(|| format!("online workshop has no machine '{id}'"))
        };
        let mut input = Self { serial, ..Self::default() };
        match command {
            Command::Wait => return Ok(Self::default()),
            Command::Queue { machine, recipe } => {
                input.action = 1;
                input.machine = machine_index(machine)?;
                input.definition = definition_index(&CONTENT.0.recipes, recipe)?;
            }
            Command::Cancel { machine } => {
                input.action = 2;
                input.machine = machine_index(machine)?;
            }
            Command::Gather { item, amount } => {
                input.action = 3;
                input.definition = definition_index(&CONTENT.0.items, item)?;
                input.amount = *amount;
            }
            Command::Deliver { item, amount } => {
                input.action = 4;
                input.definition = definition_index(&CONTENT.0.items, item)?;
                input.amount = *amount;
            }
            Command::Upgrade { machine, upgrade } => {
                input.action = 5;
                input.machine = machine_index(machine)?;
                input.definition = definition_index(&CONTENT.0.upgrades, upgrade)?;
            }
        }
        if serial == 0 {
            return Err("online player actions require a nonzero command serial".into());
        }
        Ok(input)
    }

    fn command(self) -> WireResult<Command> {
        let machine =
            || CONTENT.1.get(self.machine as usize).cloned().ok_or(WireError("unknown workshop machine index"));
        if self.action != 0 && self.serial == 0 {
            return Err(WireError("player action has no command serial"));
        }
        Ok(match self.action {
            0 => Command::Wait,
            1 => Command::Queue { machine: machine()?, recipe: definition_id(&CONTENT.0.recipes, self.definition)? },
            2 => Command::Cancel { machine: machine()? },
            3 => Command::Gather { item: definition_id(&CONTENT.0.items, self.definition)?, amount: self.amount },
            4 => Command::Deliver { item: definition_id(&CONTENT.0.items, self.definition)?, amount: self.amount },
            5 => {
                Command::Upgrade { machine: machine()?, upgrade: definition_id(&CONTENT.0.upgrades, self.definition)? }
            }
            _ => return Err(WireError("unknown workshop player action")),
        })
    }
}

/// Inventory quantities and frozen job amounts retain semantic IDs in the public view.
pub type Quantities = Vec<(String, u64)>;

/// A separately owned machine's observable state, including the authoritative reason it is blocked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineView {
    pub id: String,
    pub queue: Vec<Job>,
    pub upgrades: Vec<String>,
    pub blocked: Option<String>,
}

/// Full observable state, with processed command IDs (including rejected player actions). A processed
/// ID alone does not promise a successful transaction: inspect authoritative quantities, jobs and block
/// reasons. This presentation replica is not a save or a replacement simulation. The fixture has no
/// hidden information and predicts no transactions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkshopSnapshot {
    pub tick: u64,
    pub state_hash: u64,
    pub instance_id: String,
    pub max_units: u64,
    pub inventory: Quantities,
    pub capacities: Quantities,
    pub supply: Quantities,
    pub delivered: Quantities,
    pub machines: Vec<MachineView>,
    pub last_command: Vec<(u8, u64)>,
}

impl WorkshopSnapshot {
    /// Capture all player-observable state in deterministic machine/item order.
    pub fn capture(workshop: &Workshop) -> Self {
        let pairs = |map: &BTreeMap<String, u64>| map.iter().map(|(id, amount)| (id.clone(), *amount)).collect();
        Self {
            tick: workshop.state.tick,
            state_hash: workshop.state_hash(),
            instance_id: workshop.state.instance_id.clone(),
            max_units: workshop.state.inventory.max_units(),
            inventory: pairs(workshop.state.inventory.counts()),
            capacities: pairs(workshop.state.inventory.capacities()),
            supply: pairs(&workshop.state.supply),
            delivered: pairs(&workshop.state.delivered),
            machines: workshop
                .state
                .machines
                .values()
                .map(|machine| MachineView {
                    id: machine.id.clone(),
                    queue: machine.queue.clone(),
                    upgrades: machine.upgrades.iter().cloned().collect(),
                    blocked: machine.blocked.as_ref().map(ToString::to_string),
                })
                .collect(),
            last_command: workshop.state.last_command.iter().map(|(seat, serial)| (*seat, *serial)).collect(),
        }
    }
}

fn write_text(w: &mut Writer, text: &str) {
    // Unlike Writer::text, this never silently truncates a semantic ID or an actionable diagnostic.
    w.varint(text.len() as u64);
    w.raw(text.as_bytes());
}

fn read_text(r: &mut Reader<'_>, limit: u64) -> WireResult<String> {
    let length = r.varint_max(limit)? as usize;
    std::str::from_utf8(r.take(length)?).map(str::to_owned).map_err(|_| WireError("workshop text is not UTF-8"))
}

fn write_quantities(w: &mut Writer, quantities: &Quantities) {
    w.varint(quantities.len() as u64);
    for (id, amount) in quantities {
        w.u16(definition_index(&CONTENT.0.items, id).unwrap_or(u16::MAX));
        w.varint(*amount);
    }
}

fn read_quantities(r: &mut Reader<'_>) -> WireResult<Quantities> {
    let count = r.varint_max(CONTENT.0.items.len() as u64)?;
    let mut result = Vec::new();
    for _ in 0..count {
        let id = definition_id(&CONTENT.0.items, r.u16()?)?;
        if result.iter().any(|(old, _)| old == &id) {
            return Err(WireError("duplicate workshop item in snapshot"));
        }
        result.push((id, r.varint()?));
    }
    Ok(result)
}

/// The existing server owns its lobby/session/input infrastructure; the match owns only workshop rules.
pub struct WorkshopMatch {
    pub workshop: Workshop,
    seat_ids: Vec<u8>,
}

/// The client intentionally performs no resource prediction: inventory mutations await authority.
#[derive(Default)]
pub struct WorkshopView {
    pub snapshot: Option<WorkshopSnapshot>,
}

impl ClientView<WorkshopNet> for WorkshopView {
    fn new() -> Self {
        Self::default()
    }
    fn on_snapshot(&mut self, snapshot: &WorkshopSnapshot, _: Option<usize>, _: &[(u32, NetInput)], _: f64) {
        self.snapshot = Some(snapshot.clone());
    }
    fn on_input(&mut self, _: &NetInput) {}
    fn frame(&mut self, _: f64, _: f32) {}
    fn reset(&mut self) {
        self.snapshot = None;
    }
    fn prediction(&self) -> PredictionStats {
        PredictionStats::default()
    }
}

/// Native custom-simulation adapter. Its two-machine full snapshot must fit the active packet ceiling.
pub struct WorkshopNet;

impl NetGame for WorkshopNet {
    type Input = NetInput;
    type Match = WorkshopMatch;
    type View = WorkshopView;
    type Snapshot = WorkshopSnapshot;
    type Event = ();

    const NAME: &'static str = "automation-workshop";
    const MAX_SEATS: usize = MAX_PLAYERS;
    const CHOICES: u8 = 1;
    const UNIQUE_CHOICES: bool = false;

    fn fingerprint() -> u32 {
        StateHasher::new().u32(PROTOCOL_VERSION).str(&CONTENT.0.fingerprint()).finish() as u32
    }

    fn write_input(input: &NetInput, w: &mut Writer) {
        w.varint(input.serial);
        w.u8(input.action);
        w.u16(input.machine);
        w.u16(input.definition);
        w.varint(input.amount);
    }

    fn read_input(r: &mut Reader<'_>) -> WireResult<NetInput> {
        let input = NetInput {
            serial: r.varint()?,
            action: r.u8()?,
            machine: r.u16()?,
            definition: r.u16()?,
            amount: r.varint()?,
        };
        input.command()?;
        Ok(input)
    }

    fn write_snapshot(snapshot: &WorkshopSnapshot, w: &mut Writer) {
        w.varint(snapshot.tick);
        w.u64(snapshot.state_hash);
        write_text(w, &snapshot.instance_id);
        w.varint(snapshot.max_units);
        write_quantities(w, &snapshot.inventory);
        write_quantities(w, &snapshot.capacities);
        write_quantities(w, &snapshot.supply);
        write_quantities(w, &snapshot.delivered);
        w.varint(snapshot.machines.len() as u64);
        for machine in &snapshot.machines {
            write_text(w, &machine.id);
            w.varint(machine.queue.len() as u64);
            for job in &machine.queue {
                w.u16(definition_index(&CONTENT.0.recipes, &job.recipe).unwrap_or(u16::MAX));
                w.varint(job.rule_version as u64);
                w.varint(job.speed_divisor as u64);
                w.varint(job.duration_ticks as u64);
                w.varint(job.remaining_ticks as u64);
                write_quantities(w, &job.inputs);
                write_quantities(w, &job.outputs);
                w.bool(job.cancel_pending);
            }
            w.varint(machine.upgrades.len() as u64);
            for id in &machine.upgrades {
                w.u16(definition_index(&CONTENT.0.upgrades, id).unwrap_or(u16::MAX));
            }
            w.bool(machine.blocked.is_some());
            if let Some(blocked) = &machine.blocked {
                write_text(w, blocked);
            }
        }
        w.varint(snapshot.last_command.len() as u64);
        for (seat, serial) in &snapshot.last_command {
            w.u8(*seat);
            w.varint(*serial);
        }
    }

    fn read_snapshot(r: &mut Reader<'_>) -> WireResult<WorkshopSnapshot> {
        let tick = r.varint()?;
        let state_hash = r.u64()?;
        let instance_id = read_text(r, 128)?;
        let max_units = r.varint()?;
        let inventory = read_quantities(r)?;
        let capacities = read_quantities(r)?;
        Inventory::from_counts(max_units, capacities.iter().cloned().collect(), inventory.iter().cloned().collect())
            .map_err(|_| WireError("invalid bounded workshop inventory snapshot"))?;
        let supply = read_quantities(r)?;
        let delivered = read_quantities(r)?;
        let machine_count = r.varint_max(1_024)?;
        let mut machines = Vec::new();
        for _ in 0..machine_count {
            let id = read_text(r, 128)?;
            if machines.iter().any(|machine: &MachineView| machine.id == id) {
                return Err(WireError("duplicate workshop machine in snapshot"));
            }
            let job_count = r.varint_max(2)?;
            let mut queue = Vec::new();
            for _ in 0..job_count {
                let recipe = definition_id(&CONTENT.0.recipes, r.u16()?)?;
                let rule_version = r.varint_max(u32::MAX as u64)? as u32;
                let speed_divisor = r.varint_max(u32::MAX as u64)? as u32;
                if rule_version == 0 || speed_divisor == 0 {
                    return Err(WireError("invalid frozen workshop rule version or speed divisor"));
                }
                let duration_ticks = r.varint_max(u32::MAX as u64)? as u32;
                let remaining_ticks = r.varint_max(duration_ticks as u64)? as u32;
                if duration_ticks == 0 {
                    return Err(WireError("zero workshop production duration"));
                }
                let inputs = read_quantities(r)?;
                let outputs = read_quantities(r)?;
                let cancel_pending = r.bool()?;
                queue.push(Job {
                    recipe,
                    rule_version,
                    speed_divisor,
                    duration_ticks,
                    remaining_ticks,
                    inputs,
                    outputs,
                    cancel_pending,
                });
            }
            let upgrade_count = r.varint_max(CONTENT.0.upgrades.len() as u64)?;
            let mut upgrades = Vec::new();
            for _ in 0..upgrade_count {
                let id = definition_id(&CONTENT.0.upgrades, r.u16()?)?;
                if upgrades.contains(&id) {
                    return Err(WireError("duplicate workshop upgrade"));
                }
                upgrades.push(id);
            }
            let blocked = if r.bool()? { Some(read_text(r, 4_096)?) } else { None };
            machines.push(MachineView { id, queue, upgrades, blocked });
        }
        let command_count = r.varint_max(MAX_PLAYERS as u64)?;
        let mut last_command = Vec::new();
        for _ in 0..command_count {
            let seat = r.u8()?;
            if seat as usize >= MAX_PLAYERS || last_command.iter().any(|(old, _)| *old == seat) {
                return Err(WireError("invalid or duplicate workshop command seat"));
            }
            last_command.push((seat, r.varint()?));
        }
        Ok(WorkshopSnapshot {
            tick,
            state_hash,
            instance_id,
            max_units,
            inventory,
            capacities,
            supply,
            delivered,
            machines,
            last_command,
        })
    }

    fn write_event(_: &(), _: &mut Writer) {}
    fn read_event(_: &mut Reader<'_>) -> WireResult<()> {
        Ok(())
    }

    fn start(_: u64, seats: &[Seat], _: usize) -> (WorkshopMatch, Vec<usize>) {
        (
            WorkshopMatch {
                workshop: Workshop::new(CONTENT.0.clone(), CONTENT.1.len())
                    .expect("validated process-pinned workshop content"),
                seat_ids: seats.iter().map(|seat| seat.id).collect(),
            },
            (0..seats.len()).collect(),
        )
    }
    fn participants(game: &WorkshopMatch) -> usize {
        game.seat_ids.len()
    }
    fn step(game: &mut WorkshopMatch, inputs: &[Option<NetInput>]) -> Vec<()> {
        let commands = inputs
            .iter()
            .enumerate()
            .filter_map(|(participant, input)| {
                let input = (*input)?;
                let seat = *game.seat_ids.get(participant)?;
                if input.action == 0 {
                    return None;
                }
                Some(Envelope { seat, serial: input.serial, command: input.command().ok()? })
            })
            .collect();
        game.workshop.step(&commands);
        Vec::new()
    }
    fn release(_: &mut WorkshopMatch, _: usize) {
        // There is no player-owned machine or AI takeover. Shared production continues without input.
    }
    fn snapshot(game: &WorkshopMatch, _: Option<usize>) -> WorkshopSnapshot {
        WorkshopSnapshot::capture(&game.workshop)
    }
    fn is_over(_: &WorkshopMatch) -> bool {
        false
    }
    fn report(game: &WorkshopMatch) -> serde_json::Value {
        serde_json::json!({"tick":game.workshop.state.tick,"machines":game.workshop.state.machines.len(),"fixture":"continuous cooperative workshop"})
    }
}

/// Exact encoded datagram bytes, including the netplay envelope. It does not claim remote delivery.
pub fn snapshot_datagram_bytes(workshop: &Workshop) -> usize {
    encode_server::<WorkshopNet>(&ServerMsg::Snapshot(SnapshotMsg {
        server_tick: 0,
        applied_seq: 0,
        you: 0,
        snapshot: WorkshopSnapshot::capture(workshop),
        events: Vec::new(),
    }))
    .len()
}

/// Check a full snapshot against a selected transport's ceiling, without discarding required state.
pub fn check_snapshot_budget(workshop: &Workshop, transport_payload_limit: usize) -> Result<usize, String> {
    let bytes = snapshot_datagram_bytes(workshop);
    let limit = transport_payload_limit.min(MAX_DATAGRAM);
    if bytes > limit {
        Err(format!("workshop full snapshot needs {bytes} bytes; active native netplay limit is {limit}; larger worlds need a separate bounded replication milestone"))
    } else {
        Ok(bytes)
    }
}
