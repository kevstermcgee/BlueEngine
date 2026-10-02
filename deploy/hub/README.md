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
| `update.sh` | builds registered games' servers, installs them, and reloads only that game's rooms in the hub |

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
is not will be skipped by the hub with a log line saying why.

## 3. Build and install

```sh
bash ~/BlueEngine/deploy/hub/update.sh --force --hub --helpers
```

This builds each game's server in release mode (no graphics libraries needed), `be2-hub` from `~/BlueEngine`, and installs them
with `blue_portmap.py` and the DuckDNS script into `~/blueengine/`. It also runs each server's `--info` as a check.

## 4. Write the registry

```sh
cp ~/BlueEngine/deploy/hub/hub.conf.example ~/.config/blueengine/hub.conf
$EDITOR ~/.config/blueengine/hub.conf
mkdir -p ~/.local/share/blueengine/reports
```

Every key is explained in the example and in `src/viewer/netplay/hub/registry.rs`. The hub's `[hub]` section sets the hub port
(`listen`), the room pool (`pool_start`, `pool_size`) and `legacy`; each `[game ID]` section names a server program, whether the
game has a Public room, which settings players may choose, and how many rooms the game may have at once.

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
bash ~/BlueEngine/deploy/hub/update.sh --pull spooky-kart     # or no name: every game whose checkout changed
```

It builds that game's server, installs it, checks its `--info`, and runs `be2-hub reload spooky-kart`. Reload **retires** that
game's rooms: they disappear from the list and close as soon as they are empty, but players inside a match keep playing (for at
most 30 minutes more); the game's Public room starts again from the new build right away on a new port. Other games are not
touched. If the server file is replaced some other way, the hub notices within about 20 seconds and does the same by itself. A
broken new build (its `--info` fails) changes nothing and is logged. Players need the matching game version: the hub sends each
game's build id with the room list, and an older client is told to update before it tries to join.

Changing the hub itself (`update.sh --hub`) installs the new `be2-hub`; restart it yourself, because that ends every room of
every game: `systemctl --user restart blueengine-hub`.

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
