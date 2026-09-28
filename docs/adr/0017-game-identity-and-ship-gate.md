# ADR 0017: Every game ships its own identity, verified by a gate

Status: Accepted

## Context

The feedback's central requirement (R1): every game made with the engine must
ship a unique desktop shortcut — its own name and icon, targeting a packaged
build, created and verified by script, and enforced by the game's own check
runner rather than left to memory. Before this, a game's window title, icon and
any shortcut were whatever the author happened to type, with nothing to catch a
leftover "BlueEngine game" title, a shortcut pointed at `target/debug/`, or two
games sharing one icon on the same desktop.

## Decision

- **`assets/identity.json`** (`viewer::identity::Identity`): title (1-60
  characters, not a placeholder like "Play"/"Untitled"/"BlueEngine"), tagline and
  controls, validated and rendered as the window title, the shortcut tooltip and
  the `.exe` resource strings.
- **A deterministic, title-seeded icon** (`viewer::icon`): a self-contained
  generator (own DEFLATE/PNG encoder and ICO writer, no image-crate dependency)
  producing a 10-size `.ico`, the `icon_{16,32,64}.rgba` the window uses directly,
  and `icon.png`. `render`/`ico` are pure functions of `(title, variant)`, so a
  clash is fixed by `--variant N`, not by hand-editing art. Calibrated against
  200 sampled titles: median signature distance 31 bits, zero pairs under 6 bits
  in the sampled set (three per 238,800 in a broader adversarial sample); every
  frame verified byte-exact against three independent decoders (WIC, `LoadImage`,
  GDI+) plus an independent Python re-implementation of the ICO/PNG parse.
- **`build.rs`** (`templates/game_build.rs`) embeds the `.ico` and version info
  (`FileDescription`/`ProductName` = title, `Comments` = tagline) as Windows
  resources through `rc.exe`, so the compiled `.exe`, its window and its shortcut
  show the same identity without a separate manual step. A missing SDK, icon or
  identity file is a build warning, never a build failure.
- **`scripts/ship.py`** (`templates/game_ship.py`): `package` (release build into
  `dist/`, replacing or removing only files it installed itself — `dist/saves/*`,
  `*_save.json` and `settings.json` survive re-packaging), `shortcut` (creates or
  refreshes `<Desktop>\<Title>.lnk`, refusing to overwrite a same-named shortcut
  that targets a different program unless `--force`), and `verify` (identity,
  icon files and art, wiring, package, exe resources, the shortcut's file/icon/
  uniqueness, a real launch through the shortcut, and a smoke capture — eleven
  named checks, each always run so one failure does not hide another). Icon
  uniqueness against the desktop's existing shortcuts uses the same signature
  distance as generation, calibrated the same way: shape and colour thresholds
  set from a real desktop's shortcuts (13 distinct pictures, 86 same-picture
  pairs, 156 synthesized near-duplicates) plus that same 200-title sample, with
  every distinct pair on the right side of the line by at least 19%.
- **The ship gate**: `scripts/check.py` ends with `ship.py verify`, failing with
  `ship gate: <check>: <detail>. Run: python scripts/ship.py ship` until the game
  ships. Without a desktop (CI) the shortcut checks report `skipped: no desktop`
  explicitly, never a silent pass; `--skip-ship` runs everything else and says so;
  `--content-only` never runs the gate. `scripts/blue`/`blue.ps1` gained
  `package`/`shortcut`/`ship` subcommands. The full check also gained a `lock`
  step (`cargo metadata --format-version 1`, never `generate-lockfile`, which
  would discard the pins a scaffolded game's `Cargo.lock` was seeded with).

## Consequences

A game is not "done" by the engine's own definition until it has a working,
distinct desktop launcher, checked by running it — not by inspecting files.
`verify --launch --smoke` on Bouncer's real shipped exe (built before this
requirement existed) caught a real, pre-existing inconsistency: its window title
is "BOUNCER" but its `.exe` resources say "Bouncer", which `exe-resources`
correctly fails on. That is the gate doing its job, not a false positive; fixing
such a mismatch is the game's job, not the checker's.

## Rejected or deferred

- **Fixing Bouncer's own title/resource mismatch**: out of scope for an engine
  change; noted for whoever next touches that game.
- **A single hard-coded similarity threshold with no calibration record**: the
  chosen constants are committed next to the measurements that justify them, so
  a future change to the generator or the check can be re-validated against the
  same evidence instead of by feel.
- **Cross-platform desktop scanning**: shortcut generation and `verify --folder`
  work through pure content assertions on Linux/macOS targets, but reading a
  real Linux or macOS desktop was not exercised on real hardware.
