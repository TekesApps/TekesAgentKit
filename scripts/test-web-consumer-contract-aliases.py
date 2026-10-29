#!/usr/bin/env python3
"""Exercise the actual checker CLI against valid and missing aliased imports."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--webui', type=Path, required=True)
    parser.add_argument('--agentkit', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--checker', type=Path, default=Path(__file__).with_name('check-web-consumer-contract.py'))
    args = parser.parse_args()
    cases = [
        ('unaliased', 'RunPresentation', 0),
        ('valid_alias', 'RunPresentation as AgentKitRunPresentation', 0),
        ('missing_source_alias', 'DefinitelyMissing as LocalAlias', 1),
    ]
    results = []
    with tempfile.TemporaryDirectory(prefix='web-contract-alias-') as directory:
        fixture = Path(directory)
        for relative in [
            'package.json', 'package-lock.json', 'host/bridge.ts', 'host/http.ts', 'src/api.ts',
            'node_modules/@tekesagentkit/client-contract/package.json',
            'node_modules/@tekesagentkit/client-contract/web-contract/index.d.ts',
        ]:
            target = fixture / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(args.webui / relative, target)
        # Keep real immutable declarations and bridge inputs; vary only the imported names.
        for name, imported, expected in cases:
            (fixture / 'src/alias.ts').write_text(
                "import type { " + imported + " } from '@tekesagentkit/client-contract';\n")
            result = subprocess.run([
                sys.executable, str(args.checker), '--agentkit', str(args.agentkit),
                '--webui', str(fixture), '--base-commit', 'HEAD',
            ], capture_output=True, text=True)
            valid = result.returncode == expected
            if name == 'missing_source_alias':
                valid = valid and result.stderr.strip() == (
                    'FAIL: pinned public contract lacks imported WebUI declarations: DefinitelyMissing')
            results.append({'case': name, 'exitStatus': result.returncode,
                            'expected': expected, 'passed': valid, 'stderr': result.stderr})
    print(json.dumps(results, indent=2))
    return 0 if all(row['passed'] for row in results) else 1


if __name__ == '__main__':
    raise SystemExit(main())
