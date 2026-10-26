import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('cargo_program', Path(__file__).resolve().parents[1] / 'cargo_program.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class CargoProgramTests(unittest.TestCase):
    def artifact(self, program, *, name='headless', kind='bin', test=False):
        return json.dumps({'reason':'compiler-artifact','target':{'name':name,'kind':[kind]},
                           'profile':{'test':test},'executable':str(program)})

    def test_actual_artifact_wins_over_repository_target_guess(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); actual=root/'shared-target/triple/profile/headless';actual.parent.mkdir(parents=True);actual.write_bytes(b'current')
            stale=root/'target/debug/headless';stale.parent.mkdir(parents=True);stale.write_bytes(b'old')
            output=self.artifact(stale,test=True)+'\n'+self.artifact(actual)
            with patch.object(module.subprocess,'run',return_value=SimpleNamespace(stdout=output,stderr='')) as build:
                program,receipt=module.cargo_program(root,'package','headless')
            self.assertEqual(program,actual.resolve());self.assertEqual(receipt['origin'],'cargo-compiler-artifact')
            self.assertIn('--message-format=json',build.call_args.args[0])
            self.assertEqual(receipt['bytes'],7)

    def test_explicit_frozen_program_does_not_build_or_consult_other_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            program=Path(directory)/'frozen';program.write_bytes(b'proof')
            with patch.object(module.subprocess,'run',side_effect=AssertionError('must not build')):
                selected,receipt=module.cargo_program('.', 'package','bin',existing=program)
            self.assertEqual(selected,program.resolve());self.assertEqual(receipt['origin'],'explicit-existing')

    def test_example_selection_rejects_foreign_target_and_ambiguous_programs(self):
        with tempfile.TemporaryDirectory() as directory:
            a=Path(directory)/'a';b=Path(directory)/'b';a.touch();b.touch()
            for output in [self.artifact(a),self.artifact(a,name='probe',kind='example')+'\n'+self.artifact(b,name='probe',kind='example')]:
                with self.subTest(output=output),patch.object(module.subprocess,'run',return_value=SimpleNamespace(stdout=output,stderr='')):
                    with self.assertRaisesRegex(ValueError,'exactly one'):
                        module.cargo_program('.', 'package','probe',kind='example')

    def test_failed_build_and_missing_program_never_fall_back(self):
        with patch.object(module.subprocess,'run',side_effect=module.subprocess.CalledProcessError(1,['cargo'])):
            with self.assertRaises(module.subprocess.CalledProcessError):module.cargo_program('.', 'p','n')
        with tempfile.TemporaryDirectory() as directory:
            with patch.object(module.subprocess,'run',side_effect=AssertionError('must not build')):
                with self.assertRaises(FileNotFoundError):module.cargo_program('.', 'p','n',existing=Path(directory)/'missing')
