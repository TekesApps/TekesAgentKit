"""Public-only #29 samples and conservative history evidence; no harness IO here."""
from task_family_fixture import public_exchanges


def classification_commands(kind, parent, child_ids, *, native_source=None):
    if kind not in ('native', 'pi', 'dsh') or not isinstance(parent, str) or not parent.strip():
        raise ValueError('known endpoint and original parent required')
    prefix = []
    if kind == 'native':
        if not isinstance(native_source, dict) or not all(native_source.get(key) for key in ['cwd','provider','model']):
            raise ValueError('explicit ordinary Native source configuration required')
        parent = 'classification-parent'
        prefix = [{'op':'create','cwd':str(native_source['cwd']),'saveAs':parent},
                  {'op':'call','sessionId':parent,'method':'session.selectModel','args':{'provider':native_source['provider'],'model':native_source['model']}},
                  {'op':'open','sessionId':parent}, {'op':'compose','sessionId':parent,'text':'CLASSIFICATION_ORDINARY_PARENT'},
                  {'op':'waitSettled','sessionId':parent}]
    commands = prefix + [{'op':'open','sessionId':parent}]
    samples = [('parent', parent)]
    if kind != 'native':
        for index, child in enumerate(child_ids):
            commands.append({'op':'open','sessionId':child})
            samples.append(('child', child))
    for alias in ['visible-fork', 'archived-fork']:
        commands.extend([{'op':'call','sessionId':parent,'method':'session.fork','args':{},'saveAs':alias},
                         {'op':'open','sessionId':alias}])
        samples.append((alias, alias))
    if kind in ('native', 'pi'):
        commands.extend([{'op':'call','sessionId':parent,'method':'session.fork','args':{'ephemeral':True},'saveAs':'ephemeral'},
                         {'op':'open','sessionId':'ephemeral'}])
        samples.append(('ephemeral','ephemeral'))
    for _, session in samples:
        commands.append({'op':'presentation','sessionId':session})
    commands.append({'op':'snapshot'})
    commands.extend([{'op':'call','sessionId':'archived-fork','method':'session.archive'},
                     {'op':'waitInventoryMembership','sessionId':'archived-fork','present':False}])
    for _, session in samples:
        commands.append({'op':'presentation','sessionId':session})
    commands.append({'op':'snapshot'})
    commands.extend([{'op':'call','sessionId':'archived-fork','method':'session.unarchive'},
                     {'op':'waitInventoryMembership','sessionId':'archived-fork','present':True}, {'op':'reconnect'}])
    for _, session in samples:
        commands.extend([{'op':'open','sessionId':session}, {'op':'presentation','sessionId':session}])
    commands.append({'op':'snapshot'})
    commands.extend([{'op':'call','sessionId':'archived-fork','method':'session.archive'},
                     {'op':'waitInventoryMembership','sessionId':'archived-fork','present':False}, {'op':'snapshot'}])
    return commands, samples


def parse_classification_exchange(kind, commands, outputs, samples):
    exchanges = public_exchanges(kind, commands, outputs)
    snapshots = [row for row in exchanges if row['request']['op'] == 'snapshot']
    if len(snapshots) != 4:
        raise ValueError('baseline/retained/restored/final runtime snapshots required')
    before, retained, restored, after = [row['result'] for row in snapshots]
    if any(state.get('endpoint_id') != kind or state.get('inventory_ready') is not True
           for state in (before,retained,restored,after)):
        raise ValueError('ready original endpoint inventory snapshots required')
    aliases = {commands[row['index']]['saveAs']: row['result']['sessionId'] for row in exchanges if 'saveAs' in commands[row['index']]}
    if len(set(aliases.values())) != len(aliases):
        raise ValueError('fork identities collide')
    result = []
    for role, reference in samples:
        session = aliases.get(reference, reference)
        views = [row for row in exchanges if row['request']['op'] == 'presentation'
                 and row['request']['sessionId'] == session]
        if len(views) != 3 or not (views[0]['index'] < snapshots[0]['index'] < views[1]['index'] < snapshots[1]['index'] < views[2]['index'] < snapshots[2]['index']):
            raise ValueError('history views must surround original archive/reconnect')
        left, right, recovered = [row['result'] for row in views]
        expected = dict(endpointId=kind,threadId=session,sessionId=session)
        if (left.get('summary') or {}).get('scope') != expected or (right.get('summary') or {}).get('scope') != expected or (recovered.get('summary') or {}).get('scope') != expected:
            raise ValueError('history address changed')
        old_items = {row['id']:row for row in left['transcript']['items']}
        new_items = {row['id']:row for row in right['transcript']['items']}
        if not old_items or not all(new_items.get(key) == row for key,row in old_items.items()):
            raise ValueError('canonical history lost or rewritten')
        restored_items = {row['id']:row for row in recovered['transcript']['items']}
        if not all(restored_items.get(key) == row for key,row in old_items.items()):
            raise ValueError('unarchive/reconnect did not restore original history')
        old, new = [state['sessions'][session] for state in (before,retained)]
        final = after['sessions'][session]
        if not old['journal_ready'] or new['history_through_sequence'] < old['history_through_sequence'] or not restored['sessions'][session]['journal_ready']:
            raise ValueError('canonical lease/journal sequence lost')
        archived_ids = after.get('archived_session_ids', [])
        archived = (after.get('workspace_ready') is True and session in archived_ids) or (final.get('summary') or {}).get('status') == 'archived'
        lifecycle = [row for row in exchanges if row['request'].get('method') == 'session.archive'
                     and row['request'].get('sessionId') == session]
        is_archived_sample = role == 'archived-fork'
        if is_archived_sample:
            if len(lifecycle) != 2 or not all(session in row['result'].get('archivedSessionIds', []) for row in lifecycle):
                raise ValueError('exact successful public archive acknowledgements required')
            unarchive = [row for row in exchanges if row['request'].get('method') == 'session.unarchive'
                         and row['request'].get('sessionId') == session]
            if len(unarchive) != 1:
                raise ValueError('exact successful public unarchive acknowledgement required')
            reply = unarchive[0]['result']
            if kind == 'dsh':
                ids = reply.get('archivedSessionIds')
                acknowledged = isinstance(ids, list) and all(isinstance(value,str) for value in ids) and session not in ids
            else:
                acknowledged = reply.get('sessionId') == session
            if not acknowledged:
                raise ValueError('exact successful public unarchive acknowledgement required')
            if not archived and final.get('summary') is not None:
                raise ValueError('archived session still active')
        elif archived:
            raise ValueError('unexpected positive archive metadata')
        result.append(dict(role=role,address=dict(endpointId=kind,sessionId=session),
            summaryBefore=old.get('summary'), summaryAfter=final.get('summary'),
            archived=is_archived_sample, lifecycleArchiveAcknowledged=is_archived_sample, archiveAuthority=dict(positiveSnapshotFact=archived,workspaceReady=after.get('workspace_ready'),
                listed=session in archived_ids,summaryStatus=(final.get('summary') or {}).get('status')),
            history=dict(scope=expected,beforeItems=len(old_items),afterItems=len(new_items),
                beforeRevision=old.get('history_revision'),afterRevision=new.get('history_revision'),
                beforeThroughSequence=old['history_through_sequence'],afterThroughSequence=new['history_through_sequence'],
                originalItemsPreserved=True,unarchiveReconnectRestored=True,restoredItems=len(restored_items),journalReadyBefore=old['journal_ready'],journalReadyAfter=new['journal_ready'])))
    return dict(endpointId=kind, samples=result, realUiAcceptance=False,
                ephemeralCapability='unsupported' if kind == 'dsh' else 'exercised')


def _fact(summary, field):
    if field not in summary:
        return dict(state='missing')
    value = summary[field]
    valid = ((field == 'origin' and value in ('fork','subagent'))
             or (field == 'ephemeral' and type(value) is bool)
             or (field == 'parentSessionId' and isinstance(value, str) and bool(value.strip())))
    return dict(state='known' if valid else 'unknown', value=value)


def validate_classification_delivery(delivery, seeds):
    proof = delivery['classificationProof']
    packet = proof['inventoryPacket']
    sources = {row['endpointId']:row for row in packet['source']['classification']}
    assert set(sources) == {'native','pi','dsh'}
    observers = {row['endpointId']:row for row in packet['source']['observer']}
    assert set(observers) == set(sources)
    assert all(row['loaded'] and not row['stale'] and not row['errorPresent']
               and row['observation']['phase'] == 'observing' for row in observers.values()), 'original endpoint observer unhealthy'
    reads = {(row['address']['endpointId'],row['address']['sessionId']):row for row in proof['independentSelectedReads']}
    display = {(row['address']['endpointId'],row['address']['sessionId']) for row in packet['display']['rows']}
    checked = []
    for endpoint in seeds:
        kind = endpoint['endpointId']; source = sources[kind]
        assert source['inventoryReady'] and source['connectionEpoch'] > 0 and source['sourceRevision'] > 0
        records = {(row['address']['endpointId'],row['address']['sessionId']):row for row in source['records']}
        assert all(key[0] == kind for key in records), 'classification endpoint escaped'
        for sample in endpoint['samples']:
            key = (kind, sample['address']['sessionId']); read = reads[key]
            assert read['current'], 'independent selected read was stale'
            if kind == 'pi' and sample['role'] == 'ephemeral':
                assert key not in records and key not in display, 'previous endpoint scratch resurrected'
                assert not read['hasActiveRow'] and read['hiddenRowNotSupplemented']
            elif sample['archived']:
                assert key not in records and key not in display, 'archive resurrected'
                assert sample['lifecycleArchiveAcknowledged'], 'exact public archive acknowledgement missing'
                # Raw same-frame archive metadata may be unavailable (Native has no workspace stream).
                # The successful mutation and current membership are separate evidence.
                assert not read['hasActiveRow'] and read['hiddenRowNotSupplemented']
            else:
                record = records[key]; summary = sample['summaryAfter']
                for field in ['origin','parentSessionId','ephemeral']:
                    assert record[field] == _fact(summary,field), 'raw classification facts changed'
                expected = summary.get('origin') != 'subagent' and summary.get('ephemeral') is not True
                assert record['included'] is expected and read['hasActiveRow'] is expected
                assert (key in display) is expected, 'display membership differs from original source'
                if not expected:
                    assert key not in display and read['hiddenRowNotSupplemented'], 'hidden selected child/ephemeral resurrected'
                if kind == 'dsh': assert record['ephemeral']['state'] == 'missing', 'DSH invented ephemeral'
            history = sample['history']
            assert history['originalItemsPreserved'] and history['beforeItems'] > 0
            assert history['afterItems'] >= history['beforeItems'] and history['afterThroughSequence'] >= history['beforeThroughSequence']
            checked.append(dict(address=sample['address'],role=sample['role'],historyItems=history['beforeItems']))
    live = proof['hostOwnedEphemeral']
    address = live['address']; key = (address['endpointId'],address['sessionId'])
    assert key[0] == 'pi' and live['forkReply']['sessionId'] == key[1]
    assert key not in {(row['address']['endpointId'],row['address']['sessionId']) for ep in seeds for row in ep['samples']}
    record = next(row for row in sources['pi']['records'] if row['address'] == address)
    assert record['ephemeral'] == dict(state='known',value=True) and not record['included']
    assert key not in display and reads[key]['current'] and not reads[key]['hasActiveRow'] and reads[key]['hiddenRowNotSupplemented']
    assert live['originalItemsPreserved'] and live['beforeItems'] > 0 and live['afterItems'] == live['beforeItems']
    assert live['previousInstanceRetired']
    assert live['scope'] == dict(endpointId='pi',threadId=key[1],sessionId=key[1])
    return dict(accepted=True,evidenceScope='actual-classification-original-inventory-selected-read-and-history',
                samples=checked,unknownValuesRemainUnknown=True,confirmedMainInferred=False,
                nativeInteraction=False,physicalIme=False,desktopVisibilityProven=False)
