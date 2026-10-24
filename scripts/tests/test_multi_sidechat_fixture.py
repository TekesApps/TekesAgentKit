"""Synthetic refusal cases only; no harness, UI or native acceptance is implied."""
from pathlib import Path
import sys
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from multi_sidechat_fixture import validate_multi_sidechat_delivery
from owned_endpoint_provider import provider_handler


def scope(kind, session):
    return dict(endpointId=kind,threadId=session,sessionId=session)


def synthetic():
    seeds=[dict(endpointId=kind,sessionId=kind+str(i)) for kind in ['native','pi','dsh'] for i in range(3)]
    cases=[]
    for kind in ['native','pi','dsh']:
        row=dict(endpointId=kind,parentScope=scope(kind,kind+'0'),scratchScope=None,
                 capabilityUnavailable=kind=='dsh',parentAndOtherDraftsPreserved=True)
        if kind=='dsh':row.update(originalCreateDisabled=True,noForkAttempted=True)
        else:
            row.update(scratchScope=scope(kind,kind+'-scratch'),scratchInputCount=1,cancelInputCount=1,
                       oldEpoch=2,newEpoch=3,cancelTurnStatus='aborted',uniqueInput='owned-'+kind,
                       explicitSdkEditorFocus=True,automaticTabRefocusProven=False)
            for flag in ['originalCreate','originalHostDispatch','originalSend','runtimeCancelObserved',
                         'parentHistoryIsolated','summaryNavigation','originalComposerIdsRetained',
                         'scratchDraftRestored','canonicalReopen','freshReconnect','noDuplicateInput',
                         'originalClose','confirmedDiscard','retiredAuthority',
                         'lateCreateDeliveryDidNotStealSelection','lateSendDeliveryDidNotStealSelection',
                         'sendCompletedWhileOtherEndpointSelected','lateCloseDeliveryDidNotAffectCurrent']:
                row[flag]=True
        cases.append(row)
    return dict(accepted=True,endpointOrder=['native','pi','dsh'],hostFirstEndpoint='native',cases=cases,
                nativeInteraction=False,physicalIme=False,desktopVisibilityProven=False,crossProcess=False),seeds


class MultiSideChatReceipt(unittest.TestCase):
    def test_owned_cancel_window_is_opt_in_positive_and_bounded(self):
        for value in [0,-1,31,True,"3",float("inf"),float("nan")]:
            with self.assertRaises(ValueError):
                provider_handler(workspace=lambda:Path("/synthetic"),requests=[],cancel_hold_seconds=value)
        for value in [None,3]:
            self.assertIsInstance(provider_handler(workspace=lambda:Path("/synthetic"),requests=[],cancel_hold_seconds=value),type)

    def test_synthetic_baseline_only_validates_the_declared_receipt_scope(self):
        delivery,seeds=synthetic();result=validate_multi_sidechat_delivery(delivery,seeds)
        self.assertTrue(result['nonFirstPiProven']);self.assertFalse(result['physicalIme'])
        self.assertFalse(result['desktopVisibilityProven'])

    def test_reordered_reduced_or_first_pi_domain_is_refused(self):
        for mutate in [lambda d:d.update(endpointOrder=['pi','native','dsh']),
                       lambda d:d.update(hostFirstEndpoint='pi'),
                       lambda d:d['cases'].pop()]:
            d,s=synthetic();mutate(d)
            with self.assertRaises(AssertionError):validate_multi_sidechat_delivery(d,s)

    def test_scope_and_unconfigured_parent_cannot_be_substituted(self):
        for replacement in [scope('native','pi0'),scope('pi','pi2')]:
            d,s=synthetic();d['cases'][1]['parentScope']=replacement
            with self.assertRaises(AssertionError):validate_multi_sidechat_delivery(d,s)
        d,s=synthetic();d['cases'][1]['scratchScope']=scope('pi','pi0')
        with self.assertRaises(AssertionError):validate_multi_sidechat_delivery(d,s)

    def test_idle_completed_or_prepared_ticket_cancel_is_insufficient(self):
        for status in ['idle','completed','running',None]:
            d,s=synthetic();d['cases'][1]['cancelTurnStatus']=status
            with self.assertRaises(AssertionError):validate_multi_sidechat_delivery(d,s)
        d,s=synthetic();d['cases'][1]['runtimeCancelObserved']=False
        with self.assertRaises(AssertionError):validate_multi_sidechat_delivery(d,s)

    def test_late_delivery_input_and_epoch_proofs_are_required(self):
        for field,value in [('lateCloseDeliveryDidNotAffectCurrent',False),('explicitSdkEditorFocus',False),
                            ('automaticTabRefocusProven',True),('sendCompletedWhileOtherEndpointSelected',False),('lateCreateDeliveryDidNotStealSelection',False),('lateSendDeliveryDidNotStealSelection',False),
                            ('scratchInputCount',2),('cancelInputCount',True),('newEpoch',2),('confirmedDiscard',False)]:
            d,s=synthetic();d['cases'][1][field]=value
            with self.assertRaises(AssertionError):validate_multi_sidechat_delivery(d,s)

    def test_unsupported_dsh_and_non_desktop_evidence_remain_distinct(self):
        for mutate in [lambda d:d['cases'][2].update(noForkAttempted=False),
                       lambda d:d['cases'][2].update(scratchScope=scope('dsh','scratch')),
                       lambda d:d.update(physicalIme=True),lambda d:d.update(crossProcess=True)]:
            d,s=synthetic();mutate(d)
            with self.assertRaises(AssertionError):validate_multi_sidechat_delivery(d,s)


if __name__=='__main__':unittest.main()
