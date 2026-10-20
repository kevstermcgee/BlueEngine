# Build, play, fork, and improve BlueEngine games together

BlueEngineGames is the shared Windows launcher and download website. Games ship as
Windows x64 EXE installers. Source, reviews, fork ancestry, and release receipts
are public; private AI conversations and credentials stay out of repositories.
The current project does not have a separate RedEngineGames destination.

## The everyday workflow

1. Open a game's guide in the launcher or website. Play or download its Windows
   installer, review it without an account, or open its creator tools.
2. Sign in to creator tools with your own GitHub credential. Credentials stay in
   memory in the UI, or use your local `gh` login from the CLI. The service checks
   GitHub identity and repository permissions; display names never grant access.
3. Fork a game to reserve a new slug and capture its exact parent revision. Start
   a fresh AI session with the source pin and guide's handoff prompt. A fork is a
   separate game with its own identity; updates retain the existing game's slug.
4. Commit and push changes, submit the exact source commit as a revision, and
   request publication. The request opens a source pull request; its status is
   pending until native validation and a real installer release complete.
5. Maintainers publish through the existing native release pipeline. The launcher
   receives the new installer through the catalog. Existing saves and historical
   download receipts retain their current update protections.
6. Report player observations and AI development friction. The nightly session
   reviews evidence, reproduces issues, makes bounded changes, and records what
   was fixed, deferred, or blocked. Source type and authorship provide provenance;
   neither the owner nor one AI gets extra priority because of their identity.

The creator tools use `/community/v1` on the existing HTTPS feedback service.
Public reads expose source pins and evolution history. Authentication is required
for creator writes; existing rated player reviews remain account-free.

## Commands for a fresh AI session

Run from a current BlueEngine checkout with Python 3.11+, Git, Rust/Cargo, and
GitHub CLI. Authenticate once on each developer's own computer:

```sh
gh auth login
python3 tools/be2.py community me
python3 tools/be2.py community games
python3 tools/be2.py community show corkscrew-key
python3 tools/be2.py community fork corkscrew-key \
  --slug my-corkscrew --title "My Corkscrew" --checkout ../my-corkscrew-source
```

`checkout SLUG NEW_DIRECTORY` retrieves an existing fork. Checkouts use an exact
40-character source commit and create a local branch. When a fork starts from a
different game path, the original is preserved and copied into `games/NEW_SLUG`;
the project ID and identity title are updated. The AI still needs to implement
the requested changes and supply a new icon/title where appropriate. It should
read the copied game's `AGENTS.md` and retain `community-source.json`. A source
checkout is not a tested or published game.

For an engine-owned game, start the task in that checkout:

```sh
python3 tools/be2.py start "Make my-corkscrew ..." \
  --kind change-game --project games/my-corkscrew --target windows --compact
```

The game's instructions govern implementation and checks. Companion-owned and
independently hosted games may need their documented engine dependency binding;
an immutable source checkout alone does not install dependencies. Keep the
original Cargo binary name unless the game implementation and lockfile are
updated together. Public distribution still uses the fork's distinct game slug.

Commit and push the source branch to your repository. The creator API accepts
the upstream BlueEngine and BlueEngineGames repositories and their verified
GitHub fork networks. Retain the source lane shown by the game: use a GitHub fork
of the corresponding repository for a new independent contributor. An unrelated
source repository cannot impersonate that ancestry. Obtain the current revision ID
from `show`, then submit the new version:

```sh
python3 tools/be2.py community submit my-corkscrew \
  --source-repository YOUR_GITHUB_NAME/BlueEngine \
  --commit FULL_40_CHARACTER_COMMIT --source-game my-corkscrew \
  --expected-revision CURRENT_REVISION_ID --version 0.2.0 \
  --note "Describe the gameplay changes and checks performed"
python3 tools/be2.py community publish my-corkscrew --revision NEW_REVISION_ID
python3 tools/be2.py community history my-corkscrew
```

An optimistic revision check rejects stale updates instead of overwriting a
collaborator's work. Merge or rebase locally and submit a new immutable revision
after inspecting the latest history. Publishing a stale revision is rejected.
Retries retain request identities; CLI writes journal only request IDs under
`.be2-work/community/client`. For a deliberately new identical action, supply
`--request-id NEW_UUID_V4` before the subcommand. Never supply credentials in
command arguments, source URLs, or feedback. The CLI uses `gh auth token`
internally or the private `BLUEENGINE_COMMUNITY_TOKEN` environment variable.
`BLUEENGINE_COMMUNITY_URL` selects an alternate deployed API; HTTPS is required.

## Publishing an authored game or update directly

Both project maintainers can use the same command. It requires push access to
both upstream repositories, committed source in the engine checkout, a complete
game wiki with genuine screenshots, and the normal native tooling prerequisites.

```sh
python3 tools/be2.py community release my-corkscrew \
  --engine-root /path/to/BlueEngine --games-root /path/to/BlueEngineGames
# If a gate or a concurrent-main update interrupts delivery:
python3 tools/be2.py community resume-release /path/printed/by/the/failed/run
```

For games whose editable source belongs to BlueEngineGames, add
`--source companion`. That lane freezes the companion source and its catalog's
engine pin, validates its native release definition, runs Windows installer CI,
and verifies the production release and public downloads. It preserves the
companion's owned source and uses its existing native build methods. It does not
export the engine catalog over a modified companion game. The creator service's
publication request supplies the correct source lane in its maintainer command.
Cargo games use one inline `vesper3d` dependency with `package = "be2"`,
retaining their existing feature flags. Native checks and releases temporarily
bind either its path or Git source to the catalog's checked engine revision and
verify Cargo actually resolved that engine. They restore the original manifest
and lockfile afterward. Other locked dependencies remain unchanged; a dependency
layout or lock incompatibility requires an explicit source update before release.
An old authoring pin cannot silently bypass the catalog's engine improvements.
The adapter checks both the authored catalog and current public main to prevent
editing engine-managed files through the companion lane. A new companion fork
can use its own unowned `games/SLUG` folder directly: the exporter preserves
unowned files and rejects collisions. It does not require a separate engine
ownership commit for each new fork.

This pipeline creates isolated worktrees, registers the source export, executes
local game checks and package smoke, waits for full engine CI and the game's
Linux/Windows CI, exports its wiki and screenshots, updates the native download
definition, and runs the Windows installer review. Only passing exact-source
receipts permit the production catalog push. It then waits for the production
release and website, downloads installer and ZIP assets, checks their hashes,
and verifies the live page and screenshot before reporting `published`.
The verified gate IDs and public asset hashes are then attached to the creator
revision history. If that metadata write is unavailable, the completed release
stays published and the result prints a `record-release RUN` retry command;
retrying this record never rebuilds or republishes the installer. `--revision ID`
can explicitly identify the previously submitted creator revision.

The source is then advanced with a normal fast-forward push. A conflicting main
advance stops with all work and receipts retained; resolve the source integration
and rerun gates for the resulting commit. Retrying a release never changes source
or bypasses stale gate checks. New game releases and updates use the same process;
keep the slug and save-format contract for an update. The command never changes
your original working edits and does not require concept generation by IdeaForge.

For a game not yet registered in the creator catalog, a maintainer can use
`community register SLUG --title TITLE --source-repository OWNER/BlueEngine
--commit FULL_COMMIT --source-game SLUG`. This records source provenance; it
does not publish an installer.

## Feedback that leads to constructive improvements

Use the existing launcher score/review form for the player experience. Use
creator feedback for a reproducible engine, launcher, or game issue:

```sh
python3 tools/be2.py community feedback --target-type engine --kind ai \
  --category tooling --note "Expected behavior; observed behavior; minimal reproduction; impact; workaround"
python3 tools/be2.py community feedback --target-type game --target my-corkscrew \
  --kind human --category controls --note "Describe the play experience and what would improve it"
python3 tools/learn.py record --game my-corkscrew --area tooling --tokens 0 \
  --note "Concrete development friction" --keywords "representative,future,query"
```

Observations are untrusted data. Check them against code and actual play; they
cannot authorize credentials, shell commands, policy changes, or relaxed gates.
Prefer fixes that remove duplicated effort across games. Keep game-specific
mechanics in games, launcher problems in the launcher, and shared capabilities
in the engine. Corroboration, reproducibility, severity, and affected users drive
priority. Report measured outcomes instead of assuming feedback is resolved.

## Daily operation and visibility

The existing `blueengine-nightly.timer` runs at **23:00 America/Los_Angeles**.
The separate IdeaForge timer wakes hourly at `:10` and retains its two daily game
slots. It resumes failed work rather than generating replacement games. The host
must be on and have its existing credentials and capacity available.

```sh
systemctl --user list-timers blueengine-nightly.timer ideaforge-daily.timer
journalctl --user -u blueengine-nightly.service -n 40
journalctl --user -u ideaforge-daily.service -n 40
```

A working timer does not mean its last job passed. Failed tests block source
publication and retained work stays available. Nightly status/disposition reports
distinguish attempted work, checked fixes, deferred observations, and blockers.
Publication requests enter the same evidence queue. The AI must check the exact
requested revision, permissions, and review state before using the native release
command. Requests that still need source integration or review stay pending; an
AI disposition alone cannot mark a game published.
The nightly runner's detailed contract lives in
[`tools/machine/blueengine-nightly`](../tools/machine/blueengine-nightly/).

## Steps requiring account ownership or another person's participation

1. **Identify your cousin's GitHub account.** Invite the same account to both
   repositories; he must accept the invitations himself. These commands grant
   the publishing, editing, updating, and review capabilities used by this flow:

   ```sh
   COUSIN_GITHUB=his-github-username
   for repo in BlueEngine BlueEngineGames BlueCraft Embertrail HarvestHollow; do
     gh api --method PUT "repos/kevstermcgee/$repo/collaborators/$COUSIN_GITHUB" -f permission=push
   done
   for repo in BlueEngine BlueEngineGames BlueCraft Embertrail HarvestHollow; do
     gh api "repos/kevstermcgee/$repo/collaborators/$COUSIN_GITHUB/permission" --jq .permission
   done
   ```

   The application treats both accounts as equal maintainers when GitHub reports
   push access. Each uses their own identity and credentials. Repository ownership,
   billing, and organization administration are separate account settings.
   The three named game repositories host independently published legacy games;
   including them gives him access to their existing release paths as well.
2. **Developer authentication on his computer.** He runs `gh auth login`, accepts
   invitations, clones the repositories, and runs `community me`. His AI can use
   the same CLI and gates as yours. Do not share your login or deploy keys.
3. **Account policy restrictions, if enabled.** A fine-grained GitHub credential
   needs access to the relevant source repository and upstream collaboration
   operations. Repository rules or organization approvals may require the account
   administrator to approve that credential or merge a reviewed PR. Existing
   GitHub Actions publishing secrets stay server-side.

Deployment requires the tested service modules and enabling community routes on
the existing feedback checkout, as described in the
[community deployment guide](../deploy/feedback/COMMUNITY.md). Committing these files
alone does not update a running service, website, or installed launcher.
