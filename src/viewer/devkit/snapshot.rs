//! Save and load a game's own simulation: F5 / F9 quick saves, autosaves and "continue" for a game that
//! keeps its rules in Rust.
//!
//! The engine's stock runner already saves its world (see [`savestate`]). A
//! custom simulation implements [`Snapshot`] once (say what its state is, and how to put one back) and gets
//! the same guarantees:
//!
//! * a save is a framed, checksummed file written atomically with a last-good backup, so a crash or a full
//!   disk never costs the player the previous save;
//! * a load verifies the file end to end, refuses other games' saves, other kinds and newer versions with a
//!   plain error, upgrades older payloads through [`Migration`] steps, and is **all-or-nothing**: if the
//!   state is refused, or restoring it does not reproduce the saved [`Simulation::state_hash`], the
//!   simulation is put back exactly as it was;
//! * [`assert_resumes_exactly`] proves in one line of a test that a save taken at any tick and loaded into a
//!   brand new simulation plays out identically to the run that was never interrupted.
//!
//! ```
//! use serde::{Deserialize, Serialize};
//! use vesper3d::viewer::devkit::{
//!     snapshot::{self, assert_resumes_exactly},
//!     Simulation, Snapshot, StateHasher,
//! };
//!
//! #[derive(Default)]
//! struct Counter { total: u32 }
//! impl Simulation for Counter {
//!     type Input = u32;
//!     fn step(&mut self, add: &u32) { self.total += add; }
//!     fn state_hash(&self) -> u64 { StateHasher::new().u32(self.total).finish() }
//! }
//! #[derive(Serialize, Deserialize)]
//! struct CounterState { total: u32 }
//! impl Snapshot for Counter {
//!     const KIND: &'static str = "counter";
//!     type State = CounterState;
//!     fn capture(&self) -> CounterState { CounterState { total: self.total } }
//!     fn restore(&mut self, state: CounterState) -> Result<(), String> {
//!         self.total = state.total;
//!         Ok(())
//!     }
//! }
//!
//! let mut game = Counter::default();
//! game.step(&5);
//! let bytes = snapshot::save(&game, "after five").unwrap();
//! game.step(&100);
//! let header = snapshot::restore(&mut game, &bytes).unwrap();
//! assert_eq!((header.label.as_str(), game.total), ("after five", 5));
//! assert_resumes_exactly(Counter::default, &[1, 2, 3, 4, 5, 6], 2);
//! ```
use super::sim::Simulation;
use crate::viewer::savestate::{
    self, check_header, content_string, decode, migrate, Expect, Loaded, SaveError, SaveHeader,
    SaveSlots, Source, AUTO_SLOT,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

pub use crate::viewer::savestate::Migration;

/// A [`Simulation`] whose complete state can be written to a save file and put back.
///
/// `capture` and `restore` are the only parts a game writes. Everything that decides the future of the
/// simulation belongs in [`Snapshot::State`]: positions and velocities, timers, scores, spawn queues and
/// the [`Rng`](super::Rng) stream (it serialises as one number). Presentation (particles, camera, sounds)
/// does not: rebuild it from the state after a load.
pub trait Snapshot: Simulation {
    /// Names the payload in the save header: lower-case letters, digits, `-` and `_`. Never change it once
    /// players have saves; it is how a save of another game or another kind is recognised.
    const KIND: &'static str;
    /// Layout version of [`Snapshot::State`]. Start at 1 and add one whenever the state changes shape,
    /// together with a [`Migration`] from the old version in [`Snapshot::MIGRATIONS`].
    const VERSION: u32 = 1;
    /// One step per old version, each turning a payload of version `n` into version `n + 1`. A save older
    /// than the newest step this list can reach is refused, never guessed at.
    const MIGRATIONS: &'static [Migration] = &[];
    /// The plain-data description of the simulation.
    type State: Serialize + DeserializeOwned;
    /// Copy the whole state out. Floats must be finite (JSON cannot hold NaN or infinity); a state that
    /// cannot be written and read back is refused at save time, never discovered at load time.
    fn capture(&self) -> Self::State;
    /// Check `state` and put it in place. Return `Err` with a plain sentence for a state this game cannot
    /// resume (an unknown level, a count out of range). The caller restores the previous state itself when
    /// this returns an error or the result does not match the saved [`Simulation::state_hash`], so
    /// `restore` does not have to be atomic, only correct.
    fn restore(&mut self, state: Self::State) -> Result<(), String>;
    /// Fingerprint of what a save must match to make sense: the level, the map, the tuning file. A save only
    /// loads into a game whose fingerprint is equal. `None` (the default) accepts any save of this kind.
    fn content(&self) -> Option<u64> {
        None
    }
    /// The tick to show in a load menu (the header records it). The default is 0.
    fn save_tick(&self) -> u64 {
        0
    }
}

/// The payload of a game save: the state, and the [`Simulation::state_hash`] it had when it was saved.
#[derive(Serialize, Deserialize)]
struct Envelope<T> {
    hash: String,
    state: T,
}

fn build<S: Snapshot>(sim: &S, label: &str) -> Result<(SaveHeader, Vec<u8>), SaveError> {
    let mut header = SaveHeader::new(S::KIND, S::VERSION, label).with_tick(sim.save_tick());
    if let Some(content) = sim.content() {
        header = header.with_content(content);
    }
    let envelope = Envelope {
        hash: content_string(sim.state_hash()),
        state: sim.capture(),
    };
    Ok((header, savestate::payload_bytes(&envelope)?))
}

/// Serialise the simulation as save-file bytes. `label` is the text a load menu shows.
pub fn save<S: Snapshot>(sim: &S, label: &str) -> Result<Vec<u8>, SaveError> {
    let (header, payload) = build(sim, label)?;
    savestate::encode(&header, &payload)
}

/// Verify save-file bytes and resume from them. All-or-nothing: on any error the simulation is exactly as it
/// was before the call. The state hash is compared after a restore of a save written by the current
/// [`Snapshot::VERSION`] (a migrated save may legitimately hash differently).
pub fn restore<S: Snapshot>(sim: &mut S, bytes: &[u8]) -> Result<SaveHeader, SaveError> {
    let decoded = decode(bytes)?;
    apply(sim, decoded.header, decoded.payload)
}

/// Write the simulation to a named slot (atomic, keeping the previous good save as its backup).
pub fn save_to_slot<S: Snapshot>(
    sim: &S,
    slots: &SaveSlots,
    slot: &str,
    label: &str,
) -> Result<(), SaveError> {
    let (header, payload) = build(sim, label)?;
    slots.save(slot, &header, &payload)
}

/// Resume from a slot. When the file is damaged the previous good save is used instead and the returned
/// [`Source`] says so; show that to the player.
pub fn load_from_slot<S: Snapshot>(
    sim: &mut S,
    slots: &SaveSlots,
    slot: &str,
) -> Result<(SaveHeader, Source), SaveError> {
    let Loaded {
        header,
        payload,
        source,
    } = slots.load(slot)?;
    Ok((apply(sim, header, &payload)?, source))
}

/// Resume from what a `--load` flag names: the path of a save file, or a slot in `slots`.
pub fn load_target<S: Snapshot>(
    sim: &mut S,
    slots: &SaveSlots,
    target: &str,
) -> Result<(SaveHeader, Source), SaveError> {
    let Loaded {
        header,
        payload,
        source,
    } = slots.open(target)?;
    Ok((apply(sim, header, &payload)?, source))
}

/// Rotating autosave: the newest is slot `auto`, older ones `auto-2`, `auto-3`, ... up to `keep` files.
pub fn autosave<S: Snapshot>(
    sim: &S,
    slots: &SaveSlots,
    keep: usize,
    label: &str,
) -> Result<(), SaveError> {
    let (header, payload) = build(sim, label)?;
    slots.save_ring(AUTO_SLOT, keep, &header, &payload)
}

fn apply<S: Snapshot>(
    sim: &mut S,
    header: SaveHeader,
    payload: &[u8],
) -> Result<SaveHeader, SaveError> {
    let expect = Expect {
        kind: S::KIND,
        version: S::VERSION,
        migrations: S::MIGRATIONS,
        content: sim.content(),
    };
    check_header(&header, expect)?;
    let Value::Object(mut fields) = serde_json::from_slice::<Value>(payload)
        .map_err(|e| SaveError::Invalid(format!("payload is not valid JSON: {e}")))?
    else {
        return Err(SaveError::Invalid("payload is not a saved state".into()));
    };
    let hash = fields
        .remove("hash")
        .and_then(|v| v.as_str().map(str::to_owned));
    let Some(state) = fields.remove("state") else {
        return Err(SaveError::Invalid("the save holds no state".into()));
    };
    let current = header.version == S::VERSION;
    let state = migrate(state, header.version, S::VERSION, S::MIGRATIONS)?;
    let state: S::State =
        serde_json::from_value(state).map_err(|e| SaveError::Invalid(e.to_string()))?;
    let before = sim.capture();
    let outcome = sim.restore(state).map_err(SaveError::Invalid).and_then(|()| match hash {
        Some(saved) if current && content_string(sim.state_hash()) != saved => Err(SaveError::Invalid(
            "restoring the save did not reproduce the saved state; capture() must include everything \
             state_hash() covers"
                .into(),
        )),
        _ => Ok(()),
    });
    match outcome {
        Ok(()) => Ok(header),
        Err(error) => {
            // A failed load must not cost the running game anything.
            let _ = sim.restore(before);
            Err(error)
        }
    }
}

/// Test helper: save at every `every`-th tick of a run of `inputs`, load each save into a **brand new**
/// simulation from `make`, play the rest of the inputs, and panic (naming the tick) unless every tick of every
/// resumed run has the state hash of the run that was never interrupted, and a restored simulation saves
/// back to the same state.
///
/// This is what proves that [`Snapshot::State`] really holds everything: a forgotten timer or random-number
/// stream shows up as a divergence at the first tick that reads it.
///
/// ```
/// # use serde::{Deserialize, Serialize};
/// # use vesper3d::viewer::devkit::{snapshot::assert_resumes_exactly, Simulation, Snapshot, StateHasher};
/// # #[derive(Default)] struct Sum(u64);
/// # impl Simulation for Sum { type Input = u64; fn step(&mut self, i: &u64) { self.0 += i; }
/// #   fn state_hash(&self) -> u64 { StateHasher::new().u64(self.0).finish() } }
/// # impl Snapshot for Sum { const KIND: &'static str = "sum"; type State = u64;
/// #   fn capture(&self) -> u64 { self.0 }
/// #   fn restore(&mut self, s: u64) -> Result<(), String> { self.0 = s; Ok(()) } }
/// assert_resumes_exactly(Sum::default, &[1, 2, 3, 4], 1);
/// ```
pub fn assert_resumes_exactly<S: Snapshot>(
    make: impl Fn() -> S,
    inputs: &[S::Input],
    every: usize,
) {
    let every = every.max(1);
    let mut reference = make();
    let mut hashes = vec![reference.state_hash()];
    let mut saves = Vec::new();
    for (tick, input) in inputs.iter().enumerate() {
        if tick % every == 0 {
            saves.push((
                tick,
                save(&reference, "split").unwrap_or_else(|e| panic!("saving at tick {tick}: {e}")),
            ));
        }
        reference.step(input);
        hashes.push(reference.state_hash());
    }
    saves.push((
        inputs.len(),
        save(&reference, "end").unwrap_or_else(|e| panic!("saving at the end: {e}")),
    ));
    for (split, bytes) in saves {
        let mut resumed = make();
        restore(&mut resumed, &bytes)
            .unwrap_or_else(|e| panic!("loading the save from tick {split}: {e}"));
        assert_eq!(
            resumed.state_hash(),
            hashes[split],
            "the state loaded from the save at tick {split} differs from the state that was saved"
        );
        let payload = |sim: &S| {
            build(sim, "x")
                .map(|(_, p)| p)
                .unwrap_or_else(|e| panic!("saving again: {e}"))
        };
        assert_eq!(
            payload(&resumed),
            payload_at(&make, &inputs[..split]),
            "a simulation loaded at tick {split} does not save back to the same state"
        );
        for (offset, input) in inputs[split..].iter().enumerate() {
            resumed.step(input);
            assert_eq!(
                resumed.state_hash(),
                hashes[split + offset + 1],
                "the run resumed from the save at tick {split} diverged at tick {}",
                split + offset + 1
            );
        }
    }
}

/// The payload a fresh simulation has after running `inputs` (the reference for "saves back the same").
fn payload_at<S: Snapshot>(make: &impl Fn() -> S, inputs: &[S::Input]) -> Vec<u8> {
    let mut sim = make();
    for input in inputs {
        sim.step(input);
    }
    build(&sim, "x")
        .map(|(_, p)| p)
        .unwrap_or_else(|e| panic!("saving the reference: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewer::devkit::{Rng, StateHasher};
    use std::path::PathBuf;

    /// A little game with every kind of state a real one has: numbers, a stream of random numbers, a queue.
    #[derive(Clone)]
    struct Walker {
        x: f32,
        steps: u64,
        rng: Rng,
        queue: Vec<u8>,
        level: u32,
    }
    impl Walker {
        fn new(level: u32) -> Self {
            Self {
                x: 0.,
                steps: 0,
                rng: Rng::new(7),
                queue: vec![],
                level,
            }
        }
    }
    impl Simulation for Walker {
        type Input = f32;
        fn step(&mut self, forward: &f32) {
            self.steps += 1;
            self.x += forward * 0.1 + self.rng.range(-0.01, 0.01);
            if self.rng.chance(0.2) {
                self.queue.push(self.rng.below(100) as u8);
            }
            if self.queue.len() > 5 {
                self.queue.remove(0);
            }
        }
        fn state_hash(&self) -> u64 {
            let mut h = StateHasher::new();
            h.u64(self.steps)
                .f32(self.x)
                .u64(self.rng.state())
                .bytes(&self.queue);
            h.finish()
        }
    }
    #[derive(Serialize, Deserialize)]
    struct WalkerState {
        x: f32,
        steps: u64,
        rng: Rng,
        queue: Vec<u8>,
    }
    impl Snapshot for Walker {
        const KIND: &'static str = "walker";
        type State = WalkerState;
        fn capture(&self) -> WalkerState {
            WalkerState {
                x: self.x,
                steps: self.steps,
                rng: self.rng.clone(),
                queue: self.queue.clone(),
            }
        }
        fn restore(&mut self, s: WalkerState) -> Result<(), String> {
            if s.queue.len() > 5 {
                return Err(format!(
                    "the queue holds {} items, at most 5 fit",
                    s.queue.len()
                ));
            }
            self.x = s.x;
            self.steps = s.steps;
            self.rng = s.rng;
            self.queue = s.queue;
            Ok(())
        }
        fn content(&self) -> Option<u64> {
            Some(u64::from(self.level))
        }
        fn save_tick(&self) -> u64 {
            self.steps
        }
    }

    fn inputs(n: usize) -> Vec<f32> {
        (0..n).map(|i| if i % 7 < 4 { 1. } else { -0.5 }).collect()
    }
    fn run(sim: &mut Walker, n: usize) {
        for input in inputs(n) {
            sim.step(&input);
        }
    }
    struct Dir(PathBuf);
    impl Dir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "devkit-snapshot-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            Self(dir)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_save_resumes_identically_in_a_brand_new_simulation_at_every_tick() {
        assert_resumes_exactly(|| Walker::new(1), &inputs(120), 1);
    }

    #[test]
    fn a_state_that_forgets_something_is_caught_by_the_proof() {
        // A simulation whose `capture` drops the random stream cannot resume: the helper must say where.
        #[derive(Clone)]
        struct Forgetful(Walker);
        impl Simulation for Forgetful {
            type Input = f32;
            fn step(&mut self, i: &f32) {
                self.0.step(i)
            }
            fn state_hash(&self) -> u64 {
                self.0.state_hash()
            }
        }
        impl Snapshot for Forgetful {
            const KIND: &'static str = "forgetful";
            type State = (f32, u64);
            fn capture(&self) -> (f32, u64) {
                (self.0.x, self.0.steps)
            }
            fn restore(&mut self, s: (f32, u64)) -> Result<(), String> {
                (self.0.x, self.0.steps) = s;
                Ok(())
            }
        }
        let result = std::panic::catch_unwind(|| {
            assert_resumes_exactly(|| Forgetful(Walker::new(1)), &inputs(30), 5);
        });
        let message = *result
            .expect_err("forgetting the rng must be caught")
            .downcast::<String>()
            .unwrap();
        assert!(
            message.contains("did not reproduce the saved state") || message.contains("diverged"),
            "{message}"
        );
    }

    #[test]
    fn a_refused_or_failed_load_leaves_the_simulation_untouched() {
        let mut game = Walker::new(1);
        run(&mut game, 40);
        let bytes = save(&game, "good").unwrap();
        run(&mut game, 25);
        let before = (game.state_hash(), game.x, game.steps);
        let unchanged = |game: &Walker| (game.state_hash(), game.x, game.steps) == before;

        let mut damaged = bytes.clone();
        damaged[bytes.len() / 2] ^= 0x10;
        assert!(matches!(
            restore(&mut game, &damaged),
            Err(SaveError::Corrupt(_))
        ));
        assert!(restore(&mut game, &bytes[..bytes.len() - 9]).is_err());
        assert!(matches!(
            restore(&mut game, b"nope"),
            Err(SaveError::NotASave)
        ));
        assert!(unchanged(&game));

        // Well formed, but for another level (content), another kind, or a newer payload version.
        let mut other = Walker::new(2);
        assert!(matches!(
            restore(&mut other, &bytes),
            Err(SaveError::WrongContent { .. })
        ));
        let header = SaveHeader::new("other-game", 1, "x").with_content(1);
        let alien = savestate::encode(&header, br#"{"hash":"0","state":{}}"#).unwrap();
        assert!(matches!(
            restore(&mut game, &alien),
            Err(SaveError::WrongKind { .. })
        ));
        let header = SaveHeader::new("walker", 2, "x").with_content(1);
        let newer = savestate::encode(&header, br#"{"hash":"0","state":{}}"#).unwrap();
        assert!(matches!(
            restore(&mut game, &newer),
            Err(SaveError::NewerVersion { .. })
        ));
        assert!(unchanged(&game));

        // A payload that parses but is refused by `restore` (a queue that cannot exist) changes nothing,
        // and neither does one whose state hash the restored simulation cannot reproduce.
        let header = SaveHeader::new("walker", 1, "x").with_content(1);
        let long = br#"{"hash":"0","state":{"x":1.0,"steps":3,"rng":5,"queue":[1,2,3,4,5,6]}}"#;
        let refused = savestate::encode(&header, long).unwrap();
        assert!(
            matches!(restore(&mut game, &refused), Err(SaveError::Invalid(m)) if m.contains("at most 5"))
        );
        assert!(unchanged(&game));
        let liar = br#"{"hash":"0123456789abcdef","state":{"x":1.0,"steps":3,"rng":5,"queue":[]}}"#;
        let liar = savestate::encode(&header, liar).unwrap();
        assert!(
            matches!(restore(&mut game, &liar), Err(SaveError::Invalid(m)) if m.contains("did not reproduce"))
        );
        assert!(
            unchanged(&game),
            "a failed hash check must roll the state back"
        );
        assert!(matches!(
            restore(&mut game, &savestate::encode(&header, b"[1,2]").unwrap()),
            Err(SaveError::Invalid(_))
        ));
        let zero_rng = br#"{"hash":"0","state":{"x":1.0,"steps":3,"rng":0,"queue":[]}}"#;
        assert!(
            restore(&mut game, &savestate::encode(&header, zero_rng).unwrap()).is_err(),
            "a zero rng state is refused"
        );
        assert!(unchanged(&game));
        // And the good save still loads afterwards.
        restore(&mut game, &bytes).unwrap();
        assert_eq!(game.steps, 40);
    }

    /// Version 2 of the walker adds a `speed` field; version 1 saves are upgraded by a migration.
    #[derive(Clone)]
    struct WalkerV2 {
        inner: Walker,
        speed: f32,
    }
    #[derive(Serialize, Deserialize)]
    struct WalkerV2State {
        x: f32,
        steps: u64,
        rng: Rng,
        queue: Vec<u8>,
        speed: f32,
    }
    impl Simulation for WalkerV2 {
        type Input = f32;
        fn step(&mut self, i: &f32) {
            self.inner.step(&(i * self.speed))
        }
        fn state_hash(&self) -> u64 {
            let mut h = StateHasher::new();
            h.u64(self.inner.state_hash()).f32(self.speed);
            h.finish()
        }
    }
    impl Snapshot for WalkerV2 {
        const KIND: &'static str = "walker";
        const VERSION: u32 = 2;
        const MIGRATIONS: &'static [Migration] = &[Migration {
            from: 1,
            step: |mut v| {
                v.as_object_mut()
                    .ok_or("not an object")?
                    .insert("speed".into(), 1.0.into());
                Ok(v)
            },
        }];
        type State = WalkerV2State;
        fn capture(&self) -> WalkerV2State {
            let s = self.inner.capture();
            WalkerV2State {
                x: s.x,
                steps: s.steps,
                rng: s.rng,
                queue: s.queue,
                speed: self.speed,
            }
        }
        fn restore(&mut self, s: WalkerV2State) -> Result<(), String> {
            self.inner.restore(WalkerState {
                x: s.x,
                steps: s.steps,
                rng: s.rng,
                queue: s.queue,
            })?;
            self.speed = s.speed;
            Ok(())
        }
        fn content(&self) -> Option<u64> {
            self.inner.content()
        }
    }

    #[test]
    fn an_older_payload_version_is_migrated_and_a_missing_migration_is_refused() {
        let mut old = Walker::new(1);
        run(&mut old, 30);
        let v1 = save(&old, "old save").unwrap();
        let mut new = WalkerV2 {
            inner: Walker::new(1),
            speed: 3.,
        };
        let header = restore(&mut new, &v1).unwrap();
        assert_eq!((header.version, header.label.as_str()), (1, "old save"));
        assert_eq!(
            (new.inner.steps, new.speed),
            (30, 1.),
            "the migration supplies the new field"
        );
        // Saving now writes version 2 and it round-trips without a migration.
        let v2 = save(&new, "new save").unwrap();
        assert_eq!(decode(&v2).unwrap().header.version, 2);
        let mut again = WalkerV2 {
            inner: Walker::new(1),
            speed: 0.5,
        };
        restore(&mut again, &v2).unwrap();
        assert_eq!(again.state_hash(), new.state_hash());
        // A game that forgot to write the migration refuses the old save rather than guessing.
        #[derive(Clone)]
        struct NoMigration(WalkerV2);
        impl Simulation for NoMigration {
            type Input = f32;
            fn step(&mut self, i: &f32) {
                self.0.step(i)
            }
            fn state_hash(&self) -> u64 {
                self.0.state_hash()
            }
        }
        impl Snapshot for NoMigration {
            const KIND: &'static str = "walker";
            const VERSION: u32 = 2;
            type State = WalkerV2State;
            fn capture(&self) -> WalkerV2State {
                self.0.capture()
            }
            fn restore(&mut self, s: WalkerV2State) -> Result<(), String> {
                self.0.restore(s)
            }
            fn content(&self) -> Option<u64> {
                self.0.content()
            }
        }
        let mut lazy = NoMigration(WalkerV2 {
            inner: Walker::new(1),
            speed: 1.,
        });
        assert!(
            matches!(restore(&mut lazy, &v1), Err(SaveError::Invalid(m)) if m.contains("no migration"))
        );
    }

    #[test]
    fn slots_and_the_autosave_ring_work_and_fall_back_to_the_backup() {
        let dir = Dir::new("slots");
        let slots = SaveSlots::new(&dir.0);
        let mut game = Walker::new(1);
        assert!(matches!(
            load_from_slot(&mut game, &slots, "quick"),
            Err(SaveError::NotFound(_))
        ));
        run(&mut game, 20);
        save_to_slot(&game, &slots, "quick", "first").unwrap();
        let first = game.state_hash();
        run(&mut game, 20);
        save_to_slot(&game, &slots, "quick", "second").unwrap();
        let second = game.state_hash();
        run(&mut game, 20);
        let (header, source) = load_from_slot(&mut game, &slots, "quick").unwrap();
        assert_eq!((header.label.as_str(), source), ("second", Source::Primary));
        assert_eq!(game.state_hash(), second);
        let path = slots.path("quick").unwrap();
        let mut file = std::fs::read(&path).unwrap();
        let mid = file.len() / 2;
        file[mid] ^= 0x44;
        std::fs::write(&path, file).unwrap();
        let (header, source) = load_from_slot(&mut game, &slots, "quick").unwrap();
        assert_eq!(header.label, "first");
        assert!(matches!(source, Source::Backup(_)));
        assert_eq!(game.state_hash(), first);
        for i in 0..5 {
            run(&mut game, 3);
            autosave(&game, &slots, 3, &format!("auto {i}")).unwrap();
        }
        let names: Vec<String> = slots
            .list()
            .unwrap()
            .into_iter()
            .map(|s| s.slot)
            .filter(|s| s.starts_with("auto"))
            .collect();
        assert_eq!(names.len(), 3, "{names:?}");
        let (header, _) = load_from_slot(&mut game, &slots, "auto").unwrap();
        assert_eq!(header.label, "auto 4");
        let (header, _) = load_from_slot(&mut game, &slots, "auto-3").unwrap();
        assert_eq!(header.label, "auto 2");
    }

    #[test]
    fn a_state_that_cannot_be_written_and_read_back_is_refused_when_saving() {
        struct Broken(f32);
        impl Simulation for Broken {
            type Input = ();
            fn step(&mut self, _: &()) {}
            fn state_hash(&self) -> u64 {
                0
            }
        }
        impl Snapshot for Broken {
            const KIND: &'static str = "broken";
            type State = f32;
            fn capture(&self) -> f32 {
                self.0
            }
            fn restore(&mut self, s: f32) -> Result<(), String> {
                self.0 = s;
                Ok(())
            }
        }
        assert!(save(&Broken(1.5), "fine").is_ok());
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let error = save(&Broken(bad), "bad").unwrap_err();
            assert!(matches!(error, SaveError::Invalid(_)), "{bad}: {error}");
        }
    }

    #[test]
    fn floats_come_back_bit_for_bit() {
        // Values that a naive decimal round trip can move by one ulp: subnormals, extremes, awkward fractions.
        let mut rng = Rng::new(99);
        // 0x15ae43fd is a real one: its shortest f32 decimal (7.038531e-26), parsed through a 64-bit float as
        // JSON readers do, rounds twice and lands one ulp away. Found by scanning 4.8e9 random f32 (3 hits: this
        // value, its negative, and a repeat). `payload_bytes` writes floats widened to 64 bits, which is exact.
        let hard = f32::from_bits(0x15ae43fd);
        assert_ne!(
            format!("{hard}").parse::<f64>().unwrap() as f32,
            hard,
            "the decimal-through-f64 trap this test guards against no longer reproduces"
        );
        let mut samples = vec![
            0.1f32,
            1. / 3.,
            f32::MIN_POSITIVE,
            f32::EPSILON,
            f32::MAX,
            -f32::MAX,
            1e-45,
            -0.0,
            16_777_217.,
            hard,
            -hard,
        ];
        while samples.len() < 20_000 {
            let bits = (rng.next_u64() >> 32) as u32;
            let f = f32::from_bits(bits);
            if f.is_finite() {
                samples.push(f);
            }
        }
        struct Floats(Vec<f32>);
        impl Simulation for Floats {
            type Input = ();
            fn step(&mut self, _: &()) {}
            fn state_hash(&self) -> u64 {
                let mut h = StateHasher::new();
                for f in &self.0 {
                    h.u32(f.to_bits());
                }
                h.finish()
            }
        }
        impl Snapshot for Floats {
            const KIND: &'static str = "floats";
            type State = Vec<f32>;
            fn capture(&self) -> Vec<f32> {
                self.0.clone()
            }
            fn restore(&mut self, s: Vec<f32>) -> Result<(), String> {
                self.0 = s;
                Ok(())
            }
        }
        let bytes = save(&Floats(samples.clone()), "floats").unwrap();
        let mut back = Floats(Vec::new());
        restore(&mut back, &bytes).unwrap();
        let wrong = samples
            .iter()
            .zip(&back.0)
            .filter(|(a, b)| a.to_bits() != b.to_bits())
            .count();
        assert_eq!(
            (back.0.len(), wrong),
            (samples.len(), 0),
            "{wrong} floats changed in a save round trip"
        );
    }
}
