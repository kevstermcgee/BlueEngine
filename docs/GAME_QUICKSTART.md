# Playable prototypes without source access

Use `be2-tools game-describe` for limits and semantics, `game-schema` for JSON shape.
With a source checkout, build once: `cargo build --locked --bins`. Commands below
use `target/debug/`; packaged builds place executables in `bin/`.

```powershell
target/debug/be2-tools game-example my-game
target/debug/be2-tools game-validate my-game/game.json
target/debug/be2 --game my-game/game.json
target/debug/be2-headless --game my-game/game.json --ticks 600
```

Start exploring, move with WASD or arrow keys, aim at each blue switch and press E.
Three switches unlock the green exit; press E there to complete the objective.
Escape opens the menu. The committed example is `assets/games/three-switches/game.json`.

`game.json` references a sibling/child map. Edit movement values in `player_profile`,
feet coordinates/yaw in `spawn_points`, initial `counters`, `interactables` and `rules`.
Metres, +Y up, radians, yaw zero faces -Z. One profile per game; spawns cycle by player ID.
Use native map tools for geometry. Interactable IDs must match static axis-aligned box
node/collider/entity IDs with equal bounds. Run game-validate after every edit.

A rule example:
```json
{"id":"switch-a","on_interact":"button-a","condition":null,"once":true,
 "actions":[{"action":"increment","counter":"switches","amount":1},
            {"action":"set_enabled","entity":"button-a","enabled":false}]}
```

Actions: `increment(counter,amount)`, `set_counter(counter,value)`,
`set_enabled(entity,enabled)`, `set_mover(mover,open)`, `start_timer(timer)`, `stop_timer(timer)`, `complete`. Optional condition: `{"counter":"switches","equals":3}`.
Triggers: `on_interact` (aim + press E), `on_enter` (stepping into a `trigger_zones` AABB volume),
`on_exit` (stepping out of a trigger zone), or `on_timer` (expiration of a countdown timer). Omitted/null on_interact matches any declared enabled target.
Rules run in document order; later rules see earlier changes. Once applies globally per match. Complete ends interactions.
Enabled controls interaction and trigger zone eligibility. Kinematic `movers` smoothly translate box colliders
between closed and open states over `duration_ticks`, dynamically blocking or opening pathways for players.
`timers` provide deterministic fixed-tick countdowns (`duration_ticks`, `auto_start`, `repeats`) to dispatch delayed actions.
Targets require line of sight within 2.5 metres. Limits: 8 counters, 16 targets/zones, 16 movers, 16 timers, 16 rules,
4 actions/rule, 8 spawns; counters clamp to +/-1,000,000. No arbitrary scripts or irregular geometry mutation.

Multiplayer: launch `be2-headless --game my-game/game.json --server 127.0.0.1:7777`,
then `be2 --game my-game/game.json --connect 127.0.0.1:7777` for each client.
Both sides need identical map and game semantics. The server owns rules/state;
clients send interaction intent. For an authenticated session, add the same
`--auth-key "LONG_RANDOM_SECRET"` to both commands. Authentication uses
challenge-response, session tokens and replay protection; the development UDP
transport still does not encrypt payloads.
