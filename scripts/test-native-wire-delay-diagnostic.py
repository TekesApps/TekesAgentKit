#!/usr/bin/env python3
"""One-shot metadata-only Native cold wire diagnostic; never an official sample."""
from __future__ import annotations

import argparse
from contextlib import contextmanager
import importlib.util
from pathlib import Path
import sys
import json
import threading
import time

from performance_kernel_wire_probe import proxied_native_fixture


ROOT = Path('/Volumes/HP P900/dev/TekesAgentKit/.runtime-fixtures/stage-close-20261025/performance')
RUNNER = Path(__file__).with_name('test-stage-performance-case.py')


def load_runner():
    scripts = str(Path(__file__).resolve().parent)
    if scripts not in sys.path:
        sys.path.insert(0, scripts)
    spec = importlib.util.spec_from_file_location('stage_performance_case_wire_diagnostic', RUNNER)
    if spec is None or spec.loader is None:
        raise RuntimeError('could not load the existing stage case runner')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--case-name', required=True)
    parser.add_argument('--program', type=Path, required=True)
    parser.add_argument('--program-receipt', type=Path, required=True)
    parser.add_argument('--seeder', type=Path, required=True)
    parser.add_argument('--seeder-receipt', type=Path, required=True)
    parser.add_argument('--profile', type=Path, required=True)
    parser.add_argument('--kernel-supervisor', type=Path, required=True)
    parser.add_argument('--kernel-worker', type=Path, required=True)
    parser.add_argument('--providers-config', type=Path, required=True)
    args = parser.parse_args(argv)

    runner = load_runner()
    original_factory = runner.owned_native
    original_provider = runner.create_provider_handler
    active = {'measurement': False, 'created': False}
    provider_lock = threading.Lock()

    def provider_factory(workspace, stats, **keywords):
        base = original_provider(workspace, stats, **keywords)
        path = ROOT / args.case_name / 'native-provider-observations.jsonl'
        def record(event):
            with provider_lock:
                with path.open('a' if active['created'] else 'x', encoding='utf-8') as stream:
                    stream.write(json.dumps(event) + '\n')
                active['created'] = True
        class TimedProvider(base):
            def do_POST(self):
                observed = active['measurement']
                if observed:
                    record({'event': 'provider_request_start', 'monotonicNs': time.monotonic_ns(),
                            'businessRequestsBefore': stats.snapshot()['businessRequests']})
                try:
                    return super().do_POST()
                finally:
                    if observed:
                        record({'event': 'provider_request_return', 'monotonicNs': time.monotonic_ns(),
                                'businessRequestsAfter': stats.snapshot()['businessRequests']})
        return TimedProvider

    runner.create_provider_handler = provider_factory

    @contextmanager
    def diagnostic_factory(*factory_args, **factory_kwargs):
        case_root = Path(factory_args[4] if len(factory_args) > 4 else factory_kwargs['base_root'])
        observation_path = case_root / 'native-wire-observations.jsonl'
        with proxied_native_fixture(original_factory, observation_path,
                                    *factory_args, **factory_kwargs) as fixture:
            active['measurement'] = factory_kwargs.get('resume_receipt') is not None
            try:
                yield fixture
            finally:
                active['measurement'] = False

    runner.owned_native = diagnostic_factory
    sys.argv = [str(RUNNER), '--endpoint', 'native', '--mode', 'cold',
                '--case-name', args.case_name,
                '--program', str(args.program), '--program-receipt', str(args.program_receipt),
                '--seeder', str(args.seeder), '--seeder-receipt', str(args.seeder_receipt),
                '--profile', str(args.profile),
                '--kernel-supervisor', str(args.kernel_supervisor),
                '--kernel-worker', str(args.kernel_worker),
                '--providers-config', str(args.providers_config)]
    print('NATIVE_WIRE_DIAGNOSTIC diagnosticOnly=true officialSamples=0 retry=false', flush=True)
    return runner.main()


if __name__ == '__main__':
    raise SystemExit(main())
