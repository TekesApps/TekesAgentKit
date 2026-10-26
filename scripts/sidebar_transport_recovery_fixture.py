"""Independent refusal verifier for real Sidebar transport recovery receipts."""

ENDPOINTS = ('native', 'pi', 'dsh')
STAGES = ('before', 'failed', 'recovered')


def _check(condition, message):
    if not condition:
        raise AssertionError(message)


def _object(value, message):
    _check(isinstance(value, dict), message)
    return value


def _list(value, message):
    _check(isinstance(value, list), message)
    return value


def _integer(value):
    return isinstance(value, int) and not isinstance(value, bool)


def _addr(value):
    value = _object(value, 'session address missing')
    camel = 'endpointId' in value or 'sessionId' in value
    snake = 'endpoint_id' in value or 'session_id' in value
    _check(camel != snake, 'session address has missing or mixed key conventions')
    endpoint = value.get('endpointId' if camel else 'endpoint_id')
    session = value.get('sessionId' if camel else 'session_id')
    _check(isinstance(endpoint, str) and endpoint and isinstance(session, str) and session,
           'session address is malformed')
    return endpoint, session


def _unique_rows(rows, label):
    rows = _list(rows, f'{label} rows missing')
    keys = [_addr(row.get('address')) for row in rows]
    _check(len(keys) == len(set(keys)), f'duplicate {label} session rows')
    return dict(zip(keys, rows))


def _copy_definition(row):
    actions = _object(row.get('actions'), 'canonical source row actions missing')
    items = _list(actions.get('items'), 'canonical source action items missing')
    copies = [item for item in items if isinstance(item, dict)
              and item.get('interaction') == 'copy' and item.get('enabled') is True]
    _check(len(copies) == 1, 'before source row must expose one enabled canonical Copy definition')
    definition = copies[0]
    _check(isinstance(definition.get('id'), str) and definition['id'],
           'canonical Copy definition ID missing')
    _check(isinstance(definition.get('copyText'), str) and definition['copyText'].strip(),
           'canonical Copy text missing')
    return definition


def validate_transport_recovery(receipt, seeds, fault_endpoints, requests_before, requests_after):
    """Validate actual probe evidence; never trusts an accepted flag."""
    receipt = _object(receipt, 'probe receipt missing')
    _check(receipt.get('schemaVersion') == 1 and receipt.get('status') == 'completed', 'incomplete probe receipt')
    _check(receipt.get('evidenceScope') == 'real-client-transport-close-runtime-reconnect', 'wrong evidence scope')
    _check(receipt.get('desktopInteraction') is False, 'desktop interaction is outside this probe')
    _check(receipt.get('endpoints') == list(ENDPOINTS), 'probe endpoint inventory mismatch')
    bootstrap = _object(receipt.get('bootstrap'), 'bootstrap evidence missing')
    _check(_integer(bootstrap.get('pageActions')) and bootstrap['pageActions'] > 0,
           'original Show more action missing')
    _check(bootstrap.get('mountedCount') == 9, 'nine mounted rows not proven')
    _check(bool(fault_endpoints) and len(set(fault_endpoints)) == len(fault_endpoints),
           'invalid fault endpoint selection')
    _check(set(fault_endpoints) <= set(ENDPOINTS), 'unsupported fault endpoint')
    cases = _list(receipt.get('cases'), 'recovery cases missing')
    _check(len(cases) == len(fault_endpoints), 'case set differs from requested subset')
    case_map = {}
    for row in cases:
        row = _object(row, 'recovery case malformed')
        endpoint = row.get('endpointId')
        _check(endpoint not in case_map, 'duplicate recovery case endpoint')
        case_map[endpoint] = row
    _check(set(case_map) == set(fault_endpoints), 'case endpoints differ from requested subset')

    seeds = _list(seeds, 'owned session seeds missing')
    expected = {}
    ordered_ids = {endpoint: [] for endpoint in ENDPOINTS}
    for seed in seeds:
        seed = _object(seed, 'owned session seed malformed')
        key = _addr(seed)
        _check(key not in expected, 'duplicate owned session seed')
        expected[key] = seed
        if key[0] in ordered_ids:
            ordered_ids[key[0]].append(key[1])
    _check(len(expected) == 9 and all(len(ordered_ids[endpoint]) == 3 for endpoint in ENDPOINTS),
           'owned seed set is not nine sessions')
    expected_keys = set(expected)
    expected_bindings = {(endpoint, session) for endpoint in ENDPOINTS for session in ordered_ids[endpoint][:2]}
    all_case_drafts = None

    for failed_endpoint, case in case_map.items():
        address = _addr(case.get('address'))
        _check(address[0] == failed_endpoint and address[1] == ordered_ids[failed_endpoint][0],
               'fault address is not the endpoint first seed')
        stages = {name: case.get(name) for name in STAGES}
        _check(all(isinstance(stage, dict) for stage in stages.values()), 'missing recovery stage')
        row_ids = None
        case_selected = None
        case_drafts = None
        observer_generations = {}
        source_revisions = []
        before_source_rows = None

        for stage_name, stage in stages.items():
            delivery = _object(stage.get('delivery'), 'original Sidebar delivery receipt missing')
            _check(delivery.get('schemaVersion') == 1, 'original Sidebar delivery schema mismatch')
            source = _object(delivery.get('source'), 'Sidebar source missing')
            display = _object(delivery.get('display'), 'Sidebar display missing')
            revision = source.get('revision')
            _check(_integer(revision) and revision > 0, 'Sidebar source revision missing')
            source_revisions.append(revision)
            source_rows = _unique_rows(source.get('rows'), 'source')
            display_rows = _unique_rows(display.get('rows'), 'display')
            _check(set(source_rows) == expected_keys and set(display_rows) == expected_keys,
                   'source/display session set mismatch')
            for key, seed in expected.items():
                for label, row in (('source', source_rows[key]), ('display', display_rows[key])):
                    _check(row.get('title') == seed['title'] and row.get('folderPath') == seed['cwd'],
                           f'{label} title/cwd differs from owned seed')
                    activity = _object(row.get('activity'), f'{label} activity missing')
                    stale = activity.get('isStale')
                    _check(isinstance(stale, bool), f'{label} activity.isStale missing')
                    _check(stale is (stage_name == 'failed' and key[0] == failed_endpoint),
                           f'{label} activity stale state differs from failed endpoint')
            if stage_name == 'before':
                # The normal delivery getter omits menus. The probe captures
                # these from the exact same original host publication.
                before_source_rows = _unique_rows(stage.get('sourceActions'), 'source action')
                _check(set(before_source_rows) == expected_keys, 'source action session set mismatch')

            mounts = _list(stage.get('mounts'), 'ContentHost mount receipt missing')
            mount_keys = [_addr(row.get('address')) for row in mounts]
            _check(len(mount_keys) == len(set(mount_keys)), 'duplicate ContentHost mounted row')
            mounted_ids = {_addr(row.get('address')): row.get('rowId') for row in mounts
                           if isinstance(row, dict) and row.get('mounted') is True}
            _check(len(mounts) == 9 and all(isinstance(row, dict) and row.get('mounted') is True for row in mounts)
                   and set(mounted_ids) == expected_keys and len(set(mounted_ids.values())) == 9,
                   'actual ContentHost mounts differ from nine sessions')
            if row_ids is None:
                row_ids = mounted_ids
            else:
                _check(mounted_ids == row_ids, 'component row identity changed across recovery')

            selected = _addr(stage.get('selectedScope'))
            _check(selected == address, 'selected RunBinding is not the fault target')
            display_selected = _addr(display.get('selected'))
            _check(selected == display_selected, 'selected RunBinding differs from displayed selection')
            drafts = _list(stage.get('drafts'), 'six drafts missing')
            _check(len(drafts) == 6, 'selection or six drafts missing')
            draft_scopes = [_addr(_object(draft, 'draft entry malformed').get('scope')) for draft in drafts]
            _check(len(set(draft_scopes)) == 6 and set(draft_scopes) == expected_bindings,
                   'draft scopes differ from six configured bindings')
            _check(all(isinstance(draft.get('text'), str) and draft['text'].strip() and
                       any('\u3400' <= char <= '\u9fff' for char in draft['text']) for draft in drafts),
                   'nonempty Chinese draft evidence missing')
            if case_selected is None:
                case_selected, case_drafts = selected, drafts
            else:
                _check(selected == case_selected and drafts == case_drafts,
                       'selection or draft changed across recovery')
            _check(stage.get('observerRetryEndpoints') == [], 'observer Retry control appeared')

            runtimes = _object(stage.get('runtime'), 'runtime endpoint map missing')
            _check(set(runtimes) == set(ENDPOINTS), 'runtime endpoint map mismatch')
            observer_rows = _list(source.get('observer'), 'observer rows missing')
            observer_ids = [row.get('endpointId') for row in observer_rows if isinstance(row, dict)]
            _check(len(observer_rows) == 3 and len(observer_ids) == 3 and len(set(observer_ids)) == 3
                   and set(observer_ids) == set(ENDPOINTS), 'duplicate or missing observer endpoint')
            observers = {row['endpointId']: row for row in observer_rows}
            for endpoint in ENDPOINTS:
                runtime = _object(runtimes[endpoint], f'{endpoint} runtime evidence missing')
                fault_failed = stage_name == 'failed' and endpoint == failed_endpoint
                expected_phase = 'failed' if fault_failed else 'ready'
                _check(runtime.get('connectionPhase') == expected_phase,
                       f'{endpoint} runtime phase mismatch')
                _check(runtime.get('inventoryReady') is (not fault_failed),
                       f'{endpoint} inventory readiness mismatch')
                # disconnect() clears each loaded session journal; a failed transport
                # therefore cannot retain selected-journal readiness.
                _check(runtime.get('selectedJournalReady') is (not fault_failed),
                       f'{endpoint} selected journal readiness mismatch')
                epoch = runtime.get('connectionEpoch')
                _check(_integer(epoch) and epoch > 0, f'{endpoint} connection epoch missing')

                observation_row = observers[endpoint]
                observation = _object(observation_row.get('observation'), 'observer state missing')
                _check(observation.get('phase') == 'observing', 'observer terminated or retried')
                generation = observation.get('generation')
                _check(_integer(generation) and generation > 0 and observation_row.get('generation') == generation,
                       'observer generation receipt mismatch')
                if endpoint in observer_generations:
                    _check(observer_generations[endpoint] == generation, 'observer generation changed')
                observer_generations[endpoint] = generation
                # This flag belongs to the in-memory observer lifecycle, not
                # the transport. Only canonical row activity is stale here.
                _check(observation_row.get('stale') is False
                       and observation_row.get('loaded') is True
                       and observation_row.get('errorPresent') is False
                       and observation.get('converged') is True,
                       'memory observer lost its live publication')

        _check(source_revisions[0] < source_revisions[1] < source_revisions[2],
               'Sidebar source publication revision did not advance through stages')
        _check(case_drafts is not None, 'case draft baseline missing')
        if all_case_drafts is None:
            all_case_drafts = case_drafts
        else:
            _check(case_drafts == all_case_drafts, 'six drafts changed between endpoint fault cases')

        before_rt = stages['before']['runtime']
        failed_rt = stages['failed']['runtime']
        recovered_rt = stages['recovered']['runtime']
        epochs = [before_rt[failed_endpoint]['connectionEpoch'], failed_rt[failed_endpoint]['connectionEpoch'],
                  recovered_rt[failed_endpoint]['connectionEpoch']]
        _check(epochs[0] < epochs[1] < epochs[2], 'fault endpoint connection epoch did not advance at each transition')
        _check(stages['recovered']['delivery']['source']['revision'] > stages['failed']['delivery']['source']['revision'],
               'recovery publication revision did not advance')
        for healthy in set(ENDPOINTS) - {failed_endpoint}:
            epochs = [stages[name]['runtime'][healthy]['connectionEpoch'] for name in STAGES]
            _check(epochs[0] == epochs[1] == epochs[2], 'healthy endpoint connection epoch changed')

        gate_before = _object(stages['before'].get('gate'), 'initial gate evidence missing')
        gate_failed = _object(stages['failed'].get('gate'), 'failed gate evidence missing')
        gate_recovered = _object(stages['recovered'].get('gate'), 'recovered gate evidence missing')
        for label, gate in (('before', gate_before), ('failed', gate_failed), ('recovered', gate_recovered)):
            _check(all(_integer(gate.get(field)) and gate[field] >= 0
                       for field in ('attempts', 'successes', 'rejections', 'closeCount', 'connectionGeneration')),
                   f'{label} gate counters malformed')
            _check(gate['attempts'] == gate['successes'] + gate['rejections'],
                   f'{label} gate attempts do not equal successes plus rejections')
            _check(gate['connectionGeneration'] == gate['successes'],
                   f'{label} gate connection generation differs from successful connections')
        _check(gate_before['attempts'] == 1 and gate_before['successes'] == 1 and gate_before['rejections'] == 0
               and gate_before['closeCount'] == 0 and gate_before.get('blocked') is False
               and gate_before.get('activeEndpoint') is True,
               'initial gate is not cleanly connected')
        _check(gate_failed['attempts'] >= 5 and gate_failed['rejections'] >= 4
               and gate_failed['successes'] == 1 and gate_failed['closeCount'] == 1
               and gate_failed['connectionGeneration'] == 1 and gate_failed.get('blocked') is True
               and gate_failed.get('activeEndpoint') is False,
               'failed gate evidence invalid')
        _check(gate_recovered['attempts'] == gate_failed['attempts'] + 1
               and gate_recovered['successes'] == 2 and gate_recovered['rejections'] == gate_failed['rejections']
               and gate_recovered['closeCount'] == 1 and gate_recovered['connectionGeneration'] == 2
               and gate_recovered.get('blocked') is False and gate_recovered.get('activeEndpoint') is True,
               'recovered gate evidence invalid')

        for healthy in set(ENDPOINTS) - {failed_endpoint}:
            refresh = _object(case.get('healthyRefresh', {}).get(healthy), 'healthy refresh evidence missing')
            before_epoch = before_rt[healthy]['connectionEpoch']
            failed_epoch = failed_rt[healthy]['connectionEpoch']
            recovered_epoch = recovered_rt[healthy]['connectionEpoch']
            before_observer_revision = next(row['revision'] for row in stages['before']['delivery']['source']['observer']
                                            if row['endpointId'] == healthy)
            failed_observer_revision = next(row['revision'] for row in stages['failed']['delivery']['source']['observer']
                                            if row['endpointId'] == healthy)
            _check(refresh.get('epochBefore') == before_epoch == failed_epoch == recovered_epoch
                   and refresh.get('epochAfter') == before_epoch and
                   _integer(refresh.get('revisionAfter')) and refresh['revisionAfter'] > refresh.get('revisionBefore', 0),
                   'healthy endpoint refresh did not advance in place')
            _check(refresh.get('revisionBefore') == before_observer_revision
                   and refresh.get('revisionAfter') == failed_observer_revision,
                   'healthy refresh revisions differ from original source publication')

        _check(case.get('reconnect') == {'commandCount': 1, 'endpointId': failed_endpoint},
               'original Reconnect command evidence mismatch')
        reads = _object(case.get('selectedReads'), 'selected-read evidence missing')
        _check(reads.get('selectionGenerationOwner') == 'probe-selected-read-scope'
               and _integer(reads.get('selectionGeneration')) and reads['selectionGeneration'] > 0,
               'probe selected-read scope generation missing')
        _check(reads.get('faultAcceptedAfterRecovery') is False, 'stale selected read was accepted')
        healthy_reads = _object(reads.get('healthyAcceptedAfterRecovery'), 'healthy selected-read evidence missing')
        _check(set(healthy_reads) == set(ENDPOINTS) - {failed_endpoint}
               and all(value is True for value in healthy_reads.values()), 'healthy selected read failed')

        _check(before_source_rows is not None, 'before source rows missing')
        canonical = _copy_definition(before_source_rows[address])
        copy_result = _object(case.get('capturedDefinitionCopy'), 'captured Copy evidence missing')
        _check(copy_result.get('definitionId') == canonical['id']
               and copy_result.get('expectedText') == canonical['copyText']
               and isinstance(copy_result.get('actualText'), str) and copy_result['actualText'].strip()
               and copy_result['actualText'] == canonical['copyText']
               and _addr(copy_result.get('address')) == address
               and copy_result.get('rejectedWhileFailed') is True
               and copy_result.get('executionsAfterRecovery') == 1,
               'captured definition Copy does not match before source canonical action')

    _check(requests_before == requests_after, 'provider request paths/counts changed during probe')
    return {'accepted': True, 'evidenceScope': 'real-client-transport-close-runtime-reconnect',
            'caseCount': len(cases), 'faultEndpoints': list(fault_endpoints), 'providerRequestsUnchanged': True}
