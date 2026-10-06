import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from tools import web_release as release
from tools import web_games as web

class PackageTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup);self.root=Path(self.temp.name);self.dist=self.root/'web';self.dist.mkdir()
        for name in ['index.html','loader.js','platform.js','thumbnail.png']:(self.dist/name).write_bytes(b'test')
        (self.dist/'game.wasm').write_bytes(b'\0asm\x01\0\0\0')
        self.manifest={'schema_version':1,'runtime_abi':1,'id':'test-game','presentation':'2d','networking':'offline','title':'Test Garden','description':'Test','targets':['web'],'input':['keyboard'],'engine_revision':'a'*40,'game_revision':'b'*40,'built_at_epoch':1,'compatibility':{},'native_download':None,'thumbnail':'thumbnail.png','play':'index.html','verification':{'hash':'1'*16,'outcome':'won'},'file_sha256':{p.name:web.hashlib.sha256(p.read_bytes()).hexdigest() for p in self.dist.iterdir()}}
        self.manifest.update({'package_id':release.package_id(self.manifest['file_sha256']),
            'sources':{'engine':{'repository':'https://example.test/engine.git','revision':'a'*40,'path':'.','clean':True},'game':{'repository':'https://example.test/game.git','revision':'b'*40,'path':'.','clean':True}},
            'game_source_sha256':'c'*64,'reproduce':{'command':['build']},'required_capabilities':['WebAssembly'],
            'browser_verification':{'compiled':True,'package_valid':True,'desktop':{k:True for k in ('wasm_instantiated','playable','input','save_write','reload_read','audio_initialized','offline_reload','gameplay_scenario','update_recovery','focus_loss','focus_return')},'mobile':{k:True for k in ('wasm_instantiated','playable','input','save_write','reload_read','audio_initialized','offline_reload','gameplay_scenario','update_recovery','focus_loss','focus_return')}}})
        mock=patch.object(release,'retrieve_sources',return_value={'ok':True});mock.start();self.addCleanup(mock.stop)
        mock=patch('tools.web_reproduce.reproduce',return_value={'ok':True,'compared_fields':['package_id','file_sha256']});mock.start();self.addCleanup(mock.stop)
        self.stamp()
    def stamp(self):(self.dist/'manifest.json').write_text(json.dumps(self.manifest))
    def test_unicode_catalog_is_utf8_on_every_host(self):
        self.manifest['title']='龙 🌱';self.stamp();out=self.root/'library'
        web.directory_publish(self.dist,out)
        self.assertIn('龙 🌱',(out/'games/test-game/index.html').read_text(encoding='utf-8'))
        self.assertIn('←',(out/'games/test-game/index.html').read_text(encoding='utf-8'))
    def test_open_ended_routes_require_explicit_meaningful_evidence(self):
        proof={'hash':'1'*16,'outcome':'playing','ticks':1200,'purpose':'walk across streamed chunks'}
        self.manifest['verification']=proof;self.stamp();web.integrity(self.dist)
        for bad in [proof|{'ticks':0},proof|{'ticks':True},proof|{'purpose':''},proof|{'outcome':'lost'},proof|{'hash':'bogus'}]:
            self.manifest['verification']=bad;self.stamp()
            with self.subTest(bad=bad),self.assertRaises(web.WebError):web.integrity(self.dist)
    def test_directory_publication_links_titles_to_play_and_download_details(self):
        out=self.root/'library';self.manifest['native_download']='https://example.test/game.exe';self.stamp()
        web.directory_publish(self.dist,out)
        page=(out/'index.html').read_text();details=(out/'games/test-game/index.html').read_text()
        self.assertIn('class="game-title" href="games/test-game/"',page)
        self.assertIn('href="../../test-game/index.html"',details)
        self.assertIn('https://example.test/game.exe',details)
        self.assertIn('aria-pressed="false"',page);self.assertIn('<svg',page)
    def test_hash_missing_and_source_fallback_are_rejected(self):
        web.integrity(self.dist)
        (self.root/'game.wasm').write_bytes((self.dist/'game.wasm').read_bytes());(self.dist/'game.wasm').unlink()
        with self.assertRaisesRegex(web.WebError,'Missing packaged'):web.integrity(self.dist)
        (self.dist/'game.wasm').write_bytes(b'changed')
        with self.assertRaisesRegex(web.WebError,'hash mismatch'):web.integrity(self.dist)
    def test_unsafe_and_undeclared_files_cannot_pass(self):
        (self.dist/'hidden.txt').write_text('developer secret')
        with self.assertRaisesRegex(web.WebError,'Undeclared'):web.integrity(self.dist)
        for bad in ['../outside','/absolute','folder\\file','folder//file','x/./file','C:file','CON','assets/NUL.png','x?.png','x|y','x\x01.png']:
            with self.subTest(bad=bad),self.assertRaises(web.WebError):web.safe_file(self.dist,bad)
    def test_symlink_and_case_collisions_are_refused(self):
        self.manifest['file_sha256']['Loader.js']='0'*64;self.stamp()
        with self.assertRaisesRegex(web.WebError,'Case-colliding'):web.integrity(self.dist)
        if hasattr(web.os,'symlink'):
            try:(self.dist/'outside').symlink_to(self.root/'game.wasm')
            except OSError:return
            with self.assertRaisesRegex(web.WebError,'symlink'):web.safe_file(self.dist,'outside')
    def test_publish_preserves_other_games_and_catalog_metadata(self):
        out=self.root/'library';first=web.directory_publish(self.dist,out);self.assertIsNone(first['url'])
        self.manifest['id']='second-game';self.stamp();web.directory_publish(self.dist,out)
        self.assertTrue((out/'test-game/game.wasm').exists());self.assertTrue((out/'second-game/game.wasm').exists())
        catalog=json.loads((out/'catalog.json').read_text());self.assertEqual(len(catalog['games']),2)
        self.assertEqual(catalog['games'][0]['presentation'],'2d')
    def test_publish_will_not_replace_unrelated_user_directory(self):
        out=self.root/'library';(out/'test-game').mkdir(parents=True);(out/'test-game/private').write_text('keep')
        with self.assertRaisesRegex(web.WebError,'unrelated'):web.directory_publish(self.dist,out)
        self.assertEqual((out/'test-game/private').read_text(),'keep')

class RequirementsTests(unittest.TestCase):
    def test_explicit_web_binary_features_and_identity_are_validated(self):
        game=web.ROOT/'assets/games/leo'
        project=json.loads((game/'game.project.json').read_text())
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            for change in [{},{'features':[]},{'features':['client,offline']},{'identity':'../identity.json'},{'binary':'../game'},{'feature':['client']}]:
                value=project|{'web_build':project['web_build']|change}
                (root/'game.project.json').write_text(json.dumps(value))
                if change:
                    with self.subTest(change=change),self.assertRaisesRegex(web.WebError,'web_build'):web.validate_project(root)
                else:self.assertEqual(web.validate_project(root)['web_build']['binary'],'leo-browser')

    def test_combinations_have_actionable_diagnostics(self):
        project={'schema_version':1,'id':'tiny-station','presentation':'2d','targets':['web','windows'],'networking':'offline','input':['mouse','keyboard'],'description':'Manage a station','session_minutes':10,'complexity':'low'}
        with tempfile.TemporaryDirectory() as directory:
            game=Path(directory)
            def write(p):(game/'game.project.json').write_text(json.dumps(p))
            write(project);self.assertEqual(web.validate_project(game)['targets'],['web','windows'])
            for change,message in [({'presentation':'3d'},r'portable \+ offline'),({'networking':'native-multiplayer'},r'portable \+ offline'),({'targets':['browser']},'targets'),({'target':['web']},'Unknown project'),({'presentation':'2d','networking':'native-multiplayer','targets':['windows']},'offline only'),({'input':[]},'input'),({'id':23},'Game ID'),({'targets':[{}]},'targets'),({'input':[{}]},'input')]:
                with self.subTest(change=change):
                    write(project|change)
                    with self.assertRaisesRegex(web.WebError,message):web.validate_project(game)
            write(project|{'presentation':'3d','targets':['linux','windows'],'networking':'native-multiplayer'});web.validate_project(game)
            with self.assertRaisesRegex(web._project.ProjectError,'not declared'):web._project.native_target(game,'macos')


    def test_native_packaging_checks_current_host_and_undeclared_targets(self):
        from tools.test_game_ship import game_ship
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as folder:
            game=Path(folder);(game/'scripts').mkdir();(game/'Cargo.toml').write_text('[package]\nname="test-game"')
            (game/'scripts/project.py').write_text((web.ROOT/'templates/game_project.py').read_text())
            project={'schema_version':1,'id':'test-game','presentation':'2d','targets':['linux'],'networking':'offline','input':['keyboard'],'description':'Test','session_minutes':1,'complexity':'low'}
            (game/'game.project.json').write_text(json.dumps(project))
            with patch.object(game_ship,'host_platform',return_value='linux'),patch.object(game_ship,'Project') as create:
                game_ship.load_project(game);create.assert_called_once()
            with self.assertRaisesRegex(game_ship.ConfigError,'not declared'):game_ship.load_project(game,'windows')

class PublisherBoundaryTests(unittest.TestCase):
    def test_catalog_adapter_preserves_native_cards_and_is_idempotent(self):
        from tools.web_publish import integrate_catalog
        with tempfile.TemporaryDirectory() as folder:
            builder=Path(folder)/'build.py'
            original='import shutil\nfrom pathlib import Path\ndef build():\n    games.sort(key=lambda g: g["name"])\n    page = "native download cards"\n    (out / "index.html").write_text(page)\n'
            builder.write_text(original);integrate_catalog(builder)
            first=builder.read_text();integrate_catalog(builder)
            self.assertEqual(builder.read_text(),first)
            self.assertIn('native download cards',first)
            self.assertIn('merge_games',first)
            self.assertTrue((builder.parent/'browser_catalog.py').exists())
            compile(first,str(builder),'exec')
            builder.write_text('unexpected catalog interface')
            with self.assertRaisesRegex(ValueError,'integration point changed'):integrate_catalog(builder)
            self.assertEqual(builder.read_text(),'unexpected catalog interface')

class PortableRequirementsTests(unittest.TestCase):
    def test_all_presentations_accept_browser_and_native_on_shared_runtime(self):
        project={'schema_version':1,'id':'portable-game','runtime':'portable','targets':['web','linux','windows'],'networking':'offline','input':['keyboard','mouse'],'description':'A mixed view','session_minutes':2,'complexity':'low'}
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            for presentation in ('2d','3d','hybrid'):
                for layout in ('dpad','paddle','tap'):
                    (root/'game.project.json').write_text(json.dumps(project|{'presentation':presentation,'mobile_controls':{'layout':layout}}))
                    self.assertEqual(web.validate_project(root)['presentation'],presentation)
            for bad in ({'layout':'joystick-guess'},{'layout':'dpad','actions':7},{'layout':'dpad','action_label':'x'*25}):
                (root/'game.project.json').write_text(json.dumps(project|{'presentation':'hybrid','mobile_controls':bad}))
                with self.assertRaisesRegex(web.WebError,'mobile_controls'):web.validate_project(root)

class UnifiedCatalogTests(unittest.TestCase):
    def test_details_keep_play_and_download_above_history(self):
        import importlib.util
        spec=importlib.util.spec_from_file_location('catalog',web.ROOT/'templates/catalog/browser_catalog.py')
        catalog=importlib.util.module_from_spec(spec);spec.loader.exec_module(catalog)
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);directory=root/'games/shared';directory.mkdir(parents=True)
            page=directory/'index.html'
            page.write_text('<head></head><main><section class="game-summary"><a class="dl" href="installer.exe">Download</a></section><section><h2>Versions</h2></section></main>')
            item={'description':'Shared game','play':'shared/index.html','input':['keyboard']}
            catalog.write_details(root,'shared','Shared','Shared game','3d','offline',item)
            first=page.read_text();catalog.write_details(root,'shared','Shared','Shared game','3d','offline',item)
            self.assertEqual(first,page.read_text())
            self.assertLess(first.index('Play in browser'),first.index('Versions'))
            self.assertIn('installer.exe',first)
            self.assertIn('href="../../catalog.css"',first)

    def test_browser_and_native_join_by_id_and_preserve_downloads(self):
        import importlib.util
        spec=importlib.util.spec_from_file_location('catalog',web.ROOT/'templates/catalog/browser_catalog.py')
        catalog=importlib.util.module_from_spec(spec);spec.loader.exec_module(catalog)
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);(root/'web').mkdir();(root/'out').mkdir()
            item={'id':'shared-game','title':'Shared','description':'A hybrid','presentation':'hybrid','networking':'offline','input':['mouse'],'targets':['web','windows'],'built_at_epoch':1,'play':'shared-game/index.html','thumbnail':'shared-game/thumbnail.png','native_download':None}
            (root/'web/catalog.json').write_text(json.dumps({'games':[item,item|{'id':'browser-only','title':'<Unsafe>'}]}))
            native=[{'card':'<article class="card"><h2>Shared</h2><p class="meta">v1</p><a class="dl" href="existing.zip">Download</a></article>','name':'Shared','created':'2026-01-01'}]
            merged=catalog.merge_games(native,[{'slug':'shared-game','name':'Shared'}],root/'web',root/'out',root/'games')
            self.assertEqual(len(merged),2)
            self.assertIn('existing.zip',merged[0]['card']);self.assertIn('web/shared-game/index.html',merged[0]['card'])
            self.assertIn('data-presentation="hybrid"',merged[0]['card']);self.assertEqual(merged[0]['card'].count('data-star='),1)
            self.assertIn('&lt;Unsafe&gt;',merged[1]['card'])
            self.assertTrue((root/'out/web/catalog.json').is_file())
            self.assertEqual((root/'out/catalog.css').read_bytes(),(web.ROOT/'templates/catalog/catalog.css').read_bytes())
            self.assertTrue((root/'out/games/browser-only/index.html').is_file())


@unittest.skipUnless(web.os.environ.get('BE2_BROWSER_FIXTURE'), 'Set BE2_BROWSER_FIXTURE to a built web package for real browser negative tests')
class BrowserDependencyTests(unittest.TestCase):
    def test_checkout_asset_cannot_satisfy_undeclared_runtime_request(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);package=root/'web'
            web.shutil.copytree(web.os.environ['BE2_BROWSER_FIXTURE'],package)
            # This file exists beside the package, just as a developer checkout asset could.
            (root/'developer-only.txt').write_text('available in checkout')
            page=package/'index.html';page.write_text(page.read_text()+'<script>setTimeout(()=>fetch("../developer-only.txt"),500)</script>')
            manifest=json.loads((package/'manifest.json').read_text())
            manifest['file_sha256']['index.html']=web.hashlib.sha256(page.read_bytes()).hexdigest();manifest['package_id']=release.package_id(manifest['file_sha256'])
            (package/'manifest.json').write_text(json.dumps(manifest))
            web.integrity(package) # manifest/file checks alone cannot prove runtime dependency closure
            with self.assertRaisesRegex(web.WebError,'Browser errors|404|HTTP|Browser smoke'):
                web.browser_verify(package,root/'evidence')

    def test_even_successful_external_requests_are_undeclared_dependencies(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);package=root/'web';web.shutil.copytree(web.os.environ['BE2_BROWSER_FIXTURE'],package)
            (root/'external.txt').write_text('HTTP 200 is not proof of package completeness')
            server=web.http.server.ThreadingHTTPServer(('127.0.0.1',0),web.functools.partial(web.QuietHandler,directory=str(root)))
            thread=web.threading.Thread(target=server.serve_forever,daemon=True);thread.start()
            try:
                page=package/'index.html';page.write_text(page.read_text()+f'<script>setTimeout(()=>fetch("http://127.0.0.1:{server.server_port}/external.txt",{{mode:"no-cors"}}),500)</script>')
                manifest=json.loads((package/'manifest.json').read_text());manifest['file_sha256']['index.html']=web.hashlib.sha256(page.read_bytes()).hexdigest();manifest['package_id']=release.package_id(manifest['file_sha256'])
                (package/'manifest.json').write_text(json.dumps(manifest))
                with self.assertRaisesRegex(web.WebError,'Undeclared runtime dependency'):web.browser_verify(package,root/'evidence')
            finally:server.shutdown();server.server_close();thread.join()


class ReleaseRegressionTests(unittest.TestCase):
    def test_anonymous_fetch_rejects_missing_revision_and_source_path(self):
        import subprocess
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);subprocess.run(['git','init',str(root)],check=True,capture_output=True)
            (root/'Cargo.toml').write_text('[package]')
            subprocess.run(['git','add','.'],cwd=root,check=True,capture_output=True)
            subprocess.run(['git','-c','user.name=Test','-c','user.email=test@example.test','commit','-m','fixture'],cwd=root,check=True,capture_output=True)
            revision=release.git(['rev-parse','HEAD'],root)
            real_git=release.git;calls=[]
            def local_git(args,cwd):
                calls.append(args[:])
                if 'fetch' in args:
                    args=args[:];args[-2]=str(root)
                return real_git(args,cwd)
            def manifest(rev=revision,path='.'):
                return {'engine_revision':rev,'game_revision':rev,'sources':{r:{'repository':'https://example.test/public.git','revision':rev,'path':path,'clean':True} for r in ('engine','game')}}
            with patch.object(release,'git',side_effect=local_git):
                self.assertTrue(release.retrieve_sources(manifest())['ok'])
                self.assertEqual(sum('fetch' in c for c in calls),1)
                self.assertIn('credential.helper=',next(c for c in calls if 'fetch' in c))
                with self.assertRaisesRegex(release.ReleaseError,'retrieval failed'):release.retrieve_sources(manifest('f'*40))
                with self.assertRaisesRegex(release.ReleaseError,'path is absent'):release.retrieve_sources(manifest(path='missing'))
                with self.assertRaisesRegex(release.ReleaseError,'source hash differs'):release.retrieve_sources(manifest()|{'game_source_sha256':'0'*64})
                dirty=manifest();dirty['sources']['engine']['clean']=False
                with self.assertRaisesRegex(release.ReleaseError,'uncommitted'):release.retrieve_sources(dirty)
                wrong=manifest();wrong['engine_revision']='a'*40
                with self.assertRaisesRegex(release.ReleaseError,'contradicts'):release.retrieve_sources(wrong)
            for bad in ('source-sha256:'+revision,revision[:12]):
                with self.assertRaises(release.ReleaseError):release.validate_source({'repository':'https://example.test/repo','revision':bad,'path':'.'})

    def test_runtime_audio_closure_excludes_previews_and_reports_but_keeps_credits(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);bank=root/'assets/audio/music';bank.mkdir(parents=True)
            (bank/'bank.json').write_text(json.dumps({'music':{'score':{'file':'score.wav'}},'effects':{}}))
            for name in ('score.wav','preview-mix.wav','report.json'):(bank/name).write_text(name)
            (root/'AUDIO.md').write_text('Credits')
            self.assertEqual(release.runtime_files(root,['assets/audio','AUDIO.md'],web.safe_file),['AUDIO.md','assets/audio/music/bank.json','assets/audio/music/score.wav'])
            (bank/'score.wav').unlink()
            with self.assertRaisesRegex(release.ReleaseError,'runtime file missing'):release.runtime_files(root,['assets/audio'],web.safe_file)
            (bank/'bank.json').write_text(json.dumps({'music':{'escape':{'file':'../../../../private'}}}))
            with self.assertRaises(web.WebError):release.runtime_files(root,['assets/audio'],web.safe_file)

class PublicationGateTests(PackageTests):
    def test_failed_provenance_cannot_write_or_replace_deployment(self):
        target=self.root/'library'
        with patch.object(release,'retrieve_sources',side_effect=release.ReleaseError('Source retrieval failed')):
            with self.assertRaisesRegex(release.ReleaseError,'retrieval failed'):web.directory_publish(self.dist,target)
        self.assertFalse(target.exists())
    def test_failed_clean_reproduction_cannot_mutate_deployment(self):
        target=self.root/'library'
        with patch('tools.web_reproduce.reproduce',side_effect=release.ReleaseError('Clean reproduction differs')):
            with self.assertRaisesRegex(release.ReleaseError,'reproduction differs'):web.directory_publish(self.dist,target)
        self.assertFalse(target.exists())
    def test_missing_browser_stage_and_legacy_metadata_cannot_publish(self):
        for change in ({'browser_verification':{}},{'package_id':None}):
            old=self.manifest.copy();self.manifest.update(change);self.stamp()
            with self.assertRaises((web.WebError,release.ReleaseError)):
                if 'sources' in change:
                    with patch.object(release,'retrieve_sources',wraps=release.retrieve_sources):web.publication_gate(self.dist)
                else:web.publication_gate(self.dist)
            self.manifest=old

if __name__=='__main__':unittest.main()
