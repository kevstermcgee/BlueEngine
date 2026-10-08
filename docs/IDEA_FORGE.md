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

## What completion means

The AI scaffolds and implements the game using BlueEngine's normal authoring
tools. The supervisor independently runs game formatting, nonempty behavioral
tests, Clippy and an isolated native package smoke. A separate read-only AI review
inspects source and the actual capture; blocking issues trigger bounded repairs.
Default repair budget is two additional attempts (`--repairs`).

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
receipts cannot authorize the new source. A failed run still retains feedback
locally when the AI has provided it. Virtual/software graphics checks do not
certify physical controller hardware or speaker output.

Codex automation uses documented `exec`, sandbox, stdin, output-schema and final
message interfaces: https://learn.chatgpt.com/docs/non-interactive-mode.
