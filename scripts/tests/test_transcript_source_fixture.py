import copy
import importlib.util
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from transcript_source_fixture import validate_transcript_pair, validate_transcript_source

_RUNNER_SPEC = importlib.util.spec_from_file_location(
    'real_summary_source_transitions', Path(__file__).resolve().parents[1] / 'test-real-summary-source-transitions.py')
_RUNNER = importlib.util.module_from_spec(_RUNNER_SPEC)
_RUNNER_SPEC.loader.exec_module(_RUNNER)


MARKER = 'TRANSCRIPT_CHILD_SOURCE_MARKER'
CHILD = 'child-session-public-id'
PARENT = 'parent-session-public-id'
TURN = 'opaque transcript turn; no source ID encoding'
ERROR = 'controlled public error for TRANSCRIPT_CHILD_SOURCE_MARKER'


def raw_child(endpoint, phase):
    if endpoint == 'pi':
        entries = [
            {'id': 'run-start', 'type': 'custom', 'customType': 'agentkit.run.v1',
             'data': {'id': 'native-run-1', 'phase': 'started'}},
            {'id': 'user-entry', 'type': 'message',
             'message': {'role': 'user', 'content': [{'type': 'text', 'text': MARKER}]}},
        ]
        if phase == 'failed':
            entries += [
                {'id': 'error-entry', 'type': 'message',
                 'message': {'role': 'assistant', 'stopReason': 'error', 'errorMessage': ERROR}},
                {'id': 'run-end', 'type': 'custom', 'customType': 'agentkit.run.v1',
                 'data': {'id': 'native-run-1', 'phase': 'settled', 'aborted': False, 'timestamp': 102.0}},
            ]
        return {
            'summary': {'sessionId': CHILD, 'running': phase == 'running'},
            'journal_ready': True, 'journal_error': None, 'run_open': phase == 'running',
            'queue': [], 'native_conversation': {'sessionId': CHILD, 'generation': 'generation-1',
                'isStreaming': phase == 'running', 'entries': entries},
        }
    events = {
        '1': {'event': {'seq': 1, 'type': 'turn/start', 'time': 100.0, 'data': {'turn': 9}}},
        '2': {'event': {'seq': 2, 'type': 'user/message', 'time': 101.0,
                        'data': {'id': 'journal-user-id', 'role': 'user',
                                 'content': [{'type': 'text', 'text': MARKER},
                                 ], 'source': {'kind': 'user', 'rpcId': 'rpc-input'}}}},
    }
    if phase == 'failed':
        events['3'] = {'event': {'seq': 3, 'type': 'turn/end', 'time': 102.0,
                                 'data': {'turn': 9, 'reason': {'kind': 'error',
                                     'error': {'message': ERROR, 'code': 'INVALID_REQUEST'}}}}}
    return {'summary': {'sessionId': CHILD, 'running': phase == 'running'},
            'journal_ready': True, 'journal_error': None, 'run_open': phase == 'running',
            'queue': [], 'entries': events}


def transcript(endpoint, phase):
    user_position = ({'kind': 'native', 'entryId': 'user-entry', 'entryOrdinal': 1}
                     if endpoint == 'pi' else {'kind': 'journal', 'sequence': 2})
    user_id = 'transcript-user-item'
    user = {'id': user_id, 'kind': 'user', 'text': MARKER, 'recordId':
            'user-entry' if endpoint == 'pi' else 'journal-user-id',
            'sourcePosition': user_position, 'sourceSequence': None if endpoint == 'pi' else 2,
            'turnId': TURN, 'isVisible': True, 'isProvisional': False,
            'metadata': {'inputPresentation': {'itemId': user_id, 'turnId': TURN, 'role': 'opening'}}}
    if endpoint == 'dsh':
        user['metadata']['source'] = {'kind': 'user', 'rpcId': 'rpc-input'}
    start, end = (100.0, 102.0) if endpoint == 'dsh' else (100.0, 102.0)
    turn = {'id': TURN, 'input': MARKER,
            'startedAt': start, 'endedAt': None if phase == 'running' else end,
            'status': 'running' if phase == 'running' else 'error',
            'forkBoundary': None if phase == 'running' else ({'entryId': 'run-end'} if endpoint == 'pi' else {'atSeq': 3})}
    items = [user]
    if phase == 'failed':
        if endpoint == 'pi':
            err_position = {'kind': 'native', 'entryId': 'error-entry', 'entryOrdinal': 2}
            error_item = {'id': 'native-error-item', 'kind': 'assistant', 'text': '',
                          'recordId': 'error-entry', 'sourcePosition': err_position,
                          'turnId': TURN, 'isVisible': True, 'isProvisional': False,
                          'metadata': {'stopReason': 'error', 'errorMessage': ERROR}}
        else:
            error_item = {'id': 'journal-error-item', 'kind': 'error', 'text': ERROR,
                          'sourcePosition': {'kind': 'journal', 'sequence': 3}, 'sourceSequence': 3,
                          'turnId': TURN, 'isVisible': True, 'isProvisional': False,
                          'time': 102.0}
        items.append(error_item)
    return {'isStale': False, 'items': items, 'turns': [turn]}


def witness(endpoint, phase, *, revision=5):
    scope = {'endpointId': endpoint, 'threadId': CHILD, 'sessionId': CHILD}
    return {'projectionSucceeded': True, 'evidenceScope': 'original-publication-and-same-memory-input',
            'serverTransactionalFreshnessProven': False,
            'authority': {'endpoint_id': endpoint, 'connection': 'Ready', 'connection_epoch': 4,
                          'inventory_ready': True, 'control_ready': True, 'actionables_ready': True,
                          'actionables': {}, 'sessions': {CHILD: raw_child(endpoint, phase)}},
            'snapshot': {'connectionEpoch': 4, 'revision': revision,
                         'summary': {'scope': scope},
                         'connection': {'phase': 'connected', 'canSubmit': True},
                         'run': {'scope': scope},
                         'selection': {'destination': {'kind': 'run', 'scope': scope}},
                         'transcript': transcript(endpoint, phase)}}


def valid(endpoint, phase, **kwargs):
    return validate_transcript_source(witness(endpoint, phase, revision=5 if phase == 'running' else 6), endpoint=endpoint,
                                      child_id=CHILD, phase=phase, marker=MARKER,
                                      expected_epoch=4,
                                      expected_scope={'endpointId': endpoint, 'threadId': CHILD,
                                                      'sessionId': CHILD},
                                      **kwargs)


class TranscriptSourceFixtureTests(unittest.TestCase):
    def test_opt_in_preserves_default_command_lists_and_adds_two_child_frames(self):
        workspace = Path('/fixture/workspace')
        for endpoint, expected in (
            ('pi', ['create', 'call', 'open', 'compose', 'waitSettled', 'presentation', 'open',
                    'waitSettled', 'open', 'presentationSource', 'open', 'childPrompt',
                    'waitSourceRunOpen', 'presentationSource', 'waitSettled', 'childRead',
                    'presentationSource', 'childHistory']),
            ('dsh', ['create', 'call', 'open', 'compose', 'waitSettled', 'childCatalog', 'open',
                    'childPrompt', 'waitSettled', 'reconnect', 'open', 'presentation', 'open',
                    'presentationSource', 'open', 'childPrompt', 'waitSourceRunOpen',
                    'presentationSource', 'waitSettled', 'presentationSource', 'childHistory']),
        ):
            original = _RUNNER.commands_for(endpoint, workspace, MARKER)
            self.assertEqual([row['op'] for row in original], expected)
            opt_in = _RUNNER.commands_for(endpoint, workspace, MARKER, child_transcript_source=True)
            self.assertEqual(len(opt_in), len(original) + 2)
            source_rows = [row for row in opt_in if row['op'] == 'presentationSource']
            self.assertEqual([row['sessionId'] for row in source_rows].count('child'), 2)

    def test_four_endpoint_phase_positive_cases(self):
        for endpoint in ('pi', 'dsh'):
            for phase in ('running', 'failed'):
                with self.subTest(endpoint=endpoint, phase=phase):
                    proof = valid(endpoint, phase)
                    self.assertEqual(proof['turnStatus'], phase if phase == 'running' else 'error')
                    self.assertEqual(proof['childSessionId'], CHILD)

    def test_rejects_wrong_scope_parent_projection_and_failed_projection(self):
        for mutate in (
            lambda w: w['snapshot']['summary']['scope'].__setitem__('sessionId', PARENT),
            lambda w: w['snapshot']['selection']['destination'].__setitem__('scope',
                {'endpointId': 'pi', 'threadId': PARENT, 'sessionId': PARENT}),
            lambda w: w.__setitem__('projectionSucceeded', False),
            lambda w: w.__setitem__('serverTransactionalFreshnessProven', True),
        ):
            w = witness('pi', 'running'); mutate(w)
            with self.assertRaises(ValueError):
                validate_transcript_source(w, endpoint='pi', child_id=CHILD, phase='running',
                    marker=MARKER, expected_epoch=4,
                    expected_scope={'endpointId': 'pi', 'threadId': CHILD, 'sessionId': CHILD})

    def test_same_text_with_wrong_source_position_or_record_id_is_rejected(self):
        for field, value in (('recordId', 'different-native-record'),):
            w = witness('pi', 'running'); w['snapshot']['transcript']['items'][0][field] = value
            with self.assertRaises(ValueError):
                self._validate(w, 'pi', 'running')
        w = witness('pi', 'running')
        w['snapshot']['transcript']['items'][0]['sourcePosition']['entryOrdinal'] = 99
        with self.assertRaises(ValueError):
            self._validate(w, 'pi', 'running')
        w = witness('dsh', 'running')
        w['snapshot']['transcript']['items'][0]['recordId'] = 'wrong-journal-id'
        with self.assertRaises(ValueError):
            self._validate(w, 'dsh', 'running')

    def test_rejects_unlinked_duplicate_turns_and_changed_opaque_turn_id(self):
        w = witness('pi', 'running')
        w['snapshot']['transcript']['turns'].append(copy.deepcopy(w['snapshot']['transcript']['turns'][0]))
        with self.assertRaises(ValueError):
            self._validate(w, 'pi', 'running')
        w = witness('pi', 'running'); w['snapshot']['transcript']['items'][0]['turnId'] = 'changed'
        with self.assertRaises(ValueError):
            self._validate(w, 'pi', 'running')
        first = valid('pi', 'running')
        failed = valid('pi', 'failed')
        failed['turnId'] = 'another-opaque-id'
        with self.assertRaises(ValueError):
            validate_transcript_pair(first, failed)

    def test_rejects_stale_epoch_and_not_ready_authority(self):
        for path, value in ((('snapshot', 'transcript', 'isStale'), True),
                            (('snapshot', 'connectionEpoch'), 3),
                            (('authority', 'control_ready'), False),
                            (('snapshot', 'connection', 'phase'), 'disconnected')):
            w = witness('pi', 'running')
            target = w
            for part in path[:-1]: target = target[part]
            target[path[-1]] = value
            with self.assertRaises(ValueError):
                self._validate(w, 'pi', 'running')

    def test_running_turn_must_not_be_already_settled(self):
        w = witness('pi', 'running'); w['snapshot']['transcript']['turns'][0]['endedAt'] = 101.0
        with self.assertRaises(ValueError): self._validate(w, 'pi', 'running')

    def test_only_opening_or_inner_input_roles_are_accepted(self):
        w = witness('pi', 'running')
        w['snapshot']['transcript']['items'][0]['metadata']['inputPresentation']['role'] = 'inner'
        self._validate(w, 'pi', 'running')
        w['snapshot']['transcript']['items'][0]['metadata']['inputPresentation']['role'] = 'continuation'
        with self.assertRaises(ValueError): self._validate(w, 'pi', 'running')

    def test_running_native_user_must_follow_latest_unsettled_start_marker(self):
        w = witness('pi', 'running')
        w['authority']['sessions'][CHILD]['native_conversation']['entries'].append(
            {'id': 'unexpected-end', 'type': 'custom', 'customType': 'agentkit.run.v1',
             'data': {'id': 'native-run-1', 'phase': 'settled'}})
        with self.assertRaises(ValueError): self._validate(w, 'pi', 'running')

    def test_failed_turn_requires_current_error_and_end_time(self):
        w = witness('pi', 'failed'); w['snapshot']['transcript']['turns'][0]['endedAt'] = None
        with self.assertRaises(ValueError): self._validate(w, 'pi', 'failed')
        w = witness('dsh', 'failed'); w['snapshot']['transcript']['items'][-1]['sourcePosition']['sequence'] = 88
        with self.assertRaises(ValueError): self._validate(w, 'dsh', 'failed')
        w = witness('dsh', 'failed'); w['snapshot']['transcript']['items'].pop()
        with self.assertRaises(ValueError): self._validate(w, 'dsh', 'failed')

    def test_authored_input_must_link_to_one_transcript_item(self):
        w = witness('pi', 'running')
        w['snapshot']['transcript']['items'].append(copy.deepcopy(w['snapshot']['transcript']['items'][0]))
        with self.assertRaises(ValueError): self._validate(w, 'pi', 'running')

    def test_old_user_cannot_be_paired_to_newer_native_error_boundary(self):
        w = witness('pi', 'failed')
        native = w['authority']['sessions'][CHILD]['native_conversation']
        native['entries'].append({'id': 'later-user', 'type': 'message',
                                  'message': {'role': 'user', 'content': [{'type': 'text', 'text': 'later'}]}})
        with self.assertRaises(ValueError): self._validate(w, 'pi', 'failed')

    def test_dispatching_failed_input_is_rejected(self):
        w = witness('pi', 'failed')
        w['authority']['sessions'][CHILD]['queue'] = [{'placement': 'dispatching',
            'payload': {'sessionId': CHILD, 'mode': 'queue', 'content': [{'type': 'text', 'text': MARKER}]},
            'message': {'content': [{'type': 'text', 'text': MARKER}]}}]
        with self.assertRaises(ValueError): self._validate(w, 'pi', 'failed')

    def test_pair_requires_revision_advance_and_same_source(self):
        running = valid('dsh', 'running')
        failed = valid('dsh', 'failed')
        failed['revision'] = running['revision']
        with self.assertRaises(ValueError): validate_transcript_pair(running, failed)
        failed = valid('dsh', 'failed')
        failed['authoredInput'] = {'kind': 'journal', 'sequence': 90}
        with self.assertRaises(ValueError): validate_transcript_pair(running, failed)
        failed = valid('dsh', 'failed')
        self.assertFalse(validate_transcript_pair(running, failed)['serverTransactionalFreshnessProven'])

    def test_actionables_are_read_from_the_same_authority(self):
        for endpoint in ('pi', 'dsh'):
            w = witness(endpoint, 'running')
            w['authority']['actionables'] = {'request': {'sessionId': CHILD}}
            with self.assertRaises(ValueError): self._validate(w, endpoint, 'running')

    def test_settlement_time_and_fork_boundary_match_the_source(self):
        for endpoint in ('pi', 'dsh'):
            w = witness(endpoint, 'failed')
            w['snapshot']['transcript']['turns'][0]['endedAt'] = 999.0
            with self.assertRaises(ValueError): self._validate(w, endpoint, 'failed')
            w = witness(endpoint, 'failed')
            w['snapshot']['transcript']['turns'][0]['forkBoundary'] = {'entryId': 'another-run'}
            with self.assertRaises(ValueError): self._validate(w, endpoint, 'failed')

    def test_whole_message_sources_reject_a_foreign_content_index(self):
        for endpoint in ('pi', 'dsh'):
            for index in (0, -1):
                w = witness(endpoint, 'failed')
                w['snapshot']['transcript']['items'][index]['sourcePosition']['contentIndex'] = 7
                with self.assertRaises(ValueError): self._validate(w, endpoint, 'failed')

    def test_error_item_identity_must_be_present_and_unique(self):
        w = witness('pi', 'failed')
        w['snapshot']['transcript']['items'][-1]['id'] = None
        with self.assertRaises(ValueError): self._validate(w, 'pi', 'failed')
        w = witness('pi', 'failed')
        duplicate = copy.deepcopy(w['snapshot']['transcript']['items'][-1])
        duplicate['kind'] = 'system'
        w['snapshot']['transcript']['items'].append(duplicate)
        with self.assertRaises(ValueError): self._validate(w, 'pi', 'failed')

    def test_pair_rejects_missing_identity_and_wrong_status(self):
        with self.assertRaises(ValueError):
            validate_transcript_pair({'phase': 'running', 'revision': 1}, {'phase': 'failed', 'revision': 2})
        running, failed = valid('pi', 'running'), valid('pi', 'failed')
        failed['turnStatus'] = 'completed'
        with self.assertRaises(ValueError): validate_transcript_pair(running, failed)

    def test_direct_dsh_transcript_accepts_absent_inventory_without_modifying_it(self):
        for phase in ('running', 'failed'):
            w = witness('dsh', phase)
            w['authority']['sessions'][CHILD]['summary'] = None
            before = copy.deepcopy(w)
            proof = self._validate(w, 'dsh', phase)
            self.assertFalse(proof['inventorySummaryPresent'])
            self.assertEqual(w, before)
        w = witness('pi', 'failed')
        w['authority']['sessions'][CHILD]['summary'] = None
        with self.assertRaises(ValueError): self._validate(w, 'pi', 'failed')

    def test_default_parent_source_profile_still_requires_inventory_and_live_summary_is_checked(self):
        from summary_transition_fixture import validate_transition_child
        child = raw_child('dsh', 'failed'); child['summary'] = None
        with self.assertRaises(ValueError):
            validate_transition_child(child, endpoint='dsh', child_id=CHILD,
                                      phase='failed', marker=MARKER, actionables={})
        w = witness('dsh', 'failed')
        w['authority']['sessions'][CHILD]['summary']['running'] = True
        with self.assertRaises(ValueError): self._validate(w, 'dsh', 'failed')

    def _validate(self, w, endpoint, phase):
        return validate_transcript_source(w, endpoint=endpoint, child_id=CHILD, phase=phase,
            marker=MARKER, expected_epoch=4,
            expected_scope={'endpointId': endpoint, 'threadId': CHILD, 'sessionId': CHILD})


if __name__ == '__main__':
    unittest.main()
