"""Run source-free workflows against a freshly built native tool, never a stale package."""
import json
import os
from pathlib import Path
import subprocess
import sys

if __name__ == '__main__':
    root = Path(__file__).resolve().parents[1]
    subprocess.run(['cargo', 'build', '--locked', '--no-default-features', '--bin', 'be2-tools'],
                   cwd=root, check=True)
    metadata = subprocess.run(['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1'],
                              cwd=root, check=True, capture_output=True, text=True)
    target = Path(json.loads(metadata.stdout)['target_directory'])
    env = os.environ.copy()
    env['BE2_TOOLS'] = str(target / 'debug' / ('be2-tools.exe' if os.name == 'nt' else 'be2-tools'))
    subprocess.run([sys.executable, '-m', 'unittest', 'discover', '-s', 'tools',
                    '-p', 'test_author.py', '-v'], cwd=root, env=env, check=True)
