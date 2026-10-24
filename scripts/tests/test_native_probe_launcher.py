import contextlib
import io
import json
from pathlib import Path
import plistlib
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from native_probe_launcher import run_native_probe, validate_bundle
from native_frontmost_trace import FrontmostTrace, frontmost_identity


class NativeLaunchTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.base = Path(self.tmp.name)
        self.bundle = self.base / 'Probe.app'
        self.program = self.bundle / 'Contents/MacOS/Probe'
        self.program.parent.mkdir(parents=True)
        self.program.write_bytes(b'fixed acceptance program')
        with (self.bundle / 'Contents/Info.plist').open('wb') as out:
            plistlib.dump({'CFBundleExecutable': 'Probe', 'CFBundlePackageType': 'APPL'}, out)
        self.log = self.base / 'native.log'
        self.marker = 'NATIVE_RECEIPT '
        lease = patch('native_probe_launcher.host_lease', return_value=contextlib.nullcontext())
        lease.start()
        self.addCleanup(lease.stop)

    def launch(self, *, text='NATIVE_RECEIPT {}\n', status=0, timeout=False):
        def run(command, **kwargs):
            self.command = command
            Path(command[command.index('--stdout') + 1]).write_text(text)
            Path(command[command.index('--stderr') + 1]).write_text('original native stderr\n')
            if timeout:
                raise subprocess.TimeoutExpired(command, 1)
            return subprocess.CompletedProcess(command, status)
        with patch('native_probe_launcher.sys.platform', 'darwin'), patch('native_probe_launcher.subprocess.run', side_effect=run), contextlib.redirect_stdout(io.StringIO()) as output:
            result = run_native_probe(self.program, ['--probe'], {'TEKES_SELFTEST': 'text', 'AGENT_SUMMARY_ROW_CAPTURE': str(self.base / 'native.png'), 'UNRELATED_SERVICE_KEY': 'do-not-forward'}, self.log, self.marker, bundle=self.bundle)
        return result, output.getvalue()

    def test_app_identity_is_exact_before_launch(self):
        self.assertEqual(validate_bundle(self.program, self.bundle), self.bundle.resolve())
        with self.assertRaisesRegex(ValueError, 'exact'):
            validate_bundle(self.base / 'different-program', self.bundle)

    def test_normal_app_wait_retains_streams_and_does_not_invent_app_exit_code(self):
        result, output = self.launch()
        self.assertIn('original native stderr', result)
        self.assertIn(self.marker, result)
        proof = json.loads(output.split('NATIVE_PROBE_LAUNCH_RECEIPT ', 1)[1])
        self.assertIsNone(proof['applicationExitCode'])
        self.assertEqual(proof['launcherExitCode'], 0)
        self.assertEqual(proof['profile'], 'launch-services-app')
        self.assertNotIn('do-not-forward', str(self.command))
        self.assertIn('TEKES_SELFTEST=text', self.command)
        self.assertIn('AGENT_SUMMARY_ROW_CAPTURE=' + str(self.base / 'native.png'), self.command)
        self.assertEqual(self.command[-3:], [str(self.bundle.resolve()), '--args', '--probe'])
        self.assertNotEqual(self.command[self.command.index('--stdout') + 1], self.command[self.command.index('--stderr') + 1])

    def test_launcher_success_without_original_receipt_fails(self):
        with self.assertRaisesRegex(AssertionError, 'one complete'):
            self.launch(text='window closed\n')

    def test_duplicate_receipt_is_not_success(self):
        with self.assertRaisesRegex(AssertionError, 'one complete'):
            self.launch(text='NATIVE_RECEIPT {}\nNATIVE_RECEIPT {}\n')

    def test_panic_fails_even_after_receipt_and_launcher_zero_exit(self):
        with self.assertRaisesRegex(AssertionError, 'native crash'):
            self.launch(text='NATIVE_RECEIPT {}\nthread main panicked at source.rs:1\n')

    def test_launcher_failure_is_not_hidden_by_receipt(self):
        with self.assertRaises(AssertionError):
            self.launch(status=1)

    def test_timeout_preserves_streams_and_redacts_command(self):
        with self.assertRaisesRegex(RuntimeError, 'inspect the original app') as error:
            self.launch(timeout=True)
        self.assertNotIn('TEKES_SELFTEST', str(error.exception))
        self.assertIn('original native stderr', self.log.read_text())

    def test_direct_historical_launch_is_rejected_without_execution(self):
        with patch('native_probe_launcher.subprocess.run') as execute:
            with self.assertRaisesRegex(ValueError, 'fixed signed host'):
                run_native_probe(self.program, [], {}, self.log, self.marker)
            execute.assert_not_called()

    def test_existing_evidence_is_never_overwritten(self):
        original = Path(str(self.log) + '.stderr')
        original.write_text('historical evidence')
        with patch('native_probe_launcher.subprocess.run') as execute:
            with self.assertRaisesRegex(ValueError, 'preserve existing'):
                run_native_probe(self.program, [], {}, self.log, self.marker, bundle=self.bundle)
            execute.assert_not_called()
        self.assertEqual(original.read_text(), 'historical evidence')

    def test_frontmost_trace_only_queries_identity(self):
        with patch('native_frontmost_trace.subprocess.check_output', side_effect=['ASN:0x0-0x123:\n', '"pid"=1234\n"CFBundleIdentifier"="app.probe"\n']) as query:
            self.assertEqual(frontmost_identity()['pid'], 1234)
        self.assertEqual(query.call_args_list[0].args[0], ['/usr/bin/lsappinfo', 'front'])
        self.assertEqual(query.call_args_list[1].args[0], ['/usr/bin/lsappinfo', 'info', '-only', 'pid,bundleid,name', 'ASN:0x0-0x123:'])

    def test_frontmost_trace_rejects_unknown_pid_and_invalid_asn(self):
        for outputs in [['unknown'], ['ASN:0x0-0x123:', '"CFBundleIdentifier"="app.probe"']]:
            with self.subTest(outputs=outputs), patch('native_frontmost_trace.subprocess.check_output', side_effect=outputs), self.assertRaises(ValueError):
                frontmost_identity()

    def test_frontmost_trace_preserves_changes_and_reports_observation_failure(self):
        trace = FrontmostTrace(self.base / 'focus.jsonl')
        with patch('native_frontmost_trace.frontmost_identity', side_effect=[{'pid': 1}, {'pid': 1}, {'pid': 2}, ValueError('unavailable')]):
            for _ in range(4):
                trace.sample()
        rows = [json.loads(line) for line in trace.output.read_text().splitlines()]
        self.assertEqual([row.get('pid') for row in rows], [1, 2, None])
        self.assertTrue(rows[-1]['observationUnavailable'])


if __name__ == '__main__':
    unittest.main()
