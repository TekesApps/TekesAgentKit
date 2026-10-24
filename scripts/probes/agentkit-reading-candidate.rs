//! Candidate-only receiver test. Formal TranscriptContent + Composer; no copied
//! reading controller, pixel compensation, renderer or presentation reducer.
#![cfg(target_os = "macos")]
use std::{cell::Cell, rc::Rc};
use tekes_agent_components_ui::*;
use tekes_ui_controls::{
    ButtonTextAlign, ButtonTextRenderer, ReadingDocumentVersion, ScrollArea, TextSurfaceHost,
    TextSurfaceTextRenderer,
    composition::*,
    extended::{Axis, Stack},
};
use tekes_ui_scene::{Point, Rect, Scene, Size};
use tekes_ui_text::{
    Paragraph,
    coretext::{CoreTextFonts, CoreTextShaper},
    model::ShapeOptions,
    style::TextStyle,
    traits::Shaper,
};
use tekes_ui_tokens::{Rgba, Spacing, Theme, proposal};
const ROOT: ComponentId = ComponentId(1);
const SCROLL: ComponentId = ComponentId(2);
const LATEST: ComponentId = ComponentId(3);
fn composer_ids() -> ComposerIds {
    ComposerIds::new(4_000_000_u64 << 32)
}
struct Text(CoreTextShaper);
impl TextSurfaceHost for Text {
    fn shape_surface(
        &mut self,
        text: &str,
        role: proposal::TextRole,
        width: Option<f32>,
    ) -> Rc<Paragraph> {
        Rc::new(self.0.shape(
            text,
            &TextStyle::system(role.style().size),
            &ShapeOptions {
                width: width.map(f64::from),
                line_spacing: 0.,
            },
        ))
    }
}
impl TextSurfaceTextRenderer for Text {
    fn push_visible(&mut self, _: &mut Scene, _: &Paragraph, _: Point, _: Rgba, _: Rect) {}
}
impl ButtonTextRenderer for Text {
    fn measure(&mut self, text: &str, role: proposal::TextRole, width: f32) -> Size {
        let p = self.shape_surface(text, role, Some(width));
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
fn scope(name: &str) -> RunScope {
    RunScope::main("reading-candidate", name)
}
fn document(name: &str, start: i64, end: i64) -> TranscriptSnapshot {
    let items = (start..end)
        .map(|i| TranscriptItem {
            id: format!("{name}-{i}"),
            record_id: None,
            reconciliation_id: None,
            turn_id: "history".into(),
            kind: if i % 2 == 0 {
                TranscriptItemKind::User
            } else {
                TranscriptItemKind::Assistant
            },
            text: format!("消息 {i}：中文历史与流式内容，保留共享排版。\nSecond line {i}."),
            reasoning: String::new(),
            attachments: vec![],
            content_blocks: None,
            is_visible: true,
            is_final: false,
            is_provisional: false,
            reasoning_ended: true,
            source_sequence: Some(i),
            source_position: Some(TranscriptSourcePosition::Journal {
                sequence: i,
                content_index: None,
            }),
            time: None,
            metadata: Default::default(),
        })
        .collect();
    TranscriptSnapshot {
        items,
        ..Default::default()
    }
}
fn content(namespace: u32, name: &str) -> TranscriptContent {
    TranscriptContent::new(
        namespace,
        namespace + 1..namespace + 900,
        scope(name),
        Default::default(),
    )
    .unwrap()
}
fn transcript(host: &ContentHost, id: ComponentId) -> &TranscriptContent {
    host.get(id)
        .unwrap()
        .component::<TranscriptContent>()
        .unwrap()
}
fn transcript_mut(host: &mut ContentHost, id: ComponentId) -> &mut TranscriptContent {
    host.get_mut(id)
        .unwrap()
        .component_mut::<TranscriptContent>()
        .unwrap()
}
fn scroll(host: &ContentHost) -> &ScrollContent {
    host.get(SCROLL)
        .unwrap()
        .component::<ScrollContent>()
        .unwrap()
}
fn render(host: &mut ContentHost, text: &mut Text) {
    let frame = Rect::new(0., 0., 680., 520.);
    host.render(&mut Scene::new(), text, frame, frame, Theme::LIGHT.into())
        .unwrap();
}
fn wheel(host: &mut ContentHost, delta: f32) {
    let point = host.get(SCROLL).unwrap().frame().center();
    host.event(&ContentEvent::Scroll {
        position: point,
        delta: Point::new(0., delta),
        precise: true,
        begin: true,
        end: true,
        momentum: false,
        time: 1.,
    });
}
fn anchor(host: &ContentHost) -> ComponentId {
    match scroll(host).reading_result().unwrap().anchor {
        ContentReadingAnchorUpdate::Preserved(id)
        | ContentReadingAnchorUpdate::Fallback(Some(id)) => id,
        ref other => panic!("item anchor expected: {other:?}"),
    }
}
fn update(
    host: &mut ContentHost,
    id: ComponentId,
    name: &str,
    expected: &ReadingDocumentVersion,
    revision: u64,
    snapshot: &TranscriptSnapshot,
) -> ReadingDocumentVersion {
    let plan = transcript(host, id)
        .prepare_update(&scope(name), revision, 1, snapshot, false)
        .unwrap();
    host.update_scroll_reading_document(SCROLL, expected, revision, plan.item_ids(), |host| {
        transcript_mut(host, id)
            .apply_prepared_update(&plan)
            .unwrap()
    })
    .unwrap()
    .0
}
async fn setup() -> (
    std::sync::Arc<tekes_agent_session::SessionRuntime>,
    ContentHost,
    ComponentId,
    Text,
) {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut composer = Composer::new(composer_ids());
    composer
        .update(&controller.snapshot(), Default::default())
        .unwrap();
    let view = content(1000, "A");
    let id = view.id();
    let host = ContentHost::new(Content::new(
        ROOT,
        StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: Spacing::Regular.value(),
            },
            flexible_child: Some(SCROLL),
            children: vec![
                Content::new(
                    SCROLL,
                    ScrollContent::new(ScrollArea::default(), Content::new(id, view)),
                ),
                Content::new(LATEST, ButtonContent::new("回到最新")).preserving_focus(),
                Content::new(composer_ids().root, composer),
            ],
        },
    ))
    .unwrap();
    (
        runtime,
        host,
        id,
        Text(CoreTextShaper::new(CoreTextFonts::new())),
    )
}
fn attach(
    host: &mut ContentHost,
    id: ComponentId,
    name: &str,
    revision: u64,
    snapshot: &TranscriptSnapshot,
) -> ReadingDocumentVersion {
    let plan = transcript(host, id)
        .prepare_update(&scope(name), revision, 1, snapshot, false)
        .unwrap();
    host.set_scroll_reading_document(
        SCROLL,
        ComposerDestination::run(scope(name)).storage_key(),
        revision,
        plan.item_ids(),
        |host| {
            transcript_mut(host, id)
                .apply_prepared_update(&plan)
                .unwrap()
        },
    )
    .unwrap()
    .0
}
fn switch(
    host: &mut ContentHost,
    old: ComponentId,
    new: Content,
    name: &str,
    data: &TranscriptSnapshot,
) -> (Content, ReadingDocumentVersion) {
    let id = new.id();
    let plan = new
        .component::<TranscriptContent>()
        .unwrap()
        .prepare_update(&scope(name), 1, 1, data, false)
        .unwrap();
    let (version, saved) = host
        .set_scroll_reading_document(
            SCROLL,
            ComposerDestination::run(scope(name)).storage_key(),
            1,
            plan.item_ids(),
            |host| {
                let saved = host.remove(old).unwrap();
                host.insert(SCROLL, new).unwrap();
                transcript_mut(host, id)
                    .apply_prepared_update(&plan)
                    .unwrap();
                saved
            },
        )
        .unwrap();
    (saved, version)
}
#[tokio::test]
async fn formal_transcript_append_and_prepend_preserve_the_read_item_without_pixel_compensation() {
    let (runtime, mut host, id, mut text) = setup().await;
    let first = document("A", 20, 40);
    let v1 = attach(&mut host, id, "A", 1, &first);
    render(&mut host, &mut text);
    wheel(&mut host, 420.);
    render(&mut host, &mut text);
    let item = anchor(&host);
    let y = host.get(item).unwrap().frame().min_y();
    assert!(scroll(&host).scroll.offset.y > 0.);
    assert!(!scroll(&host).is_following_latest());
    let appended = document("A", 20, 47);
    let v2 = update(&mut host, id, "A", &v1, 2, &appended);
    render(&mut host, &mut text);
    assert_eq!(anchor(&host), item);
    assert!((host.get(item).unwrap().frame().min_y() - y).abs() <= 1.);
    let paged = document("A", 0, 47);
    let v3 = update(&mut host, id, "A", &v2, 3, &paged);
    render(&mut host, &mut text);
    assert_eq!(anchor(&host), item);
    assert!((host.get(item).unwrap().frame().min_y() - y).abs() <= 1.);
    let called = Cell::new(false);
    assert!(
        host.update_scroll_reading_document(SCROLL, &v1, 4, vec![], |_| called.set(true))
            .is_err()
    );
    assert!(!called.get());
    assert_eq!(scroll(&host).reading_document_version(), Some(&v3));
    let keys = transcript(&host, id).row_keys().to_vec();
    let mut bad = paged.clone();
    bad.items.push(bad.items[0].clone());
    assert!(
        transcript(&host, id)
            .prepare_update(&scope("A"), 4, 1, &bad, false)
            .is_err()
    );
    assert_eq!(transcript(&host, id).row_keys(), keys);
    assert_eq!(scroll(&host).reading_document_version(), Some(&v3));
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn document_switch_restores_a_and_old_attachment_never_runs_a_late_page_callback() {
    let (runtime, mut host, a, mut text) = setup().await;
    let first = document("A", 10, 40);
    let old_a = attach(&mut host, a, "A", 1, &first);
    render(&mut host, &mut text);
    wheel(&mut host, 460.);
    render(&mut host, &mut text);
    let item = anchor(&host);
    let y = host.get(item).unwrap().frame().min_y();
    let b = content(3000, "B");
    let b_id = b.id();
    let b_data = document("B", 0, 30);
    let (saved_a, b_version) = switch(&mut host, a, Content::new(b_id, b), "B", &b_data);
    render(&mut host, &mut text);
    wheel(&mut host, 180.);
    render(&mut host, &mut text);
    let (saved_b, returned) = switch(&mut host, b_id, saved_a, "A", &first);
    render(&mut host, &mut text);
    assert_ne!(old_a, returned);
    assert_eq!(anchor(&host), item);
    assert!((host.get(item).unwrap().frame().min_y() - y).abs() <= 1.);
    let called = Cell::new(false);
    assert!(
        host.update_scroll_reading_document(SCROLL, &old_a, 2, vec![], |_| called.set(true))
            .is_err()
    );
    assert!(!called.get());
    assert!(
        host.update_scroll_reading_document(SCROLL, &b_version, 2, vec![], |_| called.set(true))
            .is_err()
    );
    assert!(!called.get());
    drop(saved_b);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn return_to_latest_preserves_the_real_composer_focus_and_preedit_for_pointer_and_ax() {
    let (runtime, mut host, id, mut text) = setup().await;
    let first = document("A", 0, 30);
    let _version = attach(&mut host, id, "A", 1, &first);
    render(&mut host, &mut text);
    wheel(&mut host, 260.);
    render(&mut host, &mut text);
    assert!(!scroll(&host).is_following_latest());
    host.set_action_events(true);
    host.focus(Some(composer_ids().editor), true);
    host.event(&ContentEvent::Preedit {
        text: "中文组字".into(),
        selection: 0..4,
        clauses: vec![],
        replacement: None,
    });
    assert!(
        host.get(composer_ids().root)
            .unwrap()
            .component::<Composer>()
            .unwrap()
            .is_composing()
    );
    let before = host
        .get(composer_ids().root)
        .unwrap()
        .component::<Composer>()
        .unwrap()
        .editor()
        .input
        .text
        .clone();
    host.drain_actions();
    let point = host.get(LATEST).unwrap().frame().center();
    host.event(&ContentEvent::PointerDown(point));
    host.event(&ContentEvent::PointerUp(point));
    let mut activated = 0;
    for action in host.drain_actions() {
        if action.target == LATEST && action.kind == ContentActionKind::Activate {
            activated += 1;
            host.get_mut(SCROLL)
                .unwrap()
                .component_mut::<ScrollContent>()
                .unwrap()
                .follow_latest();
        }
    }
    assert_eq!(activated, 1);
    render(&mut host, &mut text);
    assert!(scroll(&host).is_following_latest());
    assert_eq!(host.focused(), Some(composer_ids().editor));
    wheel(&mut host, -260.);
    render(&mut host, &mut text);
    assert!(!scroll(&host).is_following_latest());
    host.accessibility_action(LATEST, None, AccessibilityAction::Press);
    let mut activated = 0;
    for action in host.drain_actions() {
        if action.target == LATEST && action.kind == ContentActionKind::Activate {
            activated += 1;
            host.get_mut(SCROLL)
                .unwrap()
                .component_mut::<ScrollContent>()
                .unwrap()
                .follow_latest();
        }
    }
    assert_eq!(activated, 1);
    render(&mut host, &mut text);
    assert!(scroll(&host).is_following_latest());
    assert_eq!(host.focused(), Some(composer_ids().editor));
    let composer = host
        .get(composer_ids().root)
        .unwrap()
        .component::<Composer>()
        .unwrap();
    assert!(composer.is_composing());
    assert_eq!(composer.editor().input.text, before);
    runtime.stop().await.unwrap();
}

/// Reproduces the documented upstream boundary, not an acceptance of this
/// behavior for RunPort. Keep fallible editor updates outside reading callbacks
/// until TekesUI #26 provides a supported preflight/rejection contract.
#[tokio::test]
async fn issue26_editor_composition_conflict_does_not_reject_the_reading_version() {
    use tekes_ui_controls::{
        DraftRevision, ExternalSelection, ExternalUndo, ExternalValueConflict, ExternalValueResult,
    };
    let (runtime, mut host, id, mut text) = setup().await;
    let first = document("A", 0, 20);
    let version = attach(&mut host, id, "A", 1, &first);
    render(&mut host, &mut text);
    let editor = &mut host
        .get_mut(composer_ids().editor)
        .unwrap()
        .component_mut::<AgentText>()
        .unwrap()
        .field
        .surface;
    assert!(editor.input.preedit("中文组字", 0..4, None));
    let baseline = editor.input.external_baseline();
    let current = editor.input.external_value().unwrap();
    let value = DraftRevision {
        identity: current.identity.clone(),
        version: current.version + 1,
        payload: "另一份权威草稿".into(),
    };
    let text_before = editor.input.text.clone();
    let rows = transcript(&host, id).row_keys().to_vec();
    let items = transcript(&host, id)
        .prepare_update(&scope("A"), 2, 1, &first, false)
        .unwrap()
        .item_ids();
    let (advanced, conflict) = host
        .update_scroll_reading_document(SCROLL, &version, 2, items, |host| {
            host.get_mut(composer_ids().editor)
                .unwrap()
                .component_mut::<AgentText>()
                .unwrap()
                .field
                .surface
                .apply_external_value(
                    &baseline,
                    &value,
                    ExternalSelection::Preserve,
                    ExternalUndo::Preserve,
                )
        })
        .unwrap();
    assert_eq!(
        conflict,
        ExternalValueResult::Conflict(ExternalValueConflict::Composition)
    );
    assert_eq!(scroll(&host).reading_document_version(), Some(&advanced));
    assert_ne!(
        advanced, version,
        "A callback conflict is not a reading transaction rejection"
    );
    let editor = host.text_surface(composer_ids().editor).unwrap();
    assert_eq!(editor.input.external_baseline(), baseline);
    assert_eq!(editor.input.text, text_before);
    assert_eq!(transcript(&host, id).row_keys(), rows);
    runtime.stop().await.unwrap();
}
