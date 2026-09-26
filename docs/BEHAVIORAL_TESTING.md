# Behavioral testing without a custom harness

The native `sim` command drives the real public input boundary at deterministic 60 Hz and emits
structured assertion outcomes. Scenario paths are portable: `game_path` is resolved
relative to the scenario file, not the current shell directory.

```sh
cargo run --locked --no-default-features --bin be2-tools -- \
  sim assets/games/three-switches/scenario.json trace.json
cargo run --locked --no-default-features --bin be2-tools -- replay-trace trace.json
```

A scenario declares players, timestamped held movement/look values and press-edge
`jump`/`interact` commands. Assertions can inspect a player position, a named counter,
completion, and a target's independent eligibility and visibility state. A failed
assertion produces JSON with `ok: false`, returns a nonzero exit status, and still
runs the remaining ticks so the report retains useful evidence.

Use `sim` for external-project acceptance tests before writing Rust helpers. Use the
public `scenario::evaluate_scenario` API when a Rust test needs the same structured
report in-process. Replay traces detect divergence in deterministic engine state; they
do not replace assertions about intended game behavior.
