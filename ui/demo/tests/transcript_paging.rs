use tekes_agent_components_ui::{TranscriptPageRead, TranscriptPaging, TranscriptPagingFacts};
use tekes_agent_presentation::{RunScope, TranscriptSnapshot};
use tekes_agent_ui_bindings::{HistoryPagePhase, HistoryPageSnapshot};
use tekes_ui_controls::composition::*;
fn facts() -> TranscriptPagingFacts {
    TranscriptPagingFacts {
        scope: RunScope::main("endpoint", "a"),
        supported: true,
        has_more_before: true,
        is_stale: false,
        source_ready: true,
        read: TranscriptPageRead::Idle,
    }
}
fn activate(control: TranscriptPaging) -> Option<RunScope> {
    let button = control.button_id();
    let root = ComponentId(button.0 - 1);
    let mut host = control.into_host().unwrap();
    host.accessibility_action(button, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    host.get(root)
        .unwrap()
        .component::<TranscriptPaging>()
        .unwrap()
        .activation(&actions)
        .cloned()
}
#[test]
fn default_library_button_dispatches_only_the_original_scope() {
    let mut control = TranscriptPaging::new(130, facts().scope.clone(), Default::default());
    control.update(facts()).unwrap();
    let button = control
        .children()
        .iter()
        .find(|child| child.id() == control.button_id())
        .unwrap()
        .component::<ButtonContent>()
        .unwrap();
    assert!(!button.button.state.disabled);
    assert_eq!(activate(control), Some(facts().scope));
}
#[test]
fn loading_stale_disconnected_unsupported_and_exhausted_never_activate() {
    for input in [
        TranscriptPagingFacts {
            read: TranscriptPageRead::Loading,
            ..facts()
        },
        TranscriptPagingFacts {
            is_stale: true,
            ..facts()
        },
        TranscriptPagingFacts {
            source_ready: false,
            ..facts()
        },
        TranscriptPagingFacts {
            supported: false,
            ..facts()
        },
        TranscriptPagingFacts {
            has_more_before: false,
            ..facts()
        },
    ] {
        let mut control = TranscriptPaging::new(131, input.scope.clone(), Default::default());
        control.update(input).unwrap();
        assert_eq!(activate(control), None);
    }
}
#[test]
fn failures_and_interruption_allow_only_an_explicit_retry_and_reject_retargeting() {
    for state in [
        TranscriptPageRead::Failed("原始读取失败".into()),
        TranscriptPageRead::Interrupted,
    ] {
        let mut control = TranscriptPaging::new(132, facts().scope.clone(), Default::default());
        let input = TranscriptPagingFacts {
            read: state,
            ..facts()
        };
        control.update(input.clone()).unwrap();
        assert!(
            control
                .update(TranscriptPagingFacts {
                    scope: RunScope::main("endpoint", "b"),
                    ..facts()
                })
                .is_err()
        );
        assert_eq!(control.facts(), Some(&input));
        assert_eq!(activate(control), Some(facts().scope));
    }
}
#[test]
fn canonical_has_more_and_source_ready_gate_new_reads_without_reusing_old_receipts() {
    let mut receipt = HistoryPageSnapshot {
        scope: facts().scope,
        revision: 4,
        connection_epoch: 2,
        request_epoch: Some(2),
        is_current: true,
        source_ready: true,
        phase: HistoryPagePhase::Loaded { extended: false },
    };
    let transcript = TranscriptSnapshot {
        has_more_before: true,
        ..Default::default()
    };
    assert!(receipt.controls(&transcript, true).can_load());
    assert!(!receipt.controls(&transcript, false).can_load());
    receipt.is_current = false;
    receipt.phase = HistoryPagePhase::Failed {
        message: "旧连接错误".into(),
    };
    // A fresh ready connection can start a new gesture; the previous epoch's
    // failure must neither display as current nor permanently disable paging.
    assert!(receipt.controls(&transcript, true).can_load());
    assert_eq!(
        receipt.controls(&transcript, true).read,
        TranscriptPageRead::Idle
    );
    receipt.source_ready = false;
    assert!(!receipt.controls(&transcript, true).can_load());
}
