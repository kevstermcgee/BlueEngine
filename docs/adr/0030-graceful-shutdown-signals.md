# ADR 0030: Graceful shutdown on signals, without a new dependency

Status: Accepted

## Context

`be2-headless` created a stop flag for its server loop and never set it. `kill`, `systemctl stop` and `docker stop` (all
SIGTERM) ended the process mid-tick: exit status 143, no "Final save" line, no files written although `--autosave` was on
(demonstrated on the real binary before the change). The obvious fix, the `ctrlc` crate, is forbidden in the headless
dependency graph (`ARCH-HEADLESS-001`, enforced by `tools/check_headless.py`).

## Decision

`viewer::shutdown` (std only; two OS calls declared directly) makes SIGINT, SIGTERM and SIGHUP on Unix, and Ctrl-C, Ctrl-Break,
console close, logoff and system shutdown on Windows, set the existing stop flag. The loop finishes its tick, writes the final
save and returns, so the process exits with status 0. `be2-headless` installs it before the loop and calls `shutdown::finished()`
after the save.

* **A second request forces an immediate exit (status 130).** A stuck server can be stopped by signalling twice, instead of
  `kill -9`. The decision is one pure function (`note_request`) that is unit tested; the forced exit uses only
  async-signal-safe calls (`write`, `_exit`).
* **Handlers touch only atomics.** A small helper thread carries the request to the `Arc<AtomicBool>` the loop reads; it ends when
  the flag is set.
* **Windows close/shutdown events wait** up to 4.5 s for `finished()`, because the system ends the process when the handler
  returns.
* `unsafe` is confined to this module (`#![allow(unsafe_code)]`, like the existing OS calls in `input.rs`), with a `SAFETY`
  note on each block. The Windows half is compiled (type-checked for `x86_64-pc-windows-gnu`, built by the Windows CI job) but **no test delivers a real console control event, so its runtime behaviour (save, then exit 0; the 4.5 s wait) is unverified**; the behavioural tests
  are Unix-only.

Not adopted: the `ctrlc` crate (forbidden in the headless graph); a notice packet to connected clients on shutdown (they time
out after the client timeout); changing the `netplay` kit's server loop (a separate API with its own runner).

## Consequences

* A routine stop saves the world. `deploy/systemd` gains `TimeoutStopSec=15` and `docker-compose.yml` `stop_grace_period: 15s`.
* The server now exits 0 on SIGTERM (systemd `Restart=on-failure` will not restart a deliberate stop) and 130 when forced.
* This is the foundation for a server management utility: "stop" can ask, then signal, then signal again, and know what each
  outcome means.
