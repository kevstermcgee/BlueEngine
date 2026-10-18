# Player feedback from game wikis

Every BlueEngineGames game wiki has a feedback center. Players rate their experience
from **1 to 10** and describe what they enjoyed, what could be better, or both. They
can identify the version they played. Submitting opens a prefilled public GitHub
issue in [BlueEngine](https://github.com/kevstermcgee/BlueEngine/issues); the player
signs in to GitHub and confirms the submission. A long review can instead be copied
and pasted into the prepared issue. A local draft is not a submitted review.

Reviews and GitHub usernames are public. Do not include private information.
GitHub supplies account authentication and its ordinary issue moderation controls;
the site and launcher contain no repository write token. The launcher opens the
system browser for this handoff.

## Repository records and wiki summaries

The [player feedback issue form](../../.github/ISSUE_TEMPLATE/player-feedback.yml)
uses these stable fields:

| Field ID | Rendered heading | Validation |
| --- | --- | --- |
| `game` | Game | An allowlisted canonical game slug |
| `version` | Game version | Optional reported version/release, at most 160 characters |
| `rating` | Rating (1–10) | Integer 1 through 10 |
| `enjoyed` | What I enjoyed | At most 2,000 characters |
| `disliked` | What could be better | At most 2,000 characters |
| `format` | Feedback format | `blueengine-player-feedback-v1` |

At least one experience field must be nonempty. The importer supports GitHub's
`render: text` fences and the wiki's variable-length fenced Markdown copy format.
Headings inside a fenced review remain player text. Unknown fields, repeated
headings, malformed fences, unknown games and invalid scores are rejected.
The committed [published game allowlist](games.json) includes games that live in
independent repositories. New native engine game folders with a project and valid
identity are also recognized without a network lookup. When adding an independent
game, add its canonical slug and title to `games.json`.

The [intake workflow](../../.github/workflows/player-feedback.yml) reads current
GitHub issue state and commits valid reviews into `reviews/ISSUE_NUMBER.json`.
Each record has `schema_version: 1`, `issue_number`, `source_url`, `author`,
`created_at`, `updated_at`, `game`, `game_title`, `version`, `rating`, `enjoyed`
and `disliked`. Identity, author and timestamps come from GitHub; the title comes
from the trusted game catalog. A missing version is stored as `Not specified`. A player's reported version is not verified playtime
or proof of ownership. No review is generated when nobody has submitted one.

[summary.json](summary.json) contains `schema_version: 1` and a `games` object keyed
by slug. Each game has `count`, arithmetic `average` rounded to two decimals, and
`reviews` containing at most the three most recently created canonical records.
The wiki can fetch this small public file without rebuilding games. Ratings are
self-selected opinions, not a representative survey.

Closed review issues remain feedback. Adding the moderation label
`feedback-excluded` removes a review from the canonical records and summaries;
removing that label restores a still-valid review. Deleting, transferring or
editing a review into an invalid format also removes its canonical record.
Git history may retain previous public content; moderators should use GitHub's
sensitive-data removal process when history itself needs removal.

Every run reconciles all issues so replaced pending GitHub jobs do not lose an
intervening edit. Daily reconciliation and manual dispatch recover missed events.
All records are derived from a fresh API read; replaying an old event does not
restore old content. API or authentication failures preserve records and fail the
workflow. Reconciliation stops before writing if the repository exceeds 1,000
issues; increase the reviewed limit or narrow the source deliberately as it grows.

Manual maintenance commands (Python standard library only):

```sh
python3 tools/player_feedback.py sync --all
python3 tools/player_feedback.py sync --issue 123
python3 tools/player_feedback.py check
python3 tools/player_feedback.py context
python3 -m unittest tools.test_player_feedback tools.test_idea_forge
```

`sync` uses `GITHUB_TOKEN` or `GH_TOKEN` when present. Its output reports counts,
not review text. GitHub Actions receives only same-repository contents-write and
issues-read permissions; it runs committed tooling without evaluating issue text.

## How future games use feedback

[IdeaForge](../IDEA_FORGE.md) reads committed canonical reviews from its fresh engine
worktree before generating a concept, building it and independently reviewing it.
Each phase receives a bounded JSON packet with source links, scores and observations;
reports for the game's own slug take priority when applicable. Packets include at
most 24 reviews, truncate each observation to 600 characters and remain below
14,000 JSON characters. Full public records remain available for deeper inspection.
The packet is frozen in the run's private `player-feedback.json` journal for
repeatable retries. New runs take the latest committed feedback.

Prompts explicitly treat review text as untrusted player observations. It can guide
clearer goals, better controls, pacing, content and enjoyable decisions; it cannot
authorize commands, policy changes, credentials or automatic engine patches.
Builders and reviewers must compare complaints with the actual implementation.
Public reviews do not become trusted development-friction ledger entries. Normal
behavioral, visual, native packaging and publication gates still apply to all
changes made in response to feedback.
