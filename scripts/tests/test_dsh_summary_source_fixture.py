import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from summary_source_fixture import SummarySourceEvidenceError, validate_witness


def dsh_witness():
    parent, child, call = "parent-dsh", "child-dsh", "call-dsh"
    raw_result = {
        "id": "result-dsh",
        "role": "tool",
        "source": {"kind": "tool", "callId": "child-call"},
        "toolCallId": "child-call",
        "isError": False,
        "content": [{"type": "text", "text": f"started subagent {child}"}],
    }
    return {
        "evidenceScope": "original-publication-and-same-memory-input",
        "serverTransactionalFreshnessProven": False,
        "projectionSucceeded": True,
        "snapshot": {
            "connectionEpoch": 5,
            "selection": {"destination": {"kind": "run", "scope": {
                "endpointId": "dsh", "threadId": parent, "sessionId": parent,
            }}},
            "transcript": {"isStale": False, "tools": [{
                "id": call,
                "name": "subagent",
                "arguments": '{"description":"Catalog proof child","prompt":"CHILD_INITIAL_PROOF"}',
                "result": raw_result,
                "resultPosition": {"kind": "journal", "sequence": 19, "contentIndex": None},
            }]},
            "summary": {
                "scope": {"endpointId": "dsh", "threadId": parent, "sessionId": parent},
                "subagents": [{"id": call, "sessionId": child,
                               "title": "Catalog proof child", "status": "completed"}],
            },
        },
        "authority": {
            "endpoint_id": "dsh", "connection_epoch": 5, "connection": "Ready",
            "inventory_ready": True, "control_ready": True, "actionables_ready": True,
            "actionables": {},
            "sessions": {
                parent: {
                    "journal_ready": True,
                    "entries": {"19": {"event": {
                        "seq": 19, "type": "tool/result", "data": {"message": raw_result},
                    }}},
                },
                child: {
                    "journal_ready": True,
                    "summary": {"sessionId": child, "running": False, "blank": False},
                    "run_open": False,
                    "queue": [],
                    # Deliberately inserted in the reverse of numeric order:
                    # sequence 19 is newer than sequence 9.
                    "entries": {
                        "19": {"event": {"seq": 19, "type": "turn/end", "data": {
                            "turn": 2, "reason": {"kind": "completed"},
                        }}},
                        "9": {"event": {"seq": 9, "type": "turn/start", "data": {"turn": 2}}},
                    },
                },
            },
        },
    }


class DshSummarySourceFixtureTests(unittest.TestCase):
    def test_accepts_opted_in_latest_completed_journal_and_keeps_status_absent(self):
        value = dsh_witness()
        receipt = validate_witness(value, endpoint="dsh", completed_journal=True)
        self.assertEqual(receipt["childStatus"], "completed")
        self.assertEqual(receipt["statusBasis"], "latest-child-journal-turn-completed")
        self.assertIsNone(receipt["rawChildStatus"])
        self.assertIsNone(receipt["indicator"])
        self.assertEqual(receipt["sourceIdentity"], {"kind": "journal", "sequence": 19})

    def test_rejects_a_later_turn_start_after_an_older_completion(self):
        value = dsh_witness()
        child = value["authority"]["sessions"]["child-dsh"]
        child["entries"]["20"] = {"event": {"seq": 20, "type": "turn/start", "data": {"turn": 3}}}
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value, endpoint="dsh", completed_journal=True)

    def test_rejects_status_that_disagrees_with_completed_journal(self):
        value = dsh_witness()
        value["snapshot"]["summary"]["subagents"][0]["status"] = "idle"
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value, endpoint="dsh", completed_journal=True)

    def test_completed_journal_is_opt_in_dsh_only_and_requires_omitted_status(self):
        value = dsh_witness()
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value, endpoint="dsh")
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value, endpoint="pi", completed_journal=True)
        value["authority"]["sessions"]["child-dsh"]["summary"]["status"] = "completed"
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value, endpoint="dsh", completed_journal=True)

    def test_rejects_missing_original_tool_result_source(self):
        value = dsh_witness()
        value["authority"]["sessions"]["parent-dsh"]["entries"].clear()
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value, endpoint="dsh", completed_journal=True)

    def test_rejects_noncompleted_or_nonlatest_end_reason(self):
        for reason in ({"kind": "failed"}, {"kind": "cancelled"}, None):
            value = dsh_witness()
            value["authority"]["sessions"]["child-dsh"]["entries"]["19"]["event"]["data"]["reason"] = reason
            with self.subTest(reason=reason), self.assertRaises(SummarySourceEvidenceError):
                validate_witness(value, endpoint="dsh", completed_journal=True)

    def test_parked_run_state_and_explicit_null_status_do_not_prove_omission(self):
        value = dsh_witness()
        value["authority"]["sessions"]["child-dsh"]["summary"]["runState"] = "parked"
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value, endpoint="dsh", completed_journal=True)
        value = dsh_witness()
        value["authority"]["sessions"]["child-dsh"]["summary"]["status"] = None
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value, endpoint="dsh", completed_journal=True)


if __name__ == "__main__":
    unittest.main()

class FrozenRunnerSourceIdentity(unittest.TestCase):
    def test_archive_identity_needs_no_git_and_rejects_changed_product_source(self):
        import hashlib
        import importlib.util
        import json
        import tempfile
        from unittest import mock
        spec = importlib.util.spec_from_file_location('dsh_frozen_runner',
            Path(__file__).resolve().parents[1] / 'test-real-dsh-summary-source.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            files = {'Cargo.toml': b'workspace', 'Cargo.lock': b'locked',
                     'crates/headless/src/main.rs': b'frozen product input'}
            for name, content in files.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(content)
            program = root / 'existing-program'
            program.write_bytes(b'already built, never executed by this test')
            manifest = root / 'frozen.json'
            manifest.write_text(json.dumps({'head': '5' * 40, 'files': {
                name: hashlib.sha256(content).hexdigest() for name, content in files.items()}}))
            with mock.patch.object(module, 'ROOT', root), mock.patch.object(
                    module.subprocess, 'run', side_effect=AssertionError('must not consult live Git')):
                identity = module._source_hashes(program, manifest)
                self.assertEqual(identity['gitRevision'], '5' * 40)
                self.assertEqual(identity['identity']['kind'], 'frozen-manifest')
                (root / 'crates/headless/src/main.rs').write_bytes(b'changed')
                with self.assertRaisesRegex(ValueError, 'product input changed'):
                    module._source_hashes(program, manifest)

    def test_frozen_manifest_rejects_escape_paths_before_reading_them(self):
        import importlib.util
        import json
        import tempfile
        from unittest import mock
        spec = importlib.util.spec_from_file_location('dsh_frozen_paths',
            Path(__file__).resolve().parents[1] / 'test-real-dsh-summary-source.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            program = root / 'program'
            program.write_bytes(b'fixture')
            manifest = root / 'manifest.json'
            manifest.write_text(json.dumps({'head': '5' * 40, 'files': {
                '../outside': '0' * 64, 'Cargo.toml': '0' * 64, 'Cargo.lock': '0' * 64,
                'crates/headless/src/main.rs': '0' * 64}}))
            with mock.patch.object(module, 'ROOT', root):
                with self.assertRaisesRegex(ValueError, 'unsafe frozen source path'):
                    module._source_hashes(program, manifest)
