//! Second consumer: depends only on AgentKit's public UI packages.
/// Business display policy stays in AgentKit presentation. Consumers forward
/// source facts and preference, without recreating live-thinking lifecycle rules.
/// This contract probe is not an OutputMessage/Transcript renderer adapter.
pub fn thinking_policy<'a>(
    input: &tekes_agent_components_ui::TranscriptThinkingInput<'a>,
) -> Result<tekes_agent_components_ui::TranscriptThinkingPolicy<'a>, String> {
    tekes_agent_components_ui::TranscriptThinkingPolicy::read(input)
        .map_err(|error| error.to_string())
}
/// Installed business-plane facts; the host does not classify assistant prose.
pub fn installed_turn_planes(
    view: &tekes_agent_components_ui::TranscriptContent,
    turn_id: &str,
) -> Result<tekes_agent_components_ui::TranscriptTurnPlanes, &'static str> {
    view.turn_planes(turn_id)
}
/// Read-only display projection; consumers keep protocol source positions and
/// reading identities unchanged instead of implementing their own sort.
pub fn display_timeline<'a>(
    section: &tekes_agent_components_ui::TranscriptSection<'a>,
    scope: &tekes_agent_components_ui::RunScope,
    sources: &tekes_agent_components_ui::TranscriptThinkingSources,
) -> Result<tekes_agent_components_ui::TranscriptDisplayOrder<'a>, String> {
    section
        .display_order(scope, sources)
        .map_err(|error| error.to_string())
}
/// The independent host forwards preference before the normal Run plan. It
/// does not calculate message frames or issue endpoint reads.
pub fn prepare_thinking_run<'a>(
    view: &mut tekes_agent_components_ui::RunPort,
    snapshot: &'a tekes_agent_components_ui::RunSnapshot,
    input: tekes_agent_components_ui::TranscriptThinkingDisplayInput,
    paging: tekes_agent_components_ui::TranscriptPagingFacts,
) -> Result<tekes_agent_components_ui::RunPortUpdate<'a>, &'static str> {
    view.set_thinking_display(input)?;
    view.prepare_update(
        snapshot,
        Default::default(),
        tekes_agent_components_ui::ComposerControlGate::OPEN,
        paging,
        true,
    )
}
/// Public component consumer: forwards a captured display frame unchanged.
pub fn install_thinking_output(
    view: &mut tekes_agent_components_ui::OutputMessage,
    publication: u64,
    epoch: u64,
    output: &tekes_agent_components_ui::TranscriptOutput<'_>,
    section: Option<&tekes_agent_components_ui::TranscriptSection<'_>>,
    thinking: &tekes_agent_components_ui::TranscriptThinkingFrame,
) -> Result<(), &'static str> {
    view.update_with_thinking(publication, epoch, output, section, thinking)
}
/// A host forwards one immutable Run snapshot. Source facts and all thinking
/// decisions are produced by AgentKit, without another runtime read or cache.
pub fn install_run_thinking_output(
    view: &mut tekes_agent_components_ui::OutputMessage,
    snapshot: &tekes_agent_components_ui::RunSnapshot,
    item_id: &str,
    mode: tekes_agent_components_ui::TranscriptThinkingMode,
    display_revision: u64,
    elapsed_seconds: u64,
) -> Result<(), String> {
    let document = tekes_agent_components_ui::TranscriptDocument::read(&snapshot.transcript)
        .map_err(|error| error.to_string())?;
    let (section, output) = document
        .sections
        .iter()
        .find_map(|section| {
            section
                .outputs
                .iter()
                .find(|row| row.item.id == item_id)
                .map(|row| (section, row))
        })
        .ok_or_else(|| "thinking output unavailable".to_owned())?;
    let frame = snapshot
        .thinking_frame(section.id, mode, display_revision, elapsed_seconds)
        .map_err(|error| error.to_string())?;
    install_thinking_output(
        view,
        snapshot.revision,
        snapshot.connection_epoch,
        output,
        Some(section),
        &frame,
    )
    .map_err(str::to_owned)
}
/// Window clearance is supplied by the host; AgentKit owns the pane body inset.
pub fn set_panel_layout(
    panel: &mut tekes_agent_components_ui::SidePanel,
    content_top_inset: f32,
) -> Result<(), &'static str> {
    panel.set_layout(tekes_agent_components_ui::SidePanelLayout { content_top_inset })
}
pub fn sidebar_session_menu_intents(
    sidebar: &tekes_agent_components_ui::Sidebar,
    records: &[tekes_ui_controls::composition::ContentActionRecord],
) -> Vec<tekes_agent_components_ui::SidebarSessionMenuIntent> {
    sidebar.take_menu_intents(records)
}
/// A host uses the rich SDK queue once; legacy `drain_actions` would discard
/// the original menu value. The resulting changes go to its existing store.
pub fn sidebar_options_intents(
    options: &tekes_agent_components_ui::SidebarOptions,
    records: &[tekes_ui_controls::composition::ContentActionRecord],
) -> Vec<tekes_agent_components_ui::SidebarOptionsIntent> {
    options.take_intents(records)
}
/// The queued gesture retains its original source/frame/target; do not resolve
/// these again from the host's newer selection when dispatching it.
pub fn prepare_feedback_change(
    source: &std::sync::Arc<tekes_agent_ui_bindings::RunBinding>,
    frame: &tekes_agent_ui_bindings::FeedbackFrame,
    target: &tekes_agent_ui_bindings::FeedbackTargetCapture,
    change: tekes_agent_ui_bindings::FeedbackChange,
) -> Result<tekes_agent_ui_bindings::FeedbackPreparation, String> {
    source
        .prepare_feedback(frame, target, change)
        .map_err(|error| error.to_string())
}
/// Keep failure/stale distinct from a successful empty committed-feedback list.
pub fn committed_feedback<'a>(
    source: &std::sync::Arc<tekes_agent_ui_bindings::RunBinding>,
    frame: &'a tekes_agent_ui_bindings::FeedbackFrame,
) -> Result<&'a tekes_agent_ui_bindings::SessionMessageFeedbackList, &'static str> {
    if !frame.is_current(source) {
        return Err("feedback frame is stale or belongs to another binding");
    }
    Ok(frame.records())
}
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
    sync_scratch_tabs_with_projection(panel, source, parent, revision, true)
}
/// Canonical parent projection availability is a host input, independent of
/// transport freshness. Waiting must not manufacture a pane for an absent parent.
pub fn sync_scratch_tabs_with_projection(
    panel: &mut tekes_agent_components_ui::SidePanel,
    source: &tekes_agent_ui_bindings::SideChatBinding,
    parent: &tekes_agent_ui_bindings::RunBinding,
    revision: u64,
    projection_ready: bool,
) -> Result<(), String> {
    let mut input = source
        .input(
            revision,
            parent.scope(),
            vec![tekes_agent_components_ui::SidePanelTab::summary("Summary")],
            Some(tekes_agent_components_ui::SidePanelTabKind::Summary),
        )
        .map_err(|error| error.to_string())?;
    input.projection_ready = projection_ready;
    panel.update(&input).map_err(str::to_owned)
}
/// Consume the observed shared inventory through public APIs only. Selection is
/// a host input; this consumer doesn't create a thread model or a pin database.
pub fn sync_observed_sidebar(
    sidebar: &mut tekes_agent_components_ui::Sidebar,
    source: &tekes_agent_ui_bindings::SidebarBinding,
) -> Result<(), &'static str> {
    let preferences = source.preferences();
    sidebar.update_preferences(
        &preferences.clone().unwrap_or_default(),
        preferences.is_some(),
    )?;
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

#[test]
fn public_summary_host_sections_and_disclosure_need_no_runtime_or_presentation_dependency() {
    use tekes_agent_components_ui::{
        RunScope, SummaryInput, SummaryRow, SummarySection, SummaryView,
    };
    use tekes_ui_controls::composition::AccessibilityAction;
    let mut view = SummaryView::new(1980, Default::default());
    view.update(&SummaryInput {
        revision: 1,
        scope: RunScope::main("consumer", "summary"),
        facts: None,
        host_sections: vec![SummarySection {
            id: "repository".into(),
            rows: vec![SummaryRow {
                id: "branch".into(),
                title: "main".into(),
                value: Default::default(),
                action: None,
            }],
        }],
    })
    .unwrap();
    let header = view.section_header_id("repository").unwrap();
    let root = view.id();
    let mut host = view.into_host().unwrap();
    assert_eq!(
        host.get(root)
            .unwrap()
            .component::<SummaryView>()
            .unwrap()
            .section_order(),
        ["repository"]
    );
    host.accessibility_action(header, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let view = host.get(root).unwrap().component::<SummaryView>().unwrap();
    assert_eq!(view.section_expanded("repository"), Some(false));
    assert!(view.take_intents(&actions).is_empty());
    assert!(view.section_layouts().is_empty());
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

/// Capture on the host's original binding, then move this request into its
/// executor. Status subscriptions supplement the existing runtime observation.
pub fn begin_history_page(
    binding: &std::sync::Arc<tekes_agent_ui_bindings::RunBinding>,
) -> Result<tekes_agent_ui_bindings::HistoryPageRequest, String> {
    binding
        .begin_history_page()
        .map_err(|error| error.to_string())
}
pub async fn read_history_page(
    request: tekes_agent_ui_bindings::HistoryPageRequest,
) -> Result<bool, String> {
    request.load().await.map_err(|error| error.to_string())
}

/// Mount a history entrance with the original binding's facts. No RunSurface or
/// private demo types are required. Custom child sources pass false here.
pub fn history_controls(
    namespace: u32,
    binding: &tekes_agent_ui_bindings::RunBinding,
    transcript: &tekes_agent_components_ui::TranscriptSnapshot,
    supports_pagination: bool,
) -> tekes_agent_components_ui::TranscriptPaging {
    let mut control = tekes_agent_components_ui::TranscriptPaging::new(
        namespace,
        binding.scope().clone(),
        Default::default(),
    );
    control
        .update(
            binding
                .history_page_state()
                .controls(transcript, supports_pagination),
        )
        .expect("same binding");
    control
}

#[test]
fn independent_host_mounts_default_paging_controls_without_a_run_surface() {
    use tekes_agent_components_ui::{
        RunScope, TranscriptPageRead, TranscriptPaging, TranscriptPagingFacts,
    };
    let mut control = TranscriptPaging::new(
        134,
        RunScope::main("endpoint", "session"),
        Default::default(),
    );
    control
        .update(TranscriptPagingFacts {
            scope: RunScope::main("endpoint", "session"),
            supported: true,
            has_more_before: true,
            is_stale: false,
            source_ready: true,
            read: TranscriptPageRead::Idle,
        })
        .unwrap();
    let button = control.button_id();
    let host = control.into_host().unwrap();
    assert!(host.get(button).is_some());
}

/// Native durable identities are a separate namespace from journal/timeline
/// keys. Hosts must keep the original RunScope with the returned entry ID.
pub fn native_transcript_entry<'a>(
    key: tekes_agent_components_ui::TranscriptMessageKey<'a>,
) -> Option<&'a str> {
    match key {
        tekes_agent_components_ui::TranscriptMessageKey::NativeEntry(id) => Some(id),
        _ => None,
    }
}
#[test]
fn independent_consumer_retains_the_same_native_entry_when_the_loaded_prefix_changes() {
    use tekes_agent_components_ui::{
        TranscriptItem, TranscriptItemKind, TranscriptSnapshot, TranscriptSourcePosition,
    };
    fn row(id: &str, entry: &str, ordinal: usize) -> TranscriptItem {
        TranscriptItem {
            id: id.into(),
            record_id: Some(entry.into()),
            reconciliation_id: None,
            turn_id: "turn".into(),
            kind: TranscriptItemKind::Assistant,
            text: "same answer".into(),
            reasoning: String::new(),
            attachments: vec![],
            content_blocks: None,
            is_visible: true,
            is_final: true,
            is_provisional: false,
            reasoning_ended: true,
            source_sequence: None,
            source_position: Some(TranscriptSourcePosition::Native {
                entry_id: entry.into(),
                entry_ordinal: ordinal,
                content_index: None,
            }),
            time: Some(9.0),
            metadata: Default::default(),
        }
    }
    let before = TranscriptSnapshot {
        items: vec![row("timeline:0", "retained", 0)],
        ..Default::default()
    };
    let after = TranscriptSnapshot {
        items: vec![
            row("timeline:0", "older", 0),
            row("timeline:1", "retained", 1),
        ],
        ..Default::default()
    };
    let a = transcript_document(&before).unwrap();
    let b = transcript_document(&after).unwrap();
    // A clipped message without a turn must not fabricate a turn footer.
    assert!(b.sections[0].output_footnote().is_none());

    assert_eq!(a.sections[0].outputs[0].key, b.sections[0].outputs[1].key);
    assert_eq!(
        native_transcript_entry(b.sections[0].outputs[1].key),
        Some("retained")
    );
    assert_ne!(a.sections[0].outputs[0].key, b.sections[0].outputs[0].key);
    let machine = output_message_row(
        641,
        &tekes_agent_components_ui::RunScope::main("endpoint", "a"),
        1,
        1,
        &b.sections[0].outputs[1],
        Some(&b.sections[0]),
    )
    .unwrap();
    let machine_ids = machine.content_ids();
    let host = machine.into_host().unwrap();
    assert_eq!(
        host.text_surface(machine_ids[0].body).unwrap().input.text,
        "same answer"
    );

    let mut user = before.clone();
    user.items[0].kind = TranscriptItemKind::User;
    user.items[0].reconciliation_id = Some("original-rpc".into());
    let user_document = transcript_document(&user).unwrap();
    let input = &user_document.sections[0].inputs[0];
    assert_eq!(native_transcript_entry(input.key), Some("retained"));
    let message = input_message_row(
        640,
        &tekes_agent_components_ui::RunScope::main("endpoint", "a"),
        1,
        1,
        input,
    )
    .unwrap();
    let ids = message.ids();
    let host = message.into_host().unwrap();
    assert!(host.get(ids.bubble).is_some());
    assert!(host.get(ids.footnote).is_some());
    assert_eq!(
        host.text_surface(ids.body).unwrap().input.text,
        "same answer"
    );

    assert!(matches!(
        input.reconciliation_key(),
        Some(tekes_agent_components_ui::TranscriptMessageKey::Submission(
            "original-rpc"
        ))
    ));
}

/// A host mounts a message row directly from canonical presentation, without
/// RunSurface or a private copy of the text/IME/layout implementation.
pub fn input_message_row(
    namespace: u32,
    scope: &tekes_agent_components_ui::RunScope,
    publication: u64,
    epoch: u64,
    input: &tekes_agent_components_ui::TranscriptInput<'_>,
) -> Result<tekes_agent_components_ui::InputMessage, &'static str> {
    let mut row = tekes_agent_components_ui::InputMessage::new(namespace, scope.clone());
    row.update(scope, publication, epoch, input)?;
    Ok(row)
}

/// Machine body composition is shared too; hosts forward drained ContentActions
/// to OutputMessage::handle_actions for local disclosure and render again.
pub fn output_message_row(
    namespace: u32,
    scope: &tekes_agent_components_ui::RunScope,
    publication: u64,
    epoch: u64,
    output: &tekes_agent_components_ui::TranscriptOutput<'_>,
    section: Option<&tekes_agent_components_ui::TranscriptSection<'_>>,
) -> Result<tekes_agent_components_ui::OutputMessage, &'static str> {
    let mut row = tekes_agent_components_ui::OutputMessage::new(namespace, scope.clone());
    row.update(scope, publication, epoch, output, section)?;
    Ok(row)
}

/// Turn output aside remains independent of message bodies and host windows.
pub fn turn_footnote(
    namespace: u32,
    scope: &tekes_agent_components_ui::RunScope,
    publication: u64,
    epoch: u64,
    turn: &tekes_agent_components_ui::TranscriptTurn,
    available: bool,
) -> Result<tekes_agent_components_ui::TurnFootnote, &'static str> {
    let mut footer =
        tekes_agent_components_ui::TurnFootnote::new(namespace, scope.clone(), Default::default());
    footer.update(scope, publication, epoch, turn, available)?;
    Ok(footer)
}

#[test]
fn independent_host_mounts_a_turn_aside_without_a_message_body() {
    use tekes_agent_components_ui::{RunScope, TranscriptTurn};
    let turn = TranscriptTurn {
        id: "turn".into(),
        status: "completed".into(),
        input: String::new(),
        started_at: None,
        ended_at: None,
        usage: Default::default(),
        fork_boundary: None,
        footnote_actions: vec![],
        live_usage: None,
        reported_usage: None,
    };
    let footer = turn_footnote(645, &RunScope::main("endpoint", "a"), 1, 1, &turn, true).unwrap();
    let id = footer.id();
    let host = footer.into_host().unwrap();
    assert_eq!(
        host.get(id)
            .unwrap()
            .component::<tekes_agent_components_ui::TurnFootnote>()
            .unwrap()
            .facts()
            .unwrap()
            .status,
        "completed"
    );
}
pub async fn execute_turn_action(
    request: tekes_agent_ui_bindings::TurnActionRequest,
) -> Result<tekes_agent_ui_bindings::ThreadActionOutcome, String> {
    request.execute().await.map_err(|error| error.to_string())
}

/// A host supplies an exclusive namespace pool and canonical presentation facts.
/// The shared content tree leaves viewport/reading transactions to TekesUI.
pub fn transcript_content(
    root_namespace: u32,
    rows: std::ops::Range<u32>,
    scope: tekes_agent_components_ui::RunScope,
    snapshot: &tekes_agent_components_ui::TranscriptSnapshot,
    publication: u64,
    epoch: u64,
) -> Result<tekes_agent_components_ui::TranscriptContent, &'static str> {
    let mut view = tekes_agent_components_ui::TranscriptContent::new(
        root_namespace,
        rows,
        scope.clone(),
        Default::default(),
    )?;
    view.update(&scope, publication, epoch, snapshot, true)?;
    Ok(view)
}
#[test]
fn independent_host_mounts_canonical_content_without_private_demo_types() {
    use tekes_agent_components_ui::{
        InputMessage, OutputMessage, RunScope, TranscriptItem, TranscriptItemKind,
        TranscriptMessageKey, TranscriptSnapshot,
    };
    let input = TranscriptItem {
        id: "input".into(),
        record_id: None,
        reconciliation_id: None,
        turn_id: "turn".into(),
        kind: TranscriptItemKind::User,
        text: "用户输入".into(),
        reasoning: String::new(),
        attachments: vec![],
        content_blocks: None,
        is_visible: true,
        is_final: true,
        is_provisional: false,
        reasoning_ended: true,
        source_sequence: None,
        source_position: None,
        time: None,
        metadata: Default::default(),
    };
    let mut output = input.clone();
    output.id = "output".into();
    output.kind = TranscriptItemKind::Assistant;
    output.text = "机器回复".into();
    let snapshot = TranscriptSnapshot {
        items: vec![input, output],
        turns: vec![tekes_agent_components_ui::TranscriptTurn {
            id: "turn".into(),
            status: "completed".into(),
            input: "用户输入".into(),
            started_at: None,
            ended_at: None,
            usage: Default::default(),
            fork_boundary: None,
            footnote_actions: vec![],
            live_usage: None,
            reported_usage: None,
        }],
        ..Default::default()
    };
    let view = transcript_content(
        900,
        901..1000,
        RunScope::main("public", "a"),
        &snapshot,
        1,
        1,
    )
    .unwrap();
    let input = view
        .message_id(TranscriptMessageKey::Item("input"))
        .unwrap();
    let output = view
        .message_id(TranscriptMessageKey::Item("output"))
        .unwrap();
    assert!(view.capabilities().mixed_content);
    assert!(view.capabilities().structured_user_content);
    assert!(!view.capabilities().keyed_reading_transactions);
    assert!(!view.capabilities().rich_content);
    let root = view.id();
    let footer = view
        .row_id(&tekes_agent_components_ui::TranscriptContentRowKey::TurnFootnote("turn".into()))
        .unwrap();
    let mut host = view.into_host().unwrap();
    assert!(host.get(root).is_some());
    let input = host
        .get(input)
        .unwrap()
        .component::<InputMessage>()
        .unwrap();
    let output = host
        .get(output)
        .unwrap()
        .component::<OutputMessage>()
        .unwrap();
    assert_eq!(
        host.text_surface(input.ids().body).unwrap().input.text,
        "用户输入"
    );
    assert_eq!(
        host.text_surface(output.content_ids()[0].body)
            .unwrap()
            .input
            .text,
        "机器回复"
    );
    let copy = input.copy_button_id().unwrap();
    host.accessibility_action(
        copy,
        None,
        tekes_ui_controls::composition::AccessibilityAction::Press,
    );
    let actions = host.drain_actions();
    let copies = host
        .get(root)
        .unwrap()
        .component::<tekes_agent_components_ui::TranscriptContent>()
        .unwrap()
        .take_copy_intents(&actions);
    assert_eq!(copies.len(), 1);
    assert_eq!(copies[0].scope(), &RunScope::main("public", "a"));
    assert_eq!(copies[0].text(), "用户输入");
    assert!(copies[0].claim());
    assert!(!copies[0].clone().claim());
    let output_copy = host
        .get(footer)
        .unwrap()
        .component::<tekes_agent_components_ui::TurnFootnote>()
        .unwrap()
        .copy_button_id()
        .unwrap();
    host.accessibility_action(
        output_copy,
        None,
        tekes_ui_controls::composition::AccessibilityAction::Press,
    );
    let actions = host.drain_actions();
    let copies = host
        .get(root)
        .unwrap()
        .component::<tekes_agent_components_ui::TranscriptContent>()
        .unwrap()
        .take_copy_intents(&actions);
    assert_eq!(copies.len(), 1);
    assert_eq!(copies[0].text(), "机器回复");
    assert_eq!(
        copies[0].source(),
        &tekes_agent_components_ui::TranscriptCopySource::TurnOutput {
            turn_id: "turn".into(),
            item_ids: vec!["output".into()]
        }
    );
}

/// A separate product assembles the same public RunPort, never the demo host.
pub fn new_agent_run_port(
    namespace: u32,
    rows: std::ops::Range<u32>,
    scope: tekes_agent_components_ui::RunScope,
) -> Result<tekes_agent_components_ui::RunPort, &'static str> {
    tekes_agent_components_ui::RunPort::standard(namespace, rows, scope)
}
#[test]
fn second_product_mounts_a_run_with_its_own_namespace_pool_and_no_demo_or_run_surface() {
    use tekes_agent_components_ui::{Composer, RunPort, RunScope, TranscriptContent};
    let view = new_agent_run_port(1100, 1104..1200, RunScope::main("public", "original")).unwrap();
    let ids = view.ids();
    let transcript = view.transcript_id().unwrap();
    let host = view.into_host().unwrap();
    assert_eq!(
        host.get(ids.root)
            .unwrap()
            .component::<RunPort>()
            .unwrap()
            .scope(),
        &RunScope::main("public", "original")
    );
    assert!(
        host.get(ids.composer.root)
            .unwrap()
            .component::<Composer>()
            .is_some()
    );
    assert!(
        host.get(transcript)
            .unwrap()
            .component::<TranscriptContent>()
            .is_some()
    );
}

/// Capture one original binding frame and lower permissions on a later epoch.
/// This adapter adds no Run state and never chooses a newer visible destination.
pub fn sync_agent_run_port(
    view: &mut tekes_agent_components_ui::RunPort,
    source: &tekes_agent_ui_bindings::RunBinding,
    supports_pagination: bool,
) -> Result<(), String> {
    let snapshot = source.current_run().ok_or("run source unavailable")?;
    let mut page = source.history_page_state();
    if page.connection_epoch != snapshot.connection_epoch {
        page.source_ready = false;
        page.is_current = false;
    }
    let controls =
        source.access() == tekes_agent_ui_bindings::PaneAccess::Ready && page.source_ready;
    view.update(
        &snapshot,
        Default::default(),
        source.access().composer_gate(),
        page.controls(&snapshot.transcript, supports_pagination),
        controls,
    )
    .map_err(str::to_owned)
}

/// Read-only child history mounts independently, without a synthetic Run snapshot.
pub fn new_readonly_child(
    namespace: u32,
    rows: std::ops::Range<u32>,
    scope: tekes_agent_components_ui::RunScope,
) -> Result<tekes_agent_components_ui::ReadOnlyTranscript, &'static str> {
    tekes_agent_components_ui::ReadOnlyTranscript::standard(
        namespace,
        rows,
        scope,
        Default::default(),
    )
}
pub fn sync_readonly_child(
    view: &mut tekes_agent_components_ui::ReadOnlyTranscript,
    source: &tekes_agent_ui_bindings::ChildPaneBinding,
    snapshot: &tekes_agent_ui_bindings::PaneSnapshot,
    reading: bool,
    error: Option<&str>,
) -> Result<(), String> {
    view.update(
        source
            .readonly_input(snapshot, reading, error)
            .map_err(|error| error.to_string())?,
    )
    .map_err(str::to_owned)
}
#[test]
fn second_product_mounts_child_history_without_composer_or_run_surface() {
    let view = new_readonly_child(
        1300,
        1303..1400,
        tekes_agent_components_ui::RunScope {
            endpoint_id: "public".into(),
            thread_id: "parent".into(),
            session_id: "bounded".into(),
        },
    )
    .unwrap();
    let ids = view.ids();
    let content = view.content_id();
    let host = view.into_host().unwrap();
    assert!(
        host.get(ids.root)
            .unwrap()
            .component::<tekes_agent_components_ui::ReadOnlyTranscript>()
            .is_some()
    );
    assert!(
        host.get(content)
            .unwrap()
            .component::<tekes_agent_components_ui::TranscriptContent>()
            .is_some()
    );
    assert!(
        host.get(ids.refresh)
            .unwrap()
            .component::<tekes_ui_controls::composition::ButtonContent>()
            .unwrap()
            .button
            .state
            .disabled
    );
}

/// A second host consumes the same original-binding gesture bridge; it does not
/// assemble RPC payloads or implement feedback retry/optimistic state.
pub fn feedback_request(
    source: &std::sync::Arc<tekes_agent_ui_bindings::RunBinding>,
    frame: Option<&tekes_agent_ui_bindings::FeedbackFrame>,
    gesture: tekes_agent_components_ui::FeedbackGesture,
) -> tekes_agent_ui_bindings::FeedbackUiRequest {
    source.prepare_feedback_gesture(frame, gesture)
}
#[test]
fn independent_host_mounts_feedback_without_a_demo_or_private_binding_type() {
    let controls = tekes_agent_components_ui::TurnFeedback::new(92, Default::default());
    let root = controls.ids().root;
    let host = controls.into_host().unwrap();
    assert!(
        host.get(root)
            .unwrap()
            .component::<tekes_agent_components_ui::TurnFeedback>()
            .unwrap()
            .state()
            .is_none()
    );
}

/// Public RunPort composition opts into per-turn feedback without a test host.
pub fn run_with_feedback(
    scope: tekes_agent_components_ui::RunScope,
) -> tekes_agent_components_ui::RunPort {
    let mut run = tekes_agent_components_ui::RunPort::standard(93, 97..120, scope).unwrap();
    run.configure_feedback(Some(Default::default())).unwrap();
    run
}
#[test]
fn independent_host_can_opt_in_to_transcript_feedback() {
    let run = run_with_feedback(tekes_agent_components_ui::RunScope::main("fixture", "a"));
    assert_eq!(run.transcript().unwrap().feedback_controls().count(), 0);
    run.into_host().unwrap();
}

#[test]
fn independent_host_reconciles_explicit_stream_receipt_through_public_api() {
    use tekes_agent_components_ui::*;
    let scope = RunScope::main("public", "stream-session");
    let mut item = TranscriptItem {
        id: "preview".into(),
        record_id: None,
        reconciliation_id: None,
        turn_id: "turn".into(),
        kind: TranscriptItemKind::Assistant,
        text: "回答".into(),
        reasoning: String::new(),
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
    item.metadata["nativeStreamId"] = "stream".into();
    let mut source = TranscriptSnapshot {
        items: vec![item],
        ..Default::default()
    };
    let mut view = transcript_content(1900, 1901..1920, scope.clone(), &source, 1, 1).unwrap();
    let original = view
        .message_id(TranscriptMessageKey::Stream("stream"))
        .unwrap();
    source.items[0].is_provisional = false;
    source.items[0].source_position = Some(TranscriptSourcePosition::Native {
        entry_id: "entry".into(),
        entry_ordinal: 0,
        content_index: None,
    });
    view.update(&scope, 2, 1, &source, true).unwrap();
    assert_eq!(
        view.message_id(TranscriptMessageKey::NativeEntry("entry")),
        Some(original)
    );
    let root = view.id();
    let mut host = view.into_host().unwrap();
    let body = host
        .get(original)
        .unwrap()
        .component::<OutputMessage>()
        .unwrap()
        .content_ids()[0]
        .body;
    source.items[0].content_blocks = Some(vec![TranscriptContentBlock {
        index: 9,
        content: TranscriptBlockContent::Text {
            text: "回答".into(),
        },
    }]);
    host.get_mut(root)
        .unwrap()
        .component_mut::<TranscriptContent>()
        .unwrap()
        .update(&scope, 3, 1, &source, true)
        .unwrap();
    assert_eq!(
        host.get(original)
            .unwrap()
            .component::<OutputMessage>()
            .unwrap()
            .content_ids()[0]
            .body,
        body
    );
}

/// A host formats shared settlement facts without deriving failure from idle.
pub fn current_failure(
    snapshot: &tekes_agent_components_ui::TranscriptSnapshot,
) -> Option<tekes_agent_components_ui::TranscriptFailure<'_>> {
    snapshot.latest_failure()
}
#[test]
fn independent_host_reads_known_failure_without_fabricating_message_or_success() {
    use tekes_agent_components_ui::*;
    let mut snapshot = TranscriptSnapshot {
        turns: vec![TranscriptTurn {
            id: "failed".into(),
            status: "error".into(),
            input: String::new(),
            started_at: None,
            ended_at: None,
            usage: Default::default(),
            fork_boundary: None,
            footnote_actions: vec![],
            live_usage: None,
            reported_usage: None,
        }],
        ..Default::default()
    };
    let failure: TranscriptFailure<'_> = current_failure(&snapshot).unwrap();
    assert_eq!(failure.turn_id, "failed");
    assert_eq!(failure.message, None);
    snapshot.turns[0].status = "completed".into();
    assert!(current_failure(&snapshot).is_none());
}

#[test]
fn independent_host_prepares_reading_items_before_committing_shared_content() {
    use tekes_agent_components_ui::{RunScope, TranscriptContent, TranscriptSnapshot};
    let scope = RunScope::main("consumer", "reading");
    let mut content =
        TranscriptContent::new(920, 921..940, scope.clone(), Default::default()).unwrap();
    let snapshot = TranscriptSnapshot {
        is_stale: true,
        ..Default::default()
    };
    let plan = content
        .prepare_update(&scope, 1, 1, &snapshot, false)
        .unwrap();
    assert!(content.row_keys().is_empty());
    assert_eq!(plan.scope(), &scope);
    assert_eq!(plan.publication(), 1);
    assert_eq!(plan.connection_epoch(), 1);
    let items = plan.item_ids();
    assert_eq!(items.len(), 1);
    content.apply_prepared_update(&plan).unwrap();
    assert_eq!(content.row_id(&plan.rows()[0].0), Some(items[0]));
    let next = content
        .prepare_update(&scope, 2, 1, &snapshot, false)
        .unwrap();
    assert_eq!(next.item_ids(), items);
    content.apply_prepared_update(&next).unwrap();
    assert!(content.apply_prepared_update(&plan).is_err());
    assert_eq!(content.row_id(&next.rows()[0].0), Some(items[0]));
}

#[test]
fn independent_host_configures_pane_columns_without_a_private_run_surface() {
    use tekes_agent_components_ui::{RunColumnLayout, RunPort, RunScope};
    let mut first = RunPort::standard(1300, 1304..1350, RunScope::main("consumer", "A")).unwrap();
    let second = RunPort::standard(1400, 1404..1450, RunScope::main("consumer", "B")).unwrap();
    let column = RunColumnLayout::Insets {
        leading: 0.15,
        trailing: 0.08,
    };
    first.set_column_layout(column).unwrap();
    let composer = first.composer().ids();
    let transcript = first.transcript_id();
    let first_id = first.ids().root;
    let mut host = first.into_host().unwrap();
    let run = host
        .get_mut(first_id)
        .unwrap()
        .component_mut::<RunPort>()
        .unwrap();
    assert_eq!(run.column_layout(), column);
    for invalid in [
        RunColumnLayout::Reading {
            maximum_width: f32::NAN,
        },
        RunColumnLayout::Reading {
            maximum_width: 319.,
        },
    ] {
        assert!(run.set_column_layout(invalid).is_err());
        assert_eq!(run.column_layout(), column);
    }
    for (leading, trailing, normalized_leading, normalized_trailing) in [
        (f32::INFINITY, 0., 0., 0.),
        (-0.1, 0., 0., 0.),
        (0.46, 0., 0.45, 0.),
        (0.2, f32::NAN, 0.2, 0.),
    ] {
        run.set_column_layout(RunColumnLayout::Insets { leading, trailing })
            .unwrap();
        assert_eq!(
            run.column_layout(),
            RunColumnLayout::Insets {
                leading: normalized_leading,
                trailing: normalized_trailing
            }
        );
    }
    run.set_column_layout(RunColumnLayout::document_pane(0.1, None))
        .unwrap();
    assert_eq!(
        run.column_layout(),
        RunColumnLayout::Insets {
            leading: 0.1,
            trailing: 0.1
        }
    );
    run.set_column_layout(RunColumnLayout::FullWidth).unwrap();
    assert_eq!(run.composer().ids(), composer);
    assert_eq!(run.transcript_id(), transcript);
    assert_eq!(second.column_layout(), RunColumnLayout::default());
    assert_eq!(second.minimum_pane_width(), 420.);
}

/// Prepare from the captured paired frame. A late task must not resample a
/// binding's newest selection/epoch or treat read-only history as writable Run.
/// The host next checks controlled_draft on the original TekesUI editor, then
/// enters the reading transaction with the token captured by its original read.
pub fn prepare_original_run<'a>(
    run: &tekes_agent_components_ui::RunPort,
    frame: &'a tekes_agent_ui_bindings::PaneSnapshot,
    paging: tekes_agent_components_ui::TranscriptPagingFacts,
) -> Result<tekes_agent_components_ui::RunPortUpdate<'a>, &'static str> {
    match frame {
        tekes_agent_ui_bindings::PaneSnapshot::Run { snapshot, access } => run.prepare_update(
            snapshot,
            Default::default(),
            access.composer_gate(),
            paging,
            *access == tekes_agent_ui_bindings::PaneAccess::Ready,
        ),
        tekes_agent_ui_bindings::PaneSnapshot::ReadOnlyChild { .. } => {
            Err("bounded history has no transactional Composer")
        }
        tekes_agent_ui_bindings::PaneSnapshot::Missing { .. } => {
            Err("original Run pointer is missing")
        }
    }
}
/// A second host consumes the shared adapter and original SDK token; no demo
/// state, editor clone or reading engine is required.
pub fn publish_original_run(
    host: &mut tekes_ui_controls::composition::ContentHost,
    root: tekes_ui_controls::composition::ComponentId,
    version: &tekes_ui_controls::ReadingDocumentVersion,
    revision: u64,
    plan: &tekes_agent_components_ui::RunPortUpdate<'_>,
) -> Result<tekes_ui_controls::ReadingDocumentVersion, tekes_agent_components_ui::RunReadingError> {
    tekes_agent_components_ui::publish_run_reading_update(host, root, version, revision, plan)
}
#[test]
fn independent_host_does_not_prepare_a_missing_pointer_as_a_new_run() {
    use tekes_agent_components_ui::{RunPort, RunScope, TranscriptPageRead, TranscriptPagingFacts};
    let scope = RunScope::main("consumer", "missing");
    let run = RunPort::standard(1600, 1604..1650, scope.clone()).unwrap();
    let frame = tekes_agent_ui_bindings::PaneSnapshot::Missing {
        scope: scope.clone(),
    };
    let paging = TranscriptPagingFacts {
        scope,
        supported: false,
        has_more_before: false,
        is_stale: true,
        source_ready: false,
        read: TranscriptPageRead::Idle,
    };
    assert!(matches!(
        prepare_original_run(&run, &frame, paging),
        Err("original Run pointer is missing")
    ));
    assert!(run.composer().destination().is_none());
    assert!(run.transcript().unwrap().row_keys().is_empty());
}

/// A product's explicit focus action must not target a hidden archived Composer.
/// This is a query of shared presentation, not a second status policy.
pub fn composer_focus_target(
    run: &tekes_agent_components_ui::RunPort,
) -> Option<tekes_agent_components_ui::ComposerIds> {
    (run.is_pane_presented() && run.composer().is_presented()).then_some(run.ids().composer)
}
#[test]
fn second_product_uses_shared_composer_presentation_for_focus() {
    let run = new_agent_run_port(
        1700,
        1704..1750,
        tekes_agent_components_ui::RunScope::main("public", "focus"),
    )
    .unwrap();
    assert!(run.displayed_snapshot().is_none());
    assert_eq!(composer_focus_target(&run), Some(run.ids().composer));
}

/// Geometry and model must describe the same arranged publication. A host must
/// not interpret a retained collapsed frame or a pre-render update as current.
pub fn arranged_run_geometry(
    run: &tekes_agent_components_ui::RunPort,
) -> Option<tekes_agent_components_ui::RunPortGeometry> {
    let geometry = run.layout_geometry()?;
    (run.is_pane_presented()
        && geometry.publication == run.displayed_snapshot().map(|snapshot| snapshot.revision)
        && geometry.connection_epoch
            == run
                .displayed_snapshot()
                .map(|snapshot| snapshot.connection_epoch))
    .then_some(geometry)
}
#[test]
fn second_product_does_not_invent_geometry_before_a_run_is_arranged() {
    let run = new_agent_run_port(
        1800,
        1804..1850,
        tekes_agent_components_ui::RunScope::main("public", "unarranged"),
    )
    .unwrap();
    assert!(arranged_run_geometry(&run).is_none());
}

/// Read shared geometry instead of estimating input height and Send/Stop from
/// another copy of the draft or run lifecycle.
pub fn arranged_composer_layout(
    run: &tekes_agent_components_ui::RunPort,
) -> Option<&tekes_agent_components_ui::ComposerLayoutGeometry> {
    if !run.is_pane_presented() {
        return None;
    }
    let layout = run.composer().layout_geometry()?;
    (Some(layout.publication) == run.displayed_snapshot().map(|snapshot| snapshot.revision))
        .then_some(layout)
}
#[test]
fn second_product_queries_shared_primary_and_waits_for_actual_composer_geometry() {
    use tekes_agent_components_ui::{ComposerPrimaryActionKind, RunScope};
    let run =
        new_agent_run_port(1950, 1954..1990, RunScope::main("public", "composer-band")).unwrap();
    assert_eq!(
        run.composer().primary_action_kind(),
        ComposerPrimaryActionKind::Send
    );
    assert_eq!(run.composer().primary_action_id(), run.ids().composer.send);
    assert!(arranged_composer_layout(&run).is_none());
}

#[test]
fn second_product_captures_real_turn_rows_without_interpreting_timestamps_or_scroll_offsets() {
    use tekes_agent_components_ui::{
        RunScope, TranscriptContent, TranscriptSnapshot, TranscriptTurn,
    };
    let scope = RunScope::main("consumer", "rail");
    let mut view =
        TranscriptContent::new(1900, 1901..1950, scope.clone(), Default::default()).unwrap();
    let mut source = TranscriptSnapshot::default();
    source.turns.push(TranscriptTurn {
        id: "original-turn".into(),
        status: "running".into(),
        input: "中文问题".into(),
        started_at: None,
        ended_at: None,
        usage: Default::default(),
        fork_boundary: None,
        footnote_actions: vec![],
        live_usage: None,
        reported_usage: None,
    });
    view.update(&scope, 1, 8, &source, false).unwrap();
    let target = view.turn_targets().unwrap().remove(0);
    assert_eq!(target.turn_id(), "original-turn");
    assert_eq!(target.scope(), &scope);
    assert_eq!(target.is_visible(), None);
    assert_eq!(target.row_ids().len(), 1); // actual retained turn footer
    view.validate_turn_target(&target).unwrap();
    view.update(&scope, 2, 8, &source, false).unwrap();
    assert!(view.validate_turn_target(&target).is_err());
}

#[test]
fn sidebar_observation_exposes_restoration_and_failure_through_public_api() {
    use tekes_agent_components_ui::{Sidebar, SidebarLabels};
    use tekes_agent_ui_bindings::SidebarPublication;
    let mut source = SidebarPublication {
        observation: Default::default(),
        activity_at: Default::default(),
        revision: 1,
        loaded: false,
        rows: vec![],
        error: None,
    };
    let mut sidebar = Sidebar::new(96, SidebarLabels::default());
    sidebar.update(&source.input(None)).unwrap();
    assert!(!sidebar.displayed_input().unwrap().loaded);
    source.revision += 1;
    source.error = Some("observation ended".into());
    sidebar.update(&source.input(None)).unwrap();
    assert!(!sidebar.displayed_input().unwrap().loaded);
    assert_eq!(
        sidebar.displayed_input().unwrap().error.as_deref(),
        Some("observation ended")
    );
    source.revision += 1;
    source.loaded = true;
    source.error = None;
    sidebar.update(&source.input(None)).unwrap();
    assert!(sidebar.displayed_input().unwrap().loaded);
}

#[test]
fn second_product_consumes_sidebar_search_footer_and_captured_retry_without_private_api() {
    use tekes_agent_components_ui::*;
    use tekes_ui_controls::composition::AccessibilityAction;
    let key = SidebarSearchKey {
        owner: "second-product-search".into(),
        endpoint_id: "dsh".into(),
        connection_epoch: 2,
        generation: 1,
        query: "中文".into(),
    };
    let input = SidebarSearchInput {
        revision: 1,
        key: Some(key.clone()),
        phase: SidebarSearchPhase::Failed {
            message: "offline".into(),
            has_more: false,
            retryable: true,
        },
    };
    let mut sidebar = Sidebar::new(97, SidebarLabels::default());
    sidebar.update_search(&input).unwrap();
    let root = sidebar.id();
    let retry = sidebar.search_retry_id().unwrap();
    let mut host = sidebar.into_host().unwrap();
    host.accessibility_action(retry, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    assert_eq!(
        host.get(root)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .take_search_intents(&actions),
        vec![SidebarSearchIntent::Retry { key }]
    );
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update_search(&SidebarSearchInput {
            revision: 2,
            key: None,
            phase: SidebarSearchPhase::Idle,
        })
        .unwrap();
    assert!(
        host.get(root)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .take_search_intents(&actions)
            .is_empty()
    );
}

/// Construct search on the original observed endpoint, with query policy owned
/// by AgentKit's renderer-independent controller.
pub fn new_sidebar_search(
    source: &tekes_agent_ui_bindings::SidebarBinding,
) -> Result<tekes_agent_ui_bindings::SidebarSearchBinding, String> {
    tekes_agent_ui_bindings::SidebarSearchBinding::new(source.runtime().clone())
        .map_err(|error| error.to_string())
}
pub fn sync_sidebar_search(
    sidebar: &mut tekes_agent_components_ui::Sidebar,
    source: &tekes_agent_ui_bindings::SidebarSearchBinding,
) -> Result<(), &'static str> {
    sidebar.update_search_from(source.owner(), &source.snapshot().input)
}
pub fn mount_sidebar_search(
    sidebar: &mut tekes_agent_components_ui::Sidebar,
    source: &tekes_agent_ui_bindings::SidebarSearchBinding,
) -> Result<(), &'static str> {
    sidebar.replace_search_source(source.owner(), &source.snapshot().input)
}

#[test]
fn independent_summary_consumer_formats_usage_slots_without_recomputing_facts() {
    use tekes_agent_components_ui::{
        RunScope, SummaryFacts, SummaryInput, SummaryLabels, SummaryUsage, SummaryUsageRowText,
        SummaryView,
    };
    let scope = RunScope::main("consumer", "usage-columns");
    let mut view = SummaryView::new(
        1981,
        SummaryLabels {
            usage_row: |row| {
                let mut label = SummaryUsageRowText::read(row);
                label.title = format!("Host: {}", label.title);
                label
            },
            ..Default::default()
        },
    );
    view.update(&SummaryInput {
        revision: 1,
        scope: scope.clone(),
        facts: Some(SummaryFacts {
            scope,
            tasks: vec![],
            subagents: vec![],
            artifacts: vec![],
            usage: Some(SummaryUsage {
                input_tokens: 1000,
                cached_input_tokens: 0,
                cache_creation_input_tokens: 0,
                output_tokens: 0,
                cost_usd: None,
                is_cost_complete: false,
            }),
        }),
        host_sections: vec![],
    })
    .unwrap();
    let ids = view.usage_row_ids("input").unwrap();
    let host = view.into_host().unwrap();
    let nodes = host.semantics();
    assert_eq!(
        nodes
            .iter()
            .find(|node| node.id == ids.title)
            .unwrap()
            .semantics
            .name,
        "Host: 输入"
    );
    assert_eq!(
        nodes
            .iter()
            .find(|node| node.id == ids.value)
            .unwrap()
            .semantics
            .name,
        "1,000"
    );
    assert!(nodes.iter().any(|node| node.semantics.name == "未知"));
}

/// A host's non-focusing navigation gesture comes from the original installed
/// Run, not a newly selected session or a duplicated list of turn identities.
pub fn prepare_turn_navigation(
    run: &tekes_agent_components_ui::RunPort,
    target: &tekes_agent_components_ui::TranscriptTurnTarget,
) -> Result<tekes_agent_components_ui::RunTurnNavigation, &'static str> {
    run.prepare_turn_navigation(target)
}
pub fn claim_turn_navigation(
    run: &tekes_agent_components_ui::RunPort,
    navigation: &tekes_agent_components_ui::RunTurnNavigation,
) -> Result<tekes_agent_components_ui::RunTurnNavigationKind, &'static str> {
    run.claim_turn_navigation(navigation)
}

/// Install the shared rail only when the fixed library has original compact
/// controls. No host list, pointer engine or minimum-height substitute is built.
pub fn install_turn_rail(
    run: &mut tekes_agent_components_ui::RunPort,
    namespace: u32,
) -> Result<(), &'static str> {
    run.install_turn_rail(namespace)
}
#[test]
fn rail_dependency_gate_is_explicit_and_preserves_the_original_run_on_unsupported_pin() {
    use tekes_agent_components_ui::{RunScope, turn_rail_supported};
    let mut run = new_agent_run_port(
        1960,
        1964..1980,
        RunScope::main("consumer", "rail-capability"),
    )
    .unwrap();
    let original = run.composer().editor() as *const _;
    let result = install_turn_rail(&mut run, 1999);
    assert_eq!(result.is_ok(), turn_rail_supported());
    assert_eq!(run.composer().editor() as *const _, original);
    assert_eq!(run.turn_rail().is_some(), turn_rail_supported());
}

/// Main-host history integration, using only the public component contract.
pub fn claim_turn_navigation_with_host(
    run: &tekes_agent_components_ui::RunPort,
    navigation: &tekes_agent_components_ui::RunTurnNavigation,
    host: &mut impl tekes_agent_components_ui::RunTurnNavigationHost,
) -> Result<tekes_agent_components_ui::RunTurnNavigationKind, &'static str> {
    run.claim_turn_navigation_with_host(navigation, host)
}

/// Second product projects the complete reader publication through the shared
/// Kit API. Selection and the monotonically increasing view revision remain host inputs.
pub fn project_sidebar_search_results(
    source: &tekes_agent_ui_bindings::SidebarPublication,
    search: &tekes_agent_ui_bindings::SidebarSearchPublication,
    endpoint: &str,
    display: &tekes_agent_components_ui::SidebarInput,
) -> Result<tekes_agent_ui_bindings::SidebarSearchProjection, &'static str> {
    source.project_search_for_display(endpoint, search, display)
}

#[test]
fn second_product_consumes_result_projection_and_empty_state_without_private_types() {
    use tekes_agent_components_ui::*;
    use tekes_agent_ui_bindings::{SidebarPublication, SidebarSearchPublication};
    let source = SidebarPublication {
        observation: Default::default(),
        activity_at: Default::default(),
        revision: 1,
        rows: vec![],
        loaded: true,
        error: Some("inventory warning".into()),
    };
    let mut display = source.input(None);
    display.revision = 7;
    let mut search = SidebarSearchPublication {
        input: SidebarSearchInput {
            revision: 1,
            key: None,
            phase: SidebarSearchPhase::Idle,
        },
        query: String::new(),
        hits: vec![],
        unresolved: vec![],
        result: None,
    };
    let clear = project_sidebar_search_results(&source, &search, "source", &display).unwrap();
    assert_eq!(clear.input.revision, 7);
    assert_eq!(clear.input.error.as_deref(), Some("inventory warning"));
    assert!(clear.input.loaded);
    search.query = "正文".into();
    search.input.key = Some(SidebarSearchKey {
        owner: "reader".into(),
        endpoint_id: "source".into(),
        connection_epoch: 1,
        generation: 1,
        query: search.query.clone(),
    });
    search.input.revision = 2;
    search.input.phase = SidebarSearchPhase::Complete { has_more: false };
    let result = project_sidebar_search_results(&source, &search, "source", &display).unwrap();
    let mut sidebar = Sidebar::new(2202, SidebarLabels::default());
    sidebar.update_search(&search.input).unwrap();
    sidebar.update(&result.input).unwrap();
    let host = sidebar.into_host().unwrap();
    assert!(
        host.semantics()
            .iter()
            .any(|node| node.semantics.name == "没有匹配的会话")
    );
}

/// Host supplies a system comparator; Kit retains all ordering and pin rules.
pub fn apply_sidebar_preferences(
    sidebar: &mut tekes_agent_components_ui::Sidebar,
    value: &tekes_agent_components_ui::SidebarPreferencesSnapshot,
    writable: bool,
    compare: tekes_agent_components_ui::SidebarTitleComparator,
) -> Result<(), &'static str> {
    sidebar.set_title_comparator(compare);
    sidebar.update_preferences(value, writable)
}
#[test]
fn independent_sidebar_preferences_consumer_keeps_explicit_collation_and_read_only_admission() {
    use tekes_agent_components_ui::*;
    let mut sidebar = Sidebar::new(1999, SidebarLabels::default());
    let value = SidebarPreferencesSnapshot {
        revision: 2,
        preferences: SidebarPreferences {
            sort: SidebarSort::TitleAscending,
            pinned_expanded: false,
            ..Default::default()
        },
    };
    apply_sidebar_preferences(&mut sidebar, &value, false, |a, b| {
        a.to_lowercase().cmp(&b.to_lowercase())
    })
    .unwrap();
    assert_eq!(sidebar.preferences(), &value);
    assert!(
        sidebar
            .move_pin_change(
                &tekes_agent_components_ui::RunScope::main("p", "s").address(),
                1
            )
            .is_none()
    );
}

#[test]
fn second_product_consumes_current_row_help_without_a_tooltip_or_query_state_machine() {
    use tekes_agent_components_ui::*;
    let source = SidebarSearchInput {
        revision: 4,
        key: Some(SidebarSearchKey {
            owner: "consumer".into(),
            endpoint_id: "p".into(),
            connection_epoch: 1,
            generation: 2,
            query: "中文".into(),
        }),
        phase: SidebarSearchPhase::Complete { has_more: false },
    };
    let address = RunScope::main("p", "s").address();
    let input = SidebarInput {
        revision: 1,
        loaded: true,
        error: None,
        selected: Some(address.clone()),
        rows: vec![SidebarRow {
            primary_folder_id: None,
            folder_path: Some("原会话目录".into()),
            address: address.clone(),
            title: "原会话".into(),
            activity: ThreadActivity {
                status: "idle".into(),
                is_stale: false,
            },
            pin: None,
            actions: ThreadActionsPresentation {
                endpoint: "p".into(),
                session_id: "s".into(),
                label: "操作".into(),
                items: vec![],
            },
        }],
        folder_filter: None,
        search_help: Some(SidebarSearchHelp {
            results_ready: true,
            source: source.clone(),
            snippets: [(address.clone(), "当前查询的摘要".into())].into(),
        }),
    };
    let mut sidebar = Sidebar::new(2203, SidebarLabels::default());
    sidebar.update_search(&source).unwrap();
    sidebar.update(&input).unwrap();
    assert_eq!(sidebar.row_help(&address), Some("当前查询的摘要"));
    let mut cleared = source.clone();
    cleared.revision += 1;
    cleared.key = None;
    cleared.phase = SidebarSearchPhase::Idle;
    sidebar.update_search(&cleared).unwrap();
    assert_eq!(sidebar.search_snippet(&address), None);
    assert_eq!(sidebar.row_help(&address), Some("原会话目录"));
}

#[test]
fn independent_product_hides_unmatched_pins_without_erasing_preferences_or_pending_status() {
    use tekes_agent_components_ui::*;
    use tekes_agent_ui_bindings::{
        SidebarPublication, SidebarSearchMatch, SidebarSearchPublication,
    };
    let address = RunScope::main("consumer-endpoint", "pin").address();
    let source = SidebarPublication {
        observation: Default::default(),
        activity_at: Default::default(),
        revision: 1,
        loaded: true,
        error: None,
        rows: vec![SidebarRow {
            primary_folder_id: None,
            folder_path: Some("/original-pin-folder".into()),
            address: address.clone(),
            title: "原置顶会话".into(),
            activity: ThreadActivity {
                status: "idle".into(),
                is_stale: false,
            },
            pin: Some(ThreadPinPresentation { pinned: true }),
            actions: ThreadActionsPresentation {
                endpoint: address.endpoint_id.clone(),
                session_id: address.session_id.clone(),
                label: "操作".into(),
                items: vec![],
            },
        }],
    };
    let search = SidebarSearchPublication {
        input: SidebarSearchInput {
            revision: 1,
            key: Some(SidebarSearchKey {
                owner: "second-product".into(),
                endpoint_id: address.endpoint_id.clone(),
                connection_epoch: 1,
                generation: 1,
                query: "尚未出现在库存的命中".into(),
            }),
            phase: SidebarSearchPhase::Complete { has_more: false },
        },
        query: "尚未出现在库存的命中".into(),
        hits: vec![SidebarSearchMatch {
            address: RunScope::main(&address.endpoint_id, "missing").address(),
            hit: tekes_agent_ui_bindings::SessionSearchHit {
                session_id: "missing".into(),
                snippet: "current hit".into(),
                extra: Default::default(),
            },
        }],
        unresolved: vec![],
        result: None,
    };
    let display = source.input(Some(address.clone()));
    let projection =
        project_sidebar_search_results(&source, &search, &address.endpoint_id, &display).unwrap();
    assert!(projection.input.loaded);
    assert!(!projection.input.search_help.as_ref().unwrap().results_ready);
    assert!(projection.input.rows.is_empty());
    assert_eq!(projection.input.selected, Some(address.clone()));
    assert!(source.rows[0].pin.as_ref().unwrap().pinned);
    let mut sidebar = Sidebar::new(2204, SidebarLabels::default());
    sidebar.update_search(&search.input).unwrap();
    sidebar.update(&projection.input).unwrap();
    assert_eq!(sidebar.row_help(&address), None);
    assert!(sidebar.row_id(&address).is_none());
    let host = sidebar.into_host().unwrap();
    let names: Vec<_> = host
        .semantics()
        .into_iter()
        .map(|node| node.semantics.name)
        .collect();
    assert!(!names.iter().any(|name| name == "原置顶会话"));
    assert!(names.iter().any(|name| name == "搜索结果尚未就绪"));
    assert!(!names.iter().any(|name| name == "没有匹配的会话"));
}

#[test]
fn second_product_consumes_scoped_folder_grouping_filter_and_original_disclosure() {
    use tekes_agent_components_ui::*;
    use tekes_agent_ui_bindings::{SidebarPublication, SidebarSearchPublication};
    use tekes_ui_controls::composition::{AccessibilityAction, ButtonContent};
    let row = |id: &str, folder: &str, pinned: bool| SidebarRow {
        primary_folder_id: Some(folder.into()),
        folder_path: Some(format!("/original/{folder}")),
        address: RunScope::main("consumer", id).address(),
        title: id.into(),
        activity: ThreadActivity {
            status: "idle".into(),
            is_stale: false,
        },
        pin: Some(ThreadPinPresentation { pinned }),
        actions: ThreadActionsPresentation {
            endpoint: "consumer".into(),
            session_id: id.into(),
            label: "操作".into(),
            items: vec![],
        },
    };
    let source = SidebarPublication {
        observation: Default::default(),
        activity_at: Default::default(),
        revision: 1,
        loaded: true,
        error: None,
        rows: vec![
            row("a", "one", false),
            row("b", "two", false),
            row("pin", "three", true),
        ],
    };
    let mut display = source.input(None);
    let key = display.rows[0].folder_key();
    display.folder_filter = Some(key.clone());
    let clear = SidebarSearchPublication {
        input: SidebarSearchInput {
            revision: 1,
            key: None,
            phase: SidebarSearchPhase::Idle,
        },
        query: String::new(),
        hits: vec![],
        unresolved: vec![],
        result: None,
    };
    let projection = project_sidebar_search_results(&source, &clear, "consumer", &display).unwrap();
    assert_eq!(projection.input.folder_filter, Some(key.clone()));
    let preferences = SidebarPreferencesSnapshot {
        revision: 1,
        preferences: SidebarPreferences {
            group_by_folder: true,
            ..Default::default()
        },
    };
    let mut sidebar = Sidebar::new(2205, SidebarLabels::default());
    sidebar.update_preferences(&preferences, true).unwrap();
    sidebar.update(&projection.input).unwrap();
    let root = sidebar.id();
    let header = sidebar.folder_header_id(&key).unwrap();
    let leaf = sidebar.row_id(&source.rows[0].address).unwrap();
    assert!(sidebar.row_id(&source.rows[1].address).is_none());
    assert!(sidebar.row_id(&source.rows[2].address).is_none());
    assert!(source.rows[2].pin.as_ref().unwrap().pinned);
    let mut host = sidebar.into_host().unwrap();
    let widget = host
        .get(leaf)
        .unwrap()
        .component::<ButtonContent>()
        .unwrap() as *const _ as usize;
    host.accessibility_action(header, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let change = host
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .take_preference_intents(&actions)
        .pop()
        .unwrap();
    assert_eq!(change.expected, preferences);
    assert_eq!(change.value.collapsed_folders, [key]);
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update_preferences(
            &SidebarPreferencesSnapshot {
                revision: 2,
                preferences: change.value,
            },
            true,
        )
        .unwrap();
    assert!(!host.semantics().iter().any(|node| node.id == leaf));
    assert_eq!(
        host.get(leaf)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap() as *const _ as usize,
        widget
    );
}

#[test]
fn independent_product_reconciles_navigation_once_without_rewriting_durable_collapse() {
    use tekes_agent_components_ui::*;
    use tekes_ui_controls::composition::AccessibilityAction;
    let address = RunScope::main("consumer", "a").address();
    let row = SidebarRow {
        primary_folder_id: Some("original-id".into()),
        folder_path: Some("/original/folder".into()),
        address: address.clone(),
        title: "a".into(),
        activity: ThreadActivity {
            status: "idle".into(),
            is_stale: false,
        },
        pin: None,
        actions: ThreadActionsPresentation {
            endpoint: address.endpoint_id.clone(),
            session_id: address.session_id.clone(),
            label: "操作".into(),
            items: vec![],
        },
    };
    let key = row.folder_key();
    let value = SidebarPreferencesSnapshot {
        revision: 1,
        preferences: SidebarPreferences {
            group_by_folder: true,
            collapsed_folders: vec![key.clone()],
            ..Default::default()
        },
    };
    let input = SidebarInput {
        folder_filter: None,
        search_help: None,
        revision: 1,
        loaded: true,
        error: None,
        selected: Some(address.clone()),
        rows: vec![row],
    };
    let event = SidebarSelectionNavigation {
        revision: 1,
        selected: Some(address),
    };
    let mut sidebar = Sidebar::new(2206, SidebarLabels::default());
    sidebar.update_preferences(&value, true).unwrap();
    sidebar.update(&input).unwrap();
    assert!(sidebar.reconcile_navigation(&event).unwrap());
    assert_eq!(sidebar.preferences(), &value);
    let root = sidebar.id();
    let header = sidebar.folder_header_id(&key).unwrap();
    let leaf = sidebar.row_id(input.selected.as_ref().unwrap()).unwrap();
    let mut host = sidebar.into_host().unwrap();
    assert!(host.semantics().iter().any(|node| node.id == leaf));
    host.accessibility_action(header, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let change = host
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .take_preference_intents(&actions)
        .pop()
        .unwrap();
    assert_eq!(change.value, value.preferences);
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update_preferences(&value, true)
        .unwrap();
    assert!(
        !host
            .get_mut(root)
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .reconcile_navigation(&event)
            .unwrap()
    );
    assert!(!host.semantics().iter().any(|node| node.id == leaf));
}
/// Preserve the captured row/source rather than resolving the newest selection
/// after an async layout. The host dispatches through TekesUI's original reveal.
pub fn sidebar_visibility_target(
    view: &tekes_agent_components_ui::Sidebar,
) -> Option<tekes_agent_components_ui::SidebarVisibilityTarget> {
    view.visibility_target()
}
pub fn claim_sidebar_visibility(
    view: &tekes_agent_components_ui::Sidebar,
    target: &tekes_agent_components_ui::SidebarVisibilityTarget,
) -> Result<(), &'static str> {
    view.claim_visibility(target)
}
#[test]
fn independent_sidebar_pages_an_empty_inventory_without_synthetic_rows_or_navigation() {
    use tekes_agent_components_ui::*;
    let mut sidebar = Sidebar::new(890, Default::default());
    sidebar
        .update(&SidebarInput {
            revision: 1,
            loaded: true,
            error: None,
            rows: vec![],
            selected: None,
            folder_filter: None,
            search_help: None,
        })
        .unwrap();
    assert_eq!(sidebar.paging().limit, SIDEBAR_PAGE_SIZE);
    assert!(!sidebar.paging().has_more);
    assert!(sidebar.page_button_id().is_none());
    assert!(sidebar_visibility_target(&sidebar).is_none());
}
/// Keep the selected-row read bound to its original source and selection scope;
/// the component retains paging, filtering, pins and navigation ownership.
pub fn supplement_selected_sidebar(
    source: &tekes_agent_ui_bindings::SidebarBinding,
    read: &tekes_agent_ui_bindings::SidebarSelectedRead,
    display: &mut tekes_agent_components_ui::SidebarInput,
    selection_generation: u64,
) -> bool {
    read.project_into(source, display, selection_generation)
}

/// Explicitly choosing the current filter/sort restarts only local paging.
pub fn restart_sidebar_page(
    view: &mut tekes_agent_components_ui::Sidebar,
) -> Result<bool, &'static str> {
    view.reset_paging()
}

/// Host menus consume a full versioned catalog, never the displayed page's headers.
pub fn sidebar_folder_choices(
    source: &tekes_agent_ui_bindings::SidebarPublication,
) -> Result<tekes_agent_ui_bindings::SidebarFolderCatalog, &'static str> {
    source.folder_catalog()
}

/// Explicit navigation only; retain the captured target and original SDK options.
pub fn reveal_sidebar_selection(
    host: &mut tekes_ui_controls::composition::ContentHost,
    target: &tekes_agent_components_ui::SidebarVisibilityTarget,
    version: &tekes_ui_controls::ReadingDocumentVersion,
    options: tekes_ui_controls::ScrollRevealOptions,
    text: &mut dyn tekes_ui_controls::ButtonTextRenderer,
) -> Result<
    tekes_ui_controls::composition::ContentRevealResult,
    tekes_agent_components_ui::SidebarRevealError,
> {
    tekes_agent_components_ui::reveal_sidebar_selection(host, target, version, options, text)
}

/// Second consumer exercises only the published shared navigation boundary.
pub fn navigate_original_run(
    host: &mut tekes_ui_controls::composition::ContentHost,
    root: tekes_ui_controls::composition::ComponentId,
    intent: &tekes_agent_components_ui::RunTurnNavigation,
    version: &tekes_ui_controls::ReadingDocumentVersion,
    role: tekes_agent_components_ui::RunNavigationHost<'_>,
    text: &mut dyn tekes_ui_controls::ButtonTextRenderer,
) -> Result<
    tekes_agent_components_ui::RunNavigationResult,
    tekes_agent_components_ui::RunNavigationError,
> {
    tekes_agent_components_ui::navigate_run_turn(host, root, intent, version, role, false, text)
}

pub fn jump_original_run(
    host: &mut tekes_ui_controls::composition::ContentHost,
    root: tekes_ui_controls::composition::ComponentId,
    intent: &tekes_agent_components_ui::RunJumpLatest,
    version: &tekes_ui_controls::ReadingDocumentVersion,
) -> Result<bool, tekes_agent_components_ui::RunNavigationError> {
    tekes_agent_components_ui::jump_run_latest(host, root, intent, version)
}

/// Independent host consumes the same bounded/read-only reading transaction.
pub fn publish_original_readonly(
    host: &mut tekes_ui_controls::composition::ContentHost,
    root: tekes_ui_controls::composition::ComponentId,
    version: &tekes_ui_controls::ReadingDocumentVersion,
    revision: u64,
    plan: &tekes_agent_components_ui::ReadOnlyTranscriptUpdate<'_>,
) -> Result<
    tekes_ui_controls::ReadingDocumentVersion,
    tekes_agent_components_ui::ReadOnlyReadingError,
> {
    tekes_agent_components_ui::publish_readonly_reading_update(host, root, version, revision, plan)
}

#[test]
fn second_summary_consumer_selects_passive_text_without_an_endpoint_or_copy_state_machine() {
    use tekes_agent_components_ui::*;
    use tekes_ui_controls::composition::*;
    let mut view = SummaryView::new(1982, Default::default());
    view.update(&SummaryInput {
        revision: 1,
        scope: RunScope::main("consumer", "selectable-summary"),
        facts: None,
        host_sections: vec![SummarySection {
            id: "application".into(),
            rows: vec![SummaryRow {
                id: "source".into(),
                title: "原始来源".into(),
                value: Default::default(),
                action: None,
            }],
        }],
    })
    .unwrap();
    let root = view.id();
    let text = view.passive_text_id("application", "source").unwrap();
    let mut host = view.into_host().unwrap();
    host.accessibility_action(text, None, AccessibilityAction::SetSelection(0..2));
    let field = host
        .get(text)
        .unwrap()
        .component::<TextFieldContent>()
        .unwrap();
    assert!(field.surface.input.read_only);
    assert_eq!(field.surface.input.selected_text(), "原始");
    let actions = host.drain_actions();
    assert!(
        host.get(root)
            .unwrap()
            .component::<SummaryView>()
            .unwrap()
            .take_intents(&actions)
            .is_empty()
    );
}

/// Preview data is shared Agent content; placement/hit/dismiss belongs to the SDK.
/// Call after a successful render and keep the captured target, not a new ordinal.
pub fn turn_preview<'a>(
    run: &tekes_agent_components_ui::RunPort,
    target: &'a tekes_agent_components_ui::TranscriptTurnTarget,
) -> Result<&'a tekes_agent_components_ui::TranscriptTurnPreview, &'static str> {
    run.turn_preview(target)
}

/// The host consumes an event-time refresh on its original source, then uses
/// its existing service/controller to refresh. No request is automatically sent.
pub fn composer_notice_refreshes(
    notice: &mut tekes_agent_components_ui::ComposerNotice,
) -> Vec<tekes_agent_components_ui::ComposerNoticeRefresh> {
    notice
        .take_refreshes()
        .into_iter()
        .filter(|request| notice.claim_refresh(request).is_ok())
        .collect()
}
#[test]
fn independent_composer_notice_uses_captured_public_callback_without_a_runtime() {
    use tekes_agent_components_ui::*;
    use tekes_ui_controls::composition::AccessibilityAction;
    let mut notice = ComposerNotice::new(1983, Default::default());
    notice
        .update(&ComposerNoticeInput {
            connection_epoch: 1,
            revision: 1,
            destination: ComposerDestination::run(RunScope::main("consumer", "notice")),
            enabled: true,
            notice: Some(ComposerNoticeState {
                text: "刷新连接目录".into(),
                can_refresh: true,
                refreshing: false,
            }),
        })
        .unwrap();
    let root = notice.id();
    let refresh = notice.refresh_id();
    let mut host = notice.into_host().unwrap();
    host.accessibility_action(refresh, None, AccessibilityAction::Press);
    let source = host
        .get_mut(root)
        .unwrap()
        .component_mut::<ComposerNotice>()
        .unwrap();
    let requests = composer_notice_refreshes(source);
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].source().destination,
        ComposerDestination::run(RunScope::main("consumer", "notice"))
    );
    assert!(composer_notice_refreshes(source).is_empty());
}

/// Route an original once-drained SDK action batch through the public RunPort;
/// local disclosures use its existing policy, without host reconstruction.
pub fn drain_local_run_actions(
    host: &mut tekes_ui_controls::composition::ContentHost,
    root: tekes_ui_controls::composition::ComponentId,
) -> Vec<tekes_agent_components_ui::RunPortIntent> {
    let actions = host.drain_actions();
    host.get_mut(root)
        .unwrap()
        .component_mut::<tekes_agent_components_ui::RunPort>()
        .unwrap()
        .take_intents(&actions)
}

/// The same shared Summary component accepts an explicit workspace owner, with
/// no runtime or fabricated session required by this consumer.
pub fn publish_summary_context(
    summary: &mut tekes_agent_components_ui::SummaryView,
    input: &tekes_agent_components_ui::SummaryContextInput,
) -> Result<(), &'static str> {
    summary.update_context(input)
}
#[test]
fn independent_workspace_summary_has_no_session_authority() {
    use tekes_agent_components_ui::*;
    let mut view = SummaryView::new(1984, Default::default());
    let input = SummaryContextInput {
        custom_rows: vec![],
        revision: 1,
        owner: SummaryOwner::Workspace {
            id: "public-consumer-workspace".into(),
        },
        facts: None,
        host_sections: vec![],
        ready: true,
    };
    publish_summary_context(&mut view, &input).unwrap();
    assert!(
        view.displayed_context_input()
            .unwrap()
            .owner
            .run_scope()
            .is_none()
    );
    assert!(view.displayed_input().is_none());
    assert!(view.take_intents(&[]).is_empty());
}

/// Host content enters the original SDK tree, including its ID/lifecycle checks.
pub fn install_summary_host_content(
    host: &mut tekes_ui_controls::composition::ContentHost,
    root: tekes_ui_controls::composition::ComponentId,
    source: &tekes_agent_components_ui::SummaryCustomRowToken,
    content: tekes_ui_controls::composition::Content,
) -> Result<(), tekes_agent_components_ui::SummaryCustomRowError> {
    tekes_agent_components_ui::install_summary_custom_row(host, root, source, content)
}
#[test]
fn independent_custom_summary_captures_the_source_before_event_delivery() {
    fn assert_async_source<T: Send + Sync>() {}
    assert_async_source::<tekes_agent_components_ui::SummaryCustomRowToken>();
    use tekes_agent_components_ui::*;
    use tekes_ui_controls::composition::*;
    let mut summary = SummaryView::new(1985, Default::default());
    let mut input = SummaryContextInput {
        revision: 1,
        owner: SummaryOwner::Workspace {
            id: "consumer-host".into(),
        },
        facts: None,
        ready: true,
        host_sections: vec![SummarySection {
            id: "product".into(),
            rows: vec![SummaryRow {
                id: "control".into(),
                title: "Product action".into(),
                value: Default::default(),
                action: None,
            }],
        }],
        custom_rows: vec![SummaryCustomRowSpec {
            section: "product".into(),
            row: "control".into(),
            content_key: "control-v1".into(),
        }],
    };
    summary.update_context(&input).unwrap();
    let root = summary.id();
    let source = summary.custom_row_token("product", "control").unwrap();
    let mut host = summary.into_host().unwrap();
    let button = ComponentId(1986u64 << 32);
    install_summary_host_content(
        &mut host,
        root,
        &source,
        Content::new(button, ButtonContent::new("Product action")),
    )
    .unwrap();
    // Capture the current immutable source before delivering the original event.
    let captured = source.clone();
    host.accessibility_action(button, None, AccessibilityAction::Press);
    let records = host.drain_action_records();
    assert_eq!(records.len(), 1);
    let view = host
        .get_mut(root)
        .unwrap()
        .component_mut::<SummaryView>()
        .unwrap();
    view.validate_custom_row_action(&captured).unwrap();
    input.revision += 1;
    input.owner = SummaryOwner::Workspace {
        id: "another-host-context".into(),
    };
    publish_summary_context_update(&mut host, root, &input).unwrap();
    assert!(
        host.get(root)
            .unwrap()
            .component::<SummaryView>()
            .unwrap()
            .validate_custom_row_action(&captured)
            .is_err()
    );
}

#[test]
fn independent_summary_consumer_keeps_child_pointer_and_control_across_status_updates() {
    use tekes_agent_components_ui::*;
    use tekes_ui_controls::composition::*;
    let scope = RunScope::main("consumer", "parent");
    let mut input = SummaryInput {
        revision: 1,
        scope: scope.clone(),
        host_sections: vec![],
        facts: Some(SummaryFacts {
            scope: scope.clone(),
            tasks: vec![],
            artifacts: vec![],
            usage: None,
            subagents: vec![SummarySubagent {
                id: "call".into(),
                title: "Child".into(),
                session_id: Some("child".into()),
                status: String::new(),
            }],
        }),
    };
    let mut summary = SummaryView::new(49005, Default::default());
    summary.update(&input).unwrap();
    let root = summary.id();
    let row = summary.action_id("subagents", "call").unwrap();
    let mut host = summary.into_host().unwrap();
    assert_eq!(
        host.get(row)
            .unwrap()
            .component::<SummaryActionRow>()
            .unwrap()
            .button()
            .label,
        "Child"
    );
    input.revision += 1;
    input.facts.as_mut().unwrap().subagents[0].status = "failed".into();
    let summary = host
        .get_mut(root)
        .unwrap()
        .component_mut::<SummaryView>()
        .unwrap();
    summary.update(&input).unwrap();
    assert_eq!(summary.action_id("subagents", "call"), Some(row));
    assert_eq!(
        input.facts.as_ref().unwrap().subagents[0].indicator(),
        Some(SummarySubagentIndicator::Failed)
    );
    host.accessibility_action(row, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let intents = host
        .get(root)
        .unwrap()
        .component::<SummaryView>()
        .unwrap()
        .take_intents(&actions);
    assert!(
        matches!(&intents[..],[SummaryIntent{scope:parent,action:SummaryAction::OpenSubagent{parent:action_parent,child_id}}] if parent==&scope && action_parent==&scope && child_id=="child")
    );
}

/// Forward a local Activity selection; the Kit owns the selection and its fences.
pub fn set_run_activity_expansion(
    view: &mut tekes_agent_components_ui::RunPort,
    turn: &str,
    expanded: bool,
) -> Result<(), &'static str> {
    view.set_activity_expanded(turn, expanded)
}

/// Read-only panes use their own local selection and captured history publication.
pub fn set_readonly_activity_expansion(
    view: &mut tekes_agent_components_ui::ReadOnlyTranscript,
    turn: &str,
    expanded: bool,
) -> Result<(), &'static str> {
    view.set_activity_expanded(turn, expanded)
}

/// Forward one canonical input-bounded segment choice; Kit owns defaults/fences.
pub fn set_run_activity_segment(
    view: &mut tekes_agent_components_ui::RunPort,
    key: &tekes_agent_components_ui::TranscriptActivitySegmentKey,
    expanded: bool,
) -> Result<(), &'static str> {
    view.set_activity_segment_expanded(key, expanded)
}
pub fn set_readonly_activity_segment(
    view: &mut tekes_agent_components_ui::ReadOnlyTranscript,
    key: &tekes_agent_components_ui::TranscriptActivitySegmentKey,
    expanded: bool,
) -> Result<(), &'static str> {
    view.set_activity_segment_expanded(key, expanded)
}

/// Consume the projector's explicit role; never infer it from system prose.
pub fn system_prompt_source(
    item: &tekes_agent_components_ui::TranscriptItem,
) -> Option<tekes_agent_components_ui::TranscriptSystemPrompt> {
    item.system_prompt()
}
pub fn input_role_source(
    item: &tekes_agent_components_ui::TranscriptItem,
) -> Option<tekes_agent_components_ui::TranscriptInputRole> {
    item.input_role()
}

pub fn narration_policy(
    timeline: tekes_agent_components_ui::TranscriptNarrationTimeline,
    items: &[tekes_agent_components_ui::TranscriptNarrationItem<'_>],
    fallbacks: &[tekes_agent_components_ui::TranscriptNarrationFallback<'_>],
) -> Result<tekes_agent_components_ui::TranscriptNarrationPolicy, String> {
    tekes_agent_components_ui::TranscriptNarrationPolicy::read(timeline, items, fallbacks)
        .map_err(|error| error.to_string())
}
/// Original source ordering only; not a session pointer or a new Agent timeline.
pub fn embedded_tool_input_boundary(
    section: &tekes_agent_components_ui::TranscriptSection<'_>,
    first: &str,
    second: &str,
) -> Result<Option<bool>, String> {
    section
        .has_input_between_tools(first, second)
        .map_err(|error| error.to_string())
}

/// Mounted original tool partitions; no cloned tool views or host timeline.
pub fn message_tool_fragments(
    content: &tekes_agent_components_ui::TranscriptContent,
    owner: &tekes_agent_components_ui::TranscriptMessageIdentity,
) -> Vec<(
    tekes_agent_components_ui::OutputMessageToolFragmentKey,
    tekes_ui_controls::composition::ComponentId,
)> {
    content.message_tool_fragment_ids(owner)
}

/// Original control identity remains queryable while history is folded. This
/// does not claim that the control is mounted or supply an SDK reading target.
pub fn retained_tool_control_id(
    content: &tekes_agent_components_ui::TranscriptContent,
    tool: &str,
) -> Option<tekes_ui_controls::composition::ComponentId> {
    content
        .retained_tool_activity(tool)
        .map(|control| control.id())
}

/// A second host verifies the original primary action through public SDK events.
/// No application-level focus restoration or duplicate IME state is involved.
pub fn verify_primary_keeps_editor(
    frame: &tekes_agent_components_ui::RunSnapshot,
) -> Result<(), &'static str> {
    use tekes_agent_components_ui::{
        Composer, ComposerIds, ComposerIntent, ComposerPrimaryActionKind,
    };
    use tekes_ui_controls::composition::{AccessibilityAction, ContentEvent};
    let ids = ComposerIds::new(65400);
    let mut composer = Composer::new(ids);
    composer.update(frame, Default::default())?;
    if composer.primary_action_kind() != ComposerPrimaryActionKind::Stop {
        return Err("consumer needs a cancellable empty-draft frame");
    }
    let mut host = composer.into_host().map_err(|_| "consumer mount failed")?;
    host.focus(Some(ids.editor), true);
    host.event(&ContentEvent::Preedit {
        text: "第二宿主中文组字".into(),
        selection: 8..8,
        clauses: vec![],
        replacement: None,
    });
    host.drain_actions();
    let baseline = host
        .text_surface(ids.editor)
        .unwrap()
        .input
        .external_baseline();
    host.accessibility_action(ids.cancel, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let intents = host
        .get_mut(ids.root)
        .unwrap()
        .component_mut::<Composer>()
        .unwrap()
        .take_intents(&actions);
    if !matches!(intents.as_slice(), [ComposerIntent::Cancel { .. }])
        || host.focused() != Some(ids.editor)
        || host
            .text_surface(ids.editor)
            .unwrap()
            .input
            .external_baseline()
            != baseline
    {
        return Err("primary action replaced the original editor owner");
    }
    Ok(())
}

/// A host can supply an already mounted destination without cloning its state.
/// The shared binding retains task authority and validates exact scope/runtime.
pub async fn open_task_from_original_run(
    source: &tekes_agent_ui_bindings::TaskPaneBinding,
    target: &tekes_agent_ui_bindings::RunBinding,
) -> Result<Option<std::sync::Arc<tekes_agent_ui_bindings::RunBinding>>, String> {
    source
        .open_resolved_from(target)
        .await
        .map_err(|error| error.to_string())
}

/// Reopen a scratch destination using its owning public pointer authority.
/// Hosts need not preserve a private per-pane draft cache.
pub async fn open_scratch_run(
    source: &tekes_agent_ui_bindings::SideChatBinding,
    parent: &tekes_agent_ui_bindings::RunBinding,
    session_id: &str,
) -> Result<tekes_agent_ui_bindings::BoundSideChatRun, String> {
    let pointer = source
        .pointer(parent.scope(), session_id)
        .map_err(|error| error.to_string())?;
    let run = source
        .open(&pointer)
        .await
        .map_err(|error| error.to_string())?;
    if !source.owns_run(&run) {
        return Err("SideChat returned a retired or foreign run".into());
    }
    Ok(run)
}
