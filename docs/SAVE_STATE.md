# Save states

A save state is a snapshot of a running game that can be put back later: F5 quick-save, F9 quick-load,
"continue", crash recovery for a dedicated server, or a file that reproduces a bug on any machine. The engine
owns the hard parts (a file that is never half written, never trusted unverified, never loaded into the wrong
game, and a load that is all-or-nothing) so a game only says what its state is.

| You are building | Save with | Load with | Where the code is |
|---|---|---|---|
| a game document (`game.json`, the stock client) | F5 | F9, or `--load quick` | [`GameSession`](../src/viewer/game_session.rs) |
| a game with its own `Sim` (`custom-sim` template) | F5 (already wired) | F9, `--load` | [`devkit::snapshot`](../src/viewer/devkit/snapshot.rs) |
| a dedicated server | `--autosave SECONDS` (and at shutdown) | `--load auto` | [`DedicatedServer::with_autosave`](../src/viewer/server.rs) |
| tooling or tests | `HeadlessWorld::save_bytes` | `restore_bytes` | [`savestate`](../src/viewer/savestate/mod.rs) |

Everything writes the same file: `<slot>.be2save`, next to the executable in a `saves` folder (or `--save-dir
DIR`). Slot names are lower-case letters, digits, `-` and `_`. F5 and F9 use the slot `quick`; autosaves
rotate through `auto`, `auto-2`, `auto-3`. A packaged game keeps its `dist/saves` when it is repackaged.

## The contract

Each line is enforced by a test named after it (`tests/save_state.rs`, `tests/save_session.rs`, the unit
tests in `src/viewer/savestate/` and `src/viewer/devkit/snapshot.rs`).

- **A save never replaces a good file with a partial one.** Bytes go to a temporary file in the same folder,
  are flushed, and only then renamed over the old save; the old save is kept as `<slot>.be2save.bak`. A full
  disk, a killed process or an antivirus lock leaves the previous save intact (`a_failed_write_leaves_the_old_file...`).
- **A damaged file is never loaded.** A SHA-256 over every byte, plus checked magic, lengths and header, is
  verified first. Flipping any single bit of a save is detected (`every_single_bit_flip_is_detected`);
  truncation at any length and random damage never panic and never load a different file. A damaged slot
  falls back to its backup and says so. The checksum guards against accidents, **not** against a player editing
  their own save: it is not a signature.
- **A save for other content never loads.** World saves carry the content fingerprint of the map and game
  document (the same one the network handshake uses); a save from another game, another kind, or a newer
  payload version is refused with a plain sentence, not guessed at.
- **A load is all-or-nothing.** The whole state is parsed, validated and checked (finite numbers, ranges,
  every prop and player accounted for) before anything changes. If applying it fails, or the restored state
  does not reproduce the checksum written with the save, the running game is put back exactly as it was
  (`a_bad_save_is_refused_and_the_world_is_left_exactly_as_it_was`).
- **A save is exact where physics is at rest or falling, and always reproducible.** Players, rules, timers,
  movers, counters, trigger zones, lifecycle tiers, and every prop's pose, velocity, sleep state and carrier
  resume bit for bit. The physics library's internal contact caches are not saved: a restore rebuilds the
  scene as it was first built and places every prop from the save. So props that are asleep or in free fall
  continue identically to the uninterrupted run, and a pile that was still settling can differ by solver noise.
  Either way **what happens after a load is a pure function of the file**, never of what the game did before
  loading it (`what_happens_after_a_load_depends_only_on_the_save...`), so a save file is a reliable bug report.
- **Numbers survive.** Floats are written widened to 64 bits so every `f32` reads back bit for bit (scanning
  4.8 billion random `f32`, three did not survive the usual decimal round trip); a state that cannot be
  written and read back (NaN, infinity) is refused when saving, not discovered when loading.
- **Formats evolve.** The frame has a format version and every payload kind has its own version plus a chain
  of migration steps; older saves are upgraded, newer ones refused. See [Versioning](#versioning).
- **Online clients do not save.** The server owns an online world. A client refuses F5/F9 with a message;
  save on the server (`--autosave`) instead.

## File layout

| Bytes | Field |
|---|---|
| 0..8 | magic `BE2SAVE` + `0x1a` |
| 8..10 | frame format version (`1`) |
| 10..12 | flags (`0`) |
| 12..16, 16..24 | header length, payload length |
| header | UTF-8 JSON: kind, version, engine, content fingerprint, tick, label, game, time |
| payload | UTF-8 JSON of the state |
| last 32 | SHA-256 of every preceding byte |

Limits: header 64 KiB, payload 64 MiB, at most 8 players, 4096 props, 65 536 lifecycle objects. A length that
lies is refused before anything is allocated.

## A game with its own simulation

Implement [`Snapshot`](../src/viewer/devkit/snapshot.rs) next to `Simulation`: say what the state is
(`capture`) and how to put one back (`restore`, which may refuse a state with a plain sentence).

```rust
impl Snapshot for Sim {
    const KIND: &'static str = "orb-run";  // never change once players have saves
    type State = SimState;                 // a serde struct of everything that decides the future
    fn capture(&self) -> SimState { /* ... */ }
    fn restore(&mut self, state: SimState) -> Result<(), String> { /* validate, then apply */ }
}
```

Then `snapshot::save_to_slot`, `snapshot::load_from_slot` and `snapshot::autosave` do the rest, and one line of
a test proves the state is complete:

```rust
snapshot::assert_resumes_exactly(|| Sim::new(7), &inputs, 25);
```

It saves every 25 ticks of a scripted run, loads each save into a **brand new** `Sim`, replays the remaining
inputs and panics, naming the tick, at the first state hash that differs from the uninterrupted run. A field
you forgot to put in `SimState` shows up as a divergence where the game first reads it. `Rng` serialises as one
number, so a random stream resumes in place. Tie saves to a level or tuning with `Snapshot::content()`.

## Versioning

Two independent numbers, so a change never strands a player's save:

1. **Frame format** (`FRAME_VERSION`): the container. It changes only if the byte layout changes. A build refuses
   a frame newer than it knows with "update the game".
2. **Payload version** (`world::VERSION` for engine worlds, `Snapshot::VERSION` for a game): the state's
   shape. Change the state, bump the version, and add a `Migration { from: n, step }` that turns a version `n`
   payload into `n + 1`. Loading runs the chain (`migrate`); a missing step is an error, never a silent skip. A
   migrated state's hash is not compared (the game may legitimately hash new fields differently).

Never edit a released migration. Adding a field that has a serde `default` needs no migration; renaming,
removing or re-typing one does.

## Inspect a file

`be2-tools save-info FILE [GAME_JSON]` verifies a save without loading it into anything and prints its header,
which file it used (the primary or its backup), and a summary: for an engine world the tick, players, props and
round; for a game's own kind the fields of its state. With a game document it says whether the save belongs to
that game. Exit status 1 and a JSON error mean neither the file nor its backup verifies.

```bash
be2-tools save-info saves/quick.be2save my-game/game.json
```

`be2-headless --game my-game/game.json --load saves/quick.be2save --ticks 600` resumes the world headless, for
reproducing a reported problem.

## What is not covered

- Cloud sync, save-file encryption or signing, compression and screenshots in the save menu. A load menu is a
  game's job: `SaveSlots::list` returns each slot's label, game, tick and time, and reports damaged slots
  instead of offering them.
- Restoring *contact caches* (see the contract): resuming a pile mid-settle is a healthy continuation, not the
  identical one.
- Rewinding an online game: the server's save is a whole-world snapshot, and clients reconnect to it.
