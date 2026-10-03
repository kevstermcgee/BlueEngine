# Playable games and source-free prototypes

## Pick the starter first

| The game's rules | Starter |
|---|---|
| fit counters, interactables, timers, triggers and movers (find things, press switches, open doors, timed objectives) | **stock**: `be2-tools new-game my-game ../my-game` |
| need enemies, projectiles, scoring, AI, waves, procedural content or per-frame physics: anything `GameDocument` cannot express | **custom-sim**: `be2-tools new-game my-game ../my-game ../BlueEngine custom-sim` |

`GameDocument` deliberately has no scripting, so do not stretch it to fit an action game. A custom-sim
game keeps its rules in a pure, seeded, fixed-step Rust library (the contract the engine asks of its own
simulation) and owns its window loop. It builds on the engine's `devkit` (frame clock, fixed stepper,
input accumulator, deterministic-replay and capture helpers, saves, screen-feel, synthesised audio) and
`kit` (batched dynamic meshes, materials, effects, HUD, sound); see [custom clients](CUSTOM_CLIENT.md) and
"Custom loops" in [shared gameplay](SHARED_GAMEPLAY.md). Both starters ship the same way.

## Definition of done (every game made with BlueEngine)

1. `python scripts/check.py` passes on the final files. Its last stage is the **ship gate**, so it fails
   until step 2 is done (`--skip-ship` runs everything else while iterating; `--content-only` needs no Cargo).
2. **The game ships as a program with its own identity**, created and verified by script, not by hand:
   - `assets/identity.json` holds the real title, a tagline and the controls. Change the placeholders the
     scaffold derived from the project name; a placeholder title (`Game`, `My Game`, ...) fails the gate.
   - The scaffold generated a title-seeded icon set (`assets/icon.ico`, `icon_{16,32,64}.rgba`, `icon.png`).
     Draw your own if you like, or regenerate with `be2-tools icon TITLE assets --replace` (add a number,
     `be2-tools icon TITLE assets 3 --replace`, for a different design when it resembles another shortcut).
   - `scripts/blue ship` builds a release package in `dist/` (exe, icon, content), creates
     `<Desktop>/<Title>.lnk` targeting `dist/`, never `target/`, with the tagline and controls as its tooltip,
     and verifies it: the shortcut's target, start-in and icon, an icon that is distinct from every other
     shortcut on that desktop, the exe's embedded icon and version info, and (with a display) the window
     title and window icon read back from a launch through the shortcut plus a smoke capture.
     Where there is no desktop (CI) those checks report `skipped: no desktop`, never a silent pass.
     Linux gets a `.desktop` file, macOS a `.command`. `scripts/blue package` and `scripts/blue shortcut`
     run the halves separately.
3. You exercised the changed controls and looked at real frames of the shipped program, and you say what
   you did not verify.

## The stock starter

```sh
be2-tools new-game my-game ../my-game
cargo run --release --manifest-path ../my-game/Cargo.toml
cargo test --manifest-path ../my-game/Cargo.toml --no-default-features
```

Its main loads `GameDocument::load(.../game.json)` and awaits
`playable::run_game_with_options(game, options, platform::focused)`. The shared runtime runs
authored rules, objectives and dynamic props locally without a server. Aim at the
blue terminal and press E to win; press E again to reset. E also picks up/drops props.
Use `--connect ADDR` for server-owned multiplayer with the same content. The existing
static `local_client::run_map` viewer is intentionally not a gameplay runner.
See [shared gameplay](SHARED_GAMEPLAY.md) for configuration and capture options.

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
Press E again to restart the loaded game; online, any joined player may restart.
Escape opens the menu; F5 quick-saves and F9 quick-loads (see [save states](SAVE_STATE.md)).
The committed example is `assets/games/three-switches/game.json`.
For a small timed puzzle with a shutter, routed movement and repeated calibration, see the
[Observatory Night Watch authoring recipe](../assets/games/observatory/README.md).

`game.json` references a sibling/child map. Edit movement values in `player_profile`,
feet coordinates/yaw in `spawn_points`, initial `counters`, `interactables` and `rules`.
Metres, +Y up, radians, yaw zero faces -Z. One profile per game; spawns cycle by player ID.
Use native map tools for geometry. Interactable IDs must match static axis-aligned box
node/collider/entity IDs with equal bounds. Run game-validate after every edit.
To move a canonical box, use an `apply` patch with `op:"translate"`, selecting its node, collider and entity
together and supplying one `delta`. The toolkit preserves their exact bounds agreement, including fractional
translations. Partial selections and inconsistent records still fail game validation.

A rule example:
```json
{"id":"switch-a","on_interact":"button-a","condition":null,"once":true,
 "actions":[{"action":"increment","counter":"switches","amount":1},
            {"action":"set_enabled","entity":"button-a","enabled":false}]}
```

Actions: `increment(counter,amount)`, `set_counter(counter,value)`,
`set_enabled(entity,enabled)`, `set_visible(entity,visible)`, `set_mover(mover,open)`, `start_timer(timer)`, `stop_timer(timer)`, `complete` (win), `fail` (lose). Both end the match: later events and timers are
ignored until the game restarts (E / X again).

Optional `condition` (null is unconditional). A leaf names a counter and one or more comparisons, which must
all hold: `equals`, `not_equals`, `less_than`, `greater_than`, `at_most`, `at_least`. Add `modulo` to compare the
remainder instead (`{"counter":"phase","modulo":2,"equals":0}` is "phase is even"). Combine leaves with `all`,
`any` and `not`, nested at most 4 deep and 16 parts in all:

```json
{"all":[{"counter":"countdown","at_least":1},{"not":{"counter":"stage","equals":3}}]}
```

The bare `{"counter":"switches","equals":3}` form is unchanged. A lost bomb timer is one rule:
`{"on_timer":"fuse","condition":{"counter":"countdown","at_most":0},"actions":[{"action":"fail"}]}`.
### Adding an interactable

An interactable is five records that must agree: in `map.json` a material, a node, a collider and an
entity, and in `game.json` an `interactables` entry. Do not write them by hand. This creates all five, validates
the game as a whole, and only then writes anything:

```sh
be2-tools add-interactable game.json vent --at=3,1.5,-2 --label="Air vent" --disabled
be2-tools add-interactable game.json vent --at=3,1.5,-2 --label="Air vent" --disabled --write
```

The first call is a dry run: it prints the records it would create and touches nothing. `--write` replaces
both files atomically and restores the map if the game cannot be written. Options use the `--name=value` form
(so negative coordinates work): `--at=X,Y,Z` (required, metres), `--size=HX,HY,HZ` (half extents, default
0.3 each), `--color=R,G,B` (linear 0..1), `--label=TEXT`, `--disabled`, `--hidden`. It warns when no rule
reacts to the new target yet (and prints a `rule_template` to adapt) and when the box overlaps another
interactable. The rule itself is yours to write: it is the one part the tool cannot guess.

### Does the game actually work?

`game-validate` says a document is well formed; it cannot say the game is playable. `game-explore` searches every
state the rules can reach, using the engine's own rule runtime, and reports what it finds:

```sh
be2-tools game-explore game.json
```

It prints the shortest way to win and to lose (`timer fuse runs out x3` means three expiries in a row), and
findings by level. **Errors** make the command exit 1: the game can never be won. **Warnings** are dead parts: a
rule that never fires, a target nothing enables or that no rule reacts to, a timer nobody listens to or never
starts, a counter that rules change but no condition reads (so it only decorates the HUD), a `fail` that can never
happen, a target switched back on after its `once` rules are spent (`once-exhausted`), and stuck states (reachable, not lost, and no longer winnable, with the way in). **Info** notes a game that
cannot be lost, and targets that can be pressed in a state where nothing happens (often armed a step too early).

Time and movement are abstracted: any running timer may run out at any moment and any enabled target may be pressed,
so "can be won" ignores a physical obstacle or a timing window, while "never" findings are exact. Counters no
condition reads are left out of the state, and counters compared only by `modulo` or only by thresholds while they
move one way are folded into equivalent values, so a repeating timer does not make the search unbounded. If
`--max-states=N` (default 100000) is reached the report says `truncated` and the "never" findings become
notes rather than warnings. Run it before writing scenarios: the shortest win is the first scenario to write, and `--scenario=OUT.json` writes it for you
(see [behavioral testing](BEHAVIORAL_TESTING.md)).

Triggers: `on_interact` (aim + press E), `on_enter` (stepping into a `trigger_zones` AABB volume),
`on_exit` (stepping out of a trigger zone), or `on_timer` (expiration of a countdown timer). Omitted/null on_interact matches any declared enabled target.
Rules run in document order; later rules see earlier changes. Complete and fail end the match.

**`once` is per match and never resets.** A `once` rule that has fired can never fire again for the rest of the match,
even if the target it acts on is switched back on. That is what you want for a one-way step (press the switch, open the
door) and wrong for anything that can be retried. For a "wrong input resets the puzzle" design, make the rules that
must run again repeatable (`once: false`) and guard each one with a counter condition so it only fires at the right
step (`{"counter":"seq_step","equals":2}`); put the reset rules *before* the step rules, because a later rule sees an
earlier rule's changes and would otherwise fire in the same press. `game-explore` warns (`once-exhausted`) when a
target is switched back on after every rule on it has used up its `once`.
Enabled controls interaction and trigger zone eligibility. Visibility is separately
replicated for interactable geometry and never changes collision or eligibility; use
both actions when an object should disappear and stop responding. Kinematic `movers` smoothly translate box colliders
between closed and open states over `duration_ticks`, dynamically blocking or opening pathways for players. A player
standing on top of a mover is carried with it in any direction, so a mover can be a lift or a moving platform, not only a
door; a mover that slides or rises into a player pushes them out through ordinary collision. The headless world and the
local client apply this; a custom client that steps `GameRuntime::step_movers` itself gets the moves back and should call
`Controller::ride` on its own controller for each one (the stock client does), or its prediction will drop the rider and
be corrected by the server.
`timers` provide deterministic fixed-tick countdowns (`duration_ticks`, `auto_start`, `repeats`) to dispatch delayed actions.
Targets require line of sight within 2.5 metres. Limits: 32 counters, 64 targets/zones, 64 movers, 64 timers, 64 rules,
4 actions/rule, 8 spawns; counters clamp to +/-1,000,000. No arbitrary scripts or irregular geometry mutation.

Run input-driven assertions against the real public boundary with
`be2-tools sim assets/games/three-switches/scenario.json`; see
[behavioral testing](BEHAVIORAL_TESTING.md).

Multiplayer development: launch `be2-headless --game my-game/game.json --server 127.0.0.1:7777 --transport development`,
then `be2 --game my-game/game.json --connect 127.0.0.1:7777 --transport development` for each client.
Both sides need identical map and game semantics. The server owns rules/state;
clients send interaction intent. For an authenticated session, add the same
`--auth-key "LONG_RANDOM_SECRET"` to both commands. Authentication uses
challenge-response, session tokens and replay protection; development UDP does not
encrypt payloads. For deployment use `--transport production` on both peers, set
`BLUE_TLS_CERT_FILE` to their shared public DER certificate, and set
`BLUE_TLS_KEY_FILE` to the server's matching PKCS#8 DER private key.
