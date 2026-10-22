#!/usr/bin/env python3
"""Verify frozen URL display facts with data-only Foundation objects on macOS.

No Swift toolchain, app/window inspection, UI events, URL requests or navigation.
The existing fixture supplies inputs; --write explicitly refreshes expected facts.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', action='store_true')
    args = parser.parse_args()
    if sys.platform != 'darwin':
        parser.error('Foundation oracle requires macOS; portable Rust tests use the frozen JSON')
    fixture = Path(__file__).resolve().parents[1] / 'crates/presentation/tests/fixtures/tool-url-foundation-20261023.json'
    expected = json.loads(fixture.read_text())
    inputs = [row['input'] for row in expected]
    if not all(isinstance(value, str) for value in inputs):
        raise ValueError('fixture inputs must be strings')
    probe = '''ObjC.import("Foundation"); var cases = INPUTS;
JSON.stringify(cases.map(function(s) {
    var u = $.NSURL.URLWithString(s);
    return {input:s, valid:!!u,
            host:u ? ObjC.unwrap(u.host) : null,
            path:u ? ObjC.unwrap(u.path) : null};
}));'''.replace('INPUTS', json.dumps(inputs))
    result = subprocess.run(['/usr/bin/osascript', '-l', 'JavaScript'], input=probe,
                            text=True, capture_output=True, check=True, timeout=15)
    actual = json.loads(result.stdout)
    if args.write:
        fixture.write_text(json.dumps(actual, ensure_ascii=False, indent=2) + '\n')
    elif actual != expected:
        raise SystemExit('Foundation facts changed; review the original property results before updating the fixture')
    print(json.dumps({'foundationFactsMatch': True, 'cases': len(actual),
                      'swiftToolchain': False, 'uiInteraction': False,
                      'networkRequests': False}, sort_keys=True))


if __name__ == '__main__':
    main()
