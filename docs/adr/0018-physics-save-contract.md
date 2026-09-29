# ADR 0018: The save contract a physics-backed simulation can keep

Status: Accepted

## Context

Three small physics games (a throw-and-shove target range, a barrel-dodging climb
and a rolling-ball golf) were built on the `custom-sim` starter with the engine's
`HeadlessWorld` or their own `rapier3d` world as the physics authority. All three
reported the same fact independently: the starter's headline save test,
`snapshot::assert_resumes_exactly`, cannot pass. The state loaded from a save
hashes exactly like the state that was saved, two loads of one save replay
identically, and then the resumed run diverges from the uninterrupted one on the
very next tick (0.1 mm growing to centimetres over a few hundred ticks; a ball
skimming a cup lip did not repeat). `docs/SAVE_STATE.md` already said contact
caches are not saved, but nobody had isolated whether that was the whole cause,
and the helper's failure gave a game nothing weaker to prove instead. A second,
related complaint: when a restore does not reproduce the saved hash, the error
("capture() must include everything state_hash() covers") names no field, so a
game with a dozen hashed pieces bisects by hand.

ADR 0016 rejected serialising rapier's internal state (it conflicts with the
engine's exact serde pin, and JSON cannot hold the infinities that state can
carry) in favour of a portable `WorldState`. This decision had to respect that.

## The experiment

`what_a_restore_forgets_isolated_piece_by_piece` (a test in
`src/viewer/prop_physics.rs`) restores a save into a world rebuilt from the
pristine scene, exactly as `restore` does, then hands that world bit-exact copies
of chosen pieces of the *uninterrupted* world's rapier state at the split and
reports the first tick at which it still diverges:

| scenario | pieces handed over | first divergence |
|---|---|---|
| two boxes, one kicked into the other | none (today's restore) | tick 1 |
| | narrow phase (contact manifolds, warm-start impulses) | never: bit-exact |
| four boxes settling into a pile | none | tick 1 |
| | narrow phase | tick 1 |
| | colliders + broad phase + narrow phase + islands + ccd (all but bodies) | tick 1 |
| | bodies + islands + narrow phase | tick 1 |
| | everything | never |

So the contact cache is the whole difference for simple contacts, and a settling
pile additionally depends on rigid-body-internal state (change flags, CCD state,
active-set ids) and broad-phase proxy order. Only the library's complete state
resumes a pile exactly, and that is precisely the non-portable, version-bound
dump ADR 0016 rejected. Saving the narrow phase alone would help two-body
contacts and mislead everyone else.

## Decision

Keep the portable save. State the contract a physics-backed simulation can meet,
give it the same one-line proof the exact contract has, and make a failed restore
name what differs:

- **`snapshot::assert_loads_replay_identically`** proves that what happens after
  a load is a pure function of the file: the loaded state has the saved hash and
  saves back to the same bytes, two brand-new simulations loaded from one save
  replay identically, and a simulation with a long history of its own loads and
  replays exactly like a brand-new one (a `restore` that forgets to reset something
  the save does not carry fails here).
- **`snapshot::assert_resumes_within(make, inputs, every, tolerance, distance)`**
  bounds the drift from the uninterrupted run with the game's own distance
  measure, and returns the worst drift seen so a test can pin it.
- **`Simulation::hash_parts`** (optional, default empty) lists named pieces of the
  state hash. Saves carry the names and hashes; a restore that does not reproduce
  the saved hash now says `part 'rng' differs`, or that every named part matches
  and `state_hash` covers something `hash_parts` does not. Older saves without
  parts, and builds that list none, get the previous message plus the hint.
- `assert_resumes_exactly` is unchanged and still the bar for everything that is
  not physics. `docs/SAVE_STATE.md`, `docs/CUSTOM_CLIENT.md`, the starter's
  `lib.rs`, `AGENTS.md` and `tests/determinism.rs` say which helper a physics
  game uses. `tests/physics_saves.rs` is the worked example: twenty rolling
  apples pass both new helpers and fail the exact one on the first tick.

Alongside, the additive prop API the same games rebuilt by hand: a movable or
removable prop floor (`set_prop_floor`, kept across restores), `prop_mass`,
`prop_half_extents`, `prop_center_of_mass`, `throw` (an uncapped release),
ID-based accessors with `prop_index` bridging to the index-based `PropPhysics`
API, `SceneBuilder::without_spawn`, and `vesper3d::rapier` re-exporting the
pinned physics crate.

## Consequences

A physics game's F5/F9 is honest: a load is a reliable bug report and a fair
continuation, not an identical one, and the test that says so is one line. "Fair"
is not "near": the drift is a fraction of a millimetre on the first tick and stays
small while bodies slide or rest, but a body that tips, lands on another facet or
collides in one run and not the other is a metre or more away within seconds
(measured in `tests/physics_saves.rs`: kicked cereal boxes 0.99 m, kicked catalog
apples 5 to 6 m). Games that need a near resume keep bodies at rest when they
save, as the target range did, or freeze them. The
strict helper's failure is no longer a dead end. Save payloads of games that list
parts grow by a few dozen bytes and remain loadable by builds that do not know
the field. The isolation test stays in the suite so the finding cannot silently
change: if a future rapier or a future restore makes a pile resume exactly, that
test fails and the documentation is tightened, not the other way round.

## Rejected or deferred

- **Serialising rapier's narrow phase** (contact caches) into `PhysicsSave`: it
  is the whole cause only for isolated contacts, it is rapier-version-bound
  internal state, and ADR 0016's reasons still hold.
- **Disabling warm-starting** (`warmstart_coefficient = 0`) to make the solver
  history-free: the pile experiment shows ordering and body-internal state
  diverge regardless, and it degrades stacking for every stock game.
- **A tolerance built into `assert_resumes_exactly`**: the exact contract must
  stay exact for rules, timers and random streams; the bounded one is a separate,
  named promise.
