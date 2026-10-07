#!/usr/bin/env python3
"""Compatibility diagnostic for retired browser gameplay; never installs/builds/publishes."""
import json
import sys


def main(argv=None):
    print(json.dumps({'schema_version': 1, 'ok': False, 'code': 'BROWSER-RETIRED',
                      'requested': list(sys.argv[1:] if argv is None else argv),
                      'reason': 'Browser gameplay/WASM builds and publication are retired.',
                      'next': 'Use be2.py start "<objective>" --kind new-game --target windows; existing games use scripts/ship.py ship on Windows.',
                      'reference': 'docs/BROWSER_WORKFLOW.md', 'builds_triggered': 0}))
    return 2


if __name__ == '__main__':
    raise SystemExit(main())
