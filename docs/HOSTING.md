# Hosting a dedicated BlueEngine server

The runnable `be2-headless` server provides authoritative 60 Hz simulation with
20 Hz snapshots, client prediction/reconciliation, spatial interest, acknowledged
deltas and keyframe recovery. The same server path accepts two explicit profiles:

- `development`: raw, unencrypted UDP for local work and network impairment tests.
- `production`: QUIC datagrams over TLS 1.3 with a pinned server certificate.

Both use UDP at the network layer; the default port is `4000/udp`. There is no silent
fallback from production to development.

## Which runtime gets what

This guide is about the **stock server**, `be2-headless --server`. A game that owns its simulation and uses
[`viewer::netplay`](NETPLAY.md) runs its own server loop and gets less. `be2-tools describe` prints the same facts with the
numbers read from the code (`runtime_support`).

| | Stock server (`be2-headless --server`) | Custom simulation on `netplay` |
|---|---|---|
| Players | 8 by default, up to 1024 with `--max-players` | `NetGame::MAX_SEATS`, each game's own |
| Datagram | 1100 bytes | 1200 bytes |
| World updates | compact binary, acknowledged partial updates, nearest records first | one snapshot format for everyone, must fit one datagram |
| Interest management | room-graph relevance per player | none |
| Prediction | stock client | your `ClientView` |
| Graceful shutdown | SIGINT/SIGTERM/SIGHUP: finish the tick, final save, exit 0 | not provided by the kit |
| Saving | `--autosave`, `--load`; saves hold up to 1024 players, so autosave works at any `--max-players` | `devkit::Snapshot` (same file format) |

`be2-headless` refuses to start when `--max-players` plus the world's objects that can become networked props exceeds replication's
1024 records per peer (the error says what to lower), because such a server would stop with a replication error once full. The built-in test lab
has a handful of objects, so its largest server is `--max-players 1021` or so, not 1024.

A `GameDocument` has its own small limits (file size, counters, flags; `be2-tools game-describe`), separate from all of the above.

## Stopping a server

`be2-headless --server` stops cleanly on **SIGINT, SIGTERM and SIGHUP** (Ctrl-C, Ctrl-Break and console close on Windows): it
finishes the current tick, writes its final autosave when `--autosave` is on, and exits with status 0. That is what
`kill`, `systemctl stop` and `docker stop` send, so a routine stop no longer loses the world since the last autosave.

```bash
kill -TERM "$(pidof be2-headless)"      # graceful: finishes the tick, saves, exits 0
kill -TERM "$PID"; kill -INT "$PID"     # a second request exits at once (status 130), without the save
```

A **second** shutdown request forces an immediate exit, so a server that will not stop (a stuck save, a wedged tick) can be
ended by signalling twice instead of `kill -9`. On Windows, closing the console or logging off gives the process only a few
seconds; the handler waits up to 4.5 s for the final save. That Windows path is compiled by CI but no test delivers a real console event to a
running server, so treat it as implemented, not verified; the Unix signals are covered by `tests/graceful_shutdown.rs`. Before this change these signals killed the process at once
(status 143) and no final save was written. The custom-simulation `netplay` kit's own server loop is unchanged.

## Managing servers with `be2-ctl`

`be2-ctl` lists, inspects, starts, stops and restarts servers on one Linux machine. Give a server a name and it registers
itself (a record and a private control socket under `~/.local/state/blueengine`, or `$BLUEENGINE_STATE_DIR`):

```bash
be2-ctl start arena --save -- --server 0.0.0.0:4000 --game games/arena.json --autosave 60
be2-ctl list
be2-ctl status arena
be2-ctl restart arena
be2-ctl stop arena
```

`stop` asks the server to shut down (final save included), then sends SIGTERM. A server that is wedged stays up until you add
`--force`, which sends a second SIGTERM and finally SIGKILL: `be2-ctl stop arena --timeout 5 --force`. Servers started without
`--name` (by hand, systemd, Docker) are listed as `pid:N` and can be stopped, but `start` and `restart` only manage servers
`be2-ctl` launched or has a saved definition for (`be2-ctl define NAME -- ARGS`), so it never fights a supervisor. `be2-ctl logs NAME`
shows a launched server's log and `be2-ctl prune` removes records of servers that are gone. `--json` on `list` and `status` is for scripts.
See ADR 0031. `be2-ctl` and `--name` are Unix only.

## Serving more than eight players

The server admits 8 players by default. `--max-players N` (up to 1024) raises every limit that would refuse the
ninth: the world's join cap, the session registry and the handshake rate. Each peer's world update is prepared from
the shared world and that peer's own state, so `--network-threads N` (`0` = one per core) prepares them on several
threads; the bytes every peer receives and the send order are identical at any thread count, and servers with fewer
than 32 peers stay on one thread because splitting would not pay.

```bash
target/release/be2-headless --server 0.0.0.0:4000 --max-players 128 --network-threads 0 --map assets/maps/starters/house.json
```

Measured on 4 cores, a snapshot broadcast to 128 players took 29.8 ms in the original code (more than one 16.7 ms
tick), 12.8 ms after the replication fix, and 4.2 ms with 4 threads (`docs/perf/README.md`). Custom code builds the
same with `DedicatedServer::with_max_players(n).with_network_threads(t)`. Add `--profile` to see where each status
window's time went (per-peer preparation, receiving, the world step; biggest first), and see `docs/perf/README.md` for
how to read it. The QUIC/TLS transport still handles all
connections on one async thread, which is not yet measured at this scale.

## Start a server and client

Build the rendering-free server and the graphical client:

```bash
cargo build --release --locked --no-default-features --bin be2-headless
cargo build --release --locked --bin be2
```

Run both peers with the same map or game document:

```bash
target/release/be2-headless --server 0.0.0.0:4000 --transport development --map assets/maps/starters/house.json
target/release/be2 --connect 127.0.0.1:4000 --transport development --map assets/maps/starters/house.json
```

Use `--game FILE` instead of `--map FILE` for a GameDocument. Do not pass both.
Protocol 8 rejects clients whose initial map/game fingerprint differs from the server.

These development commands preserve compatibility and need no certificate. Do not
expose this profile as a production Internet service.

## Production QUIC/TLS

Provision a certificate for `feta.local` plus its PKCS#8 DER private key outside the
repository. Distribute only the DER certificate to clients, then point both peers at
that public certificate:

```bash
export BLUE_TLS_KEY_FILE=/etc/blueengine/server-key.der
export BLUE_TLS_CERT_FILE=/etc/blueengine/server-cert.der
target/release/be2-headless --server 0.0.0.0:4000 --transport production --map assets/maps/starters/house.json
target/release/be2 --connect server.example:4000 --transport production --map assets/maps/starters/house.json
```

Keep the private key readable only by the server account. `BLUE_TLS_CERT_FILE` is
optional only when using the bundled certificate and a matching separately provisioned
key. Certificate rotation requires distributing the new public DER file to clients.
The certificate identity must be `feta.local`; the connection address can still be an
IP address or DNS name because the client supplies that pinned identity explicitly.

## Require client authentication

Pass the same long, random secret to both peers:

```bash
target/release/be2-headless --server 0.0.0.0:4000 --transport production --map assets/maps/starters/house.json --auth-key "LONG_RANDOM_SECRET"
target/release/be2 --connect server.example:4000 --transport production --map assets/maps/starters/house.json --auth-key "LONG_RANDOM_SECRET"
```

This mode uses HMAC-SHA256 challenge-response: the client sends a derived proof, not
the secret itself.
The server issues a session token, authenticates later datagrams and rejects replayed
packets. A keyed disconnect must include that exact token; missing or incorrect
credentials are rejected. If the operating system cannot provide secure randomness,
the server rejects the handshake instead of issuing predictable credentials. TLS
authenticates the server; `--auth-key` adds application-level client authentication.
QUIC encrypts payloads but does not hide addresses, packet sizes or
traffic timing, provide accounts, or migrate BlueEngine sessions to a new address.

## Network setup

- Allow inbound UDP on the selected port in the host firewall.
- Forward that UDP port to the host when serving through a NAT router, or use a VPN.
- `python tools/blue_portmap.py status --port 4000` inspects the bundled UPnP path;
  `enable` requests a temporary mapping and `remove` deletes only a mapping owned by
  BlueEngine. Router support and topology vary, so confirm reachability externally.
- Do not treat a successful local/LAN connection as proof that the server is reachable
  from the public internet.

## Verification and limits

For an unauthenticated development server, the smoke example checks handshake, input/snapshot flow,
delta recovery, multiple clients and clean disconnect:

```bash
cargo run --locked --example network_smoke -- 127.0.0.1:4000
```

The smoke example does not accept an authentication key. Authenticated behavior is
covered by `cargo test --locked --test secure_net`; the production transport handshake
is covered by `cargo test --locked --test transport_profiles`.

Protocol 8 sends world updates (full snapshots and deltas) in a compact binary form (`net::worldwire`: 19 to 28
bytes per walking player where JSON took about 172, so about 45 to 50 players fit one packet instead of 6); every other packet is
still JSON, and the 1100-byte ceiling stays. `DatagramTransport::payload_limit` may lower that budget per peer; QUIC
reports its negotiated datagram limit. Larger worlds arrive as partial snapshots/deltas across multiple updates, nearest
records first (ADR 0028). Rebuild both peers: protocol 7 (JSON world updates), 6 (acknowledgements) and older are
incompatible and are refused at the handshake.

The supported bound is 1,024 relevant players/props combined per peer (eight
players per server unless `--max-players` raises it), with prop IDs at most 128 UTF-8 bytes. Every individual state
record, removal, and full GameState record must fit the active budget with its
protocol envelope. Non-finite transforms, duplicate IDs, excessive counts, and
unsupported records fail with an actionable error. Checked server runners return
that error; custom loops use `try_broadcast_snapshots` or inspect the latched
`replication_error` after the compatibility `step`/`broadcast_snapshots` methods.

Per peer, the sender retains one acknowledged world and one immutable pending
packet. Only an exact acknowledgement with the current session token commits that
packet's represented state. Local acceptance does not mean delivery. Loss or queue
saturation retries the pending packet; there is no growing backlog of snapshots.
`receive_update` ignores older/duplicate updates and reports missing baselines.
Stock clients filter by session, request `Resynchronize { session, after_tick }`,
and keep acknowledging their latest applied update, including while paused.
Reconnect after disconnect/timeout creates a fresh session; a repeated Hello on a
live session is an idempotent welcome retry. A client that loses its local mirror
must request resync with its latest applied tick even if it retains its session.
If it has lost that sequence/session metadata too, reconnect after disconnect or
timeout. A delayed resync older than an already acknowledged update is ignored.

A rotating dirty record gets reserved progress. Remaining space prioritizes the
owner, removals and held props; when only one record fits, owner/fair priority
alternates. Peer order and world/GameState lane order rotate under shared queue
pressure. GameState is independently repeated in full and accepted monotonically;
world acknowledgements do **not** confirm game-state delivery. Its existing bounded
schema fits 1100 bytes, but a smaller transport can reject that individual record.

The schedule allows at most one world packet plus one GameState packet per peer
per 20 Hz opportunity: at most 44,000 application bytes/s per peer, or 22,000 without
a game record (excluding inputs, handshakes and transport overhead). Stop-and-wait
makes throughput depend on RTT and loss. It does not promise an atomic whole-world
view or simultaneous state for every entity: each record advances authoritatively
as budget allows. Eventual convergence requires changes to settle and repeated
successful data **and acknowledgement** delivery. Disconnected peers time out;
permanent congestion/loss cannot converge.

`session.replication.counters` exposes accepted packets/bytes, backpressure,
retries, acknowledgements, ignored acks and resyncs. `game_replication` counts the
independent lane. QUIC counters distinguish facade queue acceptance/fullness,
worker submission/drop and incoming drop. Its facade remains bounded at 128
outgoing and 256 incoming datagrams, with Quinn's 64-payload send/receive buffers
per connection. The worker avoids Quinn's old-datagram eviction on a full send
buffer. Five-second server summaries avoid per-packet logging.

Run deterministic correctness and actual socket liveness separately:

```bash
cargo test --locked --no-default-features --test replication_budget
cargo test --locked --no-default-features --test replication_sockets
```

The deterministic fixture synchronizes 1,024 entities and replaces all prop IDs
in 525 packets / 501,614 JSON bytes. Retained serialized world data after ack is
194,559 bytes versus 11,670,300 bytes for 60 full worlds (about 60x less). The impaired
96-prop / 700-byte fixture uses 128,499 accepted bytes over 400 opportunities,
including 97 retries and 57 queue rejections. These are synthetic payload/state
measurements with worst-length session tokens, not allocator/RSS or UDP/TLS
overhead measurements. Receiver interpolation/history adds its own
bounded presentation storage. The implementation validates all relevant records
and constructs a bounded candidate set per opportunity; it is not a constant-time
scheduler. Use
`be2-tools net-proxy LISTEN UPSTREAM PRESET` with `bad-wifi`, `mobile-3g`, `satellite`
or `congested-bursty` to exercise a development UDP session under repeatable impairment.
`be2-tools bench` returns nonzero if the authoritative simulation, snapshot, delta or
spatial-query regression budget is exceeded. Run `python tools/be2.py check` before
publishing a build.
