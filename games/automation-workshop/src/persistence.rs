//! Engine-framed persistence and explicit game-owned content migration. Originals remain untouched.
use crate::{Definitions, State, Workshop, MAX_STATE_BYTES};
use std::path::Path;
use vesper3d::runtime::{snapshot, Simulation, Snapshot};
use vesper3d::viewer::savestate::{Migration, SaveSlots};
fn schema_v1(mut state: serde_json::Value) -> Result<serde_json::Value, String> {
    let object = state.as_object_mut().ok_or("WORKSHOP_V1: state object required")?;
    let ledger = object.remove("shipped").ok_or("WORKSHOP_V1: missing shipped ledger")?;
    if object.insert("delivered".into(), ledger).is_some() {
        return Err("WORKSHOP_V1: conflicting delivery ledgers".into());
    }
    Ok(state)
}
/// A legacy serialization adapter around the same authority, not a second simulation. Version1
/// makes engine restore verify the old saved hash before any state/content migration is accepted.
struct LegacyV1(Workshop);
impl Simulation for LegacyV1 {
    type Input = <Workshop as Simulation>::Input;
    fn step(&mut self, input: &Self::Input) {
        self.0.step(input);
    }
    fn state_hash(&self) -> u64 {
        self.0.state_hash()
    }
    fn hash_parts(&self) -> Vec<(&'static str, u64)> {
        self.0.hash_parts()
    }
}
impl Snapshot for LegacyV1 {
    const KIND: &'static str = <Workshop as Snapshot>::KIND;
    const VERSION: u32 = 1;
    type State = serde_json::Value;
    fn capture(&self) -> Self::State {
        // State contains only validated integer/string data; no float/non-JSON values can occur.
        let mut value = serde_json::to_value(self.0.capture()).expect("workshop state is JSON data");
        let object = value.as_object_mut().expect("workshop state is an object");
        let ledger = object.remove("delivered").expect("workshop state has a delivery ledger");
        object.insert("shipped".into(), ledger);
        value
    }
    fn restore(&mut self, state: Self::State) -> Result<(), String> {
        let current = serde_json::from_value(schema_v1(state)?).map_err(|e| format!("WORKSHOP_V1_STATE: {e}"))?;
        self.0.restore(current)
    }
    fn content(&self) -> Option<u64> {
        self.0.content()
    }
    fn save_tick(&self) -> u64 {
        self.0.save_tick()
    }
}
impl Snapshot for Workshop {
    const KIND: &'static str = "blueengine-workshop";
    const VERSION: u32 = 2;
    const MIGRATIONS: &'static [Migration] = &[Migration { from: 1, step: schema_v1 }];
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, state: State) -> Result<(), String> {
        self.validate_state(&state)?;
        self.state = state;
        self.recompute_blocked();
        Ok(())
    }
    fn content(&self) -> Option<u64> {
        Some(self.definitions.content_hash())
    }
    fn save_tick(&self) -> u64 {
        self.state.tick
    }
}
/// Restore into a temporary copy bound to EXACT source content, validate against target, then save
/// to a new engine slot. No fallback weakens Snapshot's normal content checks.
pub fn migrate_content(
    live: &mut Workshop,
    original: &[u8],
    source: Definitions,
    destination: &Path,
    slot: &str,
) -> Result<(), String> {
    if original.len() > MAX_STATE_BYTES {
        return Err(format!("WORKSHOP_SAVE_LIMIT: max {MAX_STATE_BYTES} bytes"));
    }
    let frame = vesper3d::viewer::savestate::decode(original).map_err(|e| e.to_string())?;
    if frame.header.content != source.fingerprint() {
        return Err("WORKSHOP_MIGRATION_SOURCE: original save must match the supplied source content exactly".into());
    }
    if source.content_version == 2 {
        // This bounded compatibility policy supports additions, not retroactive V2 retuning.
        for (id, recipe) in &source.recipes {
            if serde_json::to_value(recipe).map_err(|e| e.to_string())?
                != serde_json::to_value(live.definitions.recipes.get(id).ok_or("WORKSHOP_MIGRATION: removed recipe")?)
                    .map_err(|e| e.to_string())?
            {
                return Err(format!("WORKSHOP_MIGRATION: incompatible V2 recipe tuning {id}"));
            }
        }
        for (id, upgrade) in &source.upgrades {
            if serde_json::to_value(upgrade).map_err(|e| e.to_string())?
                != serde_json::to_value(live.definitions.upgrades.get(id).ok_or("WORKSHOP_MIGRATION: removed upgrade")?)
                    .map_err(|e| e.to_string())?
            {
                return Err(format!("WORKSHOP_MIGRATION: incompatible V2 upgrade tuning {id}"));
            }
        }
    } else if source.content_hash()
        != Definitions::from_json(crate::content::V1_CONTENT, "frozen:content-v1.json")
            .map_err(|e| e.to_string())?
            .content_hash()
    {
        return Err("WORKSHOP_MIGRATION: only the frozen V1 content is supported".into());
    }
    let source_version = source.content_version;
    let mut candidate = Workshop::new(source, 2)?;
    if frame.header.version == 1 {
        if source_version != 1 {
            return Err("WORKSHOP_MIGRATION_SOURCE: version-one state requires frozen version-one content".into());
        }
        let mut legacy = LegacyV1(candidate);
        snapshot::restore(&mut legacy, original).map_err(|e| e.to_string())?;
        candidate = legacy.0;
    } else {
        snapshot::restore(&mut candidate, original).map_err(|e| e.to_string())?;
    }
    candidate.definitions = live.definitions.clone();
    candidate.validate_state(&candidate.state)?;
    candidate.recompute_blocked();
    let bytes = snapshot::save(&candidate, "Migrated workshop copy").map_err(|e| e.to_string())?;
    if bytes.len() > MAX_STATE_BYTES {
        return Err("WORKSHOP_SAVE_LIMIT: migrated copy exceeds budget".into());
    }
    let slots = SaveSlots::new(destination);
    // Never replace an already completed migrated output. Caller chooses a new slot.
    if slots.path(slot).map_err(|e| e.to_string())?.exists() {
        return Err("WORKSHOP_MIGRATION_DESTINATION: choose a new slot; completed output preserved".into());
    }
    slots.save_framed(slot, &bytes).map_err(|e| e.to_string())?;
    *live = candidate;
    Ok(())
}
pub fn migrate_v1(live: &mut Workshop, original: &[u8], destination: &Path) -> Result<(), String> {
    let source =
        Definitions::from_json(crate::content::V1_CONTENT, "frozen:content-v1.json").map_err(|e| e.to_string())?;
    migrate_content(live, original, source, destination, "migrated-v2")
}
