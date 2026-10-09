#!/usr/bin/env python3
"""Check local Markdown file links, including archive links, without network access."""
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]


def broken_links(root=ROOT):
    listed = subprocess.check_output(['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=root)
    errors = []
    for name in sorted(set(listed.decode().split('\0'))):
        path = root / name
        if path.suffix.lower() != '.md' or not path.is_file():
            continue
        raw = path.read_bytes()
        try:
            text = raw.decode('utf-8')
        except UnicodeDecodeError:
            text = raw.decode('cp1252')
        text = re.sub(r'(?m)^(```|~~~).*?\n.*?^\1[^\n]*$', '', text, flags=re.S)
        links = re.findall(r'!?\[[^\]\n]*\]\(([^)\n]+)\)', text)
        links += re.findall(r'^\s*\[[^\]]+\]:\s*(\S+)', text, flags=re.M)
        for link in links:
            target = link.strip().split(' "')[0].strip('<>')
            target = unquote(target.split('#')[0].split('?')[0])
            if not target or re.match(r'[a-zA-Z][\w+.-]*:', target) or target.startswith('//') or any(c in target for c in '{}*'):
                continue
            resolved = root / target.lstrip('/') if target.startswith('/') else path.parent / target
            if not resolved.exists():
                errors.append(f'{name}: missing {target}')
    return errors


if __name__ == '__main__':
    errors = broken_links()
    print('\n'.join(errors) if errors else 'Markdown file links: OK')
    sys.exit(bool(errors))
