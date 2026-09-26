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
