"""Identity checks for real goal projection -> original offscreen Kit footer.

These checks grant no goal/RPC authority and never fabricate a projection.
The caller owns its isolated sessions and executes the public lifecycle methods.
"""


def verify_goal_identity(receipt, scope, goal):
    assert receipt.get('offscreen') is True and receipt.get('components') is True
    assert receipt.get('nativeInteraction') is False
    assert receipt.get('isStale') is False
    proof = receipt.get('goalFootnoteProof')
    assert isinstance(proof, dict), 'public goal did not reach the original footer'
    assert proof['scope'] == scope, 'goal footer crossed its original Run scope'
    assert (proof['goalId'], proof['goalRevision'], proof['phase']) == (
        goal['id'], goal['revision'], goal['phase']), 'goal identity/revision/phase changed'
    assert goal['phase'] in ('paused', 'blocked', 'complete')
    assert isinstance(proof['turnId'], str) and proof['turnId']
    assert proof['label'] == '目标 ' + goal['phase']
    assert all(proof[key] is True for key in (
        'originalGoalProjection', 'originalFooter', 'externalEndpoint')), proof
    assert isinstance(proof['labelVisible'], bool)
    assert all(proof[key] is False for key in (
        'sourceMarkerInferred', 'durationInferred', 'nativeInteraction'))
    return proof


def verify_goal_footer(receipt, scope, goal):
    proof = verify_goal_identity(receipt, scope, goal)
    assert proof['labelVisible'] is True, proof
    return proof


def verify_goal_cleared(before, after):
    assert before.get('goalFootnoteProof') is not None
    assert after.get('offscreen') is True and after.get('components') is True
    assert after.get('nativeInteraction') is False
    assert after.get('isStale') is False
    assert 'goalFootnoteProof' in after, 'old consumer without a goal proof is not clearance evidence'
    assert after.get('goalFootnoteProof') is None, 'cleared goal reappeared in a fresh consumer'
    for key in ('transcriptItems', 'finalAssistantItems'):
        assert after[key] == before[key], 'goal clear changed the visible conversation'
    for key in ('sections', 'visibleMessages'):
        assert after['documentProjection'][key] == before['documentProjection'][key]
    for key in ('rows', 'messages', 'footnotes'):
        assert after['transcriptContentProof'][key] == before['transcriptContentProof'][key]
    return {'clearedGoalAbsent': True, 'freshConsumerAfterReconnect': True,
            'visibleConversationRetained': True, 'nativeInteraction': False}
