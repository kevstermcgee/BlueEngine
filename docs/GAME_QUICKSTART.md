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
`set_enabled(entity,enabled)`, `complete`. Optional condition: `{"counter":"switches","equals":3}`.
Omitted/null on_interact matches any declared enabled target. Rules run in document order;
later rules see earlier changes. Once applies globally per match. Complete ends interactions.
Enabled controls interaction eligibility only; it does not move/hide geometry or collision.
Targets require line of sight within 2.5 metres. Limits: 8 counters, 16 targets/rules,
4 actions/rule, 8 spawns; counters clamp to +/-1,000,000. No timers, scripts or physical doors.

Multiplayer: launch `be2-headless --game my-game/game.json --server 127.0.0.1:7777`,
then `be2 --game my-game/game.json --connect 127.0.0.1:7777` for each client.
Both sides need identical map and game semantics. The server owns rules/state;
clients send interaction intent. These are development UDP sessions without authentication.
