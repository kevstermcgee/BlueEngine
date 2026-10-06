# Generated authoring contracts: bounded foundation milestone

Starting revision: `cb5c8f1f8de653bc6bb18ed0966d12796cc91d7b` on
`tooling/ai-developer-tools`. Original main `ca2799fa7c4b34d52ec5580c9a3935c00d306865`
and its unrelated working changes remain untouched. This completes one maintenance
improvement; it introduces no gameplay features or scripting language.

## Problem and selection

The existing GameDocument and Edit Serde types, their hand-maintained JSON
schemas, and thirteen handwritten MCP input shapes duplicated authoring
contracts. Updating a GameDocument shape required reconciling the Rust type and
a JSON definition. Bounds also duplicated existing game limits. The baseline
four maintained game fixtures and two asset packs validated; no broken baseline
game is inferred from this duplication.

Choose generated contracts because this removes ongoing shape maintenance and
adds automatic drift detection while preserving runtime validators. The already
accepted CC0 importer addresses primitive-only art authoring; it is preserved.
Hot reload and mutation coordination remain stopped for the measured/reliability
reasons in their reports, rather than broadening this milestone.

## Acceptance and production path

Acceptance criteria are recorded in `docs/SCHEMAS.md`. Game authors keep using
the normal starter, documented JSON, native validators, authoritative scenarios
and stock client. Engine maintainers edit Rust types/constraint hooks, then:

```sh
python3 tools/be2.py schemas --write --plan
python3 tools/be2.py schemas --write
python3 tools/be2.py schemas --check
python3 tools/be2.py check
```

The stock switch objective and Observatory's timer/shutter/calibration objective
exercise the GameDocument contract through the existing GameRuntime and real
scenario runner. The CC0 furnished scene exercises the asset-pack contract with
native/browser rendering and headless collider metadata; earlier importer
evidence is in `MODEL_IMPORT_REPORT.md`. Schema validity stays distinct from
physical reachability, gameplay success, visuals, networking and packaging.

Focused evidence before final verification:

- `schemas --check`: ten Rust tests passed. Five maintained GameDocuments,
  two maintained packs and the existing four patch operations validate. Invalid
  bounds, identities, conditions, actions, provenance, optional enums and
  transaction sizes fail. Nullable presentation/rule fields remain valid.
- An undeclared counter passes structural validation but fails the production
  `GameDocument::validate`; schema validity cannot replace behavioral checks.
- `python3 -m unittest tools.test_workflow tools.test_schemas -q`: 50 tests,
  49 passed and one pre-existing Windows-only test skipped on Linux.
- Feature-specific strict Clippy passed (21.73 seconds after the ICU lock update).
- `python3 docs/perf/schema-generation/audit_constraints.py --before cb5c8f1`:
  319 game, 60 patch and 59 pack constraints compared, no differences. This
  structural audit is not a formal proof of logical equivalence; fixtures and
  runtime validation remain separate.

Generation parity initially failed after a Rust constraint correction while the
committed schema still contained the earlier output: eight behavioral tests
passed, parity failed. Regeneration repaired it. This is the intended drift
failure, not an ignored error. Failure messages now identify the file and exact
regeneration command without dumping entire schema objects.

## Measurements and limitations

Hardware: Intel N97, four logical CPUs, 16,158,048 kB reported RAM, Linux.
Rust 1.98.1, `itest`, bounded Cargo jobs two. Shared target and temporary files
are on the ext4 USB drive. These timings do not represent Windows or empty
dependency caches.

Observed front-door runs use the exact commands in the JSON receipts. Initial
batch, intermediate warm runs and final-source runs are separate records
(`measurements.json`, `final-measurements.json`, `final-source-measurements.json`):

| Operation | Initial run in the measured batch | Three later warm runs |
|---|---:|---:|
| `python3 tools/be2.py schemas --write` | 6.004 s (two rebuilt artifacts) | median 0.474 s |
| `python3 tools/be2.py schemas --check` | 33.630 s (new compatible ICU dependencies) | median 0.511 s |

All warm writes reported four unchanged outputs and zero rebuilt artifacts:
130/130 Cargo artifact reports were fresh. Tests still ran on every check.
Write packets were 1,094–1,095 bytes; final check packets were 888 bytes.
The final-source priming write rebuilt two engine artifacts and took 24.313 s;
the priming check took 0.706 s. These are not empty-cache measurements.
Plans ran zero commands/builds. Execution invokes one Cargo command plus four
generator commands for write, or one Cargo command for check; both also make
two Git inventory calls for existing stateless input identity.

Reproduce priming plus three warm front-door runs with:

```sh
CARGO_TARGET_DIR=/home/kevin/BlueEngine/target TMPDIR=/mnt/blueengine-usb/be2-task-tmp \
  python3 docs/perf/schema-generation/measure.py > .be2-work/schema-measure-new.json
```

For the fixture probe, copy `schema-generation/probe-Cargo.toml` to a scratch
directory as `Cargo.toml` alongside `fixture_probe.rs`, build with
`cargo build --profile itest --manifest-path SCRATCH/Cargo.toml -j 2`, and pass
the schema and fixture paths to the reported `be2-schema-baseline` executable.
The receipt records the exact original commands; the archived source reproduces
the same validator operation. Timing excludes schema parsing but includes
validator construction, fixture I/O/parsing and validation.

The first feature-generator build took 45.61 s with the existing engine cache;
the first parity-test feature build took 81 s. An accidental invocation without
the shared target took 56.050 s; it succeeded using an unshared target,
not mixed into warm measurements. Empty-cache build costs were not measured.

The same external pinned `jsonschema 0.34.0` probe validated the same fixtures
before/after generation in three alternating warm runs each: four game fixtures
median 6.229/6.957 ms and two pack fixtures 4.089/4.082 ms. These small differences
are not evidence of a validation speed improvement. Exact arguments, input hashes
and outputs are in `schema-generation/fixture-measurements-final.json`. The initial
probe build took 58.29 s. Latest validator 0.58.6 failed to compile with pinned
serde_json 1.0.140; the compatible validator was selected without changing engine
pins. Optional ICU dependencies are locked to 2.2 instead of introducing their
2.3 Rust-1.88 requirement; the engine MSRV itself was not retested here.

Generated outputs are larger: game 27,938→36,811 bytes; patch 1,939→5,944;
asset pack 6,687→8,010. The MCP bundle is 5,323 bytes. Typed documentation,
defaults and formats explain the increase. Reduced output size, faster validation
and faster compiler execution are not claimed. Shape-only GameDocument changes
reduce hand-maintained shape files from two to one plus generated output;
thirteen MCP shapes are derived rather than hand-maintained JSON. Constraints
still require deliberate Rust hooks and tests.

One fresh-context agent completed an independent authoring exercise from clean
checkout `d14c79dc5b5315589b9fd1aebc2d3d7488e10b8a`. It followed the normal
entry/context workflow, authored a 600-tick start/finish challenge with existing
stock primitives, repaired an undeclared-counter error, and proved success,
exact expiry, loss/restart/success and deterministic replay. It changed no engine
source and inspected world/loss/reset/win/menu captures. Elapsed time from
`be2 start` was 325.07 s. The report records inspected files, commands, failed
attempts and limits in `schema-generation/fresh-authoring-report.json`. This is
one post-change trial, not a before/after agent speed or token comparison.

The authored variant is now a public, reproducible fixture at
`assets/games/timed-relay`: use its README commands for validation and the three
scenarios. The normal `game_scenario` suite runs those scenarios, compares the
stock frame adapter checksum to headless authority, verifies complete round
state reset and checks that expiry/completion clears the active timer. No new
gameplay code or language was introduced. Its known-invalid counter fixture
retains the production diagnostic. Public context lookup now ranks this timed
GameDocument request first after the initial trial needed one narrowing command.
Two inspected rendered frames and command results are included alongside the report.

A concurrent report edit during a measurement caused generation to reject changed
inputs and preserve outputs; the failed receipt remains in ignored working
evidence. The final measurements were rerun on stable inputs.

The first full run caught an introduced discovery-output regression:
`capabilities::discovery_and_parser_share_command_signatures` failed because
adding the generator command made compact `describe` 6,017 bytes, exceeding its
existing strict 6,000-byte budget. The existing shutdown description was shortened
without removing its signal, autosave, exit or platform-test information. The
budget assertion and command inventory remain intact; the response is now
5,982 bytes. The failed full report is retained in
`schema-generation/engine-check-first-failure.json`. The subsequent run passed
that gate but found the low-level `schema-generate` command undocumented. Its
backend signature and supported kinds were added to `docs/SCHEMAS.md`; the
unchanged documentation-drift test passed. That failed report is retained in
`schema-generation/engine-check-doc-failure.json`.

Final local verification used:

```sh
CARGO_TARGET_DIR=/home/kevin/BlueEngine/target TMPDIR=/mnt/blueengine-usb/be2-task-tmp \
  CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 python3 tools/perf.py record --suite check \
  --note "Generated authoring schemas preserve v1 contracts and verified timed-relay stock fixture"
```

All 36 gates passed, with 3,008 test executions, in 1,058.383 s. The full report is
`schema-generation/engine-check.json`; per-stage rows are in `metrics.jsonl`.
It recorded 1,394.999 child CPU seconds, 3,187,417,088-byte maximum child RSS
(an OS child high-water mark, not simultaneous machine memory), 4,118 fresh and
449 rebuilt Cargo artifacts. It includes default/headless/2D/model/schema tests,
strict Clippy and rustdoc, five maintained games, Leo's three modes, production
authoring and Python tools. No check was removed or weakened.

The importer-only revision's earlier full run used 34 gates/2,977 executions and
took 719.623 s. Different source, suite, cache and test concurrency conditions
prevent attributing that wall-time difference to this milestone; no overall
build/test speed improvement is claimed. The measured inner-loop benefit is
zero rebuilt artifacts on warm writes and explicit focused contract verification.
Linux/Windows release, browser and rendered-game CI remain the final target gates;
inspect the Engine checks run on the pushed commit rather than treating the local
headless or schema result as cross-platform evidence.
No token telemetry, smaller-model advantage, large-world scaling, player-count
capacity or persistent-multiplayer production readiness is claimed. Runtime
simulation, physics, transport, saves and presentation bodies are unchanged.

## Next bounded milestone

Measure one schema/validator drift repair by independent agents before/after,
using the real typed maintenance path and comparable caches. This would quantify
the authoring-effort benefit beyond the demonstrated elimination of manual shape
copies. Existing feature/context lookup and the canonical checks are the entry.
