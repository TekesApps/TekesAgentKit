//! Second consumer: depends only on AgentKit's public UI packages.
/// A host acknowledges an edit against its original binding, never the newest
/// selected pane. The component keeps rejected text visible and does not retry.
pub fn apply_annotation_change(
    binding: &tekes_agent_ui_bindings::RunBinding,
    editor: &mut tekes_agent_components_ui::AnnotationEditor,
    change: &tekes_agent_components_ui::AnnotationEditChange,
) -> bool {
    let result = binding
        .edit_annotation(change)
        .map_err(|error| error.to_string());
    let applied = matches!(&result, Ok(Some(_)));
    editor.complete(change, result);
    applied
}
#[test]
fn independent_host_mounts_the_annotation_editor_without_a_run_surface_or_private_types() {
    let editor = tekes_agent_components_ui::AnnotationEditor::new(87, Default::default());
    let root = editor.ids().root;
    let host = editor.into_host().unwrap();
    assert!(
        !host
            .get(root)
            .unwrap()
            .component::<tekes_agent_components_ui::AnnotationEditor>()
            .unwrap()
            .is_open()
    );
}
/// A second host can create through its original endpoint binding without
/// importing runtime internals or a demo host. It decides whether to navigate.
pub async fn create_initial_session(
    binding: &tekes_agent_ui_bindings::RunBinding,
    creation: &tekes_agent_ui_bindings::InitialSessionCreation,
) -> Result<String, String> {
    binding
        .create_initial_session(creation)
        .await
        .map(|created| created.session_id)
        .map_err(|error| error.to_string())
}
/// Initial preset content is mounted before creation. Hosts retain navigation,
/// the source/request lifetime and explicit controller create inputs.
pub fn initial_preset_panel(
    namespace: u32,
    source: &tekes_agent_ui_bindings::InitialPresetChoices,
    destination: tekes_agent_components_ui::ComposerDestination,
) -> Result<tekes_agent_components_ui::ComposerChoices, String> {
    use tekes_agent_components_ui::{ComposerChoiceKind, ComposerChoices};
    if !matches!(
        &destination,
        tekes_agent_components_ui::ComposerDestination::NewThread { .. }
    ) {
        return Err("existing session retains its initial preset".into());
    }
    let mut panel = ComposerChoices::new(namespace, Default::default());
    let request = panel
        .open(destination, ComposerChoiceKind::Preset)
        .map_err(str::to_owned)?;
    panel
        .update(&request, source.catalog.clone())
        .map_err(str::to_owned)?;
    Ok(panel)
}
#[test]
fn independent_host_mounts_context_inside_composer_with_an_explicit_separate_namespace() {
    use tekes_agent_components_ui::{Composer, ComposerIds};
    let composer = Composer::new(ComposerIds::new(3000))
        .with_context(86, Default::default())
        .unwrap();
    let context = composer.context().unwrap().id();
    let editor = composer.ids().editor;
    let host = composer.into_host().unwrap();
    assert!(
        host.get(context).is_none(),
        "an empty context does not add a layout slot"
    );
    assert!(host.get(editor).is_some());
}
pub fn sync_context(
    context: &mut tekes_agent_components_ui::ComposerContext,
    source: &tekes_agent_ui_bindings::RunBinding,
) -> Result<(), String> {
    let snapshot = source
        .current_run()
        .ok_or_else(|| "missing or read-only run".to_owned())?;
    context
        .update(
            &snapshot,
            source.access() == tekes_agent_ui_bindings::PaneAccess::Ready,
            Default::default(),
        )
        .map_err(str::to_owned)
}
#[test]
fn independent_host_mounts_composer_choices_without_a_catalog_service_or_run_surface() {
    use tekes_agent_components_ui::{
        ComposerChoiceKind, ComposerChoices, ComposerDestination, RunScope,
    };
    let mut choices = ComposerChoices::new(85, Default::default());
    let request = choices
        .open(
            ComposerDestination::run(RunScope::main("public", "session")),
            ComposerChoiceKind::Preset,
        )
        .unwrap();
    let root = choices.id();
    let host = choices.into_host().unwrap();
    assert_eq!(
        host.get(root)
            .unwrap()
            .component::<ComposerChoices>()
            .unwrap()
            .request(),
        Some(&request)
    );
}
/// Hosts use the binding's current pointer authority rather than caching a tab's
/// creation phase or treating an absent ephemeral pointer as a new conversation.
pub fn pane_is_present(binding: &tekes_agent_ui_bindings::RunBinding) -> bool {
    binding.access() != tekes_agent_ui_bindings::PaneAccess::Missing
}
/// Task tab identity and loading/missing/error inputs come from the shared task
/// projection, never a cached session ID selected by this consumer.
pub fn resolved_task_tab(
    source: &tekes_agent_ui_bindings::TaskPaneBinding,
) -> Result<tekes_agent_components_ui::SidePanelTab, String> {
    source.tab("Task").map_err(|error| error.to_string())
}
/// Child layout and actual receiving parent come from the typed shared source.
pub fn child_tab(
    source: &tekes_agent_ui_bindings::ChildPaneBinding,
) -> tekes_agent_components_ui::SidePanelTab {
    source.tab("Subagent")
}
#[test]
fn independent_consumer_mounts_reusable_thread_rename_without_a_run_or_harness() {
    let view = tekes_agent_components_ui::ThreadRename::new(84, Default::default());
    let root = view.ids().root;
    let host = view.into_host().unwrap();
    assert!(
        !host
            .get(root)
            .unwrap()
            .component::<tekes_agent_components_ui::ThreadRename>()
            .unwrap()
            .is_open()
    );
}
/// A second host composes the canonical scratch tab inventory without importing
/// presentation internals or manufacturing pointers from session-list rows.
pub fn sync_scratch_tabs(
    panel: &mut tekes_agent_components_ui::SidePanel,
    source: &tekes_agent_ui_bindings::SideChatBinding,
    parent: &tekes_agent_ui_bindings::RunBinding,
    revision: u64,
) -> Result<(), String> {
    let input = source
        .input(
            revision,
            parent.scope(),
            vec![tekes_agent_components_ui::SidePanelTab::summary("Summary")],
            Some(tekes_agent_components_ui::SidePanelTabKind::Summary),
        )
        .map_err(|error| error.to_string())?;
    panel.update(&input).map_err(str::to_owned)
}
/// Consume the observed shared inventory through public APIs only. Selection is
/// a host input; this consumer doesn't create a thread model or a pin database.
pub fn sync_observed_sidebar(
    sidebar: &mut tekes_agent_components_ui::Sidebar,
    source: &tekes_agent_ui_bindings::SidebarBinding,
) -> Result<(), &'static str> {
    sidebar.update(&source.snapshot().input(None))
}
pub fn new_run_surface() -> tekes_agent_components_ui::RunSurface {
    tekes_agent_components_ui::RunSurface::new()
}
#[test]
fn mounts_without_harness_or_private_control_access() {
    let surface = new_run_surface();
    assert!(!surface.composing());
}

/// Standard component consumer, independent of the protocol test RunSurface.
pub fn new_composer(namespace: u64) -> tekes_agent_components_ui::Composer {
    tekes_agent_components_ui::Composer::new(tekes_agent_components_ui::ComposerIds::new(namespace))
}

#[test]
fn standard_composer_mounts_as_a_public_component_in_an_independent_host() {
    let composer = new_composer(2000);
    let ids = composer.ids();
    let host = composer.into_host().unwrap();
    let component = host
        .get(ids.root)
        .unwrap()
        .component::<tekes_agent_components_ui::Composer>()
        .unwrap();
    assert_eq!(component.destination(), None);
    assert!(!component.is_composing());
    assert!(host.get(ids.editor).is_some());
    assert!(
        host.get(ids.model).is_none(),
        "host capabilities must be explicit"
    );
}

#[test]
fn independent_host_composes_sidebar_panel_composer_and_indicators_using_only_public_api() {
    use tekes_agent_components_ui::{
        ComposerIds, Indicators, SidePanel, SidePanelLabels, Sidebar, SidebarLabels,
    };
    use tekes_ui_controls::{
        composition::*,
        extended::{Axis, Stack},
    };
    let composer = new_composer(4000);
    let composer_ids = ComposerIds::new(4000);
    let sidebar = Sidebar::new(80, SidebarLabels::default());
    let sidebar_id = sidebar.id();
    let panel = SidePanel::new(81, SidePanelLabels::default(), 4);
    let panel_id = panel.id();
    let close_id = panel.close_id();
    let create_id = panel.create_side_chat_id();
    let host = ContentHost::new(Content::new(
        ComponentId(9000),
        StackContent {
            model: Stack {
                axis: Axis::Horizontal,
                gap: 8.,
            },
            flexible_child: None,
            children: vec![
                Content::new(sidebar_id, sidebar),
                Content::new(composer_ids.root, composer),
                Content::new(panel_id, panel),
                Content::new(ComponentId(9001), Indicators::default()),
            ],
        },
    ))
    .unwrap();
    assert!(
        host.get(sidebar_id)
            .unwrap()
            .component::<Sidebar>()
            .is_some()
    );
    assert!(
        host.get(panel_id)
            .unwrap()
            .component::<SidePanel>()
            .is_some()
    );
    for id in [close_id, create_id] {
        assert!(
            host.semantics()
                .iter()
                .find(|node| node.id == id)
                .unwrap()
                .semantics
                .disabled,
            "unknown panel authority must not advertise an enabled operation"
        );
    }
    assert!(host.get(composer_ids.editor).is_some());
    assert!(
        host.get(ComponentId(9001))
            .unwrap()
            .component::<Indicators>()
            .is_some()
    );
}

#[test]
fn public_summary_mounts_without_a_runtime_or_harness_dependency() {
    let summary = tekes_agent_components_ui::SummaryView::new(
        82,
        tekes_agent_components_ui::SummaryLabels::default(),
    );
    let id = summary.id();
    let host = summary.into_host().unwrap();
    assert!(
        host.get(id)
            .unwrap()
            .component::<tekes_agent_components_ui::SummaryView>()
            .is_some()
    );
}

/// A second host supplies bounded immutable bytes from its own system picker.
/// It keeps the captured request through completion, including uncertain errors.
/// This helper does not import runtime internals, paths or the demo host.
pub async fn upload_selected_files(
    request: &tekes_agent_ui_bindings::AttachmentSelection,
    files: &[tekes_agent_ui_bindings::AttachmentFile],
) -> Result<tekes_agent_ui_bindings::AttachmentReport, String> {
    request
        .upload(files)
        .await
        .map_err(|error| error.to_string())
}

/// An independent host mounts operation facts in its chosen layout. It can
/// subscribe to AttachmentSelection without adding another Run state machine.
pub fn upload_status(namespace: u32) -> tekes_agent_components_ui::ComposerUploads {
    tekes_agent_components_ui::ComposerUploads::new(namespace, Default::default())
}
#[test]
fn independent_host_mounts_upload_status_without_a_demo_or_platform_dependency() {
    let status = upload_status(88);
    let root = status.id();
    let host = status.into_host().unwrap();
    assert!(
        host.get(root)
            .unwrap()
            .component::<tekes_agent_components_ui::ComposerUploads>()
            .is_some()
    );
}

/// Independent hosts receive immutable bytes and own their viewer capability.
pub async fn read_file_preview(
    request: &tekes_agent_ui_bindings::FilePreviewRequest,
) -> Result<tekes_agent_ui_bindings::FilePreview, String> {
    request.read().await.map_err(|error| error.to_string())
}

/// A second host can mount reasoning content without importing a demo or any
/// platform control. Its selected model and current draft effort are explicit.
pub fn reasoning_panel(
    namespace: u32,
    destination: tekes_agent_components_ui::ComposerDestination,
    source: &tekes_agent_ui_bindings::ReasoningChoices,
) -> Result<tekes_agent_components_ui::ComposerChoices, String> {
    let mut panel = tekes_agent_components_ui::ComposerChoices::new(namespace, Default::default());
    let request = panel
        .open(
            destination,
            tekes_agent_components_ui::ComposerChoiceKind::Reasoning,
        )
        .map_err(str::to_owned)?;
    panel
        .update(&request, source.catalog.clone())
        .map_err(str::to_owned)?;
    Ok(panel)
}
#[test]
fn second_host_mounts_reasoning_component_with_explicit_choice_kind() {
    use tekes_agent_components_ui::*;
    let mut panel = ComposerChoices::new(89, Default::default());
    let destination = ComposerDestination::run(tekes_agent_components_ui::RunScope::main(
        "consumer", "session",
    ));
    let request = panel
        .open(destination, ComposerChoiceKind::Reasoning)
        .unwrap();
    assert_eq!(request.kind, ComposerChoiceKind::Reasoning);
    let root = panel.id();
    let host = panel.into_host().unwrap();
    assert!(
        host.get(root)
            .unwrap()
            .component::<ComposerChoices>()
            .is_some()
    );
}

/// Agent-owned waiting content is independently mountable by another host.
pub fn actionable_card(namespace: u32) -> tekes_agent_components_ui::ActionableCard {
    tekes_agent_components_ui::ActionableCard::new(namespace, Default::default())
}
pub async fn respond_actionable(
    request: &tekes_agent_ui_bindings::ActionableResponseRequest,
) -> Result<(), String> {
    request.respond().await.map_err(|error| error.to_string())
}
#[test]
fn second_host_mounts_actionable_content_without_a_demo_or_private_control() {
    let card = actionable_card(92);
    let root = card.id();
    let host = card.into_host().unwrap();
    assert!(
        host.get(root)
            .unwrap()
            .component::<tekes_agent_components_ui::ActionableCard>()
            .is_some()
    );
}

/// Both hosts consume the same read-only message/footnote lanes, without using
/// RunSurface or implementing another projection. This is not a text renderer.
pub fn transcript_document(
    source: &tekes_agent_components_ui::TranscriptSnapshot,
) -> Result<tekes_agent_components_ui::TranscriptDocument<'_>, String> {
    tekes_agent_components_ui::TranscriptDocument::read(source).map_err(|error| error.to_string())
}
#[test]
fn independent_host_consumes_empty_and_stale_transcript_facts_without_a_view() {
    use tekes_agent_components_ui::TranscriptSnapshot;
    let source = TranscriptSnapshot {
        has_more_before: true,
        is_stale: true,
        diagnostics: vec!["partial coverage".into()],
        ..Default::default()
    };
    let document = transcript_document(&source).unwrap();
    assert!(document.sections.is_empty());
    assert!(document.has_more_before && document.is_stale);
    assert_eq!(document.diagnostics, &["partial coverage"]);
}

#[test]
fn independent_host_keeps_session_notice_outside_turns_and_footnotes() {
    use tekes_agent_components_ui::{
        TranscriptItem, TranscriptItemKind, TranscriptMessageKey, TranscriptSnapshot,
    };
    let source = TranscriptSnapshot {
        items: vec![TranscriptItem {
            id: "session-notice".into(),
            record_id: None,
            reconciliation_id: None,
            turn_id: String::new(),
            kind: TranscriptItemKind::Notice,
            text: "Worker unavailable".into(),
            reasoning: String::new(),
            attachments: vec![],
            content_blocks: None,
            is_visible: true,
            is_final: false,
            is_provisional: false,
            reasoning_ended: true,
            source_sequence: Some(4),
            source_position: None,
            time: Some(123.0),
            metadata: Default::default(),
        }],
        ..Default::default()
    };
    let document = transcript_document(&source).unwrap();
    assert!(document.sections.is_empty());
    assert_eq!(document.session_notices.len(), 1);
    assert_eq!(
        document.session_notices[0].key,
        TranscriptMessageKey::Item("session-notice")
    );
    assert_eq!(document.session_notices[0].item.text, "Worker unavailable");
    assert_eq!(transcript_event_counts(&document).unwrap(), (0, 0));
    let mut with_message = source.clone();
    let mut output = source.items[0].clone();
    output.id = "answer".into();
    output.turn_id = "turn".into();
    output.kind = TranscriptItemKind::Assistant;
    output.source_position = Some(
        tekes_agent_components_ui::TranscriptSourcePosition::Journal {
            sequence: 5,
            content_index: None,
        },
    );
    output.content_blocks = Some(vec![tekes_agent_components_ui::TranscriptContentBlock {
        index: 0,
        content: tekes_agent_components_ui::TranscriptBlockContent::Text {
            text: "正文".into(),
        },
    }]);
    with_message.items.push(output);
    assert_eq!(
        transcript_content_coverage(&transcript_document(&with_message).unwrap()),
        (1, 1, 0)
    );
    with_message.items[1].content_blocks.as_mut().unwrap().push(
        tekes_agent_components_ui::TranscriptContentBlock {
            index: 1,
            content: tekes_agent_components_ui::TranscriptBlockContent::ToolCall {
                tool_id: "not-yet-committed".into(),
            },
        },
    );
    assert_eq!(
        transcript_content_coverage(&transcript_document(&with_message).unwrap()),
        (2, 1, 1)
    );
    assert_eq!(
        transcript_event_counts(&transcript_document(&with_message).unwrap()).unwrap(),
        (1, 0)
    );
    with_message.items[1].source_position = None;
    assert_eq!(
        transcript_event_counts(&transcript_document(&with_message).unwrap()).unwrap(),
        (0, 1)
    );
}

/// One consumer can inspect proven event/container ordering while keeping
/// unplaced previews explicit; this does not implement a text or scroll engine.
pub fn transcript_event_counts(
    document: &tekes_agent_components_ui::TranscriptDocument<'_>,
) -> Result<(usize, usize), String> {
    let mut counts = (0, 0);
    for section in &document.sections {
        let order = section.event_order().map_err(|error| error.to_string())?;
        counts.0 += order.ordered.len();
        counts.1 += order.unplaced.len();
    }
    Ok(counts)
}

/// A host can report legacy/aggregate coverage and unresolved stream tool links
/// without parsing harness JSON or manufacturing a tool DTO.
pub fn transcript_content_coverage(
    document: &tekes_agent_components_ui::TranscriptDocument<'_>,
) -> (usize, usize, usize) {
    use tekes_agent_components_ui::TranscriptBlockContent;
    let mut counts = (0, 0, 0);
    for section in &document.sections {
        for item in section
            .inputs
            .iter()
            .map(|row| row.item)
            .chain(section.outputs.iter().map(|row| row.item))
        {
            if let Some(blocks) = &item.content_blocks {
                counts.0 += blocks.len();
                counts.2 += blocks
                    .iter()
                    .filter(|block| {
                        matches!(block.content, TranscriptBlockContent::ToolCall { .. })
                            && section.tool_for_block(block).is_none()
                    })
                    .count();
            } else {
                counts.1 += 1;
            }
        }
    }
    for row in &document.session_notices {
        if let Some(blocks) = &row.item.content_blocks {
            counts.0 += blocks.len();
            counts.2 += blocks
                .iter()
                .filter(|block| matches!(block.content, TranscriptBlockContent::ToolCall { .. }))
                .count();
        } else {
            counts.1 += 1;
        }
    }
    (counts.0, counts.1, counts.2)
}
