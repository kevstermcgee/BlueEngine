# Persistent reliability milestone

Review branch: `milestone/persistent-reliability`. Starting main was
`1441a433a76aa3c8d0f507dfe370a5c9583882b1` (fetched 2026-10-18).
`cb19b58` is historical comparison only. No old checkout was reset. Existing
workshop/narrative/authoring branches were inspected; those improvements were
already ancestors of main. Unrelated dirty worktrees were preserved.

## Repairs and guarantees

* Feedback imports deliberately retain `[skip ci]`. Removing it would not make a
  GITHUB_TOKEN push trigger CI. A clean no-change outcome can inherit successful
  Engine **push/main/exact-code-SHA/workflow-path** CI through at most 100
  single-parent commits changing only imported review/feed JSON and summary.
  Current feedback validation must pass. Merges, registry edits and any other
  path stop inheritance and require CI at that revision. Updated code always
  requires local checks plus hosted success on its own SHA. Pending/missing,
  failed, cancelled and mismatched CI cannot acknowledge feedback. Receipts name
  tip, crossed commits and verified ancestor. A clean no-change decision is cached
  by revision and complete feedback fingerprint set; retries recheck CI without
  another AI session. Own unchanged worktrees are removed even if CI is pending;
  modified failures remain for review. Fingerprint receipts use atomic replacement
  and fsync. No public review or fake CI run was created.
* `game_tooling.consumers` discovers maintained editable `game.project.json` and
  legacy tooling-pin declarations under `games/*` and `assets/games/*`. Historical
  migration inventory is used only to approve known old script variants, never
  to limit validation/ownership. All 21 current consumers use canonical entry
  scripts and pins, including Odd Matter and Corner Clutch. Reverie/Leo package
  preparation hooks remain intact. A fresh declaration automatically gains exact
  launcher/bootstrap/pin enforcement and freshness via the actual checker loader.
  Current pins cannot conceal copied entry code. Explicit stale binaries fail
  before game checks. Export/package integrity and hooks keep their tests.
  Exact generated-tooling changes select Python checks; unknown content, Cargo,
  Rust and build inputs retain conservative verification. Published installers and
  immutable release exports were not modified.
* Start now returns up to three relevant contracts, matching context for compound
  presentation/audio requests. Required signatures/limits remain complete.
  Async/long-function navigation and Cargo freshness already existed; stale
  learning hints were superseded while original reports/notes were retained.
  IdeaForge factual wikis no longer require story absence essays or character
  lists/notes for storyless games; rules, controls and genuine capture gates remain.
  Jump-height, offline 2D routing, exact API identifiers and browser retirement
  remain tested. Behavioral-testing commands now enable physics and networking,
  as the actual be2-tools target requires. Metadata-only prepared directories
  (`.git`, `.be2-work`, `.codex`, `.agents`) can scaffold without replacing them;
  ordinary content collisions still fail before writes. Scaffolding checks rustfmt
  before writes and formats assembled sources; all six starters pass their first
  format gate. Workshop help now exits successfully without loading assets.
* Native focus observation moved from Afterpull to shared presentation. Windows
  retains foreground-loss events; Linux uses the supported X11 backend, its own
  event connection and actual X window (never a GLX drawable). Physical keyboard
  and mouse neutrality plus controller neutrality gate resumed input. Out/in
  between frames is retained. Loss clears 2D queued edges, pointer actions and
  look; local ticks freeze. GameShell opens the menu/releases cursor and suppresses
  gameplay until fresh input. It contains no remote-authority pause mechanism.
  Afterpull uses the shared client, retaining its game-owned device mapping.
* Fullscreen stays in-place; explicit capture/size/windowed flags override saved
  preferences. Shared shells persist display mode independently of save schema;
  the 2D client persists it in per-game settings. Menu transition input is drained.
  Existing brief/skippable intro executes before game construction, consumes skip
  edges, and preserves explicit automation frame numbering.
* Macroquad 0.4.14 eagerly constructs its native audio context before a game's
  mute handling; quad-snd's ALSA worker can panic without failing rendering and
  exposes no native device/worker-health API. Muted SoundBank skips synthesis and
  audio priming, but cannot prevent upstream context creation. Capture/package
  evidence now explicitly separates rendering, audio intent and telemetry,
  scripted behavior, physical input and audible hardware. Worker diagnostics are
  retained. Successful screenshots never certify audio health. Asset/render-worker
  failures and playback submissions remain explicit SoundBank status; native
  device availability and actual sound remain unverified without hardware review.
* Inventory preview shares exactly the transaction validator, without cloning or
  committing. Workshop commands validate all fallible conditions before the atomic
  inventory transaction, then finish infallible local updates. Blocked diagnostics
  and attempts are reused only when the complete inventory count map is unchanged;
  cancellation invalidates its machine's blockage. No probabilistic hash cache or
  skipped validation is used. Snapshot import still validates complete candidates.
* Explicit V2 copy migration permits label-only item/recipe/upgrade edits and
  recipe/upgrade additions over unchanged rule fields. Full content identity still
  includes labels, so ordinary exact restore rejects different content. Migration
  restores against exact historical source first, checks compatibility separately,
  validates target state, writes a new slot safely, then activates. Tuning, removals,
  old-state hash tampering, full/failed writes and repeated destinations fail safely.
  Frozen V1 files/rules remain immutable; in-flight jobs retain their paid rules.

## Single-player proof

No Scrapworks source exists in the current declarations/branches examined. The
existing Automation Workshop is the bounded game consumer. Its native view now
states a clear goal: deliver three wire. Two machines compete for twelve storage
units; players acquire ore/coal, smelt plates, choose speed or wire-tools upgrades,
deliver goods to free capacity, and recover blocked output/refunds. D delivers the
selected item (Down/mouse chooses it). Progress and paid work persist through
engine save/reload. Explicit cosmetic content migration preserves that progress.
The headless seeded proof uses these same public commands and authority. It remains
single-player; its pre-existing optional net adapter is a separate acceptance fixture.
There are no narrative runtime dependencies, story files or invented lore.
See [PLAYTESTING.md](PLAYTESTING.md), [WORKSHOP.md](WORKSHOP.md) and measured evidence
under `docs/perf/reliability/`. Numeric evidence is not a fun/quality certification.

## Evidence and limits

Final measurements, source identities, regression counts and platform status are
recorded in `docs/perf/reliability/results.json` and the completion report. Before
and after probes use the identical release harness, hardware, profiles and content.
Historical 51-tick N97 probes remain historical, not release capacity/tail evidence.
Fresh authoring uses a separate scratch game; unavailable token/model telemetry is
explicitly unavailable. No dollar cost or universal speedup is inferred.

Subsequent milestones remain separate: larger custom-netplay snapshot/interest
scheduling and late-join/seat lifecycle; an installed-player updater with signed
release discovery, resumable download, safe activation and rollback; larger world
region saves/streaming and bounded import; advanced narrative/campaign direction.
Optional narrative retains stale-choice protection, atomic effects and save/reload
semantics without expansion here. Real public player-driven improvement is not
claimed: imported observations retain their provenance and untrusted status.
Windows x64 native installer/runtime, physical controllers and audible output
require their actual platform/device gates; Linux/cross builds cannot certify them.
