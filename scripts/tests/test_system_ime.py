"""Guard failures must never launch a window or inject input."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

path = Path(__file__).resolve().parents[1] / 'test-system-ime.py'
spec = importlib.util.spec_from_file_location('system_ime_probe', path)
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class ImeGuards(unittest.TestCase):
    def test_non_macos_never_loads_native_libraries_or_launches(self):
        with patch.object(probe.sys, 'platform', 'linux'), \
                patch.object(probe, 'MacInput') as native, \
                patch.object(probe.subprocess, 'Popen') as launch:
            with self.assertRaisesRegex(RuntimeError, 'macOS is required'):
                probe.run(Path('/not-launched'))
            native.assert_not_called()
            launch.assert_not_called()

    def test_wrong_source_never_requests_permission_or_launches(self):
        with patch.object(probe.sys, 'platform', 'darwin'), \
                patch.object(probe, 'MacInput') as native, \
                patch.object(probe.subprocess, 'Popen') as launch:
            native.return_value.source.return_value = 'com.apple.keylayout.ABC'
            with self.assertRaisesRegex(RuntimeError, 'select Simplified Pinyin'):
                probe.run(Path('/not-launched'))
            native.return_value.cg.CGPreflightPostEventAccess.assert_not_called()
            native.return_value.key.assert_not_called()
            launch.assert_not_called()

    def test_missing_event_permission_never_launches_or_injects(self):
        with patch.object(probe.sys, 'platform', 'darwin'), \
                patch.object(probe, 'MacInput') as native, \
                patch.object(probe.subprocess, 'Popen') as launch:
            native.return_value.source.return_value = 'com.apple.inputmethod.SCIM.ITABC'
            native.return_value.cg.CGPreflightPostEventAccess.return_value = False
            with self.assertRaisesRegex(RuntimeError, 'does not request permissions'):
                probe.run(Path('/not-launched'))
            native.return_value.key.assert_not_called()
            launch.assert_not_called()

    def test_app_mode_without_fresh_log_never_launches_or_injects(self):
        with patch.object(probe.sys, 'platform', 'darwin'), \
                patch.object(probe, 'MacInput') as native, \
                patch.object(probe.subprocess, 'Popen') as launch:
            native.return_value.source.return_value = 'com.apple.inputmethod.SCIM.ITABC'
            native.return_value.cg.CGPreflightPostEventAccess.return_value = True
            with self.assertRaisesRegex(ValueError, 'requires a fresh --log'):
                probe.run(Path('/not-launched'), native_app=Path('/not-launched.app'))
            native.return_value.key.assert_not_called()
            launch.assert_not_called()


if __name__ == '__main__':
    unittest.main()
