# ADR 0037: One hub, one name, every BlueEngine online game

Status: Accepted

## Context

Four games use the netplay kit (Deadfall, Spooky Kart, Prop Hunt, Slapstick) and each had copied the same ~65-line server
executable: `--listen --transport --join-key --auto-start --report-dir --seed` plus a participants flag. Their default ports
collide (Spooky Kart 4100, Prop Hunt 4101, Slapstick 4102, Deadfall 4100-4107), so no two could run on one box without
hand-editing flags. Deadfall alone had grown the rest of what a public server needs: a hub that lists and creates rooms
(`games/deadfall/src/hub.rs`, protocol `DFHB`), one server process per room, abuse limits, a non-blocking client, the std-only
Play Online rules, and systemd, router-mapping and DNS units. Almost all of that is not about Deadfall.

Kevin asked (2026-10-02) whether the one DuckDNS name could serve every online game. It can: the name only points at his home, and
one hub on one port can tell games apart. Rooms stay one process each because match settings are process-global in the games
(Deadfall's `sim::set_settings`) and cannot reach `NetGame::start`.

Constraint: **Deadfall Windows clients are already shipped** and speak `DFHB` v1 to `deadfall-kevin.duckdns.org:4100`. They compare
the hub's `build` with their raw `netgame::fingerprint()`, send `Create{bots, kills, name}` (the shipped dialog sends only the name)
and join `hub_host:room.port` over raw UDP. A reply in any other magic or version is dropped silently and they show "Can't reach the
Deadfall servers". The new hub must keep answering them.

## Decision

The std-only, window-less, always-compiled `netplay::cli` and `netplay::hub` modules and the `be2-hub` binary. Games are told apart by
a game id; everything game-specific comes from the game's own server program.

* **`netplay::cli::serve::<G>(&ServeSpec)`** is the one server `main`: the shared flags, the participants flag (clamped, or fixed
  for Deadfall's 12), `--set ID=VALUE` and one `--flag` per setting, `--status-lines`, `--exit-on-stdin-eof`, `--info`, `--help`,
  clean stop through `viewer::shutdown` (ADR 0030). `NetGame` gains two **optional defaulted** items, `settings()` and
  `configure()`, so existing implementations compile unchanged. `NetServer::run_realtime_with` takes a once-a-second hook;
  `run_realtime` is unchanged.
* **Machine formats, one module, both ends tested.** `--info` prints `key=value` lines: `game`, `fingerprint` (the raw
  `NetGame::fingerprint()`), `build`, `max_seats`, `tick_hz`, and `setting=<id>:<name>:<flag>:<kind>:<min>:<max>:<default>`
  (kind `bool|int|choice`, ids 1..=255 unique). `--status-lines` prints
  `STATUS game=<name> players=<n> max=<participants> stage=lobby|match|results build=<hex8>` once a second.
  **Build id** = the `Hello` fingerprint (`fingerprint ^ fnv(NAME)`) `^ (wire::PROTOCOL * 0x9E3779B1)`: a change to the game, its
  name or the netplay envelope changes it, so an engine bump produces "update the game" instead of silence. Hubs report it;
  `hub::local_build::<G>()` is what a client compares.
* **Settings are typed.** A room's settings are `(id, u32)` pairs validated against the schema the server printed; the hub builds
  argv itself (`--set id=value`) and no player-typed string reaches a command line (the room name is only a label). The registry
  may default them (`public_set`, `user_set`) and limit which a player may choose (`client_settings`).
* **Protocol `BEHB` v1** (UDP, little endian, one datagram each way; layout in `hub/wire.rs`): `List{game, skip}`,
  `Create{game, name, settings<=8, cookie}`, `Ping{game}`, `Cookie`; replies `Rooms`, `Created`, `Cookie`, `Error`, `Pong`, each
  stamped with the game's build. Same discipline as `DFHB`: requests padded to a per-kind minimum (200/96/32/32), replies capped
  (1000/128/32/32), silence for anything malformed. Game ids are `[a-z0-9-]{1,24}`; an unknown game gets a one-line error no
  bigger than the request. **Create needs a cookie**: a keyed hash of the source IP and a 60 s window, handed out by a `Cookie`
  request, so a forged source address cannot create rooms in somebody else's name (the reply goes to the forged address). The
  client does the extra round trip itself and refreshes an expired cookie once.
* **Registry** (a hub config file; format in `hub/registry.rs`): `[hub]` (port, pool, report dir, legacy switch, caps, rates) and
  `[game ID]` (server path, Public room on/off and name, `public_set`, `user_set`, `client_settings`, `max_rooms`, `transport`,
  `auto_start`). **The game id to server path mapping exists only in that file**; the wire never names a path. A game whose
  server is missing or whose `--info` fails is skipped with a log line and the others run. `--info` results are cached by
  (path, mtime, size).
* **Rooms** (`hub/rooms.rs`): one process per room on one shared port pool, handed out **round-robin** (the most recently freed port
  is the last reused, so a stale client lands on nothing rather than another game's room; a port something else holds is skipped);
  empty rooms close after 120 s and **rooms nobody ever joined after 45 s**; each game's Public room restarts on a fresh port if it
  dies; the hub keeps every child's stdin open and the servers exit when it closes, so even a SIGKILLed hub leaves no orphans.
* **Reload.** `be2-hub reload GAME` (a `BECT` control datagram, honoured only from loopback; not SIGHUP, which `viewer::shutdown`
  already uses to stop) re-reads that game's registry entry and its server's `--info` and **retires** the game's rooms: unlisted
  at once, closed as soon as empty (or after 30 min), so a match in progress is never ended by an update, while the game's Public
  room starts again from the new build immediately. Other games are untouched. A server binary replaced on disk is noticed by
  (mtime, size) every 10 s (it must be unchanged across two checks, so a half-copied file is never run) and handled the same
  way; a broken new build changes nothing.
* **Limits.** The Deadfall token buckets (per source burst 10 / 2 per s, create burst 3 / 1 per 30 s, global 300 / 150 per s, 4096
  tracked sources) plus per-game user-room caps, a per-creator-IP live-room cap (default 2), a global process cap, and a rate
  that is configurable for tests. A create over the cookie-less legacy protocol also draws on one global bucket (burst 3, 1 per
  20 s), because its source may be forged.
* **Legacy policy.** Datagrams are demultiplexed on their first four bytes. `DFHB` v1 is answered by an adapter that maps it to the
  game id `deadfall`: `List` returns Deadfall's rooms in the old layout with `build` = the **raw** fingerprint from the registry's
  `--info`, `Create{bots, kills}` maps to the schema's settings named `bots` and `kills` (ignored if absent), `Ping`/`Pong`
  unchanged. `legacy = refuse` answers a well-formed v1 reply whose build is deliberately wrong (fingerprint xor 1), so old clients
  show their update message: the way to retire the protocol later. Byte compatibility is proved by tests that encode requests and
  decode replies with an independent hand-written copy of the shipped client's codec, and by the same exchange over a real socket
  against the `be2-hub` binary.
* **Client half** (`hub::client`): the non-blocking `HubClient`, the std-only `Online` state machine and rules (room order,
  status text, retry for "match in progress", scroll window, honest errors from `ConnectFailure`), `local_build`, and
  `default_hub()` over `devkit::ServerChoice` (command line, `server.txt` beside the executable, last used, built-in
  `blue-engine.duckdns.org:4100`, defined once in `hub::DEFAULT_HUB`). Drawing stays in each game.
* **Deploy** (`deploy/hub/`): a user service, a portmap unit whose port list is `be2-hub ports` (hub port, then the pool, from the same
  config), the DuckDNS units (multi-name), an `update.sh` that builds each registered game's server from its own source root and runs
  `be2-hub reload GAME` instead of restarting everything, a registry example and a README with the exact human steps, including
  the switch from the Deadfall-only hub. `deadfall-kevin.duckdns.org` stays alive for shipped Deadfall clients.

Not adopted, deliberately:
* **Feta, BlueDM and Riftwake** have their own netcode and are out of scope (they cannot be supervised through `--info` and
  `--status-lines`).
* **QUIC rooms and join keys through the hub.** Hub rooms are raw UDP (the clients' "Development" transport) for simplicity; the
  registry's `transport` key is passed to the server and a game that needs more must also change its client. A known tradeoff.
* **A relay or NAT traversal.** The hub only lists and starts rooms; clients still connect to `hub_host:room_port`, so the box needs
  its ports open (the portmap unit does that over UPnP).
* **Mid-match join.** A join during a match is refused by the server (`MatchInProgress`); the client retries, as before.
* **One process for many rooms.** Settings are process-global in the games and one stuck room must not take the others down.
* **A weight or priority between games.** A shared pool and per-game caps are enough at this size.

## Consequences

* A new netplay game gets a hostable server in about ten lines (`be2-toy-server` is the template) and appears on the hub by adding
  four lines to `hub.conf` and running `be2-hub reload`.
* The shared code is exercised by `cargo test` without a network: `hub::*` unit tests inject time and fake processes; `tests/hub.rs`
  runs real `be2-toy-server` processes and the real `be2-hub` binary over loopback (ports 43000-44999, checked free first).
* Restarting the hub still ends every room of every game; updating one game does not (reload).
* A shared pool has a floor on cost: each running room is a process (about 5 MB idle), and each pool port is a UPnP mapping the router
  must renew; the default pool is 16 ports.
* Legacy `DFHB` creates are weaker than `BEHB` ones (no cookie); the global legacy bucket limits the harm and `legacy = refuse` removes it.
* `MIG-0037-GAME-SERVER-CLI` (optional adoption) tells a game upgrade that a hand-written server main can become `cli::serve`: a
  game whose `src/bin/*.rs` still instantiates `NetServer::<` is the signature. There is **no `MIG-0037-HUB-CLIENT`**: a hand-rolled
  hub client or Play Online screen has no reliable signature to grep for, and a low-confidence entry would weaken a registry whose
  value is being reliable hints (the same reasoning as ADR 0034); a game that wants the shared client reads NETPLAY.md.
