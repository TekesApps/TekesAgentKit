"""Completeness checks for original performance consumer output (no budgets)."""


def validate_events(events, mode, session_ids, endpoint):
    if not isinstance(events, list) or any(not isinstance(row, dict) for row in events):
        raise ValueError('consumer output must contain original event objects')
    if len(session_ids) != 2 or len(set(session_ids)) != 2:
        raise ValueError('exactly two distinct owned session pointers required')
    if any(row.get('error') or row.get('status') in ('failed', 'failed_or_not_run', 'not_run')
           or row.get('event') in ('iteration_failed', 'iteration_not_run', 'consumer_error')
           for row in events):
        raise ValueError('consumer reported failed or missing diagnostic samples')

    def named(rows, name, count):
        result = [row for row in rows if row.get('event') == name]
        if len(result) != count:
            raise ValueError(f'expected {count} original {name} events, got {len(result)}')
        return result

    def scoped(rows):
        scopes = [row.get('scope', {}) for row in rows]
        if ({row.get('sessionId') for row in scopes} != set(session_ids)
                or any(row.get('endpointId') != endpoint
                       or row.get('threadId') != row.get('sessionId') for row in scopes)):
            raise ValueError('original events do not cover both exact session scopes')

    started = named(events, 'consumer_started', 1)[0]
    if started.get('mode') != mode or started.get('sessionIds') != session_ids:
        raise ValueError('consumer startup mode/pointers differ from owned inputs')
    named(events, 'runtime_ready', 1)
    named(events, 'cleanup_intent', 1)
    named(events, 'runtime_cleanup_completed', 1)
    scoped(named(events, 'run_controller_history_page', 2))
    rounds = 1 if mode == 'cold' else 11
    positions = [i for i, row in enumerate(events) if row.get('event') == 'iteration_started']
    if len(positions) != rounds:
        raise ValueError('missing/duplicate original iteration boundary')
    initial = named(events[:positions[0]], 'snapshot', 2)
    scoped(initial)
    if any(row.get('usable') is not True for row in initial):
        raise ValueError('initial original projection not usable')
    for round_index, position in enumerate(positions):
        end = positions[round_index + 1] if round_index + 1 < rounds else len(events)
        rows = events[position:end]
        if rows[0].get('iteration') != round_index:
            raise ValueError('iteration order or growth trajectory incomplete')
        snapshots = named(rows, 'snapshot' if mode == 'cold' else 'history_close_open_snapshot',
                          4 if mode != 'cold' and endpoint == 'pi' else 2)
        scoped(snapshots[:2])
        if any(row.get('usable') is not True for row in snapshots):
            raise ValueError('iteration original projection not usable')
        if mode == 'cold' and endpoint == 'pi':
            restored = named(rows, 'history_close_open_snapshot', 2)
            scoped(restored)
            if any(row.get('usable') is not True for row in restored):
                raise ValueError('original Pi Runtime window restoration not usable')
        scoped(named(rows, 'history_page', 2))
        sends = named(rows, 'send', 2)
        scoped(sends)
        if ({row.get('sessionIndex') for row in sends} != {0, 1}
                or any(row.get('round') != round_index or not row.get('ticketId')
                       or not row.get('durableUser') or not row.get('firstMatchingAssistantContent')
                       or not row.get('durableMatchingAssistantSettlement') for row in sends)):
            raise ValueError('missing original round/ticket/user/assistant settlement evidence')
        reconnects = named(rows, 'reconnect', 0 if mode == 'warm' else 1)
        if any(row.get('converged') is not True or row.get('epochAdvanced') is not True
               for row in reconnects):
            raise ValueError('original reconnect did not converge')
    return {'completeOriginalIterations': rounds, 'sessionCount': 2,
            'officialSamples': 0, 'budgetsEvaluated': False}
