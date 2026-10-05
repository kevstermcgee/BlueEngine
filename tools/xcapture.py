#!/usr/bin/env python3
"""See a game on a machine with no display: run it on a virtual display and save real screenshots.

  python tools/xcapture.py GAME_BINARY [--frames 30,300] [--size 1280x720] [--out DIR] [--sheet] [--timeout 900]
                           [-- GAME ARGS...]

Runs `xvfb-run` with software OpenGL and the engine's `--capture` flags, then prints where the PNGs are. Read
them with an image viewer to judge the frame. Software rendering is slow (about 16 frames per second on a small
mini PC), so ask for a few frames, not a whole match. Anything after `--` goes to the game, for example
`-- --character ghost --autopilot`. `--sheet` also builds one contact-sheet image from all the frames.
Needs `xvfb`: `sudo apt install xvfb`.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def build_command(game, frames, size, out, extra, xvfb='xvfb-run'):
    """The command line for one capture run (pure, so it is testable without a display)."""
    width, height = size.split('x')
    last = max(int(f) for f in frames.split(','))
    audio = [] if '--audible' in extra else ['--mute']
    return [xvfb, '-a', '-s', f'-screen 0 {width}x{height}x24', str(game),
            '--capture', str(out), '--frames', frames, '--exit-after', str(last + 20),
            '--size', size, *audio, *extra]


def main(argv):
    if '--' in argv:
        split = argv.index('--')
        argv, extra = argv[:split], argv[split + 1:]
    else:
        extra = []
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('game')
    p.add_argument('--frames', default='30,300')
    p.add_argument('--size', default='1280x720')
    p.add_argument('--out', help='a directory that does not exist yet (default: a new temporary one)')
    p.add_argument('--sheet', action='store_true')
    p.add_argument('--timeout', type=int, default=900)
    args = p.parse_args(argv)
    if not shutil.which('xvfb-run'):
        print('xvfb-run is not installed. On Debian or Ubuntu: sudo apt install xvfb', file=sys.stderr)
        return 2
    game = Path(args.game).resolve()
    if not game.is_file():
        print(f'{game} is not a file: build the game first', file=sys.stderr)
        return 2
    out = Path(args.out) if args.out else Path(tempfile.mkdtemp(prefix='xcapture-')) / 'shots'
    command = build_command(game, args.frames, args.size, out, extra)
    env = {**os.environ, 'LIBGL_ALWAYS_SOFTWARE': '1'}
    try:
        done = subprocess.run(command, env=env, capture_output=True, text=True, timeout=args.timeout)
    except subprocess.TimeoutExpired:
        print(f'timed out after {args.timeout} s: ask for fewer or earlier frames', file=sys.stderr)
        return 3
    shots = sorted(out.glob('*.png'))
    for line in done.stdout.splitlines():
        if line.startswith('{"frame"'):
            print(json.loads(line)['path'])
        else:
            print(line)  # retain gameplay/audio/performance evidence from the executable
    if done.returncode or not shots:
        print(done.stderr[-1500:] or done.stdout[-1500:], file=sys.stderr)
        return done.returncode or 1
    if args.sheet and len(shots) > 1:
        sheet = out / 'sheet.png'
        subprocess.run([sys.executable, str(ROOT / 'tools' / 'contact_sheet.py'), str(sheet), '--dir', str(out)], check=False)
        if sheet.exists():
            print(sheet)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
