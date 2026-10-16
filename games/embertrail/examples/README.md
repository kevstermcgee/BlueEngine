# Development traversal probe

`route_probe` searches for a route through one isolated course using the real
fixed-step simulation and public `Intent` inputs. Course indices are zero-based.

```sh
cargo run --locked --no-default-features --example route_probe -- 3 900
cargo run --locked --no-default-features --example route_probe -- --replay 3 .blue-check/route-04.json
```

Each successful search saves its inputs and an independent replay receipt under
`.blue-check/`. Replays begin at the course's normal entrance with five lives;
they assert the course clears, its guardian is defeated, and no life is lost.
The last course also runs its ending transition and asserts the game is won.
After course initialization, only normal movement, jump, run and cast inputs
advance the replay. No position, health, enemy or collision changes assist it.

The search branches from real prior simulation states. Its scoring rewards
entering boss courts, damaging guardians, and leaving after their defeat. Court
entry and the final hit must retain their rewards: a discontinuous objective
can make a bounded search avoid a reachable battle. Set
`EMBERTRAIL_PROBE_TRACE=1` to inspect progress every 480 simulation ticks.

These receipts prove individual course reachability through normal inputs. They
do not certify a contiguous campaign run, physical controller handling, graphics,
audio playback, or the subjective quality of a human play session. A search that
reaches its bound without finding a clear also does not prove a course impossible.

`grotto_probe` separately verifies the underground reward route and far-stair
return in an isolated room. It lets the normal spawn protection expire, places
Mote at the real surface root entrance as an explicit initial fixture, then
uses public Down, movement, run and jump inputs. It never alters health,
geometry, enemies or position after that initialization. A successful route
collects the real upper lantern reward, leaves by the far stairs and replays
independently from the same fixture without losing a life. This does not prove
surface entrance access or a contiguous campaign run.

```sh
cargo run --locked --no-default-features --example grotto_probe -- 0 900
cargo run --locked --no-default-features --example grotto_probe -- --replay 0 .blue-check/grotto-route-01.json
```
