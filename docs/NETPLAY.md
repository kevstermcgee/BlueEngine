# Netplay: online play for games that own their simulation

`vesper3d::viewer::netplay` is a server and a client for any game whose rules are Rust (`custom-sim`). You
supply the rules and the layouts of what crosses the wire; the kit supplies everything else. It was extracted from
Spooky Kart, whose own copy of this code was about 2,000 lines, none of it about karts.

This is not the stock server (`be2-headless`): it has its own server loop, so `--max-players`, partial replication,
interest management and signal handling described in [HOSTING.md](HOSTING.md) do not apply. `be2-tools describe` lists both paths' limits.

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

## Starting point

Use `src/viewer/netplay/toy.rs` (`ToyGame` and `ToyView`) and
`src/bin/be2-toy-server.rs` (`netplay::cli::serve`) as the smallest complete assembly.
`cargo run --no-default-features --bin be2-toy-server -- --info` describes its shared
server configuration; `cargo test --no-default-features --test netplay` validates
lobby, match, loss, reconnect and process behavior. Retired prototype code is in
`docs/archive/multiplayer-game`; its old handshake is not a supported starter.

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
- A wrong join key, a different game version, a ninth player and another address using a player's token are refused,
  and the client says which (`NetClient::failure`; a silent server is `Unreachable`, not a refusal);
  garbage datagrams are counted, never fatal (the decoders are fuzz-tested).
- A player who leaves or goes silent mid-match is handed to the game's AI and the match finishes.
- Late joiners wait out the running match and are welcome afterwards.

## When connecting fails

`ClientState::Rejected(String)` is kept as it always was, but its text is not what to show a player. Ask the client
why: `client.failure()` returns `Option<ConnectFailure>` (set whenever `state()` is `Rejected`):

| `ConnectFailure` | Meaning |
|---|---|
| `Unreachable { addr, waited_secs }` | no datagram came back in 8 s: server offline, wrong address, or the UDP port is not forwarded. Nobody refused; nobody answered. |
| `VersionMismatch` | the server's `fingerprint()` differs: update the game |
| `WrongKey` | the server has a join key and ours did not match |
| `MatchInProgress` | a match is running; the server takes players between rounds, so retry shortly |
| `Full` | every seat is taken |
| `Other(String)` | any other reason the server sent, verbatim |

`failure.hint()` is one short, actionable sentence for each ("No reply from 203.0.113.5:27015 after 8 s: the server
may be offline, the address may be wrong, or its port may not be forwarded."). Show that, not
`format!("The server turned you away: {reason}")`, which reads like a refusal even when nobody answered. The
`Rejected` text for an unreachable server is now "No reply from {addr} after 8 s". The server's reason strings are
constants in `netplay::failure` (`REASON_VERSION`, `REASON_KEY`, `REASON_MATCH`, `full_reason`) shared with the
classifier, so they cannot drift; `ConnectFailure::classify(&str)` also recognises the older wording by keyword.

Trap: `ClientView::prediction()` has a default that reports zeros, so a game that never overrides it shows
`corrections: 0, max_error: 0` and looks perfectly predicted. The default now prints one warning to stderr naming
your view type; override it (with `PredictionStats::default()` if you really predict nothing).

## Typing and pasting a server address

`devkit::TextField` (pure logic; fields `text`, `caret`, `max_len`, `filter`) replaces a hand-rolled `Vec<char>`:
`insert_str` sanitises a paste (control characters removed, `CharFilter::{Any, Address, Name, Digits}` applied,
cut to `max_len`, a multi-line paste keeps its first non-empty line), plus `backspace`, `delete`, `move_left`,
`move_right`, `home`, `end`, `clear`, `caret_byte()` for drawing. With the `client` feature,
`input.feed_text(&mut field)` (focus-gated, native-key-reader aware) or `field.feed_frame()` (macroquad directly)
handles typing, Backspace/Delete/arrows/Home/End with hold-to-repeat, and Ctrl+V / Shift+Insert / Cmd+V from the
clipboard; `game_input::copy_to_clipboard(text)` backs a "copy invite" button. Call it only while the field has focus
and never while another field or the game is reading keys.

`devkit::resolve_ipv4(text, default_port) -> Result<SocketAddr, AddressError>` accepts `host`, `host:port` and
`ip:port` (IPv4 only, because servers bind `0.0.0.0`; an IPv6 literal gets a message saying so). `AddressError`'s
`Display` is player-facing. A name lookup blocks, so resolve when the player presses Connect (or on a thread), not
every frame; `resolve_ipv4_with` takes the lookup as a closure for tests.

The default server: `ServerChoice::first_of(dir, &[ServerSource::CliArg(flag_value(&args, "--connect")),
ServerSource::FileBesideExe("server.txt"), ServerSource::LastUsed(settings.last_server.as_deref()),
ServerSource::Builtin("play.example.com:27015")])` returns the first non-empty one, with its `origin` and, for a
`server.txt` (line 1 address, optional line 2 join key, `#` comments), the `join_key`.
`first_of_beside_exe` reads the file next to the executable. After a successful connect, store what the player typed
in `Settings::last_server` (`#[serde(default)]`: old `settings.json` files still load).

## The server main: `netplay::cli::serve`

Do not write a server `main`: `serve::<MyGame>(&ServeSpec { bin_name, about, default_listen, default_report_dir, join_key_env,
participants: Participants::Flag { flag: "racers", min: 1, max: 8, default: 8 } /* or Participants::Fixed(12) */,
default_auto_start })` is the whole executable (`src/bin/be2-toy-server.rs` is the template). It gives `--listen`,
`--transport development|production`, `--join-key` (or the environment variable you name), `--auto-start`, `--report-dir`,
`--seed`, your participants flag, one `--flag` per setting plus `--set ID=VALUE`, `--status-lines`, `--exit-on-stdin-eof`,
`--info` and `--help`, and stops cleanly on SIGINT/SIGTERM (ADR 0030). Match settings a hub or player may choose are
`NetGame::settings() -> &'static [SettingSpec]` (typed `bool|int|choice` with id, name, flag, min, max, default; ids 1..=255,
never reused) and `NetGame::configure(&[(u8, u32)])`, called once with every setting's value before the socket binds. Both are
optional and default to none.

Native servers retain the most recent 256 match reports in memory. `--history-limit N` changes this;
zero disables in-memory history. Every completed report still appends to `matches.jsonl` and match
numbering keeps increasing. Library users retain the existing unlimited default and may call
`NetServer::set_match_history_limit(Some(N))`; `None` restores unlimited retention.

`NetServer::send_stats()` distinguishes local send attempts, acceptance, backpressure,
errors and oversized messages, plus accepted bytes and snapshots. Counters reset at match start.
Archives include the same counters in an additional top-level `send` object; existing `MatchLog`
readers still parse old and new archives. The existing peer byte/packet counters and
`snapshots_sent` retain their attempted-send meaning. Queue acceptance is never a delivery ack.

`--info` prints `game=`, `fingerprint=` (the raw `NetGame::fingerprint()`), `build=` (`cli::build_id::<G>()`: the `Hello` value
folded with the netplay envelope version), `max_seats=`, `tick_hz=` and one
`setting=<id>:<name>:<flag>:<kind>:<min>:<max>:<default>` per setting, then exits 0. `--status-lines` prints
`STATUS game=<name> players=<n> max=<participants> stage=lobby|match|results build=<hex8>` once a second. `cli::{Info, Status}`
parse both. `NetServer::run_realtime_with(stop, max_ticks, |snapshot| ..)` is the hook behind the status line.

## The hub: one name, one port, every game

`be2-hub` (`netplay::hub`, ADR 0037, `deploy/hub/README.md`) lists and creates rooms for every registered game on one UDP port and
starts one server process per room from a shared port pool. Players click Play Online, see the rooms of *their* game, pick or
make one. The hub's registry (`hub.conf`) maps game ids to server programs and says which settings players may choose; a game
appears by adding a `[game ID]` section and running `be2-hub reload ID`. Wire protocol `BEHB` v1 (`hub/wire.rs`): list, create
with a source-address cookie, ping; old Deadfall `DFHB` v1 clients keep working (`hub/legacy.rs`, `legacy = serve|refuse`).
Rooms close after 120 s empty, or 45 s if nobody ever joined; each game's Public room restarts if it dies; reload retires one
game's rooms without ending matches in progress (at most 30 minutes). `be2-hub verify GAME --server CANDIDATE` checks a new server
build with the registry's own rules before it is installed, and `be2-hub status GAME` reports what the running hub holds (a reload
acknowledgement is not readiness); `deploy/hub/update.sh` uses both. Hub rooms are raw UDP, so `transport = production` is refused.

The hub also closes a room that never prints its first status within 30 seconds, or has no fresh status
for 30 seconds. Previously healthy rooms with stale status disappear from join lists during that grace.
The process reader already expires status after five seconds, so recovery follows that freshness window
plus the grace. Public rooms restart after 2 seconds; repeated exits, hangs or spawn failures double
the delay up to 60 seconds. A minute of continuous healthy Public-room status resets the backoff;
healthy private rooms do not reset it. Retired rooms are never restarted by the watchdog.
Optional `[hub]` keys `room_startup_timeout_ms`, `room_status_timeout_ms`, `public_restart_max_ms`,
and `public_restart_reset_ms` set these deadlines (1..=3600000 milliseconds). Reload starts a new
failure history. These checks use the existing native status channel; no extra listener is opened.

A game's client side is `hub::client` (std only, no window): `HubClient::new(hub_addr, game_id)` for one non-blocking request
at a time (`request_list`, `request_create_with(name, &[(setting_id, value)])`, `poll() -> Option<HubEvent>`), or the whole Play
Online state machine, `Online::new(&hub.address, MyGame::NAME, hub::local_build::<MyGame>(), now)`: call `update(now)` each frame
(it returns `Action::Join { addr, room }` when to connect), draw `view`, `rooms`, `dialog`, and call `join_selected`,
`open_dialog`, `create`, `refresh`. Its rules are public for your own screens: `order_rooms`, `room_status`, `JoinWait`,
`scroll_to`, `hub_error_message`, `connect_failure_message` (from `ConnectFailure`). `hub::default_hub(cli_arg, last_used)` is the
`ServerChoice` chain (command line, `server.txt`, last used, `blue-engine.duckdns.org:4100`). A hub whose build differs from
`local_build` (`Online::mismatch()`) would be refused by its rooms: show `update_message`.

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
