# Maintaining these machine utilities

Read README.md. Keep orchestration in Python's standard library and expensive
operations in native Cargo, Git, systemd, and Debian utilities. Preserve private
reports and avoid recording environment values, arbitrary arguments, or tokens.

Inventory never authorizes deletion. Applying cache cleanup requires review of
the exact target/profile. No engine check substitutions, automatic worktree
removal, server restarts, cloud authorization, or broad package removals.

Test behavioral safety changes with `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest -v`.
Tests must use scratch directories; never clean real caches as a test.
Update VERSION and README when behavior or the public command interface changes.
