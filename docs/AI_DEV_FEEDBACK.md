# Developing Spooky Kart on BlueEngine: feedback from an AI agent

Written 2026-09-30 after taking a game from "here is the idea" to "it is published": Spooky Kart, an
8-player Halloween kart racer with a dedicated server, bots, a controller-ready client, telemetry and a Windows
release. It complements `~/Downloads/BlueEngine-experience.md` (the Feta retrospective). Evidence is cited so
each point can be checked; the last section says what was done about it.

## In numbers

- About 6,800 lines in the finished project (`~/SpookyKart`). Roughly 1,500 are the game itself (rules, karts,
  track, bots). **About 2,900 are networking**: the codec, server, client, transport, bot clients and their tests.
  Most of that is not about karts at all.
- The engine's own `DedicatedServer` could not be used (it is built around walking characters), so a custom-sim
  multiplayer game starts from the transport and session pieces and writes everything above them.
- Wall-clock cost, roughly in order: netcode (by far the most), the window and models, physics and balance,
  three silent rendering traps, tooling papercuts, deployment.

## What worked very well (keep it)

1. **`custom-sim` scaffold.** A pure `Sim` library plus a thin window, determinism tests, save states, identity,
   icon and a ship gate on day one. The rule "the window never decides gameplay" made every later step testable
   without a display, including the whole netcode.
2. **`devkit`.** `Simulation`, `StateHasher`, `assert_deterministic`, `snapshot::assert_resumes_as_promised`,
   `Lifecycle` with `--capture`, `--script`, `--seed`. Capturing real frames and *looking at them* was the only way
   to see a 3D game from a headless box, and it worked.
3. **`kit`.** `Template`, `Batch`, `Fx`, `hud`, synthesised sounds: enough to build eight distinct karts,
   scenery and a full HUD from primitives with no assets.
4. **`DatagramTransport` moves raw bytes**, and `SessionRegistry`, `HandshakeLimiter` and the QUIC `SecureSocket`
   plug straight in. Production QUIC/TLS worked the first time once the certificate was right.
5. **`be2.py context`** returned a bounded, relevant packet instead of making me read the repository.
6. **The perf and check workflow** (profiles, `perf.py`, tiered check) turned a 17-minute gate into 2.5.

## Friction, ranked by what it cost

1. **No netcode for custom simulations (largest cost).** Lobby, sessions, input buffering, snapshots, event
   delivery, prediction, reconciliation, interpolation, bot takeover, per-race telemetry: all written from
   scratch (`src/server.rs`, `client.rs`, `wire.rs`). It is also where the bugs were: a one-shot "Ready" message
   stalled the lobby under packet loss; prediction ignores bumps so corrections scale with player count.
2. **No binary codec.** The engine speaks JSON, which does not fit 8 karts in one datagram. I wrote a
   bounds-checked reader/writer and a fuzz test. Every networked game will need the same.
3. **Silent rendering traps, each about a debugging round:**
   - A `Template` over 9,000 vertices is skipped by `Batch::add` with no message. The entire world vanished.
   - `Template::quad` needs counter-clockwise corners seen from the front; the wrong order is culled with no hint.
4. **Finding APIs meant reading engine source.** `Lifecycle::feed`, `ScriptFrame::held` versus `starts`,
   `Template::cone` argument order, the list of sound presets, `GamepadFrame` buttons, `MenuStep` all needed a
   `grep` or a file read. `context` names files but not signatures. A one-page cheat sheet of the game-facing
   API would have saved dozens of tool calls.
5. **Seeing the game on a headless machine.** No `xvfb` was installed, nothing documented how to capture, and
   software rendering ran at about 16 frames per second (a full race took 7.5 minutes to capture).
6. **Small papercuts:**
   - `scripts/blue` calls `python`; this Debian box only has `python3`.
   - `scripts/check.py` demands `BE2_TOOLS` with no attempt to find a built `be2-tools`.
   - The scaffold's first `cargo` command must not use `--locked` (documented only in a code comment).
   - `be2-tools new-game NAME DIR ENGINE TEMPLATE` argument order is easy to get wrong.
7. **Things I had to build that other games will want:** a closed track/spline with arc-length, nearest-point and
   lateral offset (`src/track.rs`); an in-memory network with delay, jitter and loss for tests
   (`transport::LoopNet`); a path-following bot.
8. **`GameShell::playing()` is false without a captured mouse.** Spooky Kart gated its race input on it and shipped with
   every key dead in the race while the menus worked; scripted and autopilot runs bypass the gate, so nothing in
   the tests or captures could show it, and it was found by playing. See item 8 of "What was done".
9. **Session and player caps are hard-coded** (`DedicatedServer` allows 8 sessions at `server.rs:230`).

## What I would tell the next agent

- Write the rules as a pure library first, add a scripted-input test for every rule, and only then draw anything.
- Let bots drive real clients in tests and in load tests: it needs no extra code and finds real bugs.
- Log a structured line per match from day one; balance and network quality become measurements.
- Check `docs/perf` before guessing about speed, and record what you change.

## What was done about it

Decisions are in `docs/adr/0022-custom-sim-multiplayer-kit.md`. Mapping the friction list above:

1. **Netcode for custom simulations:** `viewer::netplay` (server, client, `NetGame`, `ClientView`, telemetry), tested
   on a toy game and by Spooky Kart itself, which now runs on it with about 1,900 lines of its own network code deleted.
2. **Binary codec:** `net::codec::{Writer, Reader}`, fuzz-tested.
3. **Silent rendering traps:** oversize templates are split, `Template::quad_facing`, a loud panic instead of index wrap.
4. **Finding APIs:** `docs/CUSTOM_SIM_CHEATSHEET.md`, feature-index entries so `be2.py context` finds netplay, paths and
   the traps, and a pointer in `AGENTS.md`.
5. **Seeing the game headless:** `tools/xcapture.py` and `docs/HEADLESS_CAPTURE.md`.
6. **Papercuts:** `scripts/blue` uses `python3` or `python`; `scripts/check.py` finds `be2-tools` itself; the first-build
   rule (`cargo build`, not `--locked`) is in the cheat sheet.
7. **Things every game rebuilds:** `devkit::path::ClosedPath`, `net::loopback::LoopNet`, `net::any` transports.
8. **The `playing()` trap:** its documentation now says it needs a captured mouse, `GameShell::accepting_input()` is the
   right gate for a game without mouse look, and the cheat sheet warns that scripted runs cannot catch it.
9. **Hard-coded caps:** the kit's server takes its seat limit from the game (`NetGame::MAX_SEATS`); the older
   `DedicatedServer` cap of 8 is unchanged (it serves the stock client).

Still open: predicting collisions on the client (a game concern, noted in the ADR), per-client interest management,
snapshot splitting, and a generic vehicle model.
