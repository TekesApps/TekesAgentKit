"""Failure and identity boundaries for the consumer acceptance tools; no network."""
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
from acceptance_tools import (extract_source, headless_dependencies, local_packages,
                              sha256, snapshot, source_files, verify_files, verify_payload)


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


pin = load('pin', 'check-tekesui-pin.py')
replay = load('replay', 'verify-tekesui-handoff.py')
resolved_ui = load('resolved_ui', 'check-ui-consumer-inputs.py')


def manifest(files):
    hashes = {name: hashlib.sha256(value).hexdigest() for name, value in files.items()}
    return {'kind': 'development-snapshot-not-release', 'files': hashes,
            'sha256': hashlib.sha256(json.dumps(hashes, sort_keys=True,
                                               separators=(',', ':')).encode()).hexdigest()}


class Checks(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()

    def archive(self, members):
        archive = self.root / 'input.tar.gz'
        with tarfile.open(archive, 'w:gz') as tar:
            for name, kind in members:
                item = tarfile.TarInfo(name)
                if kind == 'link':
                    item.type = tarfile.SYMTYPE
                    item.linkname = '/tmp/outside'
                    tar.addfile(item)
                else:
                    item.size = 4
                    tar.addfile(item, io.BytesIO(b'data'))
        return archive

    def resolved_ui_fixture(self):
        dependency = self.root / 'dependency'
        files = {'crates/tekes-ui-controls/Cargo.toml': b'[package]\nname="tekes-ui-controls"\n',
                 'crates/tekes-ui-controls/src/lib.rs': b'// reviewed\n'}
        for name, value in files.items():
            path = dependency / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(value)
        reviewed = manifest(files)
        reviewed.update(kind='published-git-revision', rev='1' * 40,
                        repository='https://github.com/TekesApps/TekesUI.git')
        metadata = {'workspace_root': str(self.root / 'ui'), 'packages': [
            {'name': 'local-consumer', 'source': None,
             'manifest_path': str(self.root / 'ui/consumer/Cargo.toml')},
            {'name': 'tekes-ui-controls',
             'source': 'git+https://github.com/TekesApps/TekesUI.git?rev=' + '1' * 40 + '#' + '1' * 40,
             'manifest_path': str(dependency / 'crates/tekes-ui-controls/Cargo.toml')}]}
        return dependency, reviewed, metadata

    def test_resolved_ui_rejects_source_substitution_and_escaped_local_dependency(self):
        _, reviewed, metadata = self.resolved_ui_fixture()
        resolved_ui.check(self.root, metadata, reviewed)
        for source in (None, 'git+https://github.com/TekesApps/TekesUI.git?rev=main#' + '1' * 40):
            changed = json.loads(json.dumps(metadata))
            changed['packages'][1]['source'] = source
            with self.assertRaisesRegex(ValueError, 'source mismatch'):
                resolved_ui.check(self.root, changed, reviewed)
        metadata['packages'][0]['manifest_path'] = str(self.root.parent / 'outside/Cargo.toml')
        with self.assertRaisesRegex(ValueError, 'escaped'):
            resolved_ui.check(self.root, metadata, reviewed)

    def test_resolved_ui_rejects_changed_and_added_consumed_crate_inputs(self):
        dependency, reviewed, metadata = self.resolved_ui_fixture()
        path = dependency / 'crates/tekes-ui-controls/src/lib.rs'
        path.write_bytes(b'// changed\n')
        with self.assertRaisesRegex(ValueError, 'hash mismatch'):
            resolved_ui.check(self.root, metadata, reviewed)
        path.write_bytes(b'// reviewed\n')
        (path.parent / 'unreviewed.rs').write_text('// new input\n')
        with self.assertRaisesRegex(ValueError, 'unexpected files'):
            resolved_ui.check(self.root, metadata, reviewed)
        (path.parent / 'unreviewed.rs').unlink()
        (path.parent / 'unreviewed-link').symlink_to(self.root, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'unexpected symlink'):
            resolved_ui.check(self.root, metadata, reviewed)

    def test_archive_rejects_wrong_hash_before_creating_source(self):
        archive = self.archive([('handoff/file', 'file')])
        with self.assertRaisesRegex(ValueError, 'SHA256 mismatch'):
            extract_source(archive, '0' * 64, self.root / 'out')
        self.assertFalse((self.root / 'out').exists())

    def test_archive_rejects_traversal_link_and_duplicate_before_member_writes(self):
        for members in ([('../escape', 'file')], [('handoff/link', 'link')],
                        [('handoff/file', 'file'), ('handoff/file', 'file')]):
            with self.subTest(members=members):
                archive = self.archive(members)
                out = self.root / str(len(list(self.root.iterdir())))
                with self.assertRaises(ValueError):
                    extract_source(archive, sha256(archive), out)
                self.assertFalse(list(out.iterdir()))

    def test_manifest_tampering_and_symlink_cannot_pass(self):
        content = manifest({'a': b'good'})
        path = self.root / 'manifest.json'
        path.write_text(json.dumps(content))
        self.assertEqual(snapshot(path)['sha256'], content['sha256'])
        content['files']['../outside'] = '0' * 64
        path.write_text(json.dumps(content))
        with self.assertRaises(ValueError):
            snapshot(path)
        (self.root / 'a').write_bytes(b'bad')
        with self.assertRaisesRegex(ValueError, 'hash mismatch'):
            verify_files(self.root, manifest({'a': b'good'})['files'])
        (self.root / 'a').unlink()
        (self.root / 'other').write_bytes(b'good')
        (self.root / 'a').symlink_to(self.root / 'other')
        with self.assertRaisesRegex(ValueError, 'symlink'):
            verify_files(self.root, manifest({'a': b'good'})['files'])

    def test_payload_does_not_allow_unlisted_files(self):
        (self.root / 'payload-manifest.json').write_text(json.dumps({'a': '0' * 64}))
        (self.root / 'a').write_bytes(b'a')
        (self.root / 'unexpected').write_text('extra')
        with self.assertRaisesRegex(ValueError, 'unlisted'):
            verify_payload(self.root, '0' * 64)

    def test_resolved_dependency_escape_is_rejected(self):
        inside = {'manifest_path': str(self.root / 'Cargo.toml'), 'source': None}
        self.assertEqual(local_packages({'packages': [inside]}, self.root), 1)
        outside = {'manifest_path': str(self.root.parent / 'other/Cargo.toml'), 'source': None}
        with self.assertRaisesRegex(ValueError, 'escaped'):
            local_packages({'packages': [inside, outside]}, self.root)

    def test_archive_inventory_does_not_borrow_parent_git_ignore(self):
        subprocess.run(['git', 'init', '-q', str(self.root)], check=True)
        (self.root / '.gitignore').write_text('bundle/\n')
        bundle = self.root / 'bundle'
        bundle.mkdir()
        (bundle / 'unexpected.swift').write_text('invalid')
        (bundle / 'target').mkdir()
        (bundle / 'target/ignored.swift').write_text('generated')
        names, mode = source_files(bundle)
        self.assertEqual(mode, 'archive-walk')
        self.assertEqual(names, ['unexpected.swift'])

    def test_headless_guard_still_rejects_ui_and_sibling_dependencies(self):
        dep = {'name': 'tekes-ui-controls', 'kind': None, 'path': None}
        data = {'packages': [{'name': 'tekes-agent-session', 'dependencies': [dep]}]}
        with self.assertRaises(ValueError):
            headless_dependencies(data, self.root)
        dep.update(name='allowed', path=str(self.root.parent / 'sibling'))
        with self.assertRaisesRegex(ValueError, 'sibling'):
            headless_dependencies(data, self.root)

    def test_failed_command_records_exit_status_and_log(self):
        run = replay.Replay(self.root, dict(os.environ), 10)
        with self.assertRaisesRegex(ValueError, 'failed'):
            run.step('failure', [sys.executable, '-c', 'print("proof"); raise SystemExit(3)'], self.root)
        result = json.loads((self.root / 'receipt.json').read_text())['steps'][0]
        self.assertEqual((result['status'], result['exit_code']), ('failed', 3))
        self.assertIn('proof', (self.root / 'failure.log').read_text())

    def test_cli_hash_failure_keeps_failed_receipt_and_refuses_output_reuse(self):
        archive = self.archive([('handoff/a', 'file')])
        output = self.root / 'receipt'
        command = [sys.executable, str(SCRIPTS / 'verify-tekesui-handoff.py'), '--archive', str(archive),
                   '--sha256', '0' * 64, '--snapshot-sha256', '0' * 64, '--output', str(output),
                   '--target-dir', str(self.root / 'target')]
        self.assertNotEqual(subprocess.run(command, capture_output=True).returncode, 0)
        before = (output / 'receipt.json').read_bytes()
        self.assertEqual(json.loads(before)['status'], 'failed')
        self.assertNotEqual(subprocess.run(command, capture_output=True).returncode, 0)
        self.assertEqual((output / 'receipt.json').read_bytes(), before)

    def test_spawn_failure_is_recorded_instead_of_left_running(self):
        run = replay.Replay(self.root, dict(os.environ), 10)
        with self.assertRaises(OSError):
            run.step('absent', [self.root / 'no-such-executable'], self.root)
        result = json.loads((self.root / 'receipt.json').read_text())['steps'][0]
        self.assertEqual(result['status'], 'failed')

    def test_timeout_stops_its_process_and_preserves_partial_receipt(self):
        run = replay.Replay(self.root, dict(os.environ), 0.05)
        with self.assertRaises(subprocess.TimeoutExpired):
            run.step('timeout', [sys.executable, '-c', 'import time; time.sleep(30)'], self.root)
        result = json.loads((self.root / 'receipt.json').read_text())['steps'][0]
        self.assertEqual(result['status'], 'interrupted-or-timeout')

    def test_public_pin_boundary_rejects_mixed_path_and_wrong_revision(self):
        ui = self.root / 'ui'
        ui.mkdir()
        (self.root / 'Cargo.toml').write_text('[workspace]\nexclude=["ui"]\n')
        public = manifest({'Cargo.toml': b'reviewed'})
        public.update(kind='published-git-revision', baseCommit='1' * 40,
                      rev='1' * 40, repository='https://github.com/TekesApps/TekesUI.git')
        (ui / 'tekesui-source.json').write_text(json.dumps(public))
        for name, dependencies in {
            'components': ['tekes-agent-contract', 'tekes-agent-presentation', 'tekes-ui-controls', 'tekes-ui-scene', 'tekes-ui-tokens', 'tekes-ui-text', 'tekes-ui-motion', 'unicode-segmentation'],
            'bindings': ['tekes-agent-components-ui', 'tekes-agent-contract', 'tekes-agent-presentation', 'tekes-agent-session', 'tekes-agent-web', 'serde_json', 'tokio', 'async-trait', 'uuid', 'base64', 'image'],
            'consumer': ['tekes-agent-components-ui', 'tekes-agent-ui-bindings', 'tekes-ui-controls'],
        }.items():
            (ui / name).mkdir()
            text = '[package]\npublish.workspace=true\n[dependencies]\n'
            text += ''.join(f'{d}.workspace=true\n' for d in dependencies)
            if name == 'bindings':
                text += 'rusqlite={version="=0.40.2",default-features=false,features=["bundled"]}\n'
            if name == 'consumer':
                text += '[dev-dependencies]\n' + ''.join(
                    f'{d}.workspace=true\n' for d in ('tekes-ui-text', 'tekes-ui-scene', 'tekes-ui-tokens', 'serde_json'))
            (ui / name / 'Cargo.toml').write_text(text)
        header = '[workspace]\n[workspace.package]\npublish=false\nversion="0.0.0"\n[workspace.dependencies]\nunicode-segmentation="=1.13.3"\n'
        for dependency, valid in [
            ('{git="' + public['repository'] + '",rev="' + public['rev'] + '"}', True),
            ('{git="' + public['repository'] + '",rev="main"}', False),
            ('{path=".deps/tekesui/crates/tekes-ui-controls"}', False),
        ]:
            (ui / 'Cargo.toml').write_text(header + 'tekes-ui-controls=' + dependency + '\n')
            result = subprocess.run([sys.executable, str(SCRIPTS / 'check-ui-boundaries.py'),
                                     '--root', str(self.root)], capture_output=True)
            self.assertEqual(result.returncode == 0, valid, result.stderr.decode())
        # Controls are the original public reveal types; platform/harness access
        # must remain rejected even with an otherwise valid published SDK pin.
        (ui / 'Cargo.toml').write_text(header + 'tekes-ui-controls={git="' +
                                      public['repository'] + '",rev="' + public['rev'] + '"}\n')
        valid_workspace = (ui / 'Cargo.toml').read_text()
        for wrong_unicode in ['1', '=1.13.2']:
            with self.subTest(wrong_unicode=wrong_unicode):
                (ui / 'Cargo.toml').write_text(valid_workspace.replace('=1.13.3', wrong_unicode))
                result = subprocess.run([sys.executable, str(SCRIPTS / 'check-ui-boundaries.py'),
                                         '--root', str(self.root)], capture_output=True)
                self.assertNotEqual(result.returncode, 0)
        (ui / 'Cargo.toml').write_text(valid_workspace)
        consumer = ui / 'consumer/Cargo.toml'
        accepted = consumer.read_text()
        for forbidden in ['tekes-ui-platform-macos', 'tekes-agent-endpoint-kernel']:
            with self.subTest(forbidden=forbidden):
                consumer.write_text(accepted + forbidden + '.workspace=true\n')
                result = subprocess.run([sys.executable, str(SCRIPTS / 'check-ui-boundaries.py'),
                                         '--root', str(self.root)], capture_output=True)
                self.assertNotEqual(result.returncode, 0)
        consumer.write_text(accepted)
        bindings = ui / 'bindings/Cargo.toml'
        accepted = bindings.read_text()
        for old, wrong in [('=0.40.2', '0.40'), ('default-features=false', 'default-features=true'), ('features=["bundled"]', 'features=[]')]:
            with self.subTest(wrong_sqlite=wrong):
                bindings.write_text(accepted.replace(old, wrong))
                result = subprocess.run([sys.executable, str(SCRIPTS / 'check-ui-boundaries.py'),
                                         '--root', str(self.root)], capture_output=True)
                self.assertNotEqual(result.returncode, 0)
        bindings.write_text(accepted)

    def test_capture_is_disabled_for_a_published_pin(self):
        scripts = self.root / 'scripts'
        scripts.mkdir()
        entry = scripts / 'prepare-tekesui-demo.py'
        entry.write_bytes((SCRIPTS / entry.name).read_bytes())
        (self.root / 'ui').mkdir()
        lock = self.root / 'ui/tekesui-source.json'
        lock.write_text('{"kind":"published-git-revision"}')
        before = lock.read_bytes()
        result = subprocess.run([sys.executable, str(entry), '--capture', '--source', str(self.root)],
                                capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b'capture/path preparation is disabled', result.stderr)
        self.assertEqual(lock.read_bytes(), before)


class GitPin(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git('init', '-q')
        self.git('config', 'user.email', 'fixture@example.invalid')
        self.git('config', 'user.name', 'Local test fixture')
        self.file = self.root / 'crates/controls/src/lib.rs'
        self.file.parent.mkdir(parents=True)
        self.file.write_bytes(b'reviewed')
        self.reviewed = manifest({'crates/controls/src/lib.rs': b'reviewed'})
        self.commit = self.save()

    def git(self, *args):
        return subprocess.check_output(['git', '-C', str(self.root), *args]).decode().strip()

    def save(self):
        self.git('add', '.')
        self.git('-c', 'commit.gpgsign=false', 'commit', '-qm', 'isolated fixture')
        return self.git('rev-parse', 'HEAD')

    def test_pin_ignores_dirty_working_tree_and_reads_exact_committed_bytes(self):
        self.file.write_text('uncommitted different data')
        self.assertTrue(pin.git_tree_diff(self.root, self.commit, self.reviewed)['matches'])
        with self.assertRaises(ValueError):
            pin.git_tree_diff(self.root, 'main', self.reviewed)

    def test_changed_missing_and_new_consumed_inputs_require_review(self):
        self.file.write_bytes(b'changed')
        (self.file.parent / 'new.rs').write_bytes(b'new input')
        result = pin.git_tree_diff(self.root, self.save(), self.reviewed)
        self.assertFalse(result['matches'])
        self.assertEqual(len(result['changed']), 1)
        self.assertEqual(result['added_in_consumed_crates'], ['crates/controls/src/new.rs'])
        self.file.unlink()
        result = pin.git_tree_diff(self.root, self.save(), self.reviewed)
        self.assertEqual(result['missing'], ['crates/controls/src/lib.rs'])

    def test_git_symlink_is_not_an_accepted_source_blob(self):
        self.file.unlink()
        self.file.symlink_to('/tmp/outside')
        result = pin.git_tree_diff(self.root, self.save(), self.reviewed)
        self.assertFalse(result['matches'])
        self.assertEqual(result['unsupported'], ['crates/controls/src/lib.rs'])


if __name__ == '__main__':
    unittest.main()
