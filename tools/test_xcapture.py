"""The capture helper builds the command that was proven to work, and explains itself when it cannot run."""
import contextlib
import io
import unittest
import tempfile
from types import SimpleNamespace
from pathlib import Path
from unittest.mock import patch

from tools import xcapture


class CommandTests(unittest.TestCase):
    def test_the_command_uses_a_virtual_screen_of_the_requested_size_and_exits_after_the_last_frame(self):
        cmd = xcapture.build_command(Path('/g/game'), '30,300', '1280x720', Path('/tmp/o'), ['--character', 'ghost'])
        self.assertEqual(cmd[:4], ['xvfb-run', '-a', '-s', '-screen 0 1280x720x24'])
        self.assertIn('--capture', cmd)
        self.assertEqual(cmd[cmd.index('--frames') + 1], '30,300')
        self.assertEqual(cmd[cmd.index('--exit-after') + 1], '320')
        self.assertEqual(cmd[-2:], ['--character', 'ghost'])
        self.assertIn('--mute', cmd)

    def test_explicit_audio_verification_is_not_overridden_by_implicit_mute(self):
        cmd = xcapture.build_command(Path('/g/game'), '30', '640x480', Path('/tmp/o'), ['--audible'])
        self.assertIn('--audible', cmd)
        self.assertNotIn('--mute', cmd)
        explicit = xcapture.build_command(Path('/g/game'), '30', '640x480', Path('/tmp/o'), ['--audible', '--mute'])
        self.assertIn('--mute', explicit)

    def test_success_retains_executable_verification_report(self):
        with tempfile.TemporaryDirectory() as d:
            game = Path(d) / 'game'
            game.touch()
            out = Path(d) / 'shots'
            out.mkdir()
            (out / 'shot_00030.png').touch()
            report = '{"audio":{"loaded_effects":18},"garden":{"outcome":"Won"}}'
            done = SimpleNamespace(returncode=0, stdout=report+'\n', stderr='')
            output = io.StringIO()
            with patch.object(xcapture.shutil, 'which', return_value='xvfb-run'), patch.object(xcapture.subprocess, 'run', return_value=done), contextlib.redirect_stdout(output):
                self.assertEqual(xcapture.main([str(game), '--out', str(out), '--', '--audible']), 0)
            self.assertIn(report, output.getvalue())

    def test_missing_xvfb_says_how_to_install_it(self):
        err = io.StringIO()
        with patch.object(xcapture.shutil, 'which', return_value=None), contextlib.redirect_stderr(err):
            self.assertEqual(xcapture.main(['game']), 2)
        self.assertIn('sudo apt install xvfb', err.getvalue())

    def test_a_missing_game_binary_is_reported_before_anything_runs(self):
        err = io.StringIO()
        with patch.object(xcapture.shutil, 'which', return_value='/usr/bin/xvfb-run'), contextlib.redirect_stderr(err):
            self.assertEqual(xcapture.main(['/no/such/game']), 2)
        self.assertIn('build the game first', err.getvalue())


class NativeProbeTests(unittest.TestCase):
    def script(self, value):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        path = Path(directory.name) / 'input.json'
        path.write_text(__import__('json').dumps(value))
        return path

    def test_device_sequences_cover_holds_release_repeat_simultaneous_mouse_and_state(self):
        steps = [
            {'key': 'space', 'down': True}, {'key': 'd', 'down': True},
            {'wait': {'field': 'accepted_input.action_ticks', 'gte': 1}},
            {'key': 'space', 'down': False}, {'key': 'd', 'down': False},
            {'key': 'space', 'down': True}, {'key': 'space', 'down': False},
            {'move': [120, 210]}, {'button': 1, 'down': True}, {'button': 1, 'down': False},
            {'button': 3, 'down': True}, {'move': [160, 230]}, {'button': 3, 'down': False},
            {'expect': {'field': 'state.actions', 'eq': 3}},
        ]
        self.assertEqual(xcapture.input_steps(self.script(steps)), steps)
        self.assertTrue(xcapture.matches({'state': {'actions': 3}}, steps[-1]['expect']))
        self.assertFalse(xcapture.matches({'state': {}}, steps[-1]['expect']))
        self.assertFalse(xcapture.matches({'state': {'actions': 2}}, steps[-1]['expect']))

    def test_bad_scripts_and_oversized_sequences_fail_before_launch(self):
        for value in ([], [None], [{'key': 'x', 'down': 'yes'}], [{'button': 9, 'down': True}],
                      [{'move': [0]}], [{'wait': {'field': 'tick'}}], [{'key':'a', 'down':True}]*257,
                      [{'expect': {'field': 'tick', 'gte': 1}}], [{'key':'a', 'down':True}]):
            with self.subTest(value=str(value)[:60]), self.assertRaises(ValueError):
                xcapture.input_steps(self.script(value))

    def test_replay_cannot_certify_native_controls(self):
        path = self.script([{'key':'a', 'down':True}, {'expect': {'field': 'tick', 'gte': 1}}])
        game = path.parent / 'game'
        game.touch()
        with patch.object(xcapture.shutil, 'which', return_value='xvfb-run'), \
                patch.object(xcapture.subprocess, 'run') as run, contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(xcapture.main([str(game), '--input', str(path), '--', '--verify']), 4)
        run.assert_not_called()

    def test_input_probe_shares_the_games_virtual_display_and_preserves_arguments(self):
        path = self.script([{'key':'a', 'down':True}, {'expect': {'field': 'tick', 'gte': 1}}])
        game = path.parent / 'game'
        game.touch()
        out = path.parent / 'shots'
        out.mkdir()
        (out / 'shot_00030.png').touch()
        with patch.object(xcapture.shutil, 'which', return_value='xvfb-run'), \
                patch.object(xcapture.subprocess, 'run', return_value=SimpleNamespace(returncode=0, stdout='', stderr='')) as run:
            self.assertEqual(xcapture.main([str(game), '--input', str(path), '--out', str(out)]), 0)
        argv = run.call_args.args[0]
        self.assertEqual(argv[:4], ['xvfb-run', '-a', '-s', '-screen 0 1280x720x24'])
        self.assertIn('--drive-input', argv)
        self.assertIn('--input-report', argv)
        self.assertNotIn('--verify', argv)


if __name__ == '__main__':
    unittest.main()
