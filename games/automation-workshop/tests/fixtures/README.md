# Genuine version-one save

`v1.be2save` was produced by the actual version-one Workshop authority at commit
`d7e4188898e184747ac3456c897cd3dfd98c2a8c`, before the schema/content changed to two:

```text
cargo run --locked --profile itest --manifest-path games/automation-workshop/Cargo.toml --no-default-features --bin workshop -- save-v1 games/automation-workshop/tests/fixtures/v1.be2save
```

The fixture has logical tick 3, two active machines and a second queued job on
machine A. Engine `snapshot::save` wrote its frame, checksum, original content
fingerprint and state hash; it is not a rewritten v2 save. The v1 field `shipped`
becomes `delivered` in state schema two. Frozen content-v1 keeps the original
4/6-tick rules; new v2 work uses 6/8 ticks. Existing escrow and remaining progress
survive an explicit migration of a copy. The generator command was removed from
the v2 executable so it cannot mislabel new saves as v1.
