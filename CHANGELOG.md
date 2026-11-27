# Changelog

## 0.6.9 — 2026-11-27

- Resolve sidebar display and search titles via `tekes_agent_presentation::canonical_session_title` (reading `session.projections["title"]` and `summary.projections.values.title` while preserving projected nulls/clears, legacy `title`/`name` fallbacks, and `"New conversation"` defaults), replace permanent `local_titles` with authority/epoch-bounded `PendingRename` state so newer confirmed projections and reconnects supersede local edits, and surface rejected renames in `session_error` / `error_for_session` (#42).
- Replace `archive_session`'s local toggle and `local_archived` set with explicit `archive_session` (`session.archive`) and `unarchive_session` (`session.unarchive`) bindings backed by authority/epoch-bounded `PendingArchiveOp` state, update `SidebarSurface` context menu entries dynamically between `"Archive"` (`["archive"]`) and `"Unarchive"` (`["unarchive"]`), wire AppHost/demo `SidebarIntent::UnarchiveSession`, and surface rejected archive/unarchive errors per session (#42).

## 0.6.8 — 2026-11-27

- Record the originating `cancel_op_id` on `SessionErrorEntry` and unify `clear_superseded_cancel_error` and `record_cancel_error_if_not_newer` under `is_older_same_turn_cancel` so same-turn Stop outcomes are ordered by initiating operation ID across all arrival orders—including when two overlapping Stops are dispatched, the earlier rejection arrives first, and the newer Stop confirms `Ok(())` or `ConfirmedAborted` afterward (#34).
- Replace `Abortable` + `SelectAll` for session journal streams in `SessionRuntime` with `JournalStreams` so `Command::Close` (including cancelled `open_initial_session` rollback) synchronously drops the underlying `FrameStream` before replying `Ok(())`, closing idle/stale peer streams deterministically alongside lease and reconnect-intent revocation (#40).
- Harden `ui/bindings` and `ui/apphost` `native_blockers` Stop tests with observable wake synchronization and full `between` / `overlap` / `ordered` coverage.

## 0.6.7 — 2026-11-26

- Track per-session in-flight cancel operations (`SessionCancelTracking` / `CancelOpGuard`) in `RunUiState` so a newer confirmed same-turn Stop (`Ok(())`, `TurnTerminal::ConfirmedAborted`, or deferred `reconcile_uncertain_cancels`) supersedes any still-in-flight earlier failure for the same `(session, turn)`, even when the error slot was empty when the newer confirmation landed (#34).

## 0.6.6 — 2026-11-26

- Attribute every `cancel_scope_at_epoch` attempt to the active open turn (`active_turn_target`) independent of whether `connection_epoch` matches `expected_epoch`, so retrying Stop at the current epoch on the same active turn clears a prior stale-epoch failure while preserving the failure if the turn ends without retry (#34).
- Clear superseded same-turn `SessionErrorKind::Cancel` errors via `clear_superseded_cancel_error` when an in-epoch Stop retry encounters an unconfirmed HTTP/transport loss and `inspect_turn_terminal` confirms `TurnTerminal::ConfirmedAborted`, while keeping newer errors, explicit single rejections without retry, other turns, and other sessions isolated (#34).

## 0.6.5 — 2026-11-26

- Gate background `try_spawn_history_acquire` on `published_lease_generation == history_lease_generation` and synchronously await `lease.release()` on stale/rolled-back `open_initial_session` cancellation so overlapping `select_session(C)` never acquires `C` before `RunController` publishes `C`, and stale `B` journals close on the wire before `open_initial_session(B)` returns `Err(Error::Disconnected)` (#40).

## 0.6.4 — 2026-11-26

- Decouple `RunWorkbenchBinding` selection publication (`select_session`, `open_initial_session`, `create_session_async`) from `RunUiState` mutex locking via a dedicated `selection_publish` mutex so `RunController::subscribe().borrow()` read guards never stall UI-only getters (`session_error`, `error_for_session`, `last_error`, `sidebar_rows`), while keeping final selection, transcript history, and `SessionHistoryLease` consistent (#40).
- Scope Stop error clearing and aborted-turn reconciliation strictly to the originating `cancel_scope_at_epoch` operation (`op_id`, `CancelTurnTarget`, and `start_error_seq`), move reconciliation to state-update paths so `session_error` and `error_for_session` are strictly read-only, and preserve next-turn cancel rejections, queue-edit errors, and other sessions' errors (#34).

## 0.6.3 — 2026-11-26

- Preserve deferred active session subscriptions across transient handshake reconnect failures (`Error::Disconnected` / `Error::Transport(_)`) so the next healthy connection continues receiving durable `turn/end` before closing, while still retiring explicit permanent `Error::Remote(_)` server rejections (#56).
- Serialize `RunWorkbenchBinding::select_session` and `create_session_async` with `open_initial_session` under `RunUiState` so overlapping `open_initial_session(B)` and `select_session(C)` always agree on final selection, transcript, and lease ownership across reconnect (#40).
- Reconcile unconfirmed interrupted `cancel_scope_at_epoch` errors against authoritative same-session, same-turn `turn/end (reason: "aborted")` replay on reconnect without resending `session.cancel` or clearing explicit rejections, other turns, or other sessions (#34).

## 0.6.2 — 2026-11-26

- Defer closing a running session's journal when its last observer detaches across a disconnect or during the reconnect `inventory`/`follow` handshake (`has_unsettled_run`), so `SessionRuntime` keeps the stream open until durable `turn/end` arrives on the reconnected journal while releasing idle sessions promptly.
- Make `RunWorkbenchBinding::open_initial_session` atomic on error (`Error::Disconnected`), preserving prior selection, history lease, and subscription intent for both fresh bindings and previously selected sessions without scheduling an orphaned background acquire.
- Add controllable-transport reconnect handshake and initial-open regression tests in `crates/session/tests/recovery_interest.rs` and `ui/bindings/tests/native_blockers.rs`, verified with `TekesUI v0.4.12` (`427c0e5d9e6fcdb0d71d203c402decdc05761df9`) and `TekesKernel v0.9.23` (`17662494`).

## 0.6.1 — 2026-11-26

- Keep session operation errors (`cancel_scope_at_epoch`, `submit_destination` with run scope, and queue mutations) scoped per session in `RunUiState::session_errors`, add `RunWorkbenchBinding::error_for_session`, and route `TasksQueueAdapter`, `AgentRunAdapter`, and showcase `build_tasks_queue_model` through session-scoped lookup before falling back to endpoint-level `last_error`.
- Track `desired_history_session`, `history_acquire_attempted`, and `dropped` on `RunWorkbenchBinding` so switching sessions while disconnected automatically acquires the final selected session's `SessionHistoryLease` once `SessionRuntime` transitions back to `ConnectionState::Ready`, without polling or dropping views holding detached observers.
- Upgrade pinned `TekesUI` dependencies across `ui/Cargo.toml`, `ui/tekesui-source.json`, and delivery docs from `v0.4.11` (`31114fea1074d16750c7437b140d2d147cbe8eda`) to `v0.4.12` (`427c0e5d9e6fcdb0d71d203c402decdc05761df9`) for native primary/auxiliary window teardown.

## 0.6.0 — 2026-11-25

- Defer `TranscriptSurface` `RichContent::drain_link_actions` to pre-render (`AgentContainerAdapter::drain_deferred_intents`) so pointer actions on `AgentRunContainer` never invalidate `ContentHost` curve geometry before `PointerUp`, preserving native `Stop` and second `Send` clicks; add epoch-fenced `cancel_at_epoch` / `cancel_child_at_epoch` / `cancel_scope_at_epoch` and session-scoped `session_error` surfacing.
- Add per-destination draft context mutations (`append_artifact`, `append_annotation`, `remove_artifact`, `remove_annotation`) so background attach/quote never overwrites draft text/settings, and isolate `RunWorkbenchBinding` model/effort/approval controls per session with `connection_epoch` and generation fencing.
- Handle `Command::Close` for disconnected idle sessions, clean up failed `acquire_history` owners, capture runtime `Handle` in `SessionHistoryLease::drop`, and migrate `RunController::dispatch_with`, `RunWorkbenchBinding`, and `ChildRunBinding` from legacy `open_session` to scoped `SessionHistoryLease`.
- Deliver unified public workspace + five-crate `ui/` workspace pinned to TekesUI `31114fea1074d16750c7437b140d2d147cbe8eda` without local `ui/.deps` overrides.

## 0.3.9 — 2026-10-29

- Retain active Native journals when their last observer detaches, then retire them after terminal state; reopening keeps the original response.
- Retire rejected Native mux streams without duplicate close, and enable TCP_NODELAY for small mux commands.
- Preserve structured Native HTTP carrier errors and keep Native HTTP requests off system proxies, consistent with the direct WebSocket transport.
- Includes previously merged Pi message-window/recovery and read-only query fixes, plus the standalone DSH runtime probe; this maintenance release adds no new Pi or DSH scope.
- Validation: merged-main workspace tests, strict Clippy, formatting and ownership checks; Native public-consumer and permission regressions retain their exact recorded source/runtime identities.
- Scope: existing headless packages only. The AgentKit/WebUI stage candidate, Family integration, original performance failures and daily-entry upgrade remain outside this maintenance release.

## 0.3.7 — 2026-10-19

- Expose completed-turn footnote actions through the shared session view and existing thread action contract.
- Resolve turn forks to durable journal/native settlement boundaries, revalidate capabilities at execution, and preserve navigation on follow-up warnings.
- Add selected-boundary, restored-history and capability-change regression coverage plus a second public-API consumer.
- Validation: 103 workspace tests, strict workspace Clippy, format and dependency-boundary checks; real public-API history reads across Kernel/Pi/DSH. Browser acceptance remains with the consumer.

## 0.3.0 — 2026-10-18

- Pi independent child creation, parent-owned list/prompt/read/cancel, durable request-key replay,
  official extension model tool, child history/reconnect and Summary creation semantics.
- Native bounded child history through an additive Kernel read-only API, frozen paging/revision,
  effective task-source projection and explicit unsupported behavior on older Kernel hosts.
- No UI, Swift, durable transcript Mirror or Kernel mutation API changes.

## 0.2.0 — 2026-10-18

- Headless task pointer state, parent-owned subagent catalog/control, consumer-owned child history leases.
- Ephemeral SideChat lifecycle with uncertain-mutation reconciliation and confirmed-discard draft cleanup.
- Scoped Summary facts/providers, authoritative todo/usage recovery and typed host navigation actions.
- DSH catalog projection routing and child delivery/admission normalization for the released protocol.
- No UI/TekesUI or Kernel implementation changes.
- Validation: 72 Rust tests, strict Clippy, format/dependency boundaries and Linux CI; real isolated Kernel 0.9.0 / Pi 1.3.0 / DSH 0.2.1-rc.2 and 0.2.0-rc.2 integration with synthetic providers.
- Scope: a headless milestone, not full harness feature parity. Pi native subagents require explicit host services; Native bounded child history requires a public child-line projection source; DSH has no ephemeral SideChat. UI, durable client cache and large-history performance acceptance remain future work.

## 0.1.1 — 2026-10-18

- Public repository with MIT License and inherited license metadata for every workspace package.
- Expanded README, Rust onboarding, headless JSON command guide and contributor instructions.
- Read-only GitHub Actions token permissions and a bounded CI timeout.
- No changes to Run/Transcript/Composer or SessionEndpoint behavior from the 0.1.0 milestone.

## 0.1.0 — 2026-10-18

Annotated headless milestone including merged PRs #1 and #2:

- Independent Native (Kernel 0.9.0/v4), Pi 1.3.0 and DSH 0.2.1-rc.2 adapters; DSH 0.2.0-rc.2 compatibility.
- Session pointers, native ancestry / frozen-cut history, reconnect, actionables, attachments and version-checked files.
- Memory projection and explicit optional/host capability boundaries, without a disk transcript Mirror.
- Renderer-free Run, Transcript, Composer and connection state, destination-owned drafts and receipt-based optimistic input reconciliation.
- 59 local tests and synthetic-provider integration evidence, including the three presentation paths.

No UI/TekesUI integration, durable client cache, production launcher or large-history performance acceptance is included. Earlier CI attempts could not start because of account billing/spending limits; see individual workflow results for subsequent Linux validation.
