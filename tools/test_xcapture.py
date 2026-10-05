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


if __name__ == '__main__':
    unittest.main()
