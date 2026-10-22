//! Strict same-process nested-reading gate; no window, GPU or harness connection.
//! Run explicitly; a nonzero status is a missing capability, not acceptance.
#[cfg(target_os = "macos")]
mod acceptance {
    use std::rc::Rc;
    use tekes_agent_components_ui::*;
    use tekes_ui_controls::{
        ButtonTextAlign, ButtonTextRenderer, ScrollArea, TextSurfaceHost, TextSurfaceTextRenderer,
        composition::*,
        extended::{Axis, Stack},
    };
    use tekes_ui_scene::{Point, Rect, Scene, Size};
    use tekes_ui_text::{
        LabelLayout, LabelOptions, Paragraph,
        coretext::{CoreTextFonts, CoreTextShaper},
        model::ShapeOptions,
        style::TextStyle,
        traits::Shaper,
    };
    use tekes_ui_tokens::{Rgba, Theme, proposal};
    struct Text(CoreTextShaper);
    impl TextSurfaceTextRenderer for Text {
        fn push_visible(&mut self, _: &mut Scene, _: &Paragraph, _: Point, _: Rgba, _: Rect) {}
    }
    impl TextSurfaceHost for Text {
        fn shape_surface(
            &mut self,
            value: &str,
            role: proposal::TextRole,
            width: Option<f32>,
        ) -> Rc<Paragraph> {
            Rc::new(self.0.shape(
                value,
                &TextStyle::system(role.style().size),
                &ShapeOptions {
                    width: width.map(f64::from),
                    line_spacing: 0.,
                },
            ))
        }
        fn shape_label(
            &mut self,
            value: &str,
            role: proposal::TextRole,
            width: f32,
            options: LabelOptions,
        ) -> Option<Rc<LabelLayout>> {
            Some(Rc::new(self.0.shape_label(
                value,
                &TextStyle::system(role.style().size),
                &ShapeOptions {
                    width: Some(width.into()),
                    line_spacing: 0.,
                },
                options,
            )))
        }
    }
    impl ButtonTextRenderer for Text {
        fn measure(&mut self, value: &str, role: proposal::TextRole, width: f32) -> Size {
            let p = self.shape_surface(value, role, Some(width));
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
    pub fn run() -> bool {
        let scope = RunScope::main("public-consumer", "nested-reading");
        let source = TranscriptItem {
            id: "reasoning".into(),
            record_id: Some("reasoning".into()),
            reconciliation_id: None,
            turn_id: "turn".into(),
            kind: TranscriptItemKind::Assistant,
            text: String::new(),
            reasoning: "原始中文阅读位置\n".repeat(100),
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
        let mut output =
            OutputMessage::with_bounded_details(64310, scope.clone(), Default::default());
        output
            .update(
                &scope,
                1,
                1,
                &TranscriptOutput {
                    key: TranscriptMessageKey::NativeEntry("reasoning"),
                    item: &source,
                },
                None,
            )
            .unwrap();
        let output_id = output.id();
        let detail = output.content_ids()[0];
        let viewport = ComponentId(21);
        let stack = ComponentId(22);
        let appended = ComponentId(23);
        let child = Content::new(
            stack,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: 0.,
                },
                flexible_child: None,
                children: vec![Content::new(output_id, output)],
            },
        );
        let mut host = ContentHost::new(Content::new(
            viewport,
            ScrollContent::new(ScrollArea::default(), child),
        ))
        .unwrap();
        host.set_action_events(true);
        host.accessibility_action(detail.disclosure.unwrap(), None, AccessibilityAction::Press);
        let actions = host.drain_actions();
        assert!(
            host.get_mut(output_id)
                .unwrap()
                .component_mut::<OutputMessage>()
                .unwrap()
                .handle_actions(&actions)
        );
        let (version, ()) = host
            .set_scroll_reading_document(viewport, "nested-reading", 1, vec![output_id], |_| ())
            .unwrap();
        host.get_mut(viewport)
            .unwrap()
            .component_mut::<ScrollContent>()
            .unwrap()
            .follow_latest();
        let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()));
        let frame = Rect::new(0., 0., 480., 300.);
        host.render(
            &mut Scene::new(),
            &mut text,
            frame,
            frame,
            Theme::LIGHT.into(),
        )
        .unwrap();
        let before = host
            .get(viewport)
            .unwrap()
            .component::<ScrollContent>()
            .unwrap()
            .scroll
            .offset
            .y;
        for (time, delta) in [(1., 160.), (2., -60.)] {
            let position = host.get(detail.body).unwrap().visible().unwrap().center();
            host.event(&ContentEvent::Scroll {
                position,
                delta: Point::new(0., delta),
                precise: true,
                begin: true,
                end: true,
                momentum: false,
                time,
            });
            host.render(
                &mut Scene::new(),
                &mut text,
                frame,
                frame,
                Theme::LIGHT.into(),
            )
            .unwrap();
        }
        let inner = host.text_surface(detail.body).unwrap().scroll.offset.y;
        let parent = host
            .get(viewport)
            .unwrap()
            .component::<ScrollContent>()
            .unwrap();
        let following = parent.is_following_latest();
        let after_scroll = parent.scroll.offset.y;
        host.update_scroll_reading_document(
            viewport,
            &version,
            2,
            vec![output_id, appended],
            |host| {
                host.get_mut(stack)
                    .unwrap()
                    .component_mut::<StackContent>()
                    .unwrap()
                    .children
                    .push(Content::new(
                        appended,
                        TextContent {
                            text: "后来追加的另一条正文\n".repeat(40),
                            role: proposal::TextRole::UiBody,
                        },
                    ));
            },
        )
        .unwrap();
        host.render(
            &mut Scene::new(),
            &mut text,
            frame,
            frame,
            Theme::LIGHT.into(),
        )
        .unwrap();
        let after_append = host
            .get(viewport)
            .unwrap()
            .component::<ScrollContent>()
            .unwrap()
            .scroll
            .offset
            .y;
        let visible = host.get(detail.body).unwrap().visible().is_some();
        let accepted = inner > 0. && !following && (after_scroll - before).abs() < 0.5 && visible;
        println!(
            "{{\"accepted\":{accepted},\"parentFollowingAfterInnerUp\":{following},\"parentOffsetBefore\":{before},\"parentOffsetAfterScroll\":{after_scroll},\"parentOffsetAfterAppend\":{after_append},\"innerOffset\":{inner},\"detailVisibleAfterAppend\":{visible}}}"
        );
        accepted
    }
}
fn main() {
    #[cfg(target_os = "macos")]
    if !acceptance::run() {
        std::process::exit(1);
    }
    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("This CoreText layout gate requires macOS; no native window is opened.");
        std::process::exit(2);
    }
}
