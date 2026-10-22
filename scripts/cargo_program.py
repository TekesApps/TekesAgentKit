"""Select an actual Cargo executable without guessing target/profile paths."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys


def cargo_program(root, package, name, *, kind='bin', existing=None):
    if kind not in ('bin', 'example'):
        raise ValueError('unsupported Cargo executable kind')
    origin = 'explicit-existing' if existing is not None else 'cargo-compiler-artifact'
    if existing is not None:
        program = Path(existing).resolve(strict=True)
    else:
        result = subprocess.run(
            ['cargo', 'build', '--locked', '--message-format=json', '-p', package,
             '--' + kind, name], cwd=root, capture_output=True, text=True, check=True)
        sys.stderr.write(result.stderr)
        candidates = set()
        for line in result.stdout.splitlines():
            event = json.loads(line)
            if event.get('reason') == 'compiler-message':
                sys.stderr.write(event.get('message', {}).get('rendered') or '')
            target = event.get('target', {})
            if (event.get('reason') == 'compiler-artifact' and target.get('name') == name
                    and kind in target.get('kind', [])
                    and not event.get('profile', {}).get('test') and event.get('executable')):
                candidates.add(Path(event['executable']).resolve(strict=True))
        if len(candidates) != 1:
            raise ValueError('Cargo did not identify exactly one requested executable')
        program = candidates.pop()
    if not program.is_file():
        raise ValueError('Cargo acceptance executable is not a file')
    return program, {'program': str(program), 'sha256': hashlib.sha256(program.read_bytes()).hexdigest(),
                     'bytes': program.stat().st_size, 'origin': origin}
