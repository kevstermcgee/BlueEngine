# AI authoring reliability milestone

Signal Garden exercises the normal custom-sim authoring, verification and shipping path:
six ordered relays, finite cargo, regenerating pickups, patrol damage, healing, an eight-minute
defeat condition, a final return objective, restart, stock meshes/HUD/menu, effects and ambience,
settings, exact saves and a real save migration. First attempts are designed for 5–10 minutes;
human completion time is unmeasured. This is a bounded game, not an engine subsystem.

## Engine changes found through the game

The baseline was `4136080c7f42c8c2becf7b9597bf2de11e5c44a3`. Its ship verifier checked
only the executable hash and top-level entries; smoke ran in dist with developer leftovers.
The new integrity gate verifies manifest completeness, every recorded hash, portable paths,
regular files without symlinks, and stock GameDocument map dependencies without a display.
Graphical smoke copies only verified declarations outside the checkout. Player files remain
untouched in dist and are excluded from smoke. Failed runs retain full stdout/stderr.

Capture runs used to mute the entire audio path. `--audible`, `AudioStatus` and
`verify_playback(expected_variants, expected_stems)` now distinguish silent screenshots from
completed audio loading and playback submissions. The starter includes that gate.
Capture helper `--audible` opts out of its default mute and retains the executable's reports;
the final packaged audit caught and repaired that override trap with regression coverage. Missing
values for shared run flags are rejected. Analysis of actual generated WAVs exposed a chord
tail cut at nonzero amplitude; the shared synth now fades finite tails before truncation.

Building and shipping also exposed settings rows inaccessible by keyboard/pad, a generated
Linux launcher without executable permission, a Bash variable copied into PowerShell's play
command, and authoring discovery that did not find the documented tool build output. Those
are fixed in shared facilities, with regressions. Documentation corrects the trap that a serde
default alone preserves Exact saves when the state hash changes.

## Context and modification exercise

A fresh creator reads root AGENTS, the context packet, GAME_QUICKSTART, CUSTOM_SIM_CHEATSHEET
and the generated game's AGENTS/library/tests. The starter provides platform, lifecycle,
menu, settings, sound, saves and packaging wiring. Authoring starts with `tools/author.py describe`;
it now discovers source-checkout builds. Asset search found no matching garden content;
the demonstration builds its small models using stock kit primitives.

A maintainer reads `games/signal-garden/AGENTS.md`, `src/lib.rs` and `tests/determinism.rs`.
Presentation or cue changes additionally need `src/main.rs` and `src/audio.rs`. The shield
exercise touched those four implementation/test files: one cargo unit buys three seconds of
protection. It preserved the original public-input solutions. The actual version-one save
fixture at tick 331 migrates into version two and finishes the original winning route.
No engine implementation inspection was needed for that modification. It was a staged audit
by this agent, not an independent trial with a weaker model.

Creation required inspecting roughly a dozen engine implementation files for maintenance
and API diagnostics, plus templates/tests. This is an approximate count, not a token benchmark.
The modification guide removes that requirement for the next agent. Remaining copied client
plumbing (cue/audio/menu glue) is visible in main.rs rather than hidden behind another framework.

## Verification and evidence

The exact tested commit, final results, executable hash, report paths and package file hashes
are recorded in `.be2-work/ai-authoring-milestone/evidence.json` and the final delivery message.
This directory is local evidence and excluded from source publication. `dist/ship.json` records
the engine revision actually packaged. An old report is not current proof: regenerate it.
The original untracked `docs/LINUX_HEADLESS.md` was preserved and is outside this milestone.

| Layer | Command or proof |
| --- | --- |
| Discovery | `python3 tools/be2.py context game_shipping --compact`; `python3 tools/author.py describe` |
| Package regressions | `python3 -m unittest tools.test_game_ship tools.test_game_check tools.test_author.NativeDiscoveryTests` |
| Headless rules/saves | `cargo test --locked --manifest-path games/signal-garden/Cargo.toml --no-default-features` |
| Gameplay evidence | Seeds 1/7/42 won in 5,867/9,860/12,122 ticks; original routes still pass after the shield |
| Audio bytes | `GARDEN_AUDIO_OUT=DIR cargo test ... --no-default-features --test audio`; `tools/audio_report.py DIR/*.wav --json` |
| Graphics | `tools/xcapture.py`; actual world, settings, shield/save/load and outcome frames inspected |
| Package | `scripts/blue ship`; independent manifest/hash check with both display variables unset |
| Isolated executable | Declared-file copy outside source/dist; exact solver input replay with `--expect won` |
| Game final gate | `python3 scripts/check.py`; includes package/identity/shortcut gate |
| Engine final gate | `python3 tools/be2.py check --changed --plan`, then `check --changed`; default/headless tests, Clippy, rustdoc, Python contracts and game tests |
| Publication | `python3 scripts/publish_games.py check`; explicit source-only entries and normal publish workflow |
| Upgrade | `python3 tools/be2.py upgrade plan games/signal-garden --to REV`; fresh project verification, never reuse earlier reports |

The Linux release is about 3.8 MB plus icon/README. Package smoke captured two 1280×720
frames. A small-window run captured the shield and successful save/load at 640×480.
Audio backend evidence loaded 18 effect variants and one music stem with zero failures;
ALSA null output was used, so this proves submission rather than speaker audibility.
The numerical WAV report checks clipping/DC/edges and flags possible transients; it does
not judge musical quality. Updated synthesis results are retained with the final evidence.

A fresh headless library build in an empty target directory took about 322 seconds with
Cargo downloads cached and other verification running. Initial release build took about
43 seconds using existing dependency artifacts. Game-only iterative test/client rebuilds
were roughly 2–6 seconds before heavy concurrent work; no-op builds are separately recorded.
These are approximate local wall times, not comparable controlled benchmarks. Full-resolution
software rendering is slow; inspect a few normal-size frames and replay long routes at a
small resolution. Perf's fixed unattended dt is simulation timing; its measured drawing
work is separate and does not establish a hardware frame-rate guarantee.

## Limits and next improvements

Windows CI remains intact, now also testing the game; Windows/macOS runtime behavior was not
verified locally. Linux shortcut contents and uniqueness were verified; the Windows-only
shell launch/resource checks explicitly skip on Linux. OpenGL/X11, ALSA and libudev remain
system runtime dependencies. A manifest is not a signed trust boundary. Isolated smoke proves
only exercised reads, and does not forbid hardcoded absolute paths into a still-existing
checkout. No public release or remote machine installation is claimed by a local package.

The next three high-value improvements are:

1. Enforce file-access isolation and audit system libraries in package testing, so absolute
   source reads and unexercised dependencies cannot hide behind a successful smoke.
2. Make refreshing generated game tooling a versioned, executable upgrade operation;
   the existing planner is useful, but copying newer ship/check scripts is still manual.
3. Run regular independent small-model creation/modification trials with player input,
   audio listening and Windows/Linux packaged runs; reduce repeated client glue when those
   trials show a concrete common pattern.
