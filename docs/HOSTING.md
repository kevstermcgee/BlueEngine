# Hosting a dedicated BlueEngine server

The runnable `be2-headless` server provides authoritative 60 Hz simulation with
20 Hz snapshots, client prediction/reconciliation, spatial interest, acknowledged
deltas and keyframe recovery. The same server path accepts two explicit profiles:

- `development`: raw, unencrypted UDP for local work and network impairment tests.
- `production`: QUIC datagrams over TLS 1.3 with a pinned server certificate.

Both use UDP at the network layer; the default port is `4000/udp`. There is no silent
fallback from production to development.

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
same with `DedicatedServer::with_max_players(n).with_network_threads(t)`. The QUIC/TLS transport still handles all
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
Protocol 7 rejects clients whose initial map/game fingerprint differs from the server.

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

Protocol 7 keeps JSON and the 1100-byte ceiling. `DatagramTransport::payload_limit`
may lower that budget per peer; QUIC reports its negotiated datagram limit. Larger
worlds arrive as partial snapshots/deltas across multiple updates. Rebuild both
peers: protocol 6 acknowledgements and unscoped resync requests are incompatible.

The supported bound is 1,024 relevant players/props combined per peer (still eight
players per server), with prop IDs at most 128 UTF-8 bytes. Every individual state
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
