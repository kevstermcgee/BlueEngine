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
    metadata=integrity(package)
    with tempfile.TemporaryDirectory(prefix='be2-publish-') as folder:
        checkout=Path(folder)/'library'
        run(['gh','repo','clone',repository,checkout,'--','--depth=1'])
        builder=checkout/'site/build.py';workflow=checkout/'.github/workflows/pages.yml'
        if not builder.is_file() or not workflow.is_file():raise WebError('This backend expects the central library site/build.py + pages.yml contract. Use a custom adapter/directory backend for another site layout.')
        destination=checkout/'site/web'
        receipt=directory_publish(package,destination)
        integrate_catalog(builder)
        run(['git','add','site/web','site/build.py'],cwd=checkout)
        changed=bool(run(['git','diff','--cached','--name-only'],cwd=checkout).strip())
        deployment=None
        if changed:
            run(['git','-c','user.name=BlueEngine Publisher','-c','user.email=blueengine-publisher@users.noreply.github.com','commit','-m',f'Publish verified browser game {metadata["id"]}'],cwd=checkout)
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
        url=pages['html_url'].rstrip('/')+'/web/'+metadata['id']+'/'
        # Browser smoke already used the exact package. Confirm the host serves the same manifest.
        last_error=None
        for _ in range(20):
            try:
                with urllib.request.urlopen(url+'manifest.json',timeout=20) as response:remote=json.load(response)
                if remote['file_sha256']==metadata['file_sha256'] and remote['engine_revision']==metadata['engine_revision']:
                    for name,digest in metadata['file_sha256'].items():
                        with urllib.request.urlopen(url+name,timeout=30) as asset:
                            if hashlib.sha256(asset.read()).hexdigest()!=digest:raise WebError(f'Deployed file differs: {name}')
                    return {'backend':'github-pages','url':url,'repository':repository,'deployment_commit':commit if changed else None,'library_commit':commit,'already_current':not changed,'run':deployment,'remote_manifest_verified':True,'remote_files_verified':True}
                last_error='old manifest is still cached'
            except Exception as error:last_error=str(error)
            time.sleep(3)
        raise WebError(f'Pages workflow succeeded but current package could not be confirmed: {last_error}. No verified URL receipt emitted.')

def integrate_catalog(builder):
    source=builder.read_text();marker='# BlueEngine web artifacts (static publisher contract v1)'
    if marker not in source:
        anchor='    (out / "index.html").write_text(page)'
        if anchor not in source:raise ValueError('Catalog build integration point changed; update the adapter instead of guessing edits')
        source=source.replace(anchor,'''    # BlueEngine web artifacts (static publisher contract v1)
    web_root = Path(__file__).resolve().parent / "web"
    if web_root.is_dir():
        shutil.copytree(web_root, out / "web", dirs_exist_ok=True)
        page = page.replace('<main id="grid">', '<p><a href="web/index.html">Play 2D games in your browser</a></p><main id="grid">')
'''+anchor)
        builder.write_text(source)
