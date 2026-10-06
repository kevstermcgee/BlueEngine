"""Policy, atomic output, catalog identity and fetch tests; real conversion is Rust-tested."""
from argparse import Namespace
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest
import zipfile
from tools import assets, fetch_models, model_import


class ModelImportTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / "source.glb"
        self.source.write_bytes(b"fixture")
        self.args = Namespace(source=self.source, output=self.root / "pack", id="chair", pack_id="games/props", license="CC0-1.0", tags=["chair"], attribution="Fixture author", source_url="https://example.org/cc0", scale=1.0)
        self.result = {"model": {"version":1,"chunks":[],"textures":[]}, "collider": {"version":1,"bounds_min":[-1,0,-1],"bounds_max":[1,2,1],"center":[0,1,0],"half_extents":[1,1,1],"triangle_count":12,"chunk_count":1,"collision":"box"},"warnings":[]}

    def test_policy_rejects_before_build_or_writes(self):
        def no_build(*_):
            self.fail("policy failure must not build")
        self.args.license="MIT"
        with self.assertRaisesRegex(ValueError,"CC0"):
            model_import.import_model(self.args, no_build)
        self.assertFalse(self.args.output.exists())
        self.args.license="CC0-1.0"
        self.args.source_url="file:///private"
        with self.assertRaisesRegex(ValueError,"HTTP"):
            model_import.import_model(self.args, no_build)

    def test_catalog_search_and_hash_invalidation(self):
        result=model_import.import_model(self.args, lambda *_:self.result)
        pack, records=assets.adapt_external(Path(result["pack"]))
        self.assertEqual(pack["license"],"CC0-1.0")
        self.assertEqual(assets.search(records,"chair",10)[0][0]["id"],"games/props/chair")
        self.assertEqual(records[0]["geometry"]["triangle_count"],12)
        (self.args.output/"model.json").write_text('{}')
        with self.assertRaisesRegex(ValueError,"hash mismatch"):
            assets.adapt_external(Path(result["pack"]))

    def test_conversion_failure_and_existing_output_preserved(self):
        def fail(*_):
            raise ValueError("kit::lint rejected triangle")
        with self.assertRaisesRegex(ValueError,"kit::lint"):
            model_import.import_model(self.args,fail)
        self.assertFalse(self.args.output.exists())
        self.args.output.mkdir()
        saved=self.args.output/'previous.json'; saved.write_text('valuable')
        with self.assertRaisesRegex(ValueError,"already exists"):
            model_import.import_model(self.args,fail)
        self.assertEqual(saved.read_text(),'valuable')

    def test_invalid_collider_rolls_back_staging(self):
        self.result['collider']['half_extents']=[0.2,1,1]
        with self.assertRaisesRegex(ValueError,"does not cover"):
            model_import.import_model(self.args,lambda *_:self.result)
        self.assertFalse(self.args.output.exists())
        self.assertFalse(list(self.root.glob('.model-import-*')))

    def test_checksum_fetch_is_bounded_and_selective(self):
        data=io.BytesIO()
        with zipfile.ZipFile(data,'w') as z:
            z.writestr('Models/chair.glb',b'chair')
            z.writestr('../unsafe',b'unselected')
        archive=data.getvalue()
        spec={'license':'CC0-1.0','archive_url':'https://example.org/pack.zip','archive_sha256':hashlib.sha256(archive).hexdigest(),'download_budget_bytes':1024,'pack_budget_bytes':10,'files':[{'member':'Models/chair.glb','output':'chair.glb','sha256':hashlib.sha256(b'chair').hexdigest()}]}
        opener=lambda *args,**kwargs:io.BytesIO(archive)
        out=self.root/'fetched'
        result=fetch_models.fetch(out,spec,opener)
        self.assertEqual(result['files'],['chair.glb'])
        self.assertEqual((out/'chair.glb').read_bytes(),b'chair')
        spec['archive_sha256']='0'*64
        with self.assertRaisesRegex(ValueError,'checksum'):
            fetch_models.fetch(self.root/'bad',spec,opener)
        self.assertFalse((self.root/'bad').exists())


if __name__=='__main__':
    unittest.main()
