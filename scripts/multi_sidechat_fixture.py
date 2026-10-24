"""Conservative receipt checks for the original owned multi-endpoint SideChat host.

This module grants no session/pane authority and performs no IO. Synthetic tests
of these checks are not evidence that a harness or UI lifecycle has run.
"""


def _scope(value, endpoint):
    assert isinstance(value, dict) and set(value) == {'endpointId', 'threadId', 'sessionId'}, 'full original RunScope required'
    assert value['endpointId'] == endpoint, 'scope escaped original endpoint'
    assert isinstance(value['sessionId'], str) and value['sessionId'].strip()
    assert value['threadId'] == value['sessionId'], 'SideChat main receiving session scope changed'
    return value['sessionId']


def validate_multi_sidechat_delivery(delivery, seeds):
    assert delivery['accepted'] is True
    assert delivery['endpointOrder'] == ['native', 'pi', 'dsh'], 'actual endpoint order changed'
    assert delivery['hostFirstEndpoint'] == 'native', 'Pi must remain the non-first endpoint'
    for field in ['nativeInteraction', 'physicalIme', 'desktopVisibilityProven', 'crossProcess']:
        assert delivery[field] is False, 'offscreen evidence cannot claim '+field
    cases = delivery['cases']
    assert len(cases) == 3 and [row['endpointId'] for row in cases] == ['native', 'pi', 'dsh']
    grouped = {kind: [row['sessionId'] for row in seeds if row['endpointId'] == kind]
               for kind in ['native', 'pi', 'dsh']}
    assert all(len(rows) == 3 and len(set(rows)) == 3 for rows in grouped.values())
    verified = []
    inputs = set()
    for row in cases:
        kind = row['endpointId']
        parent = _scope(row['parentScope'], kind)
        assert parent in grouped[kind][:2], 'parent was not one of the actual configured Runs'
        if kind == 'dsh':
            assert row['capabilityUnavailable'] is True and row['scratchScope'] is None
            assert row['originalCreateDisabled'] is True and row['noForkAttempted'] is True
            assert row['parentAndOtherDraftsPreserved'] is True
            verified.append(dict(endpointId=kind, capabilityUnavailable=True))
            continue
        assert row['capabilityUnavailable'] is False
        assert row['explicitSdkEditorFocus'] is True and row['automaticTabRefocusProven'] is False
        scratch = _scope(row['scratchScope'], kind)
        assert scratch != parent and scratch not in grouped[kind], 'scratch reused a configured parent'
        for field in ['originalCreate', 'originalHostDispatch', 'originalSend', 'runtimeCancelObserved',
                      'parentHistoryIsolated', 'parentAndOtherDraftsPreserved', 'summaryNavigation',
                      'originalComposerIdsRetained', 'scratchDraftRestored', 'canonicalReopen',
                      'freshReconnect', 'noDuplicateInput', 'originalClose', 'confirmedDiscard', 'retiredAuthority']:
            assert row[field] is True, 'missing original lifecycle proof: '+field
        assert row['cancelTurnStatus'] in ['aborted', 'cancelled', 'interrupted'], 'Idle/completed alone is not runtime cancellation'
        assert type(row['scratchInputCount']) is int and row['scratchInputCount'] == 1
        assert type(row['cancelInputCount']) is int and row['cancelInputCount'] == 1
        assert type(row['oldEpoch']) is int and type(row['newEpoch']) is int and 0 < row['oldEpoch'] < row['newEpoch']
        assert isinstance(row['uniqueInput'], str) and row['uniqueInput'].strip() and row['uniqueInput'] not in inputs
        inputs.add(row['uniqueInput'])
        if kind == 'pi':
            assert row['lateCreateDeliveryDidNotStealSelection'] is True
            assert row['lateSendDeliveryDidNotStealSelection'] is True
            assert row['sendCompletedWhileOtherEndpointSelected'] is True
            assert row['lateCloseDeliveryDidNotAffectCurrent'] is True
        verified.append(dict(endpointId=kind,parentScope=row['parentScope'],scratchScope=row['scratchScope'],
                             scratchInputCount=row['scratchInputCount'],cancelInputCount=row['cancelInputCount'],
                             cancelTurnStatus=row['cancelTurnStatus'],oldEpoch=row['oldEpoch'],newEpoch=row['newEpoch']))
    return dict(accepted=True,evidenceScope='real-three-endpoint-original-sidechat-dispatch-and-lifecycle',
                endpointOrder=delivery['endpointOrder'],cases=verified,nonFirstPiProven=True,
                nativeInteraction=False,physicalIme=False,desktopVisibilityProven=False,crossProcess=False)
