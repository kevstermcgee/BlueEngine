# Mutation diagnostic status: stopped

The cargo-mutants 27.1.0 trial found useful coverage gaps, but failed its filtering
contract. No mutation coordinator, CI job, default installation or new test cache
is shipped. The existing full engine gates remain required.

Reproduce the blocking defect with a pinned executable:

```sh
cargo-mutants mutants --no-config --no-default-features \
  --file src/viewer/game.rs --re '^$' --list --json
```

Expected: zero mutations. Observed: ten field-removal mutations, despite a regex
that matches nothing. A requested 12-mutation rule sample expanded to 21.
Do not rely on this version for regex-scoped verification or bounded samples.
Changed-line CI acceptance was not completed.

Three regression tests remain in the normal engine suites: packet cursor progress
and refused reads, fractional display interpolation, and loading/replaying/resaving
legacy snapshots that omit optional metadata. No production mechanics changed.

See [measurements and limitations](perf/MUTATION_TESTING_REPORT.md) and
[ADR 0043](adr/0043-scoped-mutation-diagnostics.md). The compressed coordinator
prototype is retained for review; apply it only in an isolated experimental checkout.
Revisit after upstream filtering is fixed and the zero-match reproduction passes.
