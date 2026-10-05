#!/usr/bin/env python3
"""Supported 2D web workflow: propose, build, verify, publish. Build is network-independent.

Requires installed Cargo dependencies/wasm target, Chromium, Node and ws for browser checks.
Publisher contract: receive a verified static package + metadata, install it, return a receipt.
"""
import argparse
import contextlib
import functools
import hashlib
import html
import http.server
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import tomllib

ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT))
SCHEMA=1
class WebError(ValueError): pass
def run(args, cwd=None, env=None):
    result=subprocess.run([str(a) for a in args],cwd=cwd,env=env,capture_output=True,text=True)
    if result.returncode: raise WebError(f"Command failed ({result.returncode}): {' '.join(map(str,args))}\n{result.stderr[-5000:]}\n{result.stdout[-1000:]}")
    return result.stdout
def source_hash(folder):
    digest=hashlib.sha256()
    for root in ('src','assets','Cargo.toml','Cargo.lock','game.project.json'):
        path=folder/root
        for p in sorted(path.rglob('*')) if path.is_dir() else [path]:
            if p.is_file():
                digest.update(p.relative_to(folder).as_posix().encode());digest.update(p.read_bytes())
    return digest.hexdigest()
def git_revision(folder):
    try:return run(['git','rev-parse','HEAD'],cwd=folder).strip()
    except WebError:return 'source-sha256:'+source_hash(folder)
def safe_file(root,name):
    if not isinstance(name,str) or not name or '\\' in name or ':' in name or name.startswith('/') or any(p in ('','.','..') or p.endswith(('.', ' ')) or any(c in p for c in '<>"|?*') or any(ord(c)<32 for c in p) or re.fullmatch(r'(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\..*)?',p,re.I) for p in name.split('/')):
        raise WebError(f"Unsafe packaged path {name!r}: use portable relative paths")
    for part in Path(name).parts:
        root=root/part
        if root.is_symlink():raise WebError(f"Packaged symlink forbidden: {name}")
    return root
# Use the same validation source that generated native game tooling carries.
import importlib.util
_spec=importlib.util.spec_from_file_location('be2_project', ROOT/'templates/game_project.py')
_project=importlib.util.module_from_spec(_spec);_spec.loader.exec_module(_project)
def validate_project(game):
    try:return _project.validate_project(game)
    except _project.ProjectError as error:raise WebError(str(error)) from error
def integrity(dist):
    if dist.is_symlink():raise WebError('Web package directory must not be a symlink')
    m=json.loads(safe_file(dist,'manifest.json').read_text())
    if not isinstance(m,dict):raise WebError('Web manifest must contain a JSON object')
    if m.get('schema_version')!=SCHEMA or m.get('presentation')!='2d' or m.get('networking')!='offline':raise WebError('Unsupported web manifest requirements')
    if m.get('runtime_abi')!=1 or not isinstance(m.get('id'),str) or not re.fullmatch('[a-z][a-z0-9-]{0,47}',m['id']):raise WebError('Invalid manifest runtime ABI/game ID')
    for field in ('title','description','engine_revision','game_revision'):
        if not isinstance(m.get(field),str) or not m[field]:raise WebError(f'Manifest requires {field}')
    if not isinstance(m.get('targets'),list) or 'web' not in m['targets'] or any(t not in ('web','linux','windows','macos') for t in m['targets']):raise WebError('Manifest must declare supported web targets')
    if not isinstance(m.get('input'),list) or not m['input'] or any(i not in ('keyboard','mouse','controller') for i in m['input']):raise WebError('Manifest must declare supported input')
    proof=m.get('verification',{})
    if not isinstance(proof,dict) or proof.get('outcome')!='won' or not isinstance(proof.get('hash'),str) or not re.fullmatch('[0-9a-f]{16}',proof['hash']):raise WebError('Manifest requires a winning headless verification hash/outcome')
    if m.get('thumbnail')!='thumbnail.png' or m.get('play')!='index.html' or not isinstance(m.get('compatibility'),dict):raise WebError('Manifest thumbnail/play/compatibility contract is invalid')
    files=m.get('file_sha256',{})
    if not isinstance(files,dict) or not {'index.html','game.wasm','loader.js','platform.js','thumbnail.png'}<=set(files):raise WebError('Incomplete web manifest: index, wasm, loader, platform and thumbnail are required')
    if len({n.casefold() for n in files})!=len(files):raise WebError('Case-colliding packaged paths')
    for name,digest in files.items():
        p=safe_file(dist,name)
        if name=='manifest.json' or not re.fullmatch('[0-9a-f]{64}',str(digest)):raise WebError(f'Invalid hash declaration for {name}')
        if not p.is_file():raise WebError(f'Missing packaged file: {name}; source files never satisfy the package')
        if hashlib.sha256(p.read_bytes()).hexdigest()!=digest:raise WebError(f'Packaged file hash mismatch: {name}; rebuild the web package')
    actual=set()
    for p in dist.rglob('*'):
        if p.is_symlink():raise WebError(f'Packaged symlink forbidden: {p}')
        if p.is_file():actual.add(p.relative_to(dist).as_posix())
    if actual!=set(files)|{'manifest.json'}:raise WebError(f'Undeclared package files: {sorted(actual-set(files)-{"manifest.json"})}')
    if safe_file(dist,'game.wasm').read_bytes()[:8]!=b'\0asm\x01\0\0\0':raise WebError('Invalid WASM header')
    return m
class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self,*args):pass
def browser_verify(dist, evidence):
    manifest=integrity(dist);evidence.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='be2-web-isolated-') as folder:
        isolated=Path(folder)
        for name in list(manifest['file_sha256'])+['manifest.json']:
            target=safe_file(isolated,name);target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(safe_file(dist,name),target)
        handler=functools.partial(QuietHandler,directory=str(isolated))
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),handler)
        thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
        try:
            run(['node',ROOT/'tools/browser_smoke.mjs',f'http://127.0.0.1:{server.server_port}/',evidence/'browser.json',evidence/'browser.png'])
        finally:server.shutdown();server.server_close();thread.join()
    report=json.loads((evidence/'browser.json').read_text())
    if not report['ok']:raise WebError('Browser smoke failed; inspect browser.json')
    return report
def build(game, skip_browser=False):
    start=time.monotonic();project=validate_project(game)
    if 'web' not in project['targets']:raise WebError('This game does not declare web; edit the proposal/project requirements deliberately before building')
    cargo=tomllib.loads((game/'Cargo.toml').read_text());name=cargo['package']['name']
    if name!=project['id']:raise WebError('game.project id must match Cargo package name')
    identity=json.loads((game/'assets/identity.json').read_text())
    if not identity.get('title') or not identity.get('controls'):raise WebError('identity.json requires title and controls')
    target=Path(os.environ.get('CARGO_TARGET_DIR',ROOT/'target')).resolve()
    env={**os.environ,'CARGO_TARGET_DIR':str(target),'RUSTC_WRAPPER':'','CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS':'-C link-arg=--allow-undefined'}
    evidence=game/'.blue-check/web';evidence.mkdir(parents=True,exist_ok=True)
    # Settles the starter's seeded lock file, then every actual build is locked.
    run(['cargo','metadata','--offline','--format-version','1'],cwd=game,env=env)
    env['BE2_VERIFY_REPORT']=str(evidence/'native-state.json')
    state=evidence/'native-state.json';state.unlink(missing_ok=True)
    tests=run(['cargo','test','--offline','--locked','--no-default-features'],cwd=game,env=env)
    (evidence/'headless.log').write_text(tests)
    if not state.is_file():raise WebError('Tests must export BE2_VERIFY_REPORT using two_d::verify, including expected hash/outcome. Copy the two-d starter verification test.')
    expected=json.loads(state.read_text())
    if expected.get('outcome')!='won' or not re.fullmatch('[0-9a-f]{16}',expected.get('hash','')):raise WebError('The headless public-input verification route must win and emit its state hash')
    run(['cargo','build','--offline','--locked','--release','--target','wasm32-unknown-unknown'],cwd=game,env=env)
    metadata=json.loads(run(['cargo','metadata','--offline','--locked','--format-version','1'],cwd=game,env=env))
    macro=next(p for p in metadata['packages'] if p['name']=='macroquad')
    if macro['version']!='0.4.14':raise WebError('Loader pin supports Macroquad 0.4.14 only; update loader compatibility before changing it')
    mini=next(p for p in metadata['packages'] if p['name']=='miniquad')
    sound=next(p for p in metadata['packages'] if p['name']=='quad-snd')
    loader_bytes=(Path(mini['manifest_path']).parent/'js/gl.js').read_bytes()+b'\n(function(){\n'+(Path(sound['manifest_path']).parent/'js/audio.js').read_bytes()+b'\n})();\n'
    destination=game/'dist/web';destination.parent.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='web-build-',dir=destination.parent) as stage:
        out=Path(stage)
        shutil.copy2(target/'wasm32-unknown-unknown/release'/f'{name.replace("-","_")}.wasm',out/'game.wasm') if (target/'wasm32-unknown-unknown/release'/f'{name.replace("-","_")}.wasm').exists() else shutil.copy2(target/'wasm32-unknown-unknown/release'/f'{name}.wasm',out/'game.wasm')
        (out/'loader.js').write_bytes(loader_bytes);shutil.copy2(ROOT/'templates/web/platform.js',out/'platform.js');shutil.copy2(game/'assets/icon.png',out/'thumbnail.png')
        page=(ROOT/'templates/web/index.html').read_text()
        for key,value in {'title':identity['title'],'description':project['description'],'controls':identity['controls']}.items():page=page.replace('{{'+key+'}}',html.escape(value,quote=True))
        (out/'index.html').write_text(page)
        for name in identity.get('package',[]):
            source=safe_file(game,name)
            if not source.is_file():raise WebError(f'Web extra asset must be a declared regular file: {name}')
            if name in ('manifest.json','game.wasm','index.html','loader.js','platform.js','thumbnail.png'):raise WebError(f'Extra asset collides with reserved package file: {name}; place it under assets/')
            target_file=safe_file(out,name);target_file.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,target_file)
        epoch=int(os.environ.get('SOURCE_DATE_EPOCH',run(['git','show','-s','--format=%ct','HEAD'],cwd=ROOT).strip()))
        manifest={'schema_version':SCHEMA,'runtime_abi':1,'id':project['id'],'title':identity['title'],'description':project['description'],'engine_revision':git_revision(ROOT),'game_revision':git_revision(game),'game_source_sha256':source_hash(game),'presentation':project['presentation'],'targets':project['targets'],'input':project['input'],'networking':project['networking'],'built_at_epoch':epoch,'timestamp_policy':'SOURCE_DATE_EPOCH or source commit time for reproducible packaging','thumbnail':'thumbnail.png','play':'index.html','native_download':None,'compatibility':{'macroquad':macro['version'],'miniquad':mini['version'],'quad-snd':sound['version'],'loader_sha256':hashlib.sha256(loader_bytes).hexdigest(),'webgl':'WebGL 1','save_frame':1,'storage':'localStorage per origin + game ID; 4 MiB limit; Snapshot version/migrations'},'verification':expected,'file_sha256':{p.relative_to(out).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(out.rglob('*')) if p.is_file()}}
        (out/'manifest.json').write_text(json.dumps(manifest,indent=2,sort_keys=True)+'\n');integrity(out)
        if not skip_browser:browser_verify(out,evidence)
        backup=out/'previous'
        if destination.exists():
            integrity(destination) # do not replace an unrelated directory
            destination.rename(backup)
        try:
            new=out/'installed';new.mkdir()
            for name in list(manifest['file_sha256'])+['manifest.json']:
                path=safe_file(new,name);path.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(safe_file(out,name),path)
            new.rename(destination)
        except Exception:
            if backup.exists():backup.rename(destination)
            raise
    return {'ok':True,'package':str(destination),'browser':not skip_browser,'seconds':round(time.monotonic()-start,3),'manifest':manifest,'evidence':str(evidence)}
def directory_publish(package,destination):
    manifest=integrity(package);destination.mkdir(parents=True,exist_ok=True)
    slot=safe_file(destination,manifest['id'])
    if slot.exists() and (not (slot/'manifest.json').is_file() or integrity(slot)['id']!=manifest['id']):raise WebError('Refusing to replace an unrelated deployment directory')
    with tempfile.TemporaryDirectory(prefix='.deploy-',dir=destination) as stage:
        new=Path(stage)/'game';shutil.copytree(package,new);integrity(new)
        old=Path(stage)/'old'
        if slot.exists():slot.rename(old)
        try:new.rename(slot)
        except Exception:
            if old.exists():old.rename(slot)
            raise
    catalog=[]
    for p in sorted(destination.glob('*/manifest.json')):
        m=integrity(p.parent);catalog.append({k:m[k] for k in ('id','title','description','engine_revision','game_revision','presentation','networking','input','targets','built_at_epoch','compatibility')}|{'play':m['id']+'/index.html','thumbnail':m['id']+'/thumbnail.png','native_download':m['native_download']})
    temp=destination/'catalog.json.tmp';temp.write_text(json.dumps({'schema_version':1,'games':catalog},indent=2)+'\n');temp.replace(destination/'catalog.json')
    cards=''.join(f'<li><a href="{html.escape(g["play"])}">{html.escape(g["title"])}</a> — 2D · singleplayer<p>{html.escape(g["description"])}</p></li>' for g in catalog)
    (destination/'index.html').write_text('<!doctype html><meta charset="utf-8"><title>BlueEngine browser games</title><h1>Play in browser</h1><ul>'+cards+'</ul>')
    return {'backend':'directory','deployed':str(slot),'url':None,'catalog':str(destination/'catalog.json'),'remaining_external_step':'Serve this directory through your static host; each game is under /GAME_ID/. No external URL has been created.'}
def main(argv=None):
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('command',choices=['build','verify','publish','propose']);p.add_argument('game');p.add_argument('--skip-browser',action='store_true',help='Build only: explicitly unverified, cannot publish');p.add_argument('--backend',choices=['directory','github-pages'],default='directory');p.add_argument('--destination');p.add_argument('--repository');args=p.parse_args(argv)
    try:
        if args.command=='propose':
            result={'title':args.game,'presentation':'2d','gameplay':'arcade or light strategy','session_minutes':10,'input':['keyboard','mouse'],'networking':'offline','targets':['web','windows'],'complexity':'low','next':'Adjust this proposal, then create with be2-tools new-game NAME DIR ENGINE two-d. Project requirements are editable before build.'}
        else:
            game=Path(args.game).resolve()
            if args.command=='verify':result={'ok':True,'browser':browser_verify(game/'dist/web',game/'.blue-check/web')}
            else:
                if args.command=='publish':
                    if args.skip_browser:raise WebError('Publish always requires a fresh browser check; --skip-browser is build-only')
                    if args.backend=='directory' and not args.destination:raise WebError('Directory publishing needs --destination /path/to/static-library')
                    if args.backend=='github-pages' and not args.repository:raise WebError('GitHub Pages publishing needs --repository OWNER/REPO')
                result=build(game,args.skip_browser)
                if args.command=='publish':
                    if args.backend=='directory':result['publication']=directory_publish(game/'dist/web',Path(args.destination).resolve())
                    else:
                        from tools.web_publish import publish
                        result['publication']=publish(game/'dist/web',args.repository,run,integrity,directory_publish)
        print(json.dumps(result));return 0
    except (WebError,OSError,ValueError,KeyError,StopIteration) as e:print(json.dumps({'ok':False,'error':str(e)}),file=sys.stderr);return 1
if __name__=='__main__':sys.exit(main())
