from __future__ import annotations

import copy
import importlib.util
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from native_summary_transition_fixture import validate_parent_summary_sample

SPEC = importlib.util.spec_from_file_location(
    'native_paged_runner', ROOT / 'scripts/test-real-native-paged-child.py')
RUNNER = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(RUNNER)


def sample(raw_status: str, publication: int) -> dict:
    scope = {'endpointId': 'native', 'sessionId': 'p', 'threadId': 'p'}
    canonical = {'doing': 'in_progress', 'running': 'in_progress', 'error': 'failed', 'done': 'completed'}[raw_status]
    adapted = 'in_progress' if raw_status == 'doing' else raw_status
    task = {'id': 'opaque-parent-task-row', 'scope': scope, 'name': 'Review',
            'status': adapted, 'kind': 'task'}
    call = {'seq': 4, 'type': 'tool/call', 'data': {
        'callId': 'native-child-proof', 'turn': 9, 'name': 'task',
        'arguments': {'task_name': 'Review'}}}
    source_position = {'kind': 'journal', 'sequence': 4, 'contentIndex': None}
    result_position = None
    result = None
    result_error = None
    result_meta = None
    entries = {'4': {'event': call}}
    if raw_status == 'error':
        result_error = {'turn': 9, 'callId': 'native-child-proof',
                        'message': 'child failed'}
        result_position = {'kind': 'journal', 'sequence': 5, 'contentIndex': None}
        entries['5'] = {'event': {'seq': 5, 'type': 'response/error',
                                  'data': copy.deepcopy(result_error)}}
    if raw_status == 'done':
        result_position = {'kind': 'journal', 'sequence': 5, 'contentIndex': None}
        result = {'source': {'callId': 'native-child-proof'}, 'content': []}
        result_meta = {'kernel_outcome': 'ok'}
        entries['5'] = {'event': {'seq': 5, 'type': 'tool/result', 'data': {'message': result, 'meta': result_meta}}}
    tool = {'id': task['id'], 'name': 'task', 'arguments': {'task_name': 'Review'},
            'phase': raw_status, 'sourcePosition': source_position,
            'resultPosition': result_position, 'result': result, 'resultError': result_error, 'resultMeta': result_meta}
    witness = {
        'snapshot': {'revision': publication, 'connectionEpoch': 3,
                     'summary': {'scope': scope, 'tasks': [task]},
                     'transcript': {'tools': [tool], 'isStale': False}},
        'authority': {'endpoint_id': 'native', 'connection_epoch': 3,
                      'connection': 'Ready', 'inventory_ready': True, 'control_ready': True, 'actionables_ready': True,
                      'sessions': {'p': {'journal_ready': True, 'entries': entries}}},
        'projectionSucceeded': True,
        'evidenceScope': 'original-publication-and-same-memory-input',
        'serverTransactionalFreshnessProven': False,
    }
    return {
        'available': True, 'phase': 'running' if raw_status in ('running', 'doing') else 'settled',
        'publication': publication, 'connectionEpoch': 3, 'scope': scope,
        'taskId': task['id'], 'title': 'Review', 'rawStatus': raw_status,
        'canonicalStatus': canonical, 'adaptedStatus': adapted, 'rowId': 100, 'viewInstance': 'view-opaque',
        'titleInstance': 'title-opaque', 'mounted': True,
        'passive': True, 'actionable': False, 'independentRun': False,
        'tool': copy.deepcopy(tool), 'sourceCallId': 'native-child-proof',
        'sourcePosition': source_position, 'sourceEntry': {'event': call},
        'witness': witness,
    }


def frames(running: dict, settled: dict, accepted: bool = True) -> list[dict]:
    return [
        {'phase': 'running_head', 'summaryRunning': running},
        {'phase': 'settled_refresh', 'summarySettled': settled, 'summaryTransition': {
            'accepted': accepted,
            'unavailableReason': None if accepted else 'parent Summary did not publish failed',
            'profile': 'native-passive-spawn-tool', 'formalTaskLifecycleAcceptance': False,
            'parentToolFailureAcceptance': False,
            'parentProjectionOnly': True, 'statusComesFromChildWitness': False,
            'running': running, 'settled': settled,
        }},
    ]


class NativeSummaryTransitionValidatorTests(unittest.TestCase):
    def test_accepts_spawn_doing_to_done_without_formal_task_claim(self):
        running, completed = sample('doing', 10), sample('done', 12)
        result = RUNNER.validate_summary_transition(frames(running, completed))
        self.assertTrue(result['accepted'])
        self.assertEqual(validate_parent_summary_sample(running)['status'], 'in_progress')
        self.assertEqual(validate_parent_summary_sample(completed)['status'], 'done')

    def test_spawn_profile_cannot_claim_parent_failure_or_formal_task_lifecycle(self):
        for flag in ('parentToolFailureAcceptance', 'formalTaskLifecycleAcceptance'):
            candidate = frames(sample('doing', 10), sample('done', 12))
            candidate[-1]['summaryTransition'][flag] = True
            with self.subTest(flag=flag), self.assertRaisesRegex(ValueError, 'cannot claim'):
                RUNNER.validate_summary_transition(candidate)
        with self.assertRaisesRegex(ValueError, 'passive Task row'):
            RUNNER.validate_summary_transition(frames(sample('doing', 10), sample('error', 12)))

    def test_opaque_call_is_resolved_from_raw_journal_position(self):
        source = validate_parent_summary_sample(sample('doing', 10))
        self.assertEqual(source['sourceCallId'], 'native-child-proof')
        self.assertEqual(source['taskId'], 'opaque-parent-task-row')
        invalid = sample('doing', 10)
        invalid['witness']['authority']['sessions']['p']['entries']['4']['event']['data']['callId'] = 'other'
        with self.assertRaisesRegex(ValueError, 'opaque source call ID'):
            validate_parent_summary_sample(invalid)

    def test_preserves_missing_parent_terminal_as_negative_result(self):
        settled = {'available': False, 'phase': 'settled',
                   'reason': 'parent Summary has no authoritative terminal'}
        result = RUNNER.validate_summary_transition(frames(
            sample('doing', 10), settled, False))
        self.assertFalse(result['accepted'])
        self.assertEqual(result['unavailableReason'], 'parent Summary did not publish failed')

    def test_rejects_sample_fields_not_equal_to_exact_witness(self):
        mutations = [
            lambda bad: bad.update(publication=True),
            lambda bad: bad.update(connectionEpoch=4),
            lambda bad: bad.update(rawStatus='in_progress'),
            lambda bad: bad['tool'].update(phase='error'),
            lambda bad: bad['witness']['snapshot'].update(revision=11),
            lambda bad: bad['witness']['authority'].update(connection_epoch=True),
        ]
        for mutate in mutations:
            candidate = sample('doing', 10)
            mutate(candidate)
            with self.subTest(mutation=mutate), self.assertRaises(ValueError):
                validate_parent_summary_sample(candidate)

    def test_rejects_result_position_retarget_and_wrong_original_call_id(self):
        for mutate in (
            lambda bad: bad['witness']['snapshot']['transcript']['tools'][0].update(
                resultPosition={'kind': 'journal', 'sequence': 99, 'contentIndex': None}),
            lambda bad: bad['witness']['authority']['sessions']['p']['entries']['5']['event']['data'].update(
                callId='different-call'),
            lambda bad: bad['tool'].update(resultError={'turn': 9, 'callId': 'different-call'}),
        ):
            candidate = sample('error', 12)
            mutate(candidate)
            with self.subTest(mutation=mutate), self.assertRaises(ValueError):
                validate_parent_summary_sample(candidate)

    def test_rejects_unready_stale_and_success_result_labeled_as_error(self):
        for mutate in (
            lambda bad: bad['witness']['authority'].update(connection='Failed'),
            lambda bad: bad['witness']['authority'].update(control_ready=False),
            lambda bad: bad['witness']['snapshot']['transcript'].update(isStale=True),
        ):
            bad = sample('doing', 10)
            mutate(bad)
            with self.subTest(mutation=mutate), self.assertRaisesRegex(ValueError, 'fresh and ready'):
                validate_parent_summary_sample(bad)
        bad = sample('error', 12)
        message = {'source': {'callId': 'native-child-proof'}, 'content': []}
        tool = bad['witness']['snapshot']['transcript']['tools'][0]
        tool.update(result=message, resultError=None)
        bad['tool'] = copy.deepcopy(tool)
        bad['witness']['authority']['sessions']['p']['entries']['5']['event'] = {
            'seq': 5, 'type': 'tool/result', 'data': {'message': message}}
        with self.assertRaisesRegex(ValueError, 'public failure facts'):
            validate_parent_summary_sample(bad)
        error = {'code': 'child_failed', 'message': 'controlled'}
        bad['witness']['authority']['sessions']['p']['entries']['5']['event']['data']['error'] = error
        tool['resultError'] = error
        bad['tool'] = copy.deepcopy(tool)
        self.assertEqual(validate_parent_summary_sample(bad)['status'], 'error')

    def test_native_result_uses_whole_public_tool_result_message(self):
        bad = sample('error', 12)
        call = {'type': 'toolCall', 'id': 'native-child-proof', 'name': 'task',
                'arguments': {'task_name': 'Review'}}
        message = {'role': 'toolResult', 'toolCallId': 'native-child-proof',
                   'isError': True, 'content': [{'type': 'text', 'text': 'controlled error'}]}
        position = {'kind': 'native', 'entryId': 'call-entry', 'entryOrdinal': 0, 'contentIndex': 0}
        result_position = {'kind': 'native', 'entryId': 'result-entry', 'entryOrdinal': 1, 'contentIndex': None}
        parent = bad['witness']['authority']['sessions']['p']
        parent['native_conversation'] = {'entries': [
            {'id': 'call-entry', 'message': {'role': 'assistant', 'content': [call]}},
            {'id': 'result-entry', 'message': message},
        ]}
        tool = bad['witness']['snapshot']['transcript']['tools'][0]
        tool.update(sourcePosition=position, resultPosition=result_position, result=message, resultError=None)
        bad.update(sourcePosition=position, sourceEntry=call, tool=copy.deepcopy(tool))
        self.assertEqual(validate_parent_summary_sample(bad)['status'], 'error')
        message['isError'] = False
        bad['tool'] = copy.deepcopy(tool)
        with self.assertRaisesRegex(ValueError, 'original Native result'):
            validate_parent_summary_sample(bad)

    def test_rejects_malformed_and_negative_native_positions(self):
        for position in (
            {'kind': 'journal', 'sequence': True, 'contentIndex': None},
            {'kind': 'native', 'entryId': 'e', 'entryOrdinal': -1, 'contentIndex': 0},
            {'kind': 'native', 'entryId': 'e', 'entryOrdinal': 0, 'contentIndex': -1},
            None,
        ):
            candidate = sample('doing', 10)
            candidate['witness']['snapshot']['transcript']['tools'][0]['sourcePosition'] = position
            candidate['tool']['sourcePosition'] = position
            candidate['sourcePosition'] = position
            with self.subTest(position=position), self.assertRaises(ValueError):
                validate_parent_summary_sample(candidate)

    def test_requires_mounted_view_row_and_title_identity(self):
        for field, value in (('mounted', False), ('rowId', True), ('viewInstance', ''),
                             ('titleInstance', None)):
            candidate = sample('doing', 10)
            candidate[field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate_parent_summary_sample(candidate)

    def test_rejects_row_retarget_passivity_and_child_witness_substitution(self):
        terminal = sample('done', 12)
        terminal['taskId'] = 'different-call'
        with self.assertRaisesRegex(ValueError, 'does not map|identity/publication'):
            RUNNER.validate_summary_transition(frames(sample('doing', 10), terminal))
        terminal = sample('done', 12)
        terminal['passive'] = False
        with self.assertRaisesRegex(ValueError, 'passive Task row'):
            RUNNER.validate_summary_transition(frames(sample('doing', 10), terminal))
        transition = frames(sample('doing', 10), sample('done', 12))[1]['summaryTransition']
        transition['statusComesFromChildWitness'] = True
        with self.assertRaisesRegex(ValueError, 'substitutes child'):
            RUNNER.validate_summary_transition([
                {'phase': 'running_head', 'summaryRunning': sample('doing', 10)},
                {'phase': 'settled_refresh', 'summarySettled': sample('done', 12),
                 'summaryTransition': transition}])


if __name__ == '__main__':
    unittest.main()
