from __future__ import annotations
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from native_parent_failure_fixture import (
    NativeParentFailureError, validate_parent_only_catalog, validate_running_to_failed_parent,
)


def parent_sample(raw_status: str, publication: int) -> dict:
    scope = {'endpointId': 'native', 'sessionId': 'parent', 'threadId': 'parent'}
    call_id = 'call-1'
    args = {'task_name': 'Review', 'goal': 'Review', 'instructions': 'Review', 'input_sources': [],
            'output': {'mode': 'inline', 'format': 'text', 'name': 'result', 'path': '', 'contract': 'text'}}
    call = {'seq': 4, 'type': 'tool/call', 'data': {'callId': call_id, 'turn': 9, 'name': 'task', 'arguments': args}}
    denied = raw_status == 'error'
    error = {'turn': 9, 'callId': call_id, 'code': 'denied', 'message': 'Approval rejected'} if denied else None
    result = {'source': {'callId': call_id}, 'content': []} if denied else None
    meta = {'kernel_outcome': 'denied'} if denied else None
    result_pos = {'kind': 'journal', 'sequence': 5, 'contentIndex': None} if denied else None
    entries = {'4': {'event': call}}
    if denied:
        entries['5'] = {'event': {'seq': 5, 'type': 'tool/result', 'data': {'message': result, 'error': error, 'meta': meta}}}
    tool = {'id': 'parent-task-1', 'name': 'task', 'arguments': args, 'phase': raw_status,
            'sourcePosition': {'kind': 'journal', 'sequence': 4, 'contentIndex': None},
            'resultPosition': result_pos, 'result': result, 'resultError': error, 'resultMeta': meta}
    row = {'id': tool['id'], 'scope': scope, 'name': 'Review',
           'status': 'in_progress' if not denied else 'error', 'kind': 'task'}
    witness = {'snapshot': {'revision': publication, 'connectionEpoch': 3,
        'summary': {'scope': scope, 'tasks': [row]}, 'transcript': {'tools': [tool], 'isStale': False}},
        'authority': {'endpoint_id': 'native', 'connection_epoch': 3, 'connection': 'Ready',
            'inventory_ready': True, 'control_ready': True, 'actionables_ready': True, 'actionables': {'approval-action-1': {
                'id': 'approval-action-1', 'sessionId': 'parent', 'revision': 3, 'kind': 'approval',
                'payload': {'approvalId': call_id, 'callId': call_id, 'toolName': 'task'}}},
            'sessions': {'parent': {'journal_ready': True, 'entries': entries}}},
        'projectionSucceeded': True, 'evidenceScope': 'original-publication-and-same-memory-input',
        'serverTransactionalFreshnessProven': False}
    return {'available': True, 'publication': publication, 'connectionEpoch': 3, 'scope': scope,
        'taskId': tool['id'], 'title': 'Review', 'rawStatus': raw_status,
        'canonicalStatus': 'in_progress' if not denied else 'failed',
        'adaptedStatus': 'in_progress' if not denied else 'error',
        'rowId': 100, 'viewInstance': 'view-1', 'titleInstance': 'title-1', 'mounted': True,
        'tool': tool, 'sourceCallId': call_id, 'sourcePosition': tool['sourcePosition'],
        'sourceEntry': {'event': call}, 'draft': {'text': 'keep this draft'}, 'witness': witness}


def response():
    return {'accepted': True, 'outcome': 'rejected', 'actionableId': 'approval-action-1',
            'approvalId': 'call-1', 'expectedRevision': 3}


class NativeParentFailureFixtureTests(unittest.TestCase):
    def test_accepts_real_parent_journal_doing_to_denied_error(self):
        doing, failed = parent_sample('doing', 10), parent_sample('error', 12)
        failed['approval'] = {'actionableId': 'approval-action-1', 'approvalId': 'call-1', 'revision': 3,
                              'actionable': {'id': 'approval-action-1', 'sessionId': 'parent', 'revision': 3,
                                  'kind': 'approval', 'payload': {'approvalId': 'call-1', 'callId': 'call-1', 'toolName': 'task'}}}
        receipt = validate_running_to_failed_parent(doing, failed, approval_response=response(),
                                                     expected_draft='keep this draft')
        self.assertEqual(receipt['transition'], 'doing->failed')
        self.assertEqual(receipt['sourceDialect'], 'journal')
        self.assertTrue(receipt['sameMountedSummaryIdentity'])

    def test_rejects_native_or_other_unpaired_source(self):
        doing, failed = parent_sample('doing', 10), parent_sample('error', 12)
        failed['approval'] = {'actionableId': 'approval-action-1', 'approvalId': 'call-1', 'revision': 3,
                              'actionable': {'id': 'approval-action-1', 'sessionId': 'parent', 'revision': 3,
                                  'kind': 'approval', 'payload': {'approvalId': 'call-1', 'callId': 'call-1', 'toolName': 'task'}}}
        failed['sourceEntry']['event']['data']['callId'] = 'other-call'
        with self.assertRaises(NativeParentFailureError):
            validate_running_to_failed_parent(doing, failed, approval_response=response(),
                                              expected_draft='keep this draft')

    def test_rejects_changed_task_and_mount_identity(self):
        doing, failed = parent_sample('doing', 10), parent_sample('error', 12)
        failed['approval'] = {'actionableId': 'approval-action-1', 'approvalId': 'call-1', 'revision': 3,
                              'actionable': {'id': 'approval-action-1', 'sessionId': 'parent', 'revision': 3,
                                  'kind': 'approval', 'payload': {'approvalId': 'call-1', 'callId': 'call-1', 'toolName': 'task'}}}
        failed['viewInstance'] = 'replacement-view'
        with self.assertRaises(NativeParentFailureError):
            validate_running_to_failed_parent(doing, failed, approval_response=response(),
                                              expected_draft='keep this draft')

    def test_rejects_lost_draft_or_approval_mismatch(self):
        doing, failed = parent_sample('doing', 10), parent_sample('error', 12)
        failed['approval'] = {'actionableId': 'approval-action-1', 'approvalId': 'wrong-call', 'revision': 3,
                              'actionable': {'id': 'approval-action-1', 'sessionId': 'parent', 'revision': 3,
                                  'kind': 'approval', 'payload': {'approvalId': 'wrong-call', 'callId': 'call-1', 'toolName': 'task'}}}
        failed['draft']['text'] = ''
        with self.assertRaises(NativeParentFailureError):
            validate_running_to_failed_parent(doing, failed, approval_response=response(),
                                              expected_draft='keep this draft')

    def test_rejects_cross_epoch_or_non_monotonic_transition(self):
        for mutation in ('epoch', 'publication'):
            doing, failed = parent_sample('doing', 10), parent_sample('error', 12)
            original = {'id': 'approval-action-1', 'sessionId': 'parent', 'revision': 3,
                        'kind': 'approval', 'payload': {'approvalId': 'call-1', 'callId': 'call-1', 'toolName': 'task'}}
            failed['approval'] = {'actionableId': 'approval-action-1', 'approvalId': 'call-1',
                                  'revision': 3, 'actionable': original}
            if mutation == 'epoch':
                failed['connectionEpoch'] = 4
            else:
                failed['publication'] = 10
            with self.subTest(mutation=mutation), self.assertRaises(NativeParentFailureError):
                validate_running_to_failed_parent(doing, failed, approval_response=response(),
                                                  expected_draft='keep this draft')

    def test_rejects_different_parent_scope(self):
        doing, failed = parent_sample('doing', 10), parent_sample('error', 12)
        original = {'id': 'approval-action-1', 'sessionId': 'parent', 'revision': 3,
                    'kind': 'approval', 'payload': {'approvalId': 'call-1', 'callId': 'call-1', 'toolName': 'task'}}
        failed['approval'] = {'actionableId': 'approval-action-1', 'approvalId': 'call-1',
                              'revision': 3, 'actionable': original}
        failed['scope']['sessionId'] = 'other-parent'
        failed['scope']['threadId'] = 'other-parent'
        with self.assertRaises(NativeParentFailureError):
            validate_running_to_failed_parent(doing, failed, approval_response=response(),
                                              expected_draft='keep this draft')

    def test_rejects_unrelated_actionable_payload(self):
        doing, failed = parent_sample('doing', 10), parent_sample('error', 12)
        original = {'id': 'approval-action-1', 'sessionId': 'parent', 'revision': 3,
                    'kind': 'approval', 'payload': {'approvalId': 'call-1', 'callId': 'different-call', 'toolName': 'task'}}
        doing['witness']['authority']['actionables']['approval-action-1'] = original
        failed['approval'] = {'actionableId': 'approval-action-1', 'approvalId': 'call-1',
                              'revision': 3, 'actionable': original}
        with self.assertRaises(NativeParentFailureError):
            validate_running_to_failed_parent(doing, failed, approval_response=response(),
                                              expected_draft='keep this draft')

    def test_child_catalog_must_explicitly_show_parent_only(self):
        value = {'parentAvailable': True, 'entries': [{'kind': 'parent', 'id': 'parent', 'hasChildren': False}]}
        self.assertEqual(validate_parent_only_catalog(value, 'parent')['childCount'], 0)
        value['entries'].append({'kind': 'child', 'id': 'child', 'hasChildren': False})
        with self.assertRaises(NativeParentFailureError):
            validate_parent_only_catalog(value, 'parent')


if __name__ == '__main__':
    unittest.main()
