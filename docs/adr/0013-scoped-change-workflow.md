# 0013: Demand-loaded context and conservative validation scopes

Status: Accepted

## Context

Game authors were routed through broad engine documentation and duplicate Cargo
checks. The feature index is curated evidence, not a complete dependency graph;
using its file lists to omit engine regressions would give false confidence.

## Decision

Use the existing index for bounded, build-free context packets. Keep universal task
routing small and load engine invariants only for engine work. Keep full engine
validation as the default and the fallback for every unclassified changed path.
Only explicitly reviewed independent Python entry points have smaller local scopes;
Git selection includes staged, unstaged, untracked, deleted and both rename paths.
CI retains all engine gates on both operating systems. Do not cache test results.

Generated games own a single Python runner for native content checks and their own
Cargo tests. Cargo test includes compilation, so a preceding cargo check adds no
guarantee. An explicit content-only iteration command has a separately labelled
result and never certifies Rust edits. Full project validation and relevant manual
checks remain required on final files. Missing tools and command failures fail closed.

## Consequences

Project work needs less engine context and no engine-wide test reruns for game-only
changes. Full engine edits still pay for every existing gate. Adding a scope requires
dependency review and selection regression tests; editing the selector itself always
falls back to full checks. Logs preserve evidence while keeping console output small.
Existing game layouts need explicit adoption; the runner cannot infer custom content.

Implementation: [workflow](../../tools/workflow.py),
[game runner](../../templates/game_check.py), [usage and limits](../CHANGE_WORKFLOW.md).
