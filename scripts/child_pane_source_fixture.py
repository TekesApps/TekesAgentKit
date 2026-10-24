"""Validate original independent-pane sources without projecting another Run."""
from transcript_source_fixture import validate_transcript_pair, validate_transcript_source

PREFIX = 'AGENTKIT_CHILD_PANE_SOURCE '


def validate_pane_frame(frame, *, endpoint, parent, child_id, marker, expected_phase,
                        expected_epoch=None):
    if (not isinstance(frame, dict) or frame.get('phase') != expected_phase
            or frame.get('parent') != parent or frame.get('childId') != child_id
            or frame.get('marker') != marker):
        raise ValueError('pane source frame changed phase, parent, child or input')
    if (endpoint not in ('pi', 'dsh') or parent.get('endpointId') != endpoint
            or not isinstance(parent.get('threadId'), str) or not parent['threadId']
            or not isinstance(parent.get('sessionId'), str) or not parent['sessionId']
            or not isinstance(child_id, str) or not child_id
            or child_id in (parent['threadId'], parent['sessionId'])):
        raise ValueError('independent child pane identity is invalid')
    witness = frame.get('witness')
    if not isinstance(witness, dict):
        raise ValueError('original pane source witness is absent')
    epoch = witness.get('snapshot', {}).get('connectionEpoch')
    if type(epoch) is not int or epoch <= 0 or (expected_epoch is not None and epoch != expected_epoch):
        raise ValueError('pane source connection epoch changed')
    scope = {'endpointId': endpoint, 'threadId': parent['threadId'], 'sessionId': child_id}
    proof = validate_transcript_source(
        witness, endpoint=endpoint, child_id=child_id, marker=marker,
        phase=expected_phase, expected_epoch=epoch, expected_scope=scope)
    return dict(proof, paneEvidenceScope='independent-child-pane', directParent=dict(parent))


def validate_pane_receipt(receipt, running, failed):
    pair = validate_transcript_pair(running, failed)
    if running.get('directParent') != failed.get('directParent'):
        raise ValueError('pane transition changed its direct parent')
    if (not isinstance(receipt, dict) or receipt.get('accepted') is not True
            or receipt.get('evidenceScope') != 'actual-child-pane-original-controls-and-source'
            or receipt.get('parent') != running['directParent']
            or receipt.get('childId') != running['childSessionId']
            or receipt.get('childScope') != running['scope']
            or receipt.get('connectionEpoch') != running['connectionEpoch']
            or receipt.get('runningRevision') != running['revision']
            or receipt.get('failedRevision') != failed['revision']
            or receipt.get('nativeInteraction') is not False
            or receipt.get('physicalIme') is not False):
        raise ValueError('pane host receipt does not pair with its original publications')
    for key in ('openedViaOriginalControl', 'sentViaOriginalComposer', 'closedViaOriginalControl',
                'parentDraftPreserved', 'parentSelectionPreserved', 'retiredWitnessRejected'):
        if receipt.get(key) is not True:
            raise ValueError('pane host lifecycle incomplete: ' + key)
    return {'accepted': True, 'evidenceScope': 'real-independent-child-pane-original-source',
            'parent': running['directParent'], 'childScope': running['scope'],
            'connectionEpoch': running['connectionEpoch'], 'pair': pair,
            'nativeInteraction': False, 'physicalIme': False,
            'serverTransactionalFreshnessProven': False}
