# Deadfall engine upgrade feedback, 2026-10-07

## Reproduced engine defects and repairs

A rendering-free combat fixture emits eight 64-byte events per 60 Hz tick beside a 404-byte snapshot. Before repair, encoded 1200-byte packets were rejected by the 1100-byte transport: state stopped at tick 1 and local send errors increased. Respecting the transport budget restored state but still lost partial batches of events sharing a tick. `NetGame::RELIABLE_EVENTS` now opts into independently acknowledged event sequences, original event ticks, bounded reordering and retention. Existing games keep their legacy wire and public struct shapes.

An additional repeated-match fixture with 25% loss, four ticks of latency and jitter exposed an untagged lobby packet resetting the new view. Opt-in lobby packets now carry the session token and match epoch too. Final results retry at the game's usual snapshot cadence instead of slowing to 10 Hz; already delivered events remain drainable across transitions.

A deliberately oversized event verifies that an explicit counted gap permits later events and snapshots to progress. The event stream is bounded to 4096 entries, 1 MiB and ten seconds, and emits at most four event datagrams per snapshot. Queue acceptance remains distinct from delivery. Encryption follows the chosen transport; hub rooms currently use UDP without TLS.

Two-player room hooks preserve existing `MAX_SEATS` and public configuration structs while letting a room cap seats to two and require both humans before countdown. Tests also exercise rejecting a third client and cancelling countdown when the friend leaves.

## Evidence and reproduction

Focused gate: `cargo test --locked --profile itest --no-default-features --test netplay_event_delivery` (five tests, including 30% packet loss and repeated matches). Encoder and malformed-frame units live in `viewer::netplay::event_channel`; the canonical full engine check includes them and existing consumers. Full Linux/Windows CI remains required before merge.

Game tests exercise real hub room creation, all eight stable setting IDs, every mode/map, opposing duel teams and replicated cosmetics. Navigation checks cover spawn physics and paths to every pickup/base/site. The game also uses real UDP with six humans/six bots. These tests cannot establish interstate reachability or hardware rendering performance.

## Remaining useful engine work

* Typed per-room context could replace process-global `NetGame` settings and the associated test serialization.
* A shared bounded asynchronous resolver could remove game-side DNS worker glue while retaining responsive cancellation.
* Reusable 3D navigation would remove per-game graphs; jump links must use the actual controller clearance and reject walls.
* Hardware input injection would test the native key path rather than relying solely on scripts. The current complete native keyboard table is adopted by Deadfall.
* Legacy custom-sim projects without `game.project.json` still use native project check/ship scripts; automatic `check --game` cannot yet route those projects.

Learning records L-086 through L-088 include retrieval keywords and concise context hints. Retrieval was checked with `be2.py context "combat event burst packet budget lobby rematch loss" --compact`.
