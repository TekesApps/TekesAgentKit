# Standalone Composer host and lifecycle contract

Issue: [#35](https://github.com/TekesApps/TekesAgentKit/issues/35), under Composer tracker #16. Design plus uncompiled implementation candidate; not accepted or integrated.

## Current ClientKit behavior

`TekesClientKit/Sources/Composer/ClientStandaloneComposerController.swift` provides a Composer-only `NSViewController` for a document. Its constructor captures `workspaceID`, `thread`, and `runScope`; it rejects a thread outside that workspace or a scope whose `threadID` differs from the thread. It requires the host's shared `ComposerEnvironment.runControlsStores`.

The controller uses the same `ComposerDestinationStateStore` and `ComposerAnnotationStore` as the Run surface. `state(for: .run(runScope), ...)` returns the destination-owned bridge. The view controller owns a `UserControlsGraph` and may be reparented by the host; keeping the same controller/graph preserves the live editor, selection, undo stack, and marked text. `stop()` tears down its graph. It does not discard the durable destination draft. The host may append an annotation for the captured document and receives the normal `ComposerSubmissionEvent` through `onSubmission`.

`update(thread:runIsRunning:environment:)` accepts only the original workspace and thread. It also requires the destination and annotation stores to remain the exact original store instances. A different document therefore needs another controller and destination; it cannot retarget a mounted Composer by swapping props. `requestFocus()` and the height callback are host seams. The host supplies the stores, navigation/settings actions, and submission handler; the Composer owns its input/control state and submission lifecycle.

The existing test `TekesClientKit/Tests/ComposerTests/StandaloneComposerPublicAPITests.swift` verifies shared bridge identity, the same `.run(scope)` destination, hidden setup controls, annotation storage by `scope.documentID`, unresolved-submission send suppression, explicit reconciliation acknowledgement, and draft/attachment persistence after `stop()`. It does not prove a real second-window reparent, native IME continuity, or physical focus/undo behavior. Those belong to desktop acceptance, not this Rust headless contract.

Relevant ClientKit state is in `Sources/Composer/RunUserControlsGraph.swift`: `ComposerDestinationStateStore` is window-owned and keyed by durable destination; `state(for:)` returns the stable bridge; `materializeNewThread` aliases the same bridge to the receiving Run, and the receiver retires the temporary alias only after acquisition. `ComposerEnvironment.swift` exposes `RunControlsStores` and host actions. `ThreadComposerToolbarView.swift` stores `hasUnresolvedSubmission`; `ComposerCardView.swift` blocks a second send until reconciliation. The standalone controller exposes `acknowledgeSubmissionReconciliation()` but does not guess that a matching text line means the send succeeded.

## Tekes host comparison

A source search in the current Tekes checkout found no `ClientStandaloneComposerController` or equivalent standalone Composer host. Tekes configures shared `ComposerHostActions` in `Tekes/DesktopApp.swift`; that is product wiring, not a document-bound second Composer surface. Thus the immediate Rust deliverable is a reusable host binding/adapter and an independent consumer fixture. A real Tekes window reparent remains future host integration and desktop acceptance; this issue must not claim it is already present.

## Existing Rust boundary to reuse

- `crates/presentation/src/scope.rs`: `RunScope` and `ComposerDestination::Run` already provide destination identity. Draft identity excludes pane/layout. `storage_key()` uses JSON framing rather than parsing an encoded key.
- `crates/presentation/src/composer.rs`: `PresentationStore` owns per-destination drafts and settings, context revisions, selection generation, and original `SubmissionRecord`s. `prepare` snapshots the exact draft/settings, clears content while retaining settings, and refuses another unresolved submission. `finish` restores the captured content only under the captured owner; uncertain dispatch becomes `Unknown`, not an automatic retry. `acknowledge_reconciliation` is explicit. `forget_destination` requires authoritative host discard/absence.
- `crates/presentation/src/controller.rs`: `RunController` is the existing synchronized facade for edit, prepare, dispatch, settings and reconciliation. Its state must remain the sole submission state machine.
- `ui/bindings/src/lib.rs`: `RunBinding::prepare_send`, `dispatch_prepared_with`, `edit_text`, `begin_settings` and `ComposerSettingsRequest::commit` already fence mutations by destination and captured controller. `SubmissionPreparation` is the host hook for explicit settings/auth/model preparation.

Do not introduce a second Composer ledger, generate a new submission ID after detach, infer completion from text, or silently retry uncertain transport. A standalone presentation is a view/host binding to the existing `RunBinding`/`RunController` destination. UI mount identity and durable Run identity stay separate.

## Proposed additive Rust API

Add a `StandaloneComposerBinding` (or similarly explicit host adapter) in `ui/bindings` that owns a strong `Arc<RunBinding>` and a captured `RunScope`; expose a presentation snapshot and the existing destination-scoped callbacks. `RunBinding` remains the authority for edits/settings/send, and `RunController` remains the ledger. Prefer delegation to `RunBinding` over a second store. The adapter API should support:

1. `open(run: Arc<RunBinding>) -> Result<Self>` and `scope()`; validate that the binding has control access at creation.
2. `snapshot()` returning the original draft, settings, availability, and unresolved `SubmissionState` identities from the current `RunController` publication.
3. `edit_text`, `patch_context`, and `begin_settings`/captured settings commit forwarding to the original Run binding with its existing selection token and stale-pointer checks.
4. `prepare_send`/`dispatch_prepared_with` forwarding the original `PreparedSubmission` unchanged. The callback reports the original submission ID, destination and terminal/unknown state; it never creates a replacement ID on remount.
5. `acknowledge_reconciliation(submission_id)` only after host history reconciliation. A remounted surface sees the same unresolved state until that explicit call.
6. Mount lifecycle that may detach and reattach the same presentation owner without disposing the binding or ledger. Permanent close drops only the surface reference. Durable destination forget/discard remains an explicit separate host operation after authoritative discard or pointer absence.

Do not add document or window identity to the serialized Run destination unless current requirements show two distinct documents can address the same durable Run but require independent drafts. Current ClientKit explicitly keys the shared draft by `.run(runScope)`, and states that the same thread should share it across surfaces. If product semantics require per-document isolation for one Run, that is a separate identity decision and must be reconciled with ClientKit first.

The height, focus, and reveal callbacks are presentation/host seams. Rust host code can expose `ideal_height`, `request_focus`, and `on_submission`; it must not own a second scroll, IME, or submission machine. The TekesUI component may be reparented while retaining the same composer component/model owner, but native IME continuity remains unproven until desktop testing.

## Implementation and acceptance plan

This issue is a capability delivery, not a test-run ticket. Implement the additive adapter and a public consumer example, then validate:

- Two standalone mounts for the same Run resolve to the same canonical draft/settings/submission owner; another Run remains isolated.
- Detach, drop of one mount, and reattach preserve the same draft, artifacts, annotations, settings and original pending submission ID. Stopping a surface does not call `forget_destination` or cancel a dispatched request.
- A delayed edit or settings request captured for Run A cannot mutate Run B after navigation, including A → B → A generation changes.
- A prepared submission dispatches once. Dispatched timeout/cancellation remains unresolved/unknown; remount cannot retry it. Send stays gated until host reconciliation acknowledges the original ID.
- Definite pre-dispatch failure restores only the captured draft to its original owner. Late completion after authoritative forget cannot recreate a discarded destination.
- The adapter compiles through the public second-consumer API and current headless Rust workspace/package suites pass. The current AgentKit demo/host composes the adapter without a parallel draft or submission state machine.

Desktop-only acceptance is separate: actual host reparent between windows/panes; focus and selection; undo; Chinese marked-text IME continuity; height/occlusion; accessibility tree. This turn does not run those checks. A public host API and headless tests do not satisfy them.

## Dependencies and status

The current Rust implementation already provides destination isolation, selection-generation fencing, explicit settings commits, pending submission identity, restore/unknown semantics, reconciliation, and authoritative destination retirement. The missing capability is the reusable detached/standalone presentation host binding and public consumer composition with lifecycle ownership. It can be implemented without changing TekesUI if its existing component can be hosted and mounted with stable identity. Any missing generic attach/detach or height/focus capability in TekesUI should become an upstream issue; AgentKit must not fork those controls.

## Implementation handoff (2026-10-25)

Implementation has started for issue #35. The candidate adapter is in
`ui/bindings/src/standalone_composer.rs`; focused integration coverage is in
`ui/bindings/tests/standalone_composer.rs`. The coordinating session has
registered the module and added a public second-consumer forwarding function
plus a demo host composition test. These inputs have not been compiled or run;
they are newer than the already-running UI workspace batch.

The adapter retains one `Arc<RunBinding>` and gives each attach a fresh mount
ID. Detach invalidates that mount and delayed settings response while leaving
the canonical Run and its draft intact. A detached watcher is awakened by a
mount lifecycle signal; an old mount cannot continue delivering new
publications. Reattach uses the same owner and destination. Draft, settings,
context changes, prepared sends, cancellation, and reconciliation delegate to
the original binding; no second ledger is introduced. `RunBinding::acknowledge_reconciliation(id)` was
added by the coordinating session because the prior public binding had no
explicit bridge to the existing `RunController` reconciliation method.

Five headless async integration tests were added for same-destination draft
and settings retention, rejection of a late detached settings reply, pending
submission identity and different-Run isolation, and immediate watcher
termination followed by a fresh live reattach subscription, and delayed
dispatched timeout after detach retaining the original ID and newer draft
without resend. The explicit reconciliation callback uses a synthetic peer;
it does not prove real harness history recovery. Their source has not been
compiled or executed. A separate demo test uses the public consumer to update
the standard Composer while retaining its original editor ID.

This remains an uncompiled candidate. Public second-consumer and demo
composition sources exist but execution is outstanding, together with
headless package/workspace suites, integration and real protocol evidence.
Height/focus/reveal host wiring remains to be checked against the full delivery.
Desktop reparent, focus/selection,
undo, Chinese IME, height/occlusion and accessibility remain not run. The
issue remains open until its full acceptance and integration criteria pass.
