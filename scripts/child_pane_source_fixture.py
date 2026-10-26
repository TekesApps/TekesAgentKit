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


def validate_pane_recovery_frame(frame, endpoint, parent, child_id):
    """Validate real child RunPort paging and reconnect evidence from the demo host."""
    expected_scope = {'endpointId': endpoint, 'threadId': parent.get('threadId'),
                      'sessionId': child_id}
    if endpoint not in ('pi', 'dsh') or not isinstance(frame, dict) or frame.get('accepted') is not True:
        raise ValueError('independent child recovery acceptance missing')
    if frame.get('evidenceScope') != 'original-independent-child-RunPort-paging-and-host-reconnect':
        raise ValueError('recovery was not captured from the original mounted child RunPort')
    page = frame.get('pagination')
    if (not isinstance(page, dict) or page.get('originalControlPresent') is not True
            or page.get('originalControlEnabled') is not True
            or page.get('supportedByMountedBinding') is not True
            or page.get('requestedScope') != expected_scope
            or type(page.get('connectionEpoch')) is not int or page['connectionEpoch'] < 1
            or type(page.get('beforeRevision')) is not int
            or type(page.get('afterRevision')) is not int or page['afterRevision'] <= page['beforeRevision']
            or type(page.get('beforeTranscriptItems')) is not int or page['beforeTranscriptItems'] < 1
            or type(page.get('afterTranscriptItems')) is not int
            or page['afterTranscriptItems'] <= page['beforeTranscriptItems']
            or page.get('earlierPageLoaded') is not True):
        raise ValueError('original child earlier-page action did not extend this child transcript')
    reconnect = frame.get('reconnect')
    if (not isinstance(reconnect, dict)
            or reconnect.get('dispatchedThroughDemoHost') is not True
            or reconnect.get('sameParent') is not True
            or reconnect.get('sameChildPointer') is not True
            or reconnect.get('sameMountedSource') is not True
            or reconnect.get('paneRemainedSelected') is not True
            or type(reconnect.get('epochBefore')) is not int
            or reconnect['epochBefore'] != page['connectionEpoch']
            or type(reconnect.get('epochAfter')) is not int
            or reconnect['epochAfter'] <= reconnect['epochBefore']
            or reconnect.get('sourceReady') is not True
            or reconnect.get('projectionSucceeded') is not True
            or reconnect.get('sourceContainsOriginalChild') is not True
            or type(reconnect.get('transcriptItemsAfterReconnect')) is not int
            or reconnect['transcriptItemsAfterReconnect'] < page['afterTranscriptItems']):
        raise ValueError('host reconnect did not restore the same child source and transcript')
    if frame.get('parentDraftPreserved') is not True:
        raise ValueError('parent draft changed during child paging/reconnect')
    if frame.get('nativeInteraction') is not False or frame.get('physicalIme') is not False:
        raise ValueError('offscreen recovery proof claims desktop acceptance')
    records = frame.get('historyRecords')
    if not isinstance(records, dict):
        raise ValueError('original history record identities missing')
    before, after, restored = (records.get(key) for key in ('before', 'afterPage', 'afterReconnect'))
    for rows in (before, after, restored):
        if (not isinstance(rows, list) or not rows
                or any(not isinstance(row, dict) or not isinstance(row.get('id'), str)
                       or not row['id'] or not isinstance(row.get('text'), str)
                       or not isinstance(row.get('sourceRecordPosition'), dict)
                       or not row['sourceRecordPosition'] for row in rows)
                or len({row['id'] for row in rows}) != len(rows)):
            raise ValueError('original history record identity invalid or duplicated')
    after_positions = {row['id']: index for index, row in enumerate(after)}
    positions = [after_positions.get(row['id'], -1) for row in before]
    if (len(after) <= len(before) or positions[0] <= 0
            or any(a >= b for a, b in zip(positions, positions[1:]))
            or any(index < 0 or after[index] != row for index, row in zip(positions, before))
            or restored != after
            or len(before) > page['beforeTranscriptItems']
            or len(after) > page['afterTranscriptItems']
            or len(restored) > reconnect['transcriptItemsAfterReconnect']):
        raise ValueError('paging/reconnect changed original history record content or order')
    return {'accepted': True, 'endpoint': endpoint, 'parent': parent,
            'childScope': expected_scope, 'pageConnectionEpoch': page['connectionEpoch'],
            'reconnectedEpoch': reconnect['epochAfter'],
            'beforeTranscriptItems': page['beforeTranscriptItems'],
            'afterTranscriptItems': page['afterTranscriptItems'],
            'transcriptItemsAfterReconnect': reconnect['transcriptItemsAfterReconnect'],
            'originalRecordsPrepended': True, 'originalRecordOrderRestored': True,
            'noDuplicateHistoryRecords': True,
            'parentDraftPreserved': True, 'nativeInteraction': False, 'physicalIme': False}
