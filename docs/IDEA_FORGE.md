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

Publication requires exact-commit full engine CI, generated-game Windows and Linux
tests/resources/isolated launches, and a Windows installer review. The companion
export includes its native download definition and real screenshot. Only then
does a fast-forward push publish the complete production catalog. The CLI waits
for production and Pages, downloads the public installer/ZIP, checks published
SHA-256 sums and asset digests, and verifies the live page and screenshot.

The AI's sanitized friction findings include reproduction, severity, workaround
and a proposed engine improvement. They become `docs/feedback/DATE-GAME-RUN.md`
and validated `docs/learning/ledger.jsonl` entries. After verified delivery, the
game source and final feedback are pushed to engine main and the remote feedback
blob is checked. Normal CI also runs for that final documentation commit.
No transcript or credential is committed, and no findings are fabricated when
the AI reports none.

## Failures and resumption

Private logs, schemas, AI reports, phase state and delivery receipts live under
`.be2-work/idea-forge/runs/`. `CARGO_TARGET_DIR` can place compilation on a larger
or faster disk. A run lock prevents two supervisors advancing one run. Timeouts
stop only the supervisor's own command process group. Completed phases survive;
`resume` retries the failed phase rather than starting a new idea.

An agent's completion claim never substitutes for executed checks. Changed code
or commits invalidate old publication gates. Repository pushes never force or
overwrite another author's updates. If main advances concurrently, integrate it
in the retained worktree and re-run the affected checks before retrying; stale
receipts cannot authorize the new source. Before production publication, use
`ideaforge resume /path/to/run --rebuild` after integrating the changes. This
preserves the idea and AI findings, archives old receipts and reruns build,
review and publication gates. An already published run cannot be rebuilt; create
a new run for another release. A failed run still retains feedback
locally when the AI has provided it. Virtual/software graphics checks do not
certify physical controller hardware or speaker output.

Codex automation uses documented `exec`, sandbox, stdin, output-schema and final
message interfaces: https://learn.chatgpt.com/docs/non-interactive-mode.

New concepts include a concise `creative_identity`: camera/input, visual medium,
palette/contrast, typography/interface metaphor, motion, sound/music/silence,
coherence across gameplay/menus/outcomes and differences from previous examples.
Generation compares prior briefs as well as mechanics. Build prompts use the brief
and link the authoritative presentation/audio contracts; review returns separate
identity and audio assessments against actual available evidence. This is a design
brief, not an enforced style preset or questionnaire. Old concepts without a brief
remain accepted; the builder chooses and records one in the game's README.
