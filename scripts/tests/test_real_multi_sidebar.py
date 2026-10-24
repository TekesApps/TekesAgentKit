"""Negative acceptance checks; synthetic DTOs do not prove real harness delivery."""
import copy
import importlib.util
from pathlib import Path
import sys
import json
import os
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
spec = importlib.util.spec_from_file_location('real_multi_sidebar', Path(__file__).resolve().parents[1]/'test-real-multi-sidebar.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class RecoveryProcessOwnership(unittest.TestCase):
    def test_success_retains_logs_and_proves_owned_session_exit(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            result = module.run_recovery_probe([sys.executable, '-c', 'print("owned probe")'],
                                               {'PATH': '/usr/bin:/bin'}, output, timeout=5)
            self.assertEqual(result.returncode, 0)
            self.assertIn('owned probe', (output/'demo.stdout').read_text())
            self.assertTrue(json.loads((output/'probe-exit.json').read_text())['groupExited'])

    def test_timeout_closes_owned_descendant_and_retains_exit_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            script = ('import subprocess,sys,time; '
                      'child=subprocess.Popen([sys.executable,"-c","import time; time.sleep(60)"]); '
                      'print(child.pid,flush=True); time.sleep(60)')
            with self.assertRaisesRegex(AssertionError, 'recovery probe failed'):
                module.run_recovery_probe([sys.executable, '-c', script],
                                          {'PATH': '/usr/bin:/bin'}, output, timeout=0.5)
            receipt = json.loads((output/'probe-exit.json').read_text())
            self.assertTrue(receipt['timedOut'])
            self.assertTrue(receipt['groupExited'])
            with self.assertRaises(ProcessLookupError):
                os.kill(int((output/'demo.stdout').read_text().strip()), 0)


class MultiSidebarAcceptance(unittest.TestCase):
    def setUp(self):
        self.seeds = [dict(endpointId=endpoint, sessionId=str(index), title=endpoint+str(index), cwd='/owned')
                      for endpoint in ['native','pi','dsh'] for index in range(3)]
        rows = [dict(address={'endpointId':seed['endpointId'],'sessionId':seed['sessionId']},
                     title=seed['title'],folderPath=seed['cwd']) for seed in self.seeds]
        mounted = [dict(address=row['address'],rowId=index+100,presentInContentHost=True)
                   for index,row in enumerate(rows)]
        receipt = {'source':{'endpointCount':3,'loaded':True,'errorPresent':False,'rows':rows,
                            'observer':[dict(endpointId=endpoint,loaded=True,stale=False,errorPresent=False,
                                             observation={'phase':'observing','converged':True})
                                        for endpoint in ['native','pi','dsh']]},
                   'display':{'rows':rows,'paging':{'hasMore':True,'limit':5},
                              'visibilityTarget':{'address':rows[0]['address'],'itemId':100}},
                   'host':{'mountedRows':mounted[:6],'focusedComponentId':900,
                           'visibilityTargetPresentInContentHost':True}}
        self.delivery = {'stages':[]}
        for stage in ['baseline','after-original-page-action','query']:
            current = copy.deepcopy(receipt)
            if stage != 'baseline':
                current['display']['paging'] = {'hasMore':False,'limit':10}
                current['host']['mountedRows'] = copy.deepcopy(mounted)
            if stage == 'query':
                current['host']['appliedSearchProjection'] = {'localResultsReady':True,'resultsReady':True,'unresolved':[]}
                current['display']['multiSearch'] = {'children':[dict(endpointId=endpoint,phase={'state':'complete'})
                                                                  for endpoint in ['native','pi','dsh']]}
            self.delivery['stages'].append({'stage':stage,'receipt':current})

    def validate(self):
        return module.validate_delivery(self.delivery,self.seeds)

    def test_complete_dtos_satisfy_only_the_declared_non_desktop_scope(self):
        result = self.validate()
        self.assertTrue(result['mountedRevealTargetProven'])
        self.assertFalse(result['desktopVisibilityProven'])
        self.assertFalse(result['sameSessionIdCollisionProven'])

    def test_one_failed_endpoint_cannot_hide_behind_aggregate_loaded(self):
        self.delivery['stages'][0]['receipt']['source']['observer'][1]['errorPresent'] = True
        with self.assertRaisesRegex(AssertionError,'healthy'): self.validate()

    def test_missing_reveal_target_is_a_failure(self):
        self.delivery['stages'][0]['receipt']['display']['visibilityTarget'] = None
        with self.assertRaisesRegex(AssertionError,'target absent'): self.validate()

    def test_getter_id_without_host_mount_is_a_failure(self):
        self.delivery['stages'][1]['receipt']['host']['mountedRows'][0]['presentInContentHost'] = False
        with self.assertRaisesRegex(AssertionError,'node absent'): self.validate()

    def test_focus_transfer_during_page_is_a_failure(self):
        self.delivery['stages'][1]['receipt']['host']['focusedComponentId'] = 901
        with self.assertRaisesRegex(AssertionError,'focus'): self.validate()

    def test_unresolved_display_hits_and_failed_search_are_rejected(self):
        query = self.delivery['stages'][2]['receipt']
        query['host']['appliedSearchProjection']['unresolved'] = [self.seeds[0]]
        with self.assertRaisesRegex(AssertionError,'unresolved'): self.validate()
        query['host']['appliedSearchProjection']['unresolved'] = []
        query['display']['multiSearch']['children'][0]['phase']['state'] = 'failed'
        with self.assertRaisesRegex(AssertionError,'search'): self.validate()

    def test_replacing_an_existing_row_id_during_paging_is_a_failure(self):
        self.delivery['stages'][1]['receipt']['host']['mountedRows'][1]['rowId'] = 999
        with self.assertRaisesRegex(AssertionError,'identity'): self.validate()


class FamilyDeliveryAcceptance(unittest.TestCase):
    """Refusal checks only; synthetic DTOs never establish real harness acceptance."""
    def setUp(self):
        self.seeds = []
        self.delivery = dict(accepted=True, nativeInteraction=False, cases=[])
        for endpoint in ['native', 'pi', 'dsh']:
            for index in range(2):
                parent = dict(endpointId=endpoint, threadId='parent'+str(index), sessionId='parent'+str(index))
                child = 'child'+str(index)
                entry = dict(kind='child', id=child, mode='bounded' if endpoint == 'native' else 'continuable')
                task = dict(row=dict(id='task'+str(index), scope=parent)) if endpoint == 'native' else None
                self.seeds.append(dict(endpoint=endpoint, parent=dict(scope=parent),
                    child=dict(address=dict(endpointId=endpoint,sessionId=child),catalogEntry=entry),task=task))
                row = dict(endpointId=endpoint,parent=parent,childId=child,
                    childScope=dict(endpointId=endpoint,threadId=parent['threadId'],sessionId=child),
                    historyItems=1,readOnly=endpoint == 'native',
                    catalogFrame=dict(parent=parent,epoch=1,generation=1,revision=2,entry=entry),
                    parentTaskRowMountedWithoutOpenAction=endpoint == 'native',
                    canonicalTask=task['row'] if task else None)
                row.update({key:True for key in ['canonicalSourceMatched','openedViaOriginalControl',
                    'returnedViaOriginalTab','closedViaOriginalControl','lateParentResultDidNotStealSelection','lateClosedResultRejected',
                    'parentDraftPreserved','childContentMounted']})
                self.delivery['cases'].append(row)

    def validate(self):
        return module.validate_family_delivery(self.delivery, self.seeds)

    def test_complete_fixture_has_conservative_acceptance_scope(self):
        result = self.validate()
        self.assertFalse(result['nativeInteraction'])
        self.assertFalse(result['sameTaskRowDynamicPointerProven'])

    def test_mismatched_parent_child_or_catalog_is_rejected(self):
        for field in ['parent', 'childScope', 'catalogFrame']:
            saved = copy.deepcopy(self.delivery)
            if field == 'catalogFrame':
                self.delivery['cases'][0][field]['entry'] = dict(id='foreign-child')
            else:
                self.delivery['cases'][0][field] = dict(endpointId='foreign',threadId='foreign',sessionId='foreign')
            with self.assertRaises(AssertionError): self.validate()
            self.delivery = saved

    def test_duplicate_parent_cannot_replace_a_missing_case(self):
        self.delivery['cases'][1] = copy.deepcopy(self.delivery['cases'][0])
        with self.assertRaises(AssertionError): self.validate()

    def test_original_dispatch_draft_and_mount_checks_are_required(self):
        for field in ['lateParentResultDidNotStealSelection','lateClosedResultRejected','parentDraftPreserved','childContentMounted']:
            self.delivery['cases'][0][field] = False
            with self.assertRaisesRegex(AssertionError, 'incomplete'): self.validate()
            self.delivery['cases'][0][field] = True

    def test_empty_history_or_native_composer_is_rejected(self):
        self.delivery['cases'][0]['historyItems'] = 0
        with self.assertRaisesRegex(AssertionError, 'history'): self.validate()
        self.delivery['cases'][0]['historyItems'] = 1
        self.delivery['cases'][0]['readOnly'] = False
        with self.assertRaisesRegex(AssertionError, 'bounded'): self.validate()

    def test_non_native_task_cannot_be_invented(self):
        self.delivery['cases'][2]['canonicalTask'] = dict(id='invented')
        with self.assertRaisesRegex(AssertionError, 'invented'): self.validate()
