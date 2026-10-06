#!/usr/bin/env python3
"""Canonical portable browser workflow: build, verify, inspect, serve, publish and reproduce.

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
from tools import web_release as release
class WebError(ValueError): pass
def run(args, cwd=None, env=None):
    result=subprocess.run([str(a) for a in args],cwd=cwd,env=env,capture_output=True,text=True)
    if result.returncode: raise WebError(f"Command failed ({result.returncode}): {' '.join(map(str,args))}\n{result.stderr[-5000:]}\n{result.stdout[-1000:]}")
    return result.stdout
def source_hash(folder):
    digest=hashlib.sha256()
    inputs=('src','assets','scripts','build.rs','AUDIO.md','Cargo.toml','Cargo.lock','game.project.json')
    try:
        names=run(['git','ls-files','--cached','--others','--exclude-standard','-z','--',*inputs],cwd=folder).split('\0')
    except WebError:
        # A local scaffold needs neither Git initialization nor a hosting account.
        names=[]
    # Actual inputs still matter inside an ignored scratch project in a Git checkout.
    # Tracked files remain included; ignore only untracked Python interpreter outputs.
    for name in inputs:
        path=safe_file(folder,name)
        names.extend(p.relative_to(folder).as_posix() for p in (path.rglob('*') if path.is_dir() else [path])
                     if p.is_file() and '__pycache__' not in p.parts and p.suffix not in ('.pyc','.pyo'))
    for name in sorted(set(n for n in names if n)):
        path=safe_file(folder,name)
        if path.is_file():digest.update(name.encode());digest.update(path.read_bytes())
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
    m=json.loads(safe_file(dist,'manifest.json').read_text(encoding='utf-8'))
    if not isinstance(m,dict):raise WebError('Web manifest must contain a JSON object')
    if m.get('schema_version')!=SCHEMA or m.get('presentation') not in ('2d','3d','hybrid') or m.get('networking')!='offline':raise WebError('Unsupported web manifest requirements')
    if m.get('runtime_abi') not in (1,2) or not isinstance(m.get('id'),str) or not re.fullmatch('[a-z][a-z0-9-]{0,47}',m['id']):raise WebError('Invalid manifest runtime ABI/game ID')
    for field in ('title','description','engine_revision','game_revision'):
        if not isinstance(m.get(field),str) or not m[field]:raise WebError(f'Manifest requires {field}')
    if not isinstance(m.get('targets'),list) or 'web' not in m['targets'] or any(t not in ('web','linux','windows','macos') for t in m['targets']):raise WebError('Manifest must declare supported web targets')
    if not isinstance(m.get('input'),list) or not m['input'] or any(i not in ('keyboard','mouse','controller') for i in m['input']):raise WebError('Manifest must declare supported input')
    proof=m.get('verification',{})
    validate_proof(proof)
    if m.get('thumbnail')!='thumbnail.png' or m.get('play')!='index.html' or not isinstance(m.get('compatibility'),dict):raise WebError('Manifest thumbnail/play/compatibility contract is invalid')
    files=m.get('file_sha256',{})
    if m.get('runtime_abi')==2:
        if not {'mobile.js','service-worker.js','app.webmanifest'} <= set(files):raise WebError('Portable package needs mobile controls and offline installation files')
        if m.get('mobile_controls',{}).get('layout') not in ('dpad','paddle','tap'):raise WebError('Manifest needs a supported mobile control layout')
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
    if 'package_id' in m and m['package_id']!=release.package_id(files):raise WebError('Package/build ID does not match runtime hashes')
    return m
def validate_proof(proof):
    if not isinstance(proof,dict) or not isinstance(proof.get('hash'),str) or not re.fullmatch('[0-9a-f]{16}',proof['hash']):raise WebError('Headless verification requires a complete state hash')
    if proof.get('outcome')=='won':return
    if proof.get('outcome')=='playing' and type(proof.get('ticks')) is int and 1<=proof['ticks']<=100000 and isinstance(proof.get('purpose'),str) and proof['purpose'].strip():return
    raise WebError('Verification must win, or explicitly document an open-ended playing route with positive ticks and purpose; losing/empty routes cannot publish')

class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self,*args):pass

class VerificationHandler(QuietHandler):
    """Only the temporary, loopback verifier exposes an interrupted-deployment fixture."""
    def do_POST(self):
        if self.path not in ('/__be2_update','/__be2_complete'):
            self.send_error(404);return
        root=Path(self.directory)
        if self.path=='/__be2_update':
            self.server.original_thumbnail=(root/'thumbnail.png').read_bytes()
            (root/'index.html').write_text((root/'index.html').read_text(encoding='utf-8')+'\n<!-- verified upgrade fixture -->\n', encoding='utf-8', newline='\n')
            write_worker(root)
            manifest=json.loads((root/'manifest.json').read_text(encoding='utf-8'))
            manifest['file_sha256']={p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(root.rglob('*')) if p.is_file() and p.name!='manifest.json'}
            manifest['package_id']=release.package_id(manifest['file_sha256']);write_manifest(root,manifest)
            # HTTP 200 but wrong content: interrupted static deployment must not activate.
            (root/'thumbnail.png').write_bytes(b'incomplete deployment')
        else:(root/'thumbnail.png').write_bytes(self.server.original_thumbnail)
        self.send_response(200);self.send_header('Content-Type','application/json');self.end_headers()
        self.wfile.write(json.dumps({'package_id':json.loads((root/'manifest.json').read_text(encoding='utf-8'))['package_id']}).encode())

def capabilities():
    return {'ok':True,'presentation':['2d','hybrid','3d'],'runtime':'portable','networking':['offline'],
            'required':['WebAssembly','WebGL 1','Web Audio','localStorage','HTTPS or localhost for service worker'],
            'unsupported':['native UDP/QUIC multiplayer','native world renderer/worker APIs','browser/native save sync'],
            'automated':'Chromium software WebGL, emulated touch and synthetic standard gamepad',
            'human_required':['physical controllers','Android/iOS Safari','hardware audio','device performance']}

def control_contract():
    return json.loads((ROOT/'templates/web/controls.json').read_text(encoding='utf-8'))

def control_help(action_label='Action'):
    c=control_contract()
    def key(value):return value.removeprefix('Key').replace('Arrow','').replace('ShiftLeft','Left Shift').replace('ShiftRight','Right Shift')
    def buttons(values):
        names={0:'A',1:'B',9:'Start'}
        return [names.get(n,f'Pad button {n}') for n in values]
    def bindings(value):
        result=([key(value['key'])] if value.get('key') else [])+buttons(value.get('buttons',[]))
        if value.get('touch'):result.append('touch '+value['touch'])
        return result
    movement=', '.join(key(k) for k in c['movement']['keys'])
    lines=[f'Move: {movement} / left stick / touch movement']
    if action_label:lines.append(f"{action_label}: {key(c['primary']['key'])} / {buttons([c['primary']['button']])[0]} / touch {c['primary']['touch']}")
    lines+=['Look: '+c['look']['mouse']+' / right stick','Run: '+', '.join(key(k) for k in c['sprint']['keys'])]
    for name,value in c['commands'].items():
        inputs=bindings(value)
        if name=='pause':inputs+=bindings(c['commands']['start'])[:1]+buttons(c['commands']['start']['buttons'])
        lines.append(value['label']+': '+' / '.join(inputs))
    return ' · '.join(lines)

def write_manifest(out,manifest):
    (out/'manifest.json').write_text(json.dumps(manifest,indent=2,sort_keys=True)+'\n', encoding='utf-8', newline='\n')

def write_worker(out):
    hashes={p.relative_to(out).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(out.rglob('*')) if p.is_file() and p.name not in ('manifest.json','service-worker.js')}
    worker=(ROOT/'templates/web/service-worker.js').read_text(encoding='utf-8').replace('{{cache}}',release.package_id(hashes)).replace('{{hashes}}',json.dumps(hashes,sort_keys=True))
    (out/'service-worker.js').write_text(worker, encoding='utf-8', newline='\n')

def release_contract(game,manifest):
    engine=release.source(ROOT,['src','assets','templates','tools','Cargo.toml','Cargo.lock','build.rs','.cargo'])
    source=release.source(game,['src','assets','scripts','Cargo.toml','Cargo.lock','build.rs','game.project.json','AUDIO.md'])
    return {'package_id':release.package_id(manifest['file_sha256']),'sources':{'engine':engine,'game':source},
            'controls':control_contract(),'required_capabilities':capabilities()['required'],
            'reproduce':{'command':['python3','tools/be2.py','web','build',source['path']],
                         'prepare':['python3','tools/be2.py','web','prepare',source['path']],
                         'dependency_policy':'committed Cargo.lock; cargo fetch --locked, then offline locked builds',
                         'rustc':run(['rustc','--version']).strip(),'cargo':run(['cargo','--version']).strip(),
                         'target':'wasm32-unknown-unknown','profile':'release','path_remapping':'Cargo home=/cargo, engine=/blueengine, game=/game',
                         'compiler_namespace':'canonical package roots, version, features and codegen settings v1','runtime_wasm_removed_custom_sections':['name'],
                         'game_lock_sha256':hashlib.sha256((game/'Cargo.lock').read_bytes()).hexdigest(),
                         'source_date_epoch':manifest['built_at_epoch'],'engine_relative_to_game':os.path.relpath(ROOT,game)}}

def verification_summary(report):
    return {'compiled':True,'package_valid':True,'desktop':report['checks'],
            'mobile':report.get('mobile',{}).get('checks') if isinstance(report.get('mobile'),dict) else None,
            'environment':'Chromium CDP/software WebGL; emulated touch and synthetic controller','physical_device_tested':False}

def publication_gate(package):
    manifest=integrity(package)
    if manifest.get('package_id')!=release.package_id(manifest['file_sha256']):raise WebError('Publication requires package/build ID; rebuild legacy package')
    proof=manifest.get('browser_verification',{})
    if proof.get('compiled') is not True or proof.get('package_valid') is not True:raise WebError('Publication requires compilation and structural verification evidence')
    if type(manifest.get('built_at_epoch')) is not int or manifest['built_at_epoch']<=0:raise WebError('Publication requires a build timestamp')
    required=('wasm_instantiated','playable','input','save_write','reload_read','audio_initialized','offline_reload','gameplay_scenario','update_recovery','focus_loss','focus_return')
    for mode in ('desktop','mobile'):
        if not isinstance(proof.get(mode),dict) or any(proof[mode].get(k) is not True for k in required):raise WebError(f'Publication requires complete {mode} browser evidence; run web verify')
    if not manifest.get('reproduce') or not manifest.get('required_capabilities'):raise WebError('Publication requires reproduction instructions and runtime capabilities')
    if not re.fullmatch('[0-9a-f]{64}',str(manifest.get('game_source_sha256',''))):raise WebError('Publication requires committed game source hash')
    source=release.retrieve_sources(manifest)
    from tools.web_reproduce import reproduce
    reproduced=reproduce(package)
    manifest['reproducibility']={'clean_checkout':True,'empty_target':True,'package_hashes_match':True,'compared_fields':reproduced['compared_fields']}
    write_manifest(package,manifest)
    return source|{'clean_reproduction':reproduced}
def browser_verify(dist, evidence, *, preview=False):
    manifest=integrity(dist);evidence.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='be2-web-isolated-') as folder:
        isolated=Path(folder)
        for name in list(manifest['file_sha256'])+['manifest.json']:
            target=safe_file(isolated,name);target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(safe_file(dist,name),target)
        handler=functools.partial(VerificationHandler,directory=str(isolated))
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),handler)
        thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
        try:
            run(['node',ROOT/'tools/browser_smoke.mjs',f'http://127.0.0.1:{server.server_port}/',evidence/'browser.json',evidence/'browser.png',*(['--preview'] if preview else [])])
        finally:server.shutdown();server.server_close();thread.join()
    report=json.loads((evidence/'browser.json').read_text(encoding='utf-8'))
    if not report['ok']:raise WebError('Browser smoke failed; inspect browser.json')
    if manifest.get('runtime_abi')==2 and not preview:
        # Separate profile and real touch events: desktop keyboard evidence cannot stand in for mobile.
        with tempfile.TemporaryDirectory(prefix='be2-mobile-isolated-') as folder:
            isolated=Path(folder)
            for name in list(manifest['file_sha256'])+['manifest.json']:
                path=safe_file(isolated,name);path.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(safe_file(dist,name),path)
            server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(VerificationHandler,directory=str(isolated)))
            thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
            try:run(['node',ROOT/'tools/browser_smoke.mjs',f'http://127.0.0.1:{server.server_port}/',evidence/'mobile.json',evidence/'mobile.png','--mobile',*(['--preview'] if preview else [])])
            finally:server.shutdown();server.server_close();thread.join()
        report['mobile']=json.loads((evidence/'mobile.json').read_text(encoding='utf-8'))
    return report
def catalog_verify(site,evidence):
    evidence.mkdir(parents=True,exist_ok=True)
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(QuietHandler,directory=str(site)))
    thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
    try:run(['node',ROOT/'tools/catalog_smoke.mjs',f'http://127.0.0.1:{server.server_port}/',evidence/'catalog.json',evidence/'catalog.png'])
    finally:server.shutdown();server.server_close();thread.join()
    return json.loads((evidence/'catalog.json').read_text(encoding='utf-8'))
def build(game, skip_browser=False, *, preview=False):
    start=time.monotonic();project=validate_project(game)
    if (game/'Cargo.toml').is_symlink():raise WebError('Game Cargo.toml cannot be a symlink')
    if 'web' not in project['targets']:raise WebError('This game does not declare web; edit the proposal/project requirements deliberately before building')
    cargo=tomllib.loads((game/'Cargo.toml').read_text(encoding='utf-8'));name=cargo['package']['name']
    if name!=project['id']:raise WebError('game.project id must match Cargo package name')
    web=project.get('web_build');binary=web['binary'] if web else name
    if web:
        bins={b['name'] for b in cargo.get('bin',[])}
        if binary not in bins:raise WebError(f'web_build.binary {binary} is not a declared Cargo binary')
        missing=set(web['features'])-set(cargo.get('features',{}))
        if missing:raise WebError(f'web_build.features are not declared in Cargo.toml: {sorted(missing)}')
    identity=json.loads(safe_file(game,web.get('identity','assets/identity.json') if web else 'assets/identity.json').read_text(encoding='utf-8'))
    if not identity.get('title') or not identity.get('controls'):raise WebError('identity.json requires title and controls')
    target=Path(os.environ.get('CARGO_TARGET_DIR',ROOT/'target')).resolve()
    cargo_home=Path(os.environ.get('CARGO_HOME',Path.home()/'.cargo')).resolve()
    wasm_flags=['-C','link-arg=--allow-undefined',f'--remap-path-prefix={cargo_home}=/cargo',f'--remap-path-prefix={ROOT}=/blueengine',f'--remap-path-prefix={game}=/game']
    env={**os.environ,'CARGO_TARGET_DIR':str(target),'RUSTC_WRAPPER':'','RUSTC_WORKSPACE_WRAPPER':''}
    env.pop('CARGO_ENCODED_RUSTFLAGS',None)
    env.pop('RUSTFLAGS',None)
    evidence=game/('.blue-check/web-preview' if preview else '.blue-check/web');evidence.mkdir(parents=True,exist_ok=True)
    # All inputs, including dependency resolution, must be committed before release.
    run(['cargo','metadata','--offline','--locked','--format-version','1'],cwd=game,env=env)
    env['BE2_VERIFY_REPORT']=str(evidence/'native-state.json')
    state=evidence/'native-state.json';state.unlink(missing_ok=True)
    tests=run(['cargo','test','--offline','--locked','--no-default-features'],cwd=game,env=env)
    (evidence/'headless.log').write_text(tests, encoding='utf-8', newline='\n')
    if not state.is_file():raise WebError('Tests must export BE2_VERIFY_REPORT using two_d::verify, including expected hash/outcome. Copy the two-d starter verification test.')
    expected=json.loads(state.read_text(encoding='utf-8'))
    validate_proof(expected)
    build_args=['cargo','build','--offline','--locked','--release','--target','wasm32-unknown-unknown','--bin',binary]
    if web:build_args+=['--no-default-features','--features',','.join(web['features'])]
    # Cargo splits plain RUSTFLAGS on whitespace; encoded arguments preserve paths containing spaces.
    wrapper,wasm_target=release.compiler_wrapper(ROOT,target,run)
    run(build_args,cwd=game,env={**env,'CARGO_ENCODED_RUSTFLAGS':'\x1f'.join(wasm_flags),'CARGO_TARGET_DIR':str(wasm_target),'RUSTC_WRAPPER':str(wrapper),'BE2_WASM_ENGINE_ROOT':str(ROOT),'BE2_WASM_GAME_ROOT':str(game),'BE2_WASM_CARGO_HOME':str(cargo_home)})
    metadata=json.loads(run(['cargo','metadata','--offline','--locked','--format-version','1'],cwd=game,env=env))
    engine=next(p for p in metadata['packages'] if p['name']=='be2')
    if Path(engine['manifest_path']).resolve()!=ROOT/'Cargo.toml':raise WebError('Build must use this workflow engine checkout; invoke the dependency engine tools/be2.py')
    macro=next(p for p in metadata['packages'] if p['name']=='macroquad')
    if macro['version']!='0.4.14':raise WebError('Loader pin supports Macroquad 0.4.14 only; update loader compatibility before changing it')
    mini=next(p for p in metadata['packages'] if p['name']=='miniquad')
    sound=next(p for p in metadata['packages'] if p['name']=='quad-snd')
    loader_bytes=(Path(mini['manifest_path']).parent/'js/gl.js').read_bytes()+b'\n(function(){\n'+(Path(sound['manifest_path']).parent/'js/audio.js').read_bytes()+b'\n})();\n'
    destination=evidence/'package' if preview else game/'dist/web';destination.parent.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='web-build-',dir=destination.parent) as stage:
        out=Path(stage)
        compiled=wasm_target/'wasm32-unknown-unknown/release'/f'{binary}.wasm'
        shutil.copy2(compiled,evidence/'game.debug.wasm')
        (out/'game.wasm').write_bytes(release.runtime_wasm(compiled.read_bytes()))
        (out/'loader.js').write_bytes(loader_bytes);shutil.copy2(ROOT/'templates/web/platform.js',out/'platform.js');shutil.copy2(ROOT/'templates/web/mobile.js',out/'mobile.js');shutil.copy2(game/'assets/icon.png',out/'thumbnail.png')
        page=(ROOT/'templates/web/index.html').read_text(encoding='utf-8')
        mobile=project.get('mobile_controls',{'layout':'dpad','action_label':'Action'})
        for key,value in {'title':identity['title'],'description':project['description'],'controls':control_help(mobile['action_label'])}.items():page=page.replace('{{'+key+'}}',html.escape(value,quote=True))
        page=page.replace('{{control_config}}',json.dumps(control_contract()).replace('<','\\u003c'))
        page=page.replace('{{mobile_config}}',json.dumps(mobile).replace('<','\\u003c'))
        (out/'index.html').write_text(page, encoding='utf-8', newline='\n')
        (out/'app.webmanifest').write_text(json.dumps({'id':'./','name':identity['title'],'short_name':identity['title'][:24],'start_url':'./','scope':'./','display':'standalone','background_color':'#07111F','theme_color':'#07111F','icons':[{'src':'thumbnail.png','sizes':'256x256','type':'image/png'}]}), encoding='utf-8', newline='\n')
        for name in release.runtime_files(game,identity.get('package',[]),safe_file):
            if name in ('manifest.json','build.json','game.wasm','index.html','loader.js','platform.js','thumbnail.png','mobile.js','service-worker.js','app.webmanifest'):raise WebError(f'Extra asset collides with reserved package file: {name}')
            target_file=safe_file(out,name);target_file.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(safe_file(game,name),target_file)
        epoch=int(os.environ.get('SOURCE_DATE_EPOCH',run(['git','show','-s','--format=%ct','HEAD'],cwd=ROOT).strip()))
        (out/'build.json').write_text(json.dumps({'engine_revision':git_revision(ROOT),'game_revision':git_revision(game),'game_source_sha256':source_hash(game),'built_at_epoch':epoch},sort_keys=True)+'\n', encoding='utf-8', newline='\n')
        write_worker(out)
        manifest={'schema_version':SCHEMA,'runtime_abi':2,'runtime':'portable','mobile_controls':mobile,'install':{'web':'app.webmanifest','offline':True},'id':project['id'],'title':identity['title'],'description':project['description'],'engine_revision':git_revision(ROOT),'game_revision':git_revision(game),'game_source_sha256':source_hash(game),'presentation':project['presentation'],'targets':project['targets'],'input':project['input'],'networking':project['networking'],'built_at_epoch':epoch,'timestamp_policy':'SOURCE_DATE_EPOCH or source commit time for reproducible packaging','thumbnail':'thumbnail.png','play':'index.html','native_download':None,'compatibility':{'macroquad':macro['version'],'miniquad':mini['version'],'quad-snd':sound['version'],'loader_sha256':hashlib.sha256(loader_bytes).hexdigest(),'webgl':'WebGL 1','save_frame':1,'storage':'localStorage per origin + game ID; 4 MiB limit; Snapshot version/migrations'},'verification':expected,'file_sha256':{p.relative_to(out).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(out.rglob('*')) if p.is_file()}}
        manifest.update(release_contract(game,manifest))
        write_manifest(out,manifest);integrity(out)
        if not skip_browser:
            report=browser_verify(out,evidence)
            manifest['browser_verification']=verification_summary(report)
            manifest['measurements']={'desktop':report['performance'],'touch_emulation':report['mobile']['performance'],'physical_devices':[]}
            write_manifest(out,manifest)
        else:manifest['browser_verification']={'compiled':True,'package_valid':True,'remaining':'Run web verify; this build has no browser evidence'};write_manifest(out,manifest)

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
    return {'ok':True,'package':str(destination),'browser':not skip_browser,'seconds':round(time.monotonic()-start,3),'manifest':manifest,'evidence':str(evidence),'sizes':release.sizes(destination,manifest)}
def visual_preview(game):
    start=time.monotonic()
    result=build(game,skip_browser=True,preview=True)
    evidence=Path(result['evidence'])
    report=browser_verify(Path(result['package']),evidence,preview=True)
    return {**result,'scope':'visual_preview','shipping_verified':False,
            'seconds':round(time.monotonic()-start,3),'preview':report,
            'screenshots':[str(p) for p in sorted(evidence.glob('*.png'))],
            'remaining':'Inspect playing/outcome/portrait/landscape captures; run web build for complete shipping verification.'}
def directory_publish(package,destination):
    source_proof=publication_gate(package);manifest=integrity(package);destination.mkdir(parents=True,exist_ok=True)
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
        m=integrity(p.parent);catalog.append({k:m.get(k) for k in ('id','title','description','engine_revision','game_revision','presentation','networking','input','targets','built_at_epoch','compatibility','package_id','sources','browser_verification','required_capabilities')}|{'play':m['id']+'/index.html','thumbnail':m['id']+'/thumbnail.png','native_download':m['native_download']})
    temp=destination/'catalog.json.tmp';temp.write_text(json.dumps({'schema_version':1,'games':catalog},indent=2)+'\n', encoding='utf-8', newline='\n');temp.replace(destination/'catalog.json')
    spec=importlib.util.spec_from_file_location('be2_catalog',ROOT/'templates/catalog/browser_catalog.py')
    feed=importlib.util.module_from_spec(spec);spec.loader.exec_module(feed)
    cards=''.join(feed.card(g,prefix='')['card'] for g in catalog)
    for game in catalog:feed.write_details(destination,game['id'],game['title'],game['description'],game['presentation'],game['networking'],game,game.get('native_download'),prefix='')
    page=feed.enhance_page((ROOT/'templates/catalog/index.html').read_text(encoding='utf-8').replace('{{cards}}',cards))
    (destination/'index.html').write_text(page, encoding='utf-8', newline='\n')
    for name in ('app.js','style.css','catalog.css'):shutil.copy2(ROOT/'templates/catalog'/name,destination/name)
    return {'source_retrieval':source_proof,'package_id':manifest['package_id'],'backend':'directory','deployed':str(slot),'url':None,'catalog':str(destination/'catalog.json'),'remaining_external_step':'Serve this directory through your static host; each game is under /GAME_ID/. No external URL has been created.'}
def main(argv=None):
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('command',choices=['build','preview','verify','publish','propose','inspect','serve','capabilities','reproduce','prepare']);p.add_argument('game',nargs='?',default='.');p.add_argument('--port',type=int,default=8000);p.add_argument('--out');p.add_argument('--also',action='append',default=[],help='Publish additional game paths together to GitHub Pages');p.add_argument('--skip-browser',action='store_true',help='Build only: explicitly unverified, cannot publish');p.add_argument('--backend',choices=['directory','github-pages'],default='directory');p.add_argument('--destination');p.add_argument('--repository');args=p.parse_args(argv)
    try:
        if args.command=='capabilities':result=capabilities()
        elif args.command=='reproduce':
            from tools.web_reproduce import reproduce
            result=reproduce(Path(args.game).resolve(),Path(args.out).resolve() if args.out else None)
        elif args.command=='propose':
            result={'title':args.game,'presentation':'hybrid','runtime':'portable','mobile_controls':{'layout':'dpad','action_label':'Action'},'gameplay':'arcade or light strategy','session_minutes':10,'input':['keyboard','mouse'],'networking':'offline','targets':['web','windows'],'complexity':'low','next':'Adjust this proposal, then create with be2-tools new-game NAME DIR ENGINE portable. Choose 2d, 3d or hybrid deliberately; mix elements where useful. Project requirements are editable before build.'}
        else:
            game=Path(args.game).resolve()
            package=game if (game/'manifest.json').is_file() else game/'dist/web'
            if args.command=='inspect':
                manifest=integrity(package);result={'ok':True,'manifest':manifest,'sizes':release.sizes(package,manifest)}
            elif args.command=='serve':
                integrity(package)
                server=http.server.ThreadingHTTPServer(('127.0.0.1',args.port),functools.partial(QuietHandler,directory=str(package)))
                print(json.dumps({'ok':True,'url':f'http://127.0.0.1:{server.server_port}/','package':str(package)}),flush=True)
                server.serve_forever();return 0
            elif args.command=='prepare':
                run(['rustup','target','add','wasm32-unknown-unknown'])
                run(['cargo','fetch','--locked'],cwd=game)
                result={'ok':True,'next':'web build '+str(game),'capabilities':capabilities()}
            elif args.command=='preview':result=visual_preview(game)
            elif args.command=='verify':
                manifest=integrity(package);evidence=game/'.blue-check/web' if game!=package else package.parent/('.blue-check-'+package.name)/'web';report=browser_verify(package,evidence)
                manifest['browser_verification']=verification_summary(report);manifest['measurements']={'desktop':report['performance'],'touch_emulation':report['mobile']['performance'],'physical_devices':[]};write_manifest(package,manifest)
                result={'ok':True,'browser':report,'sizes':release.sizes(package,manifest)}
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
                        packages=[game/'dist/web']
                        for extra in args.also:
                            path=Path(extra).resolve();build(path);packages.append(path/'dist/web')
                        result['publication']=publish(packages,args.repository,run,integrity,directory_publish)
        print(json.dumps(result));return 0
    except (release.ReleaseError,WebError,OSError,ValueError,KeyError,StopIteration) as e:print(json.dumps({'ok':False,'error':str(e)}),file=sys.stderr);return 1
if __name__=='__main__':sys.exit(main())
