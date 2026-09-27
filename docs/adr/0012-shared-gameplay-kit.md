# 0012: Promote sandbox infrastructure into shared engine APIs

Status: Accepted

## Context
Sandbox-only controller wiring and text fixes required repeated integration work.
Placement, cosmetics and UI were private executable modules. The new-game scaffold
also contained an invalid spawn, invalid GameDocument JSON and a load-only main.

## Decision
Promote reusable Rust implementations into viewer modules. Make the sandbox and
stock character renderer import them. Keep device lifetimes in application-owned
ClientInput, route ShellActions snapshots into the shared menu, and keep graphics
behind presentation/client features. Creative map editing remains headless.
Provide a small static MapPlayer/run_map client and generate playable projects
against it. Validate the generated stock game document with the real schema.

## Consequences
Future fixes propagate through one library implementation. No sandbox source is
copied into generated games. The map palette/catalog and creative editing policy
remain opt-in application behavior. The small static client does not execute
GameDocument rules or dynamic physics; the stock client remains the full runtime.
Native foreground queries stay in the host executable. See SHARED_GAMEPLAY.md.

## Follow-up: generated gameplay runtime

The static viewer remains supported, but new-game now consumes the shared authored
runner also used by ordinary stock `--game` launches. A rendering-free GameSession
adapts local HeadlessWorld authority or online prediction/snapshot presentation;
applications no longer assemble a second rules loop. The existing prop renderer is
promoted into the library. Restart caches authored content and preserves player IDs
and monotonic ticks; any joined player may restart after completion. Protocol 6
replicates round identity and mover progress, rejecting old-round sequenced commands.
This deliberately retains bounded GameDocument behavior instead of introducing
another game framework or arbitrary scripting system.
