# Netplay: online play for games that own their simulation

`vesper3d::viewer::netplay` is a server and a client for any game whose rules are Rust (`custom-sim`). You
supply the rules and the layouts of what crosses the wire; the kit supplies everything else. It was extracted from
Spooky Kart, whose own copy of this code was about 2,000 lines, none of it about karts.

## What the kit does

**Server** (`NetServer<G, T>`): a lobby (players connect, choose, ready up), a countdown, a match, results, and
back to the lobby, forever. Sessions, join key, version check, rate limiting and timeouts come from the engine's
`SessionRegistry`. It buffers each player's inputs and applies one per tick (a lost input is skipped after a short
wait; a missing one repeats the last). Snapshots go out every `SNAPSHOT_EVERY` ticks with the events that client has
not acknowledged. A player who leaves or times out is handed to the game's own AI (`NetGame::release`). Every
finished match appends one JSON line to `matches.jsonl`: the game's own report plus per-player round-trip time,
bytes, repeated or skipped inputs, and the server's tick times.

**Client** (`NetClient<G, T>`): connects with a retrying Hello; keeps lobby choices in sync until the server's lobby
agrees (a one-shot message is lost under packet loss); sends each tick's input bundled with the last three, so a
lost datagram costs nothing; acknowledges the newest server tick; delivers each event once; measures round-trip
time; times out a dead server. What to draw and how to predict is your `ClientView`.

Works on any `DatagramTransport`: raw UDP, the engine's QUIC/TLS (`net::server_transport`,
`net::client_transport`, see `docs/HOSTING.md`), or the in-memory `net::loopback::LoopNet` used in tests.

## What you write

1. `impl NetGame for MyGame` (`src/viewer/netplay/toy.rs` is a complete, tiny example):
   - `Input`, `Snapshot`, `Event` types and their layouts, written with `net::codec::{Writer, Reader}`. Reads
     must validate (use `f32_within`, check counts); a datagram that does not parse is dropped and counted.
   - `start(seed, seats, participants)`: build the match; return which participant each seat drives. Fill the
     participants nobody drives with the game's own AI. `step(match, inputs)` gets `Some(input)` for human-driven
     participants and `None` for the rest. `snapshot`, `is_over`, `report`, `release`.
   - `fingerprint()`: change it whenever rules, numbers or maps change; mismatched peers are refused.
2. `impl ClientView<MyGame>`: keep a replica of the world. `on_snapshot` receives the server's state and the inputs
   the server has not applied yet: reset your predicted entity to the snapshot and replay those inputs on top.
   `on_input` predicts one tick immediately. `frame` interpolates everyone else and eases corrections.

Then a server is `NetServer::<MyGame, _>::new(server_transport(profile, addr)?, ServerConfig { .. })?.run_realtime(..)`
and a client loop calls `client.poll(now)`, `client.tick(input)` once per fixed tick while `state() == Playing`, and
`client.frame(now, dt)` per rendered frame.

## Guarantees you can rely on (each has a test in `tests/netplay.rs`)

- Lobby choices survive 30% packet loss. Events arrive exactly once even with loss and jitter.
- Ten percent loss leaves under a tenth of ticks running on a repeated input (redundant bundles).
- A wrong join key, a different game version, a ninth player and another address using a player's token are refused;
  garbage datagrams are counted, never fatal (the decoders are fuzz-tested).
- A player who leaves or goes silent mid-match is handed to the game's AI and the match finishes.
- Late joiners wait out the running match and are welcome afterwards.

## Limits and what is not covered

- A snapshot must fit `MAX_DATAGRAM` (1,200 bytes). The kit sheds old events first; a snapshot that is too big on
  its own is not sent (the server prints a warning). Use a compact layout, or split the world.
- One snapshot format for everyone: no per-client interest management yet.
- Prediction is your game's. Spooky Kart predicts its own kart only, so bumps show as small corrections that grow
  with the number of players (see `docs/perf`).
- Lockstep and rollback are not provided; this is server-authoritative with client prediction.

## Testing your game on it

Put a server and several clients on one `LoopNet` and step them together, one `net.advance()` per tick, with the
game's bots driving the clients (`tests/netplay.rs`, and Spooky Kart's `tests/net.rs`). A two-minute match runs in
about a second, deterministically, with whatever delay and loss you choose. Then run the real thing once over
loopback UDP (Spooky Kart's `tests/udp.rs`).
