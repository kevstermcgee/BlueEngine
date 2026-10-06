# Mutation diagnostic experiment — stopped

Baseline engine: `1a1bbd5` (importer plus the stopped hot-reload report).
Executable: cargo-mutants 27.1.0, official release archive SHA-256
`dfe6dc37d0342c891d2829b5a695aa57c2d0edecef7e7d0399a30cc6e206411e`.
The [structured measurements](mutation-testing/measurements.json) retain exact command
arguments, selected/actual mutations, baseline command output summaries and input identity.
The [prototype patch](mutation-testing/stopped-coordinator.patch.gz) decompresses to
SHA-256 `d6e9dd27b9afc698e6eed0fed5e3476ade42d3487ef93e27dc9d1c95efbda600`.
It is review evidence, not shipped tooling.

| Module / sample | Caught | Missed | Timeout | Unviable | Wall seconds |
|---|---:|---:|---:|---:|---:|
| Codec before, 8 | 2 | 1 | 0 | 5 | 78.075 |
| Codec after packet test, same 8 | 3 | 0 | 0 | 5 | 101.804 |
| Simulation initial sample, 12 | 4 | 4 | 0 | 4 | 156.418 |
| Snapshot/determinism/save initial sample, 12 | 8 | 2 | 0 | 2 | 198.611 |
| Rules interrupted, 18 of unexpected 21 | 9 | 7 | 0 | 2 | 217.115 |

These are small deterministic samples, not complete module baselines. Unviable
mutations do not count as caught defects. The rule result is incomplete and cannot
be compared to a 12-mutation cohort. Later simulation/snapshot mutation reruns were
cancelled under the stop rule, so no claim is made that their new tests killed those
mutants. Their normal behavioral checks pass.

The codec baseline executed 7 unit tests and all 18 netplay cases, filtering out
472 unrelated library cases. The survivor changed `Reader::remaining` subtraction
to addition. The added test parses a length-prefixed payload, checks remaining
bytes after reads and refuses truncation/overflow without consuming the payload.
The paired mutant was then caught. This is a reliability improvement; the experiment
does not show a build-time or whole-task time reduction (the after run was slower).

Simulation misses included APIs already covered by capacity/replication suites that
were absent from the initial selection. The stopped prototype's revised selection
included those suites, rather than duplicating their tests. A genuine fractional-pose
gap led to a quarter/half/three-quarter interpolation test retaining current look.
The snapshot default-metadata misses led to a test restoring an independently encoded
v1 envelope, comparing resumed and uninterrupted play, and resaving/reloading it.
All framing/serialization uses the engine's codecs.

Blocking reproduction (no compilation):

```sh
/mnt/blueengine-usb/developer-tools/cargo-mutants-27.1.0/cargo-mutants mutants \
  --no-config --no-default-features --file src/viewer/game.rs --re '^$' --list --json
```

Expected zero; actual ten field-removal candidates. Selecting 12 rules produced
21, including unselected fields. No coordinator or PR job is shipped.

Additional failed attempts were retained: a build used test-only target flags and
was interrupted when unrelated targets started compiling; incorrect Cargo/libtest
argument forwarding failed the unmutated baseline; a unit-filter guard correctly
reported zero cases but needed to allow rules' real integration-only coverage.
The final prototype had 12 Python safety tests passing, including omitted files,
skipped empty diffs, failed baselines, Cargo forwarding and changed-input invalidation.
Those prototype tests/tooling are preserved in the patch rather than added as gates
for an unreleased feature.

Build-free plan packets measured 0.094–0.166 seconds and 601–846 stdout bytes across
three calls per scope; zero builds, validated by a no-subprocess test. These are
prototype startup measurements, not a delivered speed claim. Mutation runs used
warm dependencies on this Linux host with USB-backed artifacts and fresh copied
source. No empty-registry/cold-machine benchmark was performed. The interrupted
rule run measured 256.681 child CPU seconds and a 1,202,851,840-byte child high-water
RSS; this is not simultaneous machine memory. Earlier cohorts did not collect CPU/RSS.
No cache hit-rate, token, fresh-agent completion or Windows mutation claim is made.
Fresh-agent mutation trials were not run because acceptance stopped at tool reliability.

Normal focused verification:

```sh
CARGO_TARGET_DIR=/home/kevin/BlueEngine/target TMPDIR=/mnt/blueengine-usb/be2-task-tmp \
  cargo test --locked --profile itest --no-default-features --lib -- \
  --test-threads=2 fractional_display_pose legacy_saves_without_optional length_prefixed_payload
```

Result: 3 passed, 0 failed, 479 filtered; 17.25 seconds compilation, 0.00 seconds
reported test time. `cargo fmt --check` passed after formatting. Full final engine
and exact-revision platform gates still apply to the complete tooling pass.

Next: revisit only after the upstream filtering defect is fixed and reproduced as
zero candidates, then verify changed-line alias coverage and relevant consumer suites.
