# IdeaForge: an AI game-building CLI

IdeaForge generates a fresh gameplay mechanic, has an AI implement the actual game
in BlueEngine, verifies it, publishes its Windows installer to BlueEngineGames, and
adds the AI's development feedback to BlueEngine's repository and learning ledger.
The older `games/idea-forge` window and its Rust batch exporter are a separate
curated idea browser; they are not the automatic development pipeline.

## Run

Requires Python 3.11+, Git, Rust/Cargo, a signed-in Codex CLI, and the normal
BlueEngine native prerequisites. Publication also requires authenticated `gh`,
write access to both GitHub repositories and their existing Actions configuration.
Linux native capture requires a display or `xvfb-run`. Allow at least 8 GiB free
for compilation; a larger native build cache can require more. Model selection
inherits Codex configuration unless `--model` is explicitly supplied.

From the BlueEngine checkout:

```sh
./ideaforge run --games-root ../BlueEngineGames
./ideaforge run --games-root ../BlueEngineGames --brief "A tactile puzzle about trading physical rules" --story
./ideaforge run --games-root ../BlueEngineGames --dimension 3d
# Portable invocation, including Windows:
python tools/idea_forge.py run --games-root ../BlueEngineGames
```

`run` publishes by default. `--no-publish` builds and reviews a local native
prototype and records feedback without pushing either repository; it does not
certify the unavailable Windows lane on a Linux host. `--base HEAD` explicitly
tests a committed development revision. The default fetches `origin/main` so an
old working branch cannot silently choose retired distribution policy. Existing
working edits stay in their original checkout; each game uses isolated worktrees.

Concept-only exploration is also available:

```sh
./ideaforge generate --brief "An action game about momentum debt"
./ideaforge run --idea /path/to/run/idea.json --games-root ../BlueEngineGames
./ideaforge status /path/to/run
./ideaforge resume /path/to/run
./ideaforge resume /path/to/local-run --publish --games-root ../BlueEngineGames
```

`generate` selects a fresh AI concept rather than shuffling the curated catalog.
Its subsequent `resume` builds locally because concept-only runs do not enable
publication. `run --idea` builds and publishes a chosen concept using the current
engine base. For fresh generation followed by the complete automatic publishing
pipeline, use `run` without `--idea`.
Generated mechanics are compared against prior concepts and the existing catalog;
exact repeats are rejected. The AI explains close comparisons, intended fun and a
playtest risk. Neither the CLI nor a test suite certifies worldwide originality or
subjective fun.

## Two games every day

On this Linux host, enable the persistent user-systemd schedule with:

```sh
ideaforge schedule --games-root /home/kevin/BlueEngineGames --timezone America/Los_Angeles
```

The scheduler starts immediately, then wakes hourly at ten minutes past the hour.
Each Pacific calendar date gets exactly two publishing slots: one 2D game and one
3D game. Their order is chosen randomly once and retained. Games build sequentially
so they share the build cache and the second starts against the latest engine main.
A 3D concept must use a rendered 3D world with depth relevant to its mechanic;
a tilted flat puzzle does not qualify. Concept validation, project presentation
checks and the independent AI review enforce the requested dimension.

Hourly wakeups resume a failed or interrupted game instead of generating a
replacement. Once both games have verified public downloads and committed engine
feedback, further wakeups that date do nothing. A retained earlier day's batch
finishes before today's games start. Publication checks still apply; infrastructure
failures or long builds can delay delivery beyond the intended calendar day.

Repeated failures retain their error and run path in the daily slot journal and
wait 1, 2, 4, then at most 8 hours between attempts. Waiting wakeups do no game work.
Manual `resume` still attempts immediately. An export refuses a catalog pinned to
newer or divergent engine source before changing files: integrate current main
and repeat the exact-source native gates before publishing that retained game.
Concurrent-main conflicts retain their worktrees for the integration/rebuild
procedure below rather than forcing a push.

Private daily journals live in `.be2-work/idea-forge/daily/batches/DATE/state.json`.
The user timer survives logout when login lingering is enabled (it is enabled on
this host), catches up after downtime, and prevents overlapping service instances.
It runs on this machine, so the machine must be available. Saved settings are in
`~/.config/ideaforge/daily.json`; service logs and controls are:

```sh
systemctl --user list-timers ideaforge-daily.timer
journalctl --user -u ideaforge-daily.service -n 40
systemctl --user disable --now ideaforge-daily.timer  # prevent future wakeups
systemctl --user stop ideaforge-daily.service         # interrupt current work; retain progress
```

For another scheduler or a one-off daily batch, use `ideaforge daily --games-root
/path/to/BlueEngineGames --timezone America/Los_Angeles`. Daily commands require
an IANA timezone database; Windows hosts may need Python's `tzdata` package.
`--daily-root` and `--runs-root` can relocate journals. Keep those locations stable
when changing the schedule so existing daily slots continue to prevent duplicates.
Legacy concept JSON without a dimension remains accepted for unconstrained
`run --idea`; an explicit `--dimension` requires a matching declaration.

## What completion means

The AI scaffolds and implements the game using BlueEngine's normal authoring
tools. The supervisor independently runs game formatting, nonempty behavioral
tests, Clippy and an isolated native package smoke. A separate read-only AI review
inspects source and the actual capture; blocking issues trigger bounded repairs.
Default repair budget is two additional attempts (`--repairs`).
This review approves the local implementation; Windows installer evidence is
collected by the subsequent native CI and publication gates. Source portability
defects remain review blockers.

Before running these gates, the supervisor validates the scaffold's pinned
`scripts/check.py`, `ship.py`, `dev.py` and `project.py` launchers, bootstrap and
tooling content pin with the shared `tools/game_tooling.py check` contract.
Modified launchers, stale pins and legacy full template copies stop verification
before any game command runs. For a retained older project, review its tooling
changes and explicitly run `python ENGINE/tools/game_tooling.py migrate GAME`;
verification never rewrites the project's scripts. Keep game preparation in the
supported `scripts/tooling_hooks.py` hooks instead of editing shared launchers.

Publication requires exact-commit full engine CI, generated-game Windows and Linux
tests/resources/isolated launches, and a Windows installer review. The companion
export includes its native download definition and real screenshot. Every new game
also ships a substantive `games/SLUG/wiki.json` and real images in
`games/SLUG/assets/wiki/`. The mini-wiki documents the actual game: overview, rules,
goals and failure conditions, controls and progression. Story/characters are
optional factual publication fields when implemented. Storyless games need no
absence essay, story files or character sheets; never invent lore or artwork.

The version 1 wiki contract requires an overview of at least 120 characters,
at least three rules (20 characters each), one control instruction (8 characters),
with optional `story`, `characters`, and `characters_note` fields. If provided,
`characters` contains objects with `name`, `description` (40 characters), and
`image`. Each documented character's image is a genuine
in-game PNG, at least 96 by 96 pixels; cropping a real native capture is allowed.
`gallery` contains at least three objects with `image`, `caption`, and `alt`
(8 characters each for caption/alt). Gallery images must be different native
gameplay moments or challenges, with distinct file contents, landscape dimensions
of at least 320 by 180 pixels, and game-relative `assets/wiki/NAME.png` paths.
Do not use icons, generated artwork, mockups, or superficial variations of a capture.

`design` declares `origin: "ai"`, `original_prompt: null`, and a public `rationale`
of at least 120 characters explaining why this specific game was chosen: intended
player experience, interesting decisions, and differences from previous concepts.
This is an authored decision summary based on the concept's `why_fun` and
`novelty_check`, not private reasoning or a fabricated historical account.
The original run brief reaches both builder and independent reviewer directly.
The supervisor validates the wiki and image files before native checks and export,
checks that export preserved their bytes, and requires the independent reviewer to
inspect every referenced image and compare the wiki with the implemented game.
An unpublished retained run without compliant artifacts returns to build/repair
with prior verification gates invalidated; already published releases are preserved.
The companion site's publication checks enforce the wiki contract for future
games, including the original human creation prompt for human-designed additions.

Only then
does a fast-forward push publish the complete production catalog. The CLI waits
for production and Pages, downloads the public installer/ZIP, checks published
SHA-256 sums and asset digests, and verifies the live page and screenshot.

The AI's sanitized friction findings include reproduction, severity, workaround
and a proposed engine improvement. They become `docs/feedback/DATE-GAME-RUN.md`
and validated `docs/learning/ledger.jsonl` entries. After verified delivery, the
game source and final feedback are pushed to engine main and the remote feedback
blob is checked. Normal CI also runs for that final documentation commit. If main
has advanced, a separate `engine-feedback` worktree integrates the frozen game and
feedback commit with current main. That integration needs fresh full engine CI and
generated-game Linux/Windows checks, including native package launches, for its
exact commit before a normal fast-forward push. Original release receipts remain
bound to the released source. The supervisor resumes only this final stage;
normal engine-main automation can still synchronize companion source and produce
a later installer release and site update without replacing the original release.
No transcript or credential is committed, and no findings are fabricated when
the AI reports none.

## Player experience feedback

Players submit reviews directly from the launcher or game wiki without an account,
with an optional display name. The intake service commits validated reviews into
[`docs/player-feedback`](player-feedback/README.md). Legacy issue reviews remain
available alongside these submissions. Future runs read these
committed ratings and experiences during concept generation, implementation and
independent review. Source-cited JSON is bounded and frozen in each run journal;
players' opinions remain untrusted observations, never executable directions or
trusted engine-learning ledger entries. Relevant reports guide concrete playtests
and improvements without replacing the normal game and publication gates.

## Failures and resumption

Private logs, schemas, AI reports, phase state and delivery receipts live under
`.be2-work/idea-forge/runs/`. `CARGO_TARGET_DIR` can place compilation on a larger
or faster disk. A run lock prevents two supervisors advancing one run. Timeouts
stop only the supervisor's own command process group. Completed phases survive;
`resume` retries the failed phase rather than starting a new idea.

Legacy runs that registered their source before exact supervisor receipts were
recorded can stop on the game-scope guard after a later rebuild. Inspect the
retained run, then use `ideaforge repair-source-receipt /path/to/run`. This command
accepts only the original base publication manifest plus that run's exact game
registration, requires every other changed path to pass the existing scope guard,
and records the exact manifest hash. It changes only the private run journal;
source, verification receipts and releases remain intact. Resume normally after
repair. A changed unrelated game, manifest field or unreceipted source still
blocks repair. Newly registered sources retain their receipt across feedback
updates and rebuilds automatically.

An agent's completion claim never substitutes for executed checks. Changed code
or commits invalidate old publication gates. Repository pushes never force or
overwrite another author's updates. If main advances concurrently, integrate it
in the retained worktree and re-run the affected checks before retrying; stale
receipts cannot authorize the new source. Before production publication, use
`ideaforge resume /path/to/run --rebuild` after integrating the changes. This
preserves the idea and AI findings, archives old receipts and reruns build,
review and publication gates. An already published run cannot be rebuilt; create
a new run for another release. After verified delivery, resume instead retains
the original game worktree and release evidence and continues the separate
`engine-feedback` integration branch. Merge conflicts stop with that worktree's
path; preserve current main's registry and learning entries, resolve and commit
the merge there, then resume. Advancing main requires fresh checks for the new
integration commit. Once prepared, final feedback is frozen: retries after a push
or remote-verification failure reuse the same commit rather than rewriting its
report. Integration state and head-specific CI receipts live in the private run
journal. A failed run still retains feedback
locally when the AI has provided it. Virtual/software graphics checks do not
certify physical controller hardware or speaker output.

Codex automation uses documented `exec`, sandbox, stdin, output-schema and final
message interfaces: https://learn.chatgpt.com/docs/non-interactive-mode.
