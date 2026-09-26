# BlueDM

BlueDM is a small, drop-in online team-deathmatch FPS prototype built on BlueEngine.
It uses the engine's reusable FPS module for its ten-weapon armory, deterministic
fire/reload timing, smooth aim-down-sights transition, operative archetypes, health,
teams, scoring, spawn protection, and respawns.

![BlueDM Cobalt Foundry gameplay](preview.png)

## Play locally

```powershell
cargo run -- --host
```

This starts a local authoritative server thread and joins it. There are no bots in
this prototype; run a second client to test combat.

For a repeatable visual smoke capture, run `cargo run -- --host --capture preview.png`.

## Play online or over a LAN

On the host machine:

```powershell
cargo run --bin bluedm-server -- --listen 0.0.0.0:4100 --key YOUR_PRIVATE_KEY
```

Allow UDP port 4100 through the host firewall/router as appropriate. Each player runs:

```powershell
cargo run -- --connect HOST_IP:4100 --key YOUR_PRIVATE_KEY
```

The prototype server uses a direct UDP join key and is intended for trusted friends
and LAN testing. It is not an account system, anti-cheat service, or DDoS-hardened
public service. BlueEngine's production QUIC/TLS profile remains the path for a later
internet-facing deployment.

## Controls

- WASD move; Shift sprint; Space jump; Ctrl crouch
- Mouse look; left mouse fire; right mouse smooth ADS; R reload
- 1-0 or mouse wheel selects any of the ten firearms
- Hold Alt or press Escape to release the mouse

## Content

- `maps/foundry.json`: validated eight-room, two-lane Cobalt Foundry arena
- `blueprints/foundry.blueprint.json`: editable source for the map
- Four procedural operative silhouettes: Vanguard, Recon, Breacher, Field Tech
- Ten procedural firearm/viewmodel styles backed by the BlueEngine armory

## Prototype boundaries

The vertical slice supports direct-IP multiplayer for up to eight players, server-
authoritative movement/combat, static-map occlusion, teams, health, kills/deaths,
respawns, ammunition, reloads, recoil/spread inputs, and ADS. It does not yet include
matchmaking, bots, voice chat, animation blending, imported skeletal meshes, accounts,
anti-cheat, or content download. Those are intentionally future milestones.
