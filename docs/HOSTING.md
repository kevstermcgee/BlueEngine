# Hosting a dedicated BlueEngine server

The runnable `be2-headless` server provides authoritative 60 Hz simulation over
JSON/UDP, with 20 Hz snapshots, client prediction/reconciliation, spatial interest,
acknowledged deltas and keyframe recovery. The default port is `4000/udp`.

## Start a server and client

Build the rendering-free server and the graphical client:

```bash
cargo build --release --locked --no-default-features --bin be2-headless
cargo build --release --locked --bin be2
```

Run both peers with the same map or game document:

```bash
target/release/be2-headless --server 0.0.0.0:4000 --map assets/maps/starters/house.json
target/release/be2 --connect 127.0.0.1:4000 --map assets/maps/starters/house.json
```

Use `--game FILE` instead of `--map FILE` for a GameDocument. Do not pass both.
Protocol 3 rejects clients whose initial map/game fingerprint differs from the server.

## Require authentication

Pass the same long, random secret to both peers:

```bash
target/release/be2-headless --server 0.0.0.0:4000 --map assets/maps/starters/house.json --auth-key "LONG_RANDOM_SECRET"
target/release/be2 --connect 127.0.0.1:4000 --map assets/maps/starters/house.json --auth-key "LONG_RANDOM_SECRET"
```

This mode uses HMAC-SHA256 challenge-response: the client sends a derived proof, not
the secret itself.
The server issues a session token, authenticates later datagrams and rejects replayed
packets. Authentication is optional; a server started without `--auth-key` accepts the
ordinary unauthenticated protocol.

The executable transport is still raw UDP. Authentication does **not** encrypt game
payloads, hide addresses/traffic patterns, provide accounts, or migrate sessions to a
new address. The library contains a QUIC/TLS datagram implementation, but the current
`be2` and `be2-headless` executables do not select it.

## Network setup

- Allow inbound UDP on the selected port in the host firewall.
- Forward that UDP port to the host when serving through a NAT router, or use a VPN.
- `python tools/blue_portmap.py status --port 4000` inspects the bundled UPnP path;
  `enable` requests a temporary mapping and `remove` deletes only a mapping owned by
  BlueEngine. Router support and topology vary, so confirm reachability externally.
- Do not treat a successful local/LAN connection as proof that the server is reachable
  from the public internet.

## Verification and limits

For an unauthenticated server, the smoke example checks handshake, input/snapshot flow,
delta recovery, multiple clients and clean disconnect:

```bash
cargo run --locked --example network_smoke -- 127.0.0.1:4000
```

The smoke example does not accept an authentication key. Authenticated behavior is
covered by `cargo test --locked --test secure_net`; a manual keyed smoke can use the
graphical client command above.

Packets are capped at 1400 bytes. Oversized snapshots are rejected rather than
fragmented, and the protocol has no compact binary codec or snapshot chunking. Use
`be2-tools net-proxy LISTEN UPSTREAM PRESET` with `bad-wifi`, `mobile-3g`, `satellite`
or `congested-bursty` to exercise a live session under repeatable impairment. Run
`python tools/be2.py check` before publishing a build.
