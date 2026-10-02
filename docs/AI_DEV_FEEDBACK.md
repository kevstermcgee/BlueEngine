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
10. **The Linux headless path cannot catch Windows-only input bugs.** On Windows the standalone clients read keys through
    a native reader that replaces macroquad's events, and it polled only a 23-key shortlist: R, digits and most letters were
    silently never pressed in custom-sim games, while every Linux test and capture passed because Linux never uses that
    reader. Nothing failed; the keys were just dead. See item 10 of "What was done".

## What I would tell the next agent

- Write the rules as a pure library first, add a scripted-input test for every rule, and only then draw anything.
- Let bots drive real clients in tests and in load tests: it needs no extra code and finds real bugs.
- Log a structured line per match from day one; balance and network quality become measurements.
- Check `docs/perf` before guessing about speed, and record what you change.
- Two quiet netplay traps: `ClientView::prediction()` defaults to zeros, so a game that never overrides it reports
  "no corrections" forever (the default now warns once on stderr); and `ClientState::Rejected("Could not reach the
  server")` once read like a refusal when nobody had answered. Use `NetClient::failure().hint()` for the player
  (`docs/NETPLAY.md`, "When connecting fails").

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
10. **Windows-only input:** the native reader now covers the full `game_input::KEY_TABLE`, can be driven by a fake key
    source on Linux (`ClientInput::begin_frame_with_key_source`, tests in `game_input.rs`), and a key outside the table
    is announced once on stderr and trips a `debug_assert!` instead of reading as never pressed. `python tools/be2.py
    check --windows` type-checks the `cfg(windows)` code for `x86_64-pc-windows-gnu` (it proves it compiles, not that it
    runs; see `docs/CHANGE_WORKFLOW.md`). Existing Windows releases need a rebuild to get the fix, and a manual key test
    on Windows is still the only proof of real input.

Still open: predicting collisions on the client (a game concern, noted in the ADR), per-client interest management,
snapshot splitting, and a generic vehicle model.

## Geometry lessons (added after Spooky Kart's visuals pass and Deadfall's art)

Art was 67% of Deadfall's ~2.4M-token development, and its weapon-model toolkit existed twice (about 1,500 lines
across two files). Spooky Kart shipped two geometry bugs that no engine test could see. What was done:
`kit::shape` (lofts, sweeps, rounded boxes, capsules, mirroring, smooth normals, `offset_strip`) and `kit::lint`
(a geometry-defect lint with the exact cases below as tests). The lessons:

- **Coplanar quads z-fight.** The kerb stripes sat at y = 0, the same plane as the road quad under them; which of the
  two won each pixel changed with the camera. Never draw two same-facing surfaces in one plane. Lift the upper one by
  more than the depth buffer can resolve at your farthest view (`lint::depth_resolution(near, distance)` is about
  `distance^2 / (near * 2^24)`), or do not draw the lower one underneath. `lint::lint(&template)` reports the pair as
  `CoplanarOverlap`; surfaces from different parts that merely share an edge are fine.
- **Never offset a polyline per segment normal without a mitre and a clamp.** The road border moved every sample
  along its own segment normal; at a tight bend the inner side's points crossed and the strip turned inside out. A corner needs the
  mitre direction (neighbouring normals averaged and stretched), a miter limit for sharp corners, and on the inside of a bend an
  offset kept short of the local centre of curvature. `Template::offset_strip` does this; `lint::strip_folds` checks any strip.
  The clamp is local: a strip that is wider than the gap between two far-apart parts of its own path can still overlap itself,
  and lint finds that as a coplanar overlap.
- **Depth ratio.** `far / near` of 7000 (0.1 / 700) leaves about 6 mm of depth resolution at 100 m and 3 cm at 230 m
  with a 24-bit buffer, so distant coplanar-ish details fight no matter how carefully you lifted them. Keep the ratio under
  about 3000 (raise `near`; most first-person games can live with 0.3 or more). The engine's own custom-sim template uses
  0.05 / 400 (ratio 8000) and is affected. `View::camera_checked(near, far)` warns once on stderr; `view::depth_ratio_warning` is the pure
  check for a test.
- **`fx_alpha` and `fx_add` are never depth-tested.** Their pipelines request `depth_test: LessOrEqual` with
  `depth_write: false`, but miniquad 0.4.8 enables `GL_DEPTH_TEST` only when `depth_write` is true
  (`src/graphics/gl.rs`, `apply_pipeline`, line ~1303 calls `glDisable(GL_DEPTH_TEST)` otherwise). Translucent surfaces and glows
  therefore show through walls. The doc comments now say so. `fx_alpha`/`fx_add` stay that way (changing them would change every
  game's look); the depth-tested path is `Materials::decal` (it writes depth and carries its own clip-space bias, because polygon
  offset is dead in miniquad 0.4.8 too) and `Batch::blob` / `kit::Shadows` (ADR 0036). A flat translucent mark on the ground that
  shows through a wall or over the car standing on it is this trap.
- **Shadows: render targets and sampling.** `render_target_ex` is always RGBA8 colour with an unreadable depth attachment; the
  default `sample_count: 1` makes miniquad blit the whole target after every draw call (use 0); targets default to `Linear`
  filtering, which corrupts a depth packed into bytes (use `Nearest`); a `sampler2D` is `lowp` unless declared `highp`; an unset
  sampler binds a white texture, so a "no map yet" state must decode as "no shadow"; GLSL 100 has no `dFdx`, so acne is fixed
  with normal-offset bias, not slope-scaled bias. `kit::shadow` does all of this; copy it rather than re-deriving it.
- **Kit primitive winding.** `Template::ball` and `Template::ring` / `soft_ring` were wound against their own normals
  (found by the lint); the kit draws without back-face culling, so it never showed, but it is fixed.

## Hosting lessons (the shared hub, ADR 0037)

- **`viewer::shutdown::install` makes SIGHUP a stop signal**, so "reload the config on SIGHUP" would end the hub and every room. The hub's
  reload is a loopback-only control datagram (`be2-hub reload GAME`).
- **A repeated nonce is a retry, not a new request.** The hub answers a second Create with the same (source, nonce) with the room the first made,
  so a test that reuses one nonce for different creates sees the first room come back instead of a refusal or a new room.
- **Killing `sh -c "..."` leaves its child holding the pipe.** A test script that stands in for a hung server must `exec` the long-running
  command, or the kill does not end it and reading its output blocks.
- **A game's `NetGame::NAME` is part of the `Hello` check and of the hub's game id rules** (1-24 of `a-z 0-9 -`): `cli::serve` refuses a name
  that is not, because the hub could not carry that game.
- **Rooms never outlive the hub only because the server watches its stdin.** A server `main` that skips `cli::serve` and ignores
  `--exit-on-stdin-eof` leaves an orphan process holding its port after a hub crash.
