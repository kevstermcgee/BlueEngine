#!/usr/bin/env python3
"""Package already-built optimized Windows binaries and sandbox content, without installation."""
import argparse
import hashlib
import json
import shutil
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    out=args.output.resolve()
    if out.exists():raise SystemExit('Output directory must be new')
    binaries=[ROOT/'target/release'/name for name in ['blueengine-sandbox.exe','be2.exe','be2-tools.exe']]
    for binary in binaries:
        if not binary.is_file():raise SystemExit(f'Missing release build: {binary}')
    out.mkdir(parents=True)
    for binary in binaries:shutil.copy2(binary,out/binary.name)
    # Keep source provenance paths meaningful in the packaged browser as well.
    shutil.copytree(ROOT/'assets',out/'assets')
    shutil.copy2(ROOT/'LICENSE',out/'LICENSE')
    shutil.copy2(ROOT/'assets/games/blueengine-sandbox/README.md',out/'README.md')
    validation=ROOT/'assets/games/blueengine-sandbox/VALIDATION.md'
    if validation.exists():shutil.copy2(validation,out/'VALIDATION.md')
    (out/'Launch BlueEngineSandbox.cmd').write_text('@echo off\ncd /d "%~dp0"\nstart "" "blueengine-sandbox.exe"\n',encoding='utf-8')
    hashes={p.relative_to(out).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(out.rglob('*')) if p.is_file()}
    (out/'SHA256.json').write_text(json.dumps(hashes,indent=2)+'\n')
    print(json.dumps({'ok':True,'output':str(out),'files':len(hashes)}))
if __name__=='__main__':main()
