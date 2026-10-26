import copy
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from performance_event_contract import validate_events


def cold_events():
    def scoped(event, session, **extra):
        return dict(event=event, scope=dict(endpointId='native', threadId=session,
                    sessionId=session), **extra)
    rows = [dict(event='consumer_started', mode='cold', sessionIds=['s0', 's1']),
            dict(event='runtime_ready')]
    rows += [scoped('snapshot', sid, usable=True) for sid in ('s0', 's1')]
    rows += [dict(event='iteration_started', iteration=0)]
    rows += [scoped('snapshot', sid, usable=True) for sid in ('s0', 's1')]
    rows += [scoped('history_page', sid) for sid in ('s0', 's1')]
    rows += [dict(event='reconnect', converged=True, epochAdvanced=True)]
    rows += [scoped('send', sid, round=0, sessionIndex=i, ticketId=f'ticket{i}',
                    durableUser={'source': 1}, firstMatchingAssistantContent={'source': 2},
                    durableMatchingAssistantSettlement={'source': 3})
             for i, sid in enumerate(('s0', 's1'))]
    rows += [scoped('run_controller_history_page', sid) for sid in ('s0', 's1')]
    return rows + [dict(event='cleanup_intent'), dict(event='runtime_cleanup_completed')]


class EventContractTests(unittest.TestCase):
    def test_complete_cold(self):
        self.assertEqual(validate_events(cold_events(), 'cold', ['s0', 's1'], 'native')
                         ['completeOriginalIterations'], 1)

    def test_empty_and_each_missing_event_rejected(self):
        with self.assertRaises(ValueError):
            validate_events([], 'cold', ['s0', 's1'], 'native')
        original = cold_events()
        for index in range(len(original)):
            with self.subTest(index=index), self.assertRaises(ValueError):
                validate_events(original[:index]+original[index+1:], 'cold', ['s0', 's1'], 'native')

    def test_missing_warm_rounds_and_wrong_source_rejected(self):
        rows = cold_events()
        with self.assertRaises(ValueError):
            validate_events(rows, 'warm', ['s0', 's1'], 'native')
        rows = copy.deepcopy(rows)
        next(row for row in rows if row['event'] == 'send')['scope']['threadId'] = 'foreign'
        with self.assertRaises(ValueError):
            validate_events(rows, 'cold', ['s0', 's1'], 'native')


if __name__ == '__main__':
    unittest.main()
