use tekes_agent_components_ui::{
    TranscriptThinkingInput, TranscriptThinkingItem, TranscriptThinkingMode,
};
use tekes_agent_ui_consumer_check::thinking_policy;

#[test]
fn public_ui_api_delegates_live_and_historical_reasoning_without_a_host_state_machine() {
    let items = [TranscriptThinkingItem {
        id: "source:reasoning",
        arrival_order: 7,
        response_frame_id: Some("response"),
        is_provisional: true,
        reasoning_has_ended: true,
        has_reasoning: true,
        has_visible_content: false,
    }];
    let mut input = TranscriptThinkingInput {
        mode: TranscriptThinkingMode::None,
        is_settled: false,
        is_live_presentation_owner: true,
        has_final_output: false,
        elapsed_seconds: 2,
        items: &items,
        calls: &[],
        narrations: &[],
    };
    let live = thinking_policy(&input).unwrap();
    assert!(live.disclosures.is_empty());
    assert_eq!(live.placeholder.unwrap().item_id, "source:reasoning");
    assert_eq!(live.placeholder.unwrap().dots, 3);
    input.is_settled = true;
    input.is_live_presentation_owner = false;
    let historical = thinking_policy(&input).unwrap();
    assert!(historical.placeholder.is_none());
    assert_eq!(historical.disclosures[0].item_id, "source:reasoning");
    assert!(!historical.disclosures[0].active);
}

#[test]
fn independent_consumer_mounts_original_disclosure_and_retires_it_when_live_thinking_is_hidden() {
    use tekes_agent_components_ui::*;
    use tekes_ui_controls::composition::{AccessibilityAction, TextFieldContent};
    let scope = RunScope::main("public", "thinking");
    let source = TranscriptItem {
        id: "streamed-reasoning".into(),
        record_id: None,
        reconciliation_id: None,
        turn_id: "turn".into(),
        kind: TranscriptItemKind::Assistant,
        text: String::new(),
        reasoning: "共享推理原文".into(),
        attachments: vec![],
        content_blocks: None,
        is_visible: true,
        is_final: false,
        is_provisional: true,
        reasoning_ended: false,
        source_sequence: None,
        source_position: None,
        time: None,
        metadata: Default::default(),
    };
    let items = [TranscriptThinkingItem {
        id: &source.id,
        arrival_order: 7,
        response_frame_id: Some("source-frame"),
        is_provisional: true,
        reasoning_has_ended: false,
        has_reasoning: true,
        has_visible_content: false,
    }];
    let mut facts = TranscriptThinkingInput {
        mode: TranscriptThinkingMode::Compact,
        is_settled: false,
        is_live_presentation_owner: true,
        has_final_output: false,
        elapsed_seconds: 1,
        items: &items,
        calls: &[],
        narrations: &[],
    };
    let compact = TranscriptThinkingFrame::read(&scope, "turn", 4, 2, 1, &facts).unwrap();
    facts.mode = TranscriptThinkingMode::None;
    let hidden = TranscriptThinkingFrame::read(&scope, "turn", 4, 2, 2, &facts).unwrap();
    let output = TranscriptOutput {
        key: TranscriptMessageKey::Stream("streamed-reasoning"),
        item: &source,
    };
    let mut view = OutputMessage::new(810, scope);
    tekes_agent_ui_consumer_check::install_thinking_output(
        &mut view, 4, 2, &output, None, &compact,
    )
    .unwrap();
    let root = view.id();
    let ids = view.content_ids()[0];
    let mut host = view.into_host().unwrap();
    assert!(host.get(ids.body).is_none());
    host.accessibility_action(ids.disclosure.unwrap(), None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<OutputMessage>()
            .unwrap()
            .handle_actions(&actions)
    );
    assert_eq!(
        host.get(ids.body)
            .unwrap()
            .component::<TextFieldContent>()
            .unwrap()
            .surface
            .input
            .text,
        "共享推理原文"
    );
    let view = host
        .get_mut(root)
        .unwrap()
        .component_mut::<OutputMessage>()
        .unwrap();
    tekes_agent_ui_consumer_check::install_thinking_output(view, 4, 2, &output, None, &hidden)
        .unwrap();
    assert!(!view.handle_actions(&actions));
    assert!(view.take_copy_intents(&actions).is_empty());
    assert!(
        view.content_ids()
            .iter()
            .all(|row| row.disclosure.is_none())
    );
    assert!(host.get(ids.body).is_none());
}
