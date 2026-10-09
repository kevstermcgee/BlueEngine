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
import ctypes
import ctypes.util
import threading
import time
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


class NativeInput:
    """XTest events cross X11 and the window backend; they never call game rules."""
    def __init__(self):
        self.x = ctypes.CDLL(ctypes.util.find_library('X11') or 'libX11.so.6')
        self.xt = ctypes.CDLL(ctypes.util.find_library('Xtst') or 'libXtst.so.6')
        self.x.XOpenDisplay.argtypes = [ctypes.c_char_p]
        self.x.XOpenDisplay.restype = ctypes.c_void_p
        self.display = self.x.XOpenDisplay(None)
        if not self.display:
            raise ValueError('INPUT-DISPLAY: XOpenDisplay failed; check virtual-display startup')
        for name, args, result in [
            ('XDefaultRootWindow', [ctypes.c_void_p], ctypes.c_ulong),
            ('XQueryTree', [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong),
                           ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)),
                           ctypes.POINTER(ctypes.c_uint)], ctypes.c_int),
            ('XSetInputFocus', [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong], ctypes.c_int),
            ('XStringToKeysym', [ctypes.c_char_p], ctypes.c_ulong),
            ('XKeysymToKeycode', [ctypes.c_void_p, ctypes.c_ulong], ctypes.c_uint),
            ('XFlush', [ctypes.c_void_p], ctypes.c_int),
            ('XCloseDisplay', [ctypes.c_void_p], ctypes.c_int),
            ('XFree', [ctypes.c_void_p], ctypes.c_int),
        ]:
            function = getattr(self.x, name)
            function.argtypes, function.restype = args, result
        for name, args in [
            ('XTestFakeKeyEvent', [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]),
            ('XTestFakeButtonEvent', [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]),
            ('XTestFakeMotionEvent', [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_ulong]),
        ]:
            getattr(self.xt, name).argtypes = args
            getattr(self.xt, name).restype = ctypes.c_int
        root = self.x.XDefaultRootWindow(self.display)
        parent, returned_root = ctypes.c_ulong(), ctypes.c_ulong()
        children, count = ctypes.POINTER(ctypes.c_ulong)(), ctypes.c_uint()
        if not self.x.XQueryTree(self.display, root, ctypes.byref(returned_root), ctypes.byref(parent),
                                 ctypes.byref(children), ctypes.byref(count)) or count.value != 1:
            if children:
                self.x.XFree(children)
            self.x.XCloseDisplay(self.display)
            raise ValueError('INPUT-WINDOW: expected exactly one game window on the isolated display')
        try:
            self.x.XSetInputFocus(self.display, children[0], 2, 0)
        finally:
            self.x.XFree(children)
        self.held_keys, self.held_buttons = set(), set()

    def send(self, step):
        if 'key' in step:
            code = self.x.XKeysymToKeycode(self.display, self.x.XStringToKeysym(step['key'].encode()))
            if not code:
                raise ValueError('INPUT-KEY: unknown X11 keysym ' + step['key'])
            ok = self.xt.XTestFakeKeyEvent(self.display, code, step['down'], 0)
            (self.held_keys.add if step['down'] else self.held_keys.discard)(code)
        elif 'button' in step:
            ok = self.xt.XTestFakeButtonEvent(self.display, step['button'], step['down'], 0)
            (self.held_buttons.add if step['down'] else self.held_buttons.discard)(step['button'])
        else:
            ok = self.xt.XTestFakeMotionEvent(self.display, -1, *step['move'], 0)
        self.x.XFlush(self.display)
        if not ok:
            raise ValueError('INPUT-XTEST: event submission failed')

    def close(self):
        for code in self.held_keys:
            self.xt.XTestFakeKeyEvent(self.display, code, 0, 0)
        for button in self.held_buttons:
            self.xt.XTestFakeButtonEvent(self.display, button, 0, 0)
        self.x.XFlush(self.display)
        self.x.XCloseDisplay(self.display)


def input_steps(path):
    """Strict bounded scripts prevent an invalid probe from launching or certifying a game."""
    if Path(path).stat().st_size > 1_000_000:
        raise ValueError('INPUT-SCRIPT: script exceeds 1 MB')
    value = json.loads(Path(path).read_text())
    if not isinstance(value, list) or not 1 <= len(value) <= 256:
        raise ValueError('INPUT-SCRIPT: expected 1..256 steps')
    for step in value:
        if not isinstance(step, dict):
            raise ValueError('INPUT-SCRIPT: each step must be an object')
        if set(step) == {'key', 'down'} and isinstance(step['key'], str) and type(step['down']) is bool:
            continue
        if set(step) == {'button', 'down'} and type(step['button']) is int and 1 <= step['button'] <= 5 and type(step['down']) is bool:
            continue
        if set(step) == {'move'} and isinstance(step['move'], list) and len(step['move']) == 2 and all(type(v) is int and 0 <= v <= 8192 for v in step['move']):
            continue
        if set(step) in ({'wait'}, {'expect'}):
            condition = step[next(iter(step))]
            if isinstance(condition, dict) and set(condition) in ({'field', 'eq'}, {'field', 'gte'}) and isinstance(condition['field'], str) and ('gte' not in condition or type(condition['gte']) in (int, float)):
                continue
        raise ValueError('INPUT-SCRIPT: use key/down, button/down, move, wait or expect')
    if not any('expect' in step for step in value) or not any(set(step) & {'key', 'button', 'move'} for step in value):
        raise ValueError('INPUT-EVIDENCE: event injection needs at least one authoritative state assertion')
    return value


def matches(report, condition):
    value = report
    for key in condition['field'].split('.'):
        if not isinstance(value, dict) or key not in value:
            return False
        value = value[key]
    if 'eq' in condition:
        return value == condition['eq']
    return isinstance(value, (int, float)) and value >= condition['gte']


def drive_input(script, command, timeout):
    steps = input_steps(script)
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=None, text=True)
    latest, lock = {}, threading.Lock()
    assertions = []
    def read():
        for line in process.stdout:
            try:
                report = json.loads(line)
                if isinstance(report, dict) and report.get('ready'):
                    with lock:
                        latest.clear()
                        latest.update(report)
                else:
                    print(line, end='', flush=True)
            except (ValueError, TypeError):
                print(line, end='', flush=True)
    reader = threading.Thread(target=read, daemon=True)
    reader.start()
    deadline = time.monotonic() + timeout
    injector = None
    try:
        def wait(condition):
            condition_deadline = min(deadline, time.monotonic() + 15)
            while time.monotonic() < condition_deadline:
                with lock:
                    if latest.get('verified'):
                        raise ValueError('INPUT-REPLAY: scripted simulation cannot certify native controls')
                    if matches(latest, condition):
                        return
                if process.poll() is not None:
                    raise ValueError('INPUT-EXIT: game ended before its authoritative assertion passed')
                time.sleep(0.02)
            raise ValueError('INPUT-TIMEOUT: authoritative assertion did not pass: ' + json.dumps(condition))
        wait({'field': 'ready', 'eq': True})
        injector = NativeInput()
        for index, step in enumerate(steps):
            if 'wait' in step:
                wait(step['wait'])
                assertions.append({'step': index, 'wait': step['wait'], 'passed': True})
            elif 'expect' in step:
                with lock:
                    if latest.get('verified'):
                        raise ValueError('INPUT-REPLAY: scripted simulation cannot certify native controls')
                    passed = matches(latest, step['expect'])
                if not passed:
                    raise ValueError(f'INPUT-ASSERT: step {index} failed: {step["expect"]}')
                assertions.append({'step': index, 'expect': step['expect'], 'passed': True})
            else:
                injector.send(step)
        if not any('expect' in step for step in steps) or not any(set(step) & {'key', 'button', 'move'} for step in steps):
            raise ValueError('INPUT-EVIDENCE: event injection needs at least one authoritative state assertion')
        with lock:
            if latest.get('verified'):
                raise ValueError('INPUT-REPLAY: scripted simulation cannot certify native controls')
            evidence = {'native_input': 'passed', 'source': 'X11 XTest', 'physical_hardware': False,
                        'steps': len(steps), 'assertions': assertions, 'final': dict(latest)}
        output = Path(command[command.index('--capture') + 1]) / 'input-evidence.json'
        output.write_text(json.dumps(evidence, indent=2) + '\n')
        print(json.dumps({'native_input': 'passed', 'evidence': str(output),
                          'tick': evidence['final'].get('tick'), 'physical_hardware': False}), flush=True)
        return process.wait(timeout=max(0.1, deadline - time.monotonic()))
    finally:
        if injector:
            injector.close()
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        reader.join(timeout=3)
        process.stdout.close()


def main(argv):
    if argv and argv[0] == '--drive-input':
        try:
            return drive_input(argv[1], argv[3:], int(argv[2]))
        except (ValueError, OSError, subprocess.TimeoutExpired) as error:
            print(str(error), file=sys.stderr)
            return 4
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
    p.add_argument('--input', type=Path, help='X11 native device script with authoritative state assertions')
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
    if args.input:
        try:
            input_steps(args.input)
            if any(arg.split('=', 1)[0] in ('--verify', '--script', '--replay', '--autopilot') for arg in extra):
                raise ValueError('INPUT-REPLAY: remove simulation replay flags for native input validation')
        except (ValueError, OSError) as error:
            print(str(error), file=sys.stderr)
            return 4
    command = build_command(game, args.frames, args.size, out, extra)
    if args.input:
        command = command[:4] + [sys.executable, str(Path(__file__).resolve()), '--drive-input',
                                  str(args.input.resolve()), str(args.timeout), *command[4:], '--input-report']
    env = {**os.environ, 'LIBGL_ALWAYS_SOFTWARE': '1'}
    try:
        # Allow the inner input driver to release devices and terminate its child on timeout.
        done = subprocess.run(command, env=env, capture_output=True, text=True, timeout=args.timeout + (10 if args.input else 0))
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
