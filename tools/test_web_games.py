import json
from pathlib import Path
import tempfile
import unittest
from tools import web_games as web

class PackageTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup);self.root=Path(self.temp.name);self.dist=self.root/'web';self.dist.mkdir()
        for name in ['index.html','loader.js','platform.js','thumbnail.png']:(self.dist/name).write_bytes(b'test')
        (self.dist/'game.wasm').write_bytes(b'\0asm\x01\0\0\0')
        self.manifest={'schema_version':1,'runtime_abi':1,'id':'test-game','presentation':'2d','networking':'offline','title':'Test Garden','description':'Test','targets':['web'],'input':['keyboard'],'engine_revision':'a'*40,'game_revision':'b'*40,'built_at_epoch':1,'compatibility':{},'native_download':None,'thumbnail':'thumbnail.png','play':'index.html','verification':{'hash':'1'*16,'outcome':'won'},'file_sha256':{p.name:web.hashlib.sha256(p.read_bytes()).hexdigest() for p in self.dist.iterdir()}}
        self.stamp()
    def stamp(self):(self.dist/'manifest.json').write_text(json.dumps(self.manifest))
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
    def test_combinations_have_actionable_diagnostics(self):
        project={'schema_version':1,'id':'tiny-station','presentation':'2d','targets':['web','windows'],'networking':'offline','input':['mouse','keyboard'],'description':'Manage a station','session_minutes':10,'complexity':'low'}
        with tempfile.TemporaryDirectory() as directory:
            game=Path(directory)
            def write(p):(game/'game.project.json').write_text(json.dumps(p))
            write(project);self.assertEqual(web.validate_project(game)['targets'],['web','windows'])
            for change,message in [({'presentation':'3d'},r'2d \+ offline'),({'networking':'native-multiplayer'},r'2d \+ offline'),({'targets':['browser']},'targets'),({'target':['web']},'Unknown project'),({'presentation':'2d','networking':'native-multiplayer','targets':['windows']},'offline only'),({'input':[]},'input'),({'id':23},'Game ID'),({'targets':[{}]},'targets'),({'input':[{}]},'input')]:
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
            original='import shutil\nfrom pathlib import Path\ndef build():\n    page = "native download cards"\n    (out / "index.html").write_text(page)\n'
            builder.write_text(original);integrate_catalog(builder)
            first=builder.read_text();integrate_catalog(builder)
            self.assertEqual(builder.read_text(),first)
            self.assertIn('native download cards',first)
            self.assertIn('out / "web"',first)
            compile(first,str(builder),'exec')
            builder.write_text('unexpected catalog interface')
            with self.assertRaisesRegex(ValueError,'integration point changed'):integrate_catalog(builder)
            self.assertEqual(builder.read_text(),'unexpected catalog interface')


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
            manifest['file_sha256']['index.html']=web.hashlib.sha256(page.read_bytes()).hexdigest()
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
                manifest=json.loads((package/'manifest.json').read_text());manifest['file_sha256']['index.html']=web.hashlib.sha256(page.read_bytes()).hexdigest()
                (package/'manifest.json').write_text(json.dumps(manifest))
                with self.assertRaisesRegex(web.WebError,'Undeclared runtime dependency'):web.browser_verify(package,root/'evidence')
            finally:server.shutdown();server.server_close();thread.join()

if __name__=='__main__':unittest.main()
