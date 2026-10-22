"""Native acceptance launches; LaunchServices status is never an app exit code.

Only the endpoint driver's owned local fixture credential may be forwarded.
No inherited credential environment or launcher command is included in receipts.
"""
import hashlib
from contextlib import nullcontext
import json
from pathlib import Path
import plistlib
import subprocess
import sys
from native_frontmost_trace import FrontmostTrace


def validate_bundle(program, bundle):
    program, bundle = Path(program).resolve(), Path(bundle).resolve()
    with (bundle / 'Contents/Info.plist').open('rb') as source:
        info = plistlib.load(source)
    name = info.get('CFBundleExecutable')
    if not isinstance(name, str) or Path(name).name != name:
        raise ValueError('invalid native bundle executable')
    if info.get('CFBundlePackageType') != 'APPL':
        raise ValueError('native launch requires an APPL bundle')
    if (bundle / 'Contents/MacOS' / name).resolve() != program:
        raise ValueError('native app must contain the exact --ui-demo executable')
    if not program.is_file():
        raise ValueError('native executable is missing')
    return bundle


def run_native_probe(program, arguments, environment, log, marker, *, bundle=None, timeout=55, observe_frontmost=False):
    """Return original native log after checking launch, crash and receipt gates.

    A LaunchServices app is not a subprocess child: its OS exit status is unknown.
    A successful wait only establishes closure. Each caller must still validate
    every original native receipt field. There is no activation or launch retry.
    """
    program, log = Path(program).resolve(), Path(log).resolve()
    digest = hashlib.sha256(program.read_bytes()).hexdigest()
    if bundle is None:
        observer = FrontmostTrace(str(log) + '.frontmost.jsonl') if observe_frontmost else nullcontext()
        with observer, log.open('w') as capture:
            result = subprocess.run([str(program), *arguments], env=environment,
                                    stdout=capture, stderr=subprocess.STDOUT, timeout=timeout)
        proof = {'profile': 'direct-executable', 'applicationExitCode': result.returncode,
                 'launcherExitCode': None, 'binarySha256': digest}
    else:
        if sys.platform != 'darwin':
            raise ValueError('LaunchServices requires macOS')
        bundle = validate_bundle(program, bundle)
        stdout_log, stderr_log = Path(str(log) + '.stdout'), Path(str(log) + '.stderr')
        stdout_log.write_text('')
        stderr_log.write_text('')
        command = ['/usr/bin/open', '-W', '-n', '--stdout', str(stdout_log), '--stderr', str(stderr_log)]
        # These values are local test controls and a per-launch local fixture
        # token, never a user's model/service key. Do not forward os.environ.
        for name in ('TEKES_SELFTEST', 'AGENT_STREAM_READING_CAPTURE', 'AGENTKIT_UI_FIXTURE_CREDENTIAL'):
            if name in environment:
                command += ['--env', name + '=' + environment[name]]
        command += [str(bundle), '--args', *arguments]
        try:
            observer = FrontmostTrace(str(log) + '.frontmost.jsonl') if observe_frontmost else nullcontext()
            with observer:
                result = subprocess.run(command, capture_output=True, text=True, timeout=timeout)
        except subprocess.TimeoutExpired:
            # Do not expose command arguments (including the owned token),
            # relaunch or infer that the application stopped after this timeout.
            raise RuntimeError('LaunchServices wait timed out; inspect the original app before another launch') from None
        finally:
            # Preserve both original streams: opening one path twice can give
            # independent file offsets and overwrite output. Combined order is
            # stream order, not an assertion of cross-stream event chronology.
            log.write_text(stderr_log.read_text() + '\n' + stdout_log.read_text())
        Path(str(log) + '.launcher').write_text((result.stdout or '') + '\n' + (result.stderr or ''))
        proof = {'profile': 'launch-services-app', 'applicationExitCode': None,
                 'launcherExitCode': result.returncode, 'binarySha256': digest}
    print('NATIVE_PROBE_LAUNCH_RECEIPT ' + json.dumps(proof, sort_keys=True), flush=True)
    text = log.read_text()
    if result.returncode != 0:
        raise AssertionError(str(log) + '\n' + text[-6000:])
    if any(signature in text for signature in ('panicked at ', 'terminating due to uncaught', 'fatal runtime error:', 'Assertion failed:')):
        raise AssertionError('native crash in ' + str(log) + '\n' + text[-6000:])
    receipts = [line for line in text.splitlines() if line.startswith(marker)]
    if len(receipts) != 1:
        raise AssertionError('expected one complete native receipt in ' + str(log) + '\n' + text[-6000:])
    return text
