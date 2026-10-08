# Focused reliability milestone for scheduled maintenance

Status: **queued for tonight's AI update; do not implement during feedback intake.**

Source: Kevin's requested reliability review, received 2026-10-07. The review
baseline is `4d8572acea96e81eeac56570d602c9cb28f2cbef`. Remote `main` was also
at that revision when this feedback was recorded. This document preserves the
requested work; its findings have **not** been reproduced or certified by this
feedback-only change.

## Starting instructions and scope

Work on <https://github.com/kevstermcgee/BlueEngine> from current `main`, record
the actual starting revision, read `AGENTS.md`, and use the repository's
task-start and scoped context tools. Revalidate every finding against current
code before changing it. If a finding is already fixed, verify its regression
coverage and move on.

Complete a focused reliability milestone covering game creation, project
checks, native shipping, deployment rollback, and multiplayer event recovery.
The goal is a dependable starter → test → package workflow and reliable
recovery through the boundary conditions below. Complete implementation and
meaningful verification during the scheduled maintenance work.

Leave navigation, rendering expansion, and other new capabilities for a later
milestone. Preserve the native Windows delivery policy and Linux
development/server support.

## 1. Preserve deployment artifact identity through failed activation and rollback

Inspect `tools/hub_deploy.py` and `tools/test_hub_deploy.py`.

Reproduce this sequence using the existing isolated deployment fixture:

1. Deploy A successfully.
2. Install B, but fail its readiness check.
3. Deploy C successfully.
4. Roll back C.
5. Restore B's source and run the ordinary updater.

At the reviewed revision, the retained previous executable is A, but the new
receipt's previous metadata describes B. Rollback installs A while assigning
B's source identity. Requesting B afterward reports “up to date” and performs
no build.

Keep metadata consistent with the executable actually retained. Validate the
artifact/hash relationship before assigning a restored source identity.
Preserve interrupted-rollback recovery and the existing behavior that prevents
an ordinary update from immediately reinstalling a revision intentionally
rolled back.

Extend the existing A → failed B → C regression through rollback and the
subsequent request for B. Verify both executable content and receipt identity.

## 2. Complete the canonical starter → check → shipping path

Inspect `tools/be2.py`, `tools/author.py`, `tools/workflow.py`,
`templates/game_check.py`, `templates/game_ship.py`, and generated-tooling
update mechanisms.

The reviewed code has inconsistent executable discovery:

* `be2.py map` builds `target/itest/be2-tools`.
* `be2.py build tools` builds `target/be2-tools/release/be2-tools`.
* The generated game checker searches neither location.
* Shipping invokes the checker without supplying a matching executable.

Use a consistent selection and freshness policy across discovery, authoring,
generated project checks, and recorded evidence. Explicit tool overrides should
have clear behavior, and alternate binaries must not silently produce evidence
for the wrong engine.

Propagate the selected absolute Cargo target directory through project commands
that launch Cargo internally. Preserve intentional feature/profile isolation
and Cargo's normal fingerprinting. Reports must describe the environment
actually used.

Regression acceptance must cover:

* First use with only the canonical authoring output available.
* Discovery after `be2.py build tools`.
* Conflicting alternate output.
* Explicit and relative target-directory overrides.
* The default workflow when `CARGO_TARGET_DIR` is initially unset.
* A newly generated representative game progressing through its declared
  checks and shipping workflow.

Update generated copies or migration support through the repository's existing
mechanism.

## 3. Repair event recovery across eviction and oversized retained entries

Inspect `src/viewer/netplay/event_channel.rs` and
`tests/netplay_event_delivery.rs`.

Construct a real Rust regression with:

* Client acknowledgement at zero.
* Sequence 1 already evicted.
* Sequences 2–5 retained but oversized for the active transport.
* Sequence 6 valid and small.
* No requirement for additional emitted events to make recovery possible.

The reviewed sender repeatedly spends its four-packet budget on gaps 2–5.
Those frames do not communicate the retention base, and the receiver ignores
them while waiting for sequence 1.

Ensure the later valid event becomes deliverable, unavailable events are
counted accurately, and state snapshots remain independent. Preserve ordering,
session/peer authentication, match-epoch handling, resource bounds, and
malformed-frame validation.

Do not fix this by allowing any later gap to skip arbitrary earlier deliverable
events. Cover packet reordering and transport-specific payload limits in the
regression.

## 4. Establish sustained delivery evidence

Add a deterministic sustained-combat scenario representing at least 60–120
seconds of simulated play, with several clients, realistic delay, jitter,
loss, and the actual transport payload budget.

Record event delivery age, state-update age, retained-event counts, and gaps.
Report useful percentiles and maxima. Use a short results phase so the test
establishes timely delivery during play.

Include a targeted rematch case with deliberately delayed packets from the
preceding match, including client inputs. Repair any additional lifecycle defect
only after reproducing it.

Set and explain appropriate bounds for the representative workload. Keep claims
limited to the tested scenario.

## 5. Close the Windows packaged-game evidence gap

Inspect current engine and BlueEngineGames native/release workflows first.

At the reviewed revisions, Windows packaging uses `--no-launch --no-smoke`,
while graphical smoke runs on Linux. Installer lifecycle tests use a stand-in
executable.

Add or connect an appropriate Windows check that launches a real clean
packaged game, confirms startup and a usable screen, exercises representative
input, and exits cleanly. Retain logs and relevant runtime evidence tied to the
tested artifact.

If the available runner cannot execute this reliably, state the exact
limitation and leave that evidence requirement explicitly outstanding. Keep
packaging and runtime results separately identifiable. Linux graphical smoke
and stand-in installer lifecycle checks do not establish real Windows
packaged-game runtime behavior.

## Working method and required completion report

Use scoped tests during iteration, then run the required integration/full
checks on the final code. Keep changes focused on these failures and their
verification. Record useful lessons with retrieval keywords and verify that
normal task queries surface them. Update documentation wherever behavior or
evidence requirements change.

Finish the implementation with:

* The starting and final revisions.
* Which findings reproduced and which were already fixed.
* What changed and the resulting behavior.
* Regression results for each corrected failure.
* Sustained-network measurements and their limits.
* Native package and Windows runtime evidence actually obtained.
* Required checks that remain unverified, with concrete reasons.
* Any compatibility or generated-project migration implications.

Make the implementation and results concrete and reviewable. Do not mark this
milestone complete merely because this request has been recorded or collected
by the nightly job.
