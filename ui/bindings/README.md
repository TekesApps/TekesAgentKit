# Agent UI runtime bindings (experimental)

`RunBinding` binds one mounted pane to existing `SessionRuntime`, `RunController`,
`ChildRun` and `SideChatController` authority. It owns observations, not windows or
another Run state machine. This package does not yet supply a formal Transcript
or RunView. It consumes the UI workspace's exact published TekesUI revision.

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
