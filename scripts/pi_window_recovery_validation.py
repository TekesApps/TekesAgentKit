"""Original public Pi snapshot proof for #68, without storage access."""


def validate_recovery_snapshots(snapshots, session_ids):
    if len(snapshots) != 6 * len(session_ids):
        raise ValueError('all six original snapshot phases per session are required')
    proofs = []
    phases = ('initial50', 'reconnected50', 'reopened50', 'paged100',
              'reconnected100', 'reopened100')
    for index, session_id in enumerate(session_ids):
        original50 = original100 = None
        phase_proofs = []
        for phase, snapshot in zip(phases, snapshots[index*6:(index+1)*6]):
            session = snapshot.get('sessions', {}).get(session_id, {})
            source = session.get('native_conversation')
            if (not isinstance(source, dict) or source.get('sessionId') != session_id
                    or session.get('journal_ready') is not True
                    or snapshot.get('connection') != 'Ready'
                    or snapshot.get('endpoint_id') != 'pi'
                    or snapshot.get('inventory_ready') is not True):
                raise ValueError('original Pi session projection is missing or not usable')
            entries = source.get('entries')
            if not isinstance(entries, list):
                raise ValueError('original native entries missing')
            ids = [entry.get('id') for entry in entries]
            if any(not isinstance(value, str) or not value for value in ids) or len(set(ids)) != len(ids):
                raise ValueError('original native record identities missing or duplicated')
            messages = [entry for entry in entries if entry.get('type') == 'message']
            expected = 50 if phase.endswith('50') else 100
            if len(messages) != expected or source.get('hasMoreBefore') is not True:
                raise ValueError(f'{phase} changed the requested message window or exhausted history')
            before = original50 if expected == 50 else original100
            if phase == 'paged100':
                before = original50
            if before is not None:
                now = {entry['id']: entry for entry in entries}
                if any(now.get(entry['id']) != entry for entry in before):
                    raise ValueError('original entry identity or content was lost/changed')
                old_ids = [entry['id'] for entry in before]
                retained = [value for value in ids if value in set(old_ids)]
                if retained != old_ids:
                    raise ValueError('original history order changed')
            if phase == 'initial50':
                original50 = entries
            elif phase == 'paged100':
                original100 = entries
            phase_proofs.append({'phase': phase, 'messageEntries': len(messages),
                'customEntries': len(entries)-len(messages), 'totalEntries': len(entries),
                'connectionEpoch': snapshot['connection_epoch'],
                'originalRecordsUnchangedInOrder': True,
                'comparedToPriorSource': before is not None})
        if not (phase_proofs[1]['connectionEpoch'] > phase_proofs[0]['connectionEpoch']
                and phase_proofs[4]['connectionEpoch'] > phase_proofs[3]['connectionEpoch']
                and phase_proofs[2]['connectionEpoch'] == phase_proofs[1]['connectionEpoch']
                and phase_proofs[5]['connectionEpoch'] == phase_proofs[4]['connectionEpoch']):
            raise ValueError('explicit reconnect did not advance both original epochs')
        proofs.append({'sessionId': session_id, 'phases': phase_proofs})
    return {'acceptedWindowRecovery': True, 'issue': 68, 'sessions': proofs,
            'performanceAccepted': False, 'officialSamples': 0,
            'privateSessionFilesRead': False, 'desktop': False}
