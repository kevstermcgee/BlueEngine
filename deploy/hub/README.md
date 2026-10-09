# The BlueEngine hub: one name, one port, every online game

What this sets up on the always-on box: one small program, `be2-hub`, listens on UDP 4100. A game's Play Online screen
asks it "which rooms exist for *my* game?"; players pick a room by name or make one. For each room the hub starts that
game's own server program on a free port from a shared pool (4101-4116 by default), closes rooms that stay empty, and keeps
a permanent **Public** room per game running. Nobody types an IP or a code, and one DuckDNS name (`blue-engine.duckdns.org`)
serves every game. Design and reasons: `docs/adr/0037-shared-multi-game-hub.md`. Nothing here is installed by the repository:
these are the steps for a human.

Files in this directory:

| File | What it is |
| --- | --- |
| `blueengine-hub.service` | systemd user service running `be2-hub` (which starts the room servers) |
| `hub.conf.example` | the registry: which games the hub carries, their server programs and room rules |
| `blueengine-portmap.service`, `.timer` | every 30 minutes, renews the router's UPnP mappings for the hub port and the whole room pool (the list comes from `be2-hub ports`) |
| `blueengine-ddns.service`, `.timer`, `blueengine-ddns.sh` | every 5 minutes, points the DuckDNS names at the home IP |
| `sources.conf.example` | where `update.sh` builds each game's server from |
| `update.sh` | builds registered games' servers, checks each candidate, installs it, and reloads only that game's rooms in the hub, then waits for the hub to show the new build running (`tools/hub_deploy.py` does the per-game work) |

## 0. What must keep working

* **Already shipped Deadfall clients** talk the old `DFHB` protocol to `deadfall-kevin.duckdns.org:4100`. The new hub answers it
  on the same port (`legacy = serve` in `hub.conf`), mapped to the game called `deadfall`. Keep that DuckDNS name pointing here.
* The hub and every room server use **UDP only**. The hub port (4100) is fixed; rooms use the pool. Open both (step 5).
* Spooky Kart, Prop Hunt, Slapstick and Deadfall all default to UDP 4100 when run by hand; under the hub the hub passes each room
  an explicit `--listen`, so they no longer collide. Do not also run a standalone server on 4100-4116.

## 1. DuckDNS names (once)

The updater already keeps `deadfall-kevin` and `blue-engine` pointing at this machine when both names are in `DOMAIN`. Check
`~/.config/deadfall/duckdns.env`; the new unit reads `~/.config/blueengine/duckdns.env` with the same content:

```sh
mkdir -p ~/.config/blueengine
install -m 600 ~/.config/deadfall/duckdns.env ~/.config/blueengine/duckdns.env    # or write it fresh, see below
grep -c '^DOMAIN=deadfall-kevin,blue-engine$' ~/.config/blueengine/duckdns.env    # 1 means both names
```

A fresh file (the token is a secret: it can repoint your names; never commit it):

```sh
cat > ~/.config/blueengine/duckdns.env <<'EOT'
DOMAIN=blue-engine,deadfall-kevin
TOKEN=paste-your-token-here
EOT
chmod 600 ~/.config/blueengine/duckdns.env
```

## 2. Say where each game's server is built from

```sh
cp ~/BlueEngine/deploy/hub/sources.conf.example ~/.config/blueengine/sources.conf
$EDITOR ~/.config/blueengine/sources.conf        # one line per game: id, source checkout, cargo bin, extra build args
```

Each game's server must be built with `netplay::cli::serve` (it prints `--info`, `--status-lines`, takes `--set`): a server that
is not will be skipped by the hub with a log line saying why, and `update.sh` refuses to install it. The path `server =` in
`hub.conf` must be the file `update.sh` installs (`~/blueengine/CARGO-BIN`); `update.sh` checks that too.

## 3. Write the registry

```sh
cp ~/BlueEngine/deploy/hub/hub.conf.example ~/.config/blueengine/hub.conf
$EDITOR ~/.config/blueengine/hub.conf
mkdir -p ~/.local/share/blueengine/reports
```

Every key is explained in the example and in `src/viewer/netplay/hub/registry.rs`. The hub's `[hub]` section sets the hub port
(`listen`), the room pool (`pool_start`, `pool_size`) and `legacy`; each `[game ID]` section names a server program, whether the
game has a Public room, which settings players may choose, and how many rooms the game may have at once.

Write this before the first build: `update.sh` checks every candidate server against the registry, and refuses to install one the registry would reject.

## 4. Build and install

```sh
bash ~/BlueEngine/deploy/hub/update.sh --force --hub --helpers
```

This builds `be2-hub` from `~/BlueEngine` first (checked with `--help` and against your `hub.conf`, then installed; the running
hub is not touched), then each game's server in release mode (no graphics libraries needed), checks every candidate with the new
`be2-hub verify` and installs them with `blue_portmap.py` and the DuckDNS script into `~/blueengine/`. On the very first run there
is no hub yet, so each game ends as "installed, activation pending"; that is correct, and step 5 starts the hub.

## 5. Switch over from the Deadfall-only hub (a short outage)

Only one program can own UDP 4100, so the old hub stops first. Rooms that are running end; do this when nobody is playing.

```sh
systemctl --user disable --now deadfall-hub.service deadfall-portmap.timer deadfall-ddns.timer
mkdir -p ~/.config/systemd/user
cp ~/BlueEngine/deploy/hub/blueengine-hub.service ~/BlueEngine/deploy/hub/blueengine-portmap.* \
   ~/BlueEngine/deploy/hub/blueengine-ddns.service ~/BlueEngine/deploy/hub/blueengine-ddns.timer ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now blueengine-hub.service blueengine-portmap.timer blueengine-ddns.timer
loginctl enable-linger "$USER"      # keep user services running when nobody is logged in, and start them at boot
systemctl --user start blueengine-ddns.service blueengine-portmap.service      # do not wait for the timers
```

If `systemctl --user status blueengine-hub` shows `status=226/NAMESPACE`, the sandbox lines are not allowed for user services on
this kernel: comment out `ProtectSystem`, `ReadWritePaths` and `PrivateTmp` in the unit and reload.

## 6. Check that it works

```sh
systemctl --user status blueengine-hub blueengine-portmap.timer blueengine-ddns.timer
journalctl --user -u blueengine-hub -n 40       # "be2-hub on 0.0.0.0:4100 ...", one "Game ..." line per game, each "Public room ... is up"
~/blueengine/be2-hub ports --config ~/.config/blueengine/hub.conf       # the UDP ports to open: 4100, then 4101-4116
ss -lunp | grep -E ':41[0-9][0-9]\b'            # 4100 (the hub) and one port per running room
python3 ~/blueengine/blue_portmap.py status --port 4100       # router mapping and the public IPv4 (repeat for the pool)
getent hosts blue-engine.duckdns.org deadfall-kevin.duckdns.org   # both should print your public IP
```

An **old Deadfall client** check, from any machine (prints a 14-byte reply whose last four bytes are Deadfall's fingerprint):

```sh
python3 - <<'EOF'
import socket, struct
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.settimeout(3)
s.sendto((b"DFHB\x01\x03" + struct.pack("<I", 7)).ljust(32, b"\0"), ("deadfall-kevin.duckdns.org", 4100))
reply = s.recv(2048); print(len(reply), reply[:6], "build", reply[10:14][::-1].hex())
EOF
```

Then, from a different network (a phone hotspot, or a friend): start each game, Play Online; the Public room should be listed.
UPnP must be enabled on the router; if `blue_portmap.py` cannot map the ports, forward UDP 4100-4116 to this machine by hand.

## 7. Updating one game

```sh
bash ~/BlueEngine/deploy/hub/update.sh --pull spooky-kart     # or no name: every game whose build inputs changed
bash ~/BlueEngine/deploy/hub/update.sh --rollback spooky-kart # reinstall the previous known-good server
```

What a run does for each game, in order. Each step either completes or stops the game there, and the output says which:

| Step | What it means | If it fails |
| --- | --- | --- |
| **inputs** | The build inputs are fingerprinted: the game's source and the source of every local path dependency (the engine, when the game depends on it by path), the manifests, `Cargo.lock`, the cargo config, the Rust toolchain (`rustc -vV`), the build arguments from `sources.conf` and the environment that changes the output (`RUSTFLAGS`, `CARGO_PROFILE_*`, `CARGO_BUILD_TARGET` ...). Git is not consulted: untracked source counts, other games, docs, tests and build output do not. The first line per game says why it will or will not rebuild. | Inputs that cannot be identified (`cargo metadata --locked --offline` fails, `Cargo.lock` missing) build nothing and change nothing. `--pull` pulls only the game's own checkout: pull the engine checkout yourself when the game depends on it by path. `python3 tools/hub_deploy.py identity ROOT BIN [cargo args]` prints the identity and its components without building. |
| **build** | `cargo build --locked --release`. The inputs are fingerprinted again afterwards: if another agent edited the engine or the game while Cargo ran, the output is not installed (run again when the tree is quiet). | Nothing changed. |
| **check** | The executable is copied beside its destination as a hidden file and `be2-hub verify` applies the rules the hub itself applies when it loads a game (`--info` parses within 5 s and 64 KB, the settings schema is valid, `public_set`/`user_set`/`client_settings` still resolve, `server =` is this file; a `game=` name that differs from the config id is only a warning, as in the hub). Then it starts the candidate on loopback with a scratch report directory and waits for its first `STATUS` line (10 s), stops it and reaps it. | The candidate is deleted. The installed server and the record of what is deployed are untouched. The next run builds and checks again. |
| **install** | The known-good installed server is kept as `CARGO-BIN.previous`, then the candidate is renamed over the installed file (atomic: same directory). | The rename did not happen; the record is put back. |
| **activate** | If the hub is running, `be2-hub reload GAME`: the hub starts the new build's Public room **first**, and only then retires the game's old rooms. A reload that cannot start the new Public room (no free port or process slot, a server that will not run) changes nothing and says so. If the hub is not running, the game is **installed, activation pending**: the hub loads the server when it starts. | The server stays installed and the state is `installed` (the hub also notices a replaced file by itself within about 20 s). The next run retries the activation without rebuilding. |
| **ready** | `be2-hub status GAME --expect-build B`: the hub reports the build its registry holds and whether the Public room's own process prints `STATUS` with build `B` (the `build=` of the candidate's `--info`). Only that is called **ready**. A game with no Public room can only be **activated** (the registry holds the build; no process exists to confirm it). | The state stays `activated`. The next run looks again before doing anything else. |

Reload **retires** the game's old rooms: they disappear from the list and close as soon as they are empty, but players inside a
match keep playing, for at most 30 minutes more (then the room is closed even with players in it). Other games are not touched:
their rooms, ports, players and settings stay as they were. A reload acknowledgement means "the hub accepted the request and started
a replacement room", never "the room works".

### What was deployed, and what an interrupted run leaves

`~/.local/share/blueengine/deployed/GAME.json` is the receipt: the identity and the hash of each component, the installed file's
hash, the `--info` summary, git revisions (for reading only; they are not part of the identity), timestamps and the **phase**:

| Phase | Meaning | The next run |
| --- | --- | --- |
| *(no receipt)* | Never deployed by this updater (the older updater's plain stamp file is ignored; the first run rebuilds once). | Builds. |
| `installing` | The run stopped around the rename. If the installed file is the recorded one, the rename happened; if not, it did not. | Recovers or builds again; the old server was never lost. |
| `installed` | The new server is in place; the hub was not asked, not running, or refused. | Retries the activation (no rebuild). |
| `activated` | The hub accepted the reload; readiness was not shown. | Asks the hub again; reloads again only if the hub answers and shows a different build or no Public room. |
| `ready` | The hub's Public room runs the new build and prints `STATUS`. | Nothing runs at all, not even Cargo. |

A receipt is trusted only while the identity still matches **and** the installed file is the recorded one. Anything else, including
a server file replaced by hand, makes the next run build.

A run exits non-zero when any game failed or stopped before `ready`; a game that is only "installed, activation pending" because the
hub is stopped is not a failure.

### Recovering

* **A bad candidate** (build error, invalid `--info`, will not start): nothing was installed; fix the game and run again.
* **"installed but NOT activated" or "NOT ready"**: the new server is installed and the old rooms may still be running. Look at
  `journalctl --user -u blueengine-hub -n 40`, then run `update.sh GAME` again (it does not rebuild), or roll back:
* **Roll back**: `update.sh --rollback GAME` verifies `CARGO-BIN.previous`, reinstalls it, reloads and waits for ready. The source
  that was rolled back is remembered, so the next run does not deploy it again until the source changes (or `--force`).
  `CARGO-BIN.previous` is only ever refreshed from a server whose activation completed.
  Its source identity follows the retained artifact: after A succeeds, B fails readiness, and C succeeds,
  rollback restores A with A's identity. Requesting B afterward builds B instead of claiming it is installed.
  The updater hashes the retained file before carrying its metadata forward; rollback checks that hash before
  assigning a source identity. A mismatched recorded artifact fails without changing the installation or receipt.
  An older artifact with no matching source metadata can be validated and restored, but receives an unknown
  source identity rather than borrowing another revision's identity.
  Remembering that rejected source never completes an unfinished rollback: the ordinary updater verifies the restored
  executable's size and SHA-256 against its receipt, resumes installation if interrupted, then resumes activation/readiness.
  A missing or inconsistent installed artifact is restored only from the receipt-matching, validated previous executable;
  if that artifact is also unavailable or inconsistent, recovery fails clearly without rebuilding the rejected revision.
  A stopped hub or failed readiness leaves the receipt incomplete and retryable. Only a verified artifact with completed
  activation/readiness may skip. Other games retain their own receipts and artifacts.
* **`no-answer` from `status`**: the running hub was started before `status` existed. Restart it when nobody is playing
  (`systemctl --user restart blueengine-hub`); until then `update.sh` honestly reports the game as activated, not ready.
* **Killed in the middle** (power, `kill -9`): run it again. The lock is released with the process.

Players need the matching game version: the hub sends each game's build id with the room list, and an older client is told to update
before it tries to join.

Changing the hub itself (`update.sh --hub`) installs the new `be2-hub` (kept as `be2-hub.previous`) after checking that it starts and
accepts your `hub.conf`; restart it yourself, because that ends every room of every game: `systemctl --user restart blueengine-hub`.

`update.sh` and everything it starts read only `BLUEENGINE_HOME`, `BLUEENGINE_CONFIG`, `BLUEENGINE_ENGINE`, `BLUEENGINE_STATE` (the
lock is `$BLUEENGINE_STATE/update.lock`, or `BLUEENGINE_LOCK`), `BLUEENGINE_SYSTEMCTL` and `BLUEENGINE_UNIT`; the tests set all of them
to temporary directories and a fake `systemctl`, so they never touch a live installation (`python3 -m unittest tools.test_hub_deploy`,
`cargo test --test hub update_sh`).

### What the hub is not

The hub lists rooms and starts processes. It is not a relay or NAT traversal (players connect to `hub_host:room_port`, so the ports
must be open), not an identity service and not an encrypted transport. Room discovery is UDP; gameplay rooms can use the existing pinned QUIC/TLS production transport.
For each Internet game set `transport = production` and `join_key_env = MY_GAME_JOIN_KEY`. Provision
that exactly 32-byte secret, `BLUE_TLS_CERT_FILE` and `BLUE_TLS_KEY_FILE` in the hub service environment;
use DER credentials for `feta.local` as described in docs/HOSTING.md. Rebuild room servers with the
current `netplay::cli::serve` admission capability. Clients use BEHB v2 metadata and `hub::connect_room`
with a separately provisioned key/public certificate (docs/NETPLAY.md). Discovery never distributes credentials.

For the user service, add a drop-in with `systemctl --user edit blueengine-hub`:

```ini
[Service]
EnvironmentFile=%h/.config/blueengine/hub-secrets.env
```

Keep that file private (mode 0600), with the named admission variables and TLS file paths. Keep DER
private keys outside source control and readable only by the server account. Supply the same environment
to an operator's `be2-hub verify --start`/update session so its isolated readiness probe can start a
production candidate; service credentials are not automatically inherited by a terminal.

Development binds are loopback unless the operator explicitly sets `BLUE_ALLOW_DEVELOPMENT_INTERNET=1`
for a trusted legacy deployment. That opt-in still uses unauthenticated, unencrypted UDP. Creation cookies
and rate limits reduce abuse; they do not establish player identity. Existing deployments must explicitly
opt in or migrate to production. BEHB v1 receives an update notice; DFHB v1 supports only unkeyed development
rooms; BECT deployment control remains v1. Upgrade hub and supported discovery clients together.

## 8. Changing rooms and limits

Edit `~/.config/blueengine/hub.conf`. A `[game ...]` change: `~/blueengine/be2-hub reload GAME --config ~/.config/blueengine/hub.conf`.
A `[hub]` change (port, pool, limits): restart the hub. If you raise `pool_size`, the portmap unit follows by itself at its next
run (`systemctl --user start blueengine-portmap.service`); the router mapping and the room pool are the same list.

## 9. Pointing a game at a different hub (testing, or a different port)

A game uses `blue-engine.duckdns.org:4100` unless a file called `server.txt` sits next to its executable (first line `host` or
`host:port`; a LAN test: `192.168.1.20`) or it is given a command-line address. Delete the file to go back to the default.

## 10. Retiring the old Deadfall protocol

When every Deadfall player has updated, set `legacy = refuse` in `[hub]` and restart the hub. Old clients then get a well-formed
reply with a deliberately wrong build and show their own "update the game" message instead of "can't reach the servers".

## Stopping and removing

```sh
systemctl --user disable --now blueengine-hub.service blueengine-portmap.timer blueengine-ddns.timer
for p in $(~/blueengine/be2-hub ports --config ~/.config/blueengine/hub.conf); do
  python3 ~/blueengine/blue_portmap.py remove --port "$p"      # if you want the router mappings gone now
done
```

Stopping the hub stops every room it started. Stopped or crashed (even `kill -9`), the servers never outlive the hub: they quit
when it goes.
