"""CI-only stock audio playback/capture on an isolated X display and null ALSA sink.

Never launches a game on Windows/the user's desktop. Requires explicit CI opt-in.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    if sys.platform != 'linux' or os.environ.get('CI') != 'true' or os.environ.get('BE2_AUDIO_OFFSCREEN') != '1' or not os.environ.get('DISPLAY'):
        parser.error('requires Linux CI, BE2_AUDIO_OFFSCREEN=1 and an isolated virtual DISPLAY')
    root = Path(__file__).resolve().parents[1]
    fixture = root / 'assets/games/observatory/content'
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    # Child games see only a null ALSA device. No desktop sound server or physical sink.
    alsa = output / 'null-alsa.conf'
    alsa.write_text('pcm.!default { type null }\n', encoding='utf-8')
    env = {**os.environ, 'ALSA_CONFIG_PATH': str(alsa), 'LIBGL_ALWAYS_SOFTWARE': '1'}
    binary = args.binary.resolve()
    def run(label, game, scenario=None, mute=False):
        capture = output / label
        command = [str(binary), '--game', str(game), '--capture', str(capture), '--settings', str(output / 'settings.json')]
        if scenario:
            command += ['--scenario', str(scenario)]
        if mute:
            command += ['--mute']
        result = subprocess.run(command, cwd=root, env=env, text=True, capture_output=True, timeout=180)
        (output / f'{label}.log').write_text(result.stdout + result.stderr, encoding='utf-8')
        return result, capture

    positive, capture = run('positive', fixture / 'game-audio.json', fixture / 'audio-loss-restart-win.json')
    if positive.returncode:
        raise RuntimeError(f'stock playback failed ({positive.returncode}): {positive.stderr[-1500:]}')
    rows = json.loads((capture / 'run.json').read_text())
    assert len(rows) == 1543 and rows[-1]['completed'] and rows[-1]['round'] == 1
    assert all(row['audio']['state'] == 'ready' and row['audio']['submitted'] for row in rows)
    events = [(row['tick'], cue) for row in rows for cue in row['audio']['cues']]
    assert len(events) == 8, 'no reset-counter cues or repeated threshold alarms'
    assert sum(cue == 'step' for _, cue in events) == 3
    assert sum(cue == 'success' for _, cue in events) == 1
    assert (900, 'battery') in events and (1200, 'battery') in events and (1201, 'shutter') in events
    assert events[-1] == (1539, 'success')
    assert abs(max(row['audio']['layers']['signal'] for row in rows) - 0.85) < 1e-6
    for file in ('world.png', 'loss-0.png', 'reset-1.png', 'win-1.png', 'menu.png'):
        assert (capture / file).stat().st_size > 1000, file

    # Copy only this fixture to a new output, corrupt one runtime artifact, and require failure.
    broken = output / 'broken'
    broken.mkdir()
    shutil.copy2(fixture / 'game-audio.json', broken / 'game.json')
    shutil.copy2(fixture / 'edited-map.json', broken / 'edited-map.json')
    shutil.copytree(fixture / 'audio', broken / 'audio')
    (broken / 'audio/music-signal.wav').unlink()
    negative, negative_capture = run('negative', broken / 'game.json')
    assert negative.returncode != 0 and 'music-signal.wav' in negative.stderr, negative.stderr
    assert not negative_capture.exists(), 'failed startup must not create a success capture'
    muted, muted_capture = run('muted', broken / 'game.json', mute=True)
    assert muted.returncode == 0, muted.stderr
    muted_rows = json.loads((muted_capture / 'run.json').read_text())
    assert all(row['audio']['state'] == 'muted' and not row['audio']['submitted'] for row in muted_rows)
    evidence = {'ok': True, 'platform': 'Linux CI virtual display / software GL / null ALSA',
                'audibility_verified': False, 'frames': len(rows), 'cue_events': events,
                'negative_exit': negative.returncode, 'negative_capture': False,
                'muted_exit': muted.returncode, 'muted_skips_missing_assets': True}
    (output / 'evidence.json').write_text(json.dumps(evidence, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(evidence))


if __name__ == '__main__':
    main()
