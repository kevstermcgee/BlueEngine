# ADR 0031: `be2-ctl`, a small manager for servers on one machine

Status: Accepted

## Context

Running a BlueEngine server meant `be2-headless --server ...` in a terminal or a unit file. Nothing could list the servers on a
machine, say how loaded one was, or stop one that would not stop except `ps` and `kill`. The need is practical: shut down a server
that is not shutting down properly, and start one for a test.

## Decision

`be2-ctl` (std only, Unix only, same headless dependency graph) with `list`, `status`, `start`, `stop`, `restart`, `logs`,
`define`, `undefine` and `prune`. A server opts in with `be2-headless --server ... --name NAME`.

* **Registration.** A named server writes `servers/NAME.json` (pid, process start time, arguments, launcher, version, protocol) and
  binds `servers/NAME.sock` in the state directory (`$BLUEENGINE_STATE_DIR`, else `$XDG_STATE_HOME/blueengine`, else
  `~/.local/state/blueengine`). The directory is 0700 and the socket 0600: only the owner may control a server. Both files are
  removed on exit. A live server with the same name is refused; a stale record is replaced.
* **One line of JSON each way** over the socket: `{"cmd":"status"}` and `{"cmd":"shutdown"}`. Requests are bounded (16 KiB), each
  connection has a 2 s I/O timeout and a bad request gets an error reply, so a hostile or broken client cannot wedge the server
  loop's control thread. `shutdown` sets the same stop flag as SIGTERM ([ADR 0030](0030-graceful-shutdown-signals.md)). Status is
  published by the loop every 30 ticks into a mutex-guarded snapshot; the control thread never touches the simulation.
* **Liveness is pid plus process start time** (`/proc/PID/stat`), so a recycled pid is not mistaken for the server that owned it,
  and `be2-ctl` never signals a process it cannot identify. Zombies count as gone.
* **`stop` escalates** because its job includes servers that misbehave: control-socket shutdown, then SIGTERM, each waiting
  `--timeout` (default 15 s). Without `--force` it then stops and says so. With `--force` it sends a second SIGTERM (an immediate
  exit, no save) and finally SIGKILL, which also ends a SIGSTOPped process.
* **`be2-headless` processes that never registered** (started by hand, systemd or Docker without `--name`) are found by scanning
  `/proc` and listed as `pid:N`; `stop pid:N` works by signal.
* **`start` and `restart` only for servers `be2-ctl` owns**: ones it launched (`BE2_LAUNCHER=be2-ctl`, detached into its own process
  group, output in `logs/NAME.log`) or that have a saved definition (`definitions.json`). Restarting something systemd or Docker
  supervises would fight its supervisor, so those get list, status and stop only, and `restart` says how to opt in (`define`).
  `start` waits up to 10 s for the server to register and answer, and shows the server's own log if it exits first.

Not adopted: Windows support (the manager runs on the Linux server machine; `be2-headless --name` reports a clear error elsewhere and
`be2-ctl` exits with a message); a daemon (state is files and sockets, nothing runs between commands); managing several machines;
a network control port.

## Consequences

`tests/server_ctl.rs` drives real servers: lifecycle and final autosave, restart keeps arguments, duplicate names, startup failure,
an unregistered server, a SIGSTOPped server that needs `--force`, stale and unreadable records, and a control socket that stays
private and answers after garbage input.
The fixture creates private, unpredictable state directories directly under `/tmp`, independently of `TMPDIR`,
so deeply nested maintenance directories cannot overflow the Unix socket pathname limit.
