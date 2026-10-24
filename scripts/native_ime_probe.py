"""Live Pinyin input for the exact normal-app native probe; never activates it.

The existing launcher owns APPL identity, stream/crash/receipt gates and wait
status. This adapter only tails its original logs and posts to a verified PID.
"""
import ctypes
import hashlib
import plistlib
from pathlib import Path
import threading
import time

from native_frontmost_trace import frontmost_identity
from native_probe_launcher import run_native_probe, validate_bundle

MARKER = 'NATIVE_AGENT_SYSTEM_IME_PROBE_OK '
SOURCE = 'com.apple.inputmethod.SCIM.ITABC'


def executable_for_pid(pid):
    library = ctypes.CDLL('/usr/lib/libproc.dylib')
    library.proc_pidpath.argtypes = [ctypes.c_int, ctypes.c_void_p, ctypes.c_uint32]
    library.proc_pidpath.restype = ctypes.c_int
    buffer = ctypes.create_string_buffer(4096)
    if library.proc_pidpath(pid, buffer, len(buffer)) <= 0:
        raise RuntimeError('owned demo executable is unavailable')
    return Path(buffer.value.decode('utf-8')).resolve(strict=True)


class StreamTail:
    """Complete UTF-8 lines only, including writes split inside a character."""
    def __init__(self, filename):
        self.filename = Path(filename)
        self.offset = 0
        self.pending = b''

    def read(self):
        if not self.filename.exists():
            return []
        with self.filename.open('rb') as stream:
            stream.seek(self.offset)
            data = stream.read()
            self.offset = stream.tell()
        parts = (self.pending + data).split(b'\n')
        self.pending = parts.pop()
        return [part.decode('utf-8') + '\n' for part in parts]


class AppInputDriver:
    def __init__(self, mac, program, bundle_id):
        self.mac = mac
        self.program = Path(program).resolve(strict=True)
        self.bundle_id = bundle_id
        self.pid = None
        self.seen = set()

    def verify_target(self):
        identity = frontmost_identity()
        pid = identity.get('pid')
        if (not isinstance(pid, int) or pid <= 0
                or identity.get('bundleId') != self.bundle_id
                or (self.pid is not None and pid != self.pid)
                or executable_for_pid(pid) != self.program):
            raise RuntimeError('owned demo is not the exact foreground app; refusing key injection')
        if self.mac.source() != SOURCE:
            raise RuntimeError('input source changed; refusing key injection')
        self.pid = pid

    def line(self, line, alive):
        for stage, keys in [('TYPE', (45, 34, 4, 0, 31)), ('COMMIT', (49,))]:
            if line.strip() != f'selftest: Log("SYSTEM_IME_{stage}_READY")':
                continue
            required = set() if stage == 'TYPE' else {'TYPE'}
            if self.seen != required:
                raise RuntimeError('duplicate or out-of-order IME stage; refusing key injection')
            for code in keys:
                if not alive():
                    raise RuntimeError('owned demo already closed; refusing key injection')
                self.verify_target()
                self.mac.key(self.pid, code)
            self.seen.add(stage)


def run_app_ime(program, bundle, log, mac, before, *, observe_frontmost=False):
    program, log = Path(program).resolve(strict=True), Path(log).resolve()
    bundle = validate_bundle(program, bundle)
    with (bundle / 'Contents/Info.plist').open('rb') as stream:
        bundle_id = plistlib.load(stream).get('CFBundleIdentifier')
    if not isinstance(bundle_id, str) or not bundle_id:
        raise ValueError('normal-app IME probe needs a bundle identifier')
    for suffix in ('', '.stdout', '.stderr', '.launcher', '.frontmost.jsonl'):
        if Path(str(log) + suffix).exists():
            raise ValueError('preserve existing IME evidence; choose a new log')
    driver = AppInputDriver(mac, program, bundle_id)
    done = threading.Event()
    outcome = []

    def launch():
        try:
            outcome.append(run_native_probe(
                program, ['-ApplePersistenceIgnoreState', 'YES', '--components',
                          '--transcript-content', '--system-ime-selftest'],
                {'TEKES_SELFTEST': 'text'}, log, MARKER, bundle=bundle,
                timeout=30, observe_frontmost=observe_frontmost))
        except Exception as error:
            outcome.append(error)
        finally:
            done.set()

    thread = threading.Thread(target=launch, daemon=True)
    tails = [StreamTail(str(log) + suffix) for suffix in ('.stderr', '.stdout')]
    thread.start()
    deadline = time.monotonic() + 35
    try:
        while time.monotonic() < deadline:
            for tail in tails:
                for line in tail.read():
                    print(line, end='', flush=True)
                    driver.line(line, lambda: not done.is_set())
            if done.is_set():
                if isinstance(outcome[0], Exception):
                    raise outcome[0]
                after = mac.source()
                if driver.seen != {'TYPE', 'COMMIT'} or after != before:
                    raise RuntimeError('normal-app IME stages/source did not complete')
                return {'softwareIme': True, 'pidTargetedEvents': True,
                        'physicalKeyboard': False, 'candidateWindow': False,
                        'profile': 'launch-services-app', 'applicationExitCode': None,
                        'launcherExitCode': 0, 'pid': driver.pid,
                        'sourceBefore': before, 'sourceAfter': after,
                        'binarySha256': hashlib.sha256(program.read_bytes()).hexdigest()}
            time.sleep(0.01)
        raise RuntimeError('normal-app IME observation timed out; inspect original app before another launch')
    finally:
        # Do not activate, relaunch, commit IME or kill an app after an input
        # guard fails. Its original selftest/launcher deadline owns closure.
        thread.join(max(0, deadline - time.monotonic()) + 1)
