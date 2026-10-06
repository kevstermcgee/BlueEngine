"""Optional GitHub Pages adapter. Engine/runtime never depends on a hosting provider.

The supported central library has site/build.py and a Pages workflow. All publication edits
occur in an isolated checkout; existing download cards, game sources and release artifacts stay intact.
"""
import hashlib
import json
from pathlib import Path
import tempfile
import time
import urllib.request

def publish(package,repository,run,integrity,directory_publish):
    WebError = ValueError
    if not repository or repository.count('/')!=1:raise WebError('GitHub Pages publish needs --repository OWNER/REPO')
    permissions=json.loads(run(['gh','api',f'repos/{repository}']))
    if not permissions.get('permissions',{}).get('push'):raise WebError('GitHub connection lacks push permission for this library. Use --backend directory for a publication-ready artifact.')
    pages=json.loads(run(['gh','api',f'repos/{repository}/pages']))
    if pages.get('build_type')!='workflow' or not pages.get('html_url'):raise WebError('Repository needs existing workflow-based GitHub Pages hosting. Use the directory backend until configured.')
    packages=list(package) if isinstance(package,(list,tuple)) else [package]
    metadata=[integrity(p) for p in packages]
    with tempfile.TemporaryDirectory(prefix='be2-publish-') as folder:
        checkout=Path(folder)/'library'
        run(['gh','repo','clone',repository,checkout,'--','--depth=1'])
        builder=checkout/'site/build.py';workflow=checkout/'.github/workflows/pages.yml'
        if not builder.is_file() or not workflow.is_file():raise WebError('This backend expects the central library site/build.py + pages.yml contract. Use a custom adapter/directory backend for another site layout.')
        destination=checkout/'site/web'
        receipts=[directory_publish(p,destination) for p in packages]
        metadata=[integrity(p) for p in packages]
        source_proof=[r['source_retrieval'] for r in receipts]
        integrate_catalog(builder)
        # Verify the actual merged site before pushing, using the current native release catalog.
        from tools.web_games import catalog_verify
        validation=Path(folder)/'validation';validation.mkdir()
        release=json.loads(run(['gh','api',f'repos/{repository}/releases/latest']))
        (validation/'release.json').write_text(json.dumps(release))
        catalog_name=next((name for name in ('Games-catalog.tsv','BlueEngineLauncher-catalog.tsv') if any(a['name']==name for a in release['assets'])),None)
        if catalog_name is None:raise WebError('Current release has no supported native game catalog; verified package retained.')
        run(['gh','release','download',release['tag_name'],'--repo',repository,'--pattern',catalog_name,'--dir',validation])
        run(['python3',builder,'--catalog',validation/catalog_name,'--release-json',validation/'release.json','--games-dir',checkout/'games','--thumbs',checkout/'site/thumbs','--out',validation/'site'])
        catalog_proof=catalog_verify(validation/'site',validation/'evidence')
        run(['git','add','site/web','site/build.py','site/browser_catalog.py','site/app.js','site/catalog.css'],cwd=checkout)
        changed=bool(run(['git','diff','--cached','--name-only'],cwd=checkout).strip())
        deployment=None
        if changed:
            run(['git','-c','user.name=BlueEngine Publisher','-c','user.email=blueengine-publisher@users.noreply.github.com','commit','-m','Publish reproducible browser games '+', '.join(m['id'] for m in metadata)],cwd=checkout)
            commit=run(['git','rev-parse','HEAD'],cwd=checkout).strip()
            run(['git','push','origin','HEAD:main'],cwd=checkout)
            # Confirm deployment, never invent a live URL from a successfully pushed commit.
            deadline=time.monotonic()+600;deployment=None
            while time.monotonic()<deadline:
                runs=json.loads(run(['gh','run','list','--repo',repository,'--workflow','pages.yml','--commit',commit,'--json','databaseId,status,conclusion']))
                if runs:
                    deployment=runs[0]
                    if deployment['status']=='completed':
                        if deployment['conclusion']!='success':raise WebError(f'Pages deployment failed: gh run view {deployment["databaseId"]} --repo {repository} --log-failed. Verified local package retained.')
                        break
                time.sleep(5)
            else:raise WebError(f'Pages deployment still pending for {commit}; local package retained. Check gh run list --repo {repository}.')
        else:
            commit=run(['git','rev-parse','HEAD'],cwd=checkout).strip()
        # Confirm every exact manifest and runtime file after the successful shared deployment.
        confirmed=[]
        for item in metadata:
            url=pages['html_url'].rstrip('/')+'/web/'+item['id']+'/'
            last_error=None
            for _ in range(20):
                try:
                    with urllib.request.urlopen(url+'manifest.json',timeout=20) as response:remote=json.load(response)
                    if remote==item:
                        for name,digest in item['file_sha256'].items():
                            with urllib.request.urlopen(url+name,timeout=30) as asset:
                                if hashlib.sha256(asset.read()).hexdigest()!=digest:raise WebError(f'Deployed file differs: {name}')
                        confirmed.append({'id':item['id'],'url':url,'package_id':item['package_id'],'remote_manifest_verified':True,'remote_files_verified':True})
                        break
                    last_error='old manifest is still cached'
                except Exception as error:last_error=str(error)
                time.sleep(3)
            else:raise WebError(f'Pages workflow succeeded but current package could not be confirmed: {last_error}. No verified URL receipt emitted.')
        return {'source_retrieval':source_proof,'backend':'github-pages','url':confirmed[0]['url'] if len(confirmed)==1 else None,'games':confirmed,'repository':repository,'deployment_commit':commit if changed else None,'library_commit':commit,'already_current':not changed,'run':deployment,'remote_manifest_verified':True,'remote_files_verified':True,'catalog_verified':catalog_proof}

def integrate_catalog(builder):
    source=builder.read_text();marker='# BlueEngine unified catalog (static publisher contract v2)'
    anchor='    (out / "index.html").write_text(page)'
    sort_anchor='    games.sort(key=lambda g: g["name"])'
    if anchor not in source or sort_anchor not in source:raise ValueError('Catalog build integration point changed; update the adapter instead of guessing edits')
    # Upgrade the prior separate-page integration, preserving the native builder.
    old='    # BlueEngine web artifacts (static publisher contract v1)\n    web_root = Path(__file__).resolve().parent / "web"\n    if web_root.is_dir():\n        shutil.copytree(web_root, out / "web", dirs_exist_ok=True)\n        page = page.replace(\'<main id="grid">\', \'<p><a href="web/index.html">Play 2D games in your browser</a></p><main id="grid">\')\n'
    source=source.replace(old,'')
    if marker not in source:
        source=source.replace(sort_anchor,'''    # BlueEngine unified catalog (static publisher contract v2)
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from browser_catalog import merge_games, enhance_page
    games = merge_games(games, rows, Path(__file__).resolve().parent / "web", out, args.games_dir)
'''+sort_anchor)
        source=source.replace(anchor,'    page = enhance_page(page)\n'+anchor)
    builder.write_text(source)
    web_index=builder.parent/'web/index.html'
    if web_index.exists():web_index.write_text('<!doctype html><meta http-equiv="refresh" content="0;url=../"><a href="../">All BlueEngine games</a>')
    templates=Path(__file__).resolve().parent.parent/'templates/catalog'
    import shutil
    for name in ('browser_catalog.py','app.js','catalog.css'):shutil.copy2(templates/name,builder.parent/name)
