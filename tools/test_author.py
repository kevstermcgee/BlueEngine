"""Protocol and real packaged-native integration checks; no engine source reads."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / 'tools' / 'author.py'


class AuthoringTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='be2-author-test-')
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)

    def call(self, *args, ok=True):
        run = subprocess.run([sys.executable, str(SCRIPT), *map(str, args)],
                             cwd=self.directory, capture_output=True, text=True, timeout=90)
        data = json.loads(run.stdout)
        self.assertEqual(run.returncode, 0 if ok else 1, data)
        self.assertEqual(data['ok'], ok, data)
        self.assertEqual(data['protocol_version'], 1)
        return data

    def new(self):
        self.call('new', 'house.json')
        return self.directory / 'house.json'

    def test_discovery_and_bounded_query(self):
        self.assertEqual(self.call('describe')['asset_count'], 17)
        self.assertEqual(len(self.call('query', 'chair furniture', '--limit', 1)['matches']), 1)
        self.assertEqual(self.call('query', 'xyznonexistent')['matches'], [])
        self.call('query', 'chair', '--limit', 21, ok=False)
        self.call('nonsense', ok=False)
        self.assertIn('$defs', self.call('schema', 'patch'))

    def test_every_asset_and_recipe_compiles(self):
        self.new()
        entries = self.call('assets')['assets'] + self.call('recipes')['recipes']
        for item in entries:
            with self.subTest(item=item['id']):
                target = item['id'] + '.json'
                self.call('add', 'house.json', item['id'], target, '--id', 'fixture', '--at=-1.8,0,-11.2')
                self.call('map', 'audit', target)
                self.call('map', 'select', target, 'fixture')

    def test_failed_transaction_preserves_files(self):
        source = self.new()
        original = source.read_bytes()
        self.call('add', 'house.json', 'cover_block', 'house.json', '--id', 'x', ok=False)
        self.assertEqual(source.read_bytes(), original)
        self.call('add', 'house.json', 'unknown', 'bad.json', '--id', 'x', ok=False)
        self.call('add', 'house.json', 'cover_block', 'bad.json', '--id', '../bad', ok=False)
        self.call('add', 'house.json', 'cover_block', 'bad.json', '--id', 'x', '--at=nan,0,0', ok=False)
        self.assertFalse((self.directory / 'bad.json').exists())
        self.call('add', 'house.json', 'cover_block', 'one.json', '--id', 'same', '--at=10,0,10')
        self.call('add', 'one.json', 'cover_block', 'bad.json', '--id', 'same', '--at=10,0,10', ok=False)
        self.assertFalse((self.directory / 'bad.json').exists())

    def test_route_and_failure_report(self):
        self.new()
        self.call('verify', 'house.json', 'report', '--route', ROOT / 'tools/examples/upstairs.route.json',
                  '--route', ROOT / 'tools/examples/garden.route.json')
        self.assertTrue(json.loads((self.directory / 'report/report.json').read_text())['ok'])
        failed = self.call('verify', 'missing.json', 'failed', ok=False)
        self.assertIn('error', failed)
        self.assertFalse(json.loads((self.directory / 'failed/report.json').read_text())['ok'])
        self.call('verify', 'house.json', 'report', ok=False)
        self.call('verify', 'house.json', 'unused', '--baseline', 'anything', ok=False)


if __name__ == '__main__':
    unittest.main()

