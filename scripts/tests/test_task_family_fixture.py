from pathlib import Path
import sys
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from task_family_fixture import (TaskFamilyEvidenceError, public_exchanges,
    parse_family_exchange,native_task_seed_commands,pi_subagent_seed_commands,dsh_subagent_seed_commands)


class PublicFamilyExchange(unittest.TestCase):
    def fixture(self,kind='native'):
        parent='parent-real';child='child-real'
        scope={'endpointId':kind,'threadId':parent,'sessionId':parent}
        tool={'id':'opaque-parent-tool','name':'task','arguments':{'task_name':'Bounded child proof'}}
        task={'id':tool['id'],'scope':scope,'kind':'task','name':'Bounded child proof','status':'completed'}
        commands=[{'op':'create','cwd':'/owned','saveAs':'parent'},
            {'op':'presentation','sessionId':'parent'},
            {'op':'childCatalog','sessionId':'parent','saveAs':'child'},
            {'op':'call','method':'subagents.history','args':{'parentSessionId':'@parent','childSessionId':'@child','maxMessages':50}}]
        outputs=[{'kind':kind,'ready':True,'version':'fixture-only'},
            {'op':'create','result':{'sessionId':parent}},
            {'op':'presentation','result':{'revision':3,'connectionEpoch':2,'summary':{'scope':scope,'tasks':[task]},'transcript':{'tools':[tool]}}},
            {'op':'childCatalog','result':{'parentAvailable':True,'entries':[{'kind':'child','id':child,'mode':'one-shot'}]}},
            {'op':'call','result':{'parentSessionId':parent,'childSessionId':child,'historyRevision':'cut',
                'sourceCall':{'name':'task','callId':'actual-server-call','arguments':tool['arguments']}}}]
        return commands,outputs

    def test_matched_dtos_do_not_claim_harness_or_ui_acceptance(self):
        c,o=self.fixture();m=parse_family_exchange('native',c,o,'parent-real','/owned')
        self.assertTrue(m['pairedPublicSourceMatched'])
        self.assertFalse(m['realHarnessAcceptance']);self.assertFalse(m['uiAcceptance'])
        self.assertTrue(m['child']['readOnly']);self.assertFalse(m['child']['independentRun'])
        self.assertEqual(m['task']['row']['scope']['sessionId'],'parent-real')

    def test_source_call_alone_cannot_manufacture_a_task(self):
        c,o=self.fixture();o[2]['result']['summary']['tasks']=[]
        with self.assertRaisesRegex(TaskFamilyEvidenceError,'Task/tool'):parse_family_exchange('native',c,o,'parent-real','/owned')

    def test_transcript_is_not_a_raw_bounded_page(self):
        c,o=self.fixture();o[-1]['result']={'tools':[],'items':[]}
        with self.assertRaises(TaskFamilyEvidenceError):parse_family_exchange('native',c,o,'parent-real','/owned')

    def test_wrong_parent_is_rejected(self):
        c,o=self.fixture();o[-1]['result']['parentSessionId']='other'
        with self.assertRaises(TaskFamilyEvidenceError):parse_family_exchange('native',c,o,'parent-real','/owned')

    def test_same_raw_ids_on_another_endpoint_do_not_match_scope(self):
        c,o=self.fixture();o[2]['result']['summary']['scope']['endpointId']='pi'
        with self.assertRaisesRegex(TaskFamilyEvidenceError,'scope'):parse_family_exchange('native',c,o,'parent-real','/owned')

    def test_missing_catalog_identity_cannot_authorize_child(self):
        c,o=self.fixture();o[3]['result']['entries']=[]
        with self.assertRaises(TaskFamilyEvidenceError):parse_family_exchange('native',c,o,'parent-real','/owned')

    def test_invented_parent_presentation_session_field_is_insufficient(self):
        c,o=self.fixture();o[2]['result']['sessionId']='parent-real';del o[2]['result']['summary']['scope']
        with self.assertRaisesRegex(TaskFamilyEvidenceError,'scope'):parse_family_exchange('native',c,o,'parent-real','/owned')

    def test_wrong_handshake_order_and_missing_result_fail_closed(self):
        for mutate in [lambda c,o:o.pop(),lambda c,o:o[0].update(kind='pi'),lambda c,o:o[1].update(op='call')]:
            c,o=self.fixture();mutate(c,o)
            with self.assertRaises(TaskFamilyEvidenceError):public_exchanges('native',c,o)

    def test_unresolved_alias_does_not_become_literal_scope(self):
        c,o=self.fixture();c[-1]['args']['childSessionId']='@missing'
        with self.assertRaises(TaskFamilyEvidenceError):public_exchanges('native',c,o)

    def test_plans_use_existing_public_routes_without_private_source_writes(self):
        native=native_task_seed_commands('explicit-parent','/owned','p','m')
        self.assertTrue(any(row.get('method')=='subagents.history' for row in native))
        for builder in [pi_subagent_seed_commands,dsh_subagent_seed_commands]:
            self.assertEqual(builder('parent','/owned','p','m')[0]['op'],'create')
        with self.assertRaises(ValueError):native_task_seed_commands('x','relative','p','m')

    def test_pi_uses_request_parent_and_canonical_summary_not_an_invented_parent_field(self):
        c,o=self.fixture('pi')
        o[2]['result']['summary'].update(tasks=[],subagents=[{'id':'opaque-call','sessionId':'child-real','title':'Model child'}])
        c[-1]={'op':'childHistory','sessionId':'parent','childSessionId':'child'}
        o[-1]={'op':'childHistory','result':{'items':[],'tools':[]}}
        result=parse_family_exchange('pi',c,o,'parent-real','/owned')
        self.assertIsNone(result['task'])
        self.assertEqual(result['child']['parentScope']['sessionId'],'parent-real')
        self.assertFalse(result['child']['controlAuthorityGrantedByManifest'])

    def test_dsh_child_presentation_must_match_the_requested_child(self):
        c,o=self.fixture('dsh')
        del c[1];del o[2]
        c[-1]={'op':'presentation','sessionId':'child'}
        o[-1]={'op':'presentation','result':{'summary':{'scope':{'endpointId':'dsh','threadId':'child-real','sessionId':'child-real'}}}}
        result=parse_family_exchange('dsh',c,o,'parent-real','/owned')
        self.assertIsNone(result['task'])
        o[-1]['result']['summary']['scope']['sessionId']='foreign-child'
        with self.assertRaises(TaskFamilyEvidenceError):parse_family_exchange('dsh',c,o,'parent-real','/owned')
