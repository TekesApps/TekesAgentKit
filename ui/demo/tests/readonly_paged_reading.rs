//! Read-only bounded history reading through the original SDK scroll transaction.
#![cfg(target_os = "macos")]

use std::{collections::BTreeSet, rc::Rc, sync::Arc, time::Duration};

use tekes_agent_components_ui::{ReadOnlyTranscript, TranscriptContent, TranscriptContentRowKey};
use tekes_agent_presentation::{ChildHistoryLayout, ComposerDestination, RunScope};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{ChildPaneBinding, RunBinding};
use tekes_agent_ui_demo::{
    component_demo::{ComponentDemo, DemoCommand},
    fixture::Fixture,
};
use tekes_ui_controls::{
    ButtonTextAlign, ButtonTextRenderer, TextSurfaceHost, TextSurfaceTextRenderer,
    composition::{
        AccessibilityAction, ComponentId, ContentEvent, ContentReadingAnchorUpdate, ScrollContent,
    },
};
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
        text: &str,
        role: proposal::TextRole,
        width: Option<f32>,
    ) -> Rc<Paragraph> {
        Rc::new(self.0.shape(
            text,
            &TextStyle::system(role.style().size * self.1),
            &ShapeOptions {
                width: width.map(f64::from),
                line_spacing: 0.,
            },
        ))
    }
}

impl ButtonTextRenderer for Text {
    fn measure(&mut self, text: &str, role: proposal::TextRole, width: f32) -> Size {
        let paragraph = self.shape_surface(text, role, Some(width));
        Size::new(paragraph.width as f32, paragraph.height as f32)
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

fn render(view: &mut ComponentDemo, text: &mut Text) {
    let frame = Rect::new(0., 0., 1180., 820.);
    view.render(&mut Scene::new(), text, frame, frame, Theme::LIGHT.into())
        .unwrap();
}

fn pane(view: &ComponentDemo) -> &ReadOnlyTranscript {
    view.tree
        .get(
            view.bounded_child_view_id()
                .expect("mounted read-only pane"),
        )
        .unwrap()
        .component::<ReadOnlyTranscript>()
        .unwrap()
}

fn scroll(view: &ComponentDemo) -> &ScrollContent {
    view.tree
        .get(pane(view).ids().viewport)
        .unwrap()
        .component::<ScrollContent>()
        .unwrap()
}

fn anchor(view: &ComponentDemo) -> ComponentId {
    match scroll(view).reading_result().unwrap().anchor {
        ContentReadingAnchorUpdate::Preserved(id)
        | ContentReadingAnchorUpdate::Fallback(Some(id)) => id,
        ref other => panic!("expected read-only message reading anchor, got {other:?}"),
    }
}

fn relative_y(view: &ComponentDemo, item: ComponentId) -> f32 {
    view.tree.get(item).unwrap().frame().min_y()
        - view
            .tree
            .get(pane(view).ids().viewport)
            .unwrap()
            .visible()
            .unwrap()
            .min_y()
}

fn message_rows(view: &ComponentDemo) -> Vec<(String, ComponentId)> {
    let root = view.bounded_child_content_id().unwrap();
    let content = view
        .tree
        .get(root)
        .unwrap()
        .component::<TranscriptContent>()
        .unwrap();
    content
        .row_keys()
        .iter()
        .filter_map(|key| match key {
            TranscriptContentRowKey::Message(identity) => {
                let id = content.row_id(key)?;
                Some((format!("{identity:?}"), id))
            }
            _ => None,
        })
        .collect()
}

fn message_item_ids(view: &ComponentDemo) -> BTreeSet<String> {
    message_rows(view)
        .into_iter()
        .map(|(identity, _)| identity)
        .collect()
}

fn scroll_by(view: &mut ComponentDemo, delta_y: f32, time: f64) {
    let viewport = pane(view).ids().viewport;
    let position = view
        .tree
        .get(viewport)
        .unwrap()
        .visible()
        .expect("rendered read-only viewport")
        .center();
    view.tree.event(&ContentEvent::Scroll {
        position,
        delta: Point::new(0., delta_y),
        precise: true,
        begin: true,
        end: true,
        momentum: false,
        time,
    });
}

fn read_command(
    view: &mut ComponentDemo,
    button: ComponentId,
    source: &Arc<ChildPaneBinding>,
    parent_scope: &RunScope,
) -> (bool, tekes_agent_components_ui::ReadOnlyTranscriptIntent) {
    let commands = view
        .accessibility_action(button, None, AccessibilityAction::Press)
        .unwrap();
    let [
        DemoCommand::ReadChild {
            panel_parent,
            source: original,
            page,
            read_intent: Some(intent),
        },
    ] = commands.as_slice()
    else {
        panic!("read-only UI must issue one original read intent")
    };
    assert!(Arc::ptr_eq(original, source));
    assert_eq!(panel_parent, parent_scope);
    (*page, intent.clone())
}

async fn scheduled_reads(
    view: &mut ComponentDemo,
    source: &Arc<ChildPaneBinding>,
    parent: &RunScope,
) {
    for command in view.take_background_commands() {
        let DemoCommand::ReadChild {
            panel_parent,
            source: original,
            page,
            read_intent,
        } = command
        else {
            panic!("unexpected scheduled operation")
        };
        assert!(Arc::ptr_eq(&original, source));
        assert_eq!(&panel_parent, parent);
        assert!(!page && read_intent.is_none());
        let snapshot = original.snapshot().await.unwrap();
        assert!(
            view.update_child_snapshot(parent, &original, snapshot)
                .unwrap()
        );
    }
}

#[tokio::test]
async fn readonly_pages_and_reconnect_preserve_reading_and_current_focus() {
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default().with_children()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();

    let parent = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "b"))
            .await
            .unwrap(),
    );
    let other = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let mut view = ComponentDemo::new([parent.clone(), other]).unwrap();
    view.enable_transcript_content().unwrap();
    let parent_scope = parent.scope().clone();
    let parent_selection = parent.current_run().unwrap().selection.clone();
    let draft = "父会话草稿在只读分页和重连中保留";
    parent
        .edit_text(
            &ComposerDestination::run(parent_scope.clone()),
            draft.into(),
        )
        .unwrap();
    view.sync().unwrap();
    let parent_editor = view.ids().editor;
    view.tree.focus(Some(parent_editor), true);

    let source = Arc::new(
        ChildPaneBinding::for_run_with_page_size(
            &parent,
            "bounded-b",
            ChildHistoryLayout::SameSession,
            None,
            48,
        )
        .await
        .unwrap(),
    );
    let initial = source.snapshot().await.unwrap();
    let tekes_agent_ui_bindings::PaneSnapshot::ReadOnlyChild { transcript, .. } = &initial else {
        panic!("expected bounded read-only child snapshot")
    };
    assert!(transcript.has_more_before);
    assert!(!transcript.items.is_empty());
    let initial_oldest = transcript.items.first().unwrap().id.clone();
    let initial_item_count = transcript.items.len();

    let request = view.begin_child_open(&parent_scope).unwrap();
    assert!(
        view.install_child_source(request, source.clone(), "有界子历史".into(), initial)
            .unwrap()
    );
    scheduled_reads(&mut view, &source, &parent_scope).await;
    let root = view.bounded_child_view_id().unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    render(&mut view, &mut text);

    assert_eq!(pane(&view).ids().root, root);
    let before_version = scroll(&view)
        .reading_document_version()
        .expect("original SDK reading document")
        .clone();
    let original_viewport = pane(&view).ids().viewport;
    let before_pairs = message_rows(&view);
    let before_rows = message_item_ids(&view);
    let before_count = before_rows.len();
    assert!(before_count > 1);
    assert_eq!(view.tree.focused(), Some(parent_editor));
    assert_eq!(view.editor().input.text, draft);

    let to_bottom = scroll(&view).scroll.document_size().height
        - scroll(&view).scroll.viewport_size().height
        - scroll(&view).scroll.offset.y;
    scroll_by(&mut view, to_bottom, 1.);
    render(&mut view, &mut text);
    let before_scroll = scroll(&view).scroll.offset.y;
    assert!(
        before_scroll > 180.,
        "fixture must genuinely overflow the viewport: {:?}",
        scroll(&view).scroll
    );
    scroll_by(&mut view, -180., 2.);
    render(&mut view, &mut text);
    assert!(
        scroll(&view).scroll.offset.y < before_scroll,
        "original wheel must move the viewport upward"
    );
    let reading_item = anchor(&view);
    assert!(view.tree.get(reading_item).unwrap().visible().is_some());
    let reading_y = relative_y(&view, reading_item);
    let reading_offset = scroll(&view).scroll.offset.y;
    assert!(!scroll(&view).is_following_latest());

    let earlier_button = pane(&view).paging_button_id();
    let (is_page, earlier) = read_command(&mut view, earlier_button, &source, &parent_scope);
    assert!(is_page);
    assert_eq!(
        earlier.read(),
        tekes_agent_components_ui::ReadOnlyTranscriptRead::Earlier
    );
    // SDK accessibility Press explicitly focuses its target. An asynchronous
    // publication must retain that current focus, rather than retarget it.
    let page_focus = view.tree.focused();
    assert_eq!(page_focus, Some(earlier_button));
    let page = source.read_intent(earlier).await.unwrap();
    let (page_oldest, page_item_count) = match &page {
        tekes_agent_ui_bindings::PaneSnapshot::ReadOnlyChild { transcript, .. } => (
            transcript.items.first().unwrap().id.clone(),
            transcript.items.len(),
        ),
        _ => panic!("expected paged read-only child snapshot"),
    };
    assert!(
        view.update_child_snapshot(&parent_scope, &source, page)
            .unwrap()
    );
    render(&mut view, &mut text);

    assert_eq!(pane(&view).ids().root, root);
    assert!(message_item_ids(&view).len() > before_count);
    assert_ne!(page_oldest, initial_oldest);
    assert!(page_item_count > initial_item_count);
    let after_rows = message_rows(&view);
    assert!(before_pairs.iter().all(|row| after_rows.contains(row)));
    assert_eq!(pane(&view).ids().viewport, original_viewport);
    let inserted = after_rows
        .iter()
        .find(|(identity, _)| !before_rows.contains(identity))
        .expect("earlier page inserts an actual message row");
    assert!(
        view.tree.get(inserted.1).unwrap().frame().min_y()
            < view.tree.get(reading_item).unwrap().frame().min_y()
    );
    assert!(view.tree.get(reading_item).unwrap().visible().is_some());
    assert_eq!(anchor(&view), reading_item);
    assert!((relative_y(&view, reading_item) - reading_y).abs() <= 1.);
    assert!(!scroll(&view).is_following_latest());
    let page_y = relative_y(&view, reading_item);
    let page_offset = scroll(&view).scroll.offset.y;
    let after_page_version = scroll(&view).reading_document_version().unwrap().clone();
    assert_eq!(after_page_version.key(), before_version.key());
    assert!(after_page_version.revision() > before_version.revision());
    assert_eq!(view.tree.focused(), page_focus);
    assert_eq!(view.editor().input.text, draft);
    assert!(view.tree.focus(Some(parent_editor), true));

    runtime.reconnect().await.unwrap();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    view.sync().unwrap();
    scheduled_reads(&mut view, &source, &parent_scope).await;
    let (refresh, _) = view.bounded_child_buttons().unwrap();
    let (is_page, refresh_intent) = read_command(&mut view, refresh, &source, &parent_scope);
    assert!(!is_page);
    assert_eq!(
        refresh_intent.read(),
        tekes_agent_components_ui::ReadOnlyTranscriptRead::Refresh
    );
    assert_eq!(view.tree.focused(), Some(refresh));
    // Model an explicit return to the parent editor while the read is pending.
    // The returning frame must preserve this newer user focus intent.
    assert!(view.tree.focus(Some(parent_editor), true));
    let refreshed = source.read_intent(refresh_intent).await.unwrap();
    assert!(
        view.update_child_snapshot(&parent_scope, &source, refreshed)
            .unwrap()
    );
    render(&mut view, &mut text);

    assert_eq!(pane(&view).ids().root, root);
    assert_eq!(message_item_ids(&view).len(), after_rows.len());
    assert_eq!(anchor(&view), reading_item);
    assert!(view.tree.get(reading_item).unwrap().visible().is_some());
    assert!((relative_y(&view, reading_item) - reading_y).abs() <= 1.);
    assert!(!scroll(&view).is_following_latest());
    let after_refresh_version = scroll(&view).reading_document_version().unwrap();
    assert_eq!(after_refresh_version.key(), before_version.key());
    assert!(after_refresh_version.revision() > after_page_version.revision());
    assert_eq!(view.tree.focused(), Some(parent_editor));
    assert_eq!(view.editor().input.text, draft);

    assert_eq!(pane(&view).ids().viewport, original_viewport);
    assert!(Arc::ptr_eq(view.current_binding().unwrap(), &parent));
    assert_eq!(parent.current_run().unwrap().selection, parent_selection);
    assert_eq!(parent.current_run().unwrap().draft.text, draft);
    println!(
        "READONLY_PAGED_READING {}",
        serde_json::json!({
            "accepted":true,"evidenceScope":"same-mounted-readonly-fixture-cpu-reading",
            "sourceScope":source.binding().scope(),"paneRoot":root.0,"viewport":original_viewport.0,
            "anchorItem":reading_item.0,"messageRowsBefore":before_count,"messageRowsAfter":after_rows.len(),
            "readingYBefore":reading_y,"readingYAfterPage":page_y,"readingYAfterReconnect":relative_y(&view,reading_item),
            "offsetBeforeUp":before_scroll,"readingOffset":reading_offset,"offsetAfterPage":page_offset,
            "offsetAfterReconnect":scroll(&view).scroll.offset.y,
            "documentKey":before_version.key(),"revisions":[before_version.revision(),after_page_version.revision(),after_refresh_version.revision()],
            "followsLatest":scroll(&view).is_following_latest(),"parentDraftAndSelectionPreserved":true,
            "pageButtonFocusPreserved":true,"newerParentEditorFocusPreserved":true,
            "nativeInteraction":false,"physicalIme":false,"newHostCheckpoint":false,"nestedScrollAcceptance":false
        })
    );
    runtime.stop().await.unwrap();
}
