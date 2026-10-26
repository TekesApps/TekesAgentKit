import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from pi_window_recovery_validation import validate_recovery_snapshots


def native_source(session, count):
    entries = []
    for index in range(120-count, 120):
        entries += [{'id': f'{session}-message-{index}', 'type': 'message',
                     'message': {'role': 'user', 'content': f'original {index}'}},
                    {'id': f'{session}-custom-{index}', 'type': 'custom', 'data': index}]
    return {'sessionId': session, 'entries': entries, 'hasMoreBefore': True}


def snapshots():
    result = []
    for i, session in enumerate(('s0', 's1')):
        for count, epoch in zip((50, 50, 50, 100, 100, 100), (2, 5, 5, 5, 8, 8)):
            result.append({'connection': 'Ready', 'connection_epoch': epoch + 6*i,
                'endpoint_id': 'pi', 'inventory_ready': True,
                'sessions': {session: {'journal_ready': True,
                                       'native_conversation': native_source(session, count)}}})
    return result


class PiWindowRecoveryValidationTests(unittest.TestCase):
    def test_same_original_50_and_paged100_survive_both_recovery_paths(self):
        proof = validate_recovery_snapshots(snapshots(), ['s0', 's1'])
        self.assertTrue(proof['acceptedWindowRecovery'])
        self.assertFalse(proof['performanceAccepted'])
        self.assertEqual(proof['sessions'][0]['phases'][3]['totalEntries'], 200)

    def test_custom_driven_expansion_or_paged_shrink_is_not_accepted(self):
        for phase, wrong_count in ((1, 100), (4, 50)):
            with self.subTest(phase=phase):
                rows = snapshots()
                rows[phase]['sessions']['s0']['native_conversation'] = native_source('s0', wrong_count)
                with self.assertRaises(ValueError):
                    validate_recovery_snapshots(rows, ['s0', 's1'])

    def test_reordered_or_modified_original_entries_rejected(self):
        for kind in ('order', 'content'):
            rows = snapshots()
            entries = rows[1]['sessions']['s0']['native_conversation']['entries']
            if kind == 'order':
                entries[0], entries[1] = entries[1], entries[0]
            else:
                entries[0]['message']['content'] = 'altered'
            with self.subTest(kind=kind), self.assertRaises(ValueError):
                validate_recovery_snapshots(rows, ['s0', 's1'])

    def test_wrong_endpoint_pointer_or_missing_epoch_advance_rejected(self):
        rows = snapshots()
        for key, value in (('endpoint_id', 'other'), ('connection_epoch', 2)):
            changed = copy.deepcopy(rows); changed[1][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                validate_recovery_snapshots(changed, ['s0', 's1'])


if __name__ == '__main__':
    unittest.main()
