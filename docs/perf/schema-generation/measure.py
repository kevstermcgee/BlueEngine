"""Measure the normal schema front door; one priming run and three warm runs.

Run at the engine root. Respect CARGO_TARGET_DIR/TMPDIR. No installations;
each operation executes real Cargo/generation/fixture checks, not cached evidence.
"""
import json
import subprocess
import sys
import time


def measure(operation, count):
    rows = []
    for attempt in range(count):
        command = [sys.executable, 'tools/be2.py', 'schemas', '--'+operation]
        started = time.monotonic()
        result = subprocess.run(command, capture_output=True, text=True)
        packet = json.loads(result.stdout)
        rows.append({'argv':command,'cache_condition':'priming' if attempt == 0 else 'warm',
                     'seconds':time.monotonic()-started,'returncode':result.returncode,
                     'output_bytes':len(result.stdout.encode('utf-8')),'packet':packet})
        if result.returncode:
            break
    return rows


if __name__ == '__main__':
    rows = measure('write',4)+measure('check',4)
    print(json.dumps({'schema_version':1,'runs':rows},indent=2))
    raise SystemExit(any(row['returncode'] for row in rows))
