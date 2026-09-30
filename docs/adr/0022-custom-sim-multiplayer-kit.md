# ADR 0022: A multiplayer kit for custom simulations, and the fixes Spooky Kart called for

Status: Accepted

## Context

Spooky Kart, an eight-player kart racer, was the first game built end to end on the `custom-sim` road with online
play. The engine's `DedicatedServer` and packet formats assume walking characters, so the game had to write its
own server and client from the transport up. Measured afterwards: about 2,900 of its 6,800 lines were networking
(codec, server, client, transport, bot clients, tests), and nearly all of that had nothing to do with karts. The
build also cost three silent rendering failures (a world of 50,000 vertices vanished without a message; ground
wound the wrong way was culled; wrong `python` name) and many reads of engine source to find signatures. Details
and evidence: `docs/AI_DEV_FEEDBACK.md`.

## Decision

1. **`viewer::netplay`** is a generic lobby-match-results server and a client for any game that implements
   `NetGame` (its rules as a match, layouts written with `net::codec`, seats to participants) and `ClientView` (its
   replica, prediction and interpolation). Sessions use the engine's `SessionRegistry`; transports are any
   `DatagramTransport`. The server is authoritative; inputs travel in redundant bundles; lobby choices are re-sent
   until the server agrees; events are delivered once by acknowledgement; a departed player is handed to the game's
   AI; each finished match appends one JSON line to `matches.jsonl`. `netplay::toy` is a complete example and
   `tests/netplay.rs` proves the guarantees on an in-memory network with loss, jitter and delay. Spooky Kart was
   ported onto it (about 1,900 lines deleted) and its own network tests, over the in-memory network and real UDP,
   still pass.
2. **Shared pieces extracted from the game:** `net::codec` (bounds-checked `Writer`/`Reader`), `net::loopback`
   (`LoopNet`: delay, jitter and loss for raw datagrams), `net::any` (`AnyTransport`, `server_transport`,
   `client_transport` for UDP or QUIC/TLS), `devkit::path` (`ClosedPath` and the yaw helpers).
3. **Rendering traps closed:** an oversize `Template` is split (`Template::split`, used by `Batch::add` and
   `to_meshes`) instead of being dropped; `Template::quad_facing` winds a quad to face its normal; a `Template` past
   65,535 vertices panics instead of wrapping its indices.
4. **Developer papercuts:** the generated `scripts/blue` uses `python3` or `python`; the generated `scripts/check.py`
   finds a built `be2-tools` in the engine checkout it depends on; `tools/xcapture.py` runs a game on a virtual
   display and prints its screenshots; `docs/CUSTOM_SIM_CHEATSHEET.md` puts the game-facing API on one page;
   `docs/NETPLAY.md` and `docs/HEADLESS_CAPTURE.md` explain the model and the recipe; the feature index has entries
   for netplay, closed paths and the rendering traps so `be2.py context` finds them.

## Consequences

- A new online custom-sim game writes its rules, three layouts and a client view, not a server. The kit's
  guarantees are tested once in the engine.
- Public APIs are only added; existing games and the engine's own server are untouched. The one behavioural change
  is deliberate: an oversize template now renders where it used to disappear.
- Prediction stays the game's job. The kit passes the unapplied inputs to `ClientView::on_snapshot` but predicts
  nothing itself, so a game that ignores collisions when predicting (as Spooky Kart does) sees corrections grow with
  player count.

## Not done, on purpose

- No per-client interest management, no lockstep or rollback, no automatic snapshot splitting: a snapshot must fit
  one 1,200-byte datagram.
- No generic vehicle model. The kart physics is entangled with the game's perks; `devkit::path` is what other racing
  or patrol games can reuse. Extract a vehicle module when a second game needs one.
- Spooky Kart's client does not yet predict kart-to-kart bumps.
