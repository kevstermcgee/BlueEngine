# BlueEngine Multiplayer Game Template

A starter template for building authoritative multiplayer prototypes with BlueEngine.

Harvested directly from real-world lessons building and shipping **Feta**.

---

## What This Template Provides Out of the Box

### 1. Complete Front-End Game Shell (`src/menu.rs`)
- **Main Menu**: Play Online, Explore Offline, Settings, Quit.
- **Connection Screen**: Direct IP:Port joining with join key support.
- **Lobby Screen**: Character selection, match settings configuration, readiness confirmation.
- **In-Game HUD**: Crosshair, round status, score/objective display, ping indicator.
- **Pause Menu**: Resume, Settings, Leave Match.
- **Match Results & Rematch**: Winner display and synchronized rematch return to lobby.

### 2. Robust Multiplayer Foundation
- **Authoritative Server**: Server runs 60 Hz deterministic simulation with `FixedTickRunner`.
- **Client Prediction & Reconciliation**: Zero-latency local movement with authoritative rollback replay using `ControllerState` / `KinematicState`.
- **Remote Interpolation**: Remote entities smoothed via interpolation buffers.
- **Reliable Action Counters**: Monotonically increasing counters for jumps, primary, secondary, and interact actions that survive packet loss.
- **Server Lag Compensation**: Bounded historical pose buffer (`PoseHistory<T>`) for fair hitscan combat with rewind clamping.
- **Transport Boundary**: The template uses the `DatagramTransport` abstraction but
  currently instantiates raw `UdpTransport`. Its join key is sent in the initial
  `Hello`; it is access control for local prototyping, not encryption or the engine's
  HMAC-authenticated session handshake.

---

## Directory Structure

```text
templates/multiplayer-game/
├── Cargo.toml               # Cargo dependencies pointing to BlueEngine
├── README.md                # Documentation and architecture guide
└── src/
    ├── main.rs              # App entrypoint (client GUI or headless server)
    ├── menu.rs              # UI screens (Main Menu, Connect, Lobby, Pause, Results)
    ├── lobby.rs             # Lobby protocol, readiness synchronization, slot assignment
    ├── game.rs              # Authoritative game logic, round phases, tick stepping
    ├── client.rs            # Client network loop, prediction, and renderer hooks
    └── server.rs            # Dedicated headless server runtime
```

---

## Quickstart

### Run Offline Exploration
```bash
cargo run
# Select "Explore Offline" from the main menu
```

### Run Dedicated Server Locally
```bash
cargo run -- --server 0.0.0.0:4000 --key my-secret-join-key
```

### Connect Clients
```bash
cargo run -- --connect 127.0.0.1:4000 --key my-secret-join-key
```

Do not expose this template server directly to an untrusted network without replacing
its raw-UDP handshake. For the engine's authenticated UDP path, use `be2-headless` and
`be2` with matching `--auth-key` values as described in
[`docs/HOSTING.md`](../../docs/HOSTING.md).

---

## Customizing This Template For Your Game

1. **Change Match Rules (`src/game.rs`)**:
   - Replace default phases (`Lobby`, `Playing`, `Finished`) with your game's specific sequence.
   - Adjust `MatchSettings` (round timers, scores to win).
2. **Add Custom Weapons / Abilities (`src/game.rs`)**:
   - Hook into `ActionCounters` (jump, primary, secondary, interact) or add game-specific counters.
   - Use `PoseHistory` for authoritative rewind validation.
3. **Customize Visuals & Maps (`src/client.rs`)**:
   - Swap the default map (`Suburban House` or `Blue Test Lab`) with your authored `.json` map document.
