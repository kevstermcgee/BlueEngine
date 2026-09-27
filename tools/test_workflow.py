"""Selection tests use real Git changes, not only hand-written path lists."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from tools import workflow

ROOT = Path(__file__).resolve().parents[1]


class ContextTests(unittest.TestCase):
    def test_bounded_exact_and_unknown(self):
        hit = workflow.context(ROOT, 'movement', 1)
        self.assertEqual(hit['matches'][0]['id'], 'movement')
        self.assertEqual(len(hit['matches']), 1)
        self.assertFalse(hit['engine_source_read'])
        self.assertLess(len(json.dumps(hit)), 3000)
        self.assertEqual(workflow.context(ROOT, 'zyxquantumunknown')['matches'], [])
        for query, limit in [('', 3), ('x' * 101, 3), ('a', 0), ('a', 6)]:
            with self.assertRaises(ValueError):
                workflow.context(ROOT, query, limit)

    def test_lookup_needs_only_feature_index(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'tools').mkdir()
            (root / 'tools/FEATURES.json').write_bytes((ROOT / 'tools/FEATURES.json').read_bytes())
            self.assertTrue(workflow.context(root, 'shared_gameplay')['matches'])


class SelectionTests(unittest.TestCase):
    def test_all_original_engine_gates_remain(self):
        commands = workflow.full_commands()
        self.assertIn(['cargo', 'fmt', '--check'], commands)
        for features in ([], ['--no-default-features']):
            self.assertIn(['cargo', 'test', '--locked', *features], commands)
            self.assertIn(['cargo', 'clippy', '--all-targets', '--locked', *features,
                           '--', '-D', 'warnings'], commands)
            self.assertIn(['cargo', 'rustdoc', '--locked', '--lib', *features,
                           '--', '-D', 'warnings'], commands)
        for script in ['tools/check_headless.py', 'tools/check_authoring.py']:
            self.assertIn([sys.executable, script], commands)

    def test_scopes_union_and_fail_closed(self):
        plan = workflow.validation_plan(['tools/author.py', 'tools/assets.py'])
        self.assertEqual(plan['scope'], 'author+assets')
        self.assertEqual(len(plan['commands']), 3)
        for path in ['src/viewer/controller.rs', 'Cargo.lock', 'README.md',
                     'tools/workflow.py', 'tools/FEATURES.json', '.github/workflows/ci.yml',
                     'assets/games/new/game.json', 'unrecognized.py']:
            plan = workflow.validation_plan(['tools/assets.py', path])
            self.assertEqual(plan['scope'], 'full', path)
            self.assertEqual(plan['commands'], workflow.full_commands())
        self.assertEqual(workflow.validation_plan([])['scope'], 'no_changes')
        self.assertEqual(workflow.validation_plan([])['commands'], [])

    def test_real_git_includes_staged_unstaged_untracked_deleted_and_renamed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def git(*args):
                return subprocess.run(['git', *args], cwd=root, check=True,
                                      capture_output=True, text=True, **workflow.console_options()).stdout.strip()
            git('init')
            git('config', 'user.email', 'test@example.invalid')
            git('config', 'user.name', 'Workflow Test')
            for name in ['staged.py', 'unstaged.py', 'deleted.py', 'old name.py']:
                (root / name).write_text('old\n')
            git('add', '.')
            git('commit', '-m', 'base')
            base = git('rev-parse', 'HEAD')
            (root / 'staged.py').write_text('new\n')
            git('add', 'staged.py')
            (root / 'unstaged.py').write_text('new\n')
            (root / 'deleted.py').unlink()
            git('mv', 'old name.py', 'new name.py')
            (root / 'untracked space.py').write_text('new\n')
            revision, paths = workflow.changed_paths(root)
            self.assertEqual(revision, base)
            self.assertEqual(set(paths), {'staged.py', 'unstaged.py', 'deleted.py',
                                         'old name.py', 'new name.py', 'untracked space.py'})
            git('add', '.')
            git('commit', '-m', 'changed')
            self.assertEqual(workflow.changed_paths(root)[1], [])
            self.assertEqual(workflow.changed_paths(root, base)[1], paths)
            for invalid in ['nonexistent', '--help']:
                with self.assertRaises(subprocess.CalledProcessError):
                    workflow.changed_paths(root, invalid)


class ConsoleTests(unittest.TestCase):
    @unittest.skipUnless(sys.platform == 'win32', 'Windows console inheritance')
    def test_child_and_grandchild_consoles_stay_hidden(self):
        from tools.test_game_check import game_check
        probe = ('import ctypes; '
                 'k=ctypes.windll.kernel32; k.GetConsoleWindow.restype=ctypes.c_void_p; '
                 'u=ctypes.windll.user32; u.IsWindowVisible.argtypes=[ctypes.c_void_p]; '
                 'h=k.GetConsoleWindow(); print(bool(h), bool(u.IsWindowVisible(h)))')
        child = (probe + '; import subprocess,sys; '
                 'subprocess.run([sys.executable,"-c",' + repr(probe) + '],check=True)')
        for options in [workflow.console_options(), game_check.console_options()]:
            result = subprocess.run([sys.executable, '-c', child], capture_output=True,
                                    text=True, check=True, **options)
            self.assertEqual(result.stdout.splitlines(), ['True False', 'True False'])


if __name__ == '__main__':
    unittest.main()
