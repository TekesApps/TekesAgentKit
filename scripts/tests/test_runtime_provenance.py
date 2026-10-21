import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('runtime_provenance', Path(__file__).resolve().parents[1] / 'runtime_provenance.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class KernelProvenanceTests(unittest.TestCase):
    def programs(self, base, description):
        supervisor = base / 'supervisor'
        supervisor.write_text('#!/bin/sh\ncat <<\'EOF\'\n' + json.dumps(description) + '\nEOF\n')
        supervisor.chmod(0o755)
        worker = base / 'worker'
        worker.write_bytes(b'fixture-worker')
        return supervisor, worker

    def test_explicit_published_programs_are_hashed_without_copying_or_building(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            supervisor, worker = self.programs(base, {'format': 1, 'identifier': 'com.tekes.kernel.supervisor', 'version': '0.9.3', 'source_revision': 'fixed'})
            a, b, proof = module.kernel_programs(base, supervisor=supervisor, worker=worker, expected_version='0.9.3')
            self.assertEqual((a, b), (supervisor.resolve(), worker.resolve()))
            self.assertEqual(proof['workerSha256'], hashlib.sha256(b'fixture-worker').hexdigest())
            self.assertEqual(proof['build']['source_revision'], 'fixed')
            self.assertEqual(set(base.iterdir()), {supervisor, worker})

    def test_legacy_target_reports_actual_dev_version_and_exact_pin_rejects_it(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            debug = base / 'target/debug'
            debug.mkdir(parents=True)
            supervisor, worker = self.programs(debug, {'format': 1, 'identifier': 'com.tekes.kernel.supervisor', 'version': '0.9.0+web-auth.dev'})
            supervisor.rename(debug / 'tekes-supervisor')
            worker.rename(debug / 'tekes-worker')
            _, _, proof = module.kernel_programs(base)
            self.assertEqual(proof['build']['version'], '0.9.0+web-auth.dev')
            with self.assertRaisesRegex(ValueError, 'does not match'):
                module.kernel_programs(base, expected_version='0.9.3')

    def test_partial_or_ambiguous_program_selection_is_rejected_before_launch(self):
        with self.assertRaisesRegex(ValueError, 'both'):
            module.kernel_programs('.', supervisor='one')
        with self.assertRaisesRegex(ValueError, 'target directory'):
            module.kernel_programs('.', target='target', supervisor='one', worker='two')

    def test_foreign_description_cannot_be_recorded_as_kernel(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            supervisor, worker = self.programs(base, {'format': 1, 'identifier': 'foreign', 'version': '0.9.3'})
            with self.assertRaisesRegex(ValueError, 'unexpected'):
                module.kernel_programs(base, supervisor=supervisor, worker=worker)

    def test_nonobject_or_unversioned_description_is_rejected(self):
        for description in [[], {'format': 1, 'identifier': 'com.tekes.kernel.supervisor'}]:
            with self.subTest(description=description), tempfile.TemporaryDirectory() as directory:
                base = Path(directory)
                supervisor, worker = self.programs(base, description)
                with self.assertRaises(ValueError):
                    module.kernel_programs(base, supervisor=supervisor, worker=worker)
