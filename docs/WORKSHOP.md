# Versioned automation workshop

External **custom Rust simulation**, not stock `GameDocument` or a capacity claim:
two independent machines share a bounded inventory.
The game owns recipes, queues, upgrades, balance, controls and drawing. Optional
`vesper3d::runtime::resources` uses the `resources` feature. Fixed 60 Hz rules use
`Simulation`; saves use `Snapshot` and `Lifecycle`. Rendering never advances production.

From the engine checkout:

```text
cargo test --locked --manifest-path games/automation-workshop/Cargo.toml --no-default-features
cargo run --locked --manifest-path games/automation-workshop/Cargo.toml --no-default-features --bin workshop -- validate
cargo run --locked --manifest-path games/automation-workshop/Cargo.toml --no-default-features --bin workshop -- inspect workshop:machine-a
cargo run --locked --manifest-path games/automation-workshop/Cargo.toml --bin automation-workshop
```

`inspect MACHINE-ID` prints bounded JSON from the same authority as the client:
active recipe/definition location, queue, logical ticks left, balances and blockage.
Read-only inspection; tests use public commands.

## One coherent transaction

Public contract: [RESOURCES.md](RESOURCES.md). No machine registration is required.

`Inventory::transact(debits, credits)` validates gross costs against pre-state,
then all final per-item/total bounds before committing together. Credits cannot
finance costs. Duplicate entries use checked integer arithmetic;
invalid/unknown IDs, overflow, missing costs or full destinations change nothing.
Sorted receipts record before/debit/credit/after balances and totals, not network
acknowledgements. The primitive owns no recipes, entities, clock or network policy.

Bounds: 64 resource IDs and 128 combined entries. Inventory schema 1 validates
loaded counts/bounds; store it in `Snapshot::State`, not another save format.
The independent headless ability-cost/refund/cooldown fixture is `tests/resources.rs`.

## Production, blockage and cancellation

Send `Command::Queue { machine, recipe }`, `Cancel { machine }`,
`Gather { item, amount }`, `Deliver { item, amount }` or
`Upgrade { machine, upgrade }` inside a numbered `Envelope { seat, serial, command }`.
Saved command serials prevent repeats. Identical duplicates apply once; conflicting
commands with the same seat/serial reject the complete command batch before mutation.
Logical time/production still advance. The client resumes its serial after loading.

Production consumes declared recipe inputs into the job's saved escrow. Each
fixed step reduces logical time; wall time and rendered frames are not authority.
Completion commits the whole declared output transaction. A full destination
keeps the completed job, its escrow and an actionable `TransactionError`; it does
not silently drop products. Delivering inventory through a player command frees
space and retries production. Cancelling refunds the complete escrow transaction;
if that refund cannot fit, the cancellation remains pending until space is freed.
Two machines update in stable instance-ID order, so contention has a deterministic
winner. Resource counts change only at validated commits or explicitly declared
recipe/source/sink boundaries.

Persistent item/recipe/upgrade IDs are namespaced definitions; machine IDs identify
instances. Presentation labels can change independently. Definitions
are loaded from the packaged asset at startup, validated and indexed once; no
large authored graph is parsed every tick. Data edits require restart, not Rust
compilation. An online session pins its content dictionary on both peers.
Duplicate IDs and unresolved references fail with the file/definition location.
Game identity survives directory renames/releases.

## Saves and compatible content changes

F5/F9 and the `save`/`load` script cues use engine-owned atomic save slots and
backups. Queues, escrow, time remaining, applied upgrades and command serials are
authoritative saved state. This fixture is non-random; future random rules must
save the RNG stream too. `SavePolicy::Exact` remains required for these rules.

Game-state version, content-set identity, game-release version and network protocol
identity are independent. The version-1-to-version-2 import is an explicit
game-owned migration of a **copy** through the engine persistence contract. It
validates the recognized old content and new definitions before accepting progress.
Exact source definitions also permit v2 recipe/upgrade additions over existing items.
V2 item/source/sink rules and existing recipe/upgrade tuning must match; retuning/removal requires another
explicit migration. An explicit copy migration may change item/recipe/upgrade labels only: compatibility ignores labels, while complete content identity and historical save validation still include them. Missing/unsupported versions and unknown content fail;
normal exact-content checks remain enabled and original files are retained.
Keep the original installation/save playable after a failed migration.

From `games/automation-workshop`, use existing inputs and a separate destination:

```text
cargo run --locked --no-default-features --bin workshop -- migrate-v1 OLD_SAVE NEW_SAVE_DIR
cargo run --locked --no-default-features --bin workshop -- migrate-content SOURCE_JSON OLD_SAVE NEW_SAVE_DIR
cargo run --locked --no-default-features --bin workshop -- probe
```

`migrate-content` optionally accepts a final target JSON path; otherwise it uses
packaged content. From the engine root, `python tools/be2.py check --game
games/automation-workshop --loop inner` is the focused check. These are iteration
checks, not Windows installer or physical-device evidence.
The fixture allows 1,024 machines, two queued jobs each and 32 commands/tick; its
probe/import budget is 2 MiB. The native view starts with two machines.
The offline 10/100/1,000-machine ladder runs real rules and reports timing/save cost,
stopping at its declared resource budget. Those probe sizes do not fit the online
snapshot budget and are not supported-world promises.

## Online support is a separate path

The fixture's online adapter implements `viewer::netplay::NetGame` over the same
`Workshop::step` rules and uses `netplay::cli::serve` for its server. A tiny complete
state snapshot must fit `min(active transport payload_limit, 1100 bytes)`, including
framing. Brief client-interruption recovery tests and payload measurements apply to this fixture only.
The current match adapter rejects late join and a fresh connection after a seat
times out; these are tested boundaries, not implemented reconnect features.
Custom netplay does not inherit the stock server's partial replication, interest
scheduling, capacity controls or autosave lifecycle. Local queue acceptance is not
remote delivery. Production QUIC/TLS requires the `networking` feature; hub rooms
use development UDP. See [NETPLAY.md](NETPLAY.md).

Large-world replication is the next network milestone beyond this snapshot budget.
Do not truncate authoritative state to pass tests or create a parallel server. A headless pass does not prove native
online UI, internet reliability, Windows runtime or commercial-game capacity.

## Add a recipe and upgrade without engine inspection

Read this page, the game's `AGENTS.md` and `assets/workshop.json`. Add a namespaced
recipe using existing item IDs and an upgrade with its declared cost and tuning.
Use `workshop -- edit CONTENT_JSON ADDITIONS_JSON` for a validated atomic semantic
edit. Run `workshop -- validate`, inspect the rule, then add a public-command test for
production, blockage and save continuation. The native recipe/upgrade selectors are
derived from the sorted definition index, so no engine or presentation edits are
required for another valid definition. Use the focused game check above, then the
normal native package/installer gates for a release.

Fresh-agent trials record all source/doc reads, tool calls, failures and verification
time. Numeric CLI usage is model-token evidence when available; missing telemetry is
unavailable, not zero. Scripted edit tests do not prove fresh-model completion.

## Persistent single-player acceptance and bounded playtesting

The native goal is to deliver three wire. Acquire ore/coal, smelt plates, choose
fast tools or wire tools, draw wire, then deliver it. Storage competes with queued
work; freeing capacity or cancelling pending work preserves every paid input.
D delivers one selected item (Down or mouse selects it). F/F11 toggles display
mode in place; Escape opens the menu; focus loss suppresses queued input through
the shared shell. F5/F9 continue through the same engine snapshot authority.

`tools/playtest.py` runs isolated headless seeded processes over this exact game.
See [PLAYTESTING.md](PLAYTESTING.md) for budgets, accounting, trace replay and the
deliberately broken fixture. Rendering, physical input and audible output remain
separate claims. Resource transaction `preview` validates through the identical
validator without cloning inventory; committing always revalidates with `transact`.
