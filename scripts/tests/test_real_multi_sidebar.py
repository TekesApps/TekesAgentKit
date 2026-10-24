"""Negative acceptance checks; synthetic DTOs do not prove real harness delivery."""
import copy
import importlib.util
from pathlib import Path
import sys
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
spec = importlib.util.spec_from_file_location('real_multi_sidebar', Path(__file__).resolve().parents[1]/'test-real-multi-sidebar.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


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
