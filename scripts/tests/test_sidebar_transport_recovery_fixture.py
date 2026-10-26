"""Focused rejection tests for the independent recovery receipt verifier."""
import copy
import sys
from pathlib import Path
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from sidebar_transport_recovery_fixture import validate_transport_recovery


class RecoveryReceipt(unittest.TestCase):
    def setUp(self):
        self.seeds = [dict(endpointId=endpoint, sessionId=f'{endpoint}-{i}', title=f'{endpoint} title {i}', cwd='/owned')
                      for endpoint in ('native', 'pi', 'dsh') for i in range(3)]
        addresses = [dict(endpointId=s['endpointId'], sessionId=s['sessionId']) for s in self.seeds]
        self.receipt = dict(schemaVersion=1, status='completed',
            evidenceScope='real-client-transport-close-runtime-reconnect', desktopInteraction=False,
            endpoints=['native', 'pi', 'dsh'], bootstrap=dict(pageActions=1, mountedCount=9), cases=[])
        for failed in ('native', 'pi', 'dsh'):
            stages = {}
            for stage_name, base_rev, fault_phase in [('before', 1, 'ready'), ('failed', 2, 'failed'), ('recovered', 3, 'ready')]:
                observer = [dict(endpointId=ep, generation=7, revision=base_rev, stale=False, loaded=True,errorPresent=False,
                                 observation=dict(phase='observing', generation=7,converged=True)) for ep in ('native', 'pi', 'dsh')]
                rows = [dict(address=a, title=s['title'], folderPath=s['cwd'],
                             activity=dict(isStale=(stage_name == 'failed' and s['endpointId'] == failed)),
                             actions=dict(items=[dict(id='copy-id', interaction='copy', enabled=True,
                                                      copyText='copy me')]))
                        for a, s in zip(addresses, self.seeds)]
                runtime = {ep:dict(connectionPhase=('failed' if ep == failed and stage_name == 'failed' else 'ready'),
                                   connectionEpoch=(1 if ep != failed or stage_name == 'before' else 2 if stage_name == 'failed' else 3),
                                   revision=base_rev, inventoryReady=not (ep == failed and stage_name == 'failed'),
                                   selectedJournalReady=not (ep == failed and stage_name == 'failed')) for ep in ('native','pi','dsh')}
                delivery = dict(schemaVersion=1, source=dict(revision=base_rev, rows=rows, observer=observer),
                                display=dict(revision=base_rev, rows=copy.deepcopy(rows), selected=dict(endpointId=failed,sessionId=f'{failed}-0')))
                stages[stage_name] = dict(delivery=delivery, runtime=runtime,
                    sourceActions=[dict(address=row['address'],actions=copy.deepcopy(row['actions'])) for row in rows],
                    mounts=[dict(address=a, rowId=100+i, mounted=True) for i,a in enumerate(addresses)],
                    gate=(dict(attempts=1,successes=1,closeCount=0,rejections=0,blocked=False,connectionGeneration=1,activeEndpoint=True)
                          if stage_name == 'before' else dict(attempts=5,successes=1,closeCount=1,rejections=4,blocked=True,connectionGeneration=1,activeEndpoint=False)
                          if stage_name == 'failed' else dict(attempts=6,successes=2,closeCount=1,rejections=4,blocked=False,connectionGeneration=2,activeEndpoint=True)),
                    selectedScope=dict(endpointId=failed, sessionId=f'{failed}-0'),
                    drafts=[dict(scope=dict(endpointId=ep,sessionId=f'{ep}-{i}'), text=f'中文草稿 {ep} {i}') for ep in ('native','pi','dsh') for i in range(2)],
                    observerRetryEndpoints=[])
            healthy = [ep for ep in ('native','pi','dsh') if ep != failed]
            self.receipt['cases'].append(dict(endpointId=failed, address=dict(endpointId=failed,sessionId=f'{failed}-0'),
                **stages,
                healthyRefresh={ep:dict(epochBefore=1,epochAfter=1,revisionBefore=1,revisionAfter=2) for ep in healthy},
                reconnect=dict(commandCount=1,endpointId=failed),
                selectedReads=dict(selectionGeneration=1,selectionGenerationOwner='probe-selected-read-scope',faultAcceptedAfterRecovery=False,healthyAcceptedAfterRecovery={ep:True for ep in healthy}),
                capturedDefinitionCopy=dict(definitionId='copy-id', address=dict(endpointId=failed,sessionId=f'{failed}-0'), rejectedWhileFailed=True,
                    expectedText='copy me',actualText='copy me',executionsAfterRecovery=1)))
        self.requests = [dict(path='/v1/chat/completions', count=3)]

    def validate(self):
        return validate_transport_recovery(self.receipt, self.seeds, ['native','pi','dsh'], self.requests, copy.deepcopy(self.requests))

    def test_complete_real_shape_accepts_with_declared_scope(self):
        self.assertTrue(self.validate()['accepted'])
        self.receipt['cases'][0]['address'] = dict(endpoint_id='native',session_id='native-0')
        self.assertTrue(self.validate()['accepted'])
        self.receipt['cases'][0]['address']['endpointId'] = 'pi'
        with self.assertRaisesRegex(AssertionError, 'mixed key'):
            self.validate()

    def test_missing_stage_or_changed_source_is_rejected(self):
        del self.receipt['cases'][0]['failed']
        with self.assertRaisesRegex(AssertionError, 'missing recovery stage'):
            self.validate()
        self.setUp()
        self.receipt['cases'][1]['recovered']['delivery']['source']['rows'][0]['title'] = 'foreign title'
        with self.assertRaisesRegex(AssertionError, 'source title/cwd'):
            self.validate()

    def test_unmounted_or_stale_observer_retry_is_rejected(self):
        self.receipt['cases'][0]['before']['mounts'][0]['mounted'] = False
        with self.assertRaisesRegex(AssertionError, 'mounts'):
            self.validate()
        self.setUp()
        self.receipt['cases'][0]['recovered']['observerRetryEndpoints'] = ['native']
        with self.assertRaisesRegex(AssertionError, 'Retry'):
            self.validate()

    def test_provider_request_delta_and_wrong_command_are_rejected(self):
        self.receipt['cases'][0]['reconnect']['commandCount'] = 2
        with self.assertRaisesRegex(AssertionError, 'Reconnect'):
            self.validate()
        self.setUp()
        after = copy.deepcopy(self.requests) + [dict(path='/v1/chat/completions', count=1)]
        with self.assertRaisesRegex(AssertionError, 'provider request'):
            validate_transport_recovery(self.receipt, self.seeds, ['native','pi','dsh'], self.requests, after)

    def test_duplicate_rows_observers_wrong_selection_and_noncanonical_stale_are_rejected(self):
        self.receipt['cases'][0]['before']['delivery']['source']['rows'].append(
            copy.deepcopy(self.receipt['cases'][0]['before']['delivery']['source']['rows'][0]))
        with self.assertRaisesRegex(AssertionError, 'duplicate source'):
            self.validate()
        self.setUp()
        self.receipt['cases'][0]['before']['delivery']['display']['rows'].append(
            copy.deepcopy(self.receipt['cases'][0]['before']['delivery']['display']['rows'][0]))
        with self.assertRaisesRegex(AssertionError, 'duplicate display'):
            self.validate()
        self.setUp()
        obs = self.receipt['cases'][0]['before']['delivery']['source']['observer']
        obs[1]['endpointId'] = obs[0]['endpointId']
        with self.assertRaisesRegex(AssertionError, 'duplicate or missing observer'):
            self.validate()
        self.setUp()
        self.receipt['cases'][0]['before']['selectedScope'] = dict(endpointId='native', sessionId='native-1')
        with self.assertRaisesRegex(AssertionError, 'fault target'):
            self.validate()
        self.setUp()
        row = self.receipt['cases'][0]['failed']['delivery']['source']['rows'][0]
        row['activity'] = dict(stale=True)
        with self.assertRaisesRegex(AssertionError, 'isStale missing'):
            self.validate()

    def test_readiness_epoch_gate_drafts_and_copy_bind_to_independent_facts(self):
        self.receipt['cases'][0]['failed']['delivery']['source']['observer'][0]['stale'] = True
        with self.assertRaisesRegex(AssertionError, 'memory observer'):
            self.validate()
        self.setUp()
        self.receipt['cases'][0]['before']['runtime']['native']['inventoryReady'] = False
        with self.assertRaisesRegex(AssertionError, 'inventory readiness'):
            self.validate()
        self.setUp()
        self.receipt['cases'][0]['failed']['runtime']['native']['selectedJournalReady'] = True
        with self.assertRaisesRegex(AssertionError, 'journal readiness'):
            self.validate()
        self.setUp()
        self.receipt['cases'][0]['recovered']['runtime']['native']['connectionEpoch'] = 2
        with self.assertRaisesRegex(AssertionError, 'epoch did not advance'):
            self.validate()
        self.setUp()
        self.receipt['cases'][0]['failed']['gate']['connectionGeneration'] = 2
        with self.assertRaisesRegex(AssertionError, 'generation differs'):
            self.validate()
        self.setUp()
        for stage in ('before', 'failed', 'recovered'):
            self.receipt['cases'][1][stage]['drafts'][0]['text'] = '中文草稿 changed'
        with self.assertRaisesRegex(AssertionError, 'between endpoint fault cases'):
            self.validate()
        self.setUp()
        self.receipt['cases'][0]['capturedDefinitionCopy']['definitionId'] = 'forged'
        with self.assertRaisesRegex(AssertionError, 'canonical action'):
            self.validate()
        self.setUp()
        self.receipt['cases'][0]['capturedDefinitionCopy']['expectedText'] = 'self-reported forged value'
        self.receipt['cases'][0]['capturedDefinitionCopy']['actualText'] = 'self-reported forged value'
        with self.assertRaisesRegex(AssertionError, 'canonical action'):
            self.validate()


if __name__ == '__main__':
    unittest.main()
