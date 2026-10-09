# Learning from the games built on Blue

The engine improves from the games made on it only if friction is captured, measured and acted on. This folder is
the loop (policy: [ADR 0038](../adr/0038-engine-learns-from-development.md)). Everything is run with
`python3 tools/learn.py` (Python 3.10+, standard library only, no network, no AI service).

| File | What it is | Committed |
|---|---|---|
| `ledger.jsonl` | one line per friction item: what it cost, what was done instead, whether the engine fixed it | yes |
| `tasks.jsonl` | the discovery benchmark: realistic task prompts with the features and files `be2.py context` should surface | yes |
| `tasks_heldout.jsonl`, `tasks_heldout2.jsonl`, `tasks_heldout3.jsonl` | further task sets written after index changes, to check the fixes generalise (`learn.py eval --tasks FILE`) | yes |
| `eval_runs.jsonl` | append-only results of `learn.py eval`, one row per task per run plus one summary row per run | yes |
| `eval_floor.json` | recall floor that `tools/test_learn.py` enforces so discovery cannot silently get worse | yes |
| `dupes.json`, `dupes.md` | last `learn.py dupes --save`: copied code across the games (paths, names, counts only) | yes |
| `REPORT.md` | one page generated from all of the above | yes |
| `../../.learning/` | `learn.py sessions` output derived from private session logs | **never** (git-ignored) |

## The commands

```sh
python3 tools/learn.py record --game spooky-kart --area networking --tokens 50000 \
        --note "what happened" --workaround "what you did" --trap "the silent failure" \
        --duplicated "~/Game/src/a.rs,~/Game/src/b.rs" --keywords "lobby,ready,packet,loss" --status open
python3 tools/be2.py context "lobby ready state under packet loss" --compact # verify retrieval
python3 tools/learn.py dupes --save        # what did the games copy? (cross-checked against the engine)
python3 tools/learn.py eval                # does `be2.py context` find the right tool? (appends to eval_runs.jsonl)
python3 tools/learn.py report              # rewrite REPORT.md (and .learning/REPORT.md if local data exists)
python3 tools/learn.py modules --write     # refresh the per-file summaries in tools/FEATURES.json that context routes by
python3 tools/learn.py sessions            # where did the effort go? local only, writes .learning/
python3 tools/learn.py scan PATH...        # leak scan: counts of secret-like values in generated files
```

Every command has `--help`; `sessions`, `dupes`, `eval` and `record` take `--json`.

## Privacy rules (non-negotiable)

Claude Code session logs under `~/.claude/projects/` hold private conversation text, tool output and real secrets
(credentials get pasted into them). So:

- `learn.py sessions` extracts **structure only**: counts, token usage numbers, tool *names*, file *paths*,
  timestamps, working directory, branch and the subagent flag. It never stores, prints or copies message text,
  tool results, thinking, command text or search patterns. Session and agent ids are replaced by 8-hex hashes; a path
  that embeds a UUID has it masked (`<uuid>`) and a path that looks like a credential becomes `<redacted-path>`.
- Its output goes only to the git-ignored `.learning/` folder (it refuses to write elsewhere inside the repo) and is
  leak-scanned before the command returns; a flagged output is deleted and the command fails.
- Nothing derived from real logs is committed. The committed ledger is seeded from committed docs and from lessons
  an agent writes by hand. `tools/test_learn.py` plants fake secrets in message text, tool input and tool output of a
  synthetic log and asserts they appear nowhere in any output, error message or report.
- `learn.py record` rejects entries that contain UUIDs, long hex or base64 runs, well-known credential prefixes,
  `name=value` credentials or the words password, secret and bearer. Keep notes about the *problem*, never the
  environment it was found in.
- `learn.py scan PATH` reports counts only (use `--where` for the key path of a match, never the value).

## Ledger schema (`ledger.jsonl`)

One JSON object per line. `record` fills `id` and `date`.

| Field | Type | Meaning |
|---|---|---|
| `id` | `L-NNN` | assigned |
| `date` | `YYYY-MM-DD` | assigned (UTC) |
| `game` | text | `spooky-kart`, `deadfall`, `prop-hunt`, `physics-games`, `engine` ... |
| `area` | enum | `networking rendering geometry input audio physics ai tooling docs workflow platform assets save ui simulation process other` |
| `tokens` | int >= 0 | measured CLI total or author estimate; `0` = unmeasured/unallocated; see the run receipt |
| `wall_seconds` | finite number >= 0, optional | elapsed game-run wall time; IdeaForge puts the total on its first finding |
| `measurement_ref` | text, optional | path to the numeric per-game receipt |
| `note` | text, one line, under 400 chars | what happened |
| `workaround` | text, optional | what the agent did instead, or the engine feature that now solves it |
| `duplicated` | list of paths, optional | code the agent had to write or copy that other games will too |
| `trap` | text, optional | the silent failure to warn the next agent about (shown by `be2.py context`) |
| `status` | `open` / `promoted` / `wontfix` / `duplicate` | `promoted` means the engine now provides it |
| `duplicate_of` | learning ID, required for duplicate status | canonical L-NNN; original report metadata stays intact |
| `ref` | text, optional (required when promoted) | the commit, ADR or tracking note: `1e2d2bc`, `ADR 0036` |
| `keywords` | up to 20 lowercase words, required for hints | words a future task would use; omission warns that the record is archive-only |
| `features` | `FEATURES.json` ids, optional | features this relates to; drives `context` hints |

`be2.py context` reads the ledger and, when entries match the task (keyword overlap with the query and the selected
features), appends a short `learned` list to the packet: at most five lines and 600 characters, nothing when
nothing matches. A promoted entry becomes "solved: ... (ref)"; an entry with a `trap` becomes "trap: ...".

## How entries are harvested and promoted

1. **Record at the end of a game task.** When a task hit friction (read engine source for something that should have
   been discoverable, copied code from another game, hit a silent failure, spent a big share of tokens on one thing)
   run `learn.py record` once per item with `--keywords` from a representative future query (at least two useful
   terms). Verify that query with `be2.py context`; feature IDs alone do not enable hints. Archive-only records
   may omit terms but the command warns explicitly. This replaces the free-form retrospective as the thing to harvest.
2. **Harvest weekly** (or before an engine change): `learn.py dupes --save` finds code copied across games,
   `learn.py report` lists open items and top candidates, and `learn.py sessions` (local) shows which areas and files
   consumed the most effort.
3. **Promote** when the thresholds in ADR 0038 are met (copied in two or more games with 100+ lines, or one friction
   item above about 50k tokens, or a bug class that escaped to players): implement it in the engine, add its ADR and,
   if existing games must act, a migration entry (`tools/upgrade_migrations.json`, ADR 0032/0034/0035), then flip the
   ledger entry to `promoted` with `learn.py close --commit COMMIT` (use a Closes-Learning trailer or --entry; the command validates the file)
   and re-run `eval`.
4. **Check discovery.** A promoted capability that `be2.py context` cannot find is not promoted: add a task to
   `tasks.jsonl` that a real agent would have typed, run `learn.py eval`, and fix `tools/FEATURES.json`
   (keywords, summary, `read_first`, `public_api`) until the right feature and files surface. Tasks are fixed first;
   fixes must describe the capability better, not special-case a prompt.

## The eval

`tasks.jsonl`: one object per line with `id`, `prompt` (at most 100 characters, which is the limit `context` enforces),
`expect_features` (feature ids that should be among the top `k` matches), `expect_paths` (paths or path fragments that
should appear anywhere in the packet an agent reads) and `notes`.

`learn.py eval` runs the real `python3 tools/be2.py context PROMPT --compact --limit K` per task and scores
feature recall@k, path recall, whether the top result is expected, and packet size in tokens (characters / 4, an
approximation). Each run appends rows to `eval_runs.jsonl` with timestamp and engine commit, so progress is visible:
the first run is the honest baseline and is never overwritten. Use `--note` to label a run and `--set-floor` to
raise the regression floor after a real improvement. `tools/test_learn.py` re-scores all four committed task sets
(`tasks.jsonl`, `tasks_heldout.jsonl`, `tasks_heldout2.jsonl`, `tasks_heldout3.jsonl`) in process and protects both
aggregate feature/path recall and each previously retrieved expected feature/path. Routine verification is
read-only and never appends to `eval_runs.jsonl`. To establish a floor for a set, pass
`--tasks docs/learning/SET.jsonl --set-floor`; review the changes before accepting them.

These sets informed tuning and are development/regression fixtures, despite their historical heldout filenames.
Feature/path retrieval and packet size estimates do not establish actual token savings, faster development or
better game-completion rates. The report computes current fixture scores read-only and distinguishes them from
historical evaluation runs.

## IdeaForge measurements and engine fixes

Each run writes numeric Codex `turn.completed` usage and wall time to
`forge-runs.jsonl`. Input tokens include cached input once. Missing CLI usage is
explicitly unavailable; wall time remains measured. Feedback links this receipt;
only its first finding carries the whole-run token/wall total, so findings do not
multiply the cost. No per-finding attribution is inferred. Retries and reviews
are included, and the receipt is replaced by run ID on resume.

Game workers write only their game folder and feedback. Changes to engine src,
tests, manifests or templates refuse the run, including changes committed during
the run (the guard compares against its original base). Report an engine issue
instead. In a separate engine branch, add an integration regression for it, then
run `python3 tools/engine_fix.py --base BASE --test SUITE::exact_test`.
It requires that test to fail against the baseline code and pass with the fix,
and runs the full engine check. A game run cannot use that path to modify its
engine; resume with a new fixed engine baseline for a new measurement.

## Close fixes and merge repeated reports

After verifying an engine fix, put `Closes-Learning: L-106, L-112` in its commit
message. Run `python3 tools/learn.py close --commit COMMIT` to resolve those IDs to
the fixing commit. Historical commits can use repeatable `--entry L-NNN`.
The commit must be in this checkout's history. The command validates the ledger
and preserves it on malformed input or unknown IDs; commit the reconciliation.
Closure records verified work; the command cannot prove that a claimed fix works.

`python3 tools/learn.py merge L-118 --into L-106` retains the original observation,
game, date and ID as a `duplicate` alias. The canonical entry holds merged keywords;
duplicate hints no longer compete for a fresh agent's context. Close the canonical
entry once its fix is verified. Frequency counts include its aliases and distinguish
report occurrences from independent games. Evidence and the five most frequent open
topics are in the [hardening feedback audit](../hardening/feedback-audit.md).
