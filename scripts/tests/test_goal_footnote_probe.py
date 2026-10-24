"""Real-goal acceptance rejects wrong identity and absent evidence; no network/UI."""
import copy
from pathlib import Path
import subprocess
import sys
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
from goal_footnote_probe import verify_goal_identity, verify_goal_footer, verify_goal_cleared


class GoalProbe(unittest.TestCase):
    def setUp(self):
        self.scope = {'endpointId': 'pi-ui', 'threadId': 'owned', 'sessionId': 'owned'}
        self.goal = {'id': 'goal', 'revision': 2, 'phase': 'paused'}
        self.receipt = {
            'offscreen': True, 'components': True, 'nativeInteraction': False, 'isStale': False,
            'transcriptItems': 2, 'finalAssistantItems': 1,
            'documentProjection': {'sections': 1, 'visibleMessages': 2},
            'transcriptContentProof': {'rows': 3, 'messages': 2, 'footnotes': 1},
            'goalFootnoteProof': dict(self.goal, scope=self.scope, goalId='goal', goalRevision=2,
                                     turnId='canonical-turn', label='目标 paused',
                                     originalGoalProjection=True, originalFooter=True,
                                     labelVisible=True, externalEndpoint=True,
                                     sourceMarkerInferred=False, durationInferred=False,
                                     nativeInteraction=False),
        }

    def test_original_identity_and_clearance_have_explicit_distinct_evidence(self):
        verify_goal_footer(self.receipt, self.scope, self.goal)
        cleared = copy.deepcopy(self.receipt)
        cleared['goalFootnoteProof'] = None
        self.assertTrue(verify_goal_cleared(self.receipt, cleared)['clearedGoalAbsent'])

    def test_foreign_scope_stale_revision_wrong_phase_and_fabricated_origin_fail(self):
        cases = [('scope', dict(self.scope, sessionId='other')),
                 ('goalId', 'other'), ('goalRevision', 1), ('phase', 'complete'),
                 ('sourceMarkerInferred', True), ('durationInferred', True),
                 ('originalFooter', False), ('labelVisible', False), ('nativeInteraction', True)]
        for key, value in cases:
            with self.subTest(key=key), self.assertRaises(AssertionError):
                bad = copy.deepcopy(self.receipt)
                bad['goalFootnoteProof'][key] = value
                verify_goal_footer(bad, self.scope, self.goal)

    def test_correct_data_is_not_promoted_to_a_visible_footer_pass(self):
        partial = copy.deepcopy(self.receipt)
        partial['goalFootnoteProof']['labelVisible'] = False
        verify_goal_identity(partial, self.scope, self.goal)
        with self.assertRaises(AssertionError):
            verify_goal_footer(partial, self.scope, self.goal)

    def test_old_consumer_stale_data_and_parse_diagnostic_cannot_prove_goal_clearance(self):
        for mutate in [lambda r: r.pop('goalFootnoteProof'),
                       lambda r: r.update(isStale=True),
                       lambda r: r['transcriptContentProof'].update(rows=4),
                       lambda r: r.update(transcriptItems=0)]:
            bad = copy.deepcopy(self.receipt)
            bad['goalFootnoteProof'] = None
            mutate(bad)
            with self.assertRaises(AssertionError):
                verify_goal_cleared(self.receipt, bad)

    def test_cli_refuses_missing_contract_inputs_and_incompatible_probes_before_launch(self):
        for arguments in [[], ['--pi-root', '/unused', '--ui-demo', '/unused', '--ui-components',
                               '--ui-transcript-content', '--ui-history-pages']]:
            result = subprocess.run([sys.executable, str(SCRIPTS / 'test-real-session-endpoints.py'),
                                     '--ui-goal-footnote', *arguments], capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertIn('--ui-goal-footnote', result.stderr)
            self.assertNotIn('Compiling ', result.stderr)
