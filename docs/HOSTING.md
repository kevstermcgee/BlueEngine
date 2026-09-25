# Hosting Guide: Running a Dedicated Server

BlueEngine provides authoritative server multiplayer with support for:
- Low-latency UDP simulation and delta compression
- Encrypted QUIC datagrams over TLS 1.3 with pinned certificates
- Predictable fixed 60 Hz simulation scheduling via `FixedTickRunner`
- Client prediction, server reconciliation, and lag-compensated hitscan

## Network Requirements

- **UDP Port**: The default game server port is `4000/udp`.
- **Inbound Access**: If hosting on a home network, forward port `4000/udp` on your router to your host machine, or use a tool like Tailscale / UPnP.
- **TLS Identity**: When running in secure QUIC mode, the server uses a PKCS#8 private key DER file. Clients pin the certificate bundled into the binary, ensuring zero public CA dependencies.

## Headless Server Execution

Run the standalone authoritative server:
```bash
cargo run --release --bin be2-headless -- --server 0.0.0.0:4000
```

Or run with an explicit map:
```bash
cargo run --release --bin be2-headless -- --server 0.0.0.0:4000 --map assets/maps/house.json
```

## Running the Smoke Test

To verify connectivity and snapshot flow against your running server:
```bash
cargo run --example network_smoke -- 127.0.0.1:4000
```
