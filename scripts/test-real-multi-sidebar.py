#!/usr/bin/env python3
"""One owned, simultaneous Native/Pi/DSH offscreen host; never desktop input.

Requires explicit existing programs. No build, installation, private history write,
ambient credentials, native-window launch or Screen Recording entry point.
"""
import argparse
import copy
from contextlib import ExitStack
import hashlib
import http.server
import json
import os
import shutil
import signal
from pathlib import Path
import subprocess
import threading
import time
import uuid
from cargo_program import cargo_program
from dsh_owned_fixture import owned_dsh
from native_owned_fixture import owned_native
from owned_endpoint_provider import provider_handler
from pi_owned_fixture import prepare_pi
from runtime_provenance import kernel_programs
from multi_sidechat_fixture import validate_multi_sidechat_delivery
from sidebar_transport_recovery_fixture import validate_transport_recovery
from sidebar_classification_fixture import classification_commands, parse_classification_exchange, validate_classification_delivery
from task_family_fixture import native_task_seed_commands, pi_subagent_seed_commands, dsh_subagent_seed_commands, parse_family_exchange

ROOT = Path(__file__).resolve().parents[1]
NATIVE_AUTH = 'AGENTKIT_MULTI_NATIVE_BEARER'
DSH_AUTH = 'AGENTKIT_MULTI_DSH_COOKIE'


def write_json(path, value):
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, 'w') as stream:
        json.dump(value, stream, indent=2, ensure_ascii=False)
        stream.write('\n')


def run_recovery_probe(command, environment, output, *, timeout=120):
    """Own a separate process session; retain logs and teardown evidence on failure."""
    write_json(output/'probe-command.json', command)  # No credentials in argv.
    with (output/'demo.stdout').open('w') as stdout, (output/'demo.stderr').open('w') as stderr:
        process = subprocess.Popen(command, env=environment, stdout=stdout, stderr=stderr,
                                   start_new_session=True, close_fds=True)
        timed_out = False
        try:
            process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
        finally:
            # Only this new session belongs to us. Close leftover Pi RPC children
            # even when the probe exited before its Runtime::stop cleanup.
            for sig in (signal.SIGTERM, signal.SIGKILL):
                try:
                    os.killpg(process.pid, sig)
                except ProcessLookupError:
                    break
                try:
                    process.wait(timeout=4)
                except subprocess.TimeoutExpired:
                    pass
            process.wait(timeout=4)
            deadline = time.monotonic()+3
            group_exited = False
            while time.monotonic() < deadline:
                try:
                    os.killpg(process.pid, 0)
                except ProcessLookupError:
                    group_exited = True
                    break
                time.sleep(0.05)
            write_json(output/'probe-exit.json', dict(pid=process.pid, exitCode=process.returncode,
                       timedOut=timed_out, groupExited=group_exited))
    assert not timed_out and process.returncode == 0 and group_exited, 'recovery probe failed; inspect retained logs/exit receipt'
    return subprocess.CompletedProcess(command, process.returncode,
                                      (output/'demo.stdout').read_text(), (output/'demo.stderr').read_text())


def address(value):
    return (value.get('endpointId', value.get('endpoint_id')),
            value.get('sessionId', value.get('session_id')))


def validate_delivery(delivery, seeds):
    stages = {row['stage']: row['receipt'] for row in delivery['stages']}
    assert set(stages) == {'baseline', 'after-original-page-action', 'query'}, 'missing stage'
    expected = {(seed['endpointId'], seed['sessionId']): seed for seed in seeds}
    for receipt in stages.values():
        source = receipt['source']
        assert source['endpointCount'] == 3 and source['loaded'] and not source['errorPresent']
        statuses = source['observer']
        assert {row['endpointId'] for row in statuses} == {'native', 'pi', 'dsh'}
        assert all(row['loaded'] and not row['stale'] and not row['errorPresent']
                   and row['observation']['phase'] == 'observing'
                   and row['observation']['converged'] for row in statuses), 'endpoint not healthy'
        rows = {address(row['address']): row for row in source['rows']}
        assert set(rows) == set(expected), 'actual public inventory differs from owned seeds'
        for key, seed in expected.items():
            assert rows[key]['title'] == seed['title'], 'canonical title mismatch'
            assert rows[key]['folderPath'] == seed['cwd'], 'canonical cwd mismatch'
        for row in receipt['display']['rows']:
            key = address(row['address'])
            assert key in rows and row['title'] == rows[key]['title'], 'display/source mismatch'
        assert all(row['presentInContentHost'] for row in receipt['host']['mountedRows']), 'row node absent'
        target = receipt['display']['visibilityTarget']
        assert target and address(target['address']) in expected, 'original reveal target absent'
        assert receipt['host']['visibilityTargetPresentInContentHost'], 'reveal target node absent'
        assert target['itemId'] in {row['rowId'] for row in receipt['host']['mountedRows']}, 'reveal row mismatch'
    before, after = (stages[name] for name in ['baseline', 'after-original-page-action'])
    assert before['display']['paging']['hasMore'], 'no positive pagination baseline'
    assert after['display']['paging']['limit'] > before['display']['paging']['limit']
    before_ids = {address(row['address']): row['rowId'] for row in before['host']['mountedRows']}
    after_ids = {address(row['address']): row['rowId'] for row in after['host']['mountedRows']}
    assert after['host']['focusedComponentId'] == before['host']['focusedComponentId'], 'paging changed focus'
    assert len(after_ids) > len(before_ids), 'original page action did not mount more rows'
    assert all(after_ids.get(key) == value for key, value in before_ids.items()), 'row identity changed'
    query = stages['query']
    assert {address(row['address']) for row in query['display']['rows']} == set(expected)
    assert query['host']['focusedComponentId'] == before['host']['focusedComponentId'], 'query changed focus'
    projection = query['host']['appliedSearchProjection']
    assert projection and projection['localResultsReady'] and projection['resultsReady']
    assert not projection['unresolved'], 'display projection still has unresolved hits'
    children = query['display']['multiSearch']['children']
    assert {child['endpointId'] for child in children} == {'native', 'pi', 'dsh'}
    assert all(child['phase']['state'] == 'complete' for child in children), 'provider search not complete'
    return {'accepted': True, 'realHarnessAcceptance': True, 'endpointCount': 3,
            'evidenceScope': 'actual-inventory-query-pagination-and-mounted-reveal-target',
            'mountedRevealTargetProven': True, 'componentFocusPreserved': True,
            'seedCount': len(seeds), 'mountedBefore': len(before_ids), 'mountedAfter': len(after_ids),
            'nativeInteraction': False, 'physicalIme': False, 'desktopVisibilityProven': False,
            'sameSessionIdCollisionProven': False}


def validate_family_delivery(delivery, seeds):
    assert delivery['accepted'] is True and delivery['nativeInteraction'] is False
    assert len(delivery['cases']) == len(seeds) == 6
    expected = {(row['endpoint'], row['parent']['scope']['sessionId']): row for row in seeds}
    actual = {(row['endpointId'], row['parent']['sessionId']): row for row in delivery['cases']}
    assert len(actual) == len(expected) and actual.keys() == expected.keys(), 'family source domain differs from actual seeds'
    for key, row in actual.items():
        seed = expected[key]
        assert row['parent'] == seed['parent']['scope'], 'family parent scope mismatch'
        assert row['childId'] == seed['child']['address']['sessionId'], 'family child mismatch'
        assert row['childScope']['endpointId'] == key[0] and row['childScope']['sessionId'] == row['childId'], 'child scope mismatch'
        assert row['childScope']['threadId'] == row['parent']['threadId'], 'child thread mismatch'
        assert all(row[name] is True for name in ['canonicalSourceMatched','openedViaOriginalControl',
            'returnedViaOriginalTab','closedViaOriginalControl','lateParentResultDidNotStealSelection','lateClosedResultRejected',
            'parentDraftPreserved','childContentMounted']), 'original family path incomplete'
        assert row['historyItems'] > 0, 'actual child history missing'
        assert row['readOnly'] is (key[0] == 'native'), 'bounded mode changed'
        frame = row['catalogFrame']
        assert frame['parent'] == row['parent'] and frame['epoch'] > 0 and frame['generation'] > 0 and frame['revision'] > 0
        assert frame['entry'] == seed['child']['catalogEntry'], 'catalog changed before UI admission'
        assert row['parentTaskRowMountedWithoutOpenAction'] is (key[0] == 'native')
        if key[0] == 'native':
            assert row['canonicalTask'] == seed['task']['row'], 'canonical Task row mismatch'
        else:
            assert row['canonicalTask'] is None, 'invented independent Task'
    return {'accepted': True, 'evidenceScope':'real-family-original-controls-and-dispatch',
            'nativeInteraction':False, 'physicalIme':False,
            'sameTaskRowDynamicPointerProven':False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['headless-program', 'pi-root', 'dsh-root', 'kernel-root',
                 'kernel-supervisor', 'kernel-worker', 'output']:
        parser.add_argument('--'+name, type=Path, required=True)
    parser.add_argument('--ui-demo', type=Path, help='Existing ordinary demo executable (required outside recovery opt-in)')
    parser.add_argument('--task-family', action='store_true', help='Use real public Task/child seeds and original family navigation receipt; separate from Sidebar acceptance')
    parser.add_argument('--classification-proof', action='store_true', help='Add actual public fork/ephemeral/archive and history retention to the family profile')
    parser.add_argument('--multi-sidechat', action='store_true', help='Original three-endpoint SideChat dispatcher lifecycle, including non-first Pi; offscreen only')
    parser.add_argument('--transport-recovery-probe', type=Path, help='Opt in to the existing sidebar_transport_recovery_probe executable')
    parser.add_argument('--recovery-endpoint', action='append', choices=['native', 'pi', 'dsh'], default=[],
                        help='Fault endpoint case to run; repeat to select a subset (default: all three)')
    args = parser.parse_args()
    recovery = args.transport_recovery_probe is not None
    if not recovery and args.ui_demo is None:
        parser.error('--ui-demo is required outside transport-recovery-probe mode')
    if recovery and (args.task_family or args.classification_proof or args.multi_sidechat):
        parser.error('transport-recovery-probe is a separate profile')
    if not recovery and args.recovery_endpoint:
        parser.error('recovery-endpoint requires transport-recovery-probe')
    if len(args.recovery_endpoint) != len(set(args.recovery_endpoint)):
        parser.error('recovery-endpoint values must be unique')
    recovery_endpoints = args.recovery_endpoint or ['native', 'pi', 'dsh']
    if args.multi_sidechat and (args.task_family or args.classification_proof):
        parser.error('multi-sidechat uses ordinary parents, separate from Task/classification profiles')
    if args.classification_proof and not args.task_family:
        parser.error('classification-proof requires task-family source seeds')
    output = args.output.resolve()
    if output.exists():
        parser.error('output must be a new owned directory')
    if not str(output).startswith('/Volumes/HP P900/dev/TekesAgentKit/'):
        parser.error('output must stay under the existing AgentKit SSD project directory')
    output.mkdir(mode=0o700)
    workspace = output/'workspace'; workspace.mkdir(mode=0o700)
    tmp = output/'tmp'; tmp.mkdir(mode=0o700)
    home = output/'host-home'; home.mkdir(mode=0o700)
    environment = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home),
                   'TMPDIR': str(tmp), 'LANG': 'en_US.UTF-8', 'LC_ALL': 'en_US.UTF-8'}
    headless, headless_identity = cargo_program(ROOT, 'tekes-agent-headless', 'tekes-agent-headless', existing=args.headless_program)
    if recovery:
        demo, demo_identity = cargo_program(ROOT/'ui', 'tekes-agent-ui-demo', 'sidebar_transport_recovery_probe',
                                            kind='example', existing=args.transport_recovery_probe)
    else:
        demo, demo_identity = cargo_program(ROOT/'ui', 'tekes-agent-ui-demo', 'tekes-agent-ui-demo', existing=args.ui_demo)
    supervisor, worker, kernel_identity = kernel_programs(args.kernel_root, supervisor=args.kernel_supervisor,
                                                        worker=args.kernel_worker, expected_version='0.9.3')
    if recovery:
        archives = output/'programs'; archives.mkdir(mode=0o700)
        for name, program, identity in [('headless', headless, headless_identity), ('probe', demo, demo_identity)]:
            archive = archives/name
            shutil.copy2(program, archive)
            assert hashlib.sha256(archive.read_bytes()).hexdigest() == identity['sha256'], 'program archive hash mismatch'
            identity['archiveOnly'] = str(archive)
    write_json(output/'programs.json', {'headless': headless_identity, 'demo': demo_identity, 'kernel': kernel_identity})
    requests = []
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), provider_handler(workspace=lambda:workspace, requests=requests,
        cancel_hold_seconds=3 if args.multi_sidechat else None))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    marker = 'agentkit-multi-'+uuid.uuid4().hex
    seeds = []
    classification_seeds = []
    started = time.time()
    try:
        with ExitStack() as stack:
            stack.callback(server.server_close)
            stack.callback(server.shutdown)
            provider_url = 'http://127.0.0.1:'+str(server.server_port)+'/v1'
            providers = json.loads((args.kernel_root/'fixtures/config/providers.canonical.json').read_text())
            native = stack.enter_context(owned_native(supervisor, worker, providers, provider_url, output, environment))
            dsh_home = output/'dsh-home'
            dsh_env = dict(environment, HOME=str(dsh_home), DSH_HOME=str(dsh_home),
                           DSH_AGENTS_HOME=str(dsh_home/'agents'), DSH_TELEMETRY_DISABLED='1',
                           AGENTKIT_SYNTHETIC_KEY='synthetic-key')
            dsh = stack.enter_context(owned_dsh(args.dsh_root, output/'dsh-data', dsh_home,
                                              workspace, provider_url, dsh_env, query_sqlite=not (args.task_family or args.multi_sidechat)))
            assert dsh.version == '0.2.1-rc.2', 'unexpected DSH package'
            pi = prepare_pi(args.pi_root, output/'pi-data', output/'pi-home', provider_url, strict_environment=True)
            if recovery:
                # Only public runtime entry files; never inspect harness sessions.
                public_inputs = {
                    'piNode': Path(pi['executable']),
                    'piCli': Path(pi['arguments'][0]),
                    'dshNode': dsh.node,
                    'dshEntry': dsh.entry,
                    'providerFixture': args.kernel_root/'fixtures/config/providers.canonical.json',
                }
                write_json(output/'runtime-inputs.json', {
                    name: {'path': str(path.resolve()), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
                    for name, path in public_inputs.items()})
            # Pi inherits the demo process environment in the public connector.
            # Explicitly blank our other harness auth keys rather than claiming
            # that PiConfiguration.environment clears all inherited variables.
            pi['environment'].update({NATIVE_AUTH: '', DSH_AUTH: ''})
            configs = [
                ('native', {'kind':'native', 'url':native['url'], 'bearer':native['bearer'], 'expectedVersion':'0.9.3'},
                 native['providers']['providers'][0]['id'], native['providers']['providers'][0]['models'][0]['id']),
                ('pi', dict(pi, expectedVersion='1.3.0'), 'endpoint-test', 'endpoint-test'),
                ('dsh', dsh.connector_config([]), 'endpoint-test', 'endpoint-test')]
            endpoints = []
            for endpoint_id, config, provider, model in configs:
                if args.task_family:
                    ids = []
                    for index in range(2):
                        if endpoint_id == 'native':
                            parent_id = str(uuid.uuid4())
                            commands = native_task_seed_commands(parent_id, workspace, provider, model)
                        else:
                            parent_id = None
                            builder = pi_subagent_seed_commands if endpoint_id == 'pi' else dsh_subagent_seed_commands
                            commands = builder('parent', workspace, provider, model)
                        write_json(output/(endpoint_id+'-family-'+str(index)+'-commands.json'), commands)
                        result = subprocess.run([str(headless)], input=json.dumps(dict(config, commands=commands)),
                                                text=True, capture_output=True, env=environment, timeout=120)
                        (output/(endpoint_id+'-family-'+str(index)+'.stdout')).write_text(result.stdout)
                        (output/(endpoint_id+'-family-'+str(index)+'.stderr')).write_text(result.stderr)
                        assert result.returncode == 0, endpoint_id+' family public seed failed'
                        rows = [json.loads(line) for line in result.stdout.splitlines()]
                        assert rows[0]['version'] == config['expectedVersion']
                        if parent_id is None:
                            parent_id = next(row['result']['sessionId'] for row in rows if row.get('op')=='create')
                        manifest = parse_family_exchange(endpoint_id, commands, rows, parent_id, workspace)
                        seeds.append(manifest)
                        ids.append(parent_id)
                else:
                    commands = []
                    for index in range(3):
                        alias = 'seed-'+str(index)
                        title = marker+' '+endpoint_id+' '+str(index)
                        text = marker+' input '+endpoint_id+' '+str(index)
                        commands.extend([
                            {'op':'create','cwd':str(workspace),'saveAs':alias},
                            {'op':'call','sessionId':alias,'method':'session.selectModel','args':{'provider':provider,'model':model}},
                            {'op':'call','sessionId':alias,'method':'session.rename','args':{'title':title}},
                            {'op':'open','sessionId':alias}, {'op':'compose','sessionId':alias,'text':text},
                            {'op':'waitSettled','sessionId':alias}])
                    commands.append({'op':'snapshot'})
                    result = subprocess.run([str(headless)], input=json.dumps(dict(config, commands=commands)),
                                            text=True, capture_output=True, env=environment, timeout=120)
                    # Config contains credentials; never persist it or echo command input.
                    (output/(endpoint_id+'-seed.stdout')).write_text(result.stdout)
                    (output/(endpoint_id+'-seed.stderr')).write_text(result.stderr)
                    assert result.returncode == 0, endpoint_id+' public seeding failed; inspect owned logs'
                    rows = [json.loads(line) for line in result.stdout.splitlines()]
                    assert rows[0]['version'] == config['expectedVersion'], 'actual runtime handshake mismatch'
                    ids = [row['result']['sessionId'] for row in rows if row.get('op') == 'create']
                    assert len(ids) == 3 and len(set(ids)) == 3
                    snapshot = next(row['result'] for row in reversed(rows) if row.get('op') == 'snapshot')
                    for index, session_id in enumerate(ids):
                        state = json.dumps(snapshot['sessions'][session_id], ensure_ascii=False)
                        assert marker+' input '+endpoint_id+' '+str(index) in state and 'ENDPOINT_REPLY_OK' in state, 'input/reply missing'
                        seeds.append({'endpointId':endpoint_id,'sessionId':session_id,
                                      'title':marker+' '+endpoint_id+' '+str(index),'cwd':str(workspace)})
                if args.classification_proof:
                    child_ids = [seed['child']['address']['sessionId'] for seed in seeds if seed['endpoint'] == endpoint_id]
                    commands, samples = classification_commands(endpoint_id, ids[0], child_ids, native_source=dict(cwd=workspace, provider=provider, model=model))
                    write_json(output/(endpoint_id+'-classification-commands.json'), commands)
                    result = subprocess.run([str(headless)], input=json.dumps(dict(config, commands=commands)),
                                            text=True, capture_output=True, env=environment, timeout=120)
                    (output/(endpoint_id+'-classification.stdout')).write_text(result.stdout)
                    (output/(endpoint_id+'-classification.stderr')).write_text(result.stderr)
                    assert result.returncode == 0, endpoint_id+' public classification seed failed'
                    outputs = [json.loads(line) for line in result.stdout.splitlines()]
                    assert outputs[0]['version'] == config['expectedVersion'], 'classification runtime version mismatch'
                    classification_seeds.append(parse_classification_exchange(endpoint_id, commands, outputs, samples))
                endpoint = {key:value for key,value in config.items()
                            if key not in ('bearer','cookie','expectedVersion','commands')}
                endpoint.update(endpointId=endpoint_id,sessions=ids[:2])
                if endpoint_id == 'native': endpoint['bearerEnv'] = NATIVE_AUTH
                if endpoint_id == 'dsh': endpoint['cookieEnv'] = DSH_AUTH
                endpoints.append(endpoint)
            write_json(output/'seeds.json', seeds)
            if args.classification_proof: write_json(output/'classification-seeds.json', classification_seeds)
            write_json(output/'endpoints.json', {'endpoints':endpoints})
            demo_env = dict(environment, **{NATIVE_AUTH:native['bearer'], DSH_AUTH:dsh.cookie})
            if recovery:
                command = [str(demo), '--endpoint-config', str((output/'endpoints.json').resolve()),
                           '--receipt', str((output/'transport-recovery-receipt.json').resolve())]
                for endpoint_id in recovery_endpoints:
                    command.extend(['--endpoint', endpoint_id])
            elif args.multi_sidechat:
                command = [str(demo),'--components','--transcript-content','--endpoint-config',str(output/'endpoints.json'),
                           '--multi-sidechat-delivery-receipt',str(output/'delivery.json'),
                           '--offscreen',str(output/'multi-sidechat.png')]
            elif args.task_family:
                command = [str(demo),'--components','--transcript-content','--endpoint-config',str(output/'endpoints.json'),
                           '--family-seed-manifest',str(output/'seeds.json'),
                           '--family-delivery-receipt',str(output/'delivery.json'),
                           '--offscreen',str(output/'family.png')]
            else:
                command = [str(demo),'--components','--endpoint-config',str(output/'endpoints.json'),
                           '--sidebar-query',marker,'--sidebar-delivery-receipt',str(output/'delivery.json'),
                           '--offscreen',str(output/'sidebar.png')]
            if args.classification_proof:
                command.extend(['--sidebar-classification-proof',str(output/'classification-seeds.json')])
            requests_before_probe = copy.deepcopy(requests) if recovery else None
            if recovery:
                result = run_recovery_probe(command, demo_env, output)
            else:
                result = subprocess.run(command, env=demo_env, text=True, capture_output=True, timeout=120)
                (output/'demo.stdout').write_text(result.stdout)
                (output/'demo.stderr').write_text(result.stderr)
            assert result.returncode == 0, 'offscreen host failed; inspect owned logs/partial receipt'
            if recovery:
                assert (output/'transport-recovery-receipt.json').is_file(), 'probe did not write its public receipt'
                # Final acceptance is delayed until ExitStack has shut down both owned services.
                pending_recovery = json.loads((output/'transport-recovery-receipt.json').read_text())
                pending_requests = requests_before_probe
                pending_command = command
                pending_result = result
                pending_native, pending_dsh = native, dsh
            else:
                delivery = json.loads((output/'delivery.json').read_text())
                accepted = (validate_multi_sidechat_delivery(delivery, seeds) if args.multi_sidechat else
                            validate_family_delivery(delivery, seeds) if args.task_family else validate_delivery(delivery, seeds))
                if args.classification_proof:
                    accepted['classification'] = validate_classification_delivery(delivery, classification_seeds)
                accepted.update(startedAt=started, finishedAt=time.time(), providerRequestPaths=requests)
                write_json(output/'acceptance.json', accepted)
                print(json.dumps(accepted))
        if recovery:
            exits = {'native': pending_native['process'].poll(), 'dsh': pending_dsh.process.poll()}
            write_json(output/'owned-service-exits.json', exits)
            assert exits == {'native': 0, 'dsh': 0}, 'owned service exit status is not clean'
            accepted = validate_transport_recovery(pending_recovery, seeds, recovery_endpoints,
                                                   pending_requests, requests)
            accepted.update(startedAt=started, finishedAt=time.time(), command=pending_command,
                            stdout=pending_result.stdout, stderr=pending_result.stderr, exitCode=pending_result.returncode,
                            endpointProgram=demo_identity, ownedServiceExits=exits, providerRequestPathsBefore=pending_requests,
                            providerRequestPathsAfter=requests, seeds=seeds)
            write_json(output/'acceptance.json', accepted)
            print(json.dumps(accepted))
    except BaseException as error:
        if recovery and not (output/'owned-service-exits.json').exists():
            write_json(output/'owned-service-exits.json', {
                'native': native['process'].poll() if 'native' in locals() else 'not-started',
                'dsh': dsh.process.poll() if 'dsh' in locals() else 'not-started'})
        if 'native' in locals():
            (output/'native-diagnostic-tail.log').write_text(native['diagnostic_tail']())
        write_json(output/'failure.json', {'accepted':False,'errorType':type(error).__name__,
                   'startedAt':started,'finishedAt':time.time(),'seedCount':len(seeds)})
        raise


if __name__ == '__main__':
    main()
