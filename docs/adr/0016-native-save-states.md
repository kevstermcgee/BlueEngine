# ADR 0016: Native save states

Status: Accepted

## Context

No layer of the engine could save or resume a running game. A game wanting F5/F9,
autosave, or crash recovery would have hand-rolled its own file format, with no
shared answer for atomic writes, corruption, versioning across content updates, or
proving a load actually reproduces what was saved. This is exactly the kind of
capability that belongs at the engine, not reimplemented per game.

Building it surfaced two facts about the physics library (`rapier3d`) that the
design had to answer:

1. **rapier's default build is not deterministic run to run.** Two freshly built,
   identically driven worlds diverged after enough ticks (its default relies on
   randomly seeded internal hash maps). This is a correctness problem independent
   of save states — it would also break deterministic replay and networked
   reconciliation — and is fixed by enabling rapier's own `enhanced-determinism`
   feature.
2. **rapier's native `serde-serialize` snapshot was tried and rejected.** It
   conflicts with the engine's exact `serde = "=1.0.219"` pin, and even worked
   around, JSON cannot represent the infinities rapier's internal state can carry.
   A save format that can silently fail to round-trip the physics engine's own
   state is worse than not having native snapshots at all.

## Decision

One framed, versioned, checksummed file format (`viewer::savestate`), shared by
every tier, built from portable state the engine itself owns and can validate —
never the physics library's internals:

- **Frame**: magic, frame format version, header (kind/version/content
  fingerprint/tick/label), payload, a SHA-256 trailer over every preceding byte.
  `write_atomic` writes to a temp file in the same directory, verifies the
  previous file before keeping it as `.bak`, renames over the target, then reads
  the result back; a failed verification restores the backup. A save is never
  half-written and a file whose bytes do not verify is never loaded
  (`AI-INVARIANT SAVE-ATOMIC-001`).
- **Engine worlds** (`savestate::world`, `HeadlessWorld::save_bytes`/
  `restore_bytes`): players, rule state, prop poses/velocities/sleep/carrier,
  lifecycle tiers — not rapier internals. A restore rebuilds the physics scene
  from a **pristine clone captured once at `PropPhysics::new`**, then places every
  prop from the save, so contact caches, islands and sleep bookkeeping after a
  load depend only on the save file, never on what the world happened to do
  before loading it. Floats are written widened to 64 bits (`payload_bytes`):
  scanning 4.8 billion random `f32` found 3 that do not survive the ordinary
  decimal round trip a naive writer would use.
- **Per tier**: `GameSession` (F5/F9, `--load`/`--save-dir` in the stock and
  generated clients — refuses when online, since the server owns that world);
  `DedicatedServer::with_autosave` and `be2-headless --autosave/--load`; and for
  a custom simulation (ADR 0015), `devkit::Snapshot` — a game implements
  `capture`/`restore` once, and `snapshot::assert_resumes_exactly` proves in one
  line of a test that a save from any tick, loaded into a **brand new**
  simulation, replays identically to the uninterrupted run.
- **All-or-nothing loads.** A state is parsed, validated and checked before
  anything changes; if applying it fails, or the restored state's hash does not
  match the one written with the save, the running game or simulation is put back
  exactly as it was.
- **Formats evolve**: a `Migration` chain per payload kind upgrades older
  payloads; a payload newer than the build understands is refused, never guessed.
- **Inspection**: `be2-tools save-info FILE [GAME.json]` verifies and summarizes a
  save without loading it into anything.

See `docs/SAVE_STATE.md` for the full contract, file layout and versioning policy.

## Consequences

Physics resumes bit-identically where it is at rest or in free fall; a save taken
mid-settle resumes as a valid but not bit-identical continuation (solver noise
from the rebuilt contact cache), which is documented, not hidden. Enabling
`enhanced-determinism` is a behavior change to the physics solver, not only to
saving — it is the fix for a real non-determinism bug, evaluated for cost like
any other engine change. A save file is now a reliable bug report: replaying it
depends only on its bytes.

## Rejected or deferred

- **Native rapier snapshots** (`serde-serialize`): rejected outright (see
  Context) in favor of the engine's own portable state.
- **Signing or encrypting saves**: the checksum guards against corruption and
  accidents, not against a player editing their own save file; that boundary was
  kept explicit rather than implying tamper-resistance the format does not have.
- **A save-menu UI, cloud sync, or compression**: left to the game;
  `SaveSlots::list` gives it the label/game/tick/time and reports damaged slots
  instead of offering them.
- **Restoring physics contact caches** to make a mid-settle resume bit-identical:
  not attempted, since it would mean serializing rapier internals (the same
  problem the native-snapshot rejection describes).
