#!/usr/bin/env bash
# Build each registered game's server from its own source checkout, check it, install it, and make the hub use it for
# that game only: `be2-hub reload GAME` retires that game's rooms (players inside keep playing until they leave, at most
# 30 minutes) and starts the game's Public room again from the new build. Nothing else is touched, and the hub is never
# restarted from here. Safe to run any time, from a timer or by hand: no sudo, no change when nothing relevant changed,
# one run at a time. The per-game logic is tools/hub_deploy.py (stdlib Python); this script is the command line, the
# lock and the hub/helper installs.
#
#   deploy/hub/update.sh                 update every game in sources.conf whose build inputs changed (or whose last
#                                        activation did not finish: that is retried without rebuilding)
#   deploy/hub/update.sh deadfall        only these games
#   deploy/hub/update.sh --pull          git pull --ff-only in each source root first
#   deploy/hub/update.sh --force         rebuild and reinstall even if nothing changed
#   deploy/hub/update.sh --rollback GAME reinstall GAME's previous known-good server and activate it
#   deploy/hub/update.sh --hub           first build be2-hub from the engine checkout, check it and install it (you restart it)
#   deploy/hub/update.sh --helpers       also install blue_portmap.py and the DuckDNS script
#
# What happens to each game (details: deploy/hub/README.md, section "Updating one game"):
#   inputs   its build inputs are fingerprinted: the game's and every path dependency's source (the engine, when the game
#            depends on it by path), manifests, Cargo.lock, toolchain, build arguments and the environment that changes
#            the output. Unchanged and fully activated: nothing runs, not even Cargo.
#   build    cargo build --locked --release; refused if an input changed while it ran.
#   check    `be2-hub verify`: the hub's own rules on --info, then an isolated start that must print a STATUS line.
#            A bad candidate is discarded; the installed server and the record of what is deployed stay as they were.
#   install  atomic rename; the previous working server is kept as <bin>.previous.
#   activate `be2-hub reload GAME`, if the hub is running. If it is not: "installed, activation pending".
#   ready    `be2-hub status GAME`: the hub's Public room process must print STATUS with the new build. A reload that
#            was accepted is reported as "activated", never as "ready".
#
# Environment (all optional; tests point every one of them at a temporary directory):
#   BLUEENGINE_HOME      where the binaries are installed                      (default ~/blueengine)
#   BLUEENGINE_CONFIG    the directory with hub.conf and sources.conf          (default ~/.config/blueengine)
#   BLUEENGINE_ENGINE    the engine checkout (for --hub and --helpers)         (default ~/BlueEngine)
#   BLUEENGINE_STATE     deployment receipts and the lock                      (default ~/.local/share/blueengine)
#   BLUEENGINE_LOCK      the one-run-at-a-time lock file                       (default $BLUEENGINE_STATE/update.lock)
#   BLUEENGINE_SYSTEMCTL the systemctl to ask whether the hub unit is active   (default systemctl)
#   BLUEENGINE_UNIT      the hub's unit name                                   (default blueengine-hub.service)
#   BLUEENGINE_READY_WAIT seconds to wait for the Public room's first STATUS   (default 30)
set -euo pipefail

HOME_DIR="${BLUEENGINE_HOME:-$HOME/blueengine}"
CONFIG_DIR="${BLUEENGINE_CONFIG:-$HOME/.config/blueengine}"
ENGINE="${BLUEENGINE_ENGINE:-$HOME/BlueEngine}"
STATE="${BLUEENGINE_STATE:-$HOME/.local/share/blueengine}"
LOCK="${BLUEENGINE_LOCK:-$STATE/update.lock}"
export BLUEENGINE_HOME="$HOME_DIR" BLUEENGINE_CONFIG="$CONFIG_DIR" BLUEENGINE_ENGINE="$ENGINE" BLUEENGINE_STATE="$STATE"
TOOLS="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../tools" && pwd)"
HUB=0; HELPERS=0; ROLLBACK=0
PYARGS=()
WANTED=()
for arg in "$@"; do
  case "$arg" in
    --force|--pull) PYARGS+=("$arg") ;;
    --rollback) ROLLBACK=1 ;;
    --hub) HUB=1 ;;
    --helpers) HELPERS=1 ;;
    -h|--help) sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "unknown option $arg" >&2; exit 2 ;;
    *) WANTED+=("$arg") ;;
  esac
done
if [ "$ROLLBACK" = 1 ] && { [ "${#WANTED[@]}" -ne 1 ] || [ "${#PYARGS[@]}" -ne 0 ]; }; then
  echo "usage: update.sh --rollback GAME" >&2; exit 2
fi

mkdir -p "$HOME_DIR" "$STATE/deployed" "$(dirname "$LOCK")"

# One run at a time.
exec 9>"$LOCK"
flock -n 9 || { echo "another update is running"; exit 0; }

status=0

if [ "$HUB" = 1 ]; then
  # First, so the games below are checked and activated by the new be2-hub (verify and status live there).
  python3 "$TOOLS/hub_deploy.py" hub "$ENGINE" || status=1
fi

if [ "$HELPERS" = 1 ]; then
  install -m 644 "$ENGINE/tools/blue_portmap.py" "$HOME_DIR/blue_portmap.py"
  install -m 755 "$ENGINE/deploy/hub/blueengine-ddns.sh" "$HOME_DIR/blueengine-ddns.sh"
  echo "installed blue_portmap.py and blueengine-ddns.sh into $HOME_DIR"
fi

if [ "$ROLLBACK" = 1 ]; then
  python3 "$TOOLS/hub_deploy.py" rollback "${WANTED[0]}" || status=1
else
  python3 "$TOOLS/hub_deploy.py" update ${PYARGS[@]+"${PYARGS[@]}"} ${WANTED[@]+"${WANTED[@]}"} || status=1
fi
exit "$status"
