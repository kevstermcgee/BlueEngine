# Task evidence binds every project input

Status: Accepted

At 957272c game input identity reused the browser builder's list, which omitted tests,
examples, benches and root-level target sources. Editing an executed integration test
could leave a prior task pass current. Source freshness must include what verification uses.

Decision: native task_inputs content identity version 2 includes all project inputs,
tracked deletions/renames and scratch-project inputs, excluding conventional generated
output directories only. Before/after identity includes tests; the first-use metadata
exception excludes only Cargo.lock. Legacy input versions cannot retain passing evidence.
Tool, environment/configuration and output identities plus retained logs remain required.
Resume re-evaluates saved routes against the current authoritative starter contract.
Agent notes still cannot certify checks; failed checks outrank incidental skips.

Acceptance: execute a game test outside src, edit it to fail, observe that the old pass
becomes unverified and the same command actually fails. Restoring exact bytes restores
identity; renaming/deleting the test invalidates it. Test edits during checks invalidate
passes even when metadata legitimately registers the root lock. No independent result cache.

Verification: tools.test_springboard through the real canonical runner in isolated fixtures.
Conservative extra invalidation after non-output project edits is intentional; no faster
agent completion, token or runtime performance claim is made.
