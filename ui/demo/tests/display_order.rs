//! Public journal projection -> shared presentation -> second consumer and RunPort.
use serde_json::json;
use tekes_agent_components_ui::*;
use tekes_agent_presentation::{ComposerDestination, PresentationStore};
use tekes_agent_session::{ConnectionState, MemoryProjectionStore};
use tekes_ui_controls::composition::*;

fn captured_store() -> MemoryProjectionStore {
    let rows = vec![
        ("turn/start", json!({"turn":1})),
        (
            "user/message",
            json!({"turn":1,"id":"input","content":[{"type":"text","text":"输入"}]}),
        ),
        (
            "tool/call",
            json!({"turn":1,"step":1,"callId":"z","name":"read","arguments":{"path":"first"}}),
        ),
        (
            "tool/call",
            json!({"turn":1,"step":1,"callId":"a","name":"read","arguments":{"path":"second"}}),
        ),
        (
            "assistant/message",
            json!({"turn":1,"step":1,"message":{"id":"response","content":[{"type":"reasoning","text":"思考"},{"type":"text","text":"工具之前的说明"}]}}),
        ),
    ];
    let entries:Vec<_>=rows.into_iter().enumerate().map(|(seq,(kind,data))|json!({"event":{"type":kind,"seq":seq,"time":100-seq,"data":data}})).collect();
    let mut store = MemoryProjectionStore::new("sep");
    store.apply("journal:s",json!({"type":"snapshot","snapshot":{"sessionId":"s","generation":"g","throughSequence":4,"windowLimit":50,"hasMoreBefore":false,"entries":entries}})).unwrap();
    store.apply("journal:s",json!({"type":"transient","generation":"g","sessionId":"s","event":{"type":"assistant/chunk","seq":9999,"time":0,"data":{"turn":1,"step":2,"chunk":{"type":"reasoning-delta","delta":"下一步思考"}}}})).unwrap();
    store.connection = ConnectionState::Ready;
    store.inventory_ready = true;
    store.control_ready = true;
    store.actionables_ready = true;
    store
}
fn captured_run() -> RunSnapshot {
    let store = captured_store();
    let mut view = PresentationStore::default();
    view.select(ComposerDestination::run(RunScope::main("sep", "s")))
        .unwrap();
    view.refresh(&store).unwrap()
}
fn paging(scope: &RunScope) -> TranscriptPagingFacts {
    TranscriptPagingFacts {
        scope: scope.clone(),
        supported: false,
        has_more_before: false,
        is_stale: false,
        source_ready: true,
        read: TranscriptPageRead::Idle,
    }
}
#[test]
fn shared_display_order_drives_original_run_rows_tools_reading_keys_and_retained_composer() {
    let scope = RunScope::main("sep", "s");
    let source = captured_run();
    let original = source.clone();
    let document = TranscriptDocument::read(&source.transcript).unwrap();
    let display = tekes_agent_ui_consumer_check::display_timeline(
        &document.sections[0],
        &scope,
        &source.transcript.thinking_sources,
    )
    .unwrap();
    assert!(display.unplaced.is_empty());
    let narration = source
        .transcript
        .items
        .iter()
        .find(|row| row.text == "工具之前的说明")
        .unwrap();
    let first = source
        .transcript
        .tools
        .iter()
        .find(|tool| tool.arguments["path"] == "first")
        .unwrap();
    let second = source
        .transcript
        .tools
        .iter()
        .find(|tool| tool.arguments["path"] == "second")
        .unwrap();
    assert!(
        matches!(display.ordered[1],TranscriptOrderedEvent::Message(row) if row.id==narration.id)
    );
    assert!(
        matches!(display.ordered[2],TranscriptOrderedEvent::ToolCall(tool) if tool.id==first.id)
    );
    let mut run = RunPort::standard(6000, 6010..6300, scope.clone()).unwrap();
    run.set_thinking_display(TranscriptThinkingDisplayInput {
        mode: TranscriptThinkingMode::Compact,
        display_revision: 1,
        elapsed_seconds: 0,
    })
    .unwrap();
    run.update(
        &source,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    let content = run.transcript().unwrap();
    let keys = content.row_keys().to_vec();
    let message =
        TranscriptContentRowKey::Message(TranscriptMessageIdentity::Item(narration.id.clone()));
    let tool = TranscriptContentRowKey::Tool(first.id.clone());
    assert!(
        keys.iter().position(|key| key == &message).unwrap()
            < keys.iter().position(|key| key == &tool).unwrap()
    );
    assert!(
        !keys
            .iter()
            .any(|key| matches!(key, TranscriptContentRowKey::Unplaced(_)))
    );
    assert_eq!(
        content
            .tool_run(&first.id)
            .unwrap()
            .summary()
            .unwrap()
            .tool_ids,
        vec![first.id.clone(), second.id.clone()]
    );
    let original_rows: Vec<_> = keys
        .iter()
        .map(|key| content.row_id(key).unwrap())
        .collect();
    let editor = run.ids().composer.editor;
    let root = run.ids().root;
    let mut host = run.into_host().unwrap();
    assert!(host.focus(Some(editor), true));
    host.event(&ContentEvent::Preedit {
        text: "中文组字".into(),
        selection: 4..4,
        clauses: vec![],
        replacement: None,
    });
    let baseline = host.text_surface(editor).unwrap().input.external_baseline();
    let run = host
        .get_mut(root)
        .unwrap()
        .component_mut::<RunPort>()
        .unwrap();
    run.update(
        &source,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    assert_eq!(run.transcript().unwrap().row_keys(), keys);
    assert_eq!(
        keys.iter()
            .map(|key| run.transcript().unwrap().row_id(key).unwrap())
            .collect::<Vec<_>>(),
        original_rows
    );
    assert_eq!(host.focused(), Some(editor));
    assert_eq!(
        host.text_surface(editor).unwrap().input.external_baseline(),
        baseline
    );
    assert_eq!(source, original);
}

#[test]
fn working_row_and_installed_plane_facts_follow_original_run_ownership_through_settlement() {
    let scope = RunScope::main("sep", "s");
    let mut store = captured_store();
    let mut presentation = PresentationStore::default();
    presentation
        .select(ComposerDestination::run(scope.clone()))
        .unwrap();
    let first = presentation.refresh(&store).unwrap();
    let turn = first.transcript.turns.last().unwrap().id.clone();
    let mut run = RunPort::standard(6400, 6410..6600, scope.clone()).unwrap();
    run.update(
        &first,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    let content = run.transcript().unwrap();
    let status = TranscriptContentRowKey::ActivityStatus(turn.clone());
    let key_index = content
        .row_keys()
        .iter()
        .position(|key| key == &status)
        .unwrap();
    assert!(key_index > 0);
    let input = first
        .transcript
        .items
        .iter()
        .find(|row| row.kind == TranscriptItemKind::User)
        .unwrap();
    assert_eq!(
        content.row_keys()[0],
        TranscriptContentRowKey::Message(TranscriptMessageIdentity::Item(input.id.clone()))
    );
    let status_id = content.row_id(&status).unwrap();
    let original_output = first
        .transcript
        .items
        .iter()
        .find(|row| row.text == "工具之前的说明")
        .unwrap();
    let output_key = TranscriptContentRowKey::Message(TranscriptMessageIdentity::Item(
        original_output.id.clone(),
    ));
    let output_id = content.row_id(&output_key).unwrap();
    let facts = tekes_agent_ui_consumer_check::installed_turn_planes(content, &turn).unwrap();
    assert!(facts.is_live_owner && facts.has_activity && !facts.has_output);
    assert_eq!(first.turn_planes(&turn).unwrap(), facts);
    assert_eq!(facts.items[&original_output.id].is_narration, Some(true));
    let root = run.ids().root;
    let mut host = run.into_host().unwrap();
    assert_eq!(
        host.get(status_id)
            .unwrap()
            .component::<TextContent>()
            .unwrap()
            .text,
        "Working"
    );
    store.apply("journal:s",json!({"type":"event","generation":"g","sessionId":"s","entry":{"event":{"type":"turn/end","seq":5,"time":200,"data":{"turn":1,"reason":"completed"}}}})).unwrap();
    let next = presentation.refresh(&store).unwrap();
    let run = host
        .get_mut(root)
        .unwrap()
        .component_mut::<RunPort>()
        .unwrap();
    run.update(
        &next,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    assert!(run.transcript().unwrap().row_id(&status).is_none());
    assert_eq!(
        run.transcript().unwrap().row_id(&output_key),
        Some(output_id)
    );
    let facts =
        tekes_agent_ui_consumer_check::installed_turn_planes(run.transcript().unwrap(), &turn)
            .unwrap();
    assert!(!facts.is_live_owner && facts.has_output);
    assert_eq!(next.turn_planes(&turn).unwrap(), facts);
    assert_eq!(
        facts.items[&original_output.id].body,
        Some(TranscriptPlane::Output)
    );
    assert!(host.get(status_id).is_none());
}
