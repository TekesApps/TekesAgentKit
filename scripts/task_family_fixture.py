"""Public-command seed plans and conservative source manifests for #23.

The builders describe calls already exercised by
``test-real-session-endpoints.py``. They do not launch a harness or create
synthetic identities. Pi callers must supply an existing parent reference and
the exact Pi connector configuration; see ``pi_seed_prerequisites``.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any


class TaskFamilyEvidenceError(ValueError):
    """Raised when returned evidence contradicts the requested ownership."""


_ENDPOINTS = {"native", "pi", "dsh"}


def _required_text(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(label + " must be a non-empty string")
    return value


def _cwd(value: str | Path) -> str:
    path = Path(value)
    if not path.is_absolute():
        raise ValueError("cwd must be absolute")
    return str(path)


def _parent(parent_ref: str) -> str:
    # Harness command streams allow a caller-owned alias (for example
    # ``parent``) as well as a public session ID. Preserve it verbatim; never
    # mint an identity or silently substitute another session.
    return _required_text(parent_ref, "parent_ref")


def native_task_seed_commands(
    parent_session_id: str,
    cwd: str | Path,
    model_provider: str,
    model_id: str,
) -> list[dict[str, Any]]:
    """Return the public Native Task + bounded-history seed sequence.

    ``parent_session_id`` is supplied by the caller, and is the actual
    session.create ID used by the established Native fixture. The task stays
    in this parent Run; only the read-only bounded history has a child ID.
    """
    parent_session_id = _required_text(parent_session_id, "parent_session_id")
    directory = _cwd(cwd)
    provider = _required_text(model_provider, "model_provider")
    model = _required_text(model_id, "model_id")
    return [
        {
            "op": "call",
            "method": "session.create",
            "args": {
                "sessionId": parent_session_id,
                "execution": {
                    "cwd": directory,
                    "read_roots": [],
                    "writable_roots": [],
                    "toolchain_roots": [],
                    "network": False,
                    "allowed_tools": ["task"],
                    "mcp": [],
                },
            },
            "saveAs": "parent",
        },
        {
            "op": "call",
            "sessionId": "parent",
            "method": "session.selectModel",
            "args": {"provider": provider, "model": model},
        },
        {
            "op": "call",
            "sessionId": "parent",
            "method": "approvals.select",
            "args": {"mode": "danger-full-access"},
        },
        {"op": "open", "sessionId": "parent"},
        {"op": "compose", "sessionId": "parent", "text": "ENDPOINT_NATIVE_CHILD"},
        {"op": "waitSettled", "sessionId": "parent"},
        {"op": "presentation", "sessionId": "parent"},
        {"op": "call", "method": "endpoint.capabilities"},
        {
            "op": "childCatalog",
            "sessionId": "parent",
            "saveAs": "bounded",
            # This is the same documented pointer extraction used by the
            # existing real fixture. The parser still checks the returned ID.
            "savePointer": "/entries/1/id",
        },
        {"op": "childHistory", "sessionId": "parent", "childSessionId": "bounded"},
        {"op": "call", "method": "subagents.history", "args": {
            "parentSessionId": "@parent", "childSessionId": "@bounded", "maxMessages": 50}},
        {"op": "reconnect"},
        {"op": "childHistory", "sessionId": "parent", "childSessionId": "bounded"},
    ]


def pi_subagent_seed_commands(
    parent_ref: str,
    cwd: str | Path,
    model_provider: str,
    model_id: str,
) -> list[dict[str, Any]]:
    """Return the established Pi SDK-child and model-subagent seed calls.

    ``parent_ref`` is the caller-selected saveAs alias for a parent created
    through the harness's public create command. The returned manifest must
    use the actual session ID returned by that create command.
    """
    parent = _parent(parent_ref)
    directory = _cwd(cwd)
    provider = _required_text(model_provider, "model_provider")
    model = _required_text(model_id, "model_id")
    return [
        {"op": "create", "cwd": directory, "saveAs": parent},
        {"op": "call", "sessionId": parent, "method": "session.selectModel",
         "args": {"provider": provider, "model": model}},
        {"op": "childCreate", "sessionId": parent, "label": "SDK child",
         "rpcId": "stable-create", "saveAs": "sdk-child"},
        {"op": "childPrompt", "sessionId": parent, "childSessionId": "sdk-child",
         "rpcId": "child-input", "content": [{"type": "text", "text": "CHILD_PI_SDK_PROOF"}]},
        {"op": "open", "sessionId": "sdk-child"},
        {"op": "waitSettled", "sessionId": "sdk-child"},
        {"op": "childRead", "sessionId": parent, "childSessionId": "sdk-child"},
        {"op": "open", "sessionId": parent},
        {"op": "compose", "sessionId": parent, "text": "ENDPOINT_PI_CHILD"},
        {"op": "waitSettled", "sessionId": parent},
        {"op": "presentation", "sessionId": parent, "saveAs": "model-child",
         "savePointer": "/summary/subagents/0/sessionId"},
        {"op": "childCatalog", "sessionId": parent},
        {"op": "childHistory", "sessionId": parent, "childSessionId": "model-child"},
        {"op": "reconnect"},
        {"op": "childRead", "sessionId": parent, "childSessionId": "sdk-child"},
    ]


def pi_seed_prerequisites() -> dict[str, Any]:
    """State the existing Pi setup requirements without copying its protocol."""
    return {
        "runtime": "Use the caller's already-installed, pinned Pi runtime.",
        "connector": "Use prepare_pi()'s existing isolated loopback provider config.",
        "modelSubagent": (
            "The AgentKit Pi connector must expose its existing tekes_subagent "
            "tool bridge; this helper does not install or redefine it."
        ),
        "additionalExtensionRequired": False,
        "sessionIdentity": "Read the actual ID from the public create result.",
        "verified": False,
        "note": "The helper itself does not inspect or launch the Pi runtime.",
    }


def dsh_subagent_seed_commands(
    parent_ref: str,
    cwd: str | Path,
    model_provider: str,
    model_id: str,
) -> list[dict[str, Any]]:
    """Return the existing public DSH catalog, child-prompt and history calls."""
    parent = _parent(parent_ref)
    directory = _cwd(cwd)
    provider = _required_text(model_provider, "model_provider")
    model = _required_text(model_id, "model_id")
    return [
        {"op": "create", "cwd": directory, "saveAs": parent},
        {"op": "call", "sessionId": parent, "method": "session.selectModel",
         "args": {"provider": provider, "model": model}},
        {"op": "open", "sessionId": parent},
        {"op": "compose", "sessionId": parent, "text": "ENDPOINT_SUBAGENT_CATALOG"},
        {"op": "waitSettled", "sessionId": parent},
        {"op": "childCatalog", "sessionId": parent, "saveAs": "child"},
        {"op": "open", "sessionId": "child"},
        {"op": "childPrompt", "sessionId": parent, "childSessionId": "child",
         "content": [{"type": "text", "text": "CHILD_CONTINUATION_PROOF"}]},
        {"op": "waitSettled", "sessionId": "child", "minimumTurn": 2},
        {"op": "reconnect"},
        {"op": "open", "sessionId": "child"},
        {"op": "presentation", "sessionId": "child"},
    ]


def _pointer(value, pointer):
    try:
        for part in pointer.lstrip('/').split('/'):
            part = part.replace('~1', '/').replace('~0', '~')
            value = value[int(part)] if isinstance(value, list) else value[part]
        return value
    except (KeyError, IndexError, TypeError, ValueError):
        raise TaskFamilyEvidenceError('public save pointer unavailable') from None


def public_exchanges(kind, commands, outputs):
    """Pair the exact executed command stream with headless's public results.

    This is descriptive evidence, never a capability or replacement dispatcher.
    headless emits one handshake then exactly one op/result per command. SaveAs
    follows its documented JSON pointers; opaque UI/tool IDs are never decoded.
    """
    if kind not in _ENDPOINTS:
        raise TaskFamilyEvidenceError('unknown harness kind')
    if not outputs or outputs[0].get('kind') != kind or outputs[0].get('ready') is not True:
        raise TaskFamilyEvidenceError('missing matching ready handshake')
    if len(outputs) != len(commands)+1:
        raise TaskFamilyEvidenceError('request/response count mismatch')
    variables, exchanges = {}, []
    for index, (command, row) in enumerate(zip(commands, outputs[1:])):
        if row.get('op') != command.get('op') or 'result' not in row:
            raise TaskFamilyEvidenceError('request/response operation mismatch')
        resolved = dict(command)
        for key in ['sessionId', 'childSessionId']:
            if key in resolved:
                resolved[key] = variables.get(resolved[key], resolved[key])
        if 'args' in command:
            def substitute(value):
                if isinstance(value, str) and value.startswith('@'):
                    if value[1:] not in variables:
                        raise TaskFamilyEvidenceError('unresolved public argument alias')
                    return variables[value[1:]]
                if isinstance(value, list): return [substitute(v) for v in value]
                if isinstance(value, dict): return {k:substitute(v) for k,v in value.items()}
                return value
            resolved['args'] = substitute(command['args'])
        exchanges.append({'index':index,'request':resolved,'result':row['result']})
        if 'saveAs' in command:
            pointer = command.get('savePointer', '/entries/0/id' if command['op']=='childCatalog' else '/sessionId')
            variables[command['saveAs']] = _pointer(row['result'], pointer)
    return exchanges


def _scope(view, endpoint, session):
    scope = (view.get('summary') or {}).get('scope') if isinstance(view, dict) else None
    expected = {'endpointId':endpoint,'threadId':session,'sessionId':session}
    if scope != expected:
        raise TaskFamilyEvidenceError('canonical Summary scope does not match requested Run')
    return scope


def parse_family_exchange(kind, commands, outputs, parent_session_id, cwd):
    """Validate returned public DTO shapes together with their actual requests.

    Callers still must preserve executed source/program/input identities and run
    the original UI dispatch. This helper never claims real or UI acceptance.
    """
    parent = _required_text(parent_session_id, 'parent_session_id')
    exchanges = public_exchanges(kind, commands, outputs)
    parent_scope = {'endpointId':kind,'threadId':parent,'sessionId':parent}
    manifest = {'schemaVersion':1,'endpoint':kind,'evidenceScope':'paired-public-seed-dtos',
                'realHarnessAcceptance':False,'uiAcceptance':False,
                'parent':{'scope':parent_scope,'cwd':_cwd(cwd)},
                'task':None,'child':None,'notVerified':['same-Task-row dynamic pointer producer','UI intent/dispatch/mount']}
    created = [e for e in exchanges if e['request']['op']=='create' or
               (e['request']['op']=='call' and e['request'].get('method')=='session.create')]
    if not any(isinstance(e['result'],dict) and e['result'].get('sessionId')==parent for e in created):
        raise TaskFamilyEvidenceError('actual parent create return missing')
    catalogs = [e for e in exchanges if e['request']['op']=='childCatalog' and e['request'].get('sessionId')==parent]
    if len(catalogs)!=1 or not isinstance(catalogs[0]['result'],dict):
        raise TaskFamilyEvidenceError('unique parent-bound catalog required')
    catalog = catalogs[0]['result']
    entries = catalog.get('entries')
    if not isinstance(entries,list) or not isinstance(catalog.get('parentAvailable'),bool):
        raise TaskFamilyEvidenceError('invalid public catalog schema')
    parent_views = [e for e in exchanges if e['request']['op']=='presentation' and e['request'].get('sessionId')==parent]
    if kind in ['native','pi']:
        if len(parent_views)!=1:
            raise TaskFamilyEvidenceError('canonical parent presentation required')
        view=parent_views[0]['result'];_scope(view,kind,parent)
        manifest['parent']['presentation']={'exchangeIndex':parent_views[0]['index'],
            'revision':view.get('revision'),'connectionEpoch':view.get('connectionEpoch')}
    if kind=='native':
        pages=[e for e in exchanges if e['request']['op']=='call' and e['request'].get('method')=='subagents.history']
        if len(pages)!=1: raise TaskFamilyEvidenceError('raw public bounded page required; TranscriptSnapshot has no sourceCall')
        exchange=pages[0];page=exchange['result'];request=exchange['request']['args']
        if not isinstance(page,dict) or request.get('parentSessionId')!=parent or page.get('parentSessionId')!=parent or page.get('childSessionId')!=request.get('childSessionId'):
            raise TaskFamilyEvidenceError('bounded page address does not match original request')
        child=page['childSessionId'];source=page.get('sourceCall')
        if not isinstance(source,dict) or source.get('name')!='task' or not source.get('callId'):
            raise TaskFamilyEvidenceError('bounded page source call missing')
        tools=[t for t in view.get('transcript',{}).get('tools',[]) if t.get('name')=='task' and t.get('arguments')==source.get('arguments')]
        tasks=[t for t in view.get('summary',{}).get('tasks',[]) if t.get('kind')=='task' and t.get('scope')==parent_scope and any(tool.get('id')==t.get('id') for tool in tools)]
        if len(tools)!=1 or len(tasks)!=1:
            raise TaskFamilyEvidenceError('actual canonical Task/tool row missing or ambiguous')
        manifest['task']={'row':tasks[0],'sourceCallId':source['callId'],'sourceExchangeIndex':exchange['index']}
        manifest['child']={'address':{'endpointId':kind,'sessionId':child},'parentScope':parent_scope,
            'readOnly':True,'independentRun':False,'historyRevision':page.get('historyRevision')}
    elif kind=='pi':
        candidates=[r for r in view.get('summary',{}).get('subagents',[]) if r.get('sessionId')]
        if len(candidates)!=1 or not candidates[0].get('id'):
            raise TaskFamilyEvidenceError('actual canonical subagent row required')
        row=candidates[0];child=row['sessionId']
        histories=[e for e in exchanges if e['request']['op']=='childHistory' and e['request'].get('sessionId')==parent and e['request'].get('childSessionId')==child]
        if len(histories)!=1 or not isinstance(histories[0]['result'],dict):
            raise TaskFamilyEvidenceError('original parent/child-bound history response missing')
        manifest['child']={'address':{'endpointId':kind,'sessionId':child},'parentScope':parent_scope,
            'summaryRow':row,'historyExchangeIndex':histories[0]['index']}
    else:
        views=[e for e in exchanges if e['request']['op']=='presentation' and e['request'].get('sessionId')!=parent]
        if len(views)!=1: raise TaskFamilyEvidenceError('unique original child presentation required')
        child=views[0]['request']['sessionId'];_scope(views[0]['result'],kind,child)
        manifest['child']={'address':{'endpointId':kind,'sessionId':child},'parentScope':parent_scope,
            'presentationExchangeIndex':views[0]['index']}
    children=[entry for entry in entries if isinstance(entry,dict) and entry.get('kind')=='child' and entry.get('id')==child]
    if child==parent or len(children)!=1:
        raise TaskFamilyEvidenceError('child identity absent or conflicts with parent catalog')
    entry=children[0]
    manifest['child'].update(catalogEntry=entry,catalogExchangeIndex=catalogs[0]['index'],
        catalogParentAvailable=catalog['parentAvailable'])
    # Record raw mode and parent availability. Never infer send capability from
    # a session ID or promote this descriptive manifest to control authority.
    manifest['child']['controlAuthorityGrantedByManifest']=False
    manifest['pairedPublicSourceMatched']=True
    return manifest
