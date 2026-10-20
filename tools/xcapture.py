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


def build_command(game, frames, size, out, extra, xvfb='xvfb-run', xvfb_log=None):
    """The command line for one capture run (pure, so it is testable without a display)."""
    width, height = size.split('x')
    last = max(int(f) for f in frames.split(','))
    audio = [] if '--audible' in extra else ['--mute']
    diagnostics = ['-e', str(xvfb_log)] if xvfb_log is not None else []
    return [xvfb, '-a', *diagnostics, '-s', f'-screen 0 {width}x{height}x24', str(game),
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
    if out.exists():
        print(f'{out} already exists: choose a new capture directory', file=sys.stderr)
        return 2
    env = {**os.environ, 'LIBGL_ALWAYS_SOFTWARE': '1'}
    timed_out = False
    # The game owns creation of its capture directory. Keep server diagnostics
    # elsewhere until it exits so a failed Xvfb startup is retained too.
    with tempfile.TemporaryDirectory(prefix='xcapture-xvfb-') as log_dir:
        xvfb_log = Path(log_dir) / 'xvfb.stderr.log'
        command = build_command(game, args.frames, args.size, out, extra, xvfb_log=xvfb_log)
        try:
            done = subprocess.run(command, env=env, capture_output=True, text=True, timeout=args.timeout)
        except subprocess.TimeoutExpired as error:
            timed_out = True
            # TimeoutExpired can carry bytes even when text=True was requested.
            def decoded(value):
                return value.decode('utf-8', errors='replace') if isinstance(value, bytes) else value or ''

            done = subprocess.CompletedProcess(command, 3, decoded(error.stdout), decoded(error.stderr))
        server_stderr = xvfb_log.read_text(encoding='utf-8', errors='replace') if xvfb_log.exists() else ''
    out.mkdir(parents=True, exist_ok=True)
    shots = sorted(out.glob('*.png'))
    for line in done.stdout.splitlines():
        if line.startswith('{"frame"') and not timed_out:
            try:
                print(json.loads(line)['path'])
            except (ValueError, KeyError, TypeError):
                print(line)
        else:
            print(line)  # retain gameplay/audio/performance evidence from the executable
    # Keep worker diagnostics even when the render process exits successfully.
    (out / 'native.stdout.log').write_text(done.stdout, encoding='utf-8')
    (out / 'native.stderr.log').write_text(done.stderr, encoding='utf-8')
    (out / 'xvfb.stderr.log').write_text(server_stderr, encoding='utf-8')
    import importlib.util
    spec = importlib.util.spec_from_file_location('be_capture_evidence', ROOT / 'templates/game_review.py')
    review = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(review)
    evidence = review.capture_subsystems(done.stdout, '--audible' not in extra)
    evidence['rendering'] = 'captured' if shots and done.returncode == 0 else 'failed'
    (out / 'subsystems.json').write_text(json.dumps(evidence, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'capture_evidence': evidence}))
    if done.returncode or not shots:
        if timed_out:
            print(f'timed out after {args.timeout} s: ask for fewer or earlier frames', file=sys.stderr)
        print(done.stderr[-1500:] or done.stdout[-1500:], file=sys.stderr)
        if server_stderr:
            print('Xvfb diagnostics:\n' + server_stderr[-1500:], file=sys.stderr)
        print(f'capture diagnostics: {out}', file=sys.stderr)
        return done.returncode or 1
    if args.sheet and len(shots) > 1:
        sheet = out / 'sheet.png'
        subprocess.run([sys.executable, str(ROOT / 'tools' / 'contact_sheet.py'), str(sheet), '--dir', str(out)], check=False)
        if sheet.exists():
            print(sheet)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
