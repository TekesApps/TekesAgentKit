import copy
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parent))
from child_pane_source_fixture import validate_pane_frame, validate_pane_receipt
import test_transcript_source_fixture as original

spec = importlib.util.spec_from_file_location('real_child_pane',
    Path(__file__).resolve().parents[1] / 'test-real-child-pane-source.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class ChildPaneSourceTests(unittest.TestCase):
    def frame(self, endpoint, phase):
        value = original.witness(endpoint, phase, revision=5 if phase == 'running' else 6)
        scope = {'endpointId': endpoint, 'threadId': 'root-thread', 'sessionId': original.CHILD}
        value['snapshot']['summary']['scope'] = scope
        value['snapshot']['run']['scope'] = scope
        value['snapshot']['selection']['destination']['scope'] = scope
        return {'phase': phase, 'parent': {'endpointId': endpoint, 'threadId': 'root-thread',
                                         'sessionId': original.PARENT},
                'childId': original.CHILD, 'marker': original.MARKER, 'witness': value}

    def proof(self, frame, **kwargs):
        return validate_pane_frame(frame, endpoint=frame['parent']['endpointId'],
            parent={'endpointId': frame['parent']['endpointId'], 'threadId': 'root-thread',
                    'sessionId': original.PARENT}, child_id=original.CHILD,
            marker=original.MARKER, expected_phase=kwargs.get('phase', frame['phase']),
            expected_epoch=kwargs.get('epoch'))

    def receipt(self, running, failed):
        return dict(accepted=True, evidenceScope='actual-child-pane-original-controls-and-source',
            parent=running['directParent'], childId=original.CHILD, childScope=running['scope'],
            connectionEpoch=running['connectionEpoch'], runningRevision=running['revision'],
            failedRevision=failed['revision'], nativeInteraction=False, physicalIme=False,
            **{key: True for key in ('openedViaOriginalControl', 'sentViaOriginalComposer',
                'closedViaOriginalControl', 'parentDraftPreserved', 'parentSelectionPreserved',
                'retiredWitnessRejected')})

    def test_actual_parent_thread_scope_and_direct_parent_are_preserved(self):
        for endpoint in ('pi', 'dsh'):
            running = self.proof(self.frame(endpoint, 'running'))
            failed = self.proof(self.frame(endpoint, 'failed'), epoch=running['connectionEpoch'])
            result = validate_pane_receipt(self.receipt(running, failed), running, failed)
            self.assertTrue(result['accepted'])
            self.assertEqual(result['childScope']['threadId'], 'root-thread')
            self.assertNotEqual(result['parent']['sessionId'], result['childScope']['threadId'])

    def test_primary_child_scope_cannot_substitute_for_actual_pane(self):
        for endpoint in ('pi', 'dsh'):
            frame = self.frame(endpoint, 'running')
            primary = {'endpointId': endpoint, 'threadId': original.CHILD, 'sessionId': original.CHILD}
            frame['witness']['snapshot']['run']['scope'] = primary
            with self.assertRaises(ValueError): self.proof(frame)

    def test_rejects_phase_pointer_marker_parent_and_epoch_changes(self):
        for field, wrong in (('phase', 'failed'), ('childId', 'foreign'), ('marker', 'old-input'),
                             ('parent', {'endpointId': 'pi', 'threadId': 'root-thread', 'sessionId': 'foreign'})):
            frame = self.frame('pi', 'running'); frame[field] = wrong
            with self.subTest(field=field), self.assertRaises(ValueError): self.proof(frame, phase='running')
        with self.assertRaises(ValueError): self.proof(self.frame('pi', 'running'), epoch=5)

    def test_source_checks_and_dispatch_retirement_are_not_weakened(self):
        for endpoint in ('pi', 'dsh'):
            frame = self.frame(endpoint, 'failed')
            frame['witness']['snapshot']['transcript']['items'][0]['recordId'] = 'foreign-record'
            with self.assertRaises(ValueError): self.proof(frame)
            frame = self.frame(endpoint, 'failed')
            frame['witness']['authority']['sessions'][original.CHILD]['queue'] = [{'placement': 'dispatching'}]
            with self.assertRaises(ValueError): self.proof(frame)

    def test_host_flags_and_pair_cannot_substitute_for_each_other(self):
        running = self.proof(self.frame('pi', 'running')); failed = self.proof(self.frame('pi', 'failed'))
        for key in ('retiredWitnessRejected', 'sentViaOriginalComposer', 'parentDraftPreserved',
                    'closedViaOriginalControl', 'parentSelectionPreserved', 'openedViaOriginalControl'):
            receipt = self.receipt(running, failed); receipt[key] = False
            with self.subTest(key=key), self.assertRaises(ValueError): validate_pane_receipt(receipt, running, failed)
        wrong = copy.deepcopy(failed); wrong['turnId'] = 'another-opaque-turn'
        with self.assertRaises(ValueError): validate_pane_receipt(self.receipt(running, failed), running, wrong)

    def test_original_frame_driver_releases_only_after_validation_and_pairs_receipt(self):
        for bad_source in (False, True):
            running_frame = self.frame('pi', 'running'); failed_frame = self.frame('pi', 'failed')
            running = self.proof(running_frame); failed = self.proof(failed_frame)
            if bad_source:
                running_frame['witness']['projectionSucceeded'] = False
            text = 'ordinary owned log\n' + '\n'.join('AGENTKIT_CHILD_PANE_SOURCE ' + json.dumps(frame)
                    for frame in (running_frame, failed_frame)) + '\n'

            class Process:
                pid = 70002
                stdout = io.StringIO(text)
                def poll(self): return 0
                def wait(self, timeout=None): return 0

            class Gate:
                marker = original.MARKER
                def __init__(self):
                    self.entered = threading.Event(); self.entered.set(); self.releases = 0
                def release_after_capture(self): self.releases += 1
                def abort(self): pass
                def receipt(self):
                    return dict(explicitlyReleased=self.releases == 1, timedOut=False, aborted=False)

            gate = Gate()
            with tempfile.TemporaryDirectory(prefix='agentkit-pane-source-test-') as name:
                case = Path(name)
                (case / 'host-receipt.json').write_text(json.dumps(self.receipt(running, failed)))
                with patch.object(runner.subprocess, 'Popen', return_value=Process()):
                    if bad_source:
                        with self.assertRaises(ValueError):
                            runner.capture_demo('pi', Path('/bin/true'), ['/bin/true'], {}, case,
                                case, gate, running_frame['parent'], original.CHILD)
                        self.assertEqual(gate.releases, 0)
                    else:
                        result = runner.capture_demo('pi', Path('/bin/true'), ['/bin/true'], {}, case,
                            case, gate, running_frame['parent'], original.CHILD)
                        self.assertTrue(result['accepted'])
                        self.assertEqual(gate.releases, 1)
                        self.assertTrue((case / 'source-running.json').is_file())
                        self.assertTrue((case / 'source-failed.json').is_file())


if __name__ == '__main__': unittest.main()
