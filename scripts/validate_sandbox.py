#!/usr/bin/env python3
"""Native audits and authored traversal routes, with a persistent JSON report."""
import argparse
import json
import subprocess
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]
def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--tool',type=Path,default=ROOT/'target/release/be2-tools.exe')
    p.add_argument('--report',type=Path,default=ROOT/'.be2-work/sandbox-validation.json')
    args=p.parse_args()
    catalog=json.loads((ROOT/'assets/games/blueengine-sandbox/catalog.json').read_text())
    results=[]
    def run(*command):
        proc=subprocess.run([str(args.tool.resolve()),*command],cwd=ROOT,capture_output=True,text=True,timeout=120)
        try: data=json.loads(proc.stdout)
        except ValueError: data={'output':proc.stdout,'error':proc.stderr}
        results.append({'command':list(command),'passed':proc.returncode==0 and data.get('ok',False),'result':data})
    for a in catalog['assets']:run('audit',a['map'])
    for m in catalog['maps']:
        if m['path'].startswith('builtin:'):continue
        run('audit',m['path'])
        route=ROOT/'assets/games/blueengine-sandbox/routes'/f"{m['id']}.json"
        if route.is_file():run('route',m['path'],str(route))
    args.report.parent.mkdir(parents=True,exist_ok=True)
    args.report.write_text(json.dumps(results,indent=2)+'\n')
    failed=[r for r in results if not r['passed']]
    print(json.dumps({'checks':len(results),'failed':failed,'report':str(args.report)},indent=2))
    raise SystemExit(bool(failed))
if __name__=='__main__':main()
