# Issue #16: Rust Composer contract audit (2026-10-24)

This audit covers the AgentKit Composer state/component layer and its current
ClientKit/Tekes references. It does not claim native rendering, physical IME,
or complete ClientKit view parity. Issue #16 and PR #10 remain the tracking
locations; this file does not change their state.

## Compared sources

- AgentKit `32986d4d000ac1fa4470029e42a44c1878b70fd9` (working tree clean at
  audit start): `ui/components/src/composer.rs`, `composer_context.rs`,
  `composer_choices.rs`, `composer_uploads.rs`, `run_port.rs`,
  `ui/bindings/src/lib.rs`, `crates/presentation/src/composer.rs`, and the
  `ui/demo/tests/composer_component.rs`, `composer_choices.rs`, and
  `composer_context.rs` suites.
- ClientKit `c289bc9d33edcd83a782ac17927a8c65a027eccd`:
  `Sources/Composer/ComposerSurfaceContext.swift`,
  `Sources/Composer/ThreadComposerView.swift`, and
  `Sources/Composer/RunUserControlsGraph.swift`.
- Tekes `8858ccd9d46590852ec7a0949aee289fc6798d70`: inspected the host
  checkout status and its Composer design acceptance window. This checkout has
  unrelated WIP; it was read-only and none of it was changed.

ClientKit and Tekes had existing unrelated working-tree changes during the
audit. No files in either checkout were modified.

## Findings

AgentKit already has the core state and callback boundaries needed for this
non-SDK slice:

- `Composer` consumes a canonical `RunSnapshot`, renders from the selected
  `ComposerDestination`, and emits typed `Edit`, `Send`, `Cancel`, `OpenEntry`,
  and captured `Context` intents. It does not dispatch runtime operations.
- `ComposerAffordances` is explicit host input for attachment, model, and
  preset entry points. Unsupported entries are omitted; a visible entry also
  requires a destination and is disabled during text composition. `RunPort`
  applies the pane gate to entries and action authority.
- The context component captures a `ComposerContextBaseline` with each action.
  Removal, source preview, annotation reveal, and annotation editing are
  separate intents behind explicit affordance checks. Context callbacks are
  rejected while composing and retain the captured baseline for the host's
  stale-write check.
- Model, reasoning, and preset selection share `ComposerChoices`. Each request
  captures its destination, kind, and generation; catalog updates and
  completion callbacks must match that request. Disabled rows and failed or
  loading catalogs cannot produce selections. The host owns catalog loading
  and applying returned choices.
- Attachment management is exposed through explicit upload/dismiss requests.
  It does not read files or claim filesystem authority; file access and staging
  remain host work.
- Send readiness uses the shared draft compiler and the canonical
  `snapshot.can_submit`; sends and cancels carry the exact presented
  destination/run scope. The Run binding remains responsible for admission and
  prepared submission receipts.

ClientKit makes the same important ownership separation explicit: its
`ComposerSurfaceContext` stores a durable draft destination independently of
the presentation target and layout mode, while `ComposerOwnershipPolicy`
decides whether the host should mount controls for an input-accepting surface.
The Thread Composer receives model/preset/permission callbacks and bindings;
the app supplies endpoint-specific catalogs, file access and submission
authority. ClientKit's `inputIsEnabled` gates the primary action while
`controlsAreEnabled` gates the wider controls configuration. AgentKit's
`ComposerControlGate` is narrower by design: it closes Send/Stop and
catalog/context entry mutation for a pane whose access is closing, unknown or
missing, while preserving its selected destination's draft editing. The gate
does not grant submission authority; the binding rechecks it.

The currently inspected Tekes tree contains no Rust Composer host integration
to compare end-to-end. Its Swift WIP is not evidence that a Rust adapter exists
or that the public Rust consumer/native UI path has passed.

No definite Rust code defect was established in this review, so no state or
component code was changed. Adding another gate or a second settings/draft
state machine without a failing case would risk changing the intentional
editable-draft behavior. The tests listed below cover the relevant action,
identity, context and catalog fencing paths.

## Remaining Issue #16 work

- Verify full RunView layout, focus behavior and same-content composer placement
  against ClientKit on the actual Rust consumer path.
- Run the system Chinese IME and accessibility acceptance. Existing preedit /
  commit tests do not prove physical macOS IME behavior.
- Exercise attachment selection/materialization through an explicit host
  callback and confirm the host's capability reporting; this audit only
  verifies component authority boundaries.
- Revisit the pane gate only if a concrete host requirement says Closing,
  Unknown, or Missing must also prevent local draft edits. Current documented
  behavior preserves the destination draft while disabling dispatch controls.

## Validation evidence

Relevant existing regression cases include:

- `standalone_composer_ime_and_switch_keep_destination_and_draft` and
  `send_readiness_uses_shared_compiler_for_empty_and_artifact_only_drafts` in
  `ui/demo/tests/composer_component.rs`.
- `host_affordance_and_run_authority_both_gate_composer_callbacks` was added
  during this audit. It verifies the combined host affordance, pane gate, and
  canonical `can_submit` rejection paths, including stale queued actions.
- `choice_catalog_retains_unknown_selection_and_restricts_disabled_pending_and_failed_reads`
  and `catalog_and_callbacks_are_fenced_across_scope_aba_versions_and_dismissal`
  in `ui/demo/tests/composer_choices.rs`.
- `embedded_context_keeps_payloads_private_uses_captured_items_and_obeys_ime_barrier`
  and `atomic_context_patches_preserve_new_text_and_settings_but_reject_send_and_selection_changes`
  in `ui/demo/tests/composer_context.rs`.

The new targeted regression passed with `cargo test --manifest-path
ui/Cargo.toml -p tekes-agent-ui-demo --test composer_component
host_affordance_and_run_authority_both_gate_composer_callbacks --locked
--offline`, reusing `/tmp/agentkit-layout-text-292-target`. Validation is
limited to headless Rust checks. There is no desktop, native rendering,
physical IME, accessibility, or Tekes host acceptance claim here.

Main-session review strengthened the new regression with positive controls:
the original Send/Model actions must exist before pane revocation, and an
active model intent must be accepted before its host affordance is withdrawn.
The queued model action is captured before withdrawal, so rejection is not a
vacuous assertion against an already absent control.

The complete targeted suites `composer_choices` (3), `composer_component` (9),
and `composer_context` (4) passed: 16 tests, zero failed/ignored. Targeted strict
Clippy, workspace format check and diff check passed. Logs are retained in
`.runtime-fixtures/issue-batch-20261024/issue16-composer-tests.log` and
`issue16-clippy.log`. No product Rust implementation or SDK revision changed.

## Later completion of the non-desktop host operation

The later [shared host operation result](rust-agent-ui-attachment-host-operation-results-20261024.md)
extracts the production demo paths/read/upload/typed-completion operation and executes it in the
owned Native/Pi/DSH offscreen proof. This supersedes the materialize-logic gap above, without
claiming OS picker, native window event-loop, physical IME or full layout acceptance.
