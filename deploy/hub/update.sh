#!/usr/bin/env bash
# Build each registered game's server from its own source checkout, install it, and make the hub use it for that game
# only: `be2-hub reload GAME` retires that game's rooms (players inside keep playing until they leave) and starts the
# game's Public room again from the new build. Nothing else is touched, and the hub is never restarted from here.
# Safe to run any time, from a timer or by hand: no sudo, no change when nothing is new, one run at a time.
#
#   deploy/hub/update.sh                 update every game in sources.conf whose checkout changed
#   deploy/hub/update.sh deadfall        only these games
#   deploy/hub/update.sh --pull          git pull --ff-only in each source root first
#   deploy/hub/update.sh --force         rebuild and reinstall even if nothing changed
#   deploy/hub/update.sh --hub           also build be2-hub from the engine checkout and install it (you restart it)
#   deploy/hub/update.sh --helpers       also install blue_portmap.py and the DuckDNS script
#
# Environment (all optional):
#   BLUEENGINE_HOME      where the binaries are installed                      (default ~/blueengine)
#   BLUEENGINE_CONFIG    the directory with hub.conf and sources.conf          (default ~/.config/blueengine)
#   BLUEENGINE_ENGINE    the engine checkout (for --hub and --helpers)         (default ~/BlueEngine)
#   BLUEENGINE_STATE     remembers what is installed                           (default ~/.local/share/blueengine)
#
# Why a game is rebuilt: its source root's HEAD commit or uncommitted changes differ from what was installed. A game that
# depends on the engine by path is not rebuilt when only the engine changed: run with --force after engine changes.
set -euo pipefail

HOME_DIR="${BLUEENGINE_HOME:-$HOME/blueengine}"
CONFIG_DIR="${BLUEENGINE_CONFIG:-$HOME/.config/blueengine}"
ENGINE="${BLUEENGINE_ENGINE:-$HOME/BlueEngine}"
STATE="${BLUEENGINE_STATE:-$HOME/.local/share/blueengine}"
UNIT=blueengine-hub.service
FORCE=0; PULL=0; HUB=0; HELPERS=0
WANTED=()
for arg in "$@"; do
  case "$arg" in
    --force) FORCE=1 ;;
    --pull) PULL=1 ;;
    --hub) HUB=1 ;;
    --helpers) HELPERS=1 ;;
    -h|--help) sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "unknown option $arg" >&2; exit 2 ;;
    *) WANTED+=("$arg") ;;
  esac
done

mkdir -p "$HOME_DIR" "$STATE/deployed" "$HOME/.cache"

# One run at a time.
exec 9>"$HOME/.cache/blueengine-update.lock"
flock -n 9 || { echo "another update is running"; exit 0; }

wanted() {
  [ ${#WANTED[@]} -eq 0 ] && return 0
  local g
  for g in "${WANTED[@]}"; do [ "$g" = "$1" ] && return 0; done
  return 1
}

expand_home() { case "$1" in "~"|"~/"*) printf '%s' "$HOME${1#\~}" ;; *) printf '%s' "$1" ;; esac; }

# Install next to the running copy, then rename into place: a running binary is never overwritten in place, and the
# hub sees the new file appear all at once.
install_binary() {  # src dest
  install -m 755 "$1" "$2.new"
  mv -f "$2.new" "$2"
}

reload_game() {  # game
  if systemctl --user is-active --quiet "$UNIT" 2>/dev/null; then
    "$HOME_DIR/be2-hub" reload "$1" --config "$CONFIG_DIR/hub.conf"
  else
    echo "$UNIT is not running; $1 will be used the next time the hub starts"
  fi
}

target_dir() {  # source-root
  (cd "$1" && cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
}

updated=0
SOURCES="$CONFIG_DIR/sources.conf"
[ -f "$SOURCES" ] || { echo "no $SOURCES (copy deploy/hub/sources.conf.example there)" >&2; exit 1; }

while read -r game root bin extra; do
  case "${game:-}" in ''|'#'*) continue ;; esac
  wanted "$game" || continue
  [ -n "${root:-}" ] && [ -n "${bin:-}" ] || { echo "sources.conf: $game needs a source root and a cargo bin name" >&2; exit 1; }
  root="$(expand_home "$root")"
  [ -d "$root" ] || { echo "$game: source root $root does not exist" >&2; exit 1; }
  if [ "$PULL" = 1 ]; then git -C "$root" pull --ff-only --quiet; fi
  stamp="$(git -C "$root" rev-parse HEAD)-$(git -C "$root" diff HEAD | sha256sum | cut -c1-12)"
  old="$(cat "$STATE/deployed/$game" 2>/dev/null || true)"
  if [ "$FORCE" = 0 ] && [ "$stamp" = "$old" ] && [ -x "$HOME_DIR/$bin" ]; then
    echo "$game: up to date (${stamp:0:12})"
    continue
  fi
  echo "$game: building $bin from $root"
  # shellcheck disable=SC2086  # $extra is a list of cargo arguments on purpose
  (cd "$root" && cargo build --locked --release $extra --bin "$bin")
  install_binary "$(target_dir "$root")/release/$bin" "$HOME_DIR/$bin"
  # Check the new server describes itself before the hub is asked to use it (a broken build changes nothing there).
  "$HOME_DIR/$bin" --info | sed 's/^/  /'
  echo "$stamp" > "$STATE/deployed/$game"
  reload_game "$game"
  updated=$((updated + 1))
done < "$SOURCES"

if [ "$HUB" = 1 ]; then
  echo "hub: building be2-hub from $ENGINE"
  (cd "$ENGINE" && cargo build --locked --release --no-default-features --bin be2-hub)
  install_binary "$(target_dir "$ENGINE")/release/be2-hub" "$HOME_DIR/be2-hub"
  echo "installed be2-hub. Restart it when nobody is playing (this ends every room):"
  echo "  systemctl --user restart $UNIT"
fi

if [ "$HELPERS" = 1 ]; then
  install -m 644 "$ENGINE/tools/blue_portmap.py" "$HOME_DIR/blue_portmap.py"
  install -m 755 "$ENGINE/deploy/hub/blueengine-ddns.sh" "$HOME_DIR/blueengine-ddns.sh"
  echo "installed blue_portmap.py and blueengine-ddns.sh into $HOME_DIR"
fi

echo "done: $updated game(s) updated"
