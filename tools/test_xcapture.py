"""The capture helper builds the command that was proven to work, and explains itself when it cannot run."""
import contextlib
import io
import json
import subprocess
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
            report = '{"audio":{"loaded_effects":18},"garden":{"outcome":"Won"}}'
            done = SimpleNamespace(returncode=0, stdout=report+'\n', stderr='')

            def capture(command, **kwargs):
                self.assertFalse(out.exists())
                out.mkdir()
                (out / 'shot_00030.png').touch()
                Path(command[command.index('-e') + 1]).write_text('server warning\n', encoding='utf-8')
                return done

            output = io.StringIO()
            with patch.object(xcapture.shutil, 'which', return_value='xvfb-run'), patch.object(xcapture.subprocess, 'run', side_effect=capture), contextlib.redirect_stdout(output):
                self.assertEqual(xcapture.main([str(game), '--out', str(out), '--', '--audible']), 0)
            self.assertIn(report, output.getvalue())
            self.assertEqual((out / 'native.stdout.log').read_text(encoding='utf-8'), report+'\n')
            self.assertEqual((out / 'xvfb.stderr.log').read_text(encoding='utf-8'), 'server warning\n')

    def test_failed_server_startup_is_reported_and_retained_before_any_capture_directory_exists(self):
        with tempfile.TemporaryDirectory() as d:
            game = Path(d) / 'game'
            game.touch()
            out = Path(d) / 'shots'
            server_error = '_XSERVTransSocketUNIXCreateListener: failed to bind listener\nCannot establish any listening sockets\n'
            client_error = 'game panicked: XOpenDisplay() failed\n'

            def fail_startup(command, **kwargs):
                self.assertFalse(out.exists())
                Path(command[command.index('-e') + 1]).write_text(server_error, encoding='utf-8')
                return SimpleNamespace(returncode=101, stdout='', stderr=client_error)

            err = io.StringIO()
            with patch.object(xcapture.shutil, 'which', return_value='xvfb-run'), patch.object(xcapture.subprocess, 'run', side_effect=fail_startup), contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(err):
                self.assertEqual(xcapture.main([str(game), '--out', str(out)]), 101)
            self.assertIn(server_error, err.getvalue())
            self.assertIn(client_error, err.getvalue())
            self.assertIn(str(out), err.getvalue())
            self.assertEqual((out / 'xvfb.stderr.log').read_text(encoding='utf-8'), server_error)
            self.assertEqual((out / 'native.stderr.log').read_text(encoding='utf-8'), client_error)
            self.assertEqual(json.loads((out / 'subsystems.json').read_text(encoding='utf-8'))['rendering'], 'failed')

    def test_timeout_retains_partial_game_and_server_diagnostics(self):
        with tempfile.TemporaryDirectory() as d:
            game = Path(d) / 'game'
            game.touch()
            out = Path(d) / 'shots'

            def timeout(command, **kwargs):
                Path(command[command.index('-e') + 1]).write_text('server diagnostic\n', encoding='utf-8')
                raise subprocess.TimeoutExpired(command, 1, output=b'{"frame":30,"path":', stderr=b'partial game error\n')

            err = io.StringIO()
            with patch.object(xcapture.shutil, 'which', return_value='xvfb-run'), patch.object(xcapture.subprocess, 'run', side_effect=timeout), contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(err):
                self.assertEqual(xcapture.main([str(game), '--out', str(out), '--timeout', '1']), 3)
            self.assertIn('timed out after 1 s', err.getvalue())
            self.assertEqual((out / 'native.stdout.log').read_text(encoding='utf-8'), '{"frame":30,"path":')
            self.assertEqual((out / 'native.stderr.log').read_text(encoding='utf-8'), 'partial game error\n')
            self.assertEqual((out / 'xvfb.stderr.log').read_text(encoding='utf-8'), 'server diagnostic\n')

    def test_failed_process_with_malformed_frame_record_retains_all_diagnostics(self):
        for record in ('{"frame":30,"path":', '{"frame":30}'):
            with self.subTest(record=record), tempfile.TemporaryDirectory() as d:
                game = Path(d) / 'game'
                game.touch()
                out = Path(d) / 'shots'

                def fail(command, **kwargs):
                    Path(command[command.index('-e') + 1]).write_text('server failure\n', encoding='utf-8')
                    return SimpleNamespace(returncode=101, stdout=record, stderr='game failure\n')

                output, err = io.StringIO(), io.StringIO()
                with patch.object(xcapture.shutil, 'which', return_value='xvfb-run'), patch.object(xcapture.subprocess, 'run', side_effect=fail), contextlib.redirect_stdout(output), contextlib.redirect_stderr(err):
                    self.assertEqual(xcapture.main([str(game), '--out', str(out)]), 101)
                self.assertIn(record, output.getvalue())
                self.assertEqual((out / 'native.stdout.log').read_text(encoding='utf-8'), record)
                self.assertEqual((out / 'native.stderr.log').read_text(encoding='utf-8'), 'game failure\n')
                self.assertEqual((out / 'xvfb.stderr.log').read_text(encoding='utf-8'), 'server failure\n')
                self.assertEqual(json.loads((out / 'subsystems.json').read_text(encoding='utf-8'))['rendering'], 'failed')

    def test_an_existing_capture_is_preserved_and_cannot_count_as_a_new_success(self):
        with tempfile.TemporaryDirectory() as d:
            game = Path(d) / 'game'
            game.touch()
            out = Path(d) / 'shots'
            out.mkdir()
            prior = out / 'shot_00030.png'
            prior.write_bytes(b'previous capture')
            with patch.object(xcapture.shutil, 'which', return_value='xvfb-run'), patch.object(xcapture.subprocess, 'run') as run, contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(xcapture.main([str(game), '--out', str(out)]), 2)
            run.assert_not_called()
            self.assertEqual(prior.read_bytes(), b'previous capture')
            self.assertEqual(list(out.iterdir()), [prior])

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
