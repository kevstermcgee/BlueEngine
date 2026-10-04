#!/usr/bin/env python3
"""Rebuild and run this game without tripping over a copy of it that is still open.

On Windows a running .exe is locked, so `cargo build` fails with "Access is denied (os error 5)" while
the previous run (or a window the user left open) still exists. This closes any instance of THIS
project's binary first, then runs cargo. Debug builds here are already fast to iterate on: the game
crate is optimised at level 2 and the dependencies (physics, math, renderer) at level 3 and are built
once, so an edit relinks only the game.

  python scripts/dev.py [--release] [--build-only] [-- GAME ARGS...]

Examples: `python scripts/dev.py -- --capture shots --frames 30`, `python scripts/dev.py --build-only`.
"""
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def package_name():
    """The [package] name from Cargo.toml (also the binary name)."""
    text = (ROOT / 'Cargo.toml').read_text(encoding='utf-8')
    match = re.search(r'^\[package\][^\[]*?^name\s*=\s*"([^"]+)"', text, re.M | re.S)
    if not match:
        sys.exit('scripts/dev.py: no [package] name in Cargo.toml')
    return match.group(1)


def target_dir():
    """Cargo's target directory as this project sees it (honours CARGO_TARGET_DIR)."""
    out = subprocess.run(['cargo', 'metadata', '--format-version', '1', '--no-deps', '--quiet'],
                         cwd=ROOT, capture_output=True, text=True)
    try:
        return Path(json.loads(out.stdout)['target_directory'])
    except (ValueError, KeyError):
        return ROOT / 'target'


def close_running(exe):
    """Stop processes started from exactly `exe` (never a same-named program elsewhere)."""
    if sys.platform != 'win32' or not exe.parent.exists():
        return
    script = ("Get-Process -Name '%s' -ErrorAction SilentlyContinue | "
              "Where-Object { $_.Path -ieq '%s' } | "
              "ForEach-Object { Write-Host ('closing running ' + $_.Path); $_ | Stop-Process -Force; $_.WaitForExit(5000) | Out-Null }"
              % (exe.stem, str(exe).replace("'", "''")))
    subprocess.run(['powershell', '-NoProfile', '-NonInteractive', '-Command', script], check=False)


def main(argv):
    release = '--release' in argv
    build_only = '--build-only' in argv
    rest = []
    if '--' in argv:
        rest = argv[argv.index('--') + 1:]
    name = package_name()
    suffix = '.exe' if sys.platform == 'win32' else ''
    exe = target_dir() / ('release' if release else 'debug') / (name + suffix)
    close_running(exe)
    command = ['cargo', 'build' if build_only else 'run']
    if release:
        command.append('--release')
    if rest and not build_only:
        command += ['--'] + rest
    return subprocess.call(command, cwd=ROOT)


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
