"""Offline checks for diagnostic process identity receipts."""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

SCRIPT_DIR=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(SCRIPT_DIR))
SPEC=importlib.util.spec_from_file_location('stage_perf_case',SCRIPT_DIR/'test-stage-performance-case.py')
MODULE=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def warm_input(endpoint='pi'):
    boundaries=[]
    roots=[{'pid':101,'startIdentity':'root-start','role':'harness'}] if endpoint!='pi' else []
    persistent=([{'pid':201,'startIdentity':'node-a','role':'harness','commandName':'node'},
                 {'pid':202,'startIdentity':'node-b','role':'harness','commandName':'node'}]
                if endpoint=='pi' else [{'pid':101,'startIdentity':'root-start',
                                         'role':'harness','commandName':'supervisor'}])
    boundaries.append({'event':'runtime_ready','monotonicNs':900_000_000,
        'osProcesses':[dict(row) for row in persistent],'error':None})
    for iteration in range(11):
        boundaries.append({'event':'iteration_started','iteration':iteration,
            'monotonicNs':1_000_000_000+iteration*1_000_000_000,
            'osProcesses':[dict(row) for row in persistent],'error':None})
        for session in (0,1):
            boundaries.append({'event':'send','round':iteration,'sessionIndex':session,
                'monotonicNs':1_500_000_000+iteration*1_000_000_000+session*100_000_000,
                'osProcesses':[dict(row) for row in persistent],'error':None})
    end=boundaries[-1]['monotonicNs']
    rss=[]
    for stamp in range(1_000_000_000,end+1,50_000_000):
        rss.append({'monotonicNs':stamp,'processes':[
            dict(row,rssKiB=512,command='/usr/local/bin/node' if row['commandName']=='node'
                 else '/tmp/tekes-supervisor') for row in persistent]})
    return boundaries,rss,{'maxObservedIntervalNs':50_000_000},roots


class WarmIdentityTests(unittest.TestCase):
    def test_pi_requires_two_stable_rpc_identities_across_all_warm_rounds(self):
        data=warm_input('pi')
        proof=MODULE.validate_warm_process_identity('pi','warm',*data)
        self.assertEqual(proof['status'],'verified')
        self.assertEqual(len(proof['expectedLongLivedHarnessIdentities']),2)

    def test_external_harness_requires_same_root_pid_and_start(self):
        data=warm_input('native')
        proof=MODULE.validate_warm_process_identity('native','warm',*data)
        self.assertEqual(proof['status'],'verified')
        next(row for row in data[0] if row.get('event')=='iteration_started'
             and row.get('iteration')==2)['osProcesses'][0]['pid']=999
        with self.assertRaisesRegex(ValueError,'identity changed'):
            MODULE.validate_warm_process_identity('native','warm',*data)

    def test_short_lived_node_probe_does_not_count_as_a_persistent_rpc(self):
        data=warm_input('pi')
        data[0][2]['osProcesses'].append({'pid':299,'startIdentity':'probe',
            'role':'harness','commandName':'node'})
        proof=MODULE.validate_warm_process_identity('pi','warm',*data)
        self.assertEqual(len(proof['expectedLongLivedHarnessIdentities']),2)

    def test_pi_rpc_replacement_across_rounds_fails(self):
        data=warm_input('pi')
        data[0][4]['osProcesses'][0]['pid']=999
        data[0][4]['osProcesses'][0]['startIdentity']='replacement'
        with self.assertRaisesRegex(ValueError,'exactly two persistent'):
            MODULE.validate_warm_process_identity('pi','warm',*data)

    def test_warm_missing_iteration_or_rss_observation_fails_closed(self):
        data=warm_input('pi')
        incomplete=[row for row in data[0]
                    if not (row.get('event')=='iteration_started' and row.get('iteration')==8)]
        with self.assertRaisesRegex(ValueError,'iteration boundaries'):
            MODULE.validate_warm_process_identity('pi','warm',incomplete,*data[1:])
        data=list(warm_input('pi'));data[1]=[]
        with self.assertRaisesRegex(ValueError,'no samples'):
            MODULE.validate_warm_process_identity('pi','warm',*data)

    def test_non_warm_does_not_claim_steady_pid_continuity(self):
        proof=MODULE.validate_warm_process_identity('pi','reconnectLifecycle',[],[],{},[])
        self.assertEqual(proof['status'],'not_applicable')


class RetirementTests(unittest.TestCase):
    def test_owned_processes_must_retire_by_cleanup_deadline(self):
        class Sampler:
            pi_embedded=False
            roots={}
            known={(401,'old'): 'agentkit'}
            import threading
            lock=threading.Lock()
        sampler=Sampler()
        boundaries=[{'event':'cleanup_intent','monotonicNs':100,'originalIntentMonotonicNs':100,
                     'osProcesses':[],'error':None},
            {'event':'runtime_cleanup_completed','monotonicNs':150,'osProcesses':[],'error':None}]
        exit_receipt={'exitStatus':0,'ownedGroupAbsent':True,'spawnMonotonicNsLowerBound':0}
        with patch.object(MODULE.time,'monotonic_ns',return_value=200):
            proof=MODULE.verify_owned_retirement(sampler,exit_receipt,boundaries,1000,[])
        self.assertEqual(proof['status'],'verified')
        self.assertEqual(proof['cleanupElapsedNs'],100)
        self.assertTrue(proof['cleanupDurationIsConservativeUpperBound'])
        sampler.known={(402,'still-running'):'harness'}
        with patch.object(MODULE,'process_table',return_value={402:{'pid':402,'ppid':0,'rssKiB':1,
            'startIdentity':'still-running','command':'/tmp/worker'}}):
            with self.assertRaisesRegex(ValueError,'processes remain'):
                MODULE.verify_owned_retirement(sampler,exit_receipt,boundaries,1000,[])

    def test_delayed_stdout_observation_cannot_shorten_original_cleanup_budget(self):
        class Sampler:
            pi_embedded=False
            roots={}
            known={}
            import threading
            lock=threading.Lock()
        boundaries=[{'event':'cleanup_intent','monotonicNs':9_500_000_000,
                     'originalIntentMonotonicNs':100,'error':None},
                    {'event':'runtime_cleanup_completed','monotonicNs':9_900_000_000,'error':None}]
        with patch.object(MODULE,'process_table',return_value={}), \
                patch.object(MODULE.time,'monotonic_ns',return_value=10_100_000_000):
            with self.assertRaisesRegex(ValueError,'cleanup budget'):
                MODULE.verify_owned_retirement(Sampler(),{'exitStatus':0,'ownedGroupAbsent':True,
                    'spawnMonotonicNsLowerBound':0},boundaries,10_000,[])

    def test_external_root_exit_code_is_bound(self):
        class Sampler:
            pi_embedded=False
            roots={}
            known={}
            import threading
            lock=threading.Lock()
        with patch.object(MODULE,'process_table',return_value={}):
            with self.assertRaisesRegex(ValueError,'external harness root'):
                MODULE.verify_owned_retirement(Sampler(),{'exitStatus':0,'ownedGroupAbsent':True},
                    [{'event':'cleanup_intent','monotonicNs':1,'error':None},
                     {'event':'runtime_cleanup_completed','monotonicNs':2,'error':None}],1000,
                    [{'operation':'measurement_retire','exitStatus':1}])


if __name__=='__main__':
    unittest.main()
