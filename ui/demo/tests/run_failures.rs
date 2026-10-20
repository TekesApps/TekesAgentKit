use tekes_agent_components_ui::*;
use tekes_agent_contract::NativeConversationSnapshot;
use tekes_agent_session::SessionProjection;

#[tokio::test]
async fn canonical_provider_error_reaches_indicators_and_a_readable_output_control() {
    let (_runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut run = controller.snapshot().as_ref().clone();
    run.run.as_mut().unwrap().error = None;
    let scope = run.run.as_ref().unwrap().scope.clone();
    let session = SessionProjection {
        journal_ready: true,
        native_conversation: Some(NativeConversationSnapshot {
            session_id: scope.session_id.clone(),
            generation: "g".into(),
            leaf_id: Some("error".into()),
            entries: vec![
                serde_json::json!({"id":"u","type":"message","message":{"role":"user","content":"input","timestamp":1}}),
                serde_json::json!({"id":"error","type":"message","message":{"role":"assistant","content":[],"timestamp":2,"stopReason":"error","errorMessage":"controlled provider error"}}),
            ],
            has_more_before: false,
            is_streaming: false,
            state: Default::default(),
        }),
        ..Default::default()
    };
    run.transcript = TranscriptSnapshot::read(&scope, &session).unwrap();
    let facts = IndicatorFacts::read(&run);
    assert_eq!(
        facts.run_error.as_deref(),
        Some("controlled provider error")
    );
    assert!(facts.accessible_text().contains("执行失败"));
    assert!(facts.last_turn_failed);
    let doc = TranscriptDocument::read(&run.transcript).unwrap();
    let output = doc.sections[0].outputs[0];
    let mut view = OutputMessage::new(920, scope.clone());
    view.update(
        &scope,
        run.revision,
        run.connection_epoch,
        &output,
        Some(&doc.sections[0]),
    )
    .unwrap();
    let body = view
        .content_ids()
        .into_iter()
        .find(|ids| ids.kind == OutputContentKind::Status)
        .expect("error remains readable even when content is explicitly empty")
        .body;
    let host = view.into_host().unwrap();
    assert!(
        host.text_surface(body)
            .unwrap()
            .input
            .text
            .contains("controlled provider error")
    );
}

#[tokio::test]
async fn a_known_failed_turn_without_text_stays_failed_but_a_new_turn_is_not_failed() {
    let (_runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut run = controller.snapshot().as_ref().clone();
    run.run.as_mut().unwrap().error = None;
    run.transcript.items.clear();
    let turn = run.transcript.turns.last_mut().unwrap();
    turn.status = "error".into();
    let failed = IndicatorFacts::read(&run);
    assert!(failed.last_turn_failed);
    assert!(failed.run_error.is_none());
    assert!(failed.accessible_text().contains("执行失败"));
    for status in ["running", "completed", "aborted"] {
        run.transcript.turns.last_mut().unwrap().status = status.into();
        let current = IndicatorFacts::read(&run);
        assert!(!current.last_turn_failed);
        assert!(current.run_error.is_none());
    }
}
