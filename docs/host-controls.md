# Shared host controls

AgentKit owns the reusable SessionRuntime and presentation operations. The Web
service exposes them to the existing TekesWebUI; another host can consume the
same contracts without embedding browser rendering. ClientKit is planned for deprecation and is not a component or delivery dependency of this
path. Future native hosts consume AgentKit interfaces and state. Adding these
APIs does not itself validate a native App integration.

The delivery design and Web consumer are tracked by
[TekesWebUI #17](https://github.com/TekesApps/TekesWebUI/issues/17), with existing
AgentKit #37 (shared service), #42 (navigation), #44 (SideChat), #55/#56 (protocol)
and #66 (commands) retaining their own applicable acceptance scope.

## Session operations

Public endpoint descriptions advertise the exact supported methods and current
connection epoch. Consumers use that epoch when capturing an action. Reads and
mutations retain the original endpoint/session identity across awaits; an old
menu or callback cannot retarget a new connection. Mutations are not retried on
unknown outcomes.

| Methods | Contract |
| --- | --- |
| `agent.catalog`, `agent.command.run`, `agent.skills.compile` | Public command/Skill catalog, explicit command invocation, and compilation of selected Skills into ordinary input. Compilation itself does not send input. |
| `agent.tools.list/resolve`, `agent.resources.list/read` | Read-only tool details and session-bound Skill resources; preserve catalog digests and resource references. No tool installation, enabling or invocation. |
| `agent.queue.edit` | Original item/scope/epoch and supported attachment-preserving edit. Unsupported destructive edits are rejected. |
| `agent.goal.read/action` | Typed Goal state and advertised actions. Create/resume may start model work; do not treat them as reads. |
| `agent.feedback.read/action` | Public assistant message identity, displayed response witness and original opaque feedback CAS version. |
| `agent.settings.read/mutate` | Non-secret schema-backed settings, CAS updates and effective-time information. Kernel policy settings are not a general system-instruction editor. |
| `agent.context.read` | Context occupancy and authoritative details, separate from cumulative billing. Missing facts remain unknown. |
| `agent.threadSearch` | Title-index search with visibility and opaque cursor. It is distinct from local list filtering and message-body search. |
| `agent.preset.describe/preview` | Typed Kernel presets and dry-run resolution using host-authorized execution. Structured creation is mutually exclusive with legacy identity configuration. |
| `agent.recovery.read/run` | Explicit-interest recovery with real returned IDs and unresolved outcomes. Ready connection does not mean work resumed. Manual recovery may execute work. |
| `agent.sideChat.state/create/close/reconcile/prompt/cancel` | Existing shared SideChat owner and history lifetime. Explicit close waits for settlement before discard; switching views or disconnecting does not close execution. |

Harness-specific capabilities stay advertised as such. Native bounded child
sessions remain read-only. DSH does not gain emulated ephemeral forks. Pi's
current settings integration is read-only even where its upstream SDK has
separate setters. Existing Pi/DSH string presets remain distinct from Kernel
structured presets.

## WorkspaceService

WorkspaceService is independent of SessionRuntime and of the chosen Harness.
Rust hosts inject `WorkspaceServiceEndpoint` and register public identities via
`SharedAgent::register_workspace_service` and `register_workspace`.
TypeScript hosts consume `@tekesagentkit/client-contract/workspace`.

`workspace.list` returns registered identities. `workspace.call` carries
`connectionId`, `workspaceId`, a finite method, its business request and `rpcId`.
It never registers roots, credentials, URLs or executables. The service returns
`{result}` or the original structured `{error}`. The supplied HTTP client does
not follow redirects or retry mutations.

The current local Web host implements the same public contract with a private,
version-bound `tekes-workspace-service` helper. Its actual path is browser →
authenticated host → helper; it does not resurrect a removed Kernel HTTP route.
Host registration owns canonical roots and separate read, file-write and Git
grants. Overlapping registrations share helper lock authority without merging
their grants. Durable RPC reservations and receipts prevent an unknown mutation
from being executed again. File saves preserve the observed revision.

Turn changes require real per-turn producer evidence. The local host therefore
does not advertise that capability merely because Git diff or a helper method
exists. Kernel source changes are outside this delivery's authorization.

Harness adapters use their own public Files/Git capabilities when those are
actually available to an external UI. Model-internal tools and private modules
are not public consumer APIs. The local host helper described above is an
explicit host implementation, not a claim that Pi and DSH delegate their native
file operations to Kernel.

DSH session files use `agent.workspace.read` with endpoint, session and connection
epoch binding. The read-only capabilities/list/read projection delegates to
DSH public workspaceFiles and typed byte pages, preserving version and truncation
facts and original errors. It never falls back to the local host. Pi currently
has model file tools and Bash RPC, but no typed Files/Git RPC; the separately
labelled host service is an explicit choice. DSH per-turn change HTTP endpoints
are distinct from current Git operations and are not consumed in this release.
