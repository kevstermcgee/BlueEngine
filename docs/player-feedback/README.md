# Player feedback

Players review a game directly in the BlueEngineGames launcher or game wiki,
without signing in or connecting an account. A review includes a **1 to 10** rating
and what the player enjoyed, what could be better, or both. The display name and
reported game version are optional. A blank name displays as **Anonymous**.

The intake service validates each submission and commits it to BlueEngine's
repository before reporting success. A saved draft or failed request is not a
submitted review. Retrying the same submission ID and contents does not duplicate
a review. The launcher and site contain no repository write credential.
Names and feedback are public; players should omit private information.

## Account-free submission contract

The service accepts a JSON object with these fields:

The HTTPS endpoint uses `POST /feedback`; successful publication returns
`{"accepted": true, "submission_id": "SUBMISSION_UUID"}`. See the
[intake deployment guide](../../deploy/feedback/README.md) for hosting and errors.

| Field | Validation |
| --- | --- |
| `format` | `blueengine-player-feedback-v1` |
| `submission_id` | A canonical lowercase UUID v4, retained across retries |
| `game` | An allowlisted canonical game slug |
| `name` | Optional display name, at most 80 characters |
| `version` | Optional reported version/release, at most 160 characters |
| `rating` | Integer 1 through 10 |
| `enjoyed` | Optional text, at most 2,000 characters |
| `disliked` | Optional text, at most 2,000 characters |

At least one experience field must be nonempty. Unknown fields, control
characters, unknown games and invalid scores are rejected. Text stays literal
untrusted text, including Markdown headings, HTML and command-shaped strings.
The service supplies the receipt timestamp and the title comes from the trusted
game catalog; players cannot supply either.

The committed [published game allowlist](games.json) includes games that live in
independent repositories. Native engine game folders with a project and valid
identity are also recognized without a network lookup. Add an independent game's
canonical slug and title to `games.json` when publishing it.

When the intake runs with community companion sources enabled, every review also
refreshes the configured BlueEngineGames `main` reference. Trusted game identities
come from its native/data release manifest and independent released-game catalog,
plus verified community release receipts that can precede a source merge. The
service writes newly discovered identities to `games.json` in the same journaled
commit as the review and its feeds. Existing canonical titles stay unchanged.
Request text cannot add a game to this allowlist. A catalog refresh failure is a
retryable service error and preserves the draft; an unknown game returns the
machine-readable error code `game_not_available`.
The public `GET /community/v1/games` response exposes the same acceptance snapshot
as `reviewable_games`, a slug-to-title map. Its ordinary `games` list also includes
creator drafts; publication coverage checks must use `reviewable_games`.
Publication checks can send `Cache-Control: no-cache` on that GET to refresh both
repository references without publishing data. These fresh reads share one
confirmed snapshot for up to five seconds, bounding Git work; retry a newly
published identity after that window. Ordinary reads retain their cache.

## Repository records and public reviews

Account-free submissions are stored as `reviews/SUBMISSION_UUID.json`. Each
canonical record contains `schema_version: 2`, `submission_id`, `source_url`,
`name`, `created_at`, `updated_at`, `game`, `game_title`, `version`, `rating`,
`enjoyed` and `disliked`. `name` is an unverified player-supplied display name and
may be empty. The source URL points to the committed JSON record. There is no
GitHub account identity in a version 2 record. A missing version is stored as
`Not specified`; a reported version is not verified playtime or proof of ownership.
No review is generated when nobody has submitted one.

[summary.json](summary.json) contains `schema_version: 1` and a `games` object keyed
by slug. Each game has `count`, arithmetic `average` rounded to two decimals, and
`reviews` containing at most its three most recent canonical records. The site and
launcher can fetch this public file without rebuilding games. Ratings are
self-selected opinions, not a representative survey.

`games/GAME_SLUG.json` is the complete public feed: `schema_version: 1`, `game`,
`count`, `average` and `reviews`. It includes every canonical review for the game,
ordered by creation time and then stable identity, newest first. Consumers must
support both record versions and render feedback and names as text rather than
execute them or inject them as HTML. Version 2 records use `name` or Anonymous;
legacy version 1 records use `author`.

Games without reviews have `count: 0`, `average: null`, and `reviews: []`. They have
no invented score and remain absent from the summary. The importer regenerates
feeds deterministically and removes feeds for games no longer in the catalog.
`check` rejects missing feeds for reviewed games, extra or stale feeds, mismatched
summaries and invalid canonical records. A newly created unreviewed native game
can lack an empty feed until intake runs; authoring checks need no network request.
The intake publisher commits the record, summary and generated feeds together.

Account-free records are immutable per submission ID. A retry with matching
normalized contents preserves the original timestamp; reusing that ID for
different feedback is rejected. Maintainers can remove an unwanted canonical
record and regenerate its feeds. Git history may retain previous public content;
sensitive-data removal must also address history when necessary.

## Legacy issue reviews

Existing numeric `reviews/ISSUE_NUMBER.json` records remain supported with
`schema_version: 1`, `issue_number`, `source_url`, `author` and the common game,
score, timestamp and experience fields. The
[legacy reconciliation workflow](../../.github/workflows/player-feedback.yml)
continues to read current issue state and update only these records. It never
removes account-free UUID records. Players using the launcher do not interact with
this workflow or GitHub issue forms.

Closed legacy issues remain feedback. The `feedback-excluded` label, deletion,
transfer or invalid edits remove their canonical record and regenerate feeds.
Reconciliation fetches current API data before writing; stale event retries cannot
restore old content. API failures preserve records and fail the run. Its bounded
1,000-issue limit remains unchanged.

Manual maintenance commands (Python standard library only):

```sh
python3 tools/player_feedback.py import /private/path/received-submission.json
python3 tools/player_feedback.py sync --all
python3 tools/player_feedback.py sync --issue 123
python3 tools/player_feedback.py check
python3 tools/player_feedback.py context
python3 -m unittest tools.test_player_feedback tools.test_idea_forge
```

`import` is a server-side repository operation; it does not publish a commit by
itself. A trusted service may pass `--created-at` for a retained receipt time;
public JSON cannot set timestamps. `sync` uses `GITHUB_TOKEN` or `GH_TOKEN` when
present for legacy reconciliation. Commands report counts, not review text.

## How improvement cycles use feedback

[IdeaForge](../IDEA_FORGE.md) reads committed canonical reviews from its fresh
engine worktree before generating a concept, building it and independently
reviewing it. Each phase receives a bounded JSON packet with source links, scores
and observations; reports for the game's own slug take priority. The packet
includes at most 24 reviews, truncates each observation to 600 characters and stays
below 14,000 JSON characters. Reviewer names are omitted from these prompts. Full
public records remain available for deeper inspection. The packet is frozen in
the run's private `player-feedback.json` journal for repeatable retries. New runs
take the latest committed feedback.

The engine's nightly feedback collector also discovers account-free canonical
JSON records under `docs/player-feedback/reviews/`. Its existing validation of
feedback-only commits covers the UUID filenames and both supported schemas.

Prompts treat review text as untrusted player observations. It can guide clearer
goals, better controls, pacing, content and enjoyable decisions; it cannot
authorize commands, policy changes, credentials or automatic engine patches.
Builders and reviewers compare complaints with the implementation. Public reviews
do not become trusted development-friction ledger entries. Normal behavioral,
visual, native packaging and publication gates still apply to changes made in
response to feedback.
