//! Published SDK RunPort integration; CoreText layout, not native/GPU acceptance.
#![cfg(target_os = "macos")]
use std::rc::Rc;
use tekes_agent_components_ui::*;
use tekes_agent_presentation::RunSnapshot;
use tekes_ui_controls::{
    ButtonTextAlign, ButtonTextRenderer, TextSurfaceHost, TextSurfaceTextRenderer, composition::*,
};
use tekes_ui_controls::{ExternalValueConflict, ExternalValueResult, ReadingDocumentVersion};
use tekes_ui_scene::{Point, Rect, Scene, Size};
use tekes_ui_text::{
    Paragraph,
    coretext::{CoreTextFonts, CoreTextShaper},
    model::ShapeOptions,
    style::TextStyle,
    traits::Shaper,
};
use tekes_ui_tokens::{Rgba, Theme, proposal};
struct Text(CoreTextShaper, f32);
impl TextSurfaceTextRenderer for Text {
    fn push_visible(&mut self, _: &mut Scene, _: &Paragraph, _: Point, _: Rgba, _: Rect) {}
}
impl TextSurfaceHost for Text {
    fn shape_label(
        &mut self,
        text: &str,
        role: proposal::TextRole,
        width: f32,
        options: tekes_ui_text::LabelOptions,
    ) -> Option<Rc<tekes_ui_text::LabelLayout>> {
        Some(tekes_agent_ui_demo::label_host::shape_label(
            &mut self.0,
            text,
            role,
            width,
            self.1,
            options,
        ))
    }

    fn type_scale(&self) -> f32 {
        self.1
    }
    fn shape_surface(
        &mut self,
        s: &str,
        role: proposal::TextRole,
        width: Option<f32>,
    ) -> Rc<Paragraph> {
        Rc::new(self.0.shape(
            s,
            &TextStyle::system(role.style().size * self.1),
            &ShapeOptions {
                width: width.map(f64::from),
                line_spacing: 0.,
            },
        ))
    }
}
impl ButtonTextRenderer for Text {
    fn measure(&mut self, s: &str, r: proposal::TextRole, w: f32) -> Size {
        let p = self.shape_surface(s, r, Some(w));
        Size::new(p.width as f32, p.height as f32)
    }
    fn draw(
        &mut self,
        _: &mut Scene,
        _: &str,
        _: proposal::TextRole,
        _: Rect,
        _: ButtonTextAlign,
        _: Rgba,
    ) {
    }
    fn text_surface_host(&mut self) -> Option<&mut dyn TextSurfaceHost> {
        Some(self)
    }
}

use tekes_agent_components_ui::RunReadingError as Failure;
fn paging(run: &RunPort, snapshot: &RunSnapshot) -> TranscriptPagingFacts {
    TranscriptPagingFacts {
        scope: run.scope().clone(),
        supported: true,
        has_more_before: snapshot.transcript.has_more_before,
        is_stale: snapshot.transcript.is_stale,
        source_ready: true,
        read: TranscriptPageRead::Idle,
    }
}
fn prepare<'a>(run: &RunPort, snapshot: &'a RunSnapshot) -> Result<RunPortUpdate<'a>, Failure> {
    run.prepare_update(
        snapshot,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(run, snapshot),
        true,
    )
    .map_err(Failure::Agent)
}
fn run(host: &ContentHost, id: ComponentId) -> &RunPort {
    host.get(id).unwrap().component::<RunPort>().unwrap()
}
fn run_mut(host: &mut ContentHost, id: ComponentId) -> &mut RunPort {
    host.get_mut(id)
        .unwrap()
        .component_mut::<RunPort>()
        .unwrap()
}
fn editor_mut(host: &mut ContentHost, id: ComponentId) -> &mut tekes_ui_controls::TextSurface {
    let editor = run(host, id).ids().composer.editor;
    &mut host
        .get_mut(editor)
        .unwrap()
        .component_mut::<AgentText>()
        .unwrap()
        .field
        .surface
}
fn render(host: &mut ContentHost, text: &mut Text) {
    let frame = Rect::new(0., 0., 760., 650.);
    host.render(&mut Scene::new(), text, frame, frame, Theme::LIGHT.into())
        .unwrap();
}
fn scroll(host: &ContentHost, id: ComponentId) -> &ScrollContent {
    host.get(run(host, id).ids().viewport)
        .unwrap()
        .component::<ScrollContent>()
        .unwrap()
}
async fn setup() -> (
    std::sync::Arc<tekes_agent_session::SessionRuntime>,
    ContentHost,
    ComponentId,
    RunSnapshot,
    ReadingDocumentVersion,
    Text,
) {
    setup_with_fixture(tekes_agent_ui_demo::fixture::Fixture::default()).await
}
async fn setup_with_fixture(
    fixture: tekes_agent_ui_demo::fixture::Fixture,
) -> (
    std::sync::Arc<tekes_agent_session::SessionRuntime>,
    ContentHost,
    ComponentId,
    RunSnapshot,
    ReadingDocumentVersion,
    Text,
) {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup_fixture(fixture)
        .await
        .unwrap();
    let mut snapshot = (*controller.snapshot()).clone();
    snapshot.draft.text = "原草稿 中文".into();
    let mut view = RunPort::standard(2100, 2104..2500, RunScope::main("fixture", "a")).unwrap();
    view.update(
        &snapshot,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&view, &snapshot),
        true,
    )
    .unwrap();
    let root = view.ids().root;
    let viewport = view.ids().viewport;
    let items = view
        .transcript()
        .unwrap()
        .row_keys()
        .iter()
        .map(|key| view.transcript().unwrap().row_id(key).unwrap())
        .collect();
    let mut host = view.into_host().unwrap();
    let (version, ()) = host
        .set_scroll_reading_document(
            viewport,
            ComposerDestination::run(RunScope::main("fixture", "a")).storage_key(),
            snapshot.revision,
            items,
            |_| (),
        )
        .unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    render(&mut host, &mut text);
    (runtime, host, root, snapshot, version, text)
}
fn append(snapshot: &RunSnapshot) -> RunSnapshot {
    let mut next = snapshot.clone();
    next.revision += 1;
    let mut item = next
        .transcript
        .items
        .iter()
        .find(|item| item.kind == TranscriptItemKind::Assistant)
        .unwrap()
        .clone();
    item.id = format!("candidate-{}", next.revision);
    item.record_id = None;
    item.reconciliation_id = None;
    item.turn_id = item.id.clone();
    item.text = "追加的中文回复".into();
    item.content_blocks = None;
    item.metadata = Default::default();
    item.reasoning = String::new();
    item.source_sequence = Some(10000 + next.revision as i64);
    item.source_position = Some(TranscriptSourcePosition::Journal {
        sequence: 10000 + next.revision as i64,
        content_index: None,
    });
    next.transcript.items.push(item);
    next
}
/// Only AgentKit chooses draft policy. The consumer calls the library check;
/// no generic validation, editing clone or scroll compensation is copied here.
fn publish_plan(
    host: &mut ContentHost,
    root: ComponentId,
    version: &ReadingDocumentVersion,
    plan: &RunPortUpdate<'_>,
) -> Result<ReadingDocumentVersion, Failure> {
    tekes_agent_ui_consumer_check::publish_original_run(
        host,
        root,
        version,
        plan.publication(),
        plan,
    )
}
fn baseline(host: &ContentHost, root: ComponentId) -> tekes_ui_controls::TextEditBaseline {
    run(host, root)
        .composer()
        .editor()
        .input
        .external_baseline()
}
#[tokio::test]
async fn availability_refresh_uses_display_revision_without_advancing_provider_publication() {
    let (runtime, mut host, root, first, version, mut text) = setup().await;
    wheel(&mut host, root, 420.);
    render(&mut host, &mut text);
    let item = anchor(&host, root);
    let viewport = run(&host, root).ids().viewport;
    // The paging/status strip changes height with availability. Reading is
    // relative to the actual visible document, not a fixed window coordinate.
    let y = host.get(item).unwrap().frame().min_y()
        - host.get(viewport).unwrap().visible().unwrap().min_y();
    let before = baseline(&host, root);
    let old_rows = rows(&host, root);
    let mut current = version;
    for gate in [ComposerControlGate::CLOSED, ComposerControlGate::OPEN] {
        let plan = run(&host, root)
            .prepare_update(
                &first,
                Default::default(),
                gate,
                paging(run(&host, root), &first),
                gate.entries,
            )
            .unwrap();
        current = tekes_agent_ui_consumer_check::publish_original_run(
            &mut host,
            root,
            &current,
            current.revision() + 1,
            &plan,
        )
        .unwrap();
        render(&mut host, &mut text);
        assert_eq!(
            run(&host, root).displayed_snapshot().unwrap().revision,
            first.revision
        );
        assert_eq!(anchor(&host, root), item);
        let current_y = host.get(item).unwrap().frame().min_y()
            - host.get(viewport).unwrap().visible().unwrap().min_y();
        assert!(
            (current_y - y).abs() <= 1.,
            "old relative y={y}; new relative y={current_y}"
        );
        assert_eq!(baseline(&host, root), before);
        assert_eq!(rows(&host, root), old_rows);
    }
    runtime.stop().await.unwrap();
}
fn rows(host: &ContentHost, root: ComponentId) -> Vec<TranscriptContentRowKey> {
    run(host, root).transcript().unwrap().row_keys().to_vec()
}

#[tokio::test]
async fn reconnect_stale_frame_keeps_a_reading_point_inside_the_completed_turn_footnote() {
    use tekes_agent_contract::EndpointCapabilities;
    use tekes_agent_presentation::{TurnActionContext, present_turn_actions};
    use tekes_ui_controls::{ScrollRevealOptions, ScrollRevealPlacement, extended::Axis};
    let (runtime, mut host, root, first, version, mut text) = setup().await;
    let mut caps = EndpointCapabilities::methods(&["session.fork"]);
    caps.can_fork_completed_turn = true;
    let mut enabled = first.clone();
    enabled.revision += 1;
    present_turn_actions(
        &TurnActionContext {
            capabilities: &caps,
            ready: true,
            running: false,
        },
        &mut enabled.transcript,
    );
    let turn = enabled
        .transcript
        .turns
        .iter()
        .find(|turn| !turn.footnote_actions.is_empty())
        .unwrap();
    let key = TranscriptContentRowKey::TurnFootnote(turn.id.clone());
    let plan = prepare(run(&host, root), &enabled).unwrap();
    let version = publish_plan(&mut host, root, &version, &plan).unwrap();
    render(&mut host, &mut text);
    let footer = run(&host, root).transcript().unwrap().row_id(&key).unwrap();
    let viewport = run(&host, root).ids().viewport;
    host.reveal_scroll_reading_item(
        viewport,
        &version,
        footer,
        ScrollRevealOptions {
            placement: ScrollRevealPlacement::Start(Axis::Vertical),
            animated: false,
            ..Default::default()
        },
        &mut text,
    )
    .unwrap();
    render(&mut host, &mut text);
    // Position the synthetic reader at the footnote's end. Removing the known
    // action during a stale frame previously truncated this local point.
    let within = host.get(footer).unwrap().frame().size.height - 1.;
    wheel(&mut host, root, within);
    render(&mut host, &mut text);
    assert_eq!(anchor(&host, root), footer);
    let y = host.get(footer).unwrap().frame().min_y()
        - host.get(viewport).unwrap().visible().unwrap().min_y();
    let mut current = version;
    for stale in [true, false] {
        let mut next = enabled.clone();
        next.revision = current.revision() + 1;
        next.transcript.is_stale = stale;
        present_turn_actions(
            &TurnActionContext {
                capabilities: &caps,
                ready: true,
                running: false,
            },
            &mut next.transcript,
        );
        assert!(
            next.transcript
                .turns
                .iter()
                .filter_map(|turn| turn.footnote_actions.first())
                .all(|action| action.enabled != stale)
        );
        let plan = prepare(run(&host, root), &next).unwrap();
        current = publish_plan(&mut host, root, &current, &plan).unwrap();
        render(&mut host, &mut text);
        assert_eq!(anchor(&host, root), footer);
        let next_y = host.get(footer).unwrap().frame().min_y()
            - host.get(viewport).unwrap().visible().unwrap().min_y();
        assert!((next_y - y).abs() <= 1., "footnote moved: {y} -> {next_y}");
    }
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn retained_demo_runs_restore_independent_reading_and_reject_pre_unmount_tokens() {
    use std::{sync::Arc, time::Duration};
    use tekes_agent_session::SessionRuntime;
    use tekes_agent_ui_bindings::RunBinding;
    use tekes_agent_ui_demo::{component_demo::ComponentDemo, fixture::Fixture};
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default().with_lifecycle()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let a = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let b = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "b"))
            .await
            .unwrap(),
    );
    for (binding, draft) in [(&a, "A 中文草稿"), (&b, "B 独立草稿")] {
        binding
            .edit_text(
                &ComposerDestination::run(binding.scope().clone()),
                draft.into(),
            )
            .unwrap();
    }
    let mut demo = ComponentDemo::new([a.clone(), b.clone()]).unwrap();
    demo.enable_transcript_content().unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let a_root = ComponentId(demo.ids().root.0 + 16);
    render(&mut demo.tree, &mut text);
    wheel(&mut demo.tree, a_root, 420.);
    render(&mut demo.tree, &mut text);
    let a_item = anchor(&demo.tree, a_root);
    let relative_y = |host: &ContentHost, root, item| {
        host.get(item).unwrap().frame().min_y()
            - host
                .get(run(host, root).ids().viewport)
                .unwrap()
                .visible()
                .unwrap()
                .min_y()
    };
    let a_y = relative_y(&demo.tree, a_root, a_item);
    let old_a_version = scroll(&demo.tree, a_root)
        .reading_document_version()
        .unwrap()
        .clone();
    let a_baseline = baseline(&demo.tree, a_root);
    let a_editor = run(&demo.tree, a_root).composer().editor() as *const _ as usize;
    demo.select(1).unwrap();
    let b_root = ComponentId(demo.ids().root.0 + 16);
    render(&mut demo.tree, &mut text);
    wheel(&mut demo.tree, b_root, 180.);
    render(&mut demo.tree, &mut text);
    let b_item = anchor(&demo.tree, b_root);
    let b_y = relative_y(&demo.tree, b_root, b_item);
    assert_ne!(a_item, b_item);
    // An explicit original-A page completes while B owns the window. No
    // current-selection resampling may publish it into B's document/editor.
    let b_version = scroll(&demo.tree, b_root)
        .reading_document_version()
        .unwrap()
        .clone();
    let b_rows = rows(&demo.tree, b_root);
    assert!(a.load_earlier().await.unwrap());
    assert_eq!(rows(&demo.tree, b_root), b_rows);
    assert_eq!(
        scroll(&demo.tree, b_root).reading_document_version(),
        Some(&b_version)
    );
    demo.select(0).unwrap();
    render(&mut demo.tree, &mut text);
    assert_eq!(anchor(&demo.tree, a_root), a_item);
    assert!((relative_y(&demo.tree, a_root, a_item) - a_y).abs() <= 1.);
    assert_eq!(
        run(&demo.tree, a_root).composer().editor() as *const _ as usize,
        a_editor
    );
    assert_ne!(
        baseline(&demo.tree, a_root),
        a_baseline,
        "unmount fences old editor work"
    );
    assert_eq!(demo.editor().input.text, "A 中文草稿");
    let restored_baseline = baseline(&demo.tree, a_root);
    let current = scroll(&demo.tree, a_root)
        .reading_document_version()
        .unwrap()
        .clone();
    assert_ne!(
        current, old_a_version,
        "unmount renews opaque attachment identity"
    );
    let snapshot = a.current_run().unwrap();
    let plan = prepare(run(&demo.tree, a_root), &snapshot).unwrap();
    let before = rows(&demo.tree, a_root);
    assert!(matches!(
        tekes_agent_ui_consumer_check::publish_original_run(
            &mut demo.tree,
            a_root,
            &old_a_version,
            current.revision() + 1,
            &plan,
        ),
        Err(Failure::Reading(_))
    ));
    assert_eq!(rows(&demo.tree, a_root), before);
    assert_eq!(baseline(&demo.tree, a_root), restored_baseline);
    demo.select(1).unwrap();
    render(&mut demo.tree, &mut text);
    assert_eq!(anchor(&demo.tree, b_root), b_item);
    assert!((relative_y(&demo.tree, b_root, b_item) - b_y).abs() <= 1.);
    assert_eq!(demo.editor().input.text, "B 独立草稿");
    // Reconnect while A is detached. Canonical refresh may replace its history
    // window, but it cannot restore B's reading point into A or submit a draft.
    let epoch = a.current_run().unwrap().connection_epoch;
    let mut a_changes = a.subscribe_run().unwrap();
    let mut b_changes = b.subscribe_run().unwrap();
    runtime.reconnect().await.unwrap();
    for (binding, changes) in [(&a, &mut a_changes), (&b, &mut b_changes)] {
        tokio::time::timeout(Duration::from_secs(3), async {
            while binding
                .current_run()
                .is_none_or(|run| run.connection_epoch <= epoch || run.transcript.is_stale)
            {
                changes.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
    }
    demo.sync().unwrap();
    render(&mut demo.tree, &mut text);
    assert_eq!(anchor(&demo.tree, b_root), b_item);
    assert!((relative_y(&demo.tree, b_root, b_item) - b_y).abs() <= 1.);
    demo.select(0).unwrap();
    render(&mut demo.tree, &mut text);
    assert_eq!(anchor(&demo.tree, a_root), a_item);
    assert!((relative_y(&demo.tree, a_root, a_item) - a_y).abs() <= 1.);
    assert_eq!(demo.editor().input.text, "A 中文草稿");
    assert!(a.current_run().unwrap().submissions.is_empty());
    assert!(b.current_run().unwrap().submissions.is_empty());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn formal_run_updates_editor_and_transcript_together_and_old_reading_token_changes_neither() {
    let (runtime, mut host, root, first, version, mut text) = setup().await;
    let editor = run(&host, root).ids().composer.editor;
    host.focus(Some(editor), true);
    let original = baseline(&host, root);
    let mut next = append(&first);
    next.draft.text = "新权威草稿".into();
    let plan = prepare(run(&host, root), &next).unwrap();
    let item_ids = plan.item_ids();
    let current = publish_plan(&mut host, root, &version, &plan).unwrap();
    render(&mut host, &mut text);
    assert_eq!(
        scroll(&host, root).reading_document_version(),
        Some(&current)
    );
    assert_eq!(
        run(&host, root).composer().editor().input.text,
        next.draft.text
    );
    assert_eq!(host.focused(), Some(editor));
    let actual = run(&host, root)
        .transcript()
        .unwrap()
        .row_keys()
        .iter()
        .map(|key| run(&host, root).transcript().unwrap().row_id(key).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(actual, item_ids);
    // An old baseline on this same editor may still acknowledge the exact
    // current value; replacing the instance would instead be ForeignEditor.
    let surface = run(&host, root).composer().editor();
    let value = surface.input.external_value().unwrap();
    assert_eq!(
        surface.check_external_value(
            &original,
            value,
            tekes_ui_controls::ExternalSelection::Preserve,
            tekes_ui_controls::ExternalUndo::Preserve
        ),
        ExternalValueResult::Unchanged
    );
    let before = baseline(&host, root);
    let old_rows = rows(&host, root);
    let mut late = append(&next);
    late.draft.text = "迟到帧不能安装".into();
    let late_plan = prepare(run(&host, root), &late).unwrap();
    assert!(matches!(
        publish_plan(&mut host, root, &version, &late_plan),
        Err(Failure::Reading(_))
    ));
    assert_eq!(baseline(&host, root), before);
    assert_eq!(rows(&host, root), old_rows);
    assert_eq!(
        scroll(&host, root).reading_document_version(),
        Some(&current)
    );
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn composition_conflict_and_edit_undo_aba_are_rejected_before_reading_version_or_transcript_changes()
 {
    let (runtime, mut host, root, first, version, _text) = setup().await;
    let mut next = append(&first);
    next.draft.text = "不能覆盖组字".into();
    let input = &mut editor_mut(&mut host, root).input;
    let original = input.text.clone();
    let len = input.len();
    assert!(input.preedit(&original, 0..len, Some(0..len)));
    let plan = prepare(run(&host, root), &next).unwrap();
    let before = baseline(&host, root);
    let old_rows = rows(&host, root);
    assert_eq!(
        publish_plan(&mut host, root, &version, &plan),
        Err(Failure::Editor(ExternalValueConflict::Composition))
    );
    assert_eq!(baseline(&host, root), before);
    assert_eq!(rows(&host, root), old_rows);
    assert_eq!(
        scroll(&host, root).reading_document_version(),
        Some(&version)
    );
    editor_mut(&mut host, root).input.cancel_composition();
    let plan = prepare(run(&host, root), &next).unwrap();
    let input = &mut editor_mut(&mut host, root).input;
    assert!(input.insert("暂存编辑", None));
    assert!(input.undo());
    let before = baseline(&host, root);
    let request = plan.controlled_draft().unwrap();
    assert_eq!(
        run(&host, root).composer().editor().check_external_value(
            request.expected(),
            request.value(),
            request.selection().clone(),
            request.undo()
        ),
        ExternalValueResult::Conflict(ExternalValueConflict::StaleBaseline)
    );
    assert_eq!(
        publish_plan(&mut host, root, &version, &plan),
        Err(Failure::Agent("stale or foreign RunPort update plan"))
    );
    assert_eq!(baseline(&host, root), before);
    assert_eq!(rows(&host, root), old_rows);
    assert_eq!(
        scroll(&host, root).reading_document_version(),
        Some(&version)
    );
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn unchanged_authority_during_preedit_can_append_transcript_without_replaying_or_submitting_the_draft()
 {
    let (runtime, mut host, root, first, version, mut text) = setup().await;
    let editor = run(&host, root).ids().composer.editor;
    host.focus(Some(editor), true);
    editor_mut(&mut host, root)
        .input
        .preedit("中文候选", 0..4, None);
    let before = baseline(&host, root);
    let next = append(&first);
    let plan = prepare(run(&host, root), &next).unwrap();
    assert!(plan.controlled_draft().is_none());
    let current = publish_plan(&mut host, root, &version, &plan).unwrap();
    render(&mut host, &mut text);
    assert_eq!(baseline(&host, root), before);
    assert!(run(&host, root).composer().is_composing());
    assert_eq!(host.focused(), Some(editor));
    assert!(
        run(&host, root)
            .transcript()
            .unwrap()
            .message_id(TranscriptMessageKey::Item(&format!(
                "candidate-{}",
                next.revision
            )))
            .is_some()
    );
    assert_eq!(
        scroll(&host, root).reading_document_version(),
        Some(&current)
    );
    assert!(run_mut(&mut host, root).take_intents(&[]).is_empty());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn controlled_revision_and_foreign_editor_fail_before_any_reading_callback_can_publish() {
    use tekes_ui_controls::{DraftRevision, ExternalSelection, ExternalUndo};
    let (runtime, mut host, root, first, version, _text) = setup().await;
    let mut next = append(&first);
    next.draft.text = "较旧的权威草稿".into();
    let surface = editor_mut(&mut host, root);
    let expected = surface.input.external_baseline();
    let acknowledged = surface.input.external_value().unwrap().clone();
    assert!(matches!(
        surface.apply_external_value(
            &expected,
            &DraftRevision {
                version: acknowledged.version + 100,
                ..acknowledged
            },
            ExternalSelection::Preserve,
            ExternalUndo::Preserve
        ),
        ExternalValueResult::Applied { .. }
    ));
    let before = baseline(&host, root);
    let before_rows = rows(&host, root);
    let plan = prepare(run(&host, root), &next).unwrap();
    assert_eq!(
        publish_plan(&mut host, root, &version, &plan),
        Err(Failure::Editor(ExternalValueConflict::Revision))
    );
    assert_eq!(baseline(&host, root), before);
    assert_eq!(rows(&host, root), before_rows);
    assert_eq!(
        scroll(&host, root).reading_document_version(),
        Some(&version)
    );
    // Another simultaneously mounted Run may show the exact same scope and
    // values; the opaque editor baseline must still reject cross-window reuse.
    let mut other = RunPort::standard(5100, 5104..5500, RunScope::main("fixture", "a")).unwrap();
    other
        .update(
            &first,
            Default::default(),
            ComposerControlGate::OPEN,
            paging(&other, &first),
            true,
        )
        .unwrap();
    let other_id = other.ids().root;
    let viewport = other.ids().viewport;
    let items = other
        .transcript()
        .unwrap()
        .row_keys()
        .iter()
        .map(|key| other.transcript().unwrap().row_id(key).unwrap())
        .collect();
    let mut other_host = other.into_host().unwrap();
    let (other_version, ()) = other_host
        .set_scroll_reading_document(
            viewport,
            "same-scope-second-window",
            first.revision,
            items,
            |_| (),
        )
        .unwrap();
    let before = baseline(&other_host, other_id);
    let other_rows = rows(&other_host, other_id);
    let request = plan.controlled_draft().unwrap();
    assert_eq!(
        run(&other_host, other_id)
            .composer()
            .editor()
            .check_external_value(
                request.expected(),
                request.value(),
                request.selection().clone(),
                request.undo()
            ),
        ExternalValueResult::Conflict(ExternalValueConflict::ForeignEditor)
    );
    assert_eq!(
        publish_plan(&mut other_host, other_id, &other_version, &plan),
        Err(Failure::Agent("stale or foreign RunPort update plan"))
    );
    assert_eq!(baseline(&other_host, other_id), before);
    assert_eq!(rows(&other_host, other_id), other_rows);
    assert_eq!(
        scroll(&other_host, other_id).reading_document_version(),
        Some(&other_version)
    );
    runtime.stop().await.unwrap();
}
fn wheel(host: &mut ContentHost, root: ComponentId, delta: f32) {
    let position = host
        .get(run(host, root).ids().viewport)
        .unwrap()
        .visible()
        .expect("actual unobstructed document clip")
        .center();
    host.event(&ContentEvent::Scroll {
        position,
        delta: Point::new(0., delta),
        precise: true,
        begin: true,
        end: true,
        momentum: false,
        time: 1.,
    });
}
fn anchor(host: &ContentHost, root: ComponentId) -> ComponentId {
    match scroll(host, root).reading_result().unwrap().anchor {
        ContentReadingAnchorUpdate::Preserved(id)
        | ContentReadingAnchorUpdate::Fallback(Some(id)) => id,
        ref other => panic!("expected item reading point, got {other:?}"),
    }
}
#[tokio::test]
async fn formal_run_append_and_prepend_preserve_the_reading_item_while_composer_height_changes() {
    let (runtime, mut host, root, first, version, mut text) = setup().await;
    wheel(&mut host, root, 420.);
    render(&mut host, &mut text);
    assert!(!scroll(&host, root).is_following_latest());
    let item = anchor(&host, root);
    let y = host.get(item).unwrap().frame().min_y();
    let editor_id = run(&host, root).ids().composer.editor;
    host.focus(Some(editor_id), true);
    let mut next = append(&first);
    next.draft.text = "较长的中文草稿布局\n".repeat(6);
    let plan = prepare(run(&host, root), &next).unwrap();
    let version = publish_plan(&mut host, root, &version, &plan).unwrap();
    render(&mut host, &mut text);
    assert_eq!(anchor(&host, root), item);
    assert!((host.get(item).unwrap().frame().min_y() - y).abs() <= 1.);
    let mut prefix = next.clone();
    prefix.revision += 1;
    let mut older = prefix
        .transcript
        .items
        .iter()
        .find(|item| item.kind == TranscriptItemKind::User)
        .unwrap()
        .clone();
    older.id = "candidate-older".into();
    older.turn_id = "candidate-older-turn".into();
    older.record_id = None;
    older.reconciliation_id = None;
    older.source_sequence = Some(0);
    older.source_position = Some(TranscriptSourcePosition::Journal {
        sequence: 0,
        content_index: None,
    });
    older.metadata = Default::default();
    prefix.transcript.turns.insert(
        0,
        TranscriptTurn {
            id: older.turn_id.clone(),
            status: "completed".into(),
            input: older.text.clone(),
            started_at: Some(0.),
            ended_at: Some(1.),
            usage: Default::default(),
            fork_boundary: None,
            footnote_actions: vec![],
            live_usage: None,
            reported_usage: None,
        },
    );
    prefix.transcript.items.insert(0, older);
    let plan = prepare(run(&host, root), &prefix).unwrap();
    let version = publish_plan(&mut host, root, &version, &plan).unwrap();
    render(&mut host, &mut text);
    let older = run(&host, root)
        .transcript()
        .unwrap()
        .message_id(TranscriptMessageKey::Item("candidate-older"))
        .unwrap();
    assert!(host.get(older).unwrap().frame().min_y() < host.get(item).unwrap().frame().min_y());
    assert_eq!(anchor(&host, root), item);
    assert!((host.get(item).unwrap().frame().min_y() - y).abs() <= 1.);
    assert_eq!(
        scroll(&host, root).reading_document_version(),
        Some(&version)
    );
    assert_eq!(host.focused(), Some(editor_id));
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn delayed_send_clear_receipt_cannot_erase_new_local_input_or_publish_its_transcript_frame() {
    let (runtime, mut host, root, first, version, _text) = setup().await;
    let send = run(&host, root).ids().composer.send;
    host.accessibility_action(send, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let intents = run_mut(&mut host, root).take_intents(&actions);
    assert_eq!(
        intents
            .iter()
            .filter(|intent| matches!(intent, RunPortIntent::Composer(ComposerIntent::Send { .. })))
            .count(),
        1
    );
    // This is a UI admission/receipt fixture, not a dispatched endpoint RPC.
    assert!(
        editor_mut(&mut host, root)
            .input
            .insert("发送后的新输入", None)
    );
    let before = baseline(&host, root);
    let before_text = run(&host, root).composer().editor().input.text.clone();
    let before_rows = rows(&host, root);
    let mut cleared = append(&first);
    cleared.draft.text.clear();
    let plan = prepare(run(&host, root), &cleared).unwrap();
    assert!(plan.controlled_draft().is_some());
    assert_eq!(
        publish_plan(&mut host, root, &version, &plan),
        Err(Failure::Editor(ExternalValueConflict::StaleBaseline))
    );
    assert_eq!(baseline(&host, root), before);
    assert_eq!(run(&host, root).composer().editor().input.text, before_text);
    assert_eq!(rows(&host, root), before_rows);
    assert_eq!(
        scroll(&host, root).reading_document_version(),
        Some(&version)
    );
    assert!(
        !run_mut(&mut host, root)
            .take_intents(&[])
            .iter()
            .any(|intent| matches!(intent, RunPortIntent::Composer(ComposerIntent::Send { .. })))
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn same_publication_close_open_aba_rejects_a_stored_plan_before_reading_callback() {
    let (runtime, mut host, root, first, version, _text) = setup().await;
    let mut next = append(&first);
    next.draft.text = "旧权限计划".into();
    let plan = prepare(run(&host, root), &next).unwrap();
    let before = baseline(&host, root);
    let before_rows = rows(&host, root);
    for gate in [ComposerControlGate::CLOSED, ComposerControlGate::OPEN] {
        let page = paging(run(&host, root), &first);
        run_mut(&mut host, root)
            .update(&first, Default::default(), gate, page, gate.entries)
            .unwrap();
    }
    assert_eq!(baseline(&host, root), before);
    let request = plan.controlled_draft().unwrap();
    assert!(matches!(
        run(&host, root).composer().editor().check_external_value(
            request.expected(),
            request.value(),
            request.selection().clone(),
            request.undo()
        ),
        ExternalValueResult::Applied { .. }
    ));
    assert_eq!(
        publish_plan(&mut host, root, &version, &plan),
        Err(Failure::Agent("stale or foreign RunPort update plan"))
    );
    assert_eq!(baseline(&host, root), before);
    assert_eq!(rows(&host, root), before_rows);
    assert_eq!(
        scroll(&host, root).reading_document_version(),
        Some(&version)
    );
    let plan = prepare(run(&host, root), &next).unwrap();
    let mut page = paging(run(&host, root), &first);
    page.read = TranscriptPageRead::Loading;
    run_mut(&mut host, root).update_paging(page).unwrap();
    assert_eq!(
        publish_plan(&mut host, root, &version, &plan),
        Err(Failure::Agent("stale or foreign RunPort update plan"))
    );
    let plan = prepare(run(&host, root), &next).unwrap();
    run_mut(&mut host, root)
        .update_actionable(first.revision, first.connection_epoch, None, false)
        .unwrap();
    assert_eq!(
        publish_plan(&mut host, root, &version, &plan),
        Err(Failure::Agent("stale or foreign RunPort update plan"))
    );
    assert_eq!(baseline(&host, root), before);
    assert_eq!(rows(&host, root), before_rows);
    assert_eq!(
        scroll(&host, root).reading_document_version(),
        Some(&version)
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn collapsed_formal_run_retains_library_reading_attachment_and_anchor_until_reopen() {
    let (runtime, mut host, root, _, version, mut text) = setup().await;
    wheel(&mut host, root, 420.);
    render(&mut host, &mut text);
    let item = anchor(&host, root);
    let y = host.get(item).unwrap().frame().min_y();
    let geometry = run(&host, root).layout_geometry().unwrap();
    for frame in [Rect::new(0., 0., 0., 650.), Rect::new(0., 0., 760., 0.)] {
        host.render(
            &mut Scene::new(),
            &mut text,
            frame,
            frame,
            Theme::LIGHT.into(),
        )
        .unwrap();
        assert!(!run(&host, root).is_pane_presented());
        assert_eq!(run(&host, root).layout_geometry(), Some(geometry));
        assert_eq!(
            scroll(&host, root).reading_document_version(),
            Some(&version)
        );
        assert!(!scroll(&host, root).is_following_latest());
    }
    render(&mut host, &mut text);
    assert!(run(&host, root).is_pane_presented());
    assert_eq!(
        scroll(&host, root).reading_document_version(),
        Some(&version)
    );
    assert_eq!(anchor(&host, root), item);
    assert!((host.get(item).unwrap().frame().min_y() - y).abs() <= 1.);
    assert!(!scroll(&host, root).is_following_latest());
    runtime.stop().await.unwrap();
}

// Append to the unchanged formal Run transaction probe. These are actual
// AgentKit rail controls, not a text/button substitute for its content.
use tekes_ui_controls::{ControlKey, ScrollRevealOptions, ScrollRevealPlacement};
fn rail(host: &ContentHost, root: ComponentId) -> &TurnRail {
    run(host, root).turn_rail().unwrap()
}
fn rail_mut(host: &mut ContentHost, root: ComponentId) -> &mut TurnRail {
    run_mut(host, root).turn_rail_mut().unwrap()
}
async fn rail_setup() -> (
    std::sync::Arc<tekes_agent_session::SessionRuntime>,
    ContentHost,
    ComponentId,
    RunSnapshot,
    ReadingDocumentVersion,
    ReadingDocumentVersion,
    Text,
) {
    let (runtime, mut host, root, snapshot, version, mut text) = setup().await;
    tekes_agent_ui_consumer_check::install_turn_rail(run_mut(&mut host, root), 35000).unwrap();
    run_mut(&mut host, root)
        .set_rail_navigation_gate(TurnRailNavigationGate {
            turn_start: true,
            latest: true,
        })
        .unwrap();
    render(&mut host, &mut text);
    let rail_version = sync_run_rail_reading(&mut host, root).unwrap();
    render(&mut host, &mut text);
    (runtime, host, root, snapshot, version, rail_version, text)
}
fn navigate_rail(
    host: &mut ContentHost,
    root: ComponentId,
    version: &ReadingDocumentVersion,
    text: &mut Text,
) -> Option<TurnRailReveal> {
    let request = rail_mut(host, root).next_reveal()?;
    reveal_run_rail_follow(host, root, version, false, text)
        .unwrap()
        .unwrap();
    render(host, text);
    Some(request)
}
fn rail_intents(host: &mut ContentHost, root: ComponentId) -> Vec<RunTurnNavigation> {
    let actions = host.drain_actions();
    run_mut(host, root)
        .take_intents(&actions)
        .into_iter()
        .filter_map(|i| match i {
            RunPortIntent::TurnNavigation(n) => Some(*n),
            _ => None,
        })
        .collect()
}
#[tokio::test]
async fn actual_rail_exact_geometry_half_open_boundaries_and_full_single_ax_names_use_original_controls()
 {
    let (runtime, mut host, root, _, _, _, mut text) = rail_setup().await;
    let targets = run(&host, root).turn_targets().unwrap();
    let geometry = run(&host, root).layout_geometry().unwrap().turn_rail;
    assert_eq!(rail(&host, root).layout_geometry(), Some(geometry));
    let ids = rail(&host, root).item_ids();
    assert_eq!(ids.len(), targets.len());
    for (i, id) in ids.iter().enumerate() {
        let f = host.get(*id).unwrap().frame();
        let layout = rail(&host, root)
            .action_layout(targets[i].turn_id())
            .unwrap();
        assert_eq!(f, layout.frame);
        assert_eq!(f.size.height, 10.);
        // ScrollContent reserves the library's focus bleed inside its viewport.
        // Rows fill that original document, without overriding the library inset.
        let document = host
            .get(ComponentId(rail(&host, root).id().0 + 2))
            .unwrap()
            .frame();
        assert_eq!(f.size.width, document.size.width);
        assert_eq!(f.min_x(), document.min_x());
        assert!(f.size.width <= geometry.track.size.width);
        let nodes = host.semantics();
        let named: Vec<_> = nodes
            .iter()
            .filter(|n| n.semantics.name.contains(targets[i].input()))
            .collect();
        assert_eq!(named.iter().filter(|n| n.id == *id).count(), 1);
        assert_eq!(nodes.iter().find(|n| n.id == *id).unwrap().frame, f);
    }
    let boundary = host.get(ids[1]).unwrap().frame().origin;
    host.event(&ContentEvent::PointerDown(Point::new(
        boundary.x + 1.,
        boundary.y,
    )));
    let intents = rail_intents(&mut host, root);
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].target().turn_id(), targets[1].turn_id());
    host.event(&ContentEvent::PointerUp(Point::new(
        boundary.x + 1.,
        boundary.y,
    )));
    assert!(rail_intents(&mut host, root).is_empty());
    render(&mut host, &mut text);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn rail_press_release_cancel_keyboard_ax_and_preedit_capture_once_without_focus_substitution()
{
    let (runtime, mut host, root, _, _, _, mut text) = rail_setup().await;
    let targets = run(&host, root).turn_targets().unwrap();
    let row = rail(&host, root).row_id(targets[0].turn_id()).unwrap();
    let editor = run(&host, root).ids().composer.editor;
    host.focus(Some(editor), true);
    host.event_with_text(
        &ContentEvent::Preedit {
            text: "轨道组字".into(),
            selection: 0..2,
            clauses: vec![],
            replacement: None,
        },
        &mut text,
    )
    .unwrap();
    render(&mut host, &mut text);
    let before = baseline(&host, root);
    let point = host.get(row).unwrap().frame().center();
    host.event(&ContentEvent::PointerDown(point));
    assert_eq!(rail_intents(&mut host, root).len(), 1);
    host.event(&ContentEvent::PointerDown(point));
    assert!(rail_intents(&mut host, root).is_empty());
    host.event(&ContentEvent::PointerMove(Point::new(900., 900.)));
    host.event(&ContentEvent::PointerUp(Point::new(900., 900.)));
    assert!(rail_intents(&mut host, root).is_empty());
    assert_eq!(host.focused(), Some(editor));
    assert_eq!(baseline(&host, root), before);
    host.accessibility_action(row, None, AccessibilityAction::Press);
    assert_eq!(rail_intents(&mut host, root).len(), 1);
    assert_eq!(baseline(&host, root), before);
    assert_eq!(host.focused(), Some(editor));
    host.event(&ContentEvent::Unmark);
    assert!(host.focus(Some(row), true));
    for key in [ControlKey::Enter, ControlKey::Space] {
        host.event(&ContentEvent::Key { key, down: true });
        let mut intents = rail_intents(&mut host, root);
        host.event(&ContentEvent::Key { key, down: false });
        intents.extend(rail_intents(&mut host, root));
        assert_eq!(intents.len(), 1);
    }
    assert_eq!(host.focused(), Some(row));
    host.event(&ContentEvent::PointerDown(point));
    assert_eq!(rail_intents(&mut host, root).len(), 1);
    // Host Cancel means window-wide input/focus cancellation in TekesUI.
    host.event(&ContentEvent::Cancel);
    host.event(&ContentEvent::PointerUp(point));
    assert!(rail_intents(&mut host, root).is_empty());
    assert_eq!(host.focused(), None);
    // Focus decoration is delegated to the original button (unit coverage).
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn event_time_rail_capture_survives_in_place_publication_without_rebinding_and_disabled_gate_rejects()
 {
    let (runtime, mut host, root, snapshot, version, _, mut text) = rail_setup().await;
    let targets = run(&host, root).turn_targets().unwrap();
    let id = rail(&host, root).row_id(targets[0].turn_id()).unwrap();
    let point = host.get(id).unwrap().frame().center();
    host.event(&ContentEvent::PointerDown(point));
    // Delay the owned intent drain across the actual shared publication plan.
    let next = append(&snapshot);
    let plan = prepare(run(&host, root), &next).unwrap();
    publish_plan(&mut host, root, &version, &plan).unwrap();
    render(&mut host, &mut text);
    let intents = rail_intents(&mut host, root);
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].target().publication(), snapshot.revision);
    assert!(run(&host, root).claim_turn_navigation(&intents[0]).is_err());
    assert_eq!(rail(&host, root).row_id(targets[0].turn_id()), Some(id));
    host.event(&ContentEvent::PointerUp(point));
    assert!(rail_intents(&mut host, root).is_empty());
    run_mut(&mut host, root)
        .set_rail_navigation_gate(Default::default())
        .unwrap();
    render(&mut host, &mut text);
    host.event(&ContentEvent::PointerDown(point));
    host.event(&ContentEvent::PointerUp(point));
    host.accessibility_action(id, None, AccessibilityAction::Press);
    assert!(rail_intents(&mut host, root).is_empty());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn rail_initial_and_changed_follow_are_honored_once_and_rejected_commands_require_explicit_retry()
 {
    let (runtime, mut host, root, _, _, rail_version, mut text) = rail_setup().await;
    let targets = run(&host, root).turn_targets().unwrap();
    rail_mut(&mut host, root).set_follow_target(Some(targets.last().unwrap().turn_id().into()));
    let first = navigate_rail(&mut host, root, &rail_version, &mut text).unwrap();
    assert_eq!(first.reason(), TurnRailRevealReason::InitialLatest);
    assert!(rail_mut(&mut host, root).next_reveal().is_none());
    rail_mut(&mut host, root).set_follow_target(Some(targets[0].turn_id().into()));
    navigate_rail(&mut host, root, &rail_version, &mut text).unwrap();
    assert!(rail_mut(&mut host, root).next_reveal().is_none());
    rail_mut(&mut host, root).set_follow_target(Some(targets.last().unwrap().turn_id().into()));
    let pending = rail_mut(&mut host, root).next_reveal().unwrap();
    let clone = pending.clone();
    rail(&host, root).claim_reveal(&pending).unwrap();
    assert!(rail(&host, root).claim_reveal(&clone).is_err());
    assert!(rail_mut(&mut host, root).finish_reveal(&pending, false));
    assert!(rail_mut(&mut host, root).next_reveal().is_none());
    rail_mut(&mut host, root).retry_reveal();
    let fresh = rail_mut(&mut host, root).next_reveal().unwrap();
    assert!(rail(&host, root).claim_reveal(&pending).is_err());
    rail(&host, root).claim_reveal(&fresh).unwrap();
    assert!(rail_mut(&mut host, root).finish_reveal(&fresh, true));
    assert!(rail_mut(&mut host, root).next_reveal().is_none());
    runtime.stop().await.unwrap();
}

fn rail_scroll(host: &ContentHost, root: ComponentId) -> &ScrollContent {
    host.get(rail(host, root).viewport_id())
        .unwrap()
        .component::<ScrollContent>()
        .unwrap()
}
fn add_turns(snapshot: &RunSnapshot, count: usize) -> RunSnapshot {
    let mut next = snapshot.clone();
    next.revision += 1;
    let template = next.transcript.turns[0].clone();
    let input = next
        .transcript
        .items
        .iter()
        .find(|i| i.kind == TranscriptItemKind::User)
        .unwrap()
        .clone();
    for index in 0..count {
        let mut turn = template.clone();
        turn.id = format!("rail-history-{}-{index}", next.revision);
        turn.input = format!("历史问题 {index} 完整中文输入");
        let mut item = input.clone();
        item.id = format!("input-{}", turn.id);
        item.turn_id = turn.id.clone();
        item.text = turn.input.clone();
        item.record_id = None;
        item.reconciliation_id = None;
        item.source_sequence = Some(20000 + index as i64);
        item.source_position = Some(TranscriptSourcePosition::Journal {
            sequence: 20000 + index as i64,
            content_index: None,
        });
        next.transcript.turns.push(turn);
        next.transcript.items.push(item);
    }
    next
}
#[tokio::test]
async fn independent_rail_overflow_initial_latest_and_manual_scroll_do_not_move_the_transcript() {
    let (runtime, mut host, root, snapshot, version, rail_version, mut text) = rail_setup().await;
    let next = add_turns(&snapshot, 80);
    let plan = prepare(run(&host, root), &next).unwrap();
    // Both owners are checked before this synchronous publication. There is no
    // event, render, await or replacement of either owner between the updates.
    assert_eq!(
        rail_scroll(&host, root).reading_document_version(),
        Some(&rail_version)
    );
    let _version = publish_plan(&mut host, root, &version, &plan).unwrap();
    let r = rail(&host, root);
    let revision = r.document_revision();
    let ids = r.item_ids();
    let viewport = r.viewport_id();
    assert!(r.layout_geometry().is_none());
    let (rail_version, ()) = host
        .update_scroll_reading_document(viewport, &rail_version, revision, ids, |_| {})
        .unwrap();
    render(&mut host, &mut text);
    let main_before = scroll(&host, root).scroll.offset;
    assert!(rail_scroll(&host, root).scroll.limit().y > 200.);
    assert!(!rail_scroll(&host, root).scroll.scrollbar_visible(true));
    assert!(!rail_scroll(&host, root).scroll.axis_enabled(false));
    let targets = run(&host, root).turn_targets().unwrap();
    let last = targets.last().unwrap().turn_id().to_owned();
    rail_mut(&mut host, root).set_follow_target(Some(last.clone()));
    navigate_rail(&mut host, root, &rail_version, &mut text).unwrap();
    let last_id = rail(&host, root).row_id(&last).unwrap();
    assert!(host.get(last_id).unwrap().visible().is_some());
    let before = rail_scroll(&host, root).scroll.offset.y;
    let point = rail(&host, root).layout_geometry().unwrap().track.center();
    host.event(&ContentEvent::Scroll {
        position: point,
        delta: Point::new(0., -90.),
        precise: true,
        begin: true,
        end: false,
        momentum: false,
        time: 1.,
    });
    render(&mut host, &mut text);
    let manual = rail_scroll(&host, root).scroll.offset.y;
    assert!(manual < before - 50.);
    rail_mut(&mut host, root).set_follow_target(Some(last));
    assert!(rail_mut(&mut host, root).next_reveal().is_none());
    render(&mut host, &mut text);
    assert_eq!(rail_scroll(&host, root).scroll.offset.y, manual);
    assert_eq!(scroll(&host, root).scroll.offset, main_before);
    rail_mut(&mut host, root).set_follow_target(Some(targets[0].turn_id().into()));
    navigate_rail(&mut host, root, &rail_version, &mut text).unwrap();
    assert!(
        host.get(rail(&host, root).row_id(targets[0].turn_id()).unwrap())
            .unwrap()
            .visible()
            .is_some()
    );
    assert_eq!(scroll(&host, root).scroll.offset, main_before);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn unmount_remount_retires_both_navigation_and_reveal_without_replacing_the_original_editor()
{
    use tekes_ui_controls::extended::{Axis, Stack};
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let snapshot = controller.snapshot();
    let mut view = RunPort::standard(2100, 2104..2500, RunScope::main("fixture", "a")).unwrap();
    view.update(
        &snapshot,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&view, &snapshot),
        true,
    )
    .unwrap();
    view.install_turn_rail(35000).unwrap();
    view.set_rail_navigation_gate(TurnRailNavigationGate {
        turn_start: true,
        latest: true,
    })
    .unwrap();
    let root = view.ids().root;
    let outer = ComponentId(1);
    let mut host = ContentHost::new(Content::new(
        outer,
        StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: 0.,
            },
            flexible_child: None,
            children: vec![Content::new(root, view)],
        },
    ))
    .unwrap();
    host.set_action_events(true);
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    render(&mut host, &mut text);
    let editor = run(&host, root).composer().editor() as *const _;
    let target = run(&host, root).turn_targets().unwrap().remove(0);
    let navigation = run(&host, root).prepare_turn_navigation(&target).unwrap();
    let reveal = rail_mut(&mut host, root).next_reveal().unwrap();
    let removed = host.remove(root).unwrap();
    assert!(
        removed
            .component::<RunPort>()
            .unwrap()
            .claim_turn_navigation(&navigation)
            .is_err()
    );
    host.insert(outer, removed).unwrap();
    render(&mut host, &mut text);
    assert_eq!(run(&host, root).composer().editor() as *const _, editor);
    assert!(run(&host, root).claim_turn_navigation(&navigation).is_err());
    assert!(rail(&host, root).claim_reveal(&reveal).is_err());
    let target = run(&host, root).turn_targets().unwrap().remove(0);
    let fresh = run(&host, root).prepare_turn_navigation(&target).unwrap();
    run(&host, root).claim_turn_navigation(&fresh).unwrap();
    runtime.stop().await.unwrap();
}

// Append to the unchanged original Run and rail probes, using only #32's
// original item reveal. No offsets are written and no margins are fabricated.
fn clipped_render(host: &mut ContentHost, text: &mut Text) {
    let frame = Rect::new(0., 0., 760., 650.);
    host.render(
        &mut Scene::new(),
        text,
        frame,
        Rect::new(0., 150., 760., 500.),
        Theme::LIGHT.into(),
    )
    .unwrap();
}
fn dispatch_start(
    host: &mut ContentHost,
    root: ComponentId,
    version: &ReadingDocumentVersion,
    intent: &RunTurnNavigation,
    text: &mut Text,
) -> ContentRevealResult {
    match tekes_agent_ui_consumer_check::navigate_original_run(
        host,
        root,
        intent,
        version,
        RunNavigationHost::Auxiliary,
        text,
    )
    .unwrap()
    {
        RunNavigationResult::TurnStart(result) => result,
        _ => panic!("ordinary turn must retain Start semantics"),
    }
}
#[tokio::test]
async fn actual_ordinary_rail_click_aligns_at_visible_clip_start_and_preserves_original_preedit() {
    let (runtime, mut host, root, _, version, _, mut text) = rail_setup().await;
    let editor = run(&host, root).ids().composer.editor;
    assert!(host.focus(Some(editor), true));
    host.event_with_text(
        &ContentEvent::Preedit {
            text: "普通回合组字".into(),
            selection: 1..4,
            clauses: vec![],
            replacement: None,
        },
        &mut text,
    )
    .unwrap();
    clipped_render(&mut host, &mut text);
    let before = baseline(&host, root);
    let original_editor = run(&host, root).composer().editor() as *const _;
    let target = run(&host, root).turn_targets().unwrap().remove(3);
    let row = rail(&host, root).row_id(target.turn_id()).unwrap();
    let point = host.get(row).unwrap().visible().unwrap().center();
    host.event(&ContentEvent::PointerDown(point));
    let intents = rail_intents(&mut host, root);
    assert_eq!(intents.len(), 1);
    let intent = &intents[0];
    assert_eq!(intent.target().turn_id(), target.turn_id());
    let x = scroll(&host, root).scroll.offset.x;
    let result = dispatch_start(&mut host, root, &version, intent, &mut text);
    assert_eq!(result.status, ContentRevealStatus::Completed);
    assert!(!result.clamped);
    clipped_render(&mut host, &mut text);
    let top = host
        .get(run(&host, root).ids().viewport)
        .unwrap()
        .visible()
        .unwrap()
        .min_y();
    assert!(top >= 150.);
    assert!((host.get(result.item).unwrap().frame().min_y() - top).abs() < 0.01);
    assert_eq!(scroll(&host, root).scroll.offset.x, x);
    assert!(!scroll(&host, root).is_following_latest());
    host.event(&ContentEvent::PointerUp(point));
    assert!(rail_intents(&mut host, root).is_empty());
    assert_eq!(
        run(&host, root).composer().editor() as *const _,
        original_editor
    );
    assert_eq!(baseline(&host, root), before);
    assert_eq!(host.focused(), Some(editor));
    assert!(run(&host, root).composer().is_composing());
    assert!(run(&host, root).claim_turn_navigation(intent).is_err());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn start_moves_an_already_visible_middle_target_and_a_viewport_tall_input_to_the_same_actual_start()
 {
    let (runtime, mut host, root, snapshot, version, _, mut text) = rail_setup().await;
    let mut next = snapshot.clone();
    next.revision += 1;
    let turn = next.transcript.turns[3].id.clone();
    let tall = "高消息中的中文内容，仍由原正文控件测量换行。\n".repeat(90);
    next.transcript
        .items
        .iter_mut()
        .find(|i| i.kind == TranscriptItemKind::User && i.turn_id == turn)
        .unwrap()
        .text = tall.clone();
    next.transcript.turns[3].input = tall;
    let plan = prepare(run(&host, root), &next).unwrap();
    let version = publish_plan(&mut host, root, &version, &plan).unwrap();
    clipped_render(&mut host, &mut text);
    let target = run(&host, root)
        .turn_targets()
        .unwrap()
        .into_iter()
        .find(|t| t.turn_id() == turn)
        .unwrap();
    let item = target.row_ids()[0];
    // First place the original tall row's midpoint in the clip. Start must
    // still move its top, rather than accepting partial visibility as success.
    host.reveal_scroll_reading_item(
        run(&host, root).ids().viewport,
        &version,
        item,
        ScrollRevealOptions {
            placement: ScrollRevealPlacement::Centered,
            animated: false,
            ..Default::default()
        },
        &mut text,
    )
    .unwrap();
    clipped_render(&mut host, &mut text);
    let viewport = run(&host, root).ids().viewport;
    let visible = host.get(viewport).unwrap().visible().unwrap();
    let row_frame = host.get(item).unwrap().frame();
    assert!(row_frame.size.height > visible.size.height);
    assert!(host.get(item).unwrap().visible().is_some());
    assert!(row_frame.min_y() < visible.min_y() - 50.);
    let target = run(&host, root)
        .turn_targets()
        .unwrap()
        .into_iter()
        .find(|t| t.turn_id() == turn)
        .unwrap();
    let intent =
        tekes_agent_ui_consumer_check::prepare_turn_navigation(run(&host, root), &target).unwrap();
    let result = dispatch_start(&mut host, root, &version, &intent, &mut text);
    assert!(!result.clamped);
    clipped_render(&mut host, &mut text);
    let visible = host.get(viewport).unwrap().visible().unwrap();
    assert!((host.get(item).unwrap().frame().min_y() - visible.min_y()).abs() < 0.01);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn actual_rail_freezes_main_history_before_a_later_reading_fact_and_public_consumer_calls_hooks_once()
 {
    #[derive(Default)]
    struct History(Vec<String>);
    impl RunTurnNavigationHost for History {
        fn record_turn_jump(&mut self, jump: &RunTurnJump) {
            self.0.push(format!(
                "record:{}:{}",
                jump.from_turn().unwrap_or("none"),
                jump.to_turn()
            ));
        }
        fn take_pending_turn_reveal(&mut self, scope: &RunScope) -> Option<String> {
            self.0.push(format!("take:{}", scope.thread_id));
            Some("held-reveal".into())
        }
    }
    let (runtime, mut host, root, _, _, _, mut text) = rail_setup().await;
    let targets = run(&host, root).turn_targets().unwrap();
    run_mut(&mut host, root)
        .set_turn_navigation_context(Some(RunTurnNavigationContext {
            bottom_is_visible: true,
        }))
        .unwrap();
    let row = rail(&host, root).row_id(targets[3].turn_id()).unwrap();
    let point = host.get(row).unwrap().visible().unwrap().center();
    host.event(&ContentEvent::PointerDown(point));
    let intents = rail_intents(&mut host, root);
    assert_eq!(intents.len(), 1);
    run_mut(&mut host, root)
        .set_turn_navigation_context(Some(RunTurnNavigationContext {
            bottom_is_visible: false,
        }))
        .unwrap();
    let mut history = History::default();
    tekes_agent_ui_consumer_check::claim_turn_navigation_with_host(
        run(&host, root),
        &intents[0],
        &mut history,
    )
    .unwrap();
    assert_eq!(
        history.0,
        vec![
            format!(
                "record:{}:{}",
                targets.last().unwrap().turn_id(),
                targets[3].turn_id()
            ),
            "take:a".into()
        ]
    );
    host.event(&ContentEvent::PointerUp(point));
    assert!(rail_intents(&mut host, root).is_empty());
    assert!(
        tekes_agent_ui_consumer_check::claim_turn_navigation_with_host(
            run(&host, root),
            &intents[0].clone(),
            &mut history
        )
        .is_err()
    );
    assert_eq!(history.0.len(), 2);
    render(&mut host, &mut text);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn original_navigation_checks_sdk_token_before_host_history_and_claims_clones_once() {
    #[derive(Default)]
    struct History(Vec<&'static str>);
    impl RunTurnNavigationHost for History {
        fn record_turn_jump(&mut self, _: &RunTurnJump) {
            self.0.push("record");
        }
        fn take_pending_turn_reveal(&mut self, _: &RunScope) -> Option<String> {
            self.0.push("take");
            None
        }
    }
    let (runtime, mut host, root, _, version, _, mut text) = rail_setup().await;
    run_mut(&mut host, root)
        .set_turn_navigation_context(Some(RunTurnNavigationContext {
            bottom_is_visible: false,
        }))
        .unwrap();
    let target = run(&host, root).turn_targets().unwrap().remove(2);
    let intent = run(&host, root).prepare_turn_navigation(&target).unwrap();
    // Renew only original SDK attachment identity, keeping the same Agent frame.
    let viewport = run(&host, root).ids().viewport;
    let items = run(&host, root)
        .transcript()
        .unwrap()
        .row_keys()
        .iter()
        .map(|k| run(&host, root).transcript().unwrap().row_id(k).unwrap())
        .collect();
    let (fresh, ()) = host
        .set_scroll_reading_document(viewport, version.key(), version.revision(), items, |_| ())
        .unwrap();
    render(&mut host, &mut text);
    let mut history = History::default();
    assert!(
        tekes_agent_ui_consumer_check::navigate_original_run(
            &mut host,
            root,
            &intent,
            &version,
            RunNavigationHost::Main(&mut history),
            &mut text
        )
        .is_err()
    );
    assert!(history.0.is_empty());
    tekes_agent_ui_consumer_check::navigate_original_run(
        &mut host,
        root,
        &intent,
        &fresh,
        RunNavigationHost::Main(&mut history),
        &mut text,
    )
    .unwrap();
    assert_eq!(history.0, vec!["record", "take"]);
    assert!(
        tekes_agent_ui_consumer_check::navigate_original_run(
            &mut host,
            root,
            &intent.clone(),
            &fresh,
            RunNavigationHost::Main(&mut history),
            &mut text
        )
        .is_err()
    );
    assert_eq!(history.0.len(), 2);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn separate_rail_attachment_and_auto_reveal_leave_transcript_and_original_editor_untouched() {
    let (runtime, mut host, root, _, version, rail_version, mut text) = rail_setup().await;
    let editor = run(&host, root).ids().composer.editor;
    host.focus(Some(editor), true);
    host.event_with_text(
        &ContentEvent::Preedit {
            text: "独立轨道组字".into(),
            selection: 1..3,
            clauses: vec![],
            replacement: None,
        },
        &mut text,
    )
    .unwrap();
    render(&mut host, &mut text);
    let before = baseline(&host, root);
    let offset = scroll(&host, root).scroll.offset;
    assert_eq!(
        sync_run_rail_reading(&mut host, root).unwrap(),
        rail_version
    );
    assert!(
        reveal_run_rail_follow(&mut host, root, &rail_version, false, &mut text)
            .unwrap()
            .is_some()
    );
    render(&mut host, &mut text);
    assert!(
        reveal_run_rail_follow(&mut host, root, &rail_version, false, &mut text)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        scroll(&host, root).reading_document_version(),
        Some(&version)
    );
    assert_eq!(scroll(&host, root).scroll.offset, offset);
    assert_eq!(baseline(&host, root), before);
    assert_eq!(host.focused(), Some(editor));
    runtime.stop().await.unwrap();
}

#[test]
fn jump_visibility_tracks_unknown_geometry_disclosures_and_preserved_occlusion_acknowledgement() {
    let mut state = RunJumpVisibility::default();
    assert!(!state.shows_button());
    state.update_geometry(41., 0.);
    assert!(state.shows_button());
    state.disclosure(1, true);
    state.disclosure(2, true);
    assert!(!state.shows_button());
    state.update_geometry(200., 0.);
    state.disclosure(1, false);
    assert!(!state.shows_button());
    state.disclosure(2, false);
    assert!(state.shows_button());
    state.update_geometry(40., 0.);
    assert!(!state.shows_button());
    state.preserved_occlusion_change(Some(0.), 20., 100.);
    assert!(state.shows_button());
    state.update_geometry(0., 80.); // queued sample predates new controls
    assert!(state.shows_button());
    state.update_geometry(0., 100.); // settled acknowledgement
    assert!(!state.shows_button());
    state.preserved_occlusion_change(Some(70.), -20., 80.);
    assert!(state.shows_button());
    state.incoming_document();
    assert!(!state.shows_button());
    state.update_geometry(f32::NAN, 80.);
    assert!(!state.shows_button());
}
fn jump_intents(host: &mut ContentHost, root: ComponentId) -> Vec<RunJumpLatest> {
    let actions = host.drain_actions();
    run_mut(host, root)
        .take_intents(&actions)
        .into_iter()
        .filter_map(|i| match i {
            RunPortIntent::JumpLatest(n) => Some(*n),
            _ => None,
        })
        .collect()
}
#[tokio::test]
async fn jump_uses_original_control_and_follow_latest_without_changing_editor_or_reading_attachment()
 {
    let (runtime, mut host, root, _, version, mut text) = setup().await;
    run_mut(&mut host, root)
        .install_jump_to_latest(36000)
        .unwrap();
    let viewport = run(&host, root).ids().viewport;
    host.get_mut(viewport)
        .unwrap()
        .component_mut::<ScrollContent>()
        .unwrap()
        .follow_latest();
    render(&mut host, &mut text);
    assert!(
        !run(&host, root).jump_is_visible(),
        "incoming document at latest must not flash Jump"
    );
    wheel(&mut host, root, -420.);
    render(&mut host, &mut text);
    assert!(run(&host, root).jump_is_visible());
    let button = run(&host, root).jump_button_id().unwrap();
    let frame = host.get(button).unwrap().frame();
    let geometry = run(&host, root).layout_geometry().unwrap();
    assert!((frame.center().x - geometry.pane.center().x).abs() < 0.01);
    assert!(
        (frame.max_y() + tekes_ui_tokens::Spacing::Regular.value()
            - geometry.composer.unwrap().min_y())
        .abs()
            < 0.01
    );
    let editor = run(&host, root).ids().composer.editor;
    host.focus(Some(editor), true);
    host.event_with_text(
        &ContentEvent::Preedit {
            text: "回到底部中文组字".into(),
            selection: 1..3,
            clauses: vec![],
            replacement: None,
        },
        &mut text,
    )
    .unwrap();
    render(&mut host, &mut text);
    let before = baseline(&host, root);
    let owner = run(&host, root).composer().editor() as *const _;
    host.event(&ContentEvent::PointerDown(frame.center()));
    let intents = jump_intents(&mut host, root);
    assert_eq!(intents.len(), 1);
    tekes_agent_ui_consumer_check::jump_original_run(&mut host, root, &intents[0], &version)
        .unwrap();
    assert!(
        tekes_agent_ui_consumer_check::jump_original_run(
            &mut host,
            root,
            &intents[0].clone(),
            &version
        )
        .is_err()
    );
    host.event(&ContentEvent::PointerUp(frame.center()));
    assert!(jump_intents(&mut host, root).is_empty());
    render(&mut host, &mut text);
    assert!(scroll(&host, root).is_following_latest());
    assert!(!run(&host, root).jump_is_visible());
    assert!(!host.semantics().iter().any(|n| n.id == button));
    assert_eq!(host.focused(), Some(editor));
    assert_eq!(baseline(&host, root), before);
    assert_eq!(run(&host, root).composer().editor() as *const _, owner);
    assert_eq!(
        scroll(&host, root).reading_document_version(),
        Some(&version)
    );
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn jump_rejects_prior_publication_and_sdk_attachment_before_following() {
    let (runtime, mut host, root, snapshot, version, mut text) = setup().await;
    run_mut(&mut host, root)
        .install_jump_to_latest(36000)
        .unwrap();
    wheel(&mut host, root, 420.);
    render(&mut host, &mut text);
    let button = run(&host, root).jump_button_id().unwrap();
    host.accessibility_action(button, None, AccessibilityAction::Press);
    let intent = jump_intents(&mut host, root).pop().unwrap();
    let mut next = snapshot.clone();
    next.revision += 1;
    let plan = prepare(run(&host, root), &next).unwrap();
    let fresh = publish_plan(&mut host, root, &version, &plan).unwrap();
    render(&mut host, &mut text);
    assert!(
        tekes_agent_ui_consumer_check::jump_original_run(&mut host, root, &intent, &fresh).is_err()
    );
    assert!(!scroll(&host, root).is_following_latest());
    host.accessibility_action(button, None, AccessibilityAction::Press);
    let intent = jump_intents(&mut host, root).pop().unwrap();
    assert!(
        tekes_agent_ui_consumer_check::jump_original_run(&mut host, root, &intent, &version)
            .is_err()
    );
    assert!(!scroll(&host, root).is_following_latest());
    tekes_agent_ui_consumer_check::jump_original_run(&mut host, root, &intent, &fresh).unwrap();
    runtime.stop().await.unwrap();
}

fn readonly_input<'a>(
    scope: &'a RunScope,
    snapshot: &'a RunSnapshot,
    publication: u64,
    loading: bool,
) -> ReadOnlyTranscriptInput<'a> {
    ReadOnlyTranscriptInput {
        scope,
        publication,
        connection_epoch: snapshot.connection_epoch,
        connection: &snapshot.connection,
        transcript: &snapshot.transcript,
        frame_current: true,
        source_ready: true,
        supports_pagination: true,
        reading: loading,
        read_error: None,
    }
}
fn readonly(host: &ContentHost, root: ComponentId) -> &ReadOnlyTranscript {
    host.get(root).unwrap().component().unwrap()
}
fn readonly_scroll(host: &ContentHost, root: ComponentId) -> &ScrollContent {
    host.get(readonly(host, root).ids().viewport)
        .unwrap()
        .component()
        .unwrap()
}
fn readonly_anchor(host: &ContentHost, root: ComponentId) -> ComponentId {
    match readonly_scroll(host, root).reading_result().unwrap().anchor {
        ContentReadingAnchorUpdate::Preserved(id)
        | ContentReadingAnchorUpdate::Fallback(Some(id)) => id,
        ref other => panic!("read-only anchor {other:?}"),
    }
}
fn readonly_y(host: &ContentHost, root: ComponentId, item: ComponentId) -> f32 {
    host.get(item).unwrap().frame().min_y()
        - host
            .get(readonly(host, root).ids().viewport)
            .unwrap()
            .visible()
            .unwrap()
            .min_y()
}
#[tokio::test]
async fn readonly_transaction_preserves_anchor_across_availability_refresh_and_history_prepend() {
    let (runtime, _, _, full, _, mut text) = setup().await;
    let scope = RunScope {
        session_id: "bounded-b".into(),
        ..RunScope::main("fixture", "b")
    };
    let mut initial = full.clone();
    let keep: std::collections::BTreeSet<_> = initial
        .transcript
        .turns
        .iter()
        .skip(5)
        .map(|t| t.id.clone())
        .collect();
    initial.transcript.turns.retain(|t| keep.contains(&t.id));
    initial
        .transcript
        .items
        .retain(|i| keep.contains(&i.turn_id));
    initial.transcript.has_more_before = true;
    let mut pane =
        tekes_agent_ui_consumer_check::new_readonly_child(37000, 37003..38000, scope.clone())
            .unwrap();
    pane.update(readonly_input(&scope, &initial, 1, false))
        .unwrap();
    let root = pane.ids().root;
    let viewport = pane.ids().viewport;
    let items = pane
        .content()
        .row_keys()
        .iter()
        .map(|k| pane.content().row_id(k).unwrap())
        .collect();
    let mut host = pane.into_host().unwrap();
    let (version, ()) = host
        .set_scroll_reading_document(viewport, "readonly-test", 0, items, |_| ())
        .unwrap();
    render(&mut host, &mut text);
    let point = host.get(viewport).unwrap().visible().unwrap().center();
    host.event(&ContentEvent::Scroll {
        position: point,
        delta: Point::new(0., 180.),
        precise: true,
        begin: true,
        end: true,
        momentum: false,
        time: 1.,
    });
    render(&mut host, &mut text);
    let item = readonly_anchor(&host, root);
    let y = readonly_y(&host, root, item);
    // Same-provider publication, but different header/paging geometry.
    let loading = readonly(&host, root)
        .prepare_update(readonly_input(&scope, &initial, 1, true))
        .unwrap();
    let version = tekes_agent_ui_consumer_check::publish_original_readonly(
        &mut host, root, &version, 1, &loading,
    )
    .unwrap();
    render(&mut host, &mut text);
    assert_eq!(readonly_anchor(&host, root), item);
    assert!((readonly_y(&host, root, item) - y).abs() <= 1.);
    let plan = readonly(&host, root)
        .prepare_update(readonly_input(&scope, &full, 2, false))
        .unwrap();
    let before = readonly(&host, root).content().row_keys().len();
    tekes_agent_ui_consumer_check::publish_original_readonly(&mut host, root, &version, 2, &plan)
        .unwrap();
    render(&mut host, &mut text);
    assert!(readonly(&host, root).content().row_keys().len() > before);
    assert_eq!(readonly_anchor(&host, root), item);
    assert!((readonly_y(&host, root, item) - y).abs() <= 1.);
    assert!(!readonly_scroll(&host, root).is_following_latest());
    assert!(!readonly(&host, root).content().feedback_enabled());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn readonly_preflight_rejects_plan_aba_and_foreign_sdk_token_without_any_publication() {
    let (runtime, _, _, full, _, mut text) = setup().await;
    let scope = RunScope::main("fixture", "readonly");
    let mut pane =
        ReadOnlyTranscript::standard(38000, 38003..39000, scope.clone(), Default::default())
            .unwrap();
    pane.update(readonly_input(&scope, &full, 1, false))
        .unwrap();
    let root = pane.ids().root;
    let viewport = pane.ids().viewport;
    let items: Vec<_> = pane
        .content()
        .row_keys()
        .iter()
        .map(|k| pane.content().row_id(k).unwrap())
        .collect();
    let mut host = pane.into_host().unwrap();
    let (version, ()) = host
        .set_scroll_reading_document(viewport, "original-readonly", 0, items.clone(), |_| ())
        .unwrap();
    render(&mut host, &mut text);
    let plan = readonly(&host, root)
        .prepare_update(readonly_input(&scope, &full, 2, false))
        .unwrap();
    let (fresh, ()) = host
        .set_scroll_reading_document(viewport, "replaced-readonly", 0, items, |_| ())
        .unwrap();
    let before = host
        .get(readonly(&host, root).ids().status)
        .unwrap()
        .component::<TextContent>()
        .unwrap()
        .text
        .clone();
    assert!(matches!(
        tekes_agent_ui_consumer_check::publish_original_readonly(
            &mut host, root, &version, 1, &plan
        ),
        Err(ReadOnlyReadingError::Reading(_))
    ));
    assert_eq!(
        readonly_scroll(&host, root).reading_document_version(),
        Some(&fresh)
    );
    assert_eq!(
        host.get(readonly(&host, root).ids().status)
            .unwrap()
            .component::<TextContent>()
            .unwrap()
            .text,
        before
    );
    host.get_mut(root)
        .unwrap()
        .component_mut::<ReadOnlyTranscript>()
        .unwrap()
        .update(readonly_input(&scope, &full, 1, true))
        .unwrap();
    host.get_mut(root)
        .unwrap()
        .component_mut::<ReadOnlyTranscript>()
        .unwrap()
        .update(readonly_input(&scope, &full, 1, false))
        .unwrap();
    assert!(matches!(
        tekes_agent_ui_consumer_check::publish_original_readonly(&mut host, root, &fresh, 1, &plan),
        Err(ReadOnlyReadingError::Agent(_))
    ));
    assert_eq!(
        readonly_scroll(&host, root).reading_document_version(),
        Some(&fresh)
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn readonly_plan_is_original_owner_only_and_formats_status_before_the_sdk_callback() {
    static FORMATS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    fn status(
        _: &tekes_agent_presentation::ConnectionPresentation,
        count: usize,
        _: bool,
    ) -> String {
        FORMATS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        format!("owned {count}")
    }
    let (runtime, _, _, full, _, mut text) = setup().await;
    let scope = RunScope::main("fixture", "bounded-owner");
    let labels = ReadOnlyTranscriptLabels {
        information: status,
        ..Default::default()
    };
    let mut pane =
        ReadOnlyTranscript::standard(39000, 39003..40000, scope.clone(), labels.clone()).unwrap();
    pane.update(readonly_input(&scope, &full, 1, false))
        .unwrap();
    let root = pane.ids().root;
    let viewport = pane.ids().viewport;
    let items: Vec<_> = pane
        .content()
        .row_keys()
        .iter()
        .map(|k| pane.content().row_id(k).unwrap())
        .collect();
    let mut host = pane.into_host().unwrap();
    let (version, ()) = host
        .set_scroll_reading_document(viewport, "owner-test", 0, items.clone(), |_| ())
        .unwrap();
    render(&mut host, &mut text);
    let plan = readonly(&host, root)
        .prepare_update(readonly_input(&scope, &full, 2, false))
        .unwrap();
    let mut other =
        ReadOnlyTranscript::standard(39000, 39003..40000, scope.clone(), labels).unwrap();
    other
        .update(readonly_input(&scope, &full, 1, false))
        .unwrap();
    let mut other = other.into_host().unwrap();
    let (other_version, ()) = other
        .set_scroll_reading_document(viewport, "owner-test", 0, items, |_| ())
        .unwrap();
    assert!(matches!(
        tekes_agent_ui_consumer_check::publish_original_readonly(
            &mut other,
            root,
            &other_version,
            1,
            &plan
        ),
        Err(ReadOnlyReadingError::Agent(_))
    ));
    assert_eq!(
        readonly_scroll(&other, root).reading_document_version(),
        Some(&other_version)
    );
    let before = FORMATS.load(std::sync::atomic::Ordering::SeqCst);
    tekes_agent_ui_consumer_check::publish_original_readonly(&mut host, root, &version, 1, &plan)
        .unwrap();
    assert_eq!(
        FORMATS.load(std::sync::atomic::Ordering::SeqCst),
        before,
        "host formatter must not run in the original SDK publication callback"
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn preview_of_original_rail_target_preserves_marked_text_and_expires_on_publication_or_collapse()
 {
    let (runtime, mut host, root, snapshot, version, _, mut text) = rail_setup().await;
    let target = rail(&host, root)
        .preview_target(&snapshot.transcript.turns[0].id)
        .unwrap();
    let preview = run(&host, root).turn_preview(&target).unwrap().clone();
    assert_eq!(preview.input, target.input());
    let editor = run(&host, root).ids().composer.editor;
    host.focus(Some(editor), true);
    host.event(&ContentEvent::Preedit {
        text: "预览保持中文组字".into(),
        selection: 1..3,
        clauses: vec![],
        replacement: None,
    });
    let baseline = baseline(&host, root);
    assert_eq!(
        tekes_agent_ui_consumer_check::turn_preview(run(&host, root), &target).unwrap(),
        &preview
    );
    assert_eq!(host.focused(), Some(editor));
    assert_eq!(baseline, self::baseline(&host, root));
    assert!(rail_intents(&mut host, root).is_empty());
    host.event(&ContentEvent::Unmark);
    let next = append(&snapshot);
    let plan = prepare(run(&host, root), &next).unwrap();
    publish_plan(&mut host, root, &version, &plan).unwrap();
    assert!(run(&host, root).turn_preview(&target).is_err());
    render(&mut host, &mut text);
    let current = rail(&host, root).preview_target(target.turn_id()).unwrap();
    assert!(run(&host, root).turn_preview(&current).is_ok());
    let collapsed = Rect::new(0., 0., 0., 700.);
    host.render(
        &mut Scene::new(),
        &mut text,
        collapsed,
        collapsed,
        Theme::LIGHT.into(),
    )
    .unwrap();
    assert!(run(&host, root).turn_preview(&current).is_err());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn composer_notice_height_changes_preserve_upward_reading_and_marked_editor() {
    let (runtime, mut host, root, _, _, _, mut text) = rail_setup().await;
    wheel(&mut host, root, 420.);
    render(&mut host, &mut text);
    let item = anchor(&host, root);
    let viewport = run(&host, root).ids().viewport;
    let y = host.get(item).unwrap().frame().min_y()
        - host.get(viewport).unwrap().visible().unwrap().min_y();
    let editor = run(&host, root).ids().composer.editor;
    host.focus(Some(editor), true);
    host.event(&ContentEvent::Preedit {
        text: "提示中文组字".into(),
        selection: 1..3,
        clauses: vec![],
        replacement: None,
    });
    let before = baseline(&host, root);
    let connection_epoch = run(&host, root)
        .displayed_snapshot()
        .unwrap()
        .connection_epoch;
    let composer = run_mut(&mut host, root).composer_mut();
    composer
        .install_notice(0xfff0_0002, Default::default())
        .unwrap();
    let destination = composer.destination().unwrap().clone();
    for (revision, notice) in [
        (
            1,
            Some(ComposerNoticeState {
                text: "需要刷新连接目录。".repeat(12),
                can_refresh: true,
                refreshing: false,
            }),
        ),
        (2, None),
    ] {
        run_mut(&mut host, root)
            .composer_mut()
            .update_notice(&ComposerNoticeInput {
                connection_epoch,
                revision,
                destination: destination.clone(),
                notice,
                enabled: true,
            })
            .unwrap();
        render(&mut host, &mut text);
        assert_eq!(host.focused(), Some(editor));
        assert_eq!(baseline(&host, root), before);
        let next = host.get(item).unwrap().frame().min_y()
            - host.get(viewport).unwrap().visible().unwrap().min_y();
        assert!((next - y).abs() <= 1.);
        assert!(!scroll(&host, root).is_following_latest());
    }
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn expanded_tool_growth_and_reflow_preserve_reading_and_explicit_collapse_can_focus() {
    use tekes_ui_controls::{ScrollRevealOptions, ScrollRevealPlacement, extended::Axis};
    let fixture = tekes_agent_ui_demo::fixture::Fixture::default().with_reading_details();
    let (runtime, mut host, root, mut snapshot, version, mut text) =
        setup_with_fixture(fixture).await;
    assert!(!snapshot.transcript.tools.is_empty());
    let tool_key = TranscriptContentRowKey::Tool(snapshot.transcript.tools[0].id.clone());
    let tool_row = run(&host, root)
        .transcript()
        .unwrap()
        .row_id(&tool_key)
        .unwrap();
    let tool = host
        .get(tool_row)
        .unwrap()
        .component::<ToolActivity>()
        .unwrap()
        .content_ids()
        .unwrap();
    let reasoning = rows(&host, root)
        .iter()
        .filter_map(|key| {
            let id = run(&host, root).transcript().unwrap().row_id(key).unwrap();
            host.get(id)
                .unwrap()
                .component::<OutputMessage>()
                .and_then(|output| {
                    output
                        .content_ids()
                        .into_iter()
                        .find(|id| id.kind == OutputContentKind::Reasoning)
                })
        })
        .next()
        .unwrap();
    for button in [tool.disclosure.unwrap(), reasoning.disclosure.unwrap()] {
        host.accessibility_action(button, None, AccessibilityAction::Press);
        let intents = tekes_agent_ui_consumer_check::drain_local_run_actions(&mut host, root);
        assert!(
            intents.is_empty(),
            "disclosure must not submit or mutate a session"
        );
    }
    render(&mut host, &mut text);
    assert!(host.get(tool.body).is_some());
    assert!(host.get(reasoning.body).is_some());
    let original_tool = host
        .get(tool.body)
        .unwrap()
        .component::<TextFieldContent>()
        .unwrap() as *const _;
    let tool_height = host.get(tool_row).unwrap().frame().size.height;
    let target = rows(&host, root)
        .iter()
        .rev()
        .filter_map(|key| {
            let id = run(&host, root).transcript().unwrap().row_id(key).unwrap();
            (host.get(id).unwrap().component::<InputMessage>().is_some()).then_some(id)
        })
        .nth(2)
        .unwrap();
    let viewport = run(&host, root).ids().viewport;
    host.reveal_scroll_reading_item(
        viewport,
        &version,
        target,
        ScrollRevealOptions {
            placement: ScrollRevealPlacement::Start(Axis::Vertical),
            animated: false,
            ..Default::default()
        },
        &mut text,
    )
    .unwrap();
    render(&mut host, &mut text);
    assert_eq!(anchor(&host, root), target);
    let relative = |host: &ContentHost| {
        host.get(target).unwrap().frame().min_y()
            - host.get(viewport).unwrap().visible().unwrap().min_y()
    };
    let y = relative(&host);
    let editor = run(&host, root).ids().composer.editor;
    host.focus(Some(editor), true);
    host.event(&ContentEvent::Preedit {
        text: "工具回传和重排期间中文组字".into(),
        selection: 2..5,
        clauses: vec![],
        replacement: None,
    });
    let before = baseline(&host, root);
    snapshot.revision += 1;
    snapshot.transcript.tools[0].result =
        Some(serde_json::json!({"text":"额外的已回传工具内容，保持原调用位置。\n".repeat(80)}));
    let plan = prepare(run(&host, root), &snapshot).unwrap();
    publish_plan(&mut host, root, &version, &plan).unwrap();
    render(&mut host, &mut text);
    assert!(host.get(tool_row).unwrap().frame().size.height > tool_height);
    assert!(
        host.get(tool_row).unwrap().frame().max_y()
            < host.get(viewport).unwrap().visible().unwrap().min_y(),
        "growing tool must be above the reading viewport"
    );
    for (width, scale) in [(600., 1.), (900., 1.4), (760., 0.85), (760., 1.)] {
        text.1 = scale;
        let frame = Rect::new(0., 0., width, 650.);
        host.render(
            &mut Scene::new(),
            &mut text,
            frame,
            frame,
            Theme::LIGHT.into(),
        )
        .unwrap();
        assert_eq!(anchor(&host, root), target);
        assert!(
            (relative(&host) - y).abs() <= 1.,
            "reflow moved original reading item"
        );
        assert!(!scroll(&host, root).is_following_latest());
        assert_eq!(host.focused(), Some(editor));
        assert_eq!(baseline(&host, root), before);
        assert_eq!(
            host.get(tool.body)
                .unwrap()
                .component::<TextFieldContent>()
                .unwrap() as *const _,
            original_tool
        );
    }
    host.event(&ContentEvent::Unmark);
    for button in [tool.disclosure.unwrap(), reasoning.disclosure.unwrap()] {
        host.accessibility_action(button, None, AccessibilityAction::Press);
        for intent in tekes_agent_ui_consumer_check::drain_local_run_actions(&mut host, root) {
            match intent {
                RunPortIntent::Composer(ComposerIntent::Edit { destination, text }) => {
                    assert_eq!(
                        Some(&destination),
                        run(&host, root).composer().destination()
                    );
                    assert_eq!(text, run(&host, root).composer().editor().input.text);
                }
                other => panic!("collapse may only flush the just-unmarked draft edit: {other:?}"),
            }
        }
    }
    render(&mut host, &mut text);
    assert!(host.get(tool.body).is_none() && host.get(reasoning.body).is_none());
    // Explicit AX activation focuses/reveals an offscreen disclosure through the
    // SDK. That deliberate navigation must not be mislabelled passive reflow.
    assert_eq!(host.focused(), reasoning.disclosure);
    assert!(
        host.get(reasoning.disclosure.unwrap())
            .unwrap()
            .visible()
            .is_some()
    );
    assert!(!scroll(&host, root).is_following_latest());
    assert!(snapshot.submissions.is_empty());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn explicit_clock_tick_uses_original_reading_transaction_and_keeps_preedit() {
    let (_runtime, mut host, id, mut snapshot, mut version, mut text) = setup().await;
    let turn_id = snapshot.transcript.turns.last().unwrap().id.clone();
    snapshot.revision += 1;
    let turn = snapshot.transcript.turns.last_mut().unwrap();
    turn.status = "running".into();
    turn.ended_at = None;
    turn.started_at = Some(10000.);
    for item in &mut snapshot.transcript.items {
        if item.turn_id == turn_id && item.kind == TranscriptItemKind::Assistant {
            item.is_final = false;
        }
    }
    // Retained open journals do not grant live ownership to an interrupted or
    // noncontinuable destination, even when stale submit/cancel flags remain.
    assert!(
        TranscriptFootnoteTiming::for_run(&snapshot, 1, Some(75000.))
            .live_turn_id
            .is_some()
    );
    for phase in [
        tekes_agent_presentation::RunPhase::Interrupted,
        tekes_agent_presentation::RunPhase::Archived,
        tekes_agent_presentation::RunPhase::Unavailable,
        tekes_agent_presentation::RunPhase::Loading,
    ] {
        let mut unavailable = snapshot.clone();
        unavailable.run.as_mut().unwrap().phase = phase;
        assert!(
            TranscriptFootnoteTiming::for_run(&unavailable, 1, Some(75000.))
                .live_turn_id
                .is_none()
        );
    }
    for availability in [
        tekes_agent_presentation::ChildComposerAvailability::Loading,
        tekes_agent_presentation::ChildComposerAvailability::OneShot,
        tekes_agent_presentation::ChildComposerAvailability::ParentUnavailable,
        tekes_agent_presentation::ChildComposerAvailability::Unavailable,
    ] {
        let mut unavailable = snapshot.clone();
        unavailable.child_availability = Some(availability);
        assert!(
            TranscriptFootnoteTiming::for_run(&unavailable, 1, Some(75000.))
                .live_turn_id
                .is_none()
        );
    }
    run_mut(&mut host, id)
        .set_footnote_clock(Some(75000.))
        .unwrap();
    let plan = prepare(run(&host, id), &snapshot).unwrap();
    version =
        publish_run_reading_update(&mut host, id, &version, version.revision() + 1, &plan).unwrap();
    render(&mut host, &mut text);
    let footer = run(&host, id)
        .transcript()
        .unwrap()
        .row_id(&TranscriptContentRowKey::TurnFootnote(turn_id))
        .unwrap();
    let info = host
        .get(footer)
        .unwrap()
        .component::<TurnFootnote>()
        .unwrap()
        .information_id()
        .unwrap();
    assert_eq!(
        host.get(info)
            .unwrap()
            .component::<TextContent>()
            .unwrap()
            .text,
        "1m 5s"
    );
    let editor = run(&host, id).ids().composer.editor;
    host.focus(Some(editor), true);
    host.event(&ContentEvent::Preedit {
        text: "clock tick 期间中文组字".into(),
        selection: 1..3,
        replacement: None,
        clauses: vec![],
    });
    let baseline = run(&host, id).composer().editor().input.external_baseline();
    run_mut(&mut host, id)
        .set_footnote_clock(Some(80000.))
        .unwrap();
    let stale = prepare(run(&host, id), &snapshot).unwrap();
    run_mut(&mut host, id)
        .set_footnote_clock(Some(85000.))
        .unwrap();
    assert!(matches!(
        publish_run_reading_update(&mut host, id, &version, version.revision() + 1, &stale),
        Err(RunReadingError::Agent(_))
    ));
    assert_eq!(
        host.get(info)
            .unwrap()
            .component::<TextContent>()
            .unwrap()
            .text,
        "1m 5s",
        "staging a clock never edits controls outside the transaction"
    );
    let plan = prepare(run(&host, id), &snapshot).unwrap();
    let next =
        publish_run_reading_update(&mut host, id, &version, version.revision() + 1, &plan).unwrap();
    render(&mut host, &mut text);
    assert_eq!(next.revision(), version.revision() + 1);
    assert_eq!(host.focused(), Some(editor));
    assert_eq!(
        run(&host, id).composer().editor().input.external_baseline(),
        baseline
    );
    assert_eq!(
        host.get(footer)
            .unwrap()
            .component::<TurnFootnote>()
            .unwrap()
            .information_id(),
        Some(info)
    );
    assert_eq!(
        host.get(info)
            .unwrap()
            .component::<TextContent>()
            .unwrap()
            .text,
        "1m 15s"
    );
    assert!(
        run_mut(&mut host, id)
            .set_footnote_clock(Some(f64::NAN))
            .is_err()
    );
}
