from __future__ import annotations

import unittest
import json
from pathlib import Path

from native_child_pane_source_fixture import MARKER
from native_paged_child_fixture import (CHILD_MARKER, STEP_TEXT, is_paged_child_request,
    phases_for_pages, validate_public_page_plan, _compare_entries,
    _read_arguments, _validate_intent_binding, _validate_prior_read_result, _validate_receipt_chain,
    _validate_projected_content, _validate_settled_witness, normalized_entry, validate_public_history)


def entry(seq: int, text: str, kind: str = 'assistant/message', append: bool = True) -> dict:
    event = {'type': kind, 'seq': seq, 'time': seq + 1,
             'data': {'text': text, 'nested': {'view': None}}}
    if append:
        event['surfaceOp'] = 'append'
    return {'event': event, 'view': None, 'isModelOnly': None}


class NativePagedChildFixtureTests(unittest.TestCase):
    def test_public_page_plan_requires_exact_frozen_frontier_and_complete_union(self):
        entries=[entry(i,str(i)) for i in range(5)]
        page={'parentSessionId':'p','childSessionId':'c','throughSequence':4,
              'historyRevision':'revision','entries':entries[1:],'hasMoreBefore':True}
        older=dict(page,entries=entries[:1],hasMoreBefore=False)
        groundtruth=dict(page,entries=entries,onePage=page)
        head={'parentSessionId':'p','childSessionId':'c','maxMessages':1}
        request=dict(head,beforeSequence=1,throughSequence=4,historyRevision='revision')
        result=validate_public_page_plan(groundtruth,[page,older],[head,request])
        self.assertEqual(len(result['pagePlan']),2)
        self.assertEqual(phases_for_pages(1),('running_head','reconnected_refresh',
                                            'reconnected_page_1','settled_refresh'))
        self.assertEqual(len(phases_for_pages(3)),6)
        with self.assertRaisesRegex(ValueError,'exact frozen frontier'):
            validate_public_page_plan(groundtruth,[page,older],[head,dict(request,beforeSequence=2)])
        with self.assertRaisesRegex(ValueError,'differ'):
            validate_public_page_plan(dict(groundtruth,entries=entries[:-1]),[page,older],[head,request])

    def test_reuses_existing_child_marker_and_ignorable_target_texts(self):
        self.assertEqual(CHILD_MARKER, MARKER)
        self.assertEqual(STEP_TEXT, tuple(f'NATIVE_PAGED_STEP_{i}' for i in range(1, 5)))

    def test_read_arguments_are_derived_from_public_schema(self):
        body = {'tools': [{'type': 'function', 'name': 'read', 'parameters': {
            'type': 'object', 'properties': {'path': {'type': 'string'}}, 'required': ['path']}}]}
        self.assertEqual(_read_arguments(body, Path('/owned/proof.txt')),
                         ('path', {'path': 'proof.txt'}))
        ambiguous = {'tools': [{'name': 'read', 'parameters': {'type': 'object',
            'properties': {'path': {'type': 'string'}, 'file': {'type': 'string'}},
            'required': ['path', 'file']}}]}
        with self.assertRaisesRegex(ValueError, 'one required string path'):
            _read_arguments(ambiguous, Path('/owned/proof.txt'))

    def test_history_groundtruth_requires_multiple_real_child_messages(self):
        parent = {'endpointId': 'native', 'threadId': 'parent', 'sessionId': 'parent'}
        entries = [entry(1, STEP_TEXT[3]), entry(2, 'call-1', 'tool/call', False),
                   entry(3, STEP_TEXT[0]), entry(4, 'call-2', 'tool/call', False),
                   entry(5, STEP_TEXT[1]), entry(6, 'call-3', 'tool/call', False),
                   entry(7, STEP_TEXT[2])]
        head = {'parentSessionId': 'parent', 'childSessionId': 'child', 'throughSequence': 7,
                'historyRevision': 'r1', 'entries': entries, 'hasMoreBefore': False, 'runOpen': True,
                'sourceCall': {'phase':'running','arguments':{'instructions':MARKER}}}
        one = dict(head, entries=entries[-2:], hasMoreBefore=True)
        proof = validate_public_history(head, one, parent=parent, child_id='child')
        self.assertEqual(proof['entryCount'], 7)
        self.assertEqual(proof['entries'], entries)
        head['entries'] = entries[:3]
        with self.assertRaisesRegex(ValueError, 'four append-origin messages'):
            validate_public_history(head, one, parent=parent, child_id='child')

    def test_continuation_uses_issued_response_or_tool_identity(self):
        self.assertTrue(is_paged_child_request({'previous_response_id':'native-paged-step-1'}))
        self.assertTrue(is_paged_child_request({'input':[{'type':'function_call_output',
                                                        'call_id':'native-paged-read-2'}]}))
        self.assertFalse(is_paged_child_request({'input':[{'type':'function_call_output',
                                                         'call_id':'unrelated'}]}))
        self.assertFalse(is_paged_child_request({'input':None}))

    def test_required_read_offset_and_limit_follow_public_schema(self):
        body = {'tools':[{'name':'read','parameters':{'properties':{
            'path':{'type':'string'},'offset':{'type':'integer'},'limit':{'type':'integer'}},
            'required':['path','offset','limit']}}]}
        self.assertEqual(_read_arguments(body,Path('/owned/proof.txt')),
                         ('path',{'path':'proof.txt','offset':1,'limit':2}))

    def test_prior_public_read_result_accepts_json_encoded_tool_output(self):
        body = {'input': [{'type': 'function_call_output', 'call_id': 'read-1',
                           'output': json.dumps({'path': 'proof.txt', 'content': 'owned proof contents'})}]}
        self.assertTrue(_validate_prior_read_result(body, 'read-1', 'owned proof contents'))
        with self.assertRaisesRegex(ValueError, 'did not contain'):
            _validate_prior_read_result(body, 'read-2', 'owned proof contents')

    def test_intent_is_pre_read_publication_and_respects_epoch_semantics(self):
        scope = {'endpointId': 'native', 'threadId': 'p', 'sessionId': 'c'}
        snapshot = {'publication': 12, 'connectionEpoch': 3}
        _validate_intent_binding({'read': 'refresh', 'scope': scope,
            'publication': 10, 'connectionEpoch': 2}, snapshot, 'refresh', scope, 'refresh')
        _validate_intent_binding({'read': 'earlier', 'scope': scope,
            'publication': 11, 'connectionEpoch': 3}, snapshot, 'earlier', scope, 'earlier')
        with self.assertRaisesRegex(ValueError, 'does not precede'):
            _validate_intent_binding({'read': 'earlier', 'scope': scope,
                'publication': 11, 'connectionEpoch': 2}, snapshot, 'earlier', scope, 'stale-earlier')
        with self.assertRaisesRegex(ValueError, 'does not precede'):
            _validate_intent_binding({'read': 'refresh', 'scope': scope,
                'publication': 12, 'connectionEpoch': 3}, snapshot, 'refresh', scope, 'refresh')

    def test_normalization_only_omits_declared_none_fields(self):
        raw = {'event': {'type': 'message', 'seq': 1, 'time': 2,
                         'data': {'view': None, 'nested': {'ignorable': None}},
                         'ignorable': None}, 'view': None, 'isModelOnly': None}
        expected = {'event': {'type': 'message', 'seq': 1, 'time': 2,
                              'data': {'view': None, 'nested': {'ignorable': None}}}}
        self.assertEqual(normalized_entry(raw), expected)
        _compare_entries([raw], [expected])
        changed = {'event': {'type': 'message', 'seq': 1, 'time': 2,
                             'data': {'view': 'different', 'nested': {'ignorable': None}}}}
        with self.assertRaisesRegex(ValueError, 'differ'):
            _compare_entries([raw], [changed])

    def test_typed_earlier_receipt_uses_exclusive_current_frontier(self):
        parent = {'endpointId': 'native', 'threadId': 'p', 'sessionId': 'p'}
        head = entry(8, 'head')
        older = entry(7, 'older')

        def receipt(operation, req, value, *, reset=False, merged=True):
            return {'operation': operation, 'readEpoch': 1, 'request': req,
                    'response': {'kind': 'page', 'value': value}, 'reset': reset,
                    'merged': merged, 'effectiveHasMoreBefore': True}

        base = {'parentSessionId': 'p', 'childSessionId': 'c', 'throughSequence': None,
                'beforeSequence': None, 'maxMessages': 1, 'historyRevision': None}
        rev = 'a' * 64
        page_head = {'parentSessionId': 'p', 'childSessionId': 'c', 'throughSequence': 9,
                     'historyRevision': rev, 'entries': [head], 'hasMoreBefore': True,
                     'runOpen': True, 'sourceCall': None}
        earlier_req = dict(base, throughSequence=9, beforeSequence=8, historyRevision=rev)
        page_older = dict(page_head, entries=[older], hasMoreBefore=False)
        _validate_receipt_chain([
            receipt('initialHead', base, page_head, reset=True),
            receipt('earlier', earlier_req, page_older),
        ], parent, 'c')
        bad = dict(earlier_req, beforeSequence=9)
        with self.assertRaisesRegex(ValueError, 'current exclusive page frontier'):
            _validate_receipt_chain([
                receipt('initialHead', base, page_head, reset=True),
                receipt('earlier', bad, page_older),
            ], parent, 'c')

    def test_projection_checks_assistant_tool_result_and_task_status(self):
        scope={'endpointId':'native','threadId':'p','sessionId':'c'}
        call={'callId':'task','name':'task','arguments':{'instructions':MARKER},'phase':'running'}
        result={'role':'user','content':[{'type':'text','text':'proof'}]}
        entries=[{'event':{'type':'tool/call','seq':2,'data':{'name':'read','arguments':{'path':'proof.txt'}}}},
                 {'event':{'type':'tool/result','seq':3,'data':{'message':result,'meta':{'kernel_outcome':'ok'}}}},
                 {'event':{'type':'assistant/message','seq':4,'surfaceOp':'append',
                    'data':{'message':{'content':[{'type':'text','text':'assistant output'}]}}}}]
        turn=json.dumps(['native','c','task-source','task'],separators=(',',':'))
        snapshot={'scope':scope,'transcript':{
            'items':[{'kind':'assistant','text':'assistant output','isVisible':True,
                      'isProvisional':False,'sourcePosition':{'sequence':4}}],
            'tools':[{'id':turn+':call','phase':'running','arguments':call['arguments']},
                     {'sourcePosition':{'sequence':2},'resultPosition':{'sequence':3},
                      'name':'read','arguments':{'path':'proof.txt'},'result':result,
                      'resultMeta':{'kernel_outcome':'ok'}}],
            'turns':[{'id':turn,'status':'running'}]}}
        source={'projectedEntries':entries,'sourceCall':call}
        _validate_projected_content(source,snapshot)
        snapshot['transcript']['items'][0]['text']='wrong output'
        with self.assertRaisesRegex(ValueError,'assistant content differs'):
            _validate_projected_content(source,snapshot)
        snapshot['transcript']['items'][0]['text']='assistant output'
        snapshot['transcript']['tools'][1]['result']={'wrong':'result'}
        with self.assertRaisesRegex(ValueError,'tool result differs'):
            _validate_projected_content(source,snapshot)

    def test_settled_witness_keeps_old_groundtruth_and_merges_new_error_page(self):
        parent = {'endpointId': 'native', 'threadId': 'p', 'sessionId': 'p'}
        old = [entry(1, 'old-1'), entry(2, 'old-2')]
        terminal = [
            {'event': {'type': 'response/error', 'seq': 3, 'time': 3,
                       'data': {'turn': 4, 'message': 'controlled failure'}}},
            {'event': {'type': 'turn/end', 'seq': 4, 'time': 4,
                       'data': {'turn': 4, 'reason': 'error'}}},
        ]
        revision, new_revision = 'a' * 64, 'b' * 64

        def page(entries, cut, rev, opened, call):
            return {'parentSessionId': 'p', 'childSessionId': 'c',
                    'throughSequence': cut, 'historyRevision': rev, 'entries': entries,
                    'hasMoreBefore': False, 'runOpen': opened, 'sourceCall': call}

        def receipt(operation, request, response_page, *, merged, reset=False):
            return {'operation': operation, 'readEpoch': 2, 'request': request,
                    'response': {'kind': 'page', 'value': response_page},
                    'reset': reset, 'merged': merged, 'effectiveHasMoreBefore': False}

        head_request = {'parentSessionId': 'p', 'childSessionId': 'c', 'throughSequence': None,
                        'beforeSequence': None, 'maxMessages': 1, 'historyRevision': None}
        old_head = page([old[-1]], 2, revision, True, {'callId': 'task-1', 'name': 'task',
                                                 'arguments': {}, 'phase': 'running'})
        old_head['hasMoreBefore']=True
        new_head = page(terminal, 4, new_revision, False, {'callId': 'task-1', 'name': 'task',
                                                            'arguments': {}, 'phase': 'error'})
        prefix_request = {'parentSessionId': 'p', 'childSessionId': 'c', 'throughSequence': 2,
                          'beforeSequence': None, 'maxMessages': 1, 'historyRevision': revision}
        # runOpen follows frozen events; sourceCall phase is current parent
        # metadata and legitimately advances from running to error.
        prefix = page([old[-1]], 2, revision, True, new_head['sourceCall'])
        prefix['hasMoreBefore']=True
        earlier_request=dict(prefix_request,beforeSequence=2)
        earlier_receipt=receipt('earlier',earlier_request,
            page(old[:1],2,revision,True,old_head['sourceCall']),merged=True)
        source = {'throughSequence': 4, 'historyRevision': new_revision,
                  'projectedEntries': old + terminal,
                  'receipts': [
                      receipt('initialHead', head_request, old_head, merged=True, reset=True),
                      earlier_receipt,
                      receipt('refreshHead', head_request, new_head, merged=True),
                      receipt('prefixValidation', prefix_request, prefix, merged=False),
                  ]}
        groundtruth = {'throughSequence': 2, 'historyRevision': revision,
                       'entries': old, 'onePage': dict(old_head,entries=[old[-1]])}
        _validate_settled_witness(source, groundtruth)
        for key, value in [('hasMoreBefore',False),('runOpen',False),
                           ('sourceCall',dict(new_head['sourceCall'],phase='unexpected')),
                           ('sourceCall',dict(new_head['sourceCall'],callId='wrong-task'))]:
            original=prefix[key]
            prefix[key]=value
            with self.subTest(field=key,value=value):
                with self.assertRaisesRegex(ValueError,'metadata changed|task phase'):
                    _validate_settled_witness(source, groundtruth)
            prefix[key]=original
        intermediate = page(old + terminal[:1],3,'c'*64,True,old_head['sourceCall'])
        original_prefix = receipt('prefixValidation',prefix_request,prefix,merged=False)
        intermediate_prefix = page(intermediate['entries'],3,'c'*64,True,new_head['sourceCall'])
        intermediate_request = dict(prefix_request,throughSequence=3,historyRevision='c'*64)
        source['receipts']=[
            receipt('initialHead',head_request,old_head,merged=True,reset=True),
            earlier_receipt,
            receipt('refreshHead',head_request,intermediate,merged=True),original_prefix,
            receipt('refreshHead',head_request,new_head,merged=True),
            receipt('prefixValidation',intermediate_request,intermediate_prefix,merged=False)]
        _validate_settled_witness(source,groundtruth)
        source['receipts'][-1]['request']=prefix_request
        with self.assertRaisesRegex(ValueError,'immediately prior frozen cut'):
            _validate_settled_witness(source,groundtruth)


if __name__ == '__main__':
    unittest.main()
