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
        sampler.register(process,'agentkit')
    except Exception:
        if process.poll() is None:
            process.terminate();process.wait(timeout=5)
        for stream in (process.stdin,process.stdout,stderr):stream.close()
        raise

    def read():
        with (case/(label+'.stdout.jsonl')).open('x') as output:
            for line in process.stdout:
                output.write(line); output.flush()
                try:
                    rows.append(json.loads(line))
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
        receipt = {'pid':process.pid,'exitStatus':process.poll(),'elapsedNs':time.monotonic_ns()-start_ns,
            'ownedGroupAbsent':not remaining,'remainingProcessGroup':remaining,
            'outputReaderStopped':not reader.is_alive(),'parseErrors':errors,'error':failure}
        write(case/(label+'-exit.json'),receipt)
    if failure or process.returncode or remaining or reader.is_alive() or errors:
        raise ValueError('owned '+label+' did not complete with valid output and retired group')
    return rows,receipt


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
    sampler=ProcessSampler(case/'seed-rss.jsonl',pi_embedded=args.endpoint=='pi')
    sampler.register(os.getpid(),'provider');sampler.start()
    provider=model='endpoint-test'

    def start_fixture(resume=None):
        nonlocal context,fixture
        if args.endpoint=='native':
            canonical=json.loads(args.providers_config.read_text())
            providers=dict(canonical,providers=[canonical['providers'][0]])
            context=owned_native(args.kernel_supervisor.resolve(),args.kernel_worker.resolve(),
                providers,url,case,env,performance_case=case,resume_receipt=resume,
                on_process_started=lambda process:sampler.register(process,'harness'))
            fixture=context.__enter__()
            return {'kind':'native','endpointId':'native','url':fixture['url'],'bearer':fixture['bearer']}
        if args.endpoint=='dsh':
            dsh_home=case/'dsh-home'
            context=owned_dsh(args.dsh_root,case/'data',dsh_home,workspace,url,
                dict(env,HOME=str(dsh_home),DSH_HOME=str(dsh_home),DSH_AGENTS_HOME=str(dsh_home/'agents'),
                     DSH_TELEMETRY_DISABLED='1',AGENTKIT_SYNTHETIC_KEY='synthetic-key'),
                query_sqlite=False,performance_case=case,resume_receipt=resume,
                on_process_started=lambda process:sampler.register(process,'harness'))
            fixture=context.__enter__()
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
            context.__exit__(None,None,None);context=None
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
        event_contract=validate_events(events,args.mode,ids,args.endpoint)
        write(case/'diagnostic-events-summary.json',{'eventCount':len(events),
            'eventsByName':{name:sum(row.get('event')==name for row in events)
                            for name in {row.get('event') for row in events}},'seedProofs':seed_proofs,
            'eventContract':event_contract,'consumerExit':consumer_exit})
    except Exception as error:
        failure=type(error).__name__+': '+str(error)
        (case/'failure-traceback.log').write_text(traceback.format_exc())
    finally:
        try:
            if context is not None:context.__exit__(None,None,None)
        except Exception as error:
            failure=failure or 'owned fixture cleanup failed: '+type(error).__name__
        try:
            measured_rss=sampler.stop()
            write(case/'last-rss-summary.json',measured_rss)
        except Exception as error:
            failure=failure or 'owned RSS sampling failed: '+type(error).__name__
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
    write(case/'diagnostic-receipt.json',{'acceptedDiagnosticPlumbing':failure is None,
        'officialSamples':0,'performanceAccepted':False,'error':failure,
        'profileSha256':sha(args.profile),'programSha256':sha(program),'mode':args.mode,
        'endpoint':args.endpoint,'seedRss':seed_rss,'measurementRss':measured_rss,
        'nativeWindow':False,'physicalIme':False,'privateHarnessHistoryRead':False})
    print('PERFORMANCE_DIAGNOSTIC_'+('FAILED' if failure else 'COMPLETED')+' '+str(case))
    return 1 if failure else 0


if __name__=='__main__':
    raise SystemExit(main())
