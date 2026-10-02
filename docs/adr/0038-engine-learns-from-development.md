# ADR 0038: The engine learns from the games built on it

Status: Accepted

## Context

Every game built on BlueEngine has paid for something the engine could have provided, and the engine learned
about it only when a person noticed and wrote it down. The evidence on this machine:

- Deadfall (`games/deadfall/docs/DEVELOPMENT_METRICS.md`): art was 67% of about 2.44M tokens; three redo passes
  cost 739k (30%); a weapon-model toolkit existed twice, about 1,500 duplicated lines, and shipping it first
  would have saved about 30% of the art tokens. `kit::shape` and `kit::lint` exist now,
  after the money was spent.
- `physics.rs` is byte-identical in tumble-maze, wobble-tower and clockwork-pinball; the dedicated-server main is
  about 65 lines identical modulo names in three games and rewritten in a fourth.
- Silent failures reached players: Windows-only dead keys (R, digits, Tab), a connect failure that read like a
  refusal, a game that gated input on `GameShell::playing()`.
- Nothing measured whether `be2.py context` finds the tool an agent needs. Spooky Kart's feedback records "dozens of
  tool calls" spent reading engine source for signatures the index never named.
- The raw material for measuring cost exists (`~/.claude/projects/*/*.jsonl` records token usage, tool calls, file
  paths and subagent flags) but it also contains private conversation text, tool output and real credentials.

## Decision

A small loop with four steps, implemented by `tools/learn.py` (Python 3.10+, standard library, no network, no AI
service) and the data in `docs/learning/`:

1. **Capture.** `learn.py record` appends one validated line to `docs/learning/ledger.jsonl` (game, area, tokens,
   note, workaround, copied paths, trap, status, ref, plus optional keywords, features and a one-line hint). It
   replaces the free-form retrospective as the thing to harvest. The ledger was seeded by reading the committed
   feedback documents (47 entries).
2. **Measure.** `learn.py sessions` (local only) aggregates where AI effort went; `learn.py dupes` finds identical
   and near-identical source across the games (normalised line shingles, comments and imports ignored, string and
   number literals and the project's own name masked, published copies and branch worktrees collapsed) and labels
   each cluster against the engine: `engine-has-equivalent: adopt` (a shared type name, or most of the identifiers,
   exist in `src/viewer` or the feature index), `no-equivalent: promote candidate`, or `engine-template-copy`;
   `learn.py eval` scores `be2.py context` on a task benchmark.
3. **Promote.** An item is promoted into the engine when any one of these holds:
   - the same code was copied into **two or more games and is 100 or more normalised lines**; or
   - one friction item cost **about 50k tokens or more** (measured or credibly estimated); or
   - a **bug class escaped to players** (a defect no engine test or capture could see).
   Below the thresholds the item stays in the ledger as `open`; recurring evidence moves it over. Promotion means:
   the engine API, an ADR when a contract changes, the documentation, the `FEATURES.json` entry, the ledger entry
   flipped to `promoted` with its `ref`, and a benchmark task if discovery was part of the failure.
4. **Verify discovery.** A promoted capability that `context` cannot surface is not promoted. The benchmark
   (`docs/learning/tasks.jsonl`) holds realistic prompts with the features and files they should reach; results
   are appended to `eval_runs.jsonl` (the first run is the honest baseline and is never rewritten) and
   `eval_floor.json` is the recall the tests enforce. Tasks are fixed before the index is changed; a fix must
   describe the capability better, not special-case a prompt.

### Retrieval, closed

- `be2.py context` appends a `learned` list when ledger entries match the task: at most five lines and 600
  characters; `solved:` (what the engine provides, with its commit or ADR), `trap:` (the silent failure) or
  `open:` (a known gap). Matching needs at least one curated keyword and two shared words in all (or one distinctive
  keyword for a one- or two-word query); a shared feature id alone never matches. No match, no key: the packet
  contract is unchanged. Measured cost when it was added: mean packet 1074 to 1103 tokens (+2.7%).
- `FEATURES.json` gains a top-level `modules` map: the first sentence of each indexed file's own module docs
  (`learn.py modules --write`). `context` puts the file whose summary or name matches the task first in
  `read_first` (the curated list is unchanged when nothing matches; a word only one or two files mention can select a file on its own), counts the summaries as searchable text of
  the feature that owns the file, and weights query words by rarity across the index. An umbrella feature such as
  `custom_simulation` no longer hides the module a task needs.

### Privacy rules (what may and may not be committed)

Session logs contain secrets. Therefore `learn.py sessions` extracts structure only (counts, token numbers, tool
names, file paths, timestamps, working directory, branch, sidechain flag), never message text, tool results,
thinking, command text or search patterns; session and agent ids are stored as 8-hex hashes; a path containing a
UUID is masked and a credential-shaped path becomes `<redacted-path>`. Output goes only to the git-ignored
`.learning/` folder (the command refuses to write elsewhere inside the repository), is leak-scanned before the
command returns (UUIDs, long hex or base64, credential prefixes, `name=value` secrets, the words token, password,
secret, `key=`) and is deleted if flagged. `tools/test_learn.py` plants fake secrets in message text, tool input,
tool output, cwd, branch and names of a synthetic log and asserts they appear nowhere in any output, error or
report.
Committed: the ledger (hand-written, secret-checked on `record`), the benchmark, eval rows, dupes results (paths,
identifier names, counts) and reports generated without the sessions section. Never committed: anything derived
from `~/.claude`, `.learning/`, or aggregates not reviewed by a person; the sessions section of the report is
written only to `.learning/REPORT.md`.

### Validation scope (ADR 0032)

`tools/learn.py`, `tools/test_learn.py` and the `docs/learning/` data files form a fifth independent Python scope in
`workflow.validation_plan`: `python -m unittest tools.test_learn` (about one second, synthetic fixtures, one smoke
test that skips when the game checkouts are absent). A `learn.py record` commit therefore does not need the full
engine gate; touching `FEATURES.json`, `workflow.py` or any other path still does, and `tools.test_learn` is part
of the full unittest list. The test re-scores the benchmark in process and fails if recall falls below the floor,
so index regressions are caught by the ordinary check without a subprocess per task. `learn.py eval` itself
appends to a tracked log, so it stays a deliberate manual step: run it after an index or ranking change and
before a release, with `--note` saying why.

### Migration entries (ADR 0032, 0034, 0035)

A promotion that existing games must act on (a changed convention, a trap that needs a rebuild) also gets an entry
in `tools/upgrade_migrations.json` with the commit that introduced it and source evidence a game can be grepped for,
as ADR 0035 did for the key table. The ledger `ref` names that commit or ADR so `learn.py report` and `be2.py
upgrade plan` tell the same story.

## Results when this was written

Benchmark: 39 realistic tasks written before any index change (41 now: two more check that the loop itself is
discoverable); recall at k=3 (features expected among the top three, expected paths anywhere in the packet),
packet size in tokens (characters / 4).

| Run | Feature recall | Path recall | Top-1 expected | Tasks fully met | Mean packet |
|---|---|---|---|---|---|
| Baseline (39 tasks) | 0.86 | 0.27 | 0.77 | 9 / 39 | 1074 |
| Ledger hint added (39) | 0.86 | 0.32 | 0.77 | 11 / 39 | 1103 |
| Final (41) | 1.00 | 0.84 | 0.92 | 33 / 41 | 1092 |

Three further sets of 12 tasks were written later (`tasks_heldout*.jsonl`) to check generalisation. Engine as of the
hint commit, then final (feature recall / path recall / tasks fully met): set 1 0.75 / 0.58 / 7 to 1.00 / 0.83 / 10;
set 2 0.82 / 0.17 / 2 to 1.00 / 0.92 / 11; set 3 0.75 / 0.25 / 3 to 0.92 / 0.83 / 10. Sets 1 and 2 informed the routing
design and set 3 informed one ranking weight, so they are partly in-sample; the honest summary is that file
routing roughly tripled the share of tasks whose packet names the right file, and that about one task in six
still misses (typically a module whose own docs do not use the words the task uses).

A first attempt added sixteen small feature records for the missed capabilities and scored 1.00 / 1.00 in sample,
but `tests/capabilities.rs` caps `be2-tools describe` (which lists every feature id) at 6,000 bytes and the records
took it to 6,171; thin records also outranked the umbrella features the tasks named. They were replaced by the
per-file routing above plus two records (`dev_tools`, `learning_loop`). The cap leaves about 60 bytes of headroom:
the next feature id that does not fit should make `describe` list ids more compactly rather than force the index to
stay coarse.

## Consequences

The ledger is only as good as the habit of recording; AGENTS.md asks for one `record` call per friction item at the
end of a game task. Keyword matching is a heuristic and will occasionally show an unrelated line (bounded to 600
characters). Dupes labels are review hints, not verdicts. Module summaries are derived from doc comments and
drift when those change; `learn.py modules --check` reports staleness. Session aggregates describe one machine and
one tool's log format. Token counts from the benchmark are a character approximation.

## Rejected or deferred

- **Summarising sessions with a model, or embeddings**: needs an AI service (excluded by `tools/README.md`) and
  sends private text out.
- **Committing session aggregates** even when they look clean: one reviewed number at a time may be copied into the
  ledger by a person; the files are not committed.
- **Automatic promotion** or writing engine code from the ledger: a person (or an agent under review) decides.
- **Running `learn.py eval` inside `be2.py check`**: it appends to a tracked file; the in-process floor test gives
  the same gate.
- **An agent environment** (`reset/step/observe/reward`, trajectory recording): recorded as an open ledger item;
  it needs its own ADR after a first real consumer (the external-inspiration study, section 8).
