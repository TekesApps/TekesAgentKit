import copy
import http.client
import http.server
import json
from pathlib import Path
import sys
import threading
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from native_child_pane_source_fixture import (MARKER, is_native_child_request,
    native_child_provider, validate_native_frame, validate_native_receipt)
from summary_transition_provider import HeldFailureGate

PARENT = {'endpointId': 'native', 'threadId': 'parent', 'sessionId': 'parent'}


def frame(phase='running'):
    status = 'running' if phase == 'running' else 'error'
    turn_id = json.dumps(['native', 'child', 'task-source', 'call'], separators=(',', ':'))
    entries = [{'event': {'seq': 0, 'type': 'turn/start', 'data': {'turn': 1}}}]
    if phase == 'failed':
        entries.extend([{'event': {'seq': 1, 'type': 'response/error', 'data': {'turn': 1}}},
                        {'event': {'seq': 2, 'type': 'turn/end', 'data': {'reason': 'error', 'turn': 1}}}])
    call = {'callId': 'call', 'name': 'task', 'arguments': {'instructions': MARKER}, 'phase': status}
    snapshot = {'scope': dict(PARENT, sessionId='child'), 'publication': 2 if phase == 'running' else 3,
        'connectionEpoch': 2, 'connection': {'phase': 'connected', 'historyIsStale': False}, 'transcript': {
            'turns': [{'id': turn_id, 'status': status},
                      {'id': json.dumps(['native', 'child', 'turn:1']), 'status': status}],
            'tools': [{'id': turn_id + ':call', 'turnId': turn_id, 'name': 'task',
                       'arguments': call['arguments'], 'phase': status}],
            'items': [] if phase == 'running' else [{'turnId': json.dumps(['native', 'child', 'turn:1']), 'kind': 'error'}],
            'isStale': False, 'hasMoreBefore': False}}
    return {'phase': phase, 'parent': PARENT, 'childId': 'child', 'marker': MARKER,
        'mountedSnapshot': copy.deepcopy(snapshot), 'witness': {'snapshot': snapshot, 'source': {
            'parentScope': PARENT, 'childId': 'child', 'pageSize': 50,
            'headRequest': {'parentSessionId': 'parent', 'childSessionId': 'child', 'maxMessages': 50},
            'rawHead': {'parentSessionId': 'parent', 'childSessionId': 'child', 'sourceCall': call,
                'entries': entries, 'throughSequence': len(entries) - 1,
                'historyRevision': phase, 'hasMoreBefore': False, 'runOpen': phase == 'running'},
            'evidenceScope': 'built-in-bounded-head-only-same-local-read',
            'serverTransactionalFreshnessProven': False}}}


class NativePaneFixtureTests(unittest.TestCase):
    def validate(self, value, phase='running', previous=None):
        return validate_native_frame(value, parent=PARENT, child_id='child',
                                     expected_phase=phase, previous=previous)

    def test_running_failed_pair_and_receipt(self):
        running = self.validate(frame())
        failed = self.validate(frame('failed'), 'failed', running)
        receipt = dict(accepted=True, parent=PARENT, childId='child', childScope=running['scope'],
            connectionEpoch=2, runningPublication=2, failedPublication=3)
        for key in ('readOnly', 'headOnly', 'openedViaOriginalControl', 'closedViaOriginalControl',
                    'parentDraftPreserved', 'parentSelectionPreserved', 'retiredWitnessRejected'):
            receipt[key] = True
        for key in ('sentViaOriginalComposer', 'nativeInteraction', 'physicalIme',
                    'serverTransactionalFreshnessProven'):
            receipt[key] = False
        self.assertTrue(validate_native_receipt(receipt, running, failed)['accepted'])
        for key in ('readOnly', 'parentDraftPreserved', 'sentViaOriginalComposer', 'physicalIme'):
            bad = dict(receipt, **{key: not receipt[key]})
            with self.subTest(key=key), self.assertRaises(ValueError):
                validate_native_receipt(bad, running, failed)

    def test_reject_mounted_and_original_source_mismatch(self):
        for field, replacement in [('publication', 1), ('scope', PARENT), ('transcript', {})]:
            bad = frame()
            bad['mountedSnapshot'][field] = replacement
            with self.subTest(field=field), self.assertRaises(ValueError):
                self.validate(bad)
        for field, replacement in [('serverTransactionalFreshnessProven', True), ('childId', 'parent')]:
            bad = frame()
            bad['witness']['source'][field] = replacement
            with self.subTest(field=field), self.assertRaises(ValueError):
                self.validate(bad)

    def test_reject_non_head_and_wrong_task_projection(self):
        for mutation in ('before', 'cached', 'status', 'arguments', 'open'):
            bad = frame()
            if mutation == 'before':
                bad['witness']['source']['headRequest']['beforeSequence'] = 0
            elif mutation == 'cached':
                bad['witness']['snapshot']['transcript']['hasMoreBefore'] = True
            elif mutation == 'status':
                bad['witness']['snapshot']['transcript']['turns'][0]['status'] = 'completed'
            elif mutation == 'arguments':
                bad['witness']['snapshot']['transcript']['tools'][0]['arguments'] = {}
            else:
                bad['witness']['source']['rawHead']['runOpen'] = False
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                self.validate(bad)

    def test_reject_cross_epoch_same_publication_and_changed_prefix(self):
        previous = self.validate(frame())
        for mutation in ('epoch', 'publication', 'prefix', 'call'):
            bad = frame('failed')
            if mutation == 'epoch':
                bad['witness']['snapshot']['connectionEpoch'] = 3
            elif mutation == 'publication':
                bad['witness']['snapshot']['publication'] = 2
            elif mutation == 'prefix':
                bad['witness']['source']['rawHead']['entries'][0]['event']['type'] = 'changed'
            else:
                bad['witness']['source']['rawHead']['sourceCall']['arguments'] = {'instructions': MARKER + ' changed'}
                bad['witness']['snapshot']['transcript']['tools'][0]['arguments'] = {'instructions': MARKER + ' changed'}
            bad['mountedSnapshot'] = copy.deepcopy(bad['witness']['snapshot'])
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                self.validate(bad, 'failed', previous)

    def test_reject_unrelated_terminal_and_projected_error(self):
        previous = self.validate(frame())
        for mutation in ('terminal', 'response', 'projected', 'turn-status'):
            bad = frame('failed')
            if mutation == 'terminal':
                bad['witness']['source']['rawHead']['entries'][-1]['event']['data']['turn'] = 2
            elif mutation == 'response':
                bad['witness']['source']['rawHead']['entries'][1]['event']['data']['turn'] = 2
            elif mutation == 'projected':
                bad['witness']['snapshot']['transcript']['items'][0]['turnId'] = json.dumps(['native', 'child', 'turn:2'])
            else:
                bad['witness']['snapshot']['transcript']['turns'][1]['status'] = 'completed'
            bad['mountedSnapshot'] = copy.deepcopy(bad['witness']['snapshot'])
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                self.validate(bad, 'failed', previous)

    def test_parent_tool_argument_is_not_child_user_input(self):
        self.assertFalse(is_native_child_request({'input': [
            {'role': 'user', 'content': 'ENDPOINT_NATIVE_CHILD'},
            {'type': 'function_call', 'arguments': json.dumps({'instructions': MARKER})}]}))
        self.assertFalse(is_native_child_request({'input': [
            {'role': 'assistant', 'content': MARKER}]}))
        self.assertTrue(is_native_child_request({'input': [
            {'role': 'user', 'content': [{'type': 'input_text', 'text': MARKER}]}]}))
        for bad in (None, {}, {'input': MARKER}, {'messages': [{'role': 'user', 'content': MARKER}]}):
            self.assertFalse(is_native_child_request(bad))

    def test_real_http_routes_parent_normally_and_holds_only_child(self):
        gate = HeldFailureGate(MARKER, 3)
        paths = []
        handler = native_child_provider(workspace=lambda: Path('/tmp'), requests=paths, gate=gate)
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        received = []
        def post(body):
            conn = http.client.HTTPConnection('127.0.0.1', server.server_port, timeout=4)
            conn.request('POST', '/v1/responses', json.dumps(body), {'Content-Type': 'application/json'})
            response = conn.getresponse()
            result = (response.status, response.read())
            conn.close()
            return result
        child = None
        try:
            status, _ = post({'input': [{'role': 'user', 'content': 'ENDPOINT_NATIVE_CHILD'},
                {'type': 'function_call', 'arguments': MARKER}]})
            self.assertEqual(status, 200)
            self.assertFalse(gate.entered.is_set())
            child = threading.Thread(target=lambda: received.append(post(
                {'input': [{'role': 'user', 'content': MARKER}]})))
            child.start()
            self.assertTrue(gate.entered.wait(1))
            self.assertFalse(received)
            gate.release_after_capture()
            child.join(3)
            self.assertFalse(child.is_alive())
            self.assertEqual(received[0][0], 400)
            self.assertIn(MARKER.encode(), received[0][1])
            self.assertEqual(len(paths), 2)
            self.assertEqual(gate.receipt(), {'requestCount': 1, 'entered': True,
                'explicitlyReleased': True, 'aborted': False, 'timedOut': False})
        finally:
            gate.abort()
            if child is not None:
                child.join(4)
            server.shutdown()
            server.server_close()
            thread.join(2)


if __name__ == '__main__':
    unittest.main()
