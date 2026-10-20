# Agent UI runtime bindings (experimental)

`RunBinding` binds one mounted pane to existing `SessionRuntime`, `RunController`,
`ChildRun` and `SideChatController` authority. It owns observations, not windows or
another Run state machine. Public RunView/TranscriptContent/ReadOnlyTranscript are
supplied by the components package; full reading/rich-content acceptance remains
experimental. The workspace consumes an exact published TekesUI revision.

`Arc<RunBinding>::read_feedback()` returns a validated, original-binding
`FeedbackFrame`; use `is_current(source)` before adopting committed records.
Read order and connection epoch are separate from opaque server CAS versions.
`TurnFeedbackTarget::read(section)` requires a proven public final-message ID,
and bounded child history is not treated as an independent feedback session.
Cold/failed reads are not an empty list, and advertised put/delete facts are not
mutation permission. Rating writes and controls are pending; see the
[feedback read contract](../../docs/rust-agent-ui-feedback-read-20261020.md).

Choose the source explicitly:

- `open_main`: a main session; acquires its own history lease.
- `open_task`: an already resolved `AgentTask`. Resolve through `TaskPanelState`
  first; loading/missing/error is a pane state, never a fallback new conversation.
- `open_child`: pass the actual direct parent, child ID and `ChildHistoryLayout`.
  Independent history uses `RunController::for_child`; refresh the existing child
  catalog before enabling send, and on relevant reconnect/control changes.
  Same-session bounded history is read-only. Its explicit source or endpoint
  bounded-history API owns history; it never displays the parent's transcript.
- `open_side_chat`: pass the shared owner and owned pointer. Missing/foreign
  pointers fail mounting; disappearance after mounting produces `Missing`.

Keep a binding with the retained pane. Before installing asynchronous content,
validate the existing `SidePanel` content token. A task pointer change retires the
old source even when task ID and tab header remain the same. Drop releases this
mount's observation; it does not cancel work or discard an ephemeral session.

On UI events, drain `Composer::take_intents` before applying newer snapshots:

- `Edit`: call `edit_text` with the captured destination. This patches text only.
- `Send`: call `send_with` with that destination, explicit prompt mode and the
  host's existing `SubmissionPreparation`. It retains RunController admission
  identity and uncertainty behavior and never retries automatically.
  Native hosts can instead call `prepare_send` synchronously, sync the controlled
  clear, then spawn `dispatch_prepared_with`. If the host abandons that prepared
  ticket before dispatch, resolve it with `cancel_prepared`; don't silently drop
  a reserved submission. Callbacks must retain the original binding and ticket.
- `Cancel`: call `cancel` with the captured run destination.
- `OpenEntry`: invoke the host's attachment/model/preset capability UI; commit
  results through existing presentation token validation. Entry UI is not
  implemented by this binding.

`sync_composer` reads the canonical snapshot and restricts controls during
SideChat Closing/Unknown. Send validates pointer authority before preparation
and after the host callback returns. Missing panes must unmount their body;
bounded children do not get a fabricated Composer snapshot. The final transport
still uses the endpoint's capability and ownership checks.

Use `subscribe_run`, `subscribe_runtime` and `subscribe_side_chat` to schedule
updates via the host's public window wake integration. Read `snapshot` when
notified; don't poll per frame. Native bounded history refreshes/paginates through
its existing `ChildRun`. SideChat reads/no-op reconciliation don't publish wakes.

Tests in `ui/demo/tests/run_binding.rs` use an isolated local SEP fixture. They
cover actual stream/cancel, lease coexistence, pagination, read-only child source,
SideChat cancellation-to-Unknown, pointer removal and host-preparation fencing.
They do not prove physical IME, native window UX or real three-harness UI parity.

## SideChat tab authority

Keep one `SideChatBinding` per endpoint runtime and share it across retained
parents. `input(revision, parent, non_scratch_tabs, selected)` appends scratch tabs
from the existing controller, and supplies the **complete** controller snapshot
to `SidePanel`. Do not filter that snapshot to the active parent: offscreen panes
also need confirmed pointer removal to retire retained content. Support is
`None` until the runtime is ready, then comes from the controller's capability
policy. Unsupported endpoints must not show an enabled create control.

Create with the captured parent and one explicit RPC ID. Open via `open(&chat)`;
the returned `BoundSideChatRun` proves the binding's role and original pointer
authority. Do not substitute a main-run cache entry with the same session ID.
Keep the binding and its Composer instance with that pane, observe both its run
and the shared owner, and use `binding.access().composer_gate()` when updating
the Composer. Captured callbacks retain their original binding and destination.
Late creation may add a tab to its original parent but must not change a newer
navigation selection or interrupt composition.

Close with `close(&expected_chat, rpc_id)`, not a session ID freshly looked up
after an async callback. The controller checks the expected pointer under the
same lock that admits Closing, cancels/settles the child, then discards it.
Only confirmed discard/authoritative absence retires its body and local draft.
Closing/Unknown retains readable content; its gate disables mutations. Failed
reads keep the pointer. Reconciliation takes an explicit authoritative host read
result; neither an absent inventory row (ephemeral sessions are hidden) nor a
generic locator's success proves existence. The demo supplies no automatic
Unknown resolver and never reads a harness's private database.

The `--components` demo mounts a separate public Composer and Indicators for an
owned scratch run. Transcript remains pending upstream #22/#23, so this is not
a complete SideChat RunView. `ui/demo/tests/sidechat_binding.rs` covers parent
isolation, late creation, streaming/cancel/discard, stale close callbacks, and
interrupted discard with explicit reconciliation. The native fixture script
exercises its own scratch transport and leaves both parent drafts unchanged.

## Task pane resolution

`TaskPaneBinding::new(runtime)` (or `for_run(parent)`) owns the existing
`TaskPanelState` plus the mounted run lifetime. Begin a selection with the actual
parent and task ID; commit the host's row query using the returned `TaskLoadToken`.
Keep observations serial within that selection; begin a new load when replacing
or cancelling it. The host supplies task lookup/observation explicitly, without
reading a harness-private database. `snapshot`, `subscribe` and `tab(title)` expose
the canonical loading/missing/error/value state to the shared SidePanel.

`open_resolved().await` returns no binding for loading/missing/error, and fences a
late history baseline against source replacement. Name/status updates preserve
the exact run/Composer draft; pointer changes, failed reads, cancellation, and
owner drop retire the old binding. `RunBinding` checks that authority both before
submission and after async host preparation. A→B→A cannot revive old callbacks.
`owns_run` lets a consumer reject substitution by a main-run cache entry with the
same session ID. Cache the returned binding with its pane, not with a task's first
session ID forever. Subscribe to task publications as well as run/runtime changes.

This adapter consumes an explicitly resolved ordinary task run. Independent child
control remains subject to the existing catalog/continuation policy. Native
bounded subagent history must use `RunBinding::open_child` with explicit layout;
do not fabricate a task session from a tool call or use parent history as fallback.

The component demo accepts `open_task_source(Arc<TaskPaneBinding>)` for an explicit
host loader/navigation input. It also routes existing Summary `OpenTask` actions
and resolves those rows from canonical SummaryFacts. Same-session todo rows do
not gain such an action. Drain `take_background_commands` after fact updates to
open the resolved source; install results through `install_task_run`. A source
change retires old content, while a failed open waits for explicit tab activation
before retry. Task-tab close is local navigation: it never cancels/discards the
remote task. External task loaders must forward their subscription to the host's
window wake. The native fixture script uses explicit host navigation for a known
same-session task row; it does not invent Summary navigation for a todo.

Tasks currently mount public Composer/Indicators only. Formal Transcript and the
complete task RunView still await the same structured-content integration as the
main run. Tests cover source ABA, stale load tokens, slow baseline completion,
host-preparation fences, draft lifetime and public-control composition.

## Child pane routing

Keep one `ChildPaneBinding` per retained child pane. `open(runtime, direct_parent,
child_id, layout, source)` and `for_run(parent_binding, ...)` return a typed child
source; its private construction prevents main-run cache substitution. Layout is
an explicit adapter registration fact, independent of continuation capability.
Native bounded history uses `SameSession` and the public bounded API (or an
explicit `ChildHistorySource`); Pi/DSH independent history uses
`IndependentSessions`. Never discover layout from a child ID or failed read.

The **direct receiving parent** can differ from the root thread shown by the
SidePanel. Keep it in `ChildPaneBinding::parent()` and `SidePanelTab::child_parent`.
Changing it or the history layout retires old content/install tokens. Child
continuation still uses RunController's existing catalog policy, queried against
that direct parent, and the runtime rechecks ownership immediately before control.
`RunSnapshot::child_availability` exposes that same derived policy to Indicators;
it is optional when decoding older snapshots. The UI does not compute another
availability state or infer permission from readable history.

`binding()` provides the existing run lease, subscriptions, send/cancel and
pagination. Independent sources produce `PaneSnapshot::Run`; bounded sources
produce `ReadOnlyChild` with no Composer snapshot. Refresh controls only for an
independent source. Failed catalog reads remove control authority while leaving
history readable. Custom sources own their pagination, so
`supports_pagination()` is false for them; supply the adapter's own paging UI.

Cache the typed source with its controls across tab switches. `retire()` or owner
drop revokes captured controls without cancelling/discarding remote work. A new
mount is a new lifetime even if its ID is reused. Before async installation, also
check the panel/navigation token; source role and UI navigation are separate.

The demo's `begin_child_open` / `install_child_source` fence async navigation;
`cached_child_source` preserves the original controller/draft on repeated opens.
Summary OpenSubagent callbacks go through the host's registered layout. The host
maps Native to bounded history and Pi/DSH to independent history at connector
registration; components do not infer this from endpoint identity. Runtime/run
subscriptions schedule updates via public window wake. `refresh_key` coalesces
initial, reconnect, observed direct-parent execution-state and owner archive changes; it grants no
control authority and does not replace a producer-specific change notification.
Explicit refresh remains available. There is no per-frame query timer.

Bounded demo content is explicitly a **history-load diagnostic**, not Transcript:
loaded-item count, stale/read-error information, refresh and supported pagination.
It preserves the last read on error and admits one read per pane at a time; a
same-key failure waits for an explicit retry. Retired/inactive async results do
not replace another parent's content or notice. `update_child_snapshot` and
`child_read_failed` finish the host's read command; always drain pending background
commands after sync. Formal child Transcript/RunView still need structured text
and reading-anchor integration, including live Native presentation acceptance.

## Shared inventory and thread actions

Create `SidebarBinding::new(agent, endpoint_id)` inside the async runtime, using
the **same** `Arc<SharedAgent>` endpoint whose runtime backs RunBinding. Register
the connector once and await its baseline before mounting the connected demo.
`EndpointConfiguration::into_connector` is the demo host's ownership transfer;
it avoids starting another Pi connector or opening the same dataRoot twice.

The binding observes `SharedAgent::follow(target="state")` directly through its
Rust API; it starts no WebUI/HTTP listener. `snapshot`/`subscribe` publish versioned
`SidebarPublication` rows, including canonical activity, pins and action definitions.
`loaded` means an inventory frame has arrived; errors retain the last readable
rows, mark activity stale and publish the error. Host navigation supplies selection
through `publication.input(selected)`. Hosts may apply their own grouping/order.

Drain `SidebarIntent` before changing inputs. Pass captured address/definition and
the **same RPC ID on reconciliation/replay** to `perform`. Mutations use the shared
`thread.performAction` dispatcher, which rechecks current capabilities and owns pin
replay. Copy returns a fresh shared value for the host's clipboard gesture, rather
than trusting stale `copy_text`. Rename passes the host's explicitly collected title.
Apply returned navigation/clear-selection/warnings in the host; the binding never
changes selection itself and never retries a mutation.

Install shared preferences through `SharedAgent::open_thread_preferences(path,
scope)` or an explicit shared host adapter before wrapping the service in Arc.
There is no UI pin database. The unreleased shared-service fix makes custom
`ThreadActionHost::pin_state` drive inventory badges as well as menus, and publishes
after host mutations. An external adapter can call `invalidate_thread_inventory`
after its local authority changes; invalidation edits no pin or session.

The demo routes newly selected inventory rows through captured navigation requests.
Newer selections, an in-progress composition or confirmed pointer disappearance
retire late open results. Reappearing pointers get fresh row IDs, and retired
callbacks cannot reactivate them. Each cached run retains its original controller,
Composer and Summary pane; async opens do not replace that revision authority.

`ThreadRename` is a separately mountable transient title editor built from the
public TekesUI TextFieldContent and buttons. Call `open` with the captured address,
enabled rename definition and canonical title. Drain `take_intents` after every
input event, before applying updates or opening a new target. It blocks empty,
composing and pending submissions; it never owns inventory titles. Dispatch a
`RenameIntent::Submit` through `SidebarBinding::perform` with its captured title.
Generate a host-unique RPC prefix and key it by ticket generation/attempt. An exact
explicit retry after an uncertain result retains that ticket/RPC identity; known
rejection or edited payload creates a new attempt. There is no automatic retry.
Return `RenameFailure::from_error` or success through `complete`; stale completions
cannot close another target. Dismissal does not cancel a remote mutation.

Use `Sidebar::set_pending` to restrict repeated local actions while dispatch is
pending; release it on completion. It cannot override disabled shared definitions.
The demo captures a navigation revision before fork/archive dispatch. A returned
fork pointer is opened through a fresh history lease even if its inventory update
has not arrived. Late completion never steals a newer selection or composition.
Archive clears only its still-current source selection, leaving an explicit empty
state and preserving retained drafts. Neither action creates a blank fallback run.

## Composer model and preset content

`ComposerChoices` is independently mountable **content**, not a new menu/window
implementation. Place it in the host's public TekesUI layout/panel/popover. Keep
the Composer's existing marked-text barrier around opening, target changes and
navigation; this button-only component does not own an input context.

1. Capture the exact `Arc<RunBinding>` and destination from `ComposerIntent::OpenEntry`.
   Call `binding.begin_settings(&destination)` once, and keep that non-cloneable
   `ComposerSettingsRequest` with the gesture. It advances the existing controller's
   selection generation; an earlier catalog/settings request can no longer commit.
2. Call `choices.open(destination, Model|Preset)` and keep its returned request.
   Read a catalog through an explicit host adapter using current endpoint capability
   and the actual receiving session. No installation, credentials or filesystem
   discovery is performed by the component. Fence host navigation before presenting
   that read; an inactive retained binding does not know the current window selection.
3. `choices.update(&request, catalog)` accepts a nonzero increasing revision and
   unique choice IDs. Loading/error rows are readable but disabled; current values
   absent from the catalog remain visible. It never defaults to the first row.
   New versions and open gestures use fresh row IDs; stale async data/AX callbacks
   cannot target a newer catalog. Invalid input leaves the prior catalog intact.
4. Drain `choices.take_intents` after each event and before updates. A Select callback
   contains the original request, catalog revision and choice. Recheck current catalog
   authority in the host. Pending blocks duplicate clicks; there is no automatic
   retry or network call. Dismissal unmounts local content without cancelling an
   already-dispatched remote operation. Retain remote reconciliation identity if
   the host performs a mutation and the result is uncertain.
5. Compile the host's **complete explicit settings map** and consume the captured
   `settings_request.commit(settings)`. This calls the existing RunController
   `commit_settings`; it does not patch message prose or parse a prompt into options.
   It preserves text/artifacts/annotations and checks task/child/SideChat lifetime.
   `Ok(false)` means the generation is obsolete; `Err` means the pane is no longer
   writable. Drop the request on cancellation. Never resolve through the newest
   visible binding or replay a stale request against another controller.
6. Complete the local choice gesture using its exact selection. On success it closes;
   an error stays visible with the catalog. Any explicit fresh settings gesture needs
   a fresh binding request. Shared Send still freezes settings in PreparedSubmission;
   the existing host SubmissionPreparation applies model/preset/auth before dispatch.

`RunBinding::model_choices(revision, rpc_id)` now reads public `session.models`
for the captured receiving session and rechecks pane ownership, Ready capability
and connection epoch after the read. A reconnect invalidates the old response.
Hosts also capture the epoch via `subscribe_runtime().inspect(...)` for the UI
gesture: reject success/error after an epoch change and retire already visible
model/reasoning controls. A completed read is not lifetime authority for a panel.
`ModelChoices` retains raw catalog/extension/failure metadata and uses framed
provider/model IDs. Explicit routableProviders governs newer catalogs. The basic
Kernel 0.9 SessionModels contract instead provides configured enabled groups plus
explicit provider failures; that legacy shape is accepted without inventing the
optional extension. Missing routing facts disable choices, rather than defaulting
an unknown provider to available. Read support and write capability stay separate.

The native ComponentDemo mounts this content and fences both main and pane
navigation. Failed reads remain in the panel; stale success/error cannot reopen it.
`ModelSelectionPreparation` is an explicit host opt-in for the single
`modelSelection: {provider, model, reasoningEffort?}` setting. It rechecks the public catalog, uses
a stable per-submission model RPC ID, validates the selection receipt, then lets
the existing controller dispatch the prompt. Failure restores the unsent draft;
there is no automatic retry. It rejects all other setting keys. Credentials,
installation and provider-specific policy remain explicit host responsibilities.

The model path has native fixture and actual three-SEP offscreen integration
receipts. These do not prove physical IME, a standard native menu, three-harness
native UX, provider-specific reasoning switches or complete model/preset parity.
See `docs/rust-agent-ui-model-integration-20261019.md` for exact scope.

## Composer draft content

`ComposerContext` renders attachment names, commands, skills, references and
annotation DTOs from the existing RunSnapshot. It does not read a file, parse
prompt syntax, upload media, resolve provenance or implement another editor.
For embedding, opt in with `Composer::with_context(namespace, labels)` and reserve
that entire namespace separately from other components. Empty content remains
retained but unmounted, preserving the original editor and layout. The demo uses
explicit namespaces above its main/pane ranges.

`Composer::update_gated` updates embedded cards. A standalone context takes the
same controller's monotonic snapshot plus an editable gate and explicit
ComposerContextAffordances. Preview/reveal/edit default off. With an embedded
Composer, marked text disables all card actions; the surrounding host must still
retain its normal cross-component composition barrier. Apply affordance changes
before the next snapshot update and drain actions before changing source inputs.

Drain `ComposerIntent::Context` with the original binding. Its immutable Arc
baseline, item and action belong to the displayed destination, not whichever
session is visible after an await. Call `binding.remove_context(&intent)` for
Remove. It delegates to shared atomic patch_context, preserving newer prose and
settings. False means the capture became stale; refresh the UI and require an
explicit new gesture. Never automatically replay against a newer capture.
Open/EditAnnotation are captured host callbacks, not evidence that preview,
source navigation has been implemented. Existing comment editing is now available
through AnnotationEditor as documented below. Verify source
locator/revision/digest and permissions before revealing or editing.

For an explicit host file-picker/upload/capture adapter, take
`ComposerContextBaseline::read(&binding.current_run()?)` from the public
tekes-agent-presentation package. Collect the host's valid artifact/annotation
DTOs, then call `binding.patch_context(&baseline, artifacts, annotations)`.
This atomically replaces only those two collections. Do not read a whole stale
draft and write it back with new media: that would erase newer text/settings.
Batch related uploads into one capture. If the capture is obsolete, retain the
owned result for explicit user recovery; do not attach it to the newest session.

The shared per-destination draft_context_revision changes on context edits,
Send (including empty context), failure restoration and materialization. A
remove/re-add of identical content therefore cannot revive a callback. It does
not change for text, settings or stream updates. Older serialized snapshots
default this field to zero; that does not authorize replacing a newer live
context. Existing Run settings gestures do not invalidate independent context
captures; NewThread captures still require the exact selection generation. Rust
struct-literal callers must supply the new snapshot field. The original binding also rechecks missing/retired/read-only/unknown
pane authority. Keep one controller authority for each component lifetime.

The demo mounts cards, implements removal and explicitly enables existing comment
editing. Native --context-selftest
uses a display-only attachment receipt and removes it before Send; no file
upload/preview is claimed. --ui-context in the real-SEP script proves reference
and annotation text return with isolated local synthetic providers. It does not
prove structured historical cards, content undo, a native file picker, an
source highlights, physical IME or VoiceOver. The separate --annotation-selftest
exercises comment editing with synthetic text services.

## Initial preset creation inputs

`InitialPresetChoices::load(runtime, revision, rpc_id)` reads the advertised public
`session.initialPresets`, or DSH `presets.list` when the former is absent. Read
errors remain errors; there is no cached/empty-success fallback. A connection
epoch change rejects the read. The normalized catalog mounts in ComposerChoices
with `ComposerChoiceKind::Preset`. Preserve the original request/destination and
host navigation generation around every await. Only mount it for NewThread;
existing sessions retain their initial preset, including blank sessions.

The adapter retains the raw source and default ID, disables unavailable/broken
rows, and rejects malformed/duplicate/ambiguous identities. A harness default is
metadata, not an explicit user selection: keep catalog.selected unset until the
host has an actual draft choice. For automatic selection, pass the original
CreateSessionRequest with agent_preset=None directly to the existing controller.
Do not insert configurationText into authored text or model instructions.

After an explicit choice, use `create_request` to validate proposed host inputs.
At Send, freeze the draft with the existing controller.prepare immediately, then
re-read with `InitialPresetChoices::revalidate_create` before dispatch_with. Match
the choice request destination to the prepared ticket, recheck the host's current
gesture/navigation authority, and pass the returned CreateSessionRequest as the
controller's create argument. If validation fails after prepare, cancel_prepared
to restore the admitted draft. Never leave a ticket reserved or automatically
replace a missing choice. Preserve the session ID and RPC identity on uncertain
creation; the controller owns materialization and submission reconciliation.

The helper sets only agent_preset and checks conflicting preset/workspace inputs.
Paths, workspace routing and permissions remain explicit host inputs. It never
calls presets.select or changes an existing session. Initial presets remain
creation inputs separate from modelSelection draft settings; an explicit host
SubmissionPreparation may still configure the created model before one prompt.

The second public consumer exposes initial_preset_panel without RunSurface. The
demo's `--initial-preset-proof --initial-preset-cwd ABSOLUTE_PATH --offscreen PNG`
script exercises public selection and shared creation with synthetic providers.
ComponentDemo now also wires the interactive New Session entry. Configure an explicit
CreateSessionRequest and host-unique RPC prefix. The current Composer keeps its original
destination until creation is confirmed; then the host mounts a normal RunBinding.
InitialSessionCreation captures stable session/RPC IDs, supports explicit automatic
creation without a preset, revalidates a selected preset, and verifies the receipt.
RunBinding.create_initial_session offers the same public path to a second consumer.
A lost/failed result retains the captured action and pending barrier; no replacement
creation or automatic retry is generated. The host still owns reconciliation UI.
Native --creation-selftest validates this entry with synthetic text services; physical
IME remains unverified. The real-SEP script now uses the same ComponentDemo entry
and normal Composer Send, rather than a separate scripted picker.

## Existing annotation comments

AnnotationEditor opens the original ComposerContextIntent and emits typed changes
with its immutable context baseline and source annotation. Drain events, apply
RunBinding.edit_annotation synchronously on the original binding, and complete
with the returned Option<Arc<RunSnapshot>>. The returned publication revision
fences older snapshots as well as acknowledging the context mutation. Feed later
snapshots from that same controller through update_authority with current pane
authority. Never redirect a delayed change to the newest selected binding.

Committed edits preserve whitespace/newlines and update only comment; excerpt,
source, locator and other draft fields remain captured/authoritative. TextField
owns selection, marked text, undo and scrolling; readonly quote preview also uses
the public field. During composition there is no business write. If a host awaits
its write, gate sends while is_pending and drain again after successful complete
to publish any newer typing. This is new authored text, not replay of a failed
write. None/error invalidates the capture and retains the visible local buffer;
reopen explicitly rather than rebase/retry automatically.

Demo inline placement is host layout. Closed editors are retained but unmounted.
All active editors participate in the host composition barrier, including the
main/pane Composers. Source reveal/highlights, selection-to-annotation capture and
structured historical cards are separate unfinished capabilities. See
docs/rust-agent-ui-annotation-editor-20261020.md for evidence and limits.

## Generic file attachment capture (experimental)

Call `binding.begin_attachments(&destination)` **before** opening the host's
system file picker. `supports_attachments()` requires canonical admission readiness, a live writable source,
Ready connection, `attachments.policy` and `session.uploadFile`. The request
retains that exact binding, draft context revision and connection epoch. The
host supplies `AttachmentFile::new(basename, media_type, Arc<[u8]>)`; paths,
permissions, MIME discovery and bounded file reading remain host capabilities.
`MAX_UPLOAD_BYTES` is the shared transport ceiling; the endpoint policy may be
stricter. Generic files are not gated by inline-image MIME support.

Await `request.upload(&files)` once and retain the selected files, request and
report until the host resolves/dismisses the operation. Each capture accepts one
attempt, including a failed preflight. RPC IDs correlate the operation; they do
**not** assert server upload idempotency. Do not abort/drop an in-flight future
as a substitute for server cancellation, or automatically retry a failed or
uncertain upload. If it is interrupted, the host must treat the result as
unknown and explicitly reconcile before choosing another upload.

The adapter preflights count, per-file and aggregate limits, then uploads
sequentially. Legacy draft attachments with unknown byte counts cannot be
assumed empty when an aggregate limit applies. Each successful receipt is
validated; known receipts survive partial failure or a changed draft in
`AttachmentReport`. An upload error is conservatively uncertain. The endpoint
may have stored a file even if no usable receipt returned; the report cannot
invent an identifier for that file or delete it implicitly.

Only an entirely successful batch is appended through the existing atomic
`patch_context`. New body/settings edits survive; Send, context ABA, reconnect,
retired task/child/SideChat authority and source mismatch prevent attachment to
a newer draft or parent run. `Detached` reports do not automatically rebase.
`ComposerArtifact::Attachment.facts` contains optional canonical byte count/MIME
for presentation/policy only; prompt compilation still emits the original
`{type:"file", receiptId}` content without those facts. Old serialized drafts
remain readable; Rust constructors must now set `facts: None` for unknown data.
This field is an unpublished API addition pending a future AgentKit release.

The second public consumer exposes `upload_selected_files` without runtime or
host imports. Deterministic tests exercise real public SEP calls against the
isolated fixture; they are not three-harness or native-system-picker acceptance.
The first binding-only milestone left the demo affordance disabled. The host
picker/status integration below supersedes that limitation; preview, durable
reconciliation, inline images and complete Native/Pi/DSH native-window
validation remain required. Shared three-harness data-path acceptance is recorded below.

### Upload progress and host picker integration

`AttachmentSelection::snapshot/subscribe` publish versioned
`AttachmentTransferSnapshot` facts outside RunSnapshot. Selecting includes
preflight; Uploading reports file index/count, never invented byte percentages.
Terminal Attached/Detached/Failed/Cancelled facts preserve the original request
and destination. `cancel_selection` and `fail_selection` work only before an
upload attempt; they cannot cancel network IO. Subscribe to wake the window;
keep the original source alive without navigating/focusing it on completion.

`ComposerUploads` is a public TekesUI Component consuming one operation's facts.
Give each new operation an unused namespace. It rejects another request,
old/conflicting revisions and terminal-to-pending changes. Its only intent is a
terminal `ComposerUploadDismiss`; pending operations cannot be dismissed as if
cancelled. The host supplies the IME interaction gate. Dismissal releases local
operation buffers, never remotely deletes uploaded files.

ComponentDemo now wires a macOS host file chooser and this public component.
The demo-only macOS dependency is exact `rfd =0.17.2` (MIT), no default features;
Kit packages import no OS dialog crate. The dialog future is constructed on the
main thread and awaited on the existing executor, following
[RFD's documented threading contract](https://docs.rs/rfd/0.17.2/rfd/#macos-non-windowed-applications-async-and-threading).
The host reads regular files on a blocking worker with a per-file ceiling plus
32-file/32-MiB selection bounds; it uses application/octet-stream for generic
files. Endpoint policy still decides the stricter limits. No image decoder or
extension-based inline-image claim is added.

The main Composer's Send/entry gates are restricted only during its own pending
operation; editing/cancellation and other sessions keep their original policy.
SidePanel file pickers now share this path; see the scoped retention integration below. Reports and bytes stay with their
original request after navigation. Complete callbacks validate terminal facts,
request and destination. File-read failures occur before uploads; upload failures
retain known receipts/bytes for explicit reconciliation. The demo has no durable
upload reconciliation across window/process shutdown; closing it during an
upload must be treated as an uncertain operation, not server cancellation.

For the isolated fixture run the native demo with `--components
--file-attachments`; actual endpoint configs expose the entry only when their
public methods support it. This enables a real system picker against a local
synthetic SEP, not a paid provider. Full preview/reconciliation UI, inline images and three-harness pane/native-window acceptance remain
unfinished. The actual three-harness shared upload/Send/history/reconnect path is
now covered by [public binding acceptance](../../docs/rust-agent-ui-upload-three-20261020.md); Native/Pi byte reads pass, while DSH generic file download is unsupported. See
[host/picker evidence](../../docs/rust-agent-ui-upload-host-20261020.md).

### SidePanel upload integration and retired sources

Task, independent-child and SideChat Composers now use the same host picker,
AttachmentSelection and ComposerUploads as the main run. The binding consumes
canonical `RunSnapshot.can_submit` in addition to pointer/connection/method
capabilities; a one-shot child or recovering source cannot acquire a new upload.
The host also respects the existing child-read and global IME barriers. Bounded
same-session history has no Composer or attachment entry.

Operation retention is keyed by request ID and the original `Arc<RunBinding>`,
not just RunScope/session address. Distinct controllers at the same address can
hold independent drafts/operations; a task replacement never inherits its old
request. Address-only `ComponentDemo.upload_report` returns None when ambiguous;
`upload_report_for_request` selects an exact original operation.

The demo retains each status Component outside cached pane bodies. Public Stack
slots place active main/pane operation facts beside their controls. Tab switching
retains the exact status instance without recreating its authority/IDs; closing
or replacing the source moves the record to an explicitly labeled recovery area.
It never falls back to the parent draft. Known receipts and immutable selected
file buffers remain until terminal Dismiss; late callbacks do not navigate or
focus the new source. Dismiss only releases local records, not remote uploads.
This is host content retention and layout; upload state and canonical draft CAS
remain in the original Kit binding/presentation.

See [SidePanel attachment evidence](../../docs/rust-agent-ui-pane-uploads-20261020.md)
for fixture source-replacement/close cases, actual native chooser paths, and the
separate real-three-harness main-run regression. These layers are not equivalent
to complete real-three-harness pane UX, physical IME or a formal Transcript.

## Explicit file preview

`supports_file_preview` requires a ready original pane and the advertised public
`session.fileAttachment` method. It is independent of Send admission. File cards
need `AttachmentFacts.attachment_id`; an upload receipt ID is not a read ID.
Legacy drafts without that metadata do not expose file preview.

Capture `begin_file_preview(&intent)` from the original Composer context Open
intent. `FilePreviewRequest::read` performs one session-scoped read, validates
identity, name, bounded base64/decoded size and media type, then returns immutable
`FilePreview` bytes. No paths, filesystem, viewer, private harness storage or
automatic retry are part of the binding. A source/context/epoch change rejects
late results; text-only draft edits remain valid.

Hosts additionally fence navigation and IME, recheck immediately before opening
a viewer, stage bytes privately if needed, and retain owned files while their
viewer may read them. The demo uses system Quick Look via direct arguments;
`ui/consumer::read_file_preview` illustrates a second public-API host. DSH has no
generic file-read method and does not gain preview through its image API.

## Reasoning levels inside model content

`ModelChoices::reasoning_choices(model_id, requested_effort)` returns the exact
model's advertised levels as `Option<ReasoningChoices>`. Missing metadata stays
unavailable (Pi may publish levels only for the current model); no conventional
list or first option is inferred. The catalog preserves order, explicit unknown
selection, configured default and model/provider disabled reasons. Duplicate IDs,
invalid defaults and malformed metadata fail explicitly.

Mount the catalog in `ComposerChoices` using `ComposerChoiceKind::Reasoning`.
Resolve a callback through `ReasoningChoices::selection` to obtain
`{provider, model, reasoningEffort}`, then commit with the original
`ComposerSettingsRequest`. Keep navigation, IME and draft-model fences; a
reasoning callback must not switch a newer draft back to its old model. The demo
mounts model and reasoning content together and closes both on selection/dismiss.
The second public consumer exposes `reasoning_panel` with the same component.

`ModelSelectionPreparation` rereads `session.models` at Send and verifies an
explicit effort is still advertised and enabled **before** session.selectModel
or prompt dispatch. A failure uses the existing submission/draft recovery path,
with no automatic retry. Choosing UI content alone does not mutate endpoint state.

## Waiting interactions (initial actionable content)

`ActionablePrompt::read(scope, source)` projects shared actionable frames without
transport or a new Run state machine. This first slice supports one-shot
approval allow/deny and one question with named single-choice options. It emits
the shared session envelope; adapters own harness-specific conversion. Unknown,
secret, multi-select, multi-question and free-text-only forms expose no invented
response. Rich forms and custom/notes input remain outstanding.

`ActionableCard` mounts like other public components. One instance belongs to
one original Run binding. Feed a monotonic publication, captured connection
epoch, canonical prompt (or None), and a host interaction gate that is false
while any Composer is composing. Unchanged publications retain button instances
and pointer tracking. Prompt/epoch changes retire old action IDs. Do not modify
the captured presentation choices before dispatch.

Drain `ActionableResponseIntent`, then capture the original binding's
`begin_actionable_response`. It verifies canonical source, scope, epoch and exact
response vocabulary. `respond` attempts at most once per captured request and
calls the existing RunController/runtime. Never rebuild/replay a request on error.
The component's pending barrier prevents repeated clicks; hosts must retain the
same request rather than creating duplicate captures for one gesture.

Complete with `intent.complete(result)` even if its card is retained offscreen,
then schedule the host's normal sync. Its local receipt belongs to that operation;
it cannot navigate, resolve shared actionables, clear a newer prompt or report an
error in another pane. Unknown errors retain a disabled inline status for explicit
reconciliation; no optimistic removal. Shared resolution drives removal.
`ActionableCard::complete` is the equivalent helper for a directly mounted card.
The second public consumer mounts the card and consumes the response request.

The real-runtime smoke script supports `--ui-components --ui-actionables` for Native, Pi and DSH. It drives public Send/card callbacks, answers the original source after navigating away, checks a durable successful answer, sends a followup and reconnects without duplicate input or cross-session draft changes. This is an offscreen consumer proof with a local synthetic provider, not physical IME/native interaction acceptance. See [the scoped receipt](../../docs/rust-agent-ui-actionables-live-20261020.md).

## History page read receipts

Capture `binding.begin_history_page()` on the original gesture and move its once-consumed `HistoryPageRequest::load()` into the host executor. `history_page_state()` and `subscribe_history_pages()` expose per-mount Loading/Loaded/Failed/Interrupted facts; keep observing the existing runtime and pointer publications too. An old epoch or retired pointer cannot acknowledge a newer request. A dropped future does not cancel the backend read, and failures never retry automatically. Canonical has-more/stale state and custom child pagination capabilities still gate the UI. This is a read receipt, not a document-install or reading-anchor token. See [the scoped contract](../../docs/rust-agent-ui-history-paging-20261020.md).

`TranscriptPaging` is an independently mountable public component over default TekesUI buttons and text. Create it once per binding with a unique namespace and fixed RunScope. Update it with `binding.history_page_state().controls(&canonical_transcript, supports_pagination)`; the transcript must come from that same binding/publication. Custom child sources pass `ChildPaneBinding::supports_pagination()`, never just has-more. On activation, verify the returned scope against the original binding and immediately capture `begin_history_page()` before scheduling. Refresh controls from the resulting Loading receipt before processing another gesture. Completion updates only the original binding; it must not navigate or report an unrelated pane's failure globally. Failed/Interrupted reads offer explicit retry; stale/disconnected/loading input disables activation. Loaded { extended: false } does not override canonical has-more. The control owns no page merge, retry loop, scroll or reading anchor.


`TurnActionDispatcher::prepare(original_binding, intent, rpc_id)` 认领一次原 turn 点击并验证共享动作/持久边界；`TurnActionRequest::execute` 派发前再次核对 pointer、runtime 身份、epoch、未确认提交及 canonical action。执行仍由 SharedAgent 的 `thread.performAction` 处理。Drop 不会重试或撤销已派发操作；尚未派发报告 Rejected，派发后丢失结果报告 Unknown。原组件回执随 retained/offscreen 内容保留；宿主执行 wake + sync/refresh，并按原导航 token 处理返回 outcome，不能直接使用当前选中会话。Unknown 没有自动查询/解锁 API，也不能按相似标题猜测分叉归属。


## Shared RunView composition

The public RunView consumes one captured RunSnapshot plus gates and page facts from its original binding. If a later HistoryPageSnapshot has another connection_epoch, lower source_ready/is_current; never substitute that epoch into the older RunSnapshot. Route the four RunViewIntent variants to the same binding before handing work to the executor. Navigation remains a host callback fenced by its original token. The second consumer demonstrates construction and snapshot synchronization without importing ComponentDemo or RunSurface; see the [RunView contract](../../docs/rust-agent-ui-run-view-20261020.md).


## Read-only child frames and gestures

ReadOnlyChild now includes per-binding snapshot invocation publication and connection_epoch. Allocate order before awaiting; a late older read must not become a newer UI publication. Reads crossing an epoch fail, and a retired source returns Missing. These unpublished UI fields are not SEP journal cursors. ChildPaneBinding::readonly_input lowers page gates when the runtime no longer matches the frame; custom sources retain no Kernel pagination capability.

Mount ReadOnlyTranscript with an exclusive namespace pool. Route its immutable Refresh/Earlier intents to the original source.read_intent, not a newly selected source. The executor checks scope, liveness, page capability/epoch and one-shot admission before IO. Refresh may explicitly renew an old frame on a ready connection; page gestures cannot cross epochs. Keep failed reads with their original pane/task, retain readable content and retry only on a new gesture. See the [read-only contract](../../docs/rust-agent-ui-readonly-child-20261020.md).
