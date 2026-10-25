#!/usr/bin/env python3
"""Diagnostic execution of one preregistered public-API performance workload.

This validates measurement plumbing before an official paired batch. It cannot
emit an accepted performance result or silently count diagnostics as cold/warm
samples. Programs/runtimes are explicit; no Cargo, desktop or private DB reads.
"""
from __future__ import annotations

import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import threading
import time
import traceback
import uuid

from dsh_owned_fixture import owned_dsh
from native_owned_fixture import owned_native
from pi_owned_fixture import prepare_pi
from performance_owned_receipt import IDENTITY_NAME, seal_closed_seed_receipt
from performance_process_sampler import ProcessSampler, classify, process_table
from performance_provider_fixture import (RequestStats, body_text, create_provider_handler,
    read_source_bytes, read_source_path, seed_commands)
from performance_public_readback import normalize_public_session
from performance_event_contract import validate_events

ROOT = Path('/Volumes/HP P900/dev/TekesAgentKit/.runtime-fixtures/stage-close-20261025/performance')


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write(path, value):
    with Path(path).open('x', encoding='utf-8') as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write('\n')


def machine_identity():
    commands = {'os':['/usr/bin/sw_vers'], 'hardware':['/usr/sbin/sysctl','hw.model','hw.memsize','hw.ncpu'],
                'power':['/usr/bin/pmset','-g','batt'], 'powerMode':['/usr/bin/pmset','-g','custom'],
                'thermal':['/usr/bin/pmset','-g','therm']}
    result = {'unixNs':time.time_ns(), 'monotonicNs':time.monotonic_ns(), 'loadAverage':os.getloadavg()}
    for name, command in commands.items():
        row = subprocess.run(command, capture_output=True, text=True, timeout=10)
        result[name] = {'exitStatus':row.returncode, 'stdout':row.stdout, 'stderr':row.stderr}
    return result


def group_rows(pid):
    result = subprocess.run(['/bin/ps','-axo','pid=,ppid=,pgid='], check=True,
                            capture_output=True,text=True,timeout=5)
    return [list(map(int,line.split())) for line in result.stdout.splitlines()
            if line.strip() and int(line.split()[2]) == pid]


def run_program(program, payload, case, label, env, sampler, *, timeout=300):
    rows, errors = [], []
    start_ns = time.monotonic_ns()
    stderr = (case/(label+'.stderr')).open('x')
    process = subprocess.Popen([str(program)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=stderr, text=True, env=env, cwd=case/'workspace', start_new_session=True)
    try:
        agentkit_root = sampler.register(process,'agentkit')
    except Exception:
        if process.poll() is None:
            process.terminate();process.wait(timeout=5)
        for stream in (process.stdin,process.stdout,stderr):stream.close()
        raise

    def read():
        with (case/(label+'.stdout.jsonl')).open('x') as output:
            with (case/(label+'-process-boundaries.jsonl')).open('x') as boundaries:
                for line in process.stdout:
                    output.write(line); output.flush()
                    try:
                        event = json.loads(line)
                        rows.append(event)
                        event_name = event.get('event')
                        if label == 'seed' and event.get('op') in ('presentation', 'snapshot'):
                            event_name = 'seed_public_' + event['op']
                        if event_name in ('consumer_started','runtime_ready','iteration_started','send',
                                          'cleanup_intent','runtime_cleanup_completed',
                                          'seed_public_presentation','seed_public_snapshot'):
                            try:
                                os_rows = process_table()
                                with sampler.lock:
                                    roots,known=dict(sampler.roots),dict(sampler.known)
                                    pi_embedded=sampler.pi_embedded
                                owned=classify(os_rows,roots,known,pi_embedded=pi_embedded)
                                compact=[{'pid':row['pid'],'startIdentity':row['startIdentity'],
                                          'role':row['role'],'commandName':Path(row['command']).name}
                                         for row in owned.values() if row['role']!='provider']
                                boundary={'rawEventIndex':len(rows)-1,'event':event_name,
                                    'iteration':event.get('iteration'),'round':event.get('round'),
                                    'sessionIndex':event.get('sessionIndex'),
                                    'originalIntentMonotonicNs':event.get('intentMonotonicNs'),
                                    'monotonicNs':time.monotonic_ns(),'osProcesses':compact,'error':None}
                            except Exception as error:
                                boundary={'rawEventIndex':len(rows)-1,'event':event_name,
                                    'iteration':event.get('iteration'),'round':event.get('round'),
                                    'sessionIndex':event.get('sessionIndex'),
                                    'monotonicNs':time.monotonic_ns(),'osProcesses':[],
                                    'error':type(error).__name__+': '+str(error)}
                                errors.append('could not observe OS process boundary')
                            boundaries.write(json.dumps(boundary)+'\n');boundaries.flush()
                    except (ValueError,TypeError):
                        errors.append('non-JSON original consumer output')

    reader = threading.Thread(target=read, name='performance-original-output', daemon=True)
    reader.start()
    failure = None
    try:
        process.stdin.write(json.dumps(payload));process.stdin.close()
        process.wait(timeout=timeout)
    except Exception as error:
        failure = type(error).__name__ + ': ' + str(error)
    finally:
        if process.poll() is None:
            os.killpg(process.pid,signal.SIGTERM)
            try:process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
        deadline = time.monotonic()+10
        while group_rows(process.pid) and time.monotonic()<deadline:time.sleep(.05)
        remaining = group_rows(process.pid)
        # Do not signal a detached group after its root exited; preserve the
        # failed cleanup and identities for coordinator inspection.
        reader.join(timeout=10);stderr.close()
        receipt = {'pid':process.pid,'startIdentity':agentkit_root['startIdentity'],
            'spawnMonotonicNsLowerBound':start_ns,
            'exitStatus':process.poll(),'elapsedNs':time.monotonic_ns()-start_ns,
            'ownedGroupAbsent':not remaining,'remainingProcessGroup':remaining,
            'outputReaderStopped':not reader.is_alive(),'parseErrors':errors,'error':failure,
            'processBoundaries':str(case/(label+'-process-boundaries.jsonl'))}
        write(case/(label+'-exit.json'),receipt)
    if failure or process.returncode or remaining or reader.is_alive() or errors:
        raise ValueError('owned '+label+' did not complete with valid output and retired group')
    return rows,receipt


def validate_warm_process_identity(endpoint, mode, boundaries, rss_rows, rss_summary,
                                   harness_roots):
    """Fail closed unless OS identities remain continuously visible in warm samples."""
    if mode != 'warm':
        return {'status':'not_applicable','reason':'steady-warm identity rule applies only to warm mode'}
    if not isinstance(boundaries,list) or not isinstance(rss_rows,list) or not isinstance(harness_roots,list):
        raise ValueError('warm process identity evidence is incomplete')
    starts=[row for row in boundaries if row.get('event')=='iteration_started']
    if len(starts)!=11 or [row.get('iteration') for row in starts]!=list(range(11)):
        raise ValueError('warm process observations require initialization and ten measured iteration boundaries')
    ready=[row for row in boundaries if row.get('event')=='runtime_ready']
    if len(ready)!=1 or ready[0].get('error') or not isinstance(ready[0].get('osProcesses'),list):
        raise ValueError('warm process observations require one initialized runtime-ready boundary')
    if any(row.get('error') or not isinstance(row.get('osProcesses'),list) for row in starts):
        raise ValueError('warm iteration boundary lacks an OS process snapshot')
    ready_ns=ready[0].get('monotonicNs')
    start_ns=starts[0].get('monotonicNs')
    sends=[row for row in boundaries if row.get('event')=='send' and row.get('round')==10]
    if len(sends)!=2 or {row.get('sessionIndex') for row in sends}!={0,1}:
        raise ValueError('warm final round is missing one original session send boundary')
    if any(row.get('error') or not isinstance(row.get('osProcesses'),list) for row in sends):
        raise ValueError('warm final send boundary lacks an OS process snapshot')
    end_ns=max(row.get('monotonicNs',0) for row in sends)
    if (type(ready_ns) is not int or type(start_ns) is not int or type(end_ns) is not int
            or ready_ns>start_ns or end_ns<=start_ns):
        raise ValueError('warm process interval has invalid monotonic timestamps')

    def identities(rows, *, node_only=False):
        return {(row.get('pid'),row.get('startIdentity')) for row in rows
                if row.get('role')=='harness'
                and (not node_only or row.get('commandName')=='node')}

    baseline_rows=starts[0]['osProcesses']
    if endpoint=='pi':
        # Persistent identities are the intersection across every round boundary.
        # A short-lived `pi --version` Node process cannot qualify as an RPC merely
        # because it happened to overlap one boundary or one 50ms RSS sample.
        persistent_sets=[identities(row['osProcesses'],node_only=True) for row in starts]
        expected=set.intersection(*persistent_sets) if persistent_sets else set()
        if len(expected)!=2:
            raise ValueError('Pi steady-warm must expose exactly two persistent per-session Node RPC identities')
    else:
        expected={(row.get('pid'),row.get('startIdentity')) for row in harness_roots
                  if row.get('role')=='harness'}
        if len(expected)!=1:
            raise ValueError('external harness must have exactly one measured root process identity')
    if any(not expected.issubset(identities(row['osProcesses'],node_only=endpoint=='pi'))
           for row in starts):
        raise ValueError('long-lived harness identity changed across a warm iteration boundary')
    if endpoint!='pi' and not expected.issubset(identities(baseline_rows)):
        raise ValueError('external harness root is missing at warm initialization boundary')
    relevant=[sample for sample in rss_rows
              if start_ns<=sample.get('monotonicNs',0)<=end_ns]
    if not relevant:
        raise ValueError('warm RSS stream has no samples across the measured identity interval')
    for sample in relevant:
        process_rows=sample.get('processes')
        if not isinstance(process_rows,list):
            raise ValueError('warm RSS sample lacks per-process OS identities')
        live={(row.get('pid'),row.get('startIdentity')) for row in process_rows
              if row.get('role')=='harness'
              and (endpoint!='pi' or Path(row.get('command','')).name=='node')}
        if not expected.issubset(live):
            raise ValueError('long-lived harness identity disappeared from a 50ms RSS observation')
    max_gap=rss_summary.get('maxObservedIntervalNs') if isinstance(rss_summary,dict) else None
    if type(max_gap) is not int or max_gap<=0 or max_gap>250_000_000:
        raise ValueError('warm RSS cadence gap exceeds the 250ms identity observation bound')
    return {'status':'verified','endpoint':endpoint,'iterationBoundaries':11,
            'measuredSendBoundaries':len(sends),'continuousRssSamples':len(relevant),
            'expectedLongLivedHarnessIdentities':[{'pid':pid,'startIdentity':start}
                for pid,start in sorted(expected)],'intervalStartMonotonicNs':start_ns,
            'intervalEndMonotonicNs':end_ns,'samplingGapBoundNs':250_000_000}


def verify_owned_retirement(sampler, consumer_exit, boundaries, cleanup_limit_ms,
                            fixture_lifecycle):
    """Cross-check sampled known children and a fresh OS census after close."""
    cleanup=[row for row in boundaries if row.get('event')=='cleanup_intent']
    if len(cleanup)!=1 or cleanup[0].get('error'):
        raise ValueError('original consumer cleanup intent is missing or unobserved')
    completed=[row for row in boundaries if row.get('event')=='runtime_cleanup_completed']
    if len(completed)!=1 or completed[0].get('error'):
        raise ValueError('original runtime cleanup completion is missing or unobserved')
    if completed[0].get('monotonicNs',0)<cleanup[0].get('monotonicNs',0):
        raise ValueError('runtime cleanup completion precedes the cleanup intent')
    if consumer_exit.get('exitStatus')!=0 or consumer_exit.get('ownedGroupAbsent') is not True:
        raise ValueError('original AgentKit consumer did not exit with its process group')
    if any(row.get('exitStatus')!=0 for row in fixture_lifecycle
           if row.get('operation') in ('seed_retire','measurement_retire')):
        raise ValueError('an explicitly owned external harness root did not exit successfully')
    with sampler.lock:
        roots,known=dict(sampler.roots),dict(sampler.known)
        pi_embedded=sampler.pi_embedded
    census=process_table()
    current=classify(census,roots,known,pi_embedded=pi_embedded)
    active=[{'pid':row['pid'],'startIdentity':row['startIdentity'],'role':row['role']}
            for row in current.values() if row['role'] in ('agentkit','harness')]
    checked=time.monotonic_ns()
    spawn_lower=consumer_exit.get('spawnMonotonicNsLowerBound')
    relative_intent=cleanup[0].get('originalIntentMonotonicNs')
    if type(spawn_lower) is not int or type(relative_intent) is not int or relative_intent < 0:
        raise ValueError('original cleanup intent lacks its monotonic lower-bound clock mapping')
    # Rust's monotonic origin occurs after Popen begins. This lower bound
    # conservatively includes process startup; delayed stdout/ps observation
    # must never make cleanup look faster than the original intent.
    intent_lower=spawn_lower+relative_intent
    elapsed=checked-intent_lower
    if elapsed < 0:
        raise ValueError('original cleanup clock lower bound follows the OS census')
    if active:
        raise ValueError('owned AgentKit or harness processes remain after cleanup: '+str(active))
    if elapsed>cleanup_limit_ms*1_000_000:
        raise ValueError('owned process retirement exceeded the preregistered cleanup budget')
    return {'status':'verified','cleanupIntentObservedMonotonicNs':cleanup[0]['monotonicNs'],
        'cleanupIntentMonotonicNsLowerBound':intent_lower,
        'cleanupDurationIsConservativeUpperBound':True,
        'runtimeCleanupCompletedMonotonicNs':completed[0]['monotonicNs'],
        'allOwnedRootsExitedMonotonicNs':checked,'cleanupElapsedNs':elapsed,
        'cleanupDeadlineMs':cleanup_limit_ms,'ownedAgentKitOrHarnessProcessesRemaining':active,
        'sampledOwnedProcessesFinalCensus':True,'providerProcessesExcluded':True}


def fixture_root_observation(endpoint, fixture, sampler):
    if endpoint=='native':
        process=fixture['process']
    elif endpoint=='dsh':
        process=fixture.process
    else:
        return None
    root=sampler.register(process,'harness')
    return {'pid':root['pid'],'startIdentity':root['startIdentity'],'role':'harness'}


def completed_jsonl_rows(path):
    """Read only complete newline-terminated rows from an active sampler file."""
    data=Path(path).read_bytes()
    lines=data.splitlines(keepends=True)
    if lines and not lines[-1].endswith((b'\n',b'\r')):
        lines.pop()
    return [json.loads(line) for line in lines]


def validate_program(program, receipt_path):
    program = program.resolve(strict=True)
    receipt = json.loads(receipt_path.read_text())
    if Path(receipt['path']).resolve(strict=True)!=program or receipt['sha256']!=sha(program):
        raise ValueError('program does not match its explicit immutable receipt')
    manifest_path=Path(receipt['sourceManifest'])
    if sha(manifest_path)!=receipt['sourceManifestSha256']:
        raise ValueError('program source manifest identity changed')
    manifest=json.loads(manifest_path.read_text())
    source=Path(manifest['source'])
    if (manifest['head']!=receipt['sourceCommit']
            or any(sha(source/path)!=digest for path,digest in manifest['files'].items())):
        raise ValueError('frozen source inputs differ from program build receipt')
    return program,receipt


def runtime_identity(args):
    node_sha='9d050fd455b56426e25d4d603c7c501cbb2630348e836cf221dcce748e90588a'
    if args.endpoint=='native':
        files=[(args.kernel_supervisor,'365eca9385f36d5897a81ba398e7f34b4b9819896d6a1b78c618a7710b14380f'),
               (args.kernel_worker,'1284014bd0a8d3cb711c0512b10762322a19af4beabc1f9e681bacb1e28fa64d'),
               (args.providers_config,'0caac12724fd6d982fc51123ea4546f2a023d6a9258da4cde63d1d27eabcaa6a')]
    elif args.endpoint=='pi':
        files=[(args.pi_root/'node/bin/node',node_sha),
               (args.pi_root/'node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js',
                'e79626f2dd6f94aa45d30f3fa63cd84319a6eefcd150b353cfaf274366926774'),
               (args.pi_root/'node_modules/@earendil-works/pi-coding-agent/package.json',
                '83c3bb7619f590c213cc96a24216eb29a819a3ef6dc548607cc037ed276e8dc5')]
    else:
        files=[(args.dsh_root/'node/bin/node',node_sha),
               (args.dsh_root/'node_modules/@deepseek-ai/dsh/lib/bin.js',
                '80112efe368efb1a39720c7ada0af00442e5131fb2744edaaff8a3e8bb499e6f'),
               (args.dsh_root/'node_modules/@deepseek-ai/dsh/package.json',
                'bea4288aa88eb54efe829686553100a75fdf09d11c5e3bf6b329badb9a0368dd')]
    result={}
    for path,expected in files:
        digest=sha(path)
        if digest!=expected:raise ValueError('fixed runtime critical file identity mismatch: '+str(path))
        result[str(path.resolve(strict=True))]=digest
    return {'criticalFiles':result,'completeDependencyClosureClaimed':False,
            'versions':{'native':'Kernel0.9.3/09508a9','pi':'Pi1.3.0/Node24.20.0',
                        'dsh':'DSH0.2.1-rc.2/Node24.20.0'}[args.endpoint]}


def public_seed(kind, config, program, workspace, case, env, sampler, provider, model):
    ids = [str(uuid.uuid4()),str(uuid.uuid4())] if kind=='native' else ['s0','s1']
    commands=[]
    for i,session_id in enumerate(ids):
        if kind=='native':
            commands.append({'op':'call','method':'session.create','args':{'sessionId':session_id,
                'execution':{'cwd':str(workspace),'read_roots':[str(workspace)],'writable_roots':[],
                    'toolchain_roots':[],'network':False,'allowed_tools':['read'],'mcp':[]}}})
        else:commands.append({'op':'create','cwd':str(workspace),'saveAs':session_id})
    for i,session_id in enumerate(ids):
        commands.extend(seed_commands(session_id,provider,model,i))
    for session_id in ids:
        # Finite complete readback of this fixed-size seed, not measurement.
        commands += [{'op':'earlier','sessionId':session_id} for _ in range(8)]
        commands += [{'op':'presentation','sessionId':session_id},{'op':'snapshot'}]
    write(case/'seed-commands.json',commands)
    rows,_ = run_program(program,dict(config,commands=commands),case,'seed',env,sampler)
    expected={'native':'0.9.3','pi':'1.3.0','dsh':'0.2.1-rc.2'}[kind]
    if not rows or rows[0].get('version')!=expected:raise ValueError('seed actual runtime handshake mismatch')
    if kind!='native':
        ids=[row['result']['sessionId'] for row in rows if row.get('op')=='create']
    presentations=[row['result'] for row in rows if row.get('op')=='presentation']
    snapshots=[row['result'] for row in rows if row.get('op')=='snapshot']
    if len(ids)!=2 or len(presentations)!=2 or len(snapshots)!=2:
        raise ValueError('public complete seed readback missing one original session')
    proofs=[]
    for i,session_id in enumerate(ids):
        scope={'endpointId':kind,'threadId':session_id,'sessionId':session_id}
        proof=normalize_public_session(presentations[i],snapshots[i]['sessions'][session_id],
            run_scope=scope,session_index=i)
        proofs.append(proof)
    write(case/'seed-public-readback.json',proofs)
    return ids,proofs


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--endpoint',choices=('native','pi','dsh'),required=True)
    parser.add_argument('--mode',choices=('cold','warm','reconnectLifecycle'),required=True)
    parser.add_argument('--case-name',required=True)
    for name in ('program','program-receipt','seeder','seeder-receipt','profile'):
        parser.add_argument('--'+name,type=Path,required=True)
    for name in ('kernel-supervisor','kernel-worker','providers-config','pi-root','dsh-root'):
        parser.add_argument('--'+name,type=Path)
    args=parser.parse_args()
    if not re.fullmatch('[a-z0-9][a-z0-9-]{1,79}',args.case_name):parser.error('invalid owned case name')
    ROOT.mkdir(parents=True,exist_ok=True)
    case=ROOT/args.case_name
    if case.exists():parser.error('owned diagnostic case already exists; evidence is never overwritten')
    program,program_receipt=validate_program(args.program,args.program_receipt)
    seeder,seeder_receipt=validate_program(args.seeder,args.seeder_receipt)
    required={'native':('kernel_supervisor','kernel_worker','providers_config'),
              'pi':('pi_root',),'dsh':('dsh_root',)}[args.endpoint]
    if any(getattr(args,key) is None for key in required):parser.error('missing explicit fixed runtime')
    runtime_before=runtime_identity(args)
    case.mkdir(mode=0o700);workspace=case/'workspace';workspace.mkdir()
    read_source_path(workspace).write_bytes(read_source_bytes())
    home=case/'host-home';home.mkdir();tmp=case/'tmp';tmp.mkdir()
    env={'PATH':'/usr/bin:/bin:/usr/sbin:/sbin','HOME':str(home),'TMPDIR':str(tmp),'LANG':'en_US.UTF-8'}
    write(case/'inputs.json',{'diagnosticOnly':True,'officialSamples':0,
        'program':program_receipt,'seeder':seeder_receipt,'profileSha256':sha(args.profile),
        'runtimeCriticalBefore':runtime_before,
        'mode':args.mode,'endpoint':args.endpoint,'machineBefore':machine_identity()})
    stats=RequestStats();handler=create_provider_handler(workspace,stats,endpoint=args.endpoint)
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),handler)
    server.daemon_threads=False;server.block_on_close=True
    thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
    url=f'http://127.0.0.1:{server.server_port}/v1'
    failure=None;config=None;fixture=None;context=None;seed_rss=None;measured_rss=None
    fixture_lifecycle=[];fixture_root_identities=[];boundaries=[];consumer_exit=None
    sampler=ProcessSampler(case/'seed-rss.jsonl',pi_embedded=args.endpoint=='pi')
    sampler.register(os.getpid(),'provider');sampler.start()
    provider=model='endpoint-test'

    def start_fixture(resume=None):
        nonlocal context,fixture
        start_intent=time.monotonic_ns()
        if args.endpoint=='native':
            canonical=json.loads(args.providers_config.read_text())
            providers=dict(canonical,providers=[canonical['providers'][0]])
            context=owned_native(args.kernel_supervisor.resolve(),args.kernel_worker.resolve(),
                providers,url,case,env,performance_case=case,resume_receipt=resume,
                on_process_started=lambda process:sampler.register(process,'harness'))
            fixture=context.__enter__()
            identity=fixture_root_observation(args.endpoint,fixture,sampler)
            fixture_root_identities.append(identity)
            fixture_lifecycle.append({'operation':'resume' if resume else 'start',
                'intentMonotonicNs':start_intent,'readyMonotonicNs':time.monotonic_ns(),
                'process':identity,'exitStatus':None})
            return {'kind':'native','endpointId':'native','url':fixture['url'],'bearer':fixture['bearer']}
        if args.endpoint=='dsh':
            dsh_home=case/'dsh-home'
            context=owned_dsh(args.dsh_root,case/'data',dsh_home,workspace,url,
                dict(env,HOME=str(dsh_home),DSH_HOME=str(dsh_home),DSH_AGENTS_HOME=str(dsh_home/'agents'),
                     DSH_TELEMETRY_DISABLED='1',AGENTKIT_SYNTHETIC_KEY='synthetic-key'),
                query_sqlite=False,performance_case=case,resume_receipt=resume,
                on_process_started=lambda process:sampler.register(process,'harness'))
            fixture=context.__enter__()
            identity=fixture_root_observation(args.endpoint,fixture,sampler)
            fixture_root_identities.append(identity)
            fixture_lifecycle.append({'operation':'resume' if resume else 'start',
                'intentMonotonicNs':start_intent,'readyMonotonicNs':time.monotonic_ns(),
                'process':identity,'exitStatus':None})
            return dict(fixture.connector_config([]),endpointId='dsh')
        raise ValueError('Pi has no separately-owned external fixture process')

    try:
        if args.endpoint=='pi':
            config=prepare_pi(args.pi_root,case/'data',case/'pi-home',url,strict_environment=True)
            config.update(endpointId='pi')
        else:
            config=start_fixture()
            if args.endpoint=='native':
                providers=json.loads(args.providers_config.read_text())
                provider=providers['providers'][0]['id'];model=providers['providers'][0]['models'][0]['id']
        ids,seed_proofs=public_seed(args.endpoint,config,seeder,workspace,case,env,sampler,provider,model)
        if args.mode=='cold' and context is not None:
            old_process=fixture['process'] if args.endpoint=='native' else fixture.process
            fixture_stop_intent=time.monotonic_ns()
            context.__exit__(None,None,None);context=None
            fixture_lifecycle.append({'operation':'seed_retire','intentMonotonicNs':fixture_stop_intent,
                'completedMonotonicNs':time.monotonic_ns(),
                'process':next((row for row in reversed(fixture_root_identities)
                                if row['pid']==old_process.pid),None),
                'exitStatus':old_process.poll()})
            seed_rss=sampler.stop();write(case/'seed-rss-summary.json',seed_rss)
            owned=[row for row in seed_rss['knownProcesses'] if row['role']!='provider']
            closed=seal_closed_seed_receipt(case/IDENTITY_NAME,exit_code=old_process.poll(),known_processes=owned)
            sampler=ProcessSampler(case/'measurement-rss.jsonl',pi_embedded=False)
            sampler.register(os.getpid(),'provider');sampler.start()
            config=start_fixture(closed)
            new_process=fixture['process'] if args.endpoint=='native' else fixture.process
            if new_process.pid==old_process.pid:raise ValueError('cold fixture did not obtain a new process PID')
            write(case/'cold-harness-restart.json',{'oldPid':old_process.pid,'oldExit':old_process.poll(),
                'newPid':new_process.pid,'sealedReceiptSha256':sha(closed),'publicSeedReadbackVerified':True})
        else:
            seed_rss=sampler.stop();write(case/'seed-rss-summary.json',seed_rss)
            sampler=ProcessSampler(case/'measurement-rss.jsonl',pi_embedded=args.endpoint=='pi')
            sampler.register(os.getpid(),'provider')
            if fixture is not None:
                sampler.register(fixture['process'] if args.endpoint=='native' else fixture.process,'harness')
            sampler.start()
        rounds=1 if args.mode=='cold' else 11
        prompts=[[body_text('measure',i,round,'user') for i in (0,1)] for round in range(rounds)]
        replies=[[body_text('measure',i,round,'assistant') for i in (0,1)] for round in range(rounds)]
        payload={'mode':args.mode,'connector':config,'sessionIds':ids,
                 'iterationPrompts':prompts,'iterationReplies':replies}
        events,consumer_exit=run_program(program,payload,case,'consumer',env,sampler,timeout=600)
        boundaries=[json.loads(line) for line in (case/'consumer-process-boundaries.jsonl').read_text().splitlines()]
        event_contract=validate_events(events,args.mode,ids,args.endpoint)
        warm_identity=validate_warm_process_identity(args.endpoint,args.mode,boundaries,
            completed_jsonl_rows(case/'measurement-rss.jsonl'),
            {'maxObservedIntervalNs':sampler.max_interval_ns},fixture_root_identities)
        write(case/'diagnostic-events-summary.json',{'eventCount':len(events),
            'eventsByName':{name:sum(row.get('event')==name for row in events)
                            for name in {row.get('event') for row in events}},'seedProofs':seed_proofs,
            'eventContract':event_contract,'consumerExit':consumer_exit,
            'warmProcessIdentity':warm_identity})
    except Exception as error:
        failure=type(error).__name__+': '+str(error)
        (case/'failure-traceback.log').write_text(traceback.format_exc())
    finally:
        try:
            if context is not None:
                process=fixture['process'] if args.endpoint=='native' else fixture.process
                fixture_stop_intent=time.monotonic_ns()
                context.__exit__(None,None,None)
                fixture_lifecycle.append({'operation':'measurement_retire',
                    'intentMonotonicNs':fixture_stop_intent,'completedMonotonicNs':time.monotonic_ns(),
                    'process':next((row for row in reversed(fixture_root_identities)
                                    if row['pid']==process.pid),None),
                    'exitStatus':process.poll()})
        except Exception as error:
            failure=failure or 'owned fixture cleanup failed: '+type(error).__name__
        try:
            if consumer_exit is None and (case/'consumer-exit.json').is_file():
                consumer_exit=json.loads((case/'consumer-exit.json').read_text())
            if consumer_exit is not None:
                boundaries=[json.loads(line) for line in
                    (case/'consumer-process-boundaries.jsonl').read_text().splitlines()]
                retirement=verify_owned_retirement(sampler,consumer_exit,boundaries,
                    json.loads(args.profile.read_text())['thresholds']['ownedCleanupDeadlineMs'],
                    fixture_lifecycle)
                write(case/'owned-process-retirement.json',retirement)
        except Exception as error:
            failure=failure or 'owned process retirement proof failed: '+type(error).__name__
        try:
            measured_rss=sampler.stop()
            write(case/'last-rss-summary.json',measured_rss)
        except Exception as error:
            failure=failure or 'owned RSS sampling failed: '+type(error).__name__
        write(case/'fixture-lifecycle.json',fixture_lifecycle)
        server.shutdown();server.server_close();thread.join(timeout=5)
        provider_result=stats.snapshot();write(case/'provider-observation.json',provider_result)
        if thread.is_alive() or provider_result['errorResponses']:
            failure=failure or 'provider failed or did not retire'
        write(case/'machine-after.json',machine_identity())
        try:
            if runtime_identity(args)!=runtime_before:
                failure=failure or 'fixed runtime inputs changed during diagnostic'
            validate_program(program,args.program_receipt)
            validate_program(seeder,args.seeder_receipt)
        except Exception as error:
            failure=failure or 'program/source/runtime identity recheck failed: '+str(error)
        if consumer_exit is not None:
            write(case/'consumer-process-boundaries.summary.json',{
                'source':consumer_exit['processBoundaries'],'boundaries':boundaries,
                'rawConsumerEventsUnmodified':True})
    write(case/'diagnostic-receipt.json',{'acceptedDiagnosticPlumbing':failure is None,
        'officialSamples':0,'performanceAccepted':False,'error':failure,
        'profileSha256':sha(args.profile),'programSha256':sha(program),'mode':args.mode,
        'endpoint':args.endpoint,'seedRss':seed_rss,'measurementRss':measured_rss,
        'nativeWindow':False,'physicalIme':False,'privateHarnessHistoryRead':False})
    print('PERFORMANCE_DIAGNOSTIC_'+('FAILED' if failure else 'COMPLETED')+' '+str(case))
    return 1 if failure else 0


if __name__=='__main__':
    raise SystemExit(main())
