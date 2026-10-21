//! Published SDK RunView integration; CoreText layout, not native/GPU acceptance.
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
fn paging(run: &RunView, snapshot: &RunSnapshot) -> TranscriptPagingFacts {
    TranscriptPagingFacts {
        scope: run.scope().clone(),
        supported: true,
        has_more_before: snapshot.transcript.has_more_before,
        is_stale: snapshot.transcript.is_stale,
        source_ready: true,
        read: TranscriptPageRead::Idle,
    }
}
fn prepare<'a>(run: &RunView, snapshot: &'a RunSnapshot) -> Result<RunViewUpdate<'a>, Failure> {
    run.prepare_update(
        snapshot,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(run, snapshot),
        true,
    )
    .map_err(Failure::Agent)
}
fn run(host: &ContentHost, id: ComponentId) -> &RunView {
    host.get(id).unwrap().component::<RunView>().unwrap()
}
fn run_mut(host: &mut ContentHost, id: ComponentId) -> &mut RunView {
    host.get_mut(id)
        .unwrap()
        .component_mut::<RunView>()
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
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut snapshot = (*controller.snapshot()).clone();
    snapshot.draft.text = "原草稿 中文".into();
    let mut view = RunView::standard(2100, 2104..2500, RunScope::main("fixture", "a")).unwrap();
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
    plan: &RunViewUpdate<'_>,
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
        Err(Failure::Agent("stale or foreign RunView update plan"))
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
    let mut other = RunView::standard(5100, 5104..5500, RunScope::main("fixture", "a")).unwrap();
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
        Err(Failure::Agent("stale or foreign RunView update plan"))
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
            .filter(|intent| matches!(intent, RunViewIntent::Composer(ComposerIntent::Send { .. })))
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
            .any(|intent| matches!(intent, RunViewIntent::Composer(ComposerIntent::Send { .. })))
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
        Err(Failure::Agent("stale or foreign RunView update plan"))
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
        Err(Failure::Agent("stale or foreign RunView update plan"))
    );
    let plan = prepare(run(&host, root), &next).unwrap();
    run_mut(&mut host, root)
        .update_actionable(first.revision, first.connection_epoch, None, false)
        .unwrap();
    assert_eq!(
        publish_plan(&mut host, root, &version, &plan),
        Err(Failure::Agent("stale or foreign RunView update plan"))
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
