from __future__ import annotations

import json
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import time
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'pi_return_gate_fixture.py'
MARKER = 'AGENTKIT_PI_UNKNOWN_GATE_9a7e'


def line(value: object) -> bytes:
    return json.dumps(value, separators=(',', ':')).encode() + b'\n'


class ReturnGateTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.receipt = self.root / 'receipt.jsonl'
        self.release = self.root / 'release'

    def start(self, child_code: str, *, limit: int = 4096):
        command = [sys.executable, str(SCRIPT), '--marker', MARKER, '--receipt', str(self.receipt),
                   '--release-file', str(self.release), '--max-gate-bytes', str(limit), '--',
                   sys.executable, '-u', '-c', child_code]
        return subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, bufsize=0)

    @staticmethod
    def read_line(process: subprocess.Popen, timeout: float = 2.0) -> bytes:
        ready, _, _ = select.select([process.stdout], [], [], timeout)
        if not ready:
            raise TimeoutError('wrapper stdout did not produce a line')
        return process.stdout.readline()

    def events(self) -> list[dict]:
        return [json.loads(row) for row in self.receipt.read_text().splitlines() if row]

    def test_prepare_control_passes_and_marked_prompt_gates_pi_stdout(self):
        code = "import json,sys\nw=lambda x:(sys.stdout.buffer.write(json.dumps(x,separators=(',',':')).encode()+b'\\n'),sys.stdout.flush())\nfor raw in sys.stdin.buffer:\n req=json.loads(raw)\n if req.get('message')=='/agentkit-control input.prepare': w({'type':'response','id':req.get('id'),'ok':True})\n elif req.get('message','').startswith('AGENTKIT_PI_UNKNOWN_GATE_9a7e'): w({'type':'response','id':req.get('id'),'accepted':True})\n"
        proc = self.start(code)
        try:
            prepare = line({'type': 'prompt', 'id': 'prepare-1', 'message': '/agentkit-control input.prepare'})
            proc.stdin.write(prepare); proc.stdin.flush()
            self.assertEqual(self.read_line(proc), line({'type': 'response', 'id': 'prepare-1', 'ok': True}))
            prompt = line({'type': 'prompt', 'id': 'run-1', 'message': MARKER + ' send this once'})
            proc.stdin.write(prompt); proc.stdin.flush()
            deadline = time.monotonic() + 1
            while time.monotonic() < deadline and len(self.events()) < 2: time.sleep(.01)
            self.assertEqual(self.events()[:2], [
                {'event': 'gate_ready', 'promptOrdinal': 1},
                {'event': 'mutation_forwarded', 'promptOrdinal': 1},
            ])
            time.sleep(.05)
            self.assertFalse(select.select([proc.stdout], [], [], .05)[0], 'Pi response leaked before release')
            self.release.touch()
            self.assertEqual(self.read_line(proc), line({'type': 'response', 'id': 'run-1', 'accepted': True}))
            proc.stdin.close()
            self.assertEqual(proc.wait(timeout=2), 0)
            event_names = [x['event'] for x in self.events()]
            self.assertLess(event_names.index('released'), event_names.index('exit'))
        finally:
            if proc.poll() is None:
                proc.kill(); proc.wait()
            for pipe in (proc.stdin, proc.stdout, proc.stderr):
                if pipe and not pipe.closed: pipe.close()

    def test_marked_wire_bytes_survive_without_json_reserialization(self):
        code = 'import sys\nfor raw in sys.stdin.buffer:\n sys.stdout.buffer.write(raw); sys.stdout.flush()'
        proc = self.start(code)
        raw = (' { "id" : "opaque", "message" : "' + MARKER + ' 中文", "type" : "prompt" } \n').encode()
        try:
            proc.stdin.write(raw); proc.stdin.flush()
            deadline = time.monotonic() + 1
            while time.monotonic() < deadline and (not self.receipt.exists() or len(self.events()) < 2):
                time.sleep(.01)
            self.assertFalse(select.select([proc.stdout], [], [], .05)[0])
            self.release.touch()
            self.assertEqual(self.read_line(proc), raw)
            proc.stdin.close()
            self.assertEqual(proc.wait(timeout=2), 0)
        finally:
            if proc.poll() is None:
                proc.kill(); proc.wait()
            for pipe in (proc.stdin, proc.stdout, proc.stderr):
                if pipe and not pipe.closed: pipe.close()

    def test_version_probe_executes_exact_command_without_creating_gate_receipt(self):
        command = [sys.executable, '-c', 'import sys; print("pi-test-version")', '--version']
        proc = subprocess.run([sys.executable, str(SCRIPT), '--marker', MARKER,
            '--receipt', str(self.receipt), '--release-file', str(self.release), '--', *command],
            capture_output=True, timeout=3)
        self.assertEqual(proc.returncode, 0)
        self.assertEqual(proc.stdout, b'pi-test-version\n')
        self.assertFalse(self.receipt.exists())
        self.assertFalse(self.release.exists())

    def test_pid_templates_make_restart_receipts_distinct(self):
        receipt_template = self.root / 'pi-return-{pid}.jsonl'
        release_template = self.root / 'pi-release-{pid}'
        child = [sys.executable, '-u', '-c', 'pass']
        command = [sys.executable, str(SCRIPT), '--marker', MARKER,
                   '--receipt', str(receipt_template), '--release-file', str(release_template),
                   '--', *child]
        proc = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, bufsize=0)
        proc.communicate(timeout=3)
        self.assertEqual(proc.returncode, 0)
        self.assertTrue(self.root.joinpath(f'pi-return-{proc.pid}.jsonl').exists())
        self.assertFalse(self.root.joinpath(f'pi-release-{proc.pid}').exists())

    def test_nonmatching_and_unintended_prompt_remain_transparent(self):
        code = 'import sys\nfor raw in sys.stdin.buffer:\n sys.stdout.buffer.write(raw); sys.stdout.flush()'
        proc = self.start(code)
        output = proc.communicate(input=line({'type':'prompt','message':'ordinary prompt'}) + line({'type':'prompt','message':'other marker'}), timeout=3)[0].splitlines()
        self.assertEqual([json.loads(x)['message'] for x in output], ['ordinary prompt', 'other marker'])
        self.assertEqual(proc.returncode, 0)
        self.assertIn('unarmed_exit', [x['event'] for x in self.events()])

    def test_gate_overflow_fails_and_records_without_silent_release(self):
        code = 'import json,sys\nsys.stdin.buffer.readline()\nsys.stdout.buffer.write((json.dumps({"type":"response","result":"' + ('x' * 200) + '"})+"\\n").encode()); sys.stdout.flush()'
        proc = self.start(code, limit=64)
        proc.stdin.write(line({'type':'prompt','message':MARKER})); proc.stdin.flush()
        proc.communicate(timeout=3)
        self.assertEqual(proc.returncode, 70)
        events = self.events()
        self.assertIn('gate_failure', [x['event'] for x in events])
        self.assertTrue(any(x['event'] == 'process_exit' and x['bufferedBytesAtExit'] == 0 for x in events))

    def test_child_exit_while_gated_is_not_reported_as_released(self):
        code = "import json,sys\nw=lambda x:(sys.stdout.write(json.dumps(x)+'\\n'),sys.stdout.flush())\nfor raw in sys.stdin.buffer:\n req=json.loads(raw)\n if req.get('message','').startswith('AGENTKIT_PI_UNKNOWN_GATE_9a7e'): w({'type':'response','value':1})\n"
        proc = self.start(code)
        proc.stdin.write(line({'type':'prompt','message':MARKER})); proc.stdin.flush()
        proc.communicate(timeout=3)
        self.assertEqual(proc.returncode, 70)
        events = self.events()
        self.assertIn('child_exit_while_gated', [x['event'] for x in events])
        self.assertNotIn('released', [x['event'] for x in events])

    def test_duplicate_marked_prompt_fails_after_the_first_forwarded_mutation(self):
        code = "import json,sys\nw=lambda x:(sys.stdout.write(json.dumps(x)+'\\n'),sys.stdout.flush())\nfor raw in sys.stdin.buffer:\n req=json.loads(raw)\n if req.get('message','').startswith('AGENTKIT_PI_UNKNOWN_GATE_9a7e'):\n  sys.stderr.write(req['message']+'\\n'); sys.stderr.flush(); w(req)\n"
        proc = self.start(code)
        proc.stdin.write(line({'type':'prompt','message':MARKER+' first'})); proc.stdin.flush()
        self.assertTrue(select.select([proc.stderr], [], [], 1)[0])
        self.assertTrue(proc.stderr.readline().decode().startswith(MARKER))
        proc.stdin.write(line({'type':'prompt','message':MARKER+' second'})); proc.stdin.flush()
        output, stderr = proc.communicate(timeout=3)
        output = output.splitlines()
        self.assertEqual(proc.returncode, 70)
        forwarded = stderr.decode().splitlines()
        self.assertEqual(sum(x.startswith(MARKER) for x in forwarded), 0)  # First line was consumed above.
        self.assertEqual([x['event'] for x in self.events()][:2], ['gate_ready', 'mutation_forwarded'])
        self.assertIn('gate_failure', [x['event'] for x in self.events()])

    def test_existing_release_path_is_rejected(self):
        self.release.touch()
        proc = self.start('pass')
        _, err = proc.communicate(timeout=2)
        self.assertNotEqual(proc.returncode, 0)
        self.assertIn(b'release paths must be new', err)


if __name__ == '__main__':
    unittest.main()
