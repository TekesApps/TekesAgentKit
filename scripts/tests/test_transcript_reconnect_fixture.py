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
from transcript_source_fixture import validate_transcript_pair, validate_transcript_source
from transcript_reconnect_fixture import validate_child_transcript_reconnect
from transcript_reconnect_fixture import validate_recovered_history
from test_summary_transition_fixture import transition
import test_summary_transition_fixture as transition_module

spec = importlib.util.spec_from_file_location('transcript_source_tests', Path(__file__).with_name('test_transcript_source_fixture.py'))
source_tests = importlib.util.module_from_spec(spec)
spec.loader.exec_module(source_tests)
runner_spec = importlib.util.spec_from_file_location('real_summary_source_transitions', Path(__file__).resolve().parents[1] / 'test-real-summary-source-transitions.py')
runner = importlib.util.module_from_spec(runner_spec)
runner_spec.loader.exec_module(runner)


class ReconnectFixtureTests(unittest.TestCase):
    def test_flag_requires_source_and_adds_reconnect_pointer_and_fresh_child_read(self):
        base = ['--headless-program','p','--pi-root','p','--dsh-root','d','--output','o']
        self.assertEqual(runner.parse_arguments(base).endpoint, 'all')
        self.assertEqual(runner.parse_arguments(base + ['--endpoint','dsh']).endpoint, 'dsh')
        with self.assertRaises(SystemExit):
            runner.parse_arguments(['--headless-program','p','--pi-root','p','--dsh-root','d','--output','o','--child-transcript-reconnect'])
        for endpoint in ('pi', 'dsh'):
            commands = runner.commands_for(endpoint, Path('/w'), 'failure', child_transcript_source=True,
                                           child_transcript_reconnect=True)
            tail = commands[-7:]
            self.assertEqual([c['op'] for c in tail], ['reconnect','open','presentationSource','open','presentationSource','presentationSource','childHistory'])
            self.assertEqual(tail[2]['saveAs'], 'freshChild')
            self.assertEqual(tail[2]['savePointer'], '/snapshot/summary/subagents/0/sessionId')
            self.assertEqual(tail[4]['sessionId'], 'parent')
            self.assertEqual(tail[5]['sessionId'], 'freshChild')
            self.assertEqual(tail[6]['childSessionId'], 'freshChild')

    def _case(self, endpoint):
        t = source_tests
        def replace(value, old, new):
            if isinstance(value, dict): return {(new if k == old else k):replace(v,old,new) for k,v in value.items()}
            if isinstance(value, list): return [replace(v,old,new) for v in value]
            return new if value == old else value
        old_running = replace(t.witness(endpoint, 'running', revision=5), t.CHILD, 'child-1')
        old_failed = replace(t.witness(endpoint, 'failed', revision=6), t.CHILD, 'child-1')
        with patch.object(transition_module, 'MARKER', t.MARKER):
            old_parent = transition(endpoint, 'failed')
        new_parent = copy.deepcopy(old_parent)
        for publication, epoch, revision in ((old_parent,4,5),(new_parent,7,8)):
            publication['snapshot']['connectionEpoch'] = epoch
            publication['authority']['connection_epoch'] = epoch
            publication['snapshot']['revision'] = revision
        fresh = copy.deepcopy(old_failed)
        fresh['snapshot']['connectionEpoch'] = 7
        fresh['authority']['connection_epoch'] = 7
        fresh['snapshot']['revision'] = 20
        return old_running, old_failed, old_parent, new_parent, fresh, t

    def test_accepts_fresh_epoch_same_pointer_and_same_failed_transcript(self):
        old_run, old_fail, parent_old, parent_new, fresh, t = self._case('pi')
        receipt = validate_child_transcript_reconnect(endpoint='pi', marker=t.MARKER,
            old_running=old_run, old_failed=old_fail, old_parent=parent_old, reconnected_parent=parent_new,
            fresh_child=fresh, fresh_pointer='child-1')
        self.assertTrue(receipt['sameFailedTranscript'])
        self.assertEqual(receipt['newConnectionEpoch'], receipt['newParentConnectionEpoch'])
        self.assertGreater(receipt['newConnectionEpoch'], receipt['oldConnectionEpoch'])
        self.assertEqual(receipt['freshPointer'], 'child-1')
        history = fresh['snapshot']['transcript']
        self.assertTrue(validate_recovered_history(history, fresh, receipt['turnId'])['sameSourceContent'])

    def test_parent_bound_history_follows_its_own_opaque_turn_identity(self):
        for endpoint in ('pi', 'dsh'):
            _, _, _, _, fresh, _ = self._case(endpoint)
            history = copy.deepcopy(fresh['snapshot']['transcript'])
            for item in history['items']:
                item['turnId'] = 'parent thread opaque turn'
            history['turns'][0]['id'] = 'parent thread opaque turn'
            receipt = validate_recovered_history(history, fresh, source_tests.TURN)
            self.assertTrue(receipt['sameSourceContent'])
            self.assertFalse(receipt['samePublication'])
            self.assertNotEqual(receipt['historyOpaqueTurnId'], receipt['primaryRunOpaqueTurnId'])

    def test_only_live_delivery_marker_may_disappear_after_reopen(self):
        old_run, old_fail, parent_old, parent_new, fresh, t = self._case('pi')
        old_fail['snapshot']['transcript']['items'][-1]['metadata']['nativeStreamId'] = 'live-only-stream'
        receipt = validate_child_transcript_reconnect(endpoint='pi', marker=t.MARKER,
            old_running=old_run, old_failed=old_fail, old_parent=parent_old,
            reconnected_parent=parent_new, fresh_child=fresh, fresh_pointer='child-1')
        self.assertTrue(receipt['sameFailedTranscript'])
        fresh['snapshot']['transcript']['items'][-1]['metadata']['unrelated'] = 'changed'
        with self.assertRaises(ValueError):
            validate_child_transcript_reconnect(endpoint='pi', marker=t.MARKER,
                old_running=old_run, old_failed=old_fail, old_parent=parent_old,
                reconnected_parent=parent_new, fresh_child=fresh, fresh_pointer='child-1')

    def test_rejects_old_epoch_old_alias_wrong_pointer_changed_source_and_nonfailed(self):
        t = source_tests
        for mutation in ('old_epoch','wrong_pointer','changed_user','not_failed','changed_error','changed_fork',
                         'parent_child_epoch_mismatch','changed_parent_source'):
            old_run, old_fail, parent_old, parent_new, fresh, _ = self._case('pi')
            fresh['snapshot']['connectionEpoch'] = 7; fresh['authority']['connection_epoch'] = 7
            if mutation == 'old_epoch': fresh['snapshot']['connectionEpoch'] = fresh['authority']['connection_epoch'] = 4
            if mutation == 'parent_child_epoch_mismatch': fresh['snapshot']['connectionEpoch'] = fresh['authority']['connection_epoch'] = 8
            if mutation == 'changed_user': fresh['snapshot']['transcript']['items'][0]['sourcePosition']['entryId'] = 'other'
            if mutation == 'not_failed': fresh['snapshot']['transcript']['turns'][0]['status'] = 'running'
            if mutation == 'changed_error': fresh['snapshot']['transcript']['items'][-1]['metadata']['errorMessage'] = 'different'
            if mutation == 'changed_fork': fresh['snapshot']['transcript']['turns'][0]['forkBoundary'] = {'entryId':'other'}
            if mutation == 'changed_parent_source':
                parent_new['snapshot']['transcript']['tools'][0]['resultPosition']['sequence'] = 99
            pointer = 'another-child' if mutation == 'wrong_pointer' else 'child-1'
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                validate_child_transcript_reconnect(endpoint='pi', marker=t.MARKER,
                    old_running=old_run, old_failed=old_fail, old_parent=parent_old, reconnected_parent=parent_new,
                    fresh_child=fresh, fresh_pointer=pointer)

    def test_recovered_child_history_must_match_fresh_failed_transcript(self):
        _, _, _, _, fresh, _ = self._case('pi')
        history = copy.deepcopy(fresh['snapshot']['transcript'])
        self.assertTrue(validate_recovered_history(history, fresh, source_tests.TURN)['sameSourceContent'])
        history['turns'][0]['forkBoundary'] = {'entryId':'different'}
        with self.assertRaises(ValueError):
            validate_recovered_history(history, fresh, source_tests.TURN)

    def test_driver_replays_primary_child_recovery_sequence_with_old_public_frames(self):
        _, _, old_parent, new_parent, fresh_child, t = self._case('pi')
        with patch.object(transition_module, 'MARKER', t.MARKER):
            running_parent = transition('pi', 'running')
        for frame, epoch, revision in ((running_parent,4,4),(old_parent,4,6)):
            frame['snapshot']['connectionEpoch'] = frame['authority']['connection_epoch'] = epoch
            frame['snapshot']['revision'] = revision
        marker = t.MARKER
        commands = runner.commands_for('pi', Path('/tmp/owned'), marker,
            child_transcript_source=True, child_transcript_reconnect=True)
        parent_frames = iter((running_parent, running_parent, old_parent, new_parent, new_parent))
        child_frames = iter((t.witness('pi','running',revision=5), t.witness('pi','failed',revision=6), fresh_child))
        def replace(value, old, new):
            if isinstance(value, dict): return {(new if k == old else k):replace(v,old,new) for k,v in value.items()}
            if isinstance(value, list): return [replace(v,old,new) for v in value]
            return new if value == old else value
        child_frames = iter(tuple(replace(frame, t.CHILD, 'child-1') for frame in child_frames))
        lines = [json.dumps({'kind':'pi','ready':True,'version':'1.3.0'})]
        for command in commands:
            if command['op'] == 'presentationSource':
                value = next(child_frames) if command.get('sessionId') in ('child','freshChild') else next(parent_frames)
                result = value
            elif command['op'] == 'childHistory' and command.get('childSessionId') == 'freshChild':
                result = fresh_child['snapshot']['transcript']
            elif command['op'] == 'childRead':
                result = {'pendingInputs':0,'running':False,'settled':True}
            else:
                result = {}
            lines.append(json.dumps({'op':command['op'],'result':result}))

        class Input:
            def write(self, _): pass
            def close(self): pass
        class Process:
            pid = 70001
            stdin = Input()
            stdout = io.StringIO('\n'.join(lines)+'\n')
            def wait(self, timeout=None): return 0
            def poll(self): return 0
        class Gate:
            def __init__(self): self.marker = marker
            entered = threading.Event()
            def release_after_capture(self): self.released = True
            def abort(self): pass
            def receipt(self): return {'requestCount':1,'explicitlyReleased':True,'aborted':False,'timedOut':False}
        gate = Gate(); gate.entered.set(); gate.released = False
        with tempfile.TemporaryDirectory(prefix='agentkit-child-reconnect-test-') as tmp:
            output = Path(tmp); (output/'commands.json').unlink(missing_ok=True)
            with patch.object(runner.subprocess, 'Popen', return_value=Process()):
                result = runner.run_profile('pi', {'expectedVersion':'1.3.0'}, commands, Path('/bin/true'),
                    Path('/tmp/owned'), output, {}, gate, child_transcript_source=True, child_transcript_reconnect=True)
            self.assertTrue(result['childTranscriptReconnect']['receipt']['sameFailedTranscript'])
            self.assertTrue((output/'witness-4.json').is_file())
            self.assertTrue((output/'transcript-reconnected.json').is_file())
            self.assertTrue((output/'recovered-child-history-receipt.json').is_file())


if __name__ == '__main__':
    unittest.main()
