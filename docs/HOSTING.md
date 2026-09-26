# Hosting a dedicated BlueEngine server

The runnable `be2-headless` server provides authoritative 60 Hz simulation with
20 Hz snapshots, client prediction/reconciliation, spatial interest, acknowledged
deltas and keyframe recovery. The same server path accepts two explicit profiles:

- `development`: raw, unencrypted UDP for local work and network impairment tests.
- `production`: QUIC datagrams over TLS 1.3 with a pinned server certificate.

Both use UDP at the network layer; the default port is `4000/udp`. There is no silent
fallback from production to development.

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
Protocol 4 rejects clients whose initial map/game fingerprint differs from the server.

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
packets. TLS authenticates the server; `--auth-key` adds application-level client
authentication. QUIC encrypts payloads but does not hide addresses, packet sizes or
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

Packets are capped at 1100 bytes across both profiles. Oversized snapshots are rejected rather than
fragmented, and the protocol has no compact binary codec or snapshot chunking. Use
`be2-tools net-proxy LISTEN UPSTREAM PRESET` with `bad-wifi`, `mobile-3g`, `satellite`
or `congested-bursty` to exercise a development UDP session under repeatable impairment.
`be2-tools bench` returns nonzero if the authoritative simulation, snapshot, delta or
spatial-query regression budget is exceeded. Run `python tools/be2.py check` before
publishing a build.
