"""Verify native authoring with fresh tools in the shared check profile (itest by default)."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import workflow

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', choices=workflow.TEST_PROFILES, default='itest')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    subprocess.run(['cargo', 'build', '--locked', *workflow.profile_flags(args.profile),
                    '--no-default-features', '--bin', 'be2-tools'],
                   cwd=root, check=True, stdout=sys.stdout, stderr=sys.stderr,
                   **workflow.console_options())
    metadata = subprocess.run(['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1'],
                              cwd=root, check=True, capture_output=True, text=True,
                              **workflow.console_options())
    target = Path(json.loads(metadata.stdout)['target_directory'])
    env = os.environ.copy()
    env['BE2_TOOLS'] = str(target / ('debug' if args.profile == 'dev' else args.profile)
                          / ('be2-tools.exe' if os.name == 'nt' else 'be2-tools'))
    subprocess.run([sys.executable, '-m', 'unittest', 'tools.test_author',
                    'tools.test_game_check', '-v'], cwd=root, env=env, check=True,
                   stdout=sys.stdout, stderr=sys.stderr,
                   **workflow.console_options())
