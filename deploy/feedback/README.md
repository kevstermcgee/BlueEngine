# Account-free player feedback intake

`tools/feedback_server.py` accepts reviews from the games launcher and website without
an account, GitHub login, OAuth redirect or reviewer token. The reviewer can supply a
name or leave it blank. Reviews and names are public repository data. Server-only Git
credentials publish the validated records to the BlueEngine repository, where existing
improvement cycles read them as untrusted player observations.

These files are deployment templates. Adding them to the repository does not start a
service or establish a public endpoint. Deploy HTTPS intake before configuring the
launcher/site endpoint. The shared game hub's UDP port does not provide this service.

## Service and repository setup

Use Linux with Python 3.10+ and Git. No Python packages are required. Install reviewed
copies of `tools/feedback_server.py` and `tools/player_feedback.py` beneath
`/opt/blueengine-feedback/tools/`; keep this service code separate from the writable
publication clone so a feedback import cannot update the running service code.

Create a dedicated `blueengine-feedback` system account, then create
`/var/lib/blueengine-feedback` owned by it with mode `0700`. Clone
`git@github.com:kevstermcgee/BlueEngine.git` to
`/var/lib/blueengine-feedback/repository`, checking out `main`. The clone must be
initialized, clean, on the configured branch and have an `origin` remote. Worktrees
and shared development checkouts are refused. The service exclusively owns this
disposable clone and resets it to the fetched remote before each publication.

Provision a repository-scoped write deploy key or equivalent server-only Git
credential. Store it outside the source repository, readable only by the service
account. Pin the remote SSH host key using a verified `known_hosts` file. Configure
SSH specifically for this clone, with explicit identity and host verification:

```sh
sudo -u blueengine-feedback git -C /var/lib/blueengine-feedback/repository config core.sshCommand \
  'ssh -F /dev/null -i /var/lib/blueengine-feedback/auth/repository_ed25519 -o IdentitiesOnly=yes -o UserKnownHostsFile=/var/lib/blueengine-feedback/auth/known_hosts -o StrictHostKeyChecking=yes -o BatchMode=yes -o ConnectTimeout=10'
```

`-F /dev/null` bypasses unrelated user/system SSH configuration while the explicit
options retain private-key selection and strict pinned-host verification. Some
systemd user sandboxes reject included system SSH configuration with `Bad owner or
permissions` even when an interactive SSH command succeeds. Verify
`git ls-remote origin refs/heads/main` with the actual service account, environment
and sandbox settings, such as a diagnostic unit matching the installed service;
an interactive-shell check alone does not verify service access. An HTTPS remote
with an appropriately scoped non-interactive credential helper also works.
Never place credentials in the endpoint URL, launcher, website, payloads or commits.
Ensure branch protection permits this identity to publish validated feedback data.

Install `blueengine-feedback.service` into `/etc/systemd/system/`, then enable/start it
after the clone and credentials are ready. The unit binds only `127.0.0.1:8091`, writes
only under its state directory and uses no developer home directory. Exactly one
service process may own a publication clone; an advisory lock prevents a second one.
Stop the service before administratively editing or replacing that clone.

## HTTPS and public routing are required

Provision a stable DNS name and a valid TLS certificate, then adapt
`nginx.conf.example` to that hostname. The configuration proxies `/feedback` and
`/health` to the loopback service, disables access logs and applies an additional
per-client rate limit without persisting addresses. Preserve request `Origin` and
`Content-Type`; do not inject an Origin for native clients. Keep port 8091 private.
Apply equivalent privacy settings to any upstream CDN/load balancer logs.

Public intake must use HTTPS. A browser page served over HTTPS cannot submit to an
HTTP endpoint. If terminating TLS directly instead, pass `--cert CERT --key KEY`
and an explicit public `--host`; the CLI refuses non-loopback cleartext listening.
The nginx proxy remains the recommended deployment for public request limits.

If an existing user deployment renews a temporary router mapping through
`blueengine-feedback-portmap.service`, install
[`blueengine-feedback-portmap.timer.d/reliability.conf`](blueengine-feedback-portmap.timer.d/reliability.conf)
under `~/.config/systemd/user/blueengine-feedback-portmap.timer.d/`, then run
`systemctl --user daemon-reload` and restart that timer. This drop-in assumes the
existing service already restricts itself to the feedback HTTPS mapping; it does
not create or change a router rule by itself. Run the existing mapping service
once to restore an expired lease, then verify public HTTPS as described below.

The timer starts 30 seconds after each activation and renews five minutes after
each mapping attempt finishes, including failed attempts. An empty setting for
**any** monotonic timer directive clears **all** monotonic triggers, so resetting
`OnUnitActiveSec=` also removes an inherited `OnBootSec=`. A timer with only a
recurrence trigger can remain `active (elapsed)` after reboot and let the router
lease expire. `Persistent=true` does not recover missed monotonic activations.
Check `systemctl --user list-timers --all blueengine-feedback-portmap.timer` after
restarting the timer and after its first execution: both must show a future NEXT
activation. A healthy local receiver alone does not establish public reachability.

The service permits only browser Origin `https://kevstermcgee.github.io`; the path of
the GitHub Pages project is not part of an Origin. Native launcher requests may omit
Origin. The endpoint does not use cookies or authenticated browser sessions. CORS is
a browser policy, not an account or authentication requirement.

## Request and publication contract

Send `POST https://YOUR_DEPLOYED_HOST/feedback` with `Content-Type: application/json`,
a valid Content-Length and this JSON body:

```json
{
  "format": "blueengine-player-feedback-v1",
  "submission_id": "f9b5b420-2c40-4f5b-8c65-1253f8bb0c7d",
  "game": "pocket-breaker",
  "name": "",
  "version": "release-2026-10-19",
  "rating": 8,
  "enjoyed": "The movement feels good.",
  "disliked": "The opening puzzle needs clearer goals."
}
```

Generate a fresh lowercase UUID v4 for each new review; retain it with the draft for
all network retries. `format`, `submission_id`, `game` and `rating` are required.
Name is optional (80 characters); version is optional (160); enjoyed/disliked are
optional individually (2,000 each), with at least one nonblank response. Rating is an
integer from 1 to 10. Game slugs must be in the engine's current game catalog. Extra
fields, duplicate JSON keys, nonfinite numbers and control characters are rejected.

HTTP 200 with `{"accepted":true,"submission_id":"..."}` means the corresponding
record and generated public feeds were successfully pushed to `origin/main`, or the
identical review was fetched from that remote during a retry. The service never
acknowledges a local-only write. It commits only the review's UUID JSON, summary and
canonical per-game feeds, using a fixed server identity and `[skip ci]` for validated
data-only publication. Existing nightly improvement work consumes the repo data.

Publication is serialized. Concurrent upstream changes cause a bounded fetch/reapply
retry without force pushes; successful remote writes with a lost acknowledgement
are detected by the same submission ID. A durable journal inside `.git` records only
the submission ID, generated output paths and Git object IDs before import. The new
commit ID is journaled before moving HEAD. This lets startup recover a process kill
or reboot during import, local commit or push using only the service's own paths.
Reusing an ID for changed feedback returns 409 and does not overwrite the original.
New feedback needs a new ID.

Validation errors return 400; untrusted browser Origins return 403; oversized requests
return 413; incorrect content types return 415; rate limits return 429; publication
failure or busy intake returns 503. Failures carry `accepted:false`; 429/503 include
`Retry-After`. Retain the review draft and submission ID on network/5xx failure. A
slow Git operation can outlive a client's timeout; retrying the same ID is safe.

The service allows up to eight simultaneous connections and twenty review requests
per minute globally, with a 20,000-byte body limit, absolute header/body read
deadlines, bounded Git command timeouts and at most three push attempts. It keeps
rate-limit timestamps only in memory, stores no IP addresses and logs no payloads.
The systemd journal receives startup/configuration messages only; proxy access logs
must remain disabled as shown. Public review files contain the optional display
name and feedback, not client networking metadata.

## Verification and operation

Run `python3 -m unittest tools.test_feedback_server tools.test_player_feedback` before
installing. The suite uses temporary local Git remotes, never production submissions;
it proves HTTP validation, remote publication, idempotency, concurrent Git updates,
rejected push recovery, lost-acknowledgement handling and actual subprocess-crash
recovery before and after publication.

After deployment, fetch `GET https://YOUR_DEPLOYED_HOST/health`. A 200 JSON response
identifies the running service without adding fake reviews. This is a liveness check,
not proof that Git credentials can fetch or push. Verify allowed-origin OPTIONS and
the launcher's configured endpoint.

Before a real submission, exercise the actual HTTPS POST and service-side fetch with
a well-formed request whose game slug is absent from the current engine catalog.
Confirm that `feedback-diagnostic-invalid` is unknown before using this example:

```sh
curl --silent --show-error --include \
  -H 'Content-Type: application/json' \
  --data '{"format":"blueengine-player-feedback-v1","submission_id":"f9b5b420-2c40-4f5b-8c65-1253f8bb0c7d","game":"feedback-diagnostic-invalid","rating":1,"enjoyed":"Deployment connectivity check; unknown game must be rejected."}' \
  https://YOUR_DEPLOYED_HOST/feedback
```

Expect HTTP 400 with `{"accepted":false,"error":"Unknown game slug"}`. Unlike a
malformed-input rejection, this response occurs after the running service fetches
the remote, refreshes its dedicated clone and checks the current catalog. It creates
no review and attempts no push. A 503 instead indicates the backend is unavailable
even if `/health` returns 200; check remote access inside the service sandbox.
This probe verifies the read path, not write permission or successful publication.
A genuine user submission must receive accepted=true and appear in
`docs/player-feedback/reviews` on the remote before claiming publication works.
Do not submit fabricated reviews to the production repository as a health probe.

For community intake using `--companion-sources`, anonymous review publication
refreshes the trusted companion reference even if a recent public catalog read
populated its cache. Native/data identities in `.release-games.json`, independent
identities in `site/released-games.json`, and verified community release receipts
extend the canonical review allowlist automatically. The allowlist, new review,
summary and feeds share one publication journal and commit; failed pushes and
interrupted imports restore the previous allowlist with the other generated data.
The service reads only metadata from the configured companion `main` and never
runs companion code. Invalid trusted metadata returns 503. A well-formed game
missing after refresh returns 400 with `code: "game_not_available"`; malformed
review fields remain ordinary validation failures.

Back up the remote repository through normal repository retention. The local clone
is disposable, not an offline queue. A 503 leaves the client responsible for retaining
and retrying the draft. To recover a damaged checkout, stop intake, replace the clone
with a fresh clean `main` clone, restore server-side Git configuration and restart.

Journal recovery removes only recorded generated outputs and their recognized
atomic-write temporary files, and only when HEAD matches the recorded base or own
commit. An unrelated dirty path, unexpected HEAD or invalid journal instead refuses
startup without discarding the work; repeated restarts cannot repair this state.
Stop intake, verify that the path is the dedicated publication clone, and preserve
a diagnostic copy before operator recovery. A fresh clone as described above is the
simplest recovery. For in-place recovery, fetch `origin main`, reset the dedicated
clone to `FETCH_HEAD`, then inspect `git ls-files --others --exclude-standard` and
remove only individually confirmed generated review/feed JSON or atomic-write
temporary files beneath `docs/player-feedback`. Inspect and remove the stale
`.git/blueengine-feedback-inflight.json` only after preserving it and confirming the
clone is safe to restart. Never run a broad cleanup against a development checkout.
Confirm `git status --porcelain` is empty before restarting; clients can retry the
retained UUID to find an already-published review or publish it once.
