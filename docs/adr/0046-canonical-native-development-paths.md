# 0046: Canonical native development and delivery paths

## Status

Accepted. Starting revision: `b8a7f9ffba70a5ca155395ce1f5eb5d7108c721a`.

## Context

Review findings from `28a8c4d` still applied: the default `client,offline` features built the
cinematic executable; `be2.py build client` and package explicitly included it; net-proxy's
Ctrl-C handler only existed with that feature. Shipping unconditionally checked desktop
shortcuts and rejected similarity to unrelated icons. The old multiplayer template owned
a separate UDP handshake/lobby, and BEA owned another map-selection GUI. Stock weapon
ranges and impulses were embedded in HeadlessWorld beside reusable simulation services.

## Decision

- Archive the old multiplayer prototype under `docs/archive/multiplayer-game`, including
  presentation/source history. Keep its existing published destination and mapping under
  engine ownership; removing that mapping otherwise deletes unchanged exported files.
  `preserve` is for independently maintained, unexported content and must not overlap that
  mapping. Read-only publication checks enforce the same ownership constraint as export.
  New projects use `NetGame`, `ClientView`, `netplay::cli::serve`
  and the existing ToyGame/ToyView/toy-server example. No replacement networking runtime.
- Default features are `client` only. `build cinematic` explicitly enables historical
  `offline` for `vesper3d`; default packages omit that executable. Scene/geometry/math and
  public library names remain compatible. Full verification explicitly covers cinematic
  features. Net-proxy reuses existing dependency-free native shutdown, including Windows.
- `verify --json --smoke` checks the package without desktop access. `ship --no-install`
  builds and smoke-tests it without installation. `ship` installs a shortcut and verifies
  its own target/start-in/icon; `--folder` and `--check-shortcut` request those checks.
  Icon similarity is opt-in advisory, including unavailable shell diagnostics. Resources,
  hashes, asset closure, isolated captures and explicit skips remain. Project checks retain
  display-independent package verification; shipping requests isolated smoke. Hosted Windows
  runners cannot create Leo's required WGL context, so actual native rendered smoke remains
  unverified there (Linux's isolated package smoke is exercised). Do not treat that limit as
  passing Windows rendering evidence or weaken normal `ship` to hide it.
- BEA's old PowerShell entry redirects to a small batch compatibility entry. Bare launch
  opens the sandbox; existing stock-client arguments still reach be2 without substitution.
  The engine desktop shortcut selects sandbox. Game shortcuts still target game dist/.
- Stock Scientist/Feta weapon policy lives in `viewer::stock_demo`. Shared fixed-tick
  timing lives in `viewer::combat`; HeadlessWorld resolves occluded prop attacks with
  supplied range/impulse/release policy. `viewer::fps` remains new-game weapon/damage policy.

## Compatibility and retained adapters

`viewer::weapons`, `viewer::wrench`, HeadlessWorld's `fire_pistol`/`fire_wrench`, and stock
wire fields remain compatibility adapters. Actual consumers include stock be2/server,
character/wrench rendering, sandbox, external published games and attack regressions.
They are not retired APIs and are excluded from new-project guidance. No protocol,
authored-data, save, identifier or public library-name migration is introduced.
Known historical generated check/ship scripts are refreshed only after byte comparison
against template history; customized copies are not overwritten. Native portable 2D/3D,
input/audio/storage, rendering-free authority, networking and game capture remain.
Separate browser-runtime cleanup is outside this decision.

## Consequences

Maintained generated scripts keep their canonical project helper (including Leo's older
project). Starter instructions retain the existing 3 KB context limit, and discovery
contracts retain shutdown/transport details and their recorded retrieval floor.
The retained in-process hub fixtures transfer an already-bound socket into startup:
allocation may retry a contested port, but gameplay assertions never retry or weaken.
The flood fixture sends each positive probe once with a 3 s receive deadline for CI
scheduling, checks its sender/nonce/reply kind, and retains the original burst bounds,
refill delay and garbage-silence windows. Fake-clock limiter tests preserve exact policy.

## Acceptance and evidence

Required: default build/package exclude cinematic; explicit cinematic CLI/unit/Clippy/doc
checks pass; headless dependency boundary and real proxy Ctrl-C pass; packages never read
Desktop unless requested; own shortcut errors still fail; similarity only warns; toy
Netplay/process tests and attack timing/occlusion/protocol regressions pass; native sandbox
captures and portable consumers pass. Linux/Windows engine CI remain required. Unavailable
device/OS interaction is reported separately, never promoted from a skip.

Commands: `python3 tools/be2.py check --changed --plan`, then `check --changed` (full
fallback for this scope); `python3 scripts/publish_games.py check`; isolated native
`ship --no-install`/`verify --smoke`; `cargo run --no-default-features --bin be2-toy-server
-- --info`; `cargo run --profile fast --bin blueengine-sandbox -- --capture NEW_DIR`.
Receipts and remaining verification status live in the ignored `.be2-work` task record.
No build-speed, memory or model-token improvement is claimed without a comparison.
