from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import native_acceptance_host as host


class FixedHostTests(unittest.TestCase):
    def test_history_cannot_be_used_as_fixed_host(self):
        with self.assertRaisesRegex(ValueError, 'archive-only'):
            host.validate_fixed_host(Path('/tmp/history/demo'), Path('/tmp/history.app'))

    def test_adhoc_or_other_team_never_passes_inspection(self):
        for bad in ('Signature=adhoc', 'TeamIdentifier=another-team'):
            lines = f'Identifier={host.BUNDLE_ID}\n{bad}\ndesignated => identifier "test"\n'
            with self.subTest(bad=bad), patch.object(host.subprocess, 'run', side_effect=[
                subprocess.CompletedProcess([], 0),
                subprocess.CompletedProcess([], 0, '', lines),
            ]), self.assertRaises(ValueError):
                host.inspect_signature('/tmp/not-launched.app')

    def test_running_host_is_not_replaced_or_signed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            program = root / 'source'
            program.write_bytes(b'original program')
            bundle = root / 'fixed.app'
            result = subprocess.CompletedProcess([], 0, 'p123\nn' + str(bundle / 'Contents/MacOS/demo') + '\n')
            with patch.object(host, 'HOST_ROOT', root), patch.object(host, 'BUNDLE', bundle), \
                    patch.object(host.subprocess, 'run', return_value=result) as execute:
                with self.assertRaisesRegex(RuntimeError, 'in use'):
                    host.install(program)
                self.assertEqual(execute.call_count, 1)
                self.assertEqual(program.read_bytes(), b'original program')

    def test_install_signs_private_copy_and_does_not_launch_or_capture(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            program = root / 'source'
            program.write_bytes(b'original program')
            bundle = root / 'fixed.app'
            commands = []

            def execute(command, **kwargs):
                commands.append(command)
                if command[0] == '/usr/sbin/lsof':
                    return subprocess.CompletedProcess(command, 0, 'p123\nn/unrelated\n')
                self.assertEqual(command[0], '/usr/bin/codesign')
                candidate = Path(command[-1])
                binary = candidate / 'Contents/MacOS' / host.EXECUTABLE
                self.assertNotEqual(binary.stat().st_ino, program.stat().st_ino)
                binary.write_bytes(b'signed private copy')
                return subprocess.CompletedProcess(command, 0)

            with patch.object(host, 'HOST_ROOT', root), patch.object(host, 'BUNDLE', bundle), \
                    patch.object(host, 'RECEIPT', root / 'identity.json'), \
                    patch.object(host.subprocess, 'run', side_effect=execute), \
                    patch.object(host, 'inspect_signature', return_value='designated => stable'):
                receipt = host.install(program)
                self.assertFalse(receipt['launched'])
                self.assertFalse(receipt['screenCapturePerformed'])
                self.assertEqual(program.read_bytes(), b'original program')
                self.assertEqual(host.validate_fixed_host(bundle / 'Contents/MacOS' / host.EXECUTABLE, bundle), receipt)
                (bundle / 'Contents/MacOS' / host.EXECUTABLE).write_bytes(b'tampered')
                with self.assertRaisesRegex(ValueError, 'receipt'):
                    host.validate_fixed_host(bundle / 'Contents/MacOS' / host.EXECUTABLE, bundle)

    def test_demo_and_candidate_sources_cannot_bypass_external_capture(self):
        sources = list((host.ROOT / 'ui/demo/src').glob('*.rs'))
        sources += list((host.ROOT / 'scripts/probes').glob('*.rs'))
        for path in sources:
            if path.name == 'acceptance_script.rs':
                continue
            self.assertNotIn('tekes_ui_platform_macos::selftest::', path.read_text(), str(path))

    def test_upgrade_rollback_and_upgrade_keep_one_live_identity_and_all_receipts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            program = root / 'source'
            bundle = root / 'fixed.app'
            signed = []

            def execute(command, **kwargs):
                if command[0] == '/usr/sbin/lsof':
                    return subprocess.CompletedProcess(command, 0, 'p123\nn/unrelated\n')
                self.assertEqual(command[0], '/usr/bin/codesign')
                self.assertEqual(command[command.index('--sign') + 1], host.SIGNING_IDENTITY)
                binary = Path(command[-1]) / 'Contents/MacOS' / host.EXECUTABLE
                signed.append(binary.read_bytes())
                binary.write_bytes(b'signed:' + binary.read_bytes())
                return subprocess.CompletedProcess(command, 0)

            with patch.object(host, 'HOST_ROOT', root), patch.object(host, 'BUNDLE', bundle), \
                    patch.object(host, 'RECEIPT', root / 'identity.json'), \
                    patch.object(host.subprocess, 'run', side_effect=execute), \
                    patch.object(host, 'inspect_signature', return_value='designated => stable'):
                for source in [b'A', b'B', b'A', b'C']:
                    program.write_bytes(source)
                    receipt = host.install(program)
                    self.assertEqual(receipt['bundlePath'], str(bundle))
                    self.assertEqual(receipt['bundleId'], host.BUNDLE_ID)
                    self.assertEqual(program.read_bytes(), source)
                    self.assertFalse(receipt['launched'])
                archives = list((root / 'archive').iterdir())
                self.assertEqual(len(archives), 3)
                self.assertCountEqual(
                    [(p / bundle.name / 'Contents/MacOS' / host.EXECUTABLE).read_bytes() for p in archives],
                    [b'signed:A', b'signed:B', b'signed:A'],
                )
                self.assertTrue(all((p / 'identity.json').is_file() for p in archives))
                # Reinstalling the same input is idempotent and never signs again.
                self.assertEqual(host.install(program), receipt)
                self.assertEqual(signed, [b'A', b'B', b'A', b'C'])


if __name__ == '__main__':
    unittest.main()
