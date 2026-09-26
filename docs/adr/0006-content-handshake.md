# 0006: Content compatibility handshake

Status: Accepted for the content handshake. Transport conclusions superseded by 0008.

## Context
The existing dedicated UDP server has sequence rejection, stale-input expiry,
authoritative ownership/combat and acknowledged delta recovery. Its hello message
did not identify map content. Identical player input on different collision maps
can diverge even if both endpoints use the same protocol version.

## Decision
Protocol 2 requires an initial map fingerprint in Hello. Hash scene, collision,
semantic entities and spatial graph before physics extraction; canonicalize graph
maps and sets. Reject mismatches before allocating a player/session. Return an
explicit rejection to the graphical client. Also reject clients when eight sessions
already exist. Resolve overlapping room membership by smallest stable RoomId.

## Consequences
Both endpoints must be rebuilt; old hello packets lacking content identity fail
closed. FNV-1a is an accidental-mismatch diagnostic, not a cryptographic digest or
authentication. The initial hash assumes the host does not later mutate content.
Array order and document representation are significant; use the same map file on
both endpoints. Future gameplay content must extend this versioned fingerprint.

JSON over bounded 1400-byte UDP remains the current codec. Compact binary encoding,
snapshot chunking, authenticated transport and server-issued reconnect tokens are
separate work; this pass does not claim to implement them. Existing address-bound
sessions are suitable for development, not authenticated Internet matches.

Malformed packets are discarded before dispatch. Receive buffers accommodate a whole
UDP datagram, avoiding truncated-prefix acceptance and Windows oversize errors. A
receive call drops at most 32 malformed datagrams; a server poll handles at most 256
valid packets before yielding to simulation. Real socket failures still propagate.
