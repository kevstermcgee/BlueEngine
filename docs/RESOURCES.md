# Bounded resource transactions for custom rules

Enable `resources` with `default-features = false` for a rendering-free custom
simulation. `vesper3d::runtime::resources::Inventory` owns bounded integer counts.
Recipes, machine state, queues, escrow, timing, upgrades, visibility and player
authorization stay in game-owned Rust. Stock `GameDocument` counters are a
separate path; this library does not add inventory semantics to them.

```rust
use std::collections::BTreeMap;
use vesper3d::runtime::resources::Inventory;
let mut items = Inventory::new(20, BTreeMap::from([
    ("workshop:ore".into(), 20), ("workshop:plate".into(), 10),
])).unwrap();
items.transact(&[], &[("workshop:ore".into(), 4)]).unwrap(); // declared source
items.transact(&[("workshop:ore".into(), 2)], &[("workshop:plate".into(), 1)]).unwrap();
```

`new(max_units, capacities)` starts empty. `from_counts(max_units, capacities,
counts)` validates saved/prepared counts and fills omitted declared counts with
zero. `quantity(id)`, `counts()`, `capacities()`, `max_units()` and `total_units()`
inspect bounded state. `validate()` checks the same invariants as deserialization.

`transact(debits, credits)` validates all gross costs against pre-state, then all
final counts and the total budget, and commits together. Credits cannot pay a cost
in the same transaction. Duplicate entries aggregate; failures preserve every
quantity. A receipt lists touched resource balances in lexicographic ID order.
`TransactionError` exposes `code`, `resource`, `available`, `required`, `capacity`
and an actionable message. Attach the resource's authored definition location in
game diagnostics. A receipt proves a local authority commit, never network delivery.

Resource definition IDs use `namespace:name`, lowercase ASCII letters/digits and
`_`, `-`, `.`; at most 128 bytes. They are persistent IDs, separate from instance
handles and localized presentation labels. One inventory permits 64 resource
definitions and one transaction 128 combined debit/credit entries before
aggregation. Quantities and bounds use checked `u64` arithmetic. Per-resource and
combined capacities are explicit. Zero budgets/capacities are supported.

Put Inventory directly in the game's engine-owned `Snapshot::State`. Its ordered
maps serialize as inventory schema version 1, independently of the game's save
schema/content version. Deserialization refuses unsupported schema versions,
duplicate map keys, unknown definitions, missing counts and exceeded bounds.
Game Snapshot restore must also validate machine escrow and recipe references.
Use exact content fingerprints and explicit copy migrations; see [SAVE_STATE.md](SAVE_STATE.md).

Production composition is game-owned: debit inputs once into a machine's escrow;
advance its logical ticks; credit outputs before clearing the completed job.
If output is full, keep the job and escrow and report `RESOURCE-CAPACITY` or
`RESOURCE-BUDGET`. Cancellation refunds escrow first; a refused refund retains
the job unchanged for recovery. Iterate machines in a stable declared order.
Conservation depends on explicit game recipes/sources/sinks and their behavioral
tests; Inventory cannot decide which conversion is legitimate.

The library has no presentation, transport, randomness or clock dependency.
Custom online games call the same authority rules through `viewer::netplay` and
choose bounded wire state. Inventory does not provide replication/fragmentation,
interest management or capacity claims; consult [NETPLAY.md](NETPLAY.md).

External contract and a second headless ability-cost/refund consumer:

```sh
cargo test --locked --profile itest --no-default-features --features resources --test resources
```

These tests exercise coherent conversion, full/blocked outputs, refunds,
cooldowns, deterministic ordering, invalid serialized state, arithmetic bounds
and an exhaustive small transaction domain. They are focused evidence; engine
integration and native packaging remain separate required gates.
