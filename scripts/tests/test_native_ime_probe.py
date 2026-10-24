import contextlib
import io
from pathlib import Path
import plistlib
import sys
import tempfile
import threading
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import native_ime_probe as probe


class NativeImeTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.bundle = self.root / 'Probe.app'
        self.program = self.bundle / 'Contents/MacOS/Probe'
        self.program.parent.mkdir(parents=True)
        self.program.write_bytes(b'fixed program')
        (self.bundle / 'Contents/Info.plist').write_bytes(plistlib.dumps({
            'CFBundleExecutable': 'Probe', 'CFBundleIdentifier': 'app.probe',
            'CFBundlePackageType': 'APPL'}))
        self.mac = Mock()
        self.mac.source.return_value = probe.SOURCE
        self.identity = {'pid': 1234, 'bundleId': 'app.probe'}

    def driver(self):
        return probe.AppInputDriver(self.mac, self.program, 'app.probe')

    def line(self, driver, stage, alive=lambda: True):
        driver.line(f'selftest: Log("SYSTEM_IME_{stage}_READY")', alive)

    def test_wrong_foreground_bundle_or_executable_posts_no_key(self):
        for identity, executable in [
            ({'pid': 12, 'bundleId': 'another.app'}, self.program),
            (self.identity, self.root / 'different-program'),
            ({'pid': None, 'bundleId': 'app.probe'}, self.program),
        ]:
            with self.subTest(identity=identity, executable=executable), \
                    patch.object(probe, 'frontmost_identity', return_value=identity), \
                    patch.object(probe, 'executable_for_pid', return_value=executable):
                with self.assertRaisesRegex(RuntimeError, 'exact foreground'):
                    self.line(self.driver(), 'TYPE')
                self.mac.key.assert_not_called()

    def test_source_change_or_closed_app_posts_no_key(self):
        with patch.object(probe, 'frontmost_identity', return_value=self.identity), \
                patch.object(probe, 'executable_for_pid', return_value=self.program):
            self.mac.source.return_value = 'other-source'
            with self.assertRaisesRegex(RuntimeError, 'input source changed'):
                self.line(self.driver(), 'TYPE')
            self.mac.key.assert_not_called()
            with self.assertRaisesRegex(RuntimeError, 'already closed'):
                self.line(self.driver(), 'TYPE', lambda: False)
            self.mac.key.assert_not_called()

    def test_changed_pid_cannot_receive_commit_even_with_same_bundle_and_path(self):
        driver = self.driver()
        with patch.object(probe, 'frontmost_identity', return_value=self.identity) as identity, \
                patch.object(probe, 'executable_for_pid', return_value=self.program):
            self.line(driver, 'TYPE')
            self.assertEqual(self.mac.key.call_count, 5)
            identity.return_value = {**self.identity, 'pid': 9999}
            with self.assertRaisesRegex(RuntimeError, 'exact foreground'):
                self.line(driver, 'COMMIT')
            self.assertEqual(self.mac.key.call_count, 5)

    def test_focus_change_mid_type_stops_remaining_keys_without_retargeting(self):
        with patch.object(probe, 'frontmost_identity', side_effect=[
            self.identity, self.identity, {'pid': 9999, 'bundleId': 'another.app'},
        ]), patch.object(probe, 'executable_for_pid', return_value=self.program):
            with self.assertRaisesRegex(RuntimeError, 'exact foreground'):
                self.line(self.driver(), 'TYPE')
        self.assertEqual([call.args for call in self.mac.key.call_args_list],
                         [(1234, 45), (1234, 34)])

    def test_duplicate_and_out_of_order_stages_do_not_send_more_keys(self):
        driver = self.driver()
        with patch.object(probe, 'frontmost_identity', return_value=self.identity), \
                patch.object(probe, 'executable_for_pid', return_value=self.program):
            with self.assertRaisesRegex(RuntimeError, 'out-of-order'):
                self.line(driver, 'COMMIT')
            self.mac.key.assert_not_called()
            self.line(driver, 'TYPE')
            with self.assertRaisesRegex(RuntimeError, 'duplicate'):
                self.line(driver, 'TYPE')
            self.assertEqual(self.mac.key.call_count, 5)

    def test_log_tail_waits_for_complete_line_and_split_utf8(self):
        filename = self.root / 'stream'
        tail = probe.StreamTail(filename)
        self.assertEqual(tail.read(), [])
        text = '组字\n'.encode()
        filename.write_bytes(text[:1])
        self.assertEqual(tail.read(), [])
        with filename.open('ab') as out:
            out.write(text[1:-1])
        self.assertEqual(tail.read(), [])
        with filename.open('ab') as out:
            out.write(text[-1:])
        self.assertEqual(tail.read(), ['组字\n'])
        self.assertEqual(tail.read(), [])

    def test_existing_evidence_is_preserved_without_launch(self):
        log = self.root / 'native.log'
        Path(str(log) + '.stderr').write_text('retained failure')
        with patch.object(probe, 'run_native_probe') as launch:
            with self.assertRaisesRegex(ValueError, 'preserve existing'):
                probe.run_app_ime(self.program, self.bundle, log, self.mac, probe.SOURCE)
            launch.assert_not_called()
        self.assertEqual(Path(str(log) + '.stderr').read_text(), 'retained failure')

    def test_normal_app_streams_live_stages_and_never_invents_app_exit_status(self):
        log = self.root / 'native.log'
        typed, committed = threading.Event(), threading.Event()

        def key(pid, code):
            self.assertEqual(pid, 1234)
            if self.mac.key.call_count == 5:
                typed.set()
            if code == 49:
                committed.set()
        self.mac.key.side_effect = key

        def launch(program, arguments, environment, output, marker, **options):
            self.assertEqual(program, self.program)
            self.assertEqual(environment, {'TEKES_SELFTEST': 'text'})
            self.assertEqual(options['bundle'], self.bundle)
            self.assertTrue(options['observe_frontmost'])
            filename = Path(str(output) + '.stderr')
            with filename.open('w') as stream:
                stream.write('selftest: Log("SYSTEM_IME_TYPE_READY")\n'); stream.flush()
                self.assertTrue(typed.wait(2))
                stream.write('selftest: Log("SYSTEM_IME_COMMIT_READY")\n'); stream.flush()
                self.assertTrue(committed.wait(2))
                stream.write(marker + 'original receipt\n')
            output.write_text(filename.read_text())
            return output.read_text()

        with patch.object(probe, 'run_native_probe', side_effect=launch), \
                patch.object(probe, 'frontmost_identity', return_value=self.identity), \
                patch.object(probe, 'executable_for_pid', return_value=self.program), \
                contextlib.redirect_stdout(io.StringIO()):
            result = probe.run_app_ime(self.program, self.bundle, log, self.mac, probe.SOURCE,
                                       observe_frontmost=True)
        self.assertEqual(result['profile'], 'launch-services-app')
        self.assertIsNone(result['applicationExitCode'])
        self.assertEqual(result['launcherExitCode'], 0)
        self.assertFalse(result['physicalKeyboard'])
        self.assertEqual([call.args[1] for call in self.mac.key.call_args_list],
                         [45, 34, 4, 0, 31, 49])

    def test_launcher_crash_is_not_hidden_or_retried(self):
        log = self.root / 'native.log'
        with patch.object(probe, 'run_native_probe', side_effect=AssertionError('native crash')) as launch:
            with self.assertRaisesRegex(AssertionError, 'native crash'):
                probe.run_app_ime(self.program, self.bundle, log, self.mac, probe.SOURCE)
            self.assertEqual(launch.call_count, 1)
            self.mac.key.assert_not_called()


if __name__ == '__main__':
    unittest.main()
