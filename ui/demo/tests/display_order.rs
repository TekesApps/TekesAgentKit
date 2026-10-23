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
fn source_proven_opening_input_is_before_activity_even_when_committed_after_a_tool() {
    let entries = vec![
        json!({"event":{"type":"turn/start","seq":0,"time":1,"data":{"turn":1}}}),
        json!({"event":{"type":"tool/call","seq":1,"time":2,"data":{"turn":1,"step":1,"callId":"before-input","name":"read","arguments":{}}}}),
        json!({"event":{"type":"user/message","seq":2,"time":3,"data":{"turn":1,"id":"opening","text":"first input"}}}),
        json!({"event":{"type":"turn/end","seq":3,"time":4,"data":{"turn":1,"reason":"completed"}}}),
    ];
    let mut store = MemoryProjectionStore::new("sep");
    store.apply("journal:s",json!({"type":"snapshot","snapshot":{"sessionId":"s","generation":"g","throughSequence":3,"windowLimit":50,"hasMoreBefore":false,"entries":entries}})).unwrap();
    store.connection = ConnectionState::Ready;
    store.inventory_ready = true;
    store.control_ready = true;
    store.actionables_ready = true;
    let scope = RunScope::main("sep", "s");
    let mut presentation = PresentationStore::default();
    presentation
        .select(ComposerDestination::run(scope.clone()))
        .unwrap();
    let source = presentation.refresh(&store).unwrap();
    let doc = TranscriptDocument::read(&source.transcript).unwrap();
    let section = &doc.sections[0];
    assert_eq!(
        tekes_agent_ui_consumer_check::input_role_source(section.inputs[0].item),
        Some(TranscriptInputRole::Opening)
    );
    let input = TranscriptContentRowKey::Message(section.inputs[0].key.into());
    let activity = TranscriptContentRowKey::ActivityDisclosure(section.id.into());
    let mut view = RunPort::standard(27000, 27010..27200, scope.clone()).unwrap();
    view.update(
        &source,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    let keys = view.transcript().unwrap().row_keys();
    assert!(
        keys.iter().position(|key| key == &input).unwrap()
            < keys.iter().position(|key| key == &activity).unwrap()
    );
    assert!(
        !keys
            .iter()
            .any(|key| matches!(key, TranscriptContentRowKey::ActivitySegment(_)))
    );
}
#[tokio::test]
async fn projected_initial_prompt_remains_before_input_and_preserves_original_disclosure_during_activity_collapse()
 {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup_fixture(
        tekes_agent_ui_demo::fixture::Fixture::default()
            .with_reading_details()
            .with_system_prompts(),
    )
    .await
    .unwrap();
    let source = controller.snapshot();
    let scope = RunScope::main("fixture", "a");
    let facts = || TranscriptPagingFacts {
        has_more_before: source.transcript.has_more_before,
        is_stale: source.transcript.is_stale,
        ..paging(&scope)
    };
    let doc = TranscriptDocument::read(&source.transcript).unwrap();
    let section = doc.sections.last().unwrap();
    let prompt = section
        .outputs
        .iter()
        .find(|row| row.item.system_prompt().is_some())
        .unwrap();
    assert_eq!(
        tekes_agent_ui_consumer_check::system_prompt_source(prompt.item)
            .unwrap()
            .placement,
        TranscriptSystemPromptPlacement::Initial
    );
    let prompt_key = TranscriptContentRowKey::Message(prompt.key.into());
    let input_key = TranscriptContentRowKey::Message(section.inputs[0].key.into());
    let activity_key = TranscriptContentRowKey::ActivityDisclosure(section.id.into());
    let mut view = RunPort::standard(25000, 25010..25500, scope.clone()).unwrap();
    view.update(
        &source,
        Default::default(),
        ComposerControlGate::OPEN,
        facts(),
        true,
    )
    .unwrap();
    let content = view.transcript().unwrap();
    let keys = content.row_keys();
    let position = |key| keys.iter().position(|candidate| candidate == key).unwrap();
    assert!(
        position(&prompt_key) < position(&input_key)
            && position(&input_key) < position(&activity_key)
    );
    let row = content.row_id(&prompt_key).unwrap();
    let parts = content.message_content_ids(&prompt.key.into());
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].kind, OutputContentKind::SystemPrompt);
    let part = parts[0];
    let ids = view.ids();
    let mut host = view.into_host().unwrap();
    assert!(host.get(part.body).is_none());
    host.accessibility_action(part.disclosure.unwrap(), None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    host.get_mut(ids.root)
        .unwrap()
        .component_mut::<RunPort>()
        .unwrap()
        .take_intents(&actions);
    assert!(host.focus(Some(ids.composer.editor), true));
    host.event(&ContentEvent::Preedit {
        text: "提示词组字".into(),
        selection: 5..5,
        clauses: vec![],
        replacement: None,
    });
    let baseline = host
        .text_surface(ids.composer.editor)
        .unwrap()
        .input
        .external_baseline();
    assert!(host.get(part.body).is_some());
    let owner = host
        .get(part.body)
        .unwrap()
        .component::<TextFieldContent>()
        .unwrap() as *const _ as usize;
    for expanded in [true, false, true] {
        let run = host
            .get_mut(ids.root)
            .unwrap()
            .component_mut::<RunPort>()
            .unwrap();
        tekes_agent_ui_consumer_check::set_run_activity_expansion(run, section.id, expanded)
            .unwrap();
        run.update(
            &source,
            Default::default(),
            ComposerControlGate::OPEN,
            facts(),
            true,
        )
        .unwrap();
        assert_eq!(run.transcript().unwrap().row_id(&prompt_key), Some(row));
        assert_eq!(
            host.get(part.body)
                .unwrap()
                .component::<TextFieldContent>()
                .unwrap() as *const _ as usize,
            owner
        );
        assert_eq!(host.focused(), Some(ids.composer.editor));
        assert_eq!(
            host.text_surface(ids.composer.editor)
                .unwrap()
                .input
                .external_baseline(),
            baseline
        );
    }
    runtime.stop().await.unwrap();
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
            < keys
                .iter()
                .position(|key| content.row_id(key) == content.row_id(&tool))
                .unwrap()
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
    assert_eq!(facts.activity, TranscriptActivityPresentation::Working);
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
    assert_eq!(
        facts.activity,
        TranscriptActivityPresentation::Settled {
            default_expanded: false
        }
    );
    assert_eq!(next.turn_planes(&turn).unwrap(), facts);
    assert_eq!(
        facts.items[&original_output.id].body,
        Some(TranscriptPlane::Output)
    );
    assert!(host.get(status_id).is_none());
}

#[test]
fn direct_task_source_keeps_original_content_without_a_working_status() {
    let scope = RunScope::main("sep", "s");
    let mut source = captured_run();
    let turn = "task-source:captured-call";
    source.transcript.turns[0].id = turn.into();
    for item in &mut source.transcript.items {
        item.turn_id = turn.into();
    }
    for tool in &mut source.transcript.tools {
        tool.turn_id = turn.into();
    }
    source.transcript.thinking_sources.live_turn_id = Some(turn.into());
    let mut run = RunPort::standard(6700, 6710..6900, scope.clone()).unwrap();
    run.update(
        &source,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    let content = run.transcript().unwrap();
    let facts = tekes_agent_ui_consumer_check::installed_turn_planes(content, turn).unwrap();
    assert!(facts.is_live_owner);
    assert_eq!(facts.activity, TranscriptActivityPresentation::Direct);
    assert_eq!(source.turn_planes(turn).unwrap(), facts);
    assert!(
        !content
            .row_keys()
            .iter()
            .any(|key| matches!(key, TranscriptContentRowKey::ActivityStatus(_)))
    );
    for item in &source.transcript.items {
        let owner = TranscriptMessageIdentity::Item(item.id.clone());
        if item.kind == TranscriptItemKind::User {
            assert!(
                content
                    .row_id(&TranscriptContentRowKey::Message(owner))
                    .is_some()
            );
        } else {
            assert!(!content.message_content_ids(&owner).is_empty());
        }
    }
    for tool in &source.transcript.tools {
        assert!(
            content.tool_run(&tool.id).is_some()
                || content
                    .row_id(&TranscriptContentRowKey::Tool(tool.id.clone()))
                    .is_some()
        );
    }
}

#[test]
fn settled_activity_parts_fold_and_restore_original_owners_without_changing_composer_preedit() {
    let scope = RunScope::main("sep", "s");
    let mut store = captured_store();
    let mut presentation = PresentationStore::default();
    presentation
        .select(ComposerDestination::run(scope.clone()))
        .unwrap();
    let first = presentation.refresh(&store).unwrap();
    let turn = first.transcript.turns[0].id.clone();
    let item = first
        .transcript
        .items
        .iter()
        .find(|row| row.text == "工具之前的说明")
        .unwrap();
    let owner = TranscriptMessageIdentity::Item(item.id.clone());
    let body = TranscriptContentRowKey::Message(owner.clone());
    let reasoning = TranscriptContentRowKey::MessageFragment(
        owner.clone(),
        OutputMessageFragmentKind::Reasoning,
    );
    let mut view = RunPort::standard(7000, 7010..7300, scope.clone()).unwrap();
    view.set_thinking_display(TranscriptThinkingDisplayInput {
        mode: TranscriptThinkingMode::Compact,
        display_revision: 1,
        elapsed_seconds: 0,
    })
    .unwrap();
    view.update(
        &first,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    let parts = view.transcript().unwrap().message_content_ids(&owner);
    assert_eq!(parts.len(), 2);
    let body_id = view.transcript().unwrap().row_id(&body).unwrap();
    let reason_id = view.transcript().unwrap().row_id(&reasoning).unwrap();
    let tool_id = view
        .transcript()
        .unwrap()
        .tool_run(&first.transcript.tools[0].id)
        .unwrap()
        .id();
    let ids = view.ids();
    let mut host = view.into_host().unwrap();
    assert!(host.focus(Some(ids.composer.editor), true));
    host.event(&ContentEvent::Preedit {
        text: "中文组字".into(),
        selection: 4..4,
        clauses: vec![],
        replacement: None,
    });
    let baseline = host
        .text_surface(ids.composer.editor)
        .unwrap()
        .input
        .external_baseline();
    store.apply("journal:s",json!({"type":"event","generation":"g","sessionId":"s","entry":{"event":{"type":"turn/end","seq":5,"time":200,"data":{"turn":1,"reason":"completed"}}}})).unwrap();
    let next = presentation.refresh(&store).unwrap();
    let run = host
        .get_mut(ids.root)
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
    let content = run.transcript().unwrap();
    assert_eq!(content.row_id(&body), Some(body_id));
    assert_eq!(content.row_id(&reasoning), None);
    assert!(content.tool_run(&first.transcript.tools[0].id).is_none());
    let mut after = content.message_content_ids(&owner);
    after.sort_by_key(|id| id.root.0);
    let mut before = parts;
    before.sort_by_key(|id| id.root.0);
    assert_eq!(after, before);
    let disclosure = content
        .row_id(&TranscriptContentRowKey::ActivityDisclosure(turn.clone()))
        .unwrap();
    let plan = run
        .prepare_update(
            &next,
            Default::default(),
            ComposerControlGate::OPEN,
            paging(&scope),
            true,
        )
        .unwrap();
    tekes_agent_ui_consumer_check::set_run_activity_expansion(run, &turn, true).unwrap();
    assert!(run.validate_prepared_update(&plan).is_err());
    run.update(
        &next,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    let content = run.transcript().unwrap();
    assert_eq!(content.row_id(&reasoning), Some(reason_id));
    assert_eq!(
        content
            .tool_run(&first.transcript.tools[0].id)
            .unwrap()
            .id(),
        tool_id
    );
    let keys = content.row_keys();
    assert!(
        keys.iter().position(|key| key == &reasoning).unwrap()
            < keys.iter().position(|key| key == &body).unwrap()
    );
    let tools = TranscriptContentRowKey::Tool(first.transcript.tools[0].id.clone());
    assert!(
        keys.iter()
            .position(|key| content.row_id(key) == content.row_id(&tools))
            .unwrap()
            < keys.iter().position(|key| key == &body).unwrap()
    );
    assert_eq!(
        content.row_id(&TranscriptContentRowKey::ActivityDisclosure(turn)),
        Some(disclosure)
    );
    assert_eq!(host.focused(), Some(ids.composer.editor));
    assert_eq!(
        host.text_surface(ids.composer.editor)
            .unwrap()
            .input
            .external_baseline(),
        baseline
    );
    assert!(
        host.accessibility_action(disclosure, None, AccessibilityAction::Press)
            .activated
    );
    let actions = host.drain_actions();
    let run = host
        .get_mut(ids.root)
        .unwrap()
        .component_mut::<RunPort>()
        .unwrap();
    let stale = run
        .prepare_update(
            &next,
            Default::default(),
            ComposerControlGate::OPEN,
            paging(&scope),
            true,
        )
        .unwrap();
    let intents = run.take_intents(&actions);
    assert!(
        matches!(intents.as_slice(),[RunPortIntent::TranscriptLayoutChanged{scope:owner}] if owner==&scope)
    );
    assert!(run.validate_prepared_update(&stale).is_err());
    run.update(
        &next,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    assert!(run.transcript().unwrap().row_id(&reasoning).is_none());
    assert_eq!(host.focused(), Some(ids.composer.editor));
    assert_eq!(
        host.text_surface(ids.composer.editor)
            .unwrap()
            .input
            .external_baseline(),
        baseline
    );
}

#[test]
fn abnormal_termination_preserves_visible_activity_and_has_a_selected_original_disclosure() {
    let scope = RunScope::main("sep", "s");
    let mut source = captured_run();
    source.transcript.items.retain(|row| !row.is_provisional);
    source.transcript.turns[0].status = "error".into();
    source.transcript.turns[0].ended_at = Some(200.);
    source.transcript.thinking_sources.live_turn_id = None;
    let turn = source.transcript.turns[0].id.clone();
    let mut run = RunPort::standard(7400, 7410..7600, scope.clone()).unwrap();
    run.update(
        &source,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    let content = run.transcript().unwrap();
    assert!(content.tool_run(&source.transcript.tools[0].id).is_some());
    let button = content
        .row_id(&TranscriptContentRowKey::ActivityDisclosure(turn))
        .unwrap();
    let host = run.into_host().unwrap();
    assert!(
        host.get(button)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap()
            .button
            .state
            .selected
    );
}

#[test]
fn readonly_activity_uses_its_own_publication_and_invalidates_old_plans_without_a_writable_run() {
    let scope = RunScope::main("sep", "s");
    let mut store = captured_store();
    store.apply("journal:s",json!({"type":"event","generation":"g","sessionId":"s","entry":{"event":{"type":"turn/end","seq":5,"time":200,"data":{"turn":1,"reason":"completed"}}}})).unwrap();
    let mut presentation = PresentationStore::default();
    presentation
        .select(ComposerDestination::run(scope.clone()))
        .unwrap();
    let source = presentation.refresh(&store).unwrap();
    let turn = source.transcript.turns[0].id.clone();
    let mut view =
        ReadOnlyTranscript::standard(7700, 7710..7900, scope.clone(), Default::default()).unwrap();
    let input = ReadOnlyTranscriptInput {
        scope: &scope,
        publication: 500,
        connection_epoch: source.connection_epoch,
        connection: &source.connection,
        transcript: &source.transcript,
        frame_current: true,
        source_ready: true,
        supports_pagination: false,
        reading: false,
        read_error: None,
    };
    view.update(input).unwrap();
    let before = view.prepare_update(input).unwrap();
    let collapsed = before.item_ids();
    tekes_agent_ui_consumer_check::set_readonly_activity_expansion(&mut view, &turn, true).unwrap();
    assert!(view.validate_prepared_update(&before).is_err());
    let after = view.prepare_update(input).unwrap();
    assert!(after.item_ids().len() > collapsed.len());
    view.apply_prepared_update(&after).unwrap();
    assert_eq!(view.content().footnote_timing().live_turn_id, None);
    assert!(!view.content().feedback_enabled());
    assert!(
        view.content()
            .tool_run(&source.transcript.tools[0].id)
            .is_some()
    );
}

fn segmented_run(reason: &str, final_reasoning: bool, consecutive: bool) -> RunSnapshot {
    let mut store = captured_store();
    let mut seq = 5;
    let mut publish = |kind: &str, data: serde_json::Value| {
        store.apply("journal:s",json!({"type":"event","generation":"g","sessionId":"s","entry":{"event":{"type":kind,"seq":seq,"time":200+seq,"data":data}}})).unwrap();
        seq += 1;
    };
    publish(
        "user/message",
        json!({"turn":1,"id":"inner","content":[{"type":"text","text":"追加输入"}]}),
    );
    if consecutive {
        publish(
            "user/message",
            json!({"turn":1,"id":"inner-again","content":[{"type":"text","text":"连续追加输入"}]}),
        );
    }
    let mut content = vec![];
    if final_reasoning {
        content.push(json!({"type":"reasoning","text":"后段思考"}));
    }
    content.push(json!({"type":"text","text":"最终回答"}));
    publish(
        "assistant/message",
        json!({"turn":1,"step":2,"sessionFinal":true,"message":{"id":"final","content":content}}),
    );
    publish("turn/end", json!({"turn":1,"reason":reason}));
    let mut presentation = PresentationStore::default();
    presentation
        .select(ComposerDestination::run(RunScope::main("sep", "s")))
        .unwrap();
    presentation.refresh(&store).unwrap()
}
fn segment_keys(
    source: &RunSnapshot,
) -> (TranscriptActivitySegmentKey, TranscriptActivitySegmentKey) {
    let document = TranscriptDocument::read(&source.transcript).unwrap();
    let section = &document.sections[0];
    (
        TranscriptActivitySegmentKey {
            turn_id: section.id.into(),
            end: TranscriptActivitySegmentEnd::BeforeInput(section.inputs[1].key.into()),
        },
        TranscriptActivitySegmentKey {
            turn_id: section.id.into(),
            end: TranscriptActivitySegmentEnd::Final,
        },
    )
}
#[test]
fn input_bounded_history_counts_semantic_entries_and_preserves_original_parts_and_preedit() {
    let source = segmented_run("completed", true, false);
    let scope = RunScope::main("sep", "s");
    let (before, final_segment) = segment_keys(&source);
    assert_eq!(
        source.turn_planes(&before.turn_id).unwrap().activity,
        TranscriptActivityPresentation::BetweenInputs
    );
    let mut view = RunPort::standard(8000, 8010..8300, scope.clone()).unwrap();
    view.set_thinking_display(TranscriptThinkingDisplayInput {
        mode: TranscriptThinkingMode::Compact,
        display_revision: 1,
        elapsed_seconds: 0,
    })
    .unwrap();
    view.update(
        &source,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    let content = view.transcript().unwrap();
    let button = content
        .row_id(&TranscriptContentRowKey::ActivitySegment(before.clone()))
        .unwrap();
    let final_button = content
        .row_id(&TranscriptContentRowKey::ActivitySegment(
            final_segment.clone(),
        ))
        .unwrap();
    let doc = TranscriptDocument::read(&source.transcript).unwrap();
    for input in &doc.sections[0].inputs {
        assert!(content.message_id(input.key).is_some());
    }
    assert!(content.tool_run(&source.transcript.tools[0].id).is_none());
    let item = &doc.sections[0].outputs[0];
    let owner = TranscriptMessageIdentity::from(item.key);
    let original = content.message_content_ids(&owner);
    assert_eq!(original.len(), 2);
    let ids = view.ids();
    let mut host = view.into_host().unwrap();
    assert_eq!(
        host.get(button)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap()
            .label,
        "Previous 3 messages"
    );
    assert!(
        !host
            .get(final_button)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap()
            .button
            .state
            .selected
    );
    assert!(host.focus(Some(ids.composer.editor), true));
    host.event(&ContentEvent::Preedit {
        text: "分段组字".into(),
        selection: 4..4,
        clauses: vec![],
        replacement: None,
    });
    let baseline = host
        .text_surface(ids.composer.editor)
        .unwrap()
        .input
        .external_baseline();
    let view = host
        .get_mut(ids.root)
        .unwrap()
        .component_mut::<RunPort>()
        .unwrap();
    let stale = view
        .prepare_update(
            &source,
            Default::default(),
            ComposerControlGate::OPEN,
            paging(&scope),
            true,
        )
        .unwrap();
    tekes_agent_ui_consumer_check::set_run_activity_segment(view, &before, true).unwrap();
    assert!(view.validate_prepared_update(&stale).is_err());
    view.update(
        &source,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    let content = view.transcript().unwrap();
    assert!(content.tool_run(&source.transcript.tools[0].id).is_some());
    let mut retained = content.message_content_ids(&owner);
    retained.sort_by_key(|part| part.root.0);
    let mut expected = original;
    expected.sort_by_key(|part| part.root.0);
    assert_eq!(retained, expected);
    assert_eq!(
        content.row_id(&TranscriptContentRowKey::ActivitySegment(before.clone())),
        Some(button)
    );
    let final_row = source
        .transcript
        .items
        .iter()
        .find(|row| row.text == "最终回答")
        .unwrap();
    let final_body = content
        .row_id(&TranscriptContentRowKey::Message(
            TranscriptMessageIdentity::Item(final_row.id.clone()),
        ))
        .unwrap();
    let keys = content.row_keys();
    let positions = |id| {
        keys.iter()
            .position(|key| content.row_id(key) == Some(id))
            .unwrap()
    };
    assert!(
        positions(button) < positions(final_button)
            && positions(final_button) < positions(final_body)
    );
    assert_eq!(host.focused(), Some(ids.composer.editor));
    assert_eq!(
        host.text_surface(ids.composer.editor)
            .unwrap()
            .input
            .external_baseline(),
        baseline
    );
    host.accessibility_action(button, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let view = host
        .get_mut(ids.root)
        .unwrap()
        .component_mut::<RunPort>()
        .unwrap();
    assert!(matches!(
        view.take_intents(&actions).as_slice(),
        [RunPortIntent::TranscriptLayoutChanged { .. }]
    ));
    view.update(
        &source,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    assert!(
        view.transcript()
            .unwrap()
            .tool_run(&source.transcript.tools[0].id)
            .is_none()
    );
    assert_eq!(
        host.text_surface(ids.composer.editor)
            .unwrap()
            .input
            .external_baseline(),
        baseline
    );
    let mut prefix = source.clone();
    prefix.revision += 1;
    let mut older = prefix
        .transcript
        .items
        .iter()
        .find(|item| item.kind == TranscriptItemKind::Assistant)
        .unwrap()
        .clone();
    older.id = "explicit-prefix-fixture".into();
    older.kind = TranscriptItemKind::Notice;
    older.record_id = None;
    older.reconciliation_id = None;
    older.is_final = false;
    older.is_provisional = false;
    older.text = "前缀历史".into();
    older.reasoning.clear();
    older.attachments.clear();
    older.content_blocks = Some(vec![TranscriptContentBlock {
        index: 0,
        content: TranscriptBlockContent::Text {
            text: older.text.clone(),
        },
    }]);
    older.source_sequence = Some(0);
    older.source_position = Some(TranscriptSourcePosition::Journal {
        sequence: 0,
        content_index: None,
    });
    prefix.transcript.items.push(older);
    let view = host
        .get_mut(ids.root)
        .unwrap()
        .component_mut::<RunPort>()
        .unwrap();
    view.update(
        &prefix,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    assert_eq!(
        view.transcript()
            .unwrap()
            .row_id(&TranscriptContentRowKey::ActivitySegment(before)),
        Some(button)
    );
}
#[test]
fn consecutive_inputs_do_not_invent_an_empty_previous_segment_and_final_empty_status_is_passive() {
    let source = segmented_run("completed", false, true);
    let scope = RunScope::main("sep", "s");
    let (before, final_segment) = segment_keys(&source);
    let mut view = RunPort::standard(8400, 8410..8600, scope.clone()).unwrap();
    view.update(
        &source,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    let doc = TranscriptDocument::read(&source.transcript).unwrap();
    let empty = TranscriptActivitySegmentKey {
        turn_id: before.turn_id.clone(),
        end: TranscriptActivitySegmentEnd::BeforeInput(doc.sections[0].inputs[2].key.into()),
    };
    let content = view.transcript().unwrap();
    assert!(
        content
            .row_id(&TranscriptContentRowKey::ActivitySegment(empty.clone()))
            .is_none()
    );
    assert!(
        content
            .row_id(&TranscriptContentRowKey::ActivitySegmentSummary(
                final_segment.clone()
            ))
            .is_some()
    );
    assert!(view.set_activity_segment_expanded(&empty, true).is_err());
    assert!(
        view.set_activity_segment_expanded(&final_segment, true)
            .is_err()
    );
    let mut foreign = before.clone();
    foreign.turn_id = "other".into();
    assert!(view.set_activity_segment_expanded(&foreign, true).is_err());
}
#[test]
fn abnormal_input_bounded_history_stays_collapsed_and_readonly_uses_its_own_plan_generation() {
    let source = segmented_run("error", true, false);
    let scope = RunScope::main("sep", "s");
    let (before, final_segment) = segment_keys(&source);
    let mut view =
        ReadOnlyTranscript::standard(8700, 8710..8900, scope.clone(), Default::default()).unwrap();
    let input = ReadOnlyTranscriptInput {
        scope: &scope,
        publication: 900,
        connection_epoch: source.connection_epoch,
        connection: &source.connection,
        transcript: &source.transcript,
        frame_current: true,
        source_ready: true,
        supports_pagination: false,
        reading: false,
        read_error: None,
    };
    view.update(input).unwrap();
    let button = view
        .content()
        .row_id(&TranscriptContentRowKey::ActivitySegment(final_segment))
        .unwrap();
    let id = view.ids().root;
    let mut host = view.into_host().unwrap();
    let control = host
        .get(button)
        .unwrap()
        .component::<ButtonContent>()
        .unwrap();
    assert!(!control.button.state.selected && control.label.starts_with("Failed"));
    let view = host
        .get_mut(id)
        .unwrap()
        .component_mut::<ReadOnlyTranscript>()
        .unwrap();
    let old = view.prepare_update(input).unwrap();
    tekes_agent_ui_consumer_check::set_readonly_activity_segment(view, &before, true).unwrap();
    assert!(view.validate_prepared_update(&old).is_err());
    view.update(input).unwrap();
    assert!(
        view.content()
            .tool_run(&source.transcript.tools[0].id)
            .is_some()
    );
    assert_eq!(view.content().footnote_timing().live_turn_id, None);
}

fn narration_overlap() -> (RunSnapshot, String, String) {
    let mut source = captured_run();
    source.revision += 1;
    let original = source
        .transcript
        .items
        .iter_mut()
        .find(|row| row.kind == TranscriptItemKind::Assistant && !row.text.is_empty())
        .unwrap();
    let first = original.id.clone();
    original.text = "完整旁白内容".repeat(6);
    original.reasoning = "短思考".into();
    original.content_blocks = Some(vec![
        TranscriptContentBlock {
            index: 0,
            content: TranscriptBlockContent::Reasoning {
                text: original.reasoning.clone(),
            },
        },
        TranscriptContentBlock {
            index: 1,
            content: TranscriptBlockContent::Text {
                text: original.text.clone(),
            },
        },
    ]);
    let mut overlap = original.clone();
    overlap.id = "z-overlapping-source-record".into();
    overlap.record_id = Some("overlap-record".into());
    overlap.reconciliation_id = None;
    overlap.source_sequence = None;
    overlap.source_position = None;
    overlap.text = "短旁白".into();
    overlap.reasoning = "另一个载体中更完整的思考".repeat(6);
    overlap.content_blocks = Some(vec![
        TranscriptContentBlock {
            index: 0,
            content: TranscriptBlockContent::Reasoning {
                text: overlap.reasoning.clone(),
            },
        },
        TranscriptContentBlock {
            index: 1,
            content: TranscriptBlockContent::Text {
                text: overlap.text.clone(),
            },
        },
    ]);
    let second = overlap.id.clone();
    let provenance = source.transcript.thinking_sources.by_id[&first].clone();
    source
        .transcript
        .thinking_sources
        .by_id
        .insert(second.clone(), provenance);
    source.transcript.items.push(overlap);
    (source, first, second)
}
fn narration_view(source: &RunSnapshot, thinking: bool) -> RunPort {
    let scope = RunScope::main("sep", "s");
    let mut view = RunPort::standard(29000, 29010..29400, scope.clone()).unwrap();
    if thinking {
        view.set_thinking_display(TranscriptThinkingDisplayInput {
            mode: TranscriptThinkingMode::Compact,
            display_revision: 1,
            elapsed_seconds: 0,
        })
        .unwrap();
    }
    view.update(
        source,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    view
}
#[test]
fn narration_carrier_keeps_original_bodies_and_independent_reasoning_owner_through_source_handoff()
{
    let (source, first, second) = narration_overlap();
    let untouched = source.clone();
    let first_owner = TranscriptMessageIdentity::Item(first.clone());
    let second_owner = TranscriptMessageIdentity::Item(second.clone());
    let first_key = TranscriptContentRowKey::Message(first_owner.clone());
    let second_key = TranscriptContentRowKey::Message(second_owner.clone());
    let reason_key = TranscriptContentRowKey::MessageFragment(
        second_owner.clone(),
        OutputMessageFragmentKind::Reasoning,
    );
    let view = narration_view(&source, true);
    let ids = view.ids();
    let content = view.transcript().unwrap();
    assert!(content.row_id(&first_key).is_some());
    assert!(content.row_id(&second_key).is_none());
    let reason = content.row_id(&reason_key).unwrap();
    let body = content.row_id(&first_key).unwrap();
    let tool = content
        .row_keys()
        .iter()
        .position(|key| {
            matches!(
                key,
                TranscriptContentRowKey::Tool(_) | TranscriptContentRowKey::ContinuousTools { .. }
            )
        })
        .unwrap();
    let position = |id| {
        content
            .row_keys()
            .iter()
            .position(|key| content.row_id(key) == Some(id))
            .unwrap()
    };
    assert!(position(reason) < position(body) && position(body) < tool);
    let first_parts = content.message_content_ids(&first_owner);
    let second_parts = content.message_content_ids(&second_owner);
    let text = first_parts
        .iter()
        .find(|part| part.kind == OutputContentKind::Text)
        .unwrap()
        .body;
    let mut host = view.into_host().unwrap();
    let owner = host
        .get(text)
        .unwrap()
        .component::<TextFieldContent>()
        .unwrap() as *const _ as usize;
    assert!(host.focus(Some(ids.composer.editor), true));
    host.event(&ContentEvent::Preedit {
        text: "旁白组字".into(),
        selection: 4..4,
        clauses: vec![],
        replacement: None,
    });
    let baseline = host
        .text_surface(ids.composer.editor)
        .unwrap()
        .input
        .external_baseline();
    let first_nodes = vec![
        source
            .transcript
            .items
            .iter()
            .find(|row| row.id == first)
            .unwrap()
            .text
            .clone(),
    ];
    let second_nodes = vec!["短旁白".into()];
    let policy = tekes_agent_ui_consumer_check::narration_policy(
        TranscriptNarrationTimeline::Running,
        &[
            TranscriptNarrationItem {
                item_id: &first,
                frame_id: "captured-frame",
                node_texts: &first_nodes,
                canonical_narration: false,
            },
            TranscriptNarrationItem {
                item_id: &second,
                frame_id: "captured-frame",
                node_texts: &second_nodes,
                canonical_narration: false,
            },
        ],
        &[],
    )
    .unwrap();
    assert_eq!(
        policy.choice_for("captured-frame"),
        Some(&TranscriptNarrationChoice::Item {
            item_id: first.clone()
        })
    );
    let mut next = source.clone();
    next.revision += 1;
    let candidate = next
        .transcript
        .items
        .iter_mut()
        .find(|row| row.id == second)
        .unwrap();
    candidate.text = "比原载体更完整的新旁白".repeat(20);
    if let TranscriptBlockContent::Text { text } =
        &mut candidate.content_blocks.as_mut().unwrap()[1].content
    {
        *text = candidate.text.clone();
    }
    let run = host
        .get_mut(ids.root)
        .unwrap()
        .component_mut::<RunPort>()
        .unwrap();
    let stale = run
        .prepare_update(
            &source,
            Default::default(),
            ComposerControlGate::OPEN,
            paging(&RunScope::main("sep", "s")),
            true,
        )
        .unwrap();
    run.update(
        &next,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&RunScope::main("sep", "s")),
        true,
    )
    .unwrap();
    assert!(run.validate_prepared_update(&stale).is_err());
    assert!(run.transcript().unwrap().row_id(&first_key).is_none());
    assert!(run.transcript().unwrap().row_id(&second_key).is_some());
    assert_eq!(
        run.transcript().unwrap().message_content_ids(&first_owner),
        first_parts
    );
    assert_eq!(
        run.transcript().unwrap().message_content_ids(&second_owner),
        second_parts
    );
    let mut restored = source.clone();
    restored.revision = next.revision + 1;
    host.get_mut(ids.root)
        .unwrap()
        .component_mut::<RunPort>()
        .unwrap()
        .update(
            &restored,
            Default::default(),
            ComposerControlGate::OPEN,
            paging(&RunScope::main("sep", "s")),
            true,
        )
        .unwrap();
    assert_eq!(
        host.get(text)
            .unwrap()
            .component::<TextFieldContent>()
            .unwrap() as *const _ as usize,
        owner
    );
    assert_eq!(host.focused(), Some(ids.composer.editor));
    assert_eq!(
        host.text_surface(ids.composer.editor)
            .unwrap()
            .input
            .external_baseline(),
        baseline
    );
    assert_eq!(source, untouched);
}
#[test]
fn unresolved_rich_body_or_unowned_frame_preserves_each_source_body_instead_of_guessing() {
    let (mut source, first, second) = narration_overlap();
    source.transcript.thinking_sources.scope = None;
    let view = narration_view(&source, false);
    for id in [&first, &second] {
        assert!(
            view.transcript()
                .unwrap()
                .row_id(&TranscriptContentRowKey::Message(
                    TranscriptMessageIdentity::Item(id.clone())
                ))
                .is_some()
        );
    }
    source.transcript.thinking_sources.scope = Some(RunScope::main("sep", "s"));
    let item = source
        .transcript
        .items
        .iter_mut()
        .find(|row| row.id == first)
        .unwrap();
    item.content_blocks
        .as_mut()
        .unwrap()
        .push(TranscriptContentBlock {
            index: 2,
            content: TranscriptBlockContent::Unknown {
                content: json!({"value":"keep"}),
            },
        });
    let view = narration_view(&source, true);
    for id in [&first, &second] {
        assert!(
            view.transcript()
                .unwrap()
                .row_id(&TranscriptContentRowKey::Message(
                    TranscriptMessageIdentity::Item(id.clone())
                ))
                .is_some()
        );
    }
}

#[cfg(target_os = "macos")]
#[path = "support/narration_reading.rs"]
mod narration_reading;

#[test]
fn embedded_original_tool_controls_do_not_group_across_source_input_but_results_do_not_split_a_run()
{
    let source = embedded_input_run();
    let scope = RunScope::main("sep", "s");
    let document = TranscriptDocument::read(&source.transcript).unwrap();
    let section = &document.sections[0];
    let output = section
        .outputs
        .iter()
        .find(|output| {
            output.item.content_blocks.as_ref().is_some_and(|blocks| {
                blocks
                    .iter()
                    .filter(|block| {
                        matches!(block.content, TranscriptBlockContent::ToolCall { .. })
                    })
                    .count()
                    == 3
            })
        })
        .unwrap();
    let calls: Vec<_> = section.tools.iter().map(|tool| tool.id.clone()).collect();
    assert_eq!(
        tekes_agent_ui_consumer_check::embedded_tool_input_boundary(section, &calls[0], &calls[1])
            .unwrap(),
        Some(true)
    );
    assert_eq!(
        tekes_agent_ui_consumer_check::embedded_tool_input_boundary(section, &calls[1], &calls[2])
            .unwrap(),
        Some(false)
    );
    let mut view = OutputMessage::new(31000, scope.clone());
    view.update(&scope, 1, source.connection_epoch, output, Some(section))
        .unwrap();
    assert!(
        view.tool_run(&calls[0]).is_none(),
        "first call stays its original standalone part"
    );
    let group = view.tool_run(&calls[1]).unwrap();
    assert_eq!(group.summary().unwrap().tool_ids, calls[1..]);
    assert_eq!(view.tool_run(&calls[2]).unwrap().id(), group.id());
    let original_parts = view.content_ids();
    assert_eq!(
        original_parts
            .iter()
            .filter(|part| part.kind == OutputContentKind::Tool)
            .count(),
        3
    );
    let root = view.id();
    let mut host = view.into_host().unwrap();
    let original = original_parts
        .iter()
        .find(|part| part.kind == OutputContentKind::Tool)
        .unwrap()
        .disclosure
        .unwrap();
    let owner = host
        .get(original)
        .unwrap()
        .component::<ButtonContent>()
        .unwrap() as *const _ as usize;
    host.get_mut(root)
        .unwrap()
        .component_mut::<OutputMessage>()
        .unwrap()
        .update(&scope, 2, source.connection_epoch, output, Some(section))
        .unwrap();
    assert_eq!(
        host.get(original)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap() as *const _ as usize,
        owner
    );
    assert_eq!(
        host.get(root)
            .unwrap()
            .component::<OutputMessage>()
            .unwrap()
            .content_ids(),
        original_parts
    );
}

fn embedded_input_run() -> RunSnapshot {
    let snapshot: serde_json::Value = serde_json::from_str(include_str!(
        "../../../crates/presentation/tests/fixtures/tool-input-boundary-20261023.json"
    ))
    .unwrap();
    let mut store = MemoryProjectionStore::new("sep");
    store
        .apply("journal:s", json!({"type":"snapshot","snapshot":snapshot}))
        .unwrap();
    store.connection = ConnectionState::Ready;
    store.inventory_ready = true;
    store.control_ready = true;
    store.actionables_ready = true;
    let mut presentation = PresentationStore::default();
    let scope = RunScope::main("sep", "s");
    presentation
        .select(ComposerDestination::run(scope.clone()))
        .unwrap();
    presentation.refresh(&store).unwrap()
}

fn embedded_segments(
    source: &RunSnapshot,
) -> (
    TranscriptMessageIdentity,
    TranscriptMessageIdentity,
    TranscriptActivitySegmentKey,
    TranscriptActivitySegmentKey,
) {
    let document = TranscriptDocument::read(&source.transcript).unwrap();
    let section = &document.sections[0];
    let output = section
        .outputs
        .iter()
        .find(|output| {
            output.item.content_blocks.as_ref().is_some_and(|blocks| {
                blocks
                    .iter()
                    .any(|block| matches!(block.content, TranscriptBlockContent::ToolCall { .. }))
            })
        })
        .unwrap();
    let inner: TranscriptMessageIdentity = section.inputs[1].key.into();
    (
        output.key.into(),
        inner.clone(),
        TranscriptActivitySegmentKey {
            turn_id: section.id.into(),
            end: TranscriptActivitySegmentEnd::BeforeInput(inner),
        },
        TranscriptActivitySegmentKey {
            turn_id: section.id.into(),
            end: TranscriptActivitySegmentEnd::Final,
        },
    )
}

fn canonical_parts(content: &TranscriptContent, first: &str) -> Vec<OutputContentIds> {
    let group = content.tool_run(first).unwrap();
    group
        .summary()
        .unwrap()
        .tool_ids
        .iter()
        .map(|id| group.tool_activity(id).unwrap().content_ids().unwrap())
        .collect()
}
fn canonical_owners(
    content: &TranscriptContent,
    source: &RunSnapshot,
) -> Vec<(ComponentId, usize)> {
    source
        .transcript
        .tools
        .iter()
        .map(|tool| {
            let control = content.retained_tool_activity(&tool.id).unwrap();
            assert_eq!(
                tekes_agent_ui_consumer_check::retained_tool_control_id(content, &tool.id),
                Some(control.id())
            );
            (control.id(), control as *const _ as usize)
        })
        .collect()
}
#[test]
fn canonical_call_controls_are_physical_rows_in_their_own_input_segments() {
    let source = origin_tool_run();
    let scope = RunScope::main("sep", "s");
    let (_, inner, before, final_segment) = embedded_segments(&source);
    let mut run = RunPort::standard(32000, 32010..32500, scope.clone()).unwrap();
    run.update(
        &source,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    let original = canonical_owners(run.transcript().unwrap(), &source);
    assert!(tool_owners(&source).iter().all(|owner| {
        run.transcript()
            .unwrap()
            .message_content_ids(owner)
            .is_empty()
    }));
    let before_id = run
        .transcript()
        .unwrap()
        .row_id(&TranscriptContentRowKey::ActivitySegment(before.clone()))
        .unwrap();
    let final_id = run
        .transcript()
        .unwrap()
        .row_id(&TranscriptContentRowKey::ActivitySegment(
            final_segment.clone(),
        ))
        .unwrap();
    let ids = run.ids();
    let mut host = run.into_host().unwrap();
    assert_eq!(
        host.get(before_id)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap()
            .label,
        "Previous 1 message"
    );
    assert!(host.focus(Some(ids.composer.editor), true));
    host.event(&ContentEvent::Preedit {
        text: "工具组字".into(),
        selection: 4..4,
        clauses: vec![],
        replacement: None,
    });
    let baseline = host
        .text_surface(ids.composer.editor)
        .unwrap()
        .input
        .external_baseline();
    let run = host
        .get_mut(ids.root)
        .unwrap()
        .component_mut::<RunPort>()
        .unwrap();
    let stale = run
        .prepare_update(
            &source,
            Default::default(),
            ComposerControlGate::OPEN,
            paging(&scope),
            true,
        )
        .unwrap();
    for segment in [&before, &final_segment] {
        run.set_activity_segment_expanded(segment, true).unwrap();
    }
    assert!(run.validate_prepared_update(&stale).is_err());
    let plan = run
        .prepare_update(
            &source,
            Default::default(),
            ComposerControlGate::OPEN,
            paging(&scope),
            true,
        )
        .unwrap();
    let predicted = plan.item_ids();
    run.apply_prepared_update(&plan).unwrap();
    let content = run.transcript().unwrap();
    let rows = tool_rows(content);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|(_, id)| predicted.contains(id)));
    let position = |id| {
        content
            .row_keys()
            .iter()
            .position(|key| content.row_id(key) == Some(id))
            .unwrap()
    };
    let inner_id = content
        .row_id(&TranscriptContentRowKey::Message(inner))
        .unwrap();
    assert!(position(rows[0].1) < position(inner_id));
    assert!(position(inner_id) < position(rows[1].1));
    assert!(position(final_id) < position(rows[1].1));
    assert_eq!(canonical_owners(content, &source), original);
    let run = host
        .get_mut(ids.root)
        .unwrap()
        .component_mut::<RunPort>()
        .unwrap();
    for expanded in [false, true] {
        for segment in [&before, &final_segment] {
            run.set_activity_segment_expanded(segment, expanded)
                .unwrap();
        }
        run.update(
            &source,
            Default::default(),
            ComposerControlGate::OPEN,
            paging(&scope),
            true,
        )
        .unwrap();
    }
    assert_eq!(tool_rows(run.transcript().unwrap()), rows);
    assert_eq!(
        canonical_owners(run.transcript().unwrap(), &source),
        original
    );
    let stolen = host
        .get_mut(rows[0].1)
        .unwrap()
        .component_mut::<ToolRun>()
        .unwrap()
        .children_mut()
        .unwrap()
        .pop()
        .unwrap();
    assert!(
        host.get(ids.root)
            .unwrap()
            .component::<RunPort>()
            .unwrap()
            .prepare_update(
                &source,
                Default::default(),
                ComposerControlGate::OPEN,
                paging(&scope),
                true
            )
            .is_err()
    );
    host.get_mut(rows[0].1)
        .unwrap()
        .component_mut::<ToolRun>()
        .unwrap()
        .children_mut()
        .unwrap()
        .push(stolen);
    assert!(
        host.get(ids.root)
            .unwrap()
            .component::<RunPort>()
            .unwrap()
            .prepare_update(
                &source,
                Default::default(),
                ComposerControlGate::OPEN,
                paging(&scope),
                true
            )
            .is_ok()
    );
    assert_eq!(host.focused(), Some(ids.composer.editor));
    assert_eq!(
        host.text_surface(ids.composer.editor)
            .unwrap()
            .input
            .external_baseline(),
        baseline
    );
}
fn origin_tool_run() -> RunSnapshot {
    let snapshot: serde_json::Value = serde_json::from_str(include_str!(
        "../../../crates/presentation/tests/fixtures/tool-origin-input-boundary-20261023.json"
    ))
    .unwrap();
    let mut store = MemoryProjectionStore::new("sep");
    store
        .apply("journal:s", json!({"type":"snapshot","snapshot":snapshot}))
        .unwrap();
    store.connection = ConnectionState::Ready;
    let mut presentation = PresentationStore::default();
    presentation
        .select(ComposerDestination::run(RunScope::main("sep", "s")))
        .unwrap();
    presentation.refresh(&store).unwrap()
}
fn tool_owners(source: &RunSnapshot) -> Vec<TranscriptMessageIdentity> {
    TranscriptDocument::read(&source.transcript)
        .unwrap()
        .sections[0]
        .outputs
        .iter()
        .filter(|output| {
            output.item.content_blocks.as_ref().is_some_and(|blocks| {
                blocks
                    .iter()
                    .any(|block| matches!(block.content, TranscriptBlockContent::ToolCall { .. }))
            })
        })
        .map(|output| output.key.into())
        .collect()
}
fn tool_rows(content: &TranscriptContent) -> Vec<(String, ComponentId)> {
    content
        .row_keys()
        .iter()
        .filter_map(|key| {
            if let TranscriptContentRowKey::ContinuousTools { first_tool_id, .. } = key {
                Some((first_tool_id.clone(), content.row_id(key).unwrap()))
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn late_message_references_leave_the_original_independent_tool_owners_in_place() {
    let snapshot: serde_json::Value = serde_json::from_str(include_str!(
        "../../../crates/presentation/tests/fixtures/tool-input-boundary-20261023.json"
    ))
    .unwrap();
    let mut prefix = snapshot.clone();
    prefix["throughSequence"] = json!(6);
    prefix["entries"]
        .as_array_mut()
        .unwrap()
        .retain(|entry| entry["event"]["seq"].as_i64().unwrap() <= 6);
    let mut store = MemoryProjectionStore::new("sep");
    store
        .apply("journal:s", json!({"type":"snapshot","snapshot":prefix}))
        .unwrap();
    store.connection = ConnectionState::Ready;
    let scope = RunScope::main("sep", "s");
    let mut presentation = PresentationStore::default();
    presentation
        .select(ComposerDestination::run(scope.clone()))
        .unwrap();
    let first = presentation.refresh(&store).unwrap();
    let calls: Vec<_> = first
        .transcript
        .tools
        .iter()
        .map(|tool| tool.id.clone())
        .collect();
    let mut run = RunPort::standard(33000, 33010..33500, scope.clone()).unwrap();
    run.update(
        &first,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    let owners: Vec<_> = calls
        .iter()
        .map(|id| {
            let tool = run
                .transcript()
                .unwrap()
                .tool_run(id)
                .unwrap()
                .tool_activity(id)
                .unwrap();
            (tool.id(), tool as *const _ as usize)
        })
        .collect();
    for entry in snapshot["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["event"]["seq"].as_i64().unwrap() > 6)
    {
        store
            .apply(
                "journal:s",
                json!({"type":"event","generation":"g","sessionId":"s","entry":entry}),
            )
            .unwrap();
    }
    let next = presentation.refresh(&store).unwrap();
    run.update(
        &next,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    let (owner, _, before, final_segment) = embedded_segments(&next);
    assert!(
        run.transcript()
            .unwrap()
            .message_content_ids(&owner)
            .is_empty(),
        "late references do not create duplicate tool parts or an empty status row"
    );
    for segment in [&before, &final_segment] {
        run.set_activity_segment_expanded(segment, true).unwrap();
    }
    run.update(
        &next,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    for (id, expected) in calls.iter().zip(owners) {
        let tool = run
            .transcript()
            .unwrap()
            .tool_run(id)
            .unwrap()
            .tool_activity(id)
            .unwrap();
        assert_eq!((tool.id(), tool as *const _ as usize), expected);
    }
    assert!(tool_owners(&next).iter().all(|owner| {
        run.transcript()
            .unwrap()
            .message_tool_fragment_ids(owner)
            .is_empty()
    }));
    let before_id = run
        .transcript()
        .unwrap()
        .row_id(&TranscriptContentRowKey::ActivitySegment(before))
        .unwrap();
    let host = run.into_host().unwrap();
    assert_eq!(
        host.get(before_id)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap()
            .label,
        "Previous 1 message"
    );
}

#[test]
fn controlled_thinking_repartitions_global_runs_without_replacing_canonical_calls() {
    let source = cross_owner_tool_run(false);
    let scope = RunScope::main("sep", "s");
    let mut run = RunPort::standard(34000, 34010..34500, scope.clone()).unwrap();
    run.update(
        &source,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    let original = canonical_owners(run.transcript().unwrap(), &source);
    for (index, mode) in [
        TranscriptThinkingMode::None,
        TranscriptThinkingMode::Compact,
        TranscriptThinkingMode::None,
    ]
    .into_iter()
    .enumerate()
    {
        run.set_thinking_display(TranscriptThinkingDisplayInput {
            mode,
            display_revision: index as u64 + 1,
            elapsed_seconds: 0,
        })
        .unwrap();
        let plan = run
            .prepare_update(
                &source,
                Default::default(),
                ComposerControlGate::OPEN,
                paging(&scope),
                true,
            )
            .unwrap();
        let predicted = plan.item_ids();
        run.apply_prepared_update(&plan).unwrap();
        let content = run.transcript().unwrap();
        assert_eq!(
            content
                .row_keys()
                .iter()
                .map(|key| content.row_id(key).unwrap())
                .collect::<Vec<_>>(),
            predicted
        );
        assert_eq!(
            tool_rows(content).len(),
            if mode == TranscriptThinkingMode::Compact {
                2
            } else {
                1
            },
            "mode={mode:?}, rows={:?}",
            content.row_keys()
        );
        assert_eq!(canonical_owners(content, &source), original);
    }
}
#[test]
fn settled_tool_history_retains_reasoning_even_when_live_thinking_is_hidden() {
    let source = cross_owner_tool_run(true);
    let scope = RunScope::main("sep", "s");
    let mut run = RunPort::standard(35000, 35010..35500, scope.clone()).unwrap();
    run.update(
        &source,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    let original = canonical_owners(run.transcript().unwrap(), &source);
    run.set_activity_expanded(&source.transcript.turns[0].id, true)
        .unwrap();
    for (index, mode) in [
        TranscriptThinkingMode::Compact,
        TranscriptThinkingMode::None,
    ]
    .into_iter()
    .enumerate()
    {
        run.set_thinking_display(TranscriptThinkingDisplayInput {
            mode,
            display_revision: index as u64 + 1,
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
        assert_eq!(tool_rows(content).len(), 2);
        assert!(content.row_keys().iter().any(|key| matches!(
            key,
            TranscriptContentRowKey::MessageFragment(_, OutputMessageFragmentKind::Reasoning)
        )));
        assert_eq!(canonical_owners(content, &source), original);
    }
}

fn cross_owner_tool_run(settled: bool) -> RunSnapshot {
    let mut snapshot: serde_json::Value = serde_json::from_str(include_str!(
        "../../../crates/presentation/tests/fixtures/tool-origin-input-boundary-20261023.json"
    ))
    .unwrap();
    snapshot["entries"]
        .as_array_mut()
        .unwrap()
        .retain(|entry| entry["event"]["seq"] != 3);
    if !settled {
        snapshot["entries"]
            .as_array_mut()
            .unwrap()
            .retain(|entry| entry["event"]["seq"].as_i64().unwrap() < 8);
        snapshot["throughSequence"] = json!(5);
    }
    let content = snapshot["entries"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["event"]["seq"] == 4)
        .unwrap()["event"]["data"]["message"]["content"]
        .as_array_mut()
        .unwrap();
    content.insert(0, json!({"type":"reasoning", "text":"保留原思考"}));
    let mut store = MemoryProjectionStore::new("sep");
    store
        .apply(
            "journal:s",
            serde_json::json!({"type":"snapshot","snapshot":snapshot}),
        )
        .unwrap();
    store.connection = ConnectionState::Ready;
    let mut presentation = PresentationStore::default();
    presentation
        .select(ComposerDestination::run(RunScope::main("sep", "s")))
        .unwrap();
    presentation.refresh(&store).unwrap()
}

#[test]
fn cumulative_frame_upsert_keeps_original_calls_header_and_composer_preedit() {
    let mut snapshot: serde_json::Value = serde_json::from_str(include_str!(
        "../../../crates/presentation/tests/fixtures/tool-origin-input-boundary-20261023.json"
    ))
    .unwrap();
    snapshot["throughSequence"] = json!(4);
    snapshot["entries"]
        .as_array_mut()
        .unwrap()
        .retain(|entry| entry["event"]["seq"].as_i64().unwrap() <= 4);
    let mut store = MemoryProjectionStore::new("sep");
    store
        .apply("journal:s", json!({"type":"snapshot","snapshot":snapshot}))
        .unwrap();
    store.connection = ConnectionState::Ready;
    let scope = RunScope::main("sep", "s");
    let mut presentation = PresentationStore::default();
    presentation
        .select(ComposerDestination::run(scope.clone()))
        .unwrap();
    let first = presentation.refresh(&store).unwrap();
    assert_eq!(first.transcript.tools.len(), 3);
    let old_positions: Vec<_> = first
        .transcript
        .tools
        .iter()
        .map(|call| (call.id.clone(), call.source_position.clone()))
        .collect();
    let b = first
        .transcript
        .tools
        .iter()
        .find(|call| call.arguments["path"] == "b")
        .unwrap()
        .id
        .clone();
    let frame = TranscriptDocument::read(&first.transcript)
        .unwrap()
        .sections[0]
        .outputs
        .iter()
        .find(|output| {
            output.item.content_blocks.as_ref().is_some_and(|blocks| {
                blocks.iter().any(|block|
            matches!(&block.content, TranscriptBlockContent::ToolCall { tool_id } if tool_id == &b))
            })
        })
        .unwrap()
        .item
        .clone();
    let mut run = RunPort::standard(36000, 36010..36500, scope.clone()).unwrap();
    run.update(
        &first,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope),
        true,
    )
    .unwrap();
    let owners = canonical_owners(run.transcript().unwrap(), &first);
    let header = run
        .transcript()
        .unwrap()
        .tool_run(&b)
        .unwrap()
        .disclosure_id()
        .unwrap();
    let header_owner = run.transcript().unwrap().tool_run(&b).unwrap().id();
    let stale = run
        .prepare_update(
            &first,
            Default::default(),
            ComposerControlGate::OPEN,
            paging(&scope),
            true,
        )
        .unwrap();
    let ids = run.ids();
    let mut host = run.into_host().unwrap();
    let header_widget = host
        .get(header)
        .unwrap()
        .component::<ButtonContent>()
        .unwrap() as *const _ as usize;
    assert!(host.focus(Some(ids.composer.editor), true));
    host.event(&ContentEvent::Preedit {
        text: "连续工具更新".into(),
        selection: 6..6,
        clauses: vec![],
        replacement: None,
    });
    let baseline = host
        .text_surface(ids.composer.editor)
        .unwrap()
        .input
        .external_baseline();
    let events = [
        json!({"type":"tool/result","seq":5,"time":95,"data":{"turn":1,"message":{"source":{"callId":"a"},"content":[{"type":"text","text":"a result"}]}}}),
        json!({"type":"assistant/message","seq":6,"time":94,"data":{"turn":1,"step":2,"message":{"id":"bc-origin","content":[
            {"type":"tool-call","id":"b","name":"read","arguments":{"path":"b"}},
            {"type":"tool-call","id":"c","name":"read","arguments":{"path":"c"}},
            {"type":"tool-call","id":"d","name":"read","arguments":{"path":"d"}}
        ]}}}),
        json!({"type":"tool/result","seq":7,"time":93,"data":{"turn":1,"message":{"source":{"callId":"b"},"content":[{"type":"text","text":"b result"}]}}}),
        json!({"type":"tool/result","seq":8,"time":92,"data":{"turn":1,"message":{"source":{"callId":"c"},"content":[{"type":"text","text":"c result"}]}}}),
        json!({"type":"tool/result","seq":9,"time":91,"data":{"turn":1,"message":{"source":{"callId":"d"},"content":[{"type":"text","text":"d result"}]}}}),
        json!({"type":"assistant/message","seq":10,"time":90,"data":{"turn":1,"step":3,"sessionFinal":true,"message":{"id":"final","content":[{"type":"text","text":"完成"}]}}}),
        json!({"type":"turn/end","seq":11,"time":89,"data":{"turn":1,"reason":"completed"}}),
    ];
    for (index, event) in events.into_iter().enumerate() {
        store
            .apply(
                "journal:s",
                json!({"type":"event","generation":"g","sessionId":"s","entry":{"event":event}}),
            )
            .unwrap();
        let next = presentation.refresh(&store).unwrap();
        for (id, position) in &old_positions {
            assert_eq!(
                &next
                    .transcript
                    .tools
                    .iter()
                    .find(|call| &call.id == id)
                    .unwrap()
                    .source_position,
                position
            );
        }
        if index >= 1 {
            assert_eq!(next.transcript.tools.len(), 4);
            let changed_frame = next
                .transcript
                .items
                .iter()
                .find(|item| item.id == frame.id)
                .unwrap();
            assert_ne!(
                changed_frame.source_position, frame.source_position,
                "the message frame really moved while old calls retained their source"
            );
        }
        let run = host
            .get_mut(ids.root)
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
        if index == 6 {
            let (_, _, before, final_segment) = embedded_segments(&next);
            for segment in [&before, &final_segment] {
                run.set_activity_segment_expanded(segment, true).unwrap();
            }
            run.update(
                &next,
                Default::default(),
                ComposerControlGate::OPEN,
                paging(&scope),
                true,
            )
            .unwrap();
        }
        assert!(run.validate_prepared_update(&stale).is_err());
        let content = run.transcript().unwrap();
        assert_eq!(canonical_owners(content, &first), owners);
        let group = content.tool_run(&b).unwrap();
        assert_eq!(group.id(), header_owner);
        assert_eq!(group.disclosure_id(), Some(header));
        assert_eq!(
            group.summary().unwrap().tool_ids.len(),
            if index == 0 { 2 } else { 3 }
        );
        assert!(
            tool_owners(&next)
                .iter()
                .all(|owner| content.message_tool_fragment_ids(owner).is_empty())
        );
        assert_eq!(
            host.get(header)
                .unwrap()
                .component::<ButtonContent>()
                .unwrap() as *const _ as usize,
            header_widget
        );
        assert_eq!(host.focused(), Some(ids.composer.editor));
        assert_eq!(
            host.text_surface(ids.composer.editor)
                .unwrap()
                .input
                .external_baseline(),
            baseline
        );
    }
}
