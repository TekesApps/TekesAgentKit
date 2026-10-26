#!/usr/bin/env python3
"""Fixed-runtime #68 protocol regression; consumes explicit prebuilt programs."""
import argparse
import http.server
import importlib.util
import json
import os
from pathlib import Path
import threading
import traceback

from performance_process_sampler import ProcessSampler
from performance_provider_fixture import RequestStats, create_provider_handler, read_source_bytes, read_source_path
from pi_owned_fixture import prepare_pi
from pi_window_recovery_validation import validate_recovery_snapshots

spec = importlib.util.spec_from_file_location('owned_performance_case',
    Path(__file__).with_name('test-stage-performance-case.py'))
case_tools = importlib.util.module_from_spec(spec)
spec.loader.exec_module(case_tools)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--program', type=Path, required=True)
    parser.add_argument('--program-receipt', type=Path, required=True)
    parser.add_argument('--seeder', type=Path)
    parser.add_argument('--seeder-receipt', type=Path)
    parser.add_argument('--pi-root', type=Path, required=True)
    parser.add_argument('--case-name', required=True)
    args = parser.parse_args()
    if not args.case_name.replace('-', '').isalnum():
        parser.error('case name must be a simple owned name')
    args.endpoint = 'pi'
    program, receipt = case_tools.validate_program(args.program, args.program_receipt)
    if bool(args.seeder) != bool(args.seeder_receipt):
        parser.error('seeder and seeder-receipt must be supplied together')
    seeder, seeder_receipt = (case_tools.validate_program(args.seeder, args.seeder_receipt)
                             if args.seeder else (program, receipt))
    runtime_before = case_tools.runtime_identity(args)
    case = case_tools.ROOT / args.case_name
    case.mkdir(mode=0o700)
    workspace = case/'workspace'; workspace.mkdir()
    read_source_path(workspace).write_bytes(read_source_bytes())
    home = case/'host-home'; home.mkdir()
    tmp = case/'tmp'; tmp.mkdir()
    env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home),
           'TMPDIR': str(tmp), 'LANG': 'en_US.UTF-8'}
    stats = RequestStats()
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0),
        create_provider_handler(workspace, stats, endpoint='pi'))
    server.daemon_threads = False; server.block_on_close = True
    thread = threading.Thread(target=server.serve_forever)
    thread.start()
    sampler = ProcessSampler(case/'process-rss.jsonl', pi_embedded=True)
    sampler.register(os.getpid(), 'provider'); sampler.start()
    result = None; failure = None; exits = {}
    case_tools.write(case/'inputs.json', {'program': receipt, 'seeder': seeder_receipt,
        'runtime': runtime_before,
        'scope': 'original native 50/100 window through reconnect and close/open',
        'officialSamples': 0})
    try:
        config = prepare_pi(args.pi_root, case/'data', case/'pi-home',
            f'http://127.0.0.1:{server.server_port}/v1', strict_environment=True)
        config['endpointId'] = 'pi'
        ids, seed = case_tools.public_seed('pi', config, seeder, workspace, case,
                                         env, sampler, 'endpoint-test', 'endpoint-test')
        commands = []
        for session_id in ids:
            commands.extend([
                {'op': 'open', 'sessionId': session_id}, {'op': 'snapshot'},
                {'op': 'reconnect'}, {'op': 'open', 'sessionId': session_id}, {'op': 'snapshot'},
                {'op': 'close', 'sessionId': session_id}, {'op': 'open', 'sessionId': session_id},
                {'op': 'snapshot'}, {'op': 'earlier', 'sessionId': session_id}, {'op': 'snapshot'},
                {'op': 'reconnect'}, {'op': 'open', 'sessionId': session_id}, {'op': 'snapshot'},
                {'op': 'close', 'sessionId': session_id}, {'op': 'open', 'sessionId': session_id},
                {'op': 'snapshot'}])
        case_tools.write(case/'recovery-commands.json', commands)
        rows, exits = case_tools.run_program(program, dict(config, commands=commands), case,
                                             'recovery', env, sampler, timeout=120)
        if rows[0] != {'kind': 'pi', 'ready': True, 'version': '1.3.0'}:
            raise ValueError('fixed Pi handshake mismatch')
        snapshots = [row['result'] for row in rows if row.get('op') == 'snapshot']
        result = validate_recovery_snapshots(snapshots, ids)
        result['seedProofs'] = seed
    except Exception as error:
        failure = type(error).__name__ + ': ' + str(error)
        (case/'failure-traceback.log').write_text(traceback.format_exc())
    finally:
        try:
            rss = sampler.stop()
            case_tools.write(case/'rss-summary.json', rss)
            current = case_tools.classify(case_tools.process_table(), sampler.roots, sampler.known,
                                          pi_embedded=True)
            if any(row['role'] in ('agentkit', 'harness') for row in current.values()):
                failure = failure or 'owned Pi/AgentKit processes remain after recovery'
        except Exception as error:
            failure = failure or 'owned process observation failed: ' + str(error)
        server.shutdown(); server.server_close(); thread.join(timeout=5)
        provider = stats.snapshot()
        case_tools.write(case/'provider-observation.json', provider)
        if thread.is_alive() or provider['errorResponses']:
            failure = failure or 'owned provider did not retire without errors'
        try:
            case_tools.validate_program(program, args.program_receipt)
            if args.seeder:
                case_tools.validate_program(seeder, args.seeder_receipt)
            if case_tools.runtime_identity(args) != runtime_before:
                failure = failure or 'fixed Pi runtime changed'
        except Exception as error:
            failure = failure or 'identity recheck failed: ' + str(error)
    case_tools.write(case/'window-recovery-receipt.json', {
        'accepted': failure is None and result is not None, 'issue': 68,
        'sourceCommit': receipt['sourceCommit'], 'programSha256': receipt['sha256'],
        'seederSourceCommit': seeder_receipt['sourceCommit'],
        'seederSha256': seeder_receipt['sha256'],
        'runtime': runtime_before, 'result': result, 'processExit': exits,
        'error': failure, 'officialSamples': 0, 'performanceAccepted': False})
    print('PI_WINDOW_RECOVERY_' + ('FAILED ' if failure else 'ACCEPTED ') + str(case))
    return 1 if failure else 0


if __name__ == '__main__':
    raise SystemExit(main())
