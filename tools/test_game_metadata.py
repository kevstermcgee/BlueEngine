"""Portable metadata travels in files; inspection never runs untrusted content."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import unittest
from unittest import mock
import zipfile

from tools import game_metadata as metadata


def document():
    return {'format': 'blueengine.game', 'schema_version': 1,
            'game': {'slug': 'star-route', 'title': '星の道', 'version': '1.2.3',
                     'description': 'Choose your own route.', 'controls': 'Arrows move; A jumps',
                     'authors': ['Public Creator'], 'tags': ['puzzle']},
            'source': {'repository': 'studio/games', 'commit': 'a' * 40, 'game_path': 'games/star-route'},
            'engine': {'repository': 'studio/engine', 'commit': 'b' * 40}}


def pe(*, overlay=b'', certificate=b''):
    """A bounded PE fixture, not a runnable Windows game or installer."""
    data = bytearray(1024)
    data[:2] = b'MZ'
    struct.pack_into('<I', data, 60, 128)
    data[128:132] = b'PE\0\0'
    struct.pack_into('<HH', data, 132, 0x8664, 1)
    struct.pack_into('<H', data, 148, 240)
    struct.pack_into('<H', data, 152, 0x20b)
    struct.pack_into('<I', data, 152 + 108, 16)
    data[392:400] = b'.text\0\0\0'
    struct.pack_into('<II', data, 392 + 16, 512, 512)
    data.extend(overlay)
    if certificate:
        data.extend(b'\0' * (-len(data) % 8))
        struct.pack_into('<II', data, 152 + 112 + 32, len(data), len(certificate))
        data.extend(certificate)
    return bytes(data)


class MetadataTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='metadata 星 ')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def file(self, name, data):
        path = self.root / name
        path.write_bytes(data)
        return path

    def test_renamed_executable_roundtrip_and_idempotent_replacement(self):
        path = self.file('original.exe', pe())
        original = path.read_bytes()
        metadata.embed(path, document())
        self.assertTrue(path.read_bytes().startswith(original))
        first = path.read_bytes()
        metadata.embed(path, document())
        self.assertEqual(path.read_bytes(), first)
        renamed = path.rename(self.root / 'renamed.exe')
        with mock.patch.object(subprocess, 'run', side_effect=AssertionError('inspection executed code')):
            self.assertEqual(metadata.extract(renamed), document())
        changed = document()
        changed['game']['version'] = '2.0'
        metadata.embed(renamed, changed)
        self.assertEqual(metadata.extract(renamed), changed)

    def test_known_inno_output_keeps_original_overlay(self):
        path = self.file('game-setup.exe', pe(overlay=b'known producer compressed installer payload'))
        before = path.read_bytes()
        with self.assertRaisesRegex(metadata.MetadataError, 'unknown executable overlay'):
            metadata.embed(path, document())
        self.assertEqual(path.read_bytes(), before)
        metadata.embed(path, document(), allow_inno=True)
        self.assertTrue(path.read_bytes().startswith(before))
        self.assertEqual(metadata.extract(path), document())

    def test_signed_executable_read_before_certificate_but_never_modified(self):
        certificate = struct.pack('<IHH', 16, 0x200, 2) + b'fixture!'
        path = self.file('signed.exe', pe(overlay=metadata.make_envelope(document()), certificate=certificate))
        before = path.read_bytes()
        self.assertEqual(metadata.extract(path), document())
        with self.assertRaisesRegex(metadata.MetadataError, 'signed executable'):
            metadata.embed(path, document(), allow_inno=True)
        self.assertEqual(path.read_bytes(), before)
        path.write_bytes(before + metadata.make_envelope(document()))
        with self.assertRaisesRegex(metadata.MetadataError, 'certificate extent'):
            metadata.extract(path)

    def test_tampered_truncated_and_oversized_envelope_rejected(self):
        original = pe() + metadata.make_envelope(document())
        path = self.file('tampered.exe', original)
        broken = bytearray(original)
        broken[1030] ^= 1
        path.write_bytes(broken)
        with self.assertRaisesRegex(metadata.MetadataError, 'checksum'):
            metadata.extract(path)
        broken = bytearray(original)
        struct.pack_into('<Q', broken, len(broken) - len(metadata.MAGIC) - 8, metadata.MAX_PAYLOAD + 1)
        path.write_bytes(broken)
        with self.assertRaisesRegex(metadata.MetadataError, 'length'):
            metadata.extract(path)
        path.write_bytes(b'MZ')
        with self.assertRaises(metadata.MetadataError):
            metadata.extract(path)

    def test_batch_comments_preserve_body_and_never_execute(self):
        body = b'@echo off\r\necho harmless & exit /b 0\r\n'
        path = self.file('launch.bat', body)
        metadata.embed(path, document())
        original = path.read_bytes()
        self.assertTrue(original.endswith(body))
        self.assertTrue(all(line.startswith(b'@rem ') for line in original[:-len(body)].splitlines()))
        with mock.patch.object(subprocess, 'run', side_effect=AssertionError('executed batch')):
            self.assertEqual(metadata.extract(path), document())
        metadata.embed(path, document())
        self.assertEqual(path.read_bytes(), original)
        path.write_bytes(original.replace(b'@rem ey', b'@rem & ', 1))
        with self.assertRaises(metadata.MetadataError):
            metadata.extract(path)

    def test_zip_renamed_and_extracted_manifest_keep_identity(self):
        path = self.root / 'game.zip'
        with zipfile.ZipFile(path, 'w') as archive:
            archive.writestr('Play-game.exe', pe(), compress_type=zipfile.ZIP_DEFLATED)
            archive.writestr('assets/map.json', '{}')
            archive.writestr(metadata.MANIFEST, metadata.canonical_payload(document()), compress_type=zipfile.ZIP_DEFLATED)
        with self.assertRaises(metadata.MetadataError):
            metadata.extract(path)
        metadata.embed(path, document())
        renamed = path.rename(self.root / 'different-name.zip')
        self.assertEqual(metadata.extract(renamed), document())
        with zipfile.ZipFile(renamed) as archive:
            self.assertEqual(archive.read('Play-game.exe'), pe())
            self.assertEqual(archive.getinfo(metadata.MANIFEST).compress_type, zipfile.ZIP_STORED)
            self.assertEqual(metadata.decode_payload(archive.read(metadata.MANIFEST)), document())
            self.assertTrue(archive.comment.endswith(metadata.MAGIC))
        metadata.embed(renamed, document())
        self.assertEqual(metadata.extract(renamed), document())

    def test_zip_comment_mismatch_aliases_and_duplicates_rejected(self):
        for name in ('BLUEENGINE.GAME.JSON', metadata.MANIFEST):
            with self.subTest(name=name):
                path = self.root / 'bad.zip'
                with zipfile.ZipFile(path, 'w') as archive:
                    archive.writestr(metadata.MANIFEST, metadata.canonical_payload(document()))
                    if name != metadata.MANIFEST:
                        archive.writestr(name, metadata.canonical_payload(document()))
                    changed = document()
                    changed['game']['title'] = 'different'
                    archive.comment = metadata.make_envelope(changed)
                with self.assertRaises(metadata.MetadataError):
                    metadata.extract(path)

    def test_unsafe_archive_members_and_script_encodings_are_not_stamped(self):
        for member in ('../outside.exe', './blueengine.game.json', 'dir\\play.exe', '/root.exe', 'NUL.txt', 'dir/trailing. '):
            with self.subTest(member=member):
                path = self.root / 'unsafe.zip'
                with zipfile.ZipFile(path, 'w') as archive:
                    archive.writestr(member, b'payload')
                original = path.read_bytes()
                with self.assertRaises(metadata.MetadataError):
                    metadata.embed(path, document())
                self.assertEqual(path.read_bytes(), original)
        for prefix in (b'\xef\xbb\xbf', b'\xff\xfe', b'\xfe\xff'):
            path = self.file('encoded.bat', prefix + b'@echo off')
            original = path.read_bytes()
            with self.assertRaises(metadata.MetadataError):
                metadata.embed(path, document())
            self.assertEqual(path.read_bytes(), original)

    def test_fake_directory_in_zip_comment_cannot_bypass_inspection_limits(self):
        path = self.root / 'ambiguous.zip'
        with zipfile.ZipFile(path, 'w') as archive:
            archive.writestr('game.exe', b'owned payload')
            # This fake EOCD deliberately has a comment length that misses EOF.
            # stdlib zipfile still picks it, whereas a validated ZIP reader must
            # keep the real record and its bounded directory authoritative.
            archive.comment = struct.pack('<4sHHHHIIH', b'PK\x05\x06', 0, 0, 1, 1,
                                          metadata.MAX_DIRECTORY + 1, 0, 0) + b'x'
        with mock.patch.object(metadata.zipfile, 'ZipFile', side_effect=AssertionError('unbounded archive parser reached')):
            with self.assertRaisesRegex(metadata.MetadataError, 'ambiguous ZIP end record'):
                metadata.extract(path)

    def test_public_lineage_is_kept_without_claiming_parent_as_current_source(self):
        (self.root / 'Cargo.toml').write_text('[package]\nname="child-game"\nversion="1.0.0"\n')
        receipt = {'source': document()['source'], 'lineage': {'root_game': 'ancestor', 'parent_game': 'parent', 'parent_revision': 'a' * 64, 'revision_id': 'b' * 64}}
        (self.root / 'community-source.json').write_text(json.dumps(receipt))
        result = metadata.build_document(self.root)
        self.assertEqual(result['lineage'], receipt['lineage'])
        self.assertIsNone(result['source'])

    def test_shipping_parsed_cargo_preserves_metadata_without_python311_tomllib(self):
        (self.root / 'Cargo.toml').write_text('[package]\nname="portable-game"\nversion="3.10.0"\n')
        parsed = {'package': {'name': 'portable-game', 'version': '3.10.0',
                              'authors': ['Public Name <private@example.invalid>'], 'keywords': ['offline']}}
        with mock.patch.dict('sys.modules', {'tomllib': None}):
            result = metadata.build_document(self.root, cargo=parsed)
            self.assertEqual(result['game']['version'], '3.10.0')
            self.assertEqual(result['game']['authors'], ['Public Name'])
            self.assertEqual(result['game']['tags'], ['offline'])
            with self.assertRaisesRegex(metadata.MetadataError, 'Python 3.11.*scripts/ship.py package'):
                metadata.build_document(self.root)

    def test_legacy_unicode_title_uses_stable_cargo_identity(self):
        (self.root / 'assets').mkdir()
        (self.root / 'assets/identity.json').write_text(json.dumps({'title': '星の道', 'tagline': 'An original route', 'controls': 'Arrows'}))
        parsed = {'package': {'name': 'star_route', 'version': '1.0.0'}}
        result = metadata.build_document(self.root, cargo=parsed)
        self.assertEqual(result['game']['title'], '星の道')
        self.assertEqual(result['game']['slug'], 'star-route')

    def test_tree_content_hint_requires_a_complete_git_object_id(self):
        for tree in (None, '', 'abc123', 'not-a-tree', 42):
            changed = document()
            changed['source']['tree'] = tree
            with self.subTest(tree=tree), self.assertRaisesRegex(metadata.MetadataError, 'source.tree'):
                metadata.canonical_payload(changed)
        changed['source']['tree'] = 'c' * 40
        self.assertEqual(metadata.decode_payload(metadata.canonical_payload(changed)), changed)

    def test_noncanonical_duplicate_invalid_source_and_oversize_rejected(self):
        with self.assertRaisesRegex(metadata.MetadataError, 'canonical'):
            metadata.decode_payload(json.dumps(document()).encode())
        payload = metadata.canonical_payload(document()).replace(b'"schema_version":1', b'"schema_version":1,"schema_version":1')
        with self.assertRaisesRegex(metadata.MetadataError, 'duplicate'):
            metadata.decode_payload(payload)
        for field in ('../../secret', 'C:/private', 'games/CON', 'games/a\\b'):
            changed = document()
            changed['source']['game_path'] = field
            with self.assertRaises(metadata.MetadataError):
                metadata.canonical_payload(changed)
        changed = document()
        changed['wiki'] = {'text': ['x' * 16000] * 4}
        with self.assertRaisesRegex(metadata.MetadataError, 'exceeds'):
            metadata.canonical_payload(changed)

    def test_clean_source_only_and_public_authors(self):
        root = self.root
        (root / 'assets').mkdir()
        (root / 'assets/identity.json').write_text(json.dumps({'title': 'A route', 'tagline': 'Original game', 'controls': 'WASD'}))
        (root / 'Cargo.toml').write_text('[package]\nname="a-route"\nversion="0.1.0"\nauthors=["Public Name <private@example.com>"]\n')
        def git(*args):
            subprocess.run(['git', '-C', str(root), *args], check=True, capture_output=True)
        git('init')
        git('config', 'user.email', 'fixture@example.invalid')
        git('config', 'user.name', 'Fixture')
        git('remote', 'add', 'origin', 'https://github.com/public/games.git')
        git('add', '.')
        git('commit', '-m', 'fixture')
        result = metadata.build_document(root)
        self.assertEqual(result['source']['repository'], 'public/games')
        self.assertEqual(result['source']['game_path'], '.')
        self.assertEqual(result['source']['tree'], subprocess.run(['git', '-C', str(root), 'rev-parse', 'HEAD^{tree}'], check=True, capture_output=True, text=True).stdout.strip())
        self.assertEqual(result['game']['authors'], ['Public Name'])
        self.assertNotIn('private@example.com', metadata.canonical_payload(result).decode())
        (root / 'untracked.rs').write_text('unpublished source')
        self.assertIsNone(metadata.build_document(root)['source'])
        self.assertIsNone(metadata.build_document(root, source=None)['source'])


if __name__ == '__main__':
    unittest.main()
