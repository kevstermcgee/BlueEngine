# Enable collaborative game creation on the feedback service

The creator API shares the existing HTTPS feedback service, publication clone,
checkout lock, and mutation lock. Account-free `POST /feedback` keeps its existing
contract. New routes live under `/community/v1`; writes require each caller's own
GitHub credential. No write credential is bundled in the launcher or website.

Deploy reviewed copies of these four files together in the service's code
directory, outside its writable source checkout:

```text
tools/feedback_server.py
tools/player_feedback.py
tools/community_store.py
tools/community_server.py
```

Append `--community --companion-sources` to the existing service invocation.
Retain its checkout, TLS certificate, key, loopback/proxy settings, and configured
repository-scoped publication identity. The existing writable path covers
`docs/community/events/` and the crash journals inside `.git`; a second clone or
write credential is unnecessary. Restart only this service after copying its
code. Preserve its state and clean clone; never use a developer worktree for
the publication checkout.

Existing sparse checkouts must also include `docs/community`. With the service
stopped, inspect its selection and extend it without replacing the original
player-feedback selection:

```sh
systemctl --user stop blueengine-feedback.service
git -C SERVICE_CLONE sparse-checkout list
git -C SERVICE_CLONE sparse-checkout add docs/community
git -C SERVICE_CLONE sparse-checkout list
systemctl --user start blueengine-feedback.service
```

A full checkout needs no sparse configuration change. Keep
`docs/player-feedback` selected in a sparse checkout. Without the community
selection, Git rejects the new event at staging and the API returns 503. Do not
work around this with `git add --sparse`: later synchronization must keep
committed community events visible to the catalog and request retry lookup.

The optional companion source cache fetches public history into
`refs/blueengine-community/companion-main` inside that clone. It never checks out
or executes the companion source. Preload a large first fetch with the service
stopped, using the service identity, then start it again:

```sh
git -C SERVICE_CLONE fetch --no-write-fetch-head --no-tags \
  https://github.com/kevstermcgee/BlueEngineGames.git \
  refs/heads/main:refs/blueengine-community/companion-main
```

The existing backend Git timeout bounds public requests. A cold full-history
fetch can exceed that timeout; operator preloading prevents repeated incomplete
first requests. Engine-owned games come from the engine clone, and preserved
companion sources fill the remaining editable games. An immutable archived binary
without editable source requires a deliberate source migration before forking it
into a buildable new game. Source archives are development downloads; playable
downloads remain Windows EXE installers.

For nginx or another reverse proxy, route `/community/` to the same backend as
`/feedback`. Preserve `Authorization`, `Origin`, and `Content-Type`; apply the
existing request limits and keep access logs disabled. The public URL uses the
external HTTPS port, not necessarily the backend listener's port. On Kevin's host
the public base is `https://blue-engine.duckdns.org/community/v1`; the process
listens internally on port 8443. Keep the original review routes accessible.

Default browser origins are the GitHub Pages origin and the standard Tauri local
origins. `--origin` can explicitly replace this list for another deployment;
never configure a wildcard or the opaque `null` origin. Native CLI requests can
omit Origin. CORS does not replace identity/permission checks.

Verify the deployed service before updating the website or launcher:

```sh
curl --fail https://blue-engine.duckdns.org/health
curl --fail https://blue-engine.duckdns.org/community/v1/games
python3 tools/community.py me
python3 tools/community.py show corkscrew-key
python3 tools/community_store.py check --root SERVICE_CLONE
```

Use read-only probes against production; integration tests create their own Git
remotes and HTTP servers. A valid response from `/health` verifies the anonymous
intake process; a successful `/community/v1/games` also verifies the creator
catalog. The signed launcher publishing workflow checks both endpoints.

Community events are immutable per request ID, Git-backed, and acknowledged only
after publication. Retries preserve the same receipt; stale revisions conflict
instead of overwriting another creator. Catalog reads are temporarily cached and
accepted creator writes invalidate the cache. Five verified workflow receipts
certify engine-owned releases; the companion native lane uses four matching
Linux/Windows game, installer, production, and Pages receipts. Both lanes verify the companion catalog's
exact engine pin, public release asset hashes, and the registered game source.

The existing nightly runner consumes community feedback and publication requests.
Install its reviewed `run.py` and siblings `companion.py` and `engine_games.py`,
plus its prompt and result schema together; preserve private configuration and feedback queue state. Keep the existing nightly timer. See the
[daily runner contract](../../tools/machine/blueengine-nightly/README.md) and
[collaborative workflow](../../docs/COLLABORATION.md) for developer commands and
the account invitation steps.
