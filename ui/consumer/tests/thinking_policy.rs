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
