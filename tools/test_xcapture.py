"""The capture helper builds the command that was proven to work, and explains itself when it cannot run."""
import contextlib
import io
import unittest
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
