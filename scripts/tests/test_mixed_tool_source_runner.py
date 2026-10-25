"""Acceptance provenance and failure boundaries; no harness or desktop."""
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
SPEC = importlib.util.spec_from_file_location('mixed_source_runner', SCRIPTS / 'test-real-mixed-tool-source.py')
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)


class MixedSourceRunnerTests(unittest.TestCase):
    def test_program_sha_cannot_borrow_another_sources_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            program, receipt = root / 'program', root / 'receipt.json'
            program.write_bytes(b'not executed')
            program = program.resolve(strict=True)
            value = {'sourceCommit': 'a' * 40, 'sourceManifestSha256': 'b' * 64,
                     'path': str(program), 'sha256': hashlib.sha256(program.read_bytes()).hexdigest()}
            receipt.write_text(json.dumps(value))
            source = {'head': 'a' * 40, 'manifestSha256': 'b' * 64}
            runner.verify_program(program, receipt, source)
            for field, wrong in (('sourceCommit', 'c' * 40), ('sourceManifestSha256', 'c' * 64),
                                 ('path', str(root / 'different')), ('sha256', '0' * 64)):
                changed = dict(value, **{field: wrong})
                receipt.write_text(json.dumps(changed))
                with self.subTest(field=field), self.assertRaises((ValueError, FileNotFoundError)):
                    runner.verify_program(program, receipt, source)

    def test_original_proof_cannot_omit_controls_or_claim_desktop(self):
        proof = dict.fromkeys(runner.PROOF_KEYS, True)
        proof.update(canonicalTools=3, nativeInteraction=False, physicalIme=False,
                     readingAnchorAcceptance=False)
        runner.validate_proof({'offscreen': True, 'mixedToolProof': proof})
        for field, wrong in (('originalControlsRetained', False), ('realReadResults', False),
                             ('canonicalTools', 2), ('physicalIme', True)):
            with self.subTest(field=field), self.assertRaises(ValueError):
                runner.validate_proof({'offscreen': True, 'mixedToolProof': dict(proof, **{field: wrong})})

    def test_cleanup_reads_only_original_group_and_fails_closed_on_ps_error(self):
        output = subprocess.CompletedProcess([], 0, ' 11 1 11\n 12 11 11\n 20 1 20\n', '')
        with patch.object(runner.subprocess, 'run', return_value=output):
            self.assertEqual([row['pid'] for row in runner.group_snapshot(11)], [11, 12])
            self.assertFalse(runner.group_exists(99))
        with patch.object(runner.subprocess, 'run', side_effect=subprocess.CalledProcessError(1, ['ps'])):
            with self.assertRaises(subprocess.CalledProcessError):
                runner.group_exists(11)


if __name__ == '__main__':
    unittest.main()
