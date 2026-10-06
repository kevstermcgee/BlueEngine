# ADR 0043: Stop mutation coordination when scope filtering is unreliable

Status: stopped; retain the independently useful regression tests.

The requested diagnostic must mutate only the intended modules and changed lines,
run focused suites, retain failed baselines, and leave shipping verification intact.
A thin coordinator around pinned cargo-mutants 27.1.0 was prototyped with Cargo-owned
freshness, source identity from task_inputs, one mutation worker/two compiler jobs,
explicit baseline/failure/skipped/unverified states, and runtime path-alias coverage.
No engine ownership boundary or gameplay capability changed.

It found a packet-reader coverage gap and a successful test killed that mutation.
Simulation/snapshot samples exposed additional gaps. Existing capacity and replication
tests should be included rather than duplicated; one interpolation test and one legacy
snapshot replay/resave test were added and pass normally.

However, the tool produced 21 mutations after selecting 12. A direct `--re '^$'`
reproduction returned ten field-removal mutations instead of zero. The coordinator
therefore could not certify scoped mutation coverage reliably. Under the user's
explicit stop rule, discard the coordinator and non-blocking PR job, preserve their
review patch and measured failures, and keep the three useful Rust regression tests.
Existing engine gates and public APIs remain intact.

Reconsider only after a pinned upstream version passes the zero-match filter and
changed-line/path-alias checks, then repeat focused module acceptance including
relevant consumer suites. Do not substitute a broad run or silently drop mutations.
See docs/perf/MUTATION_TESTING_REPORT.md for exact commands and results.
