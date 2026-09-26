# 0008: One authoritative path over explicit transport profiles

Status: Accepted. Supersedes the UDP-only transport conclusions in 0006.

## Context

The authoritative server owned `UdpTransport` directly even though a datagram trait
and QUIC/TLS implementation existed. Packet framing was duplicated in UDP helpers,
so the executable QUIC implementation could not reach normal session, simulation or
replication behavior. Operators also had no unambiguous development/production choice.

## Decision

`DedicatedServer<T: DatagramTransport>` owns only a configured datagram transport.
Protocol packet encoding and malformed-packet filtering are default operations on the
transport boundary. Both executables select `development` (raw UDP) or `production`
(QUIC datagrams over TLS 1.3); compatibility defaults to development, while deployment
examples select production explicitly. All profiles share an 1100-byte payload ceiling.

The production client pins `BLUE_TLS_CERT_FILE`, falling back to the bundled certificate,
and uses its `feta.local` identity. The server reads the same certificate and the
matching PKCS#8 DER key from `BLUE_TLS_KEY_FILE`. TLS authenticates
the server and encrypts payloads; optional `--auth-key` remains the bounded client
authentication mechanism. No silent transport fallback is allowed.

## Consequences

The authoritative simulation, session checks, packet limits and snapshot behavior are
identical across transports. Tests can inject alternate transports, and an end-to-end
QUIC test exercises the real handshake through `DedicatedServer`. UDP remains useful
for deterministic proxy/loss tooling but must not be described as production-secure.
Certificate rotation requires distributing a matching pinned certificate, and
the protocol still lacks snapshot chunking, traffic-analysis resistance and migration.
