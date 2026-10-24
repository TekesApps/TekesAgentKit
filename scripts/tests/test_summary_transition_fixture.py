import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parent))
from summary_source_fixture import SummarySourceEvidenceError, validate_witness
from test_summary_source_fixture import witness

MARKER = 'CONTROLLED_CHILD_FAILURE'


def transition(endpoint, phase):
    value = witness()
    value['authority']['endpoint_id'] = endpoint
    value['snapshot']['summary']['scope']['endpointId'] = endpoint
    value['snapshot']['selection']['destination']['scope']['endpointId'] = endpoint
    value['snapshot']['summary']['subagents'][0]['status'] = phase
    child = {'summary': {'sessionId': 'child-1', 'running': phase == 'running', 'blank': True},
             'journal_ready': True, 'run_open': phase == 'running', 'queue': [], 'entries': {}}
    if endpoint == 'pi':
        entries = [
            {'id': 'start', 'type': 'custom', 'customType': 'agentkit.run.v1', 'data': {'phase': 'started', 'id': 'run'}},
            {'id': 'user', 'type': 'message', 'message': {'role': 'user', 'content': [{'type': 'text', 'text': MARKER}]}}]
        if phase == 'failed':
            entries += [
                {'id': 'error', 'type': 'message', 'message': {'role': 'assistant', 'stopReason': 'error', 'errorMessage': MARKER}},
                {'id': 'end', 'type': 'custom', 'customType': 'agentkit.run.v1', 'data': {'phase': 'settled', 'id': 'run', 'aborted': False}}]
        child['native_conversation'] = {'sessionId': 'child-1', 'generation': 'g',
                                       'isStreaming': phase == 'running', 'entries': entries}
    else:
        events = [
            {'seq': 29, 'type': 'turn/start', 'data': {'turn': 3}},
            {'seq': 32, 'type': 'user/message', 'data': {'source': {'kind': 'user'}, 'content': [{'type': 'text', 'text': MARKER}]}}]
        if phase == 'failed':
            events.append({'seq': 36, 'type': 'turn/end', 'data': {'turn': 3, 'reason': {'kind': 'error', 'error': {'message': MARKER}}}})
        child['entries'] = {str(event['seq']): {'event': event} for event in reversed(events)}
    value['authority']['sessions']['child-1'] = child
    return value


def validate(value, endpoint, phase):
    return validate_witness(value, endpoint=endpoint, transition_phase=phase, authored_marker=MARKER)


class SummaryTransitionFixtureTests(unittest.TestCase):
    def test_accepts_original_running_and_error_sources_without_rewriting_blank_or_status(self):
        for endpoint in ('pi', 'dsh'):
            for phase in ('running', 'failed'):
                value = transition(endpoint, phase)
                before = copy.deepcopy(value)
                proof = validate(value, endpoint, phase)
                self.assertEqual(proof['childStatus'], phase)
                self.assertEqual(proof['indicator'], phase)
                self.assertIsNone(proof['rawChildStatus'])
                self.assertEqual(value, before)

    def test_running_may_retain_only_its_own_dispatching_queue_item(self):
        value = transition('pi', 'running')
        item = {'placement': 'dispatching', 'payload': {'sessionId': 'child-1', 'mode': 'queue',
                'content': [{'type': 'text', 'text': MARKER}]},
                'message': {'content': [{'type': 'text', 'text': MARKER}]}}
        value['authority']['sessions']['child-1']['queue'] = [item]
        proof = validate(value, 'pi', 'running')
        self.assertEqual(proof['transitionSource']['capturedDispatchingQueueCount'], 1)
        for field, replacement in [('placement', 'queued'), ('payload', {'sessionId': 'other'})]:
            wrong = copy.deepcopy(value)
            wrong['authority']['sessions']['child-1']['queue'][0][field] = replacement
            with self.assertRaises(SummarySourceEvidenceError):
                validate(wrong, 'pi', 'running')

    def test_transitions_do_not_expand_the_default_or_completed_profiles(self):
        value = transition('pi', 'running')
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value)
        for kwargs in [dict(transition_phase='future', authored_marker=MARKER),
                       dict(transition_phase='running'), dict(authored_marker=MARKER),
                       dict(endpoint='native', transition_phase='running', authored_marker=MARKER),
                       dict(endpoint='dsh', completed_journal=True, transition_phase='running', authored_marker=MARKER)]:
            with self.subTest(kwargs=kwargs), self.assertRaises(SummarySourceEvidenceError):
                validate_witness(value, **kwargs)

    def test_later_or_unrelated_authored_input_does_not_prove_this_failure(self):
        for endpoint in ('pi', 'dsh'):
            value = transition(endpoint, 'failed')
            child = value['authority']['sessions']['child-1']
            if endpoint == 'pi':
                child['native_conversation']['entries'].append({'id': 'new-user', 'type': 'message', 'message': {'role': 'user', 'content': [{'type': 'text', 'text': 'unrelated'}]}})
            else:
                child['entries']['40'] = {'event': {'seq': 40, 'type': 'user/message', 'data': {'source': {'kind': 'user'}, 'content': [{'type': 'text', 'text': 'unrelated'}]}}}
            with self.assertRaises(SummarySourceEvidenceError):
                validate(value, endpoint, 'failed')

    def test_parent_tool_error_or_pointer_text_cannot_replace_child_evidence(self):
        value = transition('pi', 'failed')
        del value['authority']['sessions']['parent-1']['entries']['2']
        with self.assertRaises(SummarySourceEvidenceError):
            validate(value, 'pi', 'failed')

    def test_a_new_native_run_or_mismatched_settlement_invalidates_old_error(self):
        for change in ('new', 'mismatch', 'aborted'):
            value = transition('pi', 'failed')
            entries = value['authority']['sessions']['child-1']['native_conversation']['entries']
            if change == 'new': entries.append({'id': 'new', 'type': 'custom', 'customType': 'agentkit.run.v1', 'data': {'phase': 'started', 'id': 'next'}})
            elif change == 'mismatch': entries[-1]['data']['id'] = 'other'
            else: entries[-1]['data']['aborted'] = True
            with self.subTest(change=change), self.assertRaises(SummarySourceEvidenceError):
                validate(value, 'pi', 'failed')

    def test_latest_numeric_journal_boundary_not_an_older_error_controls_the_profile(self):
        value = transition('dsh', 'failed')
        value['authority']['sessions']['child-1']['entries']['100'] = {'event': {'seq': 100, 'type': 'turn/start', 'data': {'turn': 4}}}
        with self.assertRaises(SummarySourceEvidenceError):
            validate(value, 'dsh', 'failed')

    def test_unreadable_queued_or_actionable_child_is_not_transition_proof(self):
        for kind in ('read-error', 'not-ready', 'queue', 'parked', 'actionable'):
            value = transition('pi', 'failed')
            child = value['authority']['sessions']['child-1']
            if kind == 'read-error': child['journal_error'] = 'unreadable'
            elif kind == 'not-ready': child['journal_ready'] = False
            elif kind == 'queue': child['queue'] = [{'text': 'next'}]
            elif kind == 'parked': child['summary']['runState'] = 'parked'
            else: value['authority']['actionables'] = {'question': {'sessionId': 'child-1'}}
            with self.subTest(kind=kind), self.assertRaises(SummarySourceEvidenceError):
                validate(value, 'pi', 'failed')

    def test_provider_error_text_without_current_public_error_is_insufficient(self):
        for endpoint in ('pi', 'dsh'):
            value = transition(endpoint, 'failed')
            child = value['authority']['sessions']['child-1']
            if endpoint == 'pi': child['native_conversation']['entries'][2]['message']['stopReason'] = 'stop'
            else: child['entries']['36']['event']['data']['reason']['kind'] = 'completed'
            with self.assertRaises(SummarySourceEvidenceError):
                validate(value, endpoint, 'failed')


if __name__ == '__main__':
    unittest.main()
