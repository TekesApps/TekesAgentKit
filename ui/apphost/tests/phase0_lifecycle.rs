//! Phase 0 lifecycle, AppHost slot-swap, ReadingController prepend anchor, and IME/controlled-edit tests.

use std::sync::Arc;
use tekes_agent_apphost::{AgentContainerAdapter, ManagedAgentContainer, SharedAgentWorkbench};
use tekes_agent_components_ui::ResponsiveWidthMode;
use tekes_agent_ui_bindings::UiWakeNotifier;
use tekes_ui_app_host::{
    AppWindow, ContainerContent, OthersOptions, PresentationOptions, PresentationState,
};
use tekes_ui_controls::{
    ButtonTextAlign, ButtonTextRenderer, DraftRevision, ExternalSelection, ExternalUndo,
    ExternalValueConflict, ExternalValueResult, ReadingAnchorUpdate, ReadingController,
    TextRangeMapping, TextSurface, TextSurfaceHost, TextSurfaceTextRenderer,
    composition::{
        Component, ComponentId, Content, ContentEvent, ContentHost, StackContent, TextContent,
        TextFieldContent, TextSubmitPolicy, TextViewport,
    },
};
use tekes_ui_render::Renderer;
use tekes_ui_scene::{Point, Rect, Scene, Size};
use tekes_ui_text::{
    Paragraph, ShapeOptions, TextStyle,
    coretext::{CoreTextFonts, CoreTextShaper},
    traits::Shaper,
};
use tekes_ui_tokens::{Palette, Rgba, Theme, proposal};

const SHELL_ID: ComponentId = ComponentId(1);
const STACK_ID: ComponentId = ComponentId(2);
const NOTE_ID: ComponentId = ComponentId(3);
const COMPOSER_ID: ComponentId = ComponentId(4);

struct CoreTextTestBackend(CoreTextShaper);

impl CoreTextTestBackend {
    fn new() -> Self {
        Self(CoreTextShaper::new(CoreTextFonts::new()))
    }
}

impl TextSurfaceTextRenderer for CoreTextTestBackend {
    fn push_visible(&mut self, _: &mut Scene, _: &Paragraph, _: Point, _: Rgba, _: Rect) {}
}

impl TextSurfaceHost for CoreTextTestBackend {
    fn shape_surface(
        &mut self,
        text: &str,
        role: proposal::TextRole,
        width: Option<f32>,
    ) -> RcTextParagraph {
        std::rc::Rc::new(self.0.shape(
            text,
            &TextStyle::system(role.style().size),
            &ShapeOptions {
                width: width.map(f64::from),
                line_spacing: 0.0,
            },
        ))
    }
}

type RcTextParagraph = std::rc::Rc<Paragraph>;

impl ButtonTextRenderer for CoreTextTestBackend {
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

#[derive(Default)]
struct DemoPhase0Adapter {
    last_width_mode: Option<ResponsiveWidthMode>,
}

impl AgentContainerAdapter for DemoPhase0Adapter {
    fn sync_from_workbench(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        width_mode: ResponsiveWidthMode,
    ) {
        self.last_width_mode = Some(width_mode);
        let status = workbench
            .background_status()
            .unwrap_or_else(|| "idle".to_owned());
        if let Some(node) = host.get_mut(NOTE_ID)
            && let Some(text) = node.component_mut::<TextContent>()
        {
            text.set_text(format!("{status} ({width_mode:?})"));
        }
    }
}

fn demo_content(label: &str) -> Content {
    let mut composer = TextFieldContent::new("输入消息", "");
    composer.show_metadata = false;
    composer.viewport = TextViewport::Grow {
        min: 44.0,
        max: 180.0,
    };
    composer.submit_policy = TextSubmitPolicy::CommandEnter;
    Content::new(
        STACK_ID,
        StackContent::vertical(
            8.0,
            vec![
                Content::new(NOTE_ID, TextContent::new(label)),
                Content::new(COMPOSER_ID, composer),
            ],
        )
        .with_flexible_child(COMPOSER_ID),
    )
}

#[test]
fn phase0_wake_publisher_and_inactive_deferral_work_without_frame_pump() {
    let workbench = SharedAgentWorkbench::new();
    let mut container = ManagedAgentContainer::build_source(
        SHELL_ID,
        demo_content("initial"),
        workbench.clone(),
        DemoPhase0Adapter::default(),
        false,
    )
    .unwrap();

    let bg = workbench.publish_background_note_sender();
    std::thread::spawn(move || {
        bg.publish("stream-ready");
    })
    .join()
    .unwrap();

    // Inactive container skips rebuilds while hidden in an unselected Others tab.
    container.with_host(&mut |host| {
        host.set_window_active(false);
    });
    assert!(!container.is_active());
    container.prepare_for_render();
    assert_eq!(container.render_sync_count(), 0);
    container.with_host(&mut |host| {
        let note = host
            .get(NOTE_ID)
            .unwrap()
            .component::<TextContent>()
            .unwrap();
        assert_eq!(note.text(), "idle (Expanded)");
    });

    // Re-activating triggers synchronized projection immediately.
    container.with_host(&mut |host| {
        host.set_window_active(true);
    });
    assert!(container.is_active());
    container.prepare_for_render();
    assert_eq!(container.render_sync_count(), 1);
    container.with_host(&mut |host| {
        let note = host
            .get(NOTE_ID)
            .unwrap()
            .component::<TextContent>()
            .unwrap();
        assert_eq!(note.text(), "stream-ready (Expanded)");
    });

    // UiWakeNotifier can notify safely with zero dead panics when zero wakes registered.
    let hub: Arc<dyn UiWakeNotifier> = workbench.wake_hub();
    hub.wake();
}

#[test]
fn phase0_slot_swap_preserves_mount_identity_and_active_cjk_preedit_and_responsive_width() {
    let workbench = SharedAgentWorkbench::new();
    let run_container = ManagedAgentContainer::build_local(
        "agent-run",
        SHELL_ID,
        demo_content("run"),
        workbench.clone(),
        DemoPhase0Adapter::default(),
        false,
    )
    .unwrap();
    let mut summary_container = ManagedAgentContainer::build_local(
        "agent-summary",
        ComponentId(101),
        Content::new(ComponentId(102), TextContent::new("summary")),
        workbench.clone(),
        DemoPhase0Adapter::default(),
        false,
    )
    .unwrap();

    let mut run_box = run_container;
    let initial_composer_mount = {
        let mut mount = 0;
        run_box.with_host(&mut |host| {
            mount = host.get(COMPOSER_ID).unwrap().mount_identity();
            assert!(host.focus(Some(COMPOSER_ID), true));
            let resp = host.event(&ContentEvent::Preedit {
                text: "zhongwen".to_owned(),
                selection: 8..8,
                clauses: std::iter::once(0..8).collect(),
                replacement: None,
            });
            assert!(resp.consumed);
            let field = host
                .get(COMPOSER_ID)
                .unwrap()
                .component::<TextFieldContent>()
                .unwrap();
            assert!(field.composing());

            // External draft update during active CJK IME preedit must be safely rejected as Composition conflict!
            let field_mut = host
                .get_mut(COMPOSER_ID)
                .unwrap()
                .component_mut::<TextFieldContent>()
                .unwrap();
            let baseline = field_mut.input().external_baseline();
            let result = field_mut.apply_external_value(
                &baseline,
                &DraftRevision {
                    identity: "session:a".to_owned(),
                    version: 1,
                    payload: "remote overwrite".to_owned(),
                },
                ExternalSelection::Preserve,
                ExternalUndo::Transaction,
            );
            assert_eq!(
                result,
                ExternalValueResult::Conflict(ExternalValueConflict::Composition)
            );
        });
        mount
    };

    // Simulate Compact width in `others` vs Expanded width in `focus` on the SAME persistent host.
    let mut text = CoreTextTestBackend::new();
    let mut scene = Scene::new();
    let palette = Palette::from(Theme::LIGHT);
    let compact = Rect::new(0.0, 0.0, 340.0, 600.0);
    run_box.with_host(&mut |host| {
        host.render(&mut scene, &mut text, compact, compact, palette)
            .unwrap();
    });

    // Assemble AppWindow with Run in `others` (Topology B style) and promote to `focus`, verifying zero rebuild.
    summary_container = summary_container.title("Workspace");
    let containers = vec![summary_container.into(), run_box.into()];
    let presentation = PresentationOptions::new("agent-summary")
        .workspace_switch(true)
        .others(OthersOptions::new(vec!["agent-run".to_owned()]));

    let mut state = PresentationState::new(&containers, presentation.clone()).unwrap();
    assert_eq!(state.focus_id(), "agent-summary");
    state.promote("agent-run").unwrap();
    assert_eq!(state.focus_id(), "agent-run");

    let app = AppWindow::from_containers(containers, presentation).unwrap();
    let (_menus, mut hosted) = app.into_window_handler().unwrap();
    let mut gpu = Renderer::new_headless().unwrap();
    let png = hosted
        .render_headless_rgba8(&mut gpu, (960, 640), 1)
        .unwrap();
    assert_eq!(png.len(), 960 * 640 * 4);
    assert!(initial_composer_mount > 0);
}

#[test]
fn phase0_reading_controller_preserves_anchor_across_prepend_and_reflow() {
    let mut text_backend = CoreTextTestBackend::new();
    let mut surface = TextSurface::new("", true, true);
    surface.set_role(proposal::TextRole::UiBody);
    let mut reading = ReadingController::default();

    let initial_turns: Vec<String> = (20..=40)
        .map(|i| {
            format!("轮次 {i:02}：这是已经加载的对话记录内容，用于验证向前回溯历史时的锚点连续性。")
        })
        .collect();
    let initial_doc = initial_turns.join("\n\n");
    let v1 = reading
        .set_document(&mut surface, "transcript:a", 1, &initial_doc)
        .unwrap();

    let wide = Rect::new(0.0, 0.0, 720.0, 360.0);
    reading.layout_with(&mut surface, wide, Some(wide), 2.0, 1.0, |s, w| {
        text_backend.shape_surface(s, proposal::TextRole::UiBody, w)
    });
    // Scroll down to mid-document so we are reading around turn 30 in history mode.
    surface.scroll_mut().scroll_by(0.0, 220.0);
    reading.stop_following(&mut surface);
    reading.layout_with(&mut surface, wide, Some(wide), 2.0, 1.0, |s, w| {
        text_backend.shape_surface(s, proposal::TextRole::UiBody, w)
    });
    let offset_before_prepend = surface.scroll().offset.y;
    assert!(offset_before_prepend > 100.0);

    // Prepend turns 01..=19 before turns 20..=40.
    let older: Vec<String> = (1..=19)
        .map(|i| format!("轮次 {i:02}：更早的历史消息内容。"))
        .collect();
    let prefix = format!("{}\n\n", older.join("\n\n"));
    let updated_doc = format!("{prefix}{initial_doc}");
    let prefix_utf16 = prefix.encode_utf16().count();
    let initial_utf16 = initial_doc.encode_utf16().count();

    let update = reading
        .update_document(
            &mut surface,
            &v1,
            2,
            &updated_doc,
            &[TextRangeMapping {
                old: 0..initial_utf16,
                new: prefix_utf16..(prefix_utf16 + initial_utf16),
            }],
        )
        .unwrap();
    assert_eq!(update.anchor, ReadingAnchorUpdate::Mapped);

    reading.layout_with(&mut surface, wide, Some(wide), 2.0, 1.0, |s, w| {
        text_backend.shape_surface(s, proposal::TextRole::UiBody, w)
    });
    // Scroll offset must advance by roughly the prepended paragraph height so visible text stays anchored!
    assert!(
        surface.scroll().offset.y > offset_before_prepend + 150.0,
        "offset_before={offset_before_prepend}, offset_after={}",
        surface.scroll().offset.y
    );

    // Reflow from wide (focus) to narrow (others) preserves history intent (does not jump to bottom).
    let narrow = Rect::new(0.0, 0.0, 320.0, 360.0);
    reading.layout_with(&mut surface, narrow, Some(narrow), 2.0, 1.0, |s, w| {
        text_backend.shape_surface(s, proposal::TextRole::UiBody, w)
    });
    assert!(!reading.is_following_latest());
}
