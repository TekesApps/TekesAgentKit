#![cfg(target_os = "macos")]
use std::rc::Rc;
use tekes_agent_components_ui::{OutputContentKind, OutputMessage};
use tekes_ui_controls::{
    ButtonTextAlign, ButtonTextRenderer, TextSurfaceHost, TextSurfaceTextRenderer,
    composition::ContentEvent,
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

use serde_json::json;
use tekes_agent_presentation::{
    RunScope, TranscriptBlockContent as Block, TranscriptContentBlock, TranscriptItem,
    TranscriptItemKind, TranscriptMessageKey, TranscriptOutput, TranscriptSection,
    TranscriptToolCall,
};
use tekes_ui_controls::composition::{
    AccessibilityAction, ComponentId, ContentHost, TextFieldContent,
};
fn scope() -> RunScope {
    RunScope::main("fixture", "a")
}
fn item() -> TranscriptItem {
    TranscriptItem {
        id: "message".into(),
        record_id: Some("native-message".into()),
        reconciliation_id: None,
        turn_id: "turn".into(),
        kind: TranscriptItemKind::Assistant,
        text: "中文回复".into(),
        reasoning: String::new(),
        attachments: vec![],
        content_blocks: Some(vec![block(
            0,
            Block::Text {
                text: "中文回复".into(),
            },
        )]),
        is_visible: true,
        is_final: false,
        is_provisional: true,
        reasoning_ended: false,
        source_sequence: None,
        source_position: None,
        time: None,
        metadata: json!({}),
    }
}
fn block(index: usize, content: Block) -> TranscriptContentBlock {
    TranscriptContentBlock { index, content }
}
fn output(item: &TranscriptItem) -> TranscriptOutput<'_> {
    TranscriptOutput {
        key: TranscriptMessageKey::NativeEntry("native-message"),
        item,
    }
}
fn tool() -> TranscriptToolCall {
    TranscriptToolCall {
        result_meta: None,
        result_error: None,
        id: "tool".into(),
        turn_id: "turn".into(),
        name: "read".into(),
        arguments: json!({"path":"a.rs"}),
        result: None,
        phase: "doing".into(),
        call_view: None,
        result_view: None,
        source_position: None,
        result_position: None,
    }
}
fn section(tool: &TranscriptToolCall) -> TranscriptSection<'_> {
    TranscriptSection {
        id: "turn",
        turn: None,
        inputs: vec![],
        outputs: vec![],
        tools: vec![tool],
    }
}
fn view(host: &ContentHost, id: ComponentId) -> &OutputMessage {
    host.get(id).unwrap().component::<OutputMessage>().unwrap()
}
fn view_mut(host: &mut ContentHost, id: ComponentId) -> &mut OutputMessage {
    host.get_mut(id)
        .unwrap()
        .component_mut::<OutputMessage>()
        .unwrap()
}
fn toggle(host: &mut ContentHost, root: ComponentId, button: ComponentId) -> bool {
    host.accessibility_action(button, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    view_mut(host, root).handle_actions(&actions)
}
fn tool_body(
    host: &ContentHost,
    root: ComponentId,
    kind: tekes_agent_presentation::TranscriptToolDetailKind,
) -> ComponentId {
    view(host, root)
        .tool_detail_sections()
        .into_iter()
        .find(|ids| ids.kind == kind)
        .unwrap()
        .body
}
#[test]
fn source_order_tool_details_and_opaque_payloads_remain_distinct() {
    let mut item = item();
    let tool = tool();
    item.content_blocks = Some(vec![
        block(
            0,
            Block::Text {
                text: "正文前段".into(),
            },
        ),
        block(
            1,
            Block::Reasoning {
                text: "推理原文".into(),
            },
        ),
        block(
            2,
            Block::ToolCall {
                tool_id: "tool".into(),
            },
        ),
        block(
            3,
            Block::Text {
                text: "正文后段".into(),
            },
        ),
        block(
            4,
            Block::Attachment {
                content: json!({"type":"file","name":"a.txt"}),
            },
        ),
        block(
            5,
            Block::Unknown {
                content: json!({"type":"future","value":"保留"}),
            },
        ),
    ]);
    let mut row = OutputMessage::new(701, scope());
    row.update(&scope(), 1, 1, &output(&item), Some(&section(&tool)))
        .unwrap();
    let root = row.id();
    let ids = row.content_ids();
    assert_eq!(
        ids.iter().map(|i| i.kind).collect::<Vec<_>>(),
        vec![
            OutputContentKind::Text,
            OutputContentKind::Reasoning,
            OutputContentKind::Tool,
            OutputContentKind::Text,
            OutputContentKind::Attachment,
            OutputContentKind::Unknown
        ]
    );
    let mut host = row.into_host().unwrap();
    assert_eq!(
        host.text_surface(ids[0].body).unwrap().input.text,
        "正文前段"
    );
    for i in [1, 2, 4, 5] {
        assert!(host.get(ids[i].body).is_none());
        assert!(toggle(&mut host, root, ids[i].disclosure.unwrap()));
    }
    assert_eq!(
        host.text_surface(ids[1].body).unwrap().input.text,
        "推理原文"
    );
    let input = tool_body(
        &host,
        root,
        tekes_agent_presentation::TranscriptToolDetailKind::Input,
    );
    let output = tool_body(
        &host,
        root,
        tekes_agent_presentation::TranscriptToolDetailKind::Output,
    );
    assert!(
        host.text_surface(input)
            .unwrap()
            .input
            .text
            .contains("a.rs")
    );
    assert!(host.text_surface(output).unwrap().input.text.is_empty());
    assert!(
        host.text_surface(ids[4].body)
            .unwrap()
            .input
            .text
            .contains("a.txt")
    );
    assert!(
        host.text_surface(ids[5].body)
            .unwrap()
            .input
            .text
            .contains("保留")
    );
}
#[test]
fn streaming_and_tool_completion_keep_existing_text_selection_focus_and_disclosure() {
    let mut item = item();
    item.content_blocks.as_mut().unwrap().push(block(
        1,
        Block::ToolCall {
            tool_id: "tool".into(),
        },
    ));
    let mut tool = tool();
    let mut row = OutputMessage::new(702, scope());
    row.update(&scope(), 1, 1, &output(&item), Some(&section(&tool)))
        .unwrap();
    let root = row.id();
    let ids = row.content_ids();
    let mut host = row.into_host().unwrap();
    assert!(host.focus(Some(ids[0].body), true));
    host.get_mut(ids[0].body)
        .unwrap()
        .component_mut::<TextFieldContent>()
        .unwrap()
        .surface
        .input
        .select_range(0..2);
    assert!(toggle(&mut host, root, ids[1].disclosure.unwrap()));
    assert!(host.focus(Some(ids[0].body), true));
    item.content_blocks.as_mut().unwrap()[0] = block(
        0,
        Block::Text {
            text: "中文回复，继续流式追加".into(),
        },
    );
    tool.result = Some(json!({"text":"完整结果"}));
    tool.phase = "done".into();
    view_mut(&mut host, root)
        .update(&scope(), 2, 1, &output(&item), Some(&section(&tool)))
        .unwrap();
    assert_eq!(view(&host, root).content_ids(), ids);
    assert_eq!(host.focused(), Some(ids[0].body));
    assert_eq!(
        host.text_surface(ids[0].body).unwrap().input.selection(),
        0..2
    );
    assert!(
        host.text_surface(tool_body(
            &host,
            root,
            tekes_agent_presentation::TranscriptToolDetailKind::Output
        ))
        .unwrap()
        .input
        .text
        .contains("完整结果")
    );
    assert!(toggle(&mut host, root, ids[1].disclosure.unwrap()));
    assert!(host.get(ids[1].body).is_none());
    tool.result = Some(json!({"text":"重连核对结果"}));
    view_mut(&mut host, root)
        .update(&scope(), 3, 2, &output(&item), Some(&section(&tool)))
        .unwrap();
    assert!(toggle(&mut host, root, ids[1].disclosure.unwrap()));
    assert!(
        host.text_surface(tool_body(
            &host,
            root,
            tekes_agent_presentation::TranscriptToolDetailKind::Output
        ))
        .unwrap()
        .input
        .text
        .contains("重连核对结果")
    );
}
#[test]
fn unresolved_tools_do_not_resolve_from_another_turn_and_replaced_ids_retire() {
    let mut item = item();
    item.content_blocks = Some(vec![block(
        0,
        Block::ToolCall {
            tool_id: "tool".into(),
        },
    )]);
    let mut row = OutputMessage::new(703, scope());
    row.update(&scope(), 1, 1, &output(&item), None).unwrap();
    let root = row.id();
    let ids = row.content_ids();
    let mut host = row.into_host().unwrap();
    let mut tool = tool();
    tool.turn_id = "other".into();
    view_mut(&mut host, root)
        .update(&scope(), 2, 1, &output(&item), Some(&section(&tool)))
        .unwrap();
    toggle(&mut host, root, ids[0].disclosure.unwrap());
    assert_eq!(host.text_surface(ids[0].body).unwrap().input.text, "tool");
    host.accessibility_action(ids[0].disclosure.unwrap(), None, AccessibilityAction::Press);
    let old_actions = host.drain_actions();
    item.content_blocks = Some(vec![block(
        0,
        Block::ToolCall {
            tool_id: "new-tool".into(),
        },
    )]);
    view_mut(&mut host, root)
        .update(&scope(), 3, 1, &output(&item), None)
        .unwrap();
    assert_ne!(
        view(&host, root).content_ids()[0].disclosure,
        ids[0].disclosure
    );
    assert!(!view_mut(&mut host, root).handle_actions(&old_actions));
}
#[test]
fn invalid_scope_publication_identity_and_block_order_leave_content_unchanged() {
    let item = item();
    let mut row = OutputMessage::new(704, scope());
    row.update(&scope(), 2, 2, &output(&item), None).unwrap();
    let ids = row.content_ids();
    assert!(
        row.update(&RunScope::main("fixture", "b"), 3, 2, &output(&item), None)
            .is_err()
    );
    assert!(row.update(&scope(), 1, 2, &output(&item), None).is_err());
    assert!(row.update(&scope(), 3, 1, &output(&item), None).is_err());
    assert!(row.update(&scope(), 2, 3, &output(&item), None).is_err());
    assert!(
        row.update(
            &scope(),
            3,
            2,
            &TranscriptOutput {
                key: TranscriptMessageKey::NativeEntry("other"),
                item: &item
            },
            None
        )
        .is_err()
    );
    let mut changed = item.clone();
    changed.content_blocks.as_mut().unwrap().push(block(
        0,
        Block::Text {
            text: "非法重复".into(),
        },
    ));
    assert!(row.update(&scope(), 3, 2, &output(&changed), None).is_err());
    assert_eq!(row.content_ids(), ids);
    let host = row.into_host().unwrap();
    assert_eq!(
        host.text_surface(ids[0].body).unwrap().input.text,
        "中文回复"
    );
}
#[test]
fn aggregate_source_coverage_is_explicit_and_does_not_fabricate_order() {
    let mut item = item();
    item.content_blocks = None;
    item.reasoning = "旧推理".into();
    let mut row = OutputMessage::new(705, scope());
    row.update(&scope(), 1, 1, &output(&item), None).unwrap();
    assert_eq!(
        row.content_ids().iter().map(|i| i.kind).collect::<Vec<_>>(),
        vec![
            OutputContentKind::Coverage,
            OutputContentKind::Reasoning,
            OutputContentKind::Text
        ]
    );
}
#[test]
fn machine_content_stays_leading_in_light_dark_and_narrow_layouts() {
    let mut item = item();
    item.content_blocks = Some(vec![
        block(
            0,
            Block::Reasoning {
                text: "推理中文内容。".repeat(10),
            },
        ),
        block(
            1,
            Block::Text {
                text: "机器正文中文段落。".repeat(12),
            },
        ),
    ]);
    let mut row = OutputMessage::new(706, scope());
    row.update(&scope(), 1, 1, &output(&item), None).unwrap();
    let root = row.id();
    let ids = row.content_ids();
    let mut host = row.into_host().unwrap();
    toggle(&mut host, root, ids[0].disclosure.unwrap());
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let mut heights = vec![];
    for (theme, width) in [
        (Theme::LIGHT, 600.),
        (Theme::DARK, 600.),
        (Theme::DARK, 180.),
    ] {
        let frame = Rect::new(40., 20., width, 2000.);
        host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
            .unwrap();
        for id in [ids[0].root, ids[0].body, ids[1].body] {
            let f = host.get(id).unwrap().frame();
            assert!((f.min_x() - frame.min_x()).abs() < 0.01);
            assert!(f.size.width <= width);
        }
        let a = host.get(ids[0].body).unwrap().frame();
        let b = host.get(ids[1].body).unwrap().frame();
        assert!(b.min_y() > a.max_y());
        heights.push(b.size.height);
    }
    assert!(heights[2] > heights[1]);
    host.focus(Some(ids[1].body), true);
    host.event(&ContentEvent::Commit {
        text: "不可编辑".into(),
        replacement: None,
    });
    assert!(
        host.text_surface(ids[1].body)
            .unwrap()
            .input
            .text
            .starts_with("机器正文")
    );
}

#[test]
fn status_and_empty_payloads_remain_visible_and_host_copy_is_explicit() {
    use tekes_agent_components_ui::{OutputMessageLabels, OutputMessageText};
    let mut item = item();
    item.kind = TranscriptItemKind::Error;
    item.content_blocks = Some(vec![]);
    let labels = OutputMessageLabels {
        text: |key| {
            match key {
                OutputMessageText::Error => "Host error",
                _ => "Host content",
            }
            .into()
        },
        ..Default::default()
    };
    let mut row = OutputMessage::with_labels(707, scope(), labels);
    row.update(&scope(), 1, 1, &output(&item), None).unwrap();
    let ids = row.content_ids();
    let host = row.into_host().unwrap();
    assert_eq!(
        host.text_surface(ids[0].body).unwrap().input.text,
        "Host error"
    );
    item.kind = TranscriptItemKind::Assistant;
    let mut empty = OutputMessage::new(708, scope());
    empty.update(&scope(), 1, 1, &output(&item), None).unwrap();
    let ids = empty.content_ids();
    assert_eq!(ids[0].kind, OutputContentKind::Status);
    assert!(
        !empty
            .into_host()
            .unwrap()
            .text_surface(ids[0].body)
            .unwrap()
            .input
            .text
            .is_empty()
    );
}
#[test]
fn repeated_public_tool_references_preserve_order_and_disclosure_semantics() {
    let mut item = item();
    item.content_blocks = Some(vec![
        block(
            0,
            Block::ToolCall {
                tool_id: "tool".into(),
            },
        ),
        block(
            1,
            Block::ToolCall {
                tool_id: "tool".into(),
            },
        ),
    ]);
    let tool = tool();
    let mut row = OutputMessage::new(709, scope());
    row.update(&scope(), 1, 1, &output(&item), Some(&section(&tool)))
        .unwrap();
    let root = row.id();
    let ids = row.content_ids();
    assert_ne!(ids[0].disclosure, ids[1].disclosure);
    let mut host = row.into_host().unwrap();
    assert!(toggle(&mut host, root, ids[0].disclosure.unwrap()));
    let nodes = host.semantics();
    assert_eq!(
        nodes
            .iter()
            .find(|n| n.id == ids[0].root)
            .unwrap()
            .semantics
            .expanded,
        Some(true)
    );
    assert_eq!(
        nodes
            .iter()
            .find(|n| n.id == ids[1].root)
            .unwrap()
            .semantics
            .expanded,
        Some(false)
    );
}

#[test]
fn known_empty_assistant_containers_stay_hidden_until_prose_arrives_and_legacy_stays_visible() {
    use tekes_agent_presentation::TranscriptSourcePosition as Position;
    use tekes_ui_controls::composition::Component;
    let mut message = item();
    message.text.clear();
    message.content_blocks = Some(vec![]);
    message.source_position = Some(Position::Native {
        entry_id: "native-message".into(),
        entry_ordinal: 4,
        content_index: None,
    });
    let mut call = tool();
    call.source_position = Some(Position::Native {
        entry_id: "native-message".into(),
        entry_ordinal: 4,
        content_index: Some(0),
    });
    let container = section(&call);
    let mut standalone = OutputMessage::new(722, scope());
    standalone
        .update(&scope(), 1, 1, &output(&message), None)
        .unwrap();
    assert!(standalone.content_ids().is_empty());
    assert!(!standalone.active());
    let mut row = OutputMessage::new(720, scope());
    row.update(&scope(), 1, 1, &output(&message), Some(&container))
        .unwrap();
    assert!(row.content_ids().is_empty());
    assert!(!row.active());
    let root = row.id();
    let mut host = row.into_host().unwrap();
    message.text = "后来到达的正文".into();
    message.content_blocks = Some(vec![block(
        0,
        Block::Text {
            text: message.text.clone(),
        },
    )]);
    view_mut(&mut host, root)
        .update(&scope(), 2, 1, &output(&message), Some(&container))
        .unwrap();
    assert!(view(&host, root).active());
    let id = view(&host, root).content_ids()[0].body;
    assert_eq!(host.text_surface(id).unwrap().input.text, "后来到达的正文");
    message.text.clear();
    message.content_blocks = None;
    let mut other = call.clone();
    other.source_position = Some(Position::Native {
        entry_id: "other-container".into(),
        entry_ordinal: 4,
        content_index: Some(0),
    });
    let mut ordinal = call.clone();
    ordinal.source_position = Some(Position::Native {
        entry_id: "native-message".into(),
        entry_ordinal: 5,
        content_index: Some(0),
    });
    let mut turn = call;
    turn.turn_id = "other-turn".into();
    for unrelated in [other, ordinal, turn] {
        let unproven = section(&unrelated);
        let mut row = OutputMessage::new(721, scope());
        row.update(&scope(), 1, 1, &output(&message), Some(&unproven))
            .unwrap();
        assert_eq!(row.content_ids()[0].kind, OutputContentKind::Status);
        assert!(row.active());
    }
}

#[test]
fn embedded_tool_uses_producer_terminal_description_and_retains_raw_detail_owner_on_update() {
    use tekes_ui_controls::composition::ButtonContent;
    let mut item = item();
    item.content_blocks = Some(vec![block(
        0,
        Block::ToolCall {
            tool_id: "tool".into(),
        },
    )]);
    let mut tool = tool();
    tool.name = "shell".into();
    tool.arguments =
        json!({"command":"echo original\nfull body","description":"argument fallback"});
    tool.call_view = Some(
        json!({"for":"call","view":{"card":"terminal","description":"检查项目","title":"echo original\nfull body"}}),
    );
    let original = tool.clone();
    let mut row = OutputMessage::new(729, scope());
    row.update(&scope(), 1, 1, &output(&item), Some(&section(&tool)))
        .unwrap();
    let root = row.id();
    let ids = row.content_ids()[0];
    let mut host = row.into_host().unwrap();
    let header = host
        .get(ids.disclosure.unwrap())
        .unwrap()
        .component::<ButtonContent>()
        .unwrap();
    assert!(header.label.contains("Shell · 检查项目 · doing"));
    assert!(!header.label.contains("echo original"));
    assert!(toggle(&mut host, root, ids.disclosure.unwrap()));
    let input = tool_body(
        &host,
        root,
        tekes_agent_presentation::TranscriptToolDetailKind::Input,
    );
    let output_body = tool_body(
        &host,
        root,
        tekes_agent_presentation::TranscriptToolDetailKind::Output,
    );
    let owner = host.text_surface(input).unwrap() as *const _ as usize;
    assert!(
        host.text_surface(input)
            .unwrap()
            .input
            .text
            .contains("full body")
    );
    assert_eq!(tool, original);
    tool.phase = "done".into();
    tool.result = Some(json!({"text":"complete raw result"}));
    tool.result_view =
        Some(json!({"for":"result","view":{"title":"must not replace terminal action"}}));
    view_mut(&mut host, root)
        .update(&scope(), 2, 1, &output(&item), Some(&section(&tool)))
        .unwrap();
    assert_eq!(view(&host, root).content_ids()[0], ids);
    assert_eq!(
        host.text_surface(input).unwrap() as *const _ as usize,
        owner
    );
    let header = host
        .get(ids.disclosure.unwrap())
        .unwrap()
        .component::<ButtonContent>()
        .unwrap();
    assert!(header.label.contains("Shell · 检查项目 · done"));
    assert!(!header.label.contains("must not replace terminal action"));
    assert!(
        host.text_surface(output_body)
            .unwrap()
            .input
            .text
            .contains("complete raw result")
    );
    let long = "原服务端提供的完整工具描述 👩🏽‍💻 e\u{301} ".repeat(20);
    tool.call_view.as_mut().unwrap()["view"]["description"] = json!(long);
    view_mut(&mut host, root)
        .update(&scope(), 3, 1, &output(&item), Some(&section(&tool)))
        .unwrap();
    for scale in [1., 1.4] {
        let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), scale);
        let mut heights = vec![];
        for width in [600., 180., 600.] {
            let frame = Rect::new(20., 20., width, 5000.);
            host.render(
                &mut Scene::new(),
                &mut text,
                frame,
                frame,
                Theme::LIGHT.into(),
            )
            .unwrap();
            let header = host.get(ids.disclosure.unwrap()).unwrap();
            assert!(header.frame().size.width <= width + 0.02);
            assert!(
                header
                    .component::<ButtonContent>()
                    .unwrap()
                    .label
                    .contains(&long)
            );
            assert_eq!(
                host.semantics()
                    .iter()
                    .find(|node| node.id == ids.disclosure.unwrap())
                    .unwrap()
                    .semantics
                    .name,
                header.component::<ButtonContent>().unwrap().label
            );
            heights.push(header.frame().size.height);
        }
        assert!(
            heights.iter().all(|h| (*h - heights[0]).abs() < 0.02),
            "original SDK title must stay single line across widths"
        );
    }
}
#[test]
fn bounded_detail_caps_reasoning_and_tools_but_keeps_ordinary_prose_in_document_flow() {
    let mut message = item();
    let long = "中文详情 👩🏽‍💻 e\u{301} 一行完整内容\n".repeat(100);
    message.text = String::new();
    message.reasoning = long.clone();
    message.content_blocks = Some(vec![
        block(0, Block::Reasoning { text: long.clone() }),
        block(
            1,
            Block::ToolCall {
                tool_id: "tool".into(),
            },
        ),
    ]);
    let mut call = tool();
    call.phase = "done".into();
    call.result = Some(json!({"content":[{"type":"text","text":long}]}));
    let mut row = OutputMessage::with_bounded_details(710, scope(), Default::default());
    row.update(&scope(), 1, 1, &output(&message), Some(&section(&call)))
        .unwrap();
    let root = row.id();
    let ids = row.content_ids();
    let mut host = row.into_host().unwrap();
    for id in &ids {
        assert!(toggle(&mut host, root, id.disclosure.unwrap()));
    }
    for scale in [1., 1.5] {
        let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), scale);
        let mut scene = Scene::default();
        host.render(
            &mut scene,
            &mut text,
            Rect::new(0., 0., 360., 100000.),
            Rect::new(0., 0., 360., 100000.),
            Theme::LIGHT.into(),
        )
        .unwrap();
        let fields = ids
            .iter()
            .filter(|id| id.kind != OutputContentKind::Tool)
            .map(|id| id.body)
            .chain(
                view(&host, root)
                    .tool_detail_sections()
                    .into_iter()
                    .map(|id| id.body),
            )
            .collect::<Vec<_>>();
        for body in fields {
            let field = host
                .get(body)
                .unwrap()
                .component::<TextFieldContent>()
                .unwrap();
            assert!(
                field.surface.outer.size.height <= 240. * scale + 0.5,
                "expanded {:?}: {} at scale {}",
                body,
                field.surface.outer.size.height,
                scale
            );
            assert_eq!(
                field.surface.scroll.scroll_policy(),
                (
                    tekes_ui_controls::ScrollPolicy::Disabled,
                    tekes_ui_controls::ScrollPolicy::Auto
                )
            );
            assert!(field.reading.is_some());
        }
    }
    let mut plain = item();
    plain.text = "正文仍属于外层文档\n".repeat(100);
    plain.content_blocks = Some(vec![block(
        0,
        Block::Text {
            text: plain.text.clone(),
        },
    )]);
    let mut row = OutputMessage::with_bounded_details(711, scope(), Default::default());
    row.update(&scope(), 1, 1, &output(&plain), None).unwrap();
    let id = row.content_ids()[0].body;
    let mut host = row.into_host().unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    host.render(
        &mut Scene::default(),
        &mut text,
        Rect::new(0., 0., 360., 100000.),
        Rect::new(0., 0., 360., 100000.),
        Theme::LIGHT.into(),
    )
    .unwrap();
    assert!(host.text_surface(id).unwrap().outer.size.height > 240.);
}
#[test]
fn bounded_detail_live_reasoning_minimum_ends_on_the_same_original_text_owner() {
    let mut message = item();
    message.text.clear();
    message.reasoning = "一行推理".into();
    message.content_blocks = Some(vec![block(
        0,
        Block::Reasoning {
            text: message.reasoning.clone(),
        },
    )]);
    let mut row = OutputMessage::with_bounded_details(712, scope(), Default::default());
    row.update(&scope(), 1, 1, &output(&message), None).unwrap();
    let root = row.id();
    let ids = row.content_ids()[0];
    let mut host = row.into_host().unwrap();
    assert!(toggle(&mut host, root, ids.disclosure.unwrap()));
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.4);
    let frame = Rect::new(0., 0., 400., 1000.);
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let owner = host.text_surface(ids.body).unwrap() as *const _ as usize;
    assert!(host.text_surface(ids.body).unwrap().outer.size.height >= 72. * 1.4 - 0.5);
    assert!(
        !host
            .text_surface(ids.body)
            .unwrap()
            .scroll
            .axis_enabled(true)
    );
    assert!(host.focus(Some(ids.body), true));
    host.get_mut(ids.body)
        .unwrap()
        .component_mut::<TextFieldContent>()
        .unwrap()
        .surface
        .input
        .select_range(0..2);
    let selection = host.text_surface(ids.body).unwrap().input.selection();
    message.reasoning_ended = true;
    message.is_provisional = false;
    view_mut(&mut host, root)
        .update(&scope(), 2, 1, &output(&message), None)
        .unwrap();
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::DARK.into(),
    )
    .unwrap();
    assert!(host.text_surface(ids.body).unwrap().outer.size.height < 72. * 1.4);
    assert_eq!(
        host.text_surface(ids.body).unwrap() as *const _ as usize,
        owner
    );
    assert_eq!(
        host.text_surface(ids.body).unwrap().input.selection(),
        selection
    );
    assert_eq!(host.focused(), Some(ids.body));
}
#[test]
fn bounded_detail_original_reading_anchor_survives_append_reflow_and_reopen() {
    let mut message = item();
    message.text = "后续正文".into();
    message.reasoning = "中文阅读位置 👩🏽‍💻 e\u{301}\n".repeat(100);
    message.content_blocks = Some(vec![
        block(
            0,
            Block::Reasoning {
                text: message.reasoning.clone(),
            },
        ),
        block(
            1,
            Block::Text {
                text: message.text.clone(),
            },
        ),
    ]);
    let mut row = OutputMessage::with_bounded_details(713, scope(), Default::default());
    row.update(&scope(), 1, 1, &output(&message), None).unwrap();
    let root = row.id();
    let ids = row.content_ids()[0];
    let mut host = row.into_host().unwrap();
    assert!(toggle(&mut host, root, ids.disclosure.unwrap()));
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let frame = Rect::new(0., 0., 360., 1000.);
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    assert!(host.focus(Some(ids.body), true));
    host.get_mut(ids.body)
        .unwrap()
        .component_mut::<TextFieldContent>()
        .unwrap()
        .surface
        .input
        .select_range(0..2);
    let owner = host.text_surface(ids.body).unwrap() as *const _ as usize;
    let selection = host.text_surface(ids.body).unwrap().input.selection();
    host.event(&ContentEvent::Scroll {
        position: host.get(ids.body).unwrap().visible().unwrap().center(),
        delta: Point::new(0., 160.),
        precise: true,
        begin: true,
        end: true,
        momentum: false,
        time: 1.,
    });
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let surface = host.text_surface(ids.body).unwrap();
    assert!(surface.scroll.offset.y > 0.);
    let anchor = surface.reflow_anchor.unwrap().utf16;
    assert!(anchor > 0);
    message
        .reasoning
        .push_str(&"后来追加，不打断上方阅读\n".repeat(100));
    message.content_blocks.as_mut().unwrap()[0].content = Block::Reasoning {
        text: message.reasoning.clone(),
    };
    view_mut(&mut host, root)
        .update(&scope(), 2, 1, &output(&message), None)
        .unwrap();
    for width in [360., 180., 360.] {
        let frame = Rect::new(0., 0., width, 1000.);
        host.render(
            &mut Scene::new(),
            &mut text,
            frame,
            frame,
            Theme::DARK.into(),
        )
        .unwrap();
        let surface = host.text_surface(ids.body).unwrap();
        assert_eq!(surface.reflow_anchor.unwrap().utf16, anchor);
        assert_eq!(surface.input.selection(), selection);
        assert_eq!(surface as *const _ as usize, owner);
        assert_eq!(host.focused(), Some(ids.body));
    }
    assert!(toggle(&mut host, root, ids.disclosure.unwrap()));
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    assert!(host.get(ids.body).is_none());
    message.reasoning.push_str("折叠期间追加");
    message.content_blocks.as_mut().unwrap()[0].content = Block::Reasoning {
        text: message.reasoning.clone(),
    };
    view_mut(&mut host, root)
        .update(&scope(), 3, 1, &output(&message), None)
        .unwrap();
    assert!(toggle(&mut host, root, ids.disclosure.unwrap()));
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let surface = host.text_surface(ids.body).unwrap();
    assert_eq!(surface as *const _ as usize, owner);
    assert_eq!(surface.reflow_anchor.unwrap().utf16, anchor);
    assert_eq!(surface.input.selection(), selection);
}
#[test]
fn default_detail_path_remains_inline_while_the_nested_reading_contract_is_pending() {
    let mut message = item();
    message.text.clear();
    message.reasoning = "默认可运行路径\n".repeat(100);
    message.content_blocks = Some(vec![block(
        0,
        Block::Reasoning {
            text: message.reasoning.clone(),
        },
    )]);
    let mut row = OutputMessage::new(714, scope());
    row.update(&scope(), 1, 1, &output(&message), None).unwrap();
    let root = row.id();
    let detail = row.content_ids()[0];
    let mut host = row.into_host().unwrap();
    assert!(toggle(&mut host, root, detail.disclosure.unwrap()));
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let frame = Rect::new(0., 0., 400., 10000.);
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let field = host
        .get(detail.body)
        .unwrap()
        .component::<TextFieldContent>()
        .unwrap();
    assert!(field.surface.outer.size.height > 240.);
    assert!(field.reading.is_none());
    assert_eq!(
        field.surface.scroll.scroll_policy(),
        (
            tekes_ui_controls::ScrollPolicy::Disabled,
            tekes_ui_controls::ScrollPolicy::Disabled
        )
    );
}

#[test]
fn tool_resolution_changes_retire_body_and_copy_ids_and_preserve_the_explicit_viewport_policy() {
    let mut message = item();
    message.content_blocks = Some(vec![block(
        0,
        Block::ToolCall {
            tool_id: "tool".into(),
        },
    )]);
    let mut row = OutputMessage::with_bounded_details(715, scope(), Default::default());
    row.update(&scope(), 1, 1, &output(&message), None).unwrap();
    let root = row.id();
    let missing = row.content_ids()[0];
    let mut host = row.into_host().unwrap();
    assert!(toggle(&mut host, root, missing.disclosure.unwrap()));
    let call = tool();
    view_mut(&mut host, root)
        .update(&scope(), 2, 1, &output(&message), Some(&section(&call)))
        .unwrap();
    let group = view(&host, root).content_ids()[0];
    assert_ne!(group.body, missing.body);
    assert!(host.get(missing.body).is_none());
    let fields = view(&host, root).tool_detail_sections();
    host.accessibility_action(fields[0].copy, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    assert_eq!(view(&host, root).take_copy_intents(&actions).len(), 1);
    view_mut(&mut host, root)
        .update(&scope(), 3, 1, &output(&message), None)
        .unwrap();
    let unresolved = view(&host, root).content_ids()[0];
    assert_ne!(unresolved.body, group.body);
    assert_ne!(unresolved.body, missing.body);
    assert!(host.get(group.body).is_none());
    assert!(view(&host, root).take_copy_intents(&actions).is_empty());
    let field = host
        .get(unresolved.body)
        .unwrap()
        .component::<TextFieldContent>()
        .unwrap();
    assert!(field.reading.is_some());
    assert_eq!(
        field.surface.scroll.scroll_policy(),
        (
            tekes_ui_controls::ScrollPolicy::Disabled,
            tekes_ui_controls::ScrollPolicy::Auto
        )
    );
    view_mut(&mut host, root)
        .update(&scope(), 4, 1, &output(&message), Some(&section(&call)))
        .unwrap();
    assert_ne!(
        view(&host, root).tool_detail_sections()[0].body,
        fields[0].body
    );
}

#[test]
fn embedded_tool_formatters_receive_the_original_scope_even_when_ids_repeat() {
    use tekes_agent_components_ui::{OutputMessageLabels, RunScope};
    use tekes_ui_controls::composition::ButtonContent;
    let mut message = item();
    message.content_blocks = Some(vec![block(
        0,
        Block::ToolCall {
            tool_id: "tool".into(),
        },
    )]);
    let call = tool();
    let labels = OutputMessageLabels {
        scoped_tool: Some(|scope, _| {
            (
                format!("owner:{}/{}", scope.endpoint_id, scope.session_id),
                String::new(),
            )
        }),
        tool_sections: Some(|scope, tool, _| {
            let mut details = tool.presentation_details(Default::default());
            details.sections[0].title = format!("Input @ {}", scope.endpoint_id);
            details
        }),
        ..Default::default()
    };
    for (endpoint, namespace) in [("one", 716), ("two", 717)] {
        let own = RunScope::main(endpoint, "same-session-id");
        let mut view = OutputMessage::with_labels(namespace, own.clone(), labels);
        view.update(&own, 1, 1, &output(&message), Some(&section(&call)))
            .unwrap();
        let root = view.id();
        let ids = view.content_ids()[0];
        let fields = view.tool_detail_sections();
        let mut host = view.into_host().unwrap();
        assert!(
            host.get(ids.disclosure.unwrap())
                .unwrap()
                .component::<ButtonContent>()
                .unwrap()
                .label
                .contains(&format!("owner:{endpoint}/same-session-id"))
        );
        assert!(toggle(&mut host, root, ids.disclosure.unwrap()));
        assert_eq!(
            host.get(fields[0].body)
                .unwrap()
                .component::<TextFieldContent>()
                .unwrap()
                .field
                .label,
            format!("Input @ {endpoint}")
        );
        let foreign = RunScope::main("foreign", "same-session-id");
        assert!(
            view_mut(&mut host, root)
                .update(&foreign, 2, 1, &output(&message), Some(&section(&call)))
                .is_err()
        );
    }
}

#[test]
fn embedded_continuous_tools_move_original_details_across_grouping_changes() {
    let mut a = tool();
    a.id = "a".into();
    let mut b = a.clone();
    b.id = "b".into();
    let section = TranscriptSection {
        id: "turn",
        turn: None,
        inputs: vec![],
        outputs: vec![],
        tools: vec![&a, &b],
    };
    let mut item = item();
    item.content_blocks = Some(vec![
        block(
            0,
            Block::ToolCall {
                tool_id: "a".into(),
            },
        ),
        block(
            1,
            Block::ToolCall {
                tool_id: "b".into(),
            },
        ),
    ]);
    let mut message = OutputMessage::new(65100, scope());
    message
        .update(&scope(), 1, 1, &output(&item), Some(&section))
        .unwrap();
    let root = message.id();
    let group = message.tool_run("a").unwrap().id();
    assert_eq!(message.tool_run("b").unwrap().id(), group);
    let old_ids = message.content_ids();
    let header = message.tool_run("a").unwrap().disclosure_id().unwrap();
    let mut host = message.into_host().unwrap();
    assert!(toggle(&mut host, root, header));
    assert!(toggle(&mut host, root, old_ids[0].disclosure.unwrap()));
    let bodies = view(&host, root).tool_detail_sections();
    let field = bodies[0].body;
    let owner = host.text_surface(field).unwrap() as *const _ as usize;
    item.content_blocks = Some(vec![
        block(
            0,
            Block::Text {
                text: "narration".into(),
            },
        ),
        block(
            1,
            Block::ToolCall {
                tool_id: "a".into(),
            },
        ),
        block(
            2,
            Block::ToolCall {
                tool_id: "b".into(),
            },
        ),
    ]);
    view_mut(&mut host, root)
        .update(&scope(), 2, 1, &output(&item), Some(&section))
        .unwrap();
    assert_eq!(view(&host, root).tool_run("a").unwrap().id(), group);
    assert!(view(&host, root).tool_run("a").unwrap().is_expanded());
    assert_eq!(view(&host, root).content_ids()[1], old_ids[0]);
    assert_eq!(
        host.text_surface(field).unwrap() as *const _ as usize,
        owner
    );
    item.content_blocks = Some(vec![
        block(
            0,
            Block::ToolCall {
                tool_id: "a".into(),
            },
        ),
        block(
            1,
            Block::Text {
                text: "break".into(),
            },
        ),
        block(
            2,
            Block::ToolCall {
                tool_id: "b".into(),
            },
        ),
    ]);
    view_mut(&mut host, root)
        .update(&scope(), 3, 1, &output(&item), Some(&section))
        .unwrap();
    assert!(view(&host, root).tool_run("a").is_none());
    assert_eq!(view(&host, root).content_ids()[0], old_ids[0]);
    assert_eq!(
        host.text_surface(field).unwrap() as *const _ as usize,
        owner
    );
    item.content_blocks = Some(vec![
        block(
            0,
            Block::ToolCall {
                tool_id: "a".into(),
            },
        ),
        block(
            1,
            Block::ToolCall {
                tool_id: "b".into(),
            },
        ),
        block(
            2,
            Block::ToolCall {
                tool_id: "a".into(),
            },
        ),
    ]);
    view_mut(&mut host, root)
        .update(&scope(), 4, 1, &output(&item), Some(&section))
        .unwrap();
    assert!(view(&host, root).tool_run("a").is_none());
    assert!(view(&host, root).tool_run("b").is_none());
    assert_eq!(view(&host, root).content_ids().len(), 3);
}

#[test]
fn structured_tool_headers_share_report_rules_across_scopes_and_repeated_references() {
    use std::sync::atomic::{AtomicU64, Ordering};
    use tekes_agent_components_ui::{
        OutputMessageLabels, ToolHeaderAdapter, TranscriptToolChangeCount,
        TranscriptToolHeaderRole, TranscriptToolReport, TranscriptToolReportWording,
        TranscriptToolRowInputs,
    };
    static REV: AtomicU64 = AtomicU64::new(1);
    let labels = OutputMessageLabels {
        tool_header: Some(ToolHeaderAdapter {
            revision: |_| REV.load(Ordering::SeqCst),
            inputs: |scope, _| TranscriptToolRowInputs {
                report: Some(TranscriptToolReport {
                    verb: "read".into(),
                    subject: Some(scope.endpoint_id.clone()),
                    result_count: None,
                    result_object: None,
                    note: None,
                    added_lines: Some(0),
                    removed_lines: (scope.endpoint_id == "one").then_some(0),
                }),
                workspace_change_count: Some(TranscriptToolChangeCount {
                    additions: 7,
                    deletions: 2,
                }),
            },
            name: None,
            stopped: || "Stopped".into(),
            report_wording: TranscriptToolReportWording {
                subject_phrase: |v, s| format!("{s}: {v}"),
                verb: str::to_owned,
                magnitude: |n, o| format!("{n} {o}"),
                join: |a, b| format!("{a} · {b}"),
            },
        }),
        // The structured header wins; the old custom detail hook stays supported.
        scoped_tool: Some(|_, _| ("legacy title".into(), "legacy body".into())),
        ..Default::default()
    };
    let mut item = item();
    item.content_blocks = Some(vec![
        block(
            0,
            Block::ToolCall {
                tool_id: "tool".into(),
            },
        ),
        block(
            1,
            Block::ToolCall {
                tool_id: "tool".into(),
            },
        ),
    ]);
    for (endpoint, namespace, delta) in [("one", 719, "+0 −0"), ("two", 720, "+7 −2")] {
        let own = RunScope::main(endpoint, "same-session");
        let mut call = tool();
        call.phase = "done".into();
        let mut message = OutputMessage::with_labels(namespace, own.clone(), labels);
        message
            .update(&own, 1, 1, &output(&item), Some(&section(&call)))
            .unwrap();
        assert_eq!(message.tool_headers().len(), 2);
        assert!(
            message
                .tool_headers()
                .iter()
                .all(|h| h.text == format!("{endpoint}: read    {delta}"))
        );
        let root = message.id();
        let ids = message.content_ids();
        let mut host = message.into_host().unwrap();
        assert!(toggle(&mut host, root, ids[0].disclosure.unwrap()));
        let field = view(&host, root).tool_detail_sections()[0].body;
        let owner = host.text_surface(field).unwrap() as *const _ as usize;
        let mut unversioned = call.clone();
        unversioned.result_meta = Some(json!({"unrelated":true}));
        REV.store(2, Ordering::SeqCst);
        assert!(
            view_mut(&mut host, root)
                .update(&own, 1, 1, &output(&item), Some(&section(&unversioned)))
                .is_err()
        );
        REV.store(1, Ordering::SeqCst);
        call.phase = "error".into();
        call.result = Some(json!({"text":"original failure"}));
        view_mut(&mut host, root)
            .update(&own, 2, 1, &output(&item), Some(&section(&call)))
            .unwrap();
        for header in view(&host, root).tool_headers() {
            assert_eq!(header.text, format!("Read · original failure    {delta}"));
            assert!(
                header
                    .spans
                    .iter()
                    .all(|s| s.role != TranscriptToolHeaderRole::Secondary)
            );
        }
        assert_eq!(
            host.text_surface(field).unwrap() as *const _ as usize,
            owner
        );
        for ids in &ids {
            let label = &host
                .get(ids.disclosure.unwrap())
                .unwrap()
                .component::<tekes_ui_controls::composition::ButtonContent>()
                .unwrap()
                .label;
            assert!(label.contains(delta));
            assert!(!label.contains("legacy title"));
        }
    }
}

use tekes_agent_presentation::{
    TranscriptThinkingFrame, TranscriptThinkingInput, TranscriptThinkingItem,
    TranscriptThinkingMode,
};
fn thinking_item() -> TranscriptItem {
    let mut source = item();
    source.text.clear();
    source.reasoning = "保留的推理中文".into();
    source.content_blocks = Some(vec![block(
        0,
        Block::Reasoning {
            text: source.reasoning.clone(),
        },
    )]);
    source
}
fn thinking_frame(
    source: &TranscriptItem,
    scope: &RunScope,
    version: (u64, u64, u64),
    mode: TranscriptThinkingMode,
    elapsed: u64,
    live: bool,
) -> TranscriptThinkingFrame {
    // Explicit owned fixture facts, not a production first-arrival adapter.
    let items = [TranscriptThinkingItem {
        id: &source.id,
        arrival_order: 7,
        response_frame_id: Some("fixture-response"),
        is_provisional: source.is_provisional,
        reasoning_has_ended: source.reasoning_ended,
        has_reasoning: true,
        has_visible_content: !source.text.is_empty(),
    }];
    TranscriptThinkingFrame::read(
        scope,
        &source.turn_id,
        version.0,
        version.1,
        version.2,
        &TranscriptThinkingInput {
            mode,
            is_settled: !live,
            is_live_presentation_owner: live,
            has_final_output: source.is_final,
            elapsed_seconds: elapsed,
            items: &items,
            calls: &[],
            narrations: &[],
        },
    )
    .unwrap()
}
#[test]
fn controlled_none_is_a_passive_status_without_a_reasoning_target_or_visible_prose() {
    let mut source = thinking_item();
    source.content_blocks.as_mut().unwrap().push(block(
        1,
        Block::Reasoning {
            text: "续段".into(),
        },
    ));
    source.reasoning.push_str("续段");
    let frame = thinking_frame(
        &source,
        &scope(),
        (1, 0, 1),
        TranscriptThinkingMode::None,
        1,
        true,
    );
    let mut row = OutputMessage::new(81, scope());
    row.update_with_thinking(1, 0, &output(&source), None, &frame)
        .unwrap();
    let ids = row.content_ids();
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0].kind, OutputContentKind::Status);
    assert!(ids[0].disclosure.is_none());
    let host = row.into_host().unwrap();
    assert_eq!(
        host.text_surface(ids[0].body).unwrap().input.text,
        "思考中.."
    );
    assert_eq!(source.reasoning, "保留的推理中文续段");
}
#[test]
fn controlled_setting_changes_retire_old_disclosures_without_forging_run_authority() {
    let source = thinking_item();
    let compact = thinking_frame(
        &source,
        &scope(),
        (1, 0, 1),
        TranscriptThinkingMode::Compact,
        1,
        true,
    );
    let hidden = thinking_frame(
        &source,
        &scope(),
        (1, 0, 2),
        TranscriptThinkingMode::None,
        1,
        true,
    );
    let mut row = OutputMessage::new(82, scope());
    row.update_with_thinking(1, 0, &output(&source), None, &compact)
        .unwrap();
    let root = row.id();
    let before = row.content_ids()[0];
    let mut host = row.into_host().unwrap();
    assert!(toggle(&mut host, root, before.disclosure.unwrap()));
    assert_eq!(
        host.text_surface(before.body).unwrap().input.text,
        source.reasoning
    );
    host.accessibility_action(before.disclosure.unwrap(), None, AccessibilityAction::Press);
    let delayed = host.drain_actions();
    view_mut(&mut host, root)
        .update_with_thinking(1, 0, &output(&source), None, &hidden)
        .unwrap();
    assert!(host.get(before.body).is_none());
    assert!(!view_mut(&mut host, root).handle_actions(&delayed));
    assert!(view(&host, root).take_copy_intents(&delayed).is_empty());
    assert!(
        view_mut(&mut host, root)
            .update_with_thinking(1, 0, &output(&source), None, &compact)
            .is_err()
    );
    assert!(
        view_mut(&mut host, root)
            .update(&scope(), 2, 0, &output(&source), None)
            .is_err()
    );
    let compact = thinking_frame(
        &source,
        &scope(),
        (1, 0, 3),
        TranscriptThinkingMode::Compact,
        1,
        true,
    );
    view_mut(&mut host, root)
        .update_with_thinking(1, 0, &output(&source), None, &compact)
        .unwrap();
    let after = view(&host, root).content_ids()[0];
    assert_ne!(after.disclosure, before.disclosure);
    assert_ne!(after.body, before.body);
    assert!(
        host.get(after.body).is_none(),
        "new disclosure starts collapsed"
    );
    assert!(!view_mut(&mut host, root).handle_actions(&delayed));
}
#[test]
fn display_tick_preserves_passive_owner_and_rejects_unversioned_or_foreign_frames() {
    let source = thinking_item();
    let first = thinking_frame(
        &source,
        &scope(),
        (1, 2, 1),
        TranscriptThinkingMode::None,
        1,
        true,
    );
    let mut row = OutputMessage::new(83, scope());
    row.update_with_thinking(1, 2, &output(&source), None, &first)
        .unwrap();
    let before = row.content_ids();
    let conflict = thinking_frame(
        &source,
        &scope(),
        (1, 2, 1),
        TranscriptThinkingMode::None,
        2,
        true,
    );
    assert!(
        row.update_with_thinking(1, 2, &output(&source), None, &conflict)
            .is_err()
    );
    let tick = thinking_frame(
        &source,
        &scope(),
        (1, 2, 2),
        TranscriptThinkingMode::None,
        2,
        true,
    );
    row.update_with_thinking(1, 2, &output(&source), None, &tick)
        .unwrap();
    assert_eq!(row.content_ids(), before);
    for frame in [
        thinking_frame(
            &source,
            &RunScope::main("fixture", "b"),
            (1, 2, 3),
            TranscriptThinkingMode::Compact,
            2,
            true,
        ),
        thinking_frame(
            &source,
            &scope(),
            (2, 2, 3),
            TranscriptThinkingMode::Compact,
            2,
            true,
        ),
        thinking_frame(
            &source,
            &scope(),
            (1, 3, 3),
            TranscriptThinkingMode::Compact,
            2,
            true,
        ),
    ] {
        assert!(
            row.update_with_thinking(1, 2, &output(&source), None, &frame)
                .is_err()
        );
    }
    assert_eq!(row.content_ids(), before);
}
#[test]
fn same_publication_source_change_cannot_hide_inside_a_new_display_revision() {
    let source = thinking_item();
    let first = thinking_frame(
        &source,
        &scope(),
        (1, 0, 1),
        TranscriptThinkingMode::None,
        1,
        true,
    );
    let mut row = OutputMessage::new(84, scope());
    row.update_with_thinking(1, 0, &output(&source), None, &first)
        .unwrap();
    let before = row.content_ids();
    let bad_source = thinking_frame(
        &source,
        &scope(),
        (1, 0, 2),
        TranscriptThinkingMode::Compact,
        1,
        false,
    );
    assert!(
        row.update_with_thinking(1, 0, &output(&source), None, &bad_source)
            .is_err()
    );
    let mut changed = source.clone();
    changed.reasoning.push_str("unversioned");
    let display = thinking_frame(
        &source,
        &scope(),
        (1, 0, 2),
        TranscriptThinkingMode::Compact,
        1,
        true,
    );
    assert!(
        row.update_with_thinking(1, 0, &output(&changed), None, &display)
            .is_err()
    );
    assert_eq!(row.content_ids(), before);
}
#[test]
fn settlement_reopens_retained_reasoning_and_epoch_change_resets_display_revision() {
    let mut source = thinking_item();
    let frame = thinking_frame(
        &source,
        &scope(),
        (1, 0, 8),
        TranscriptThinkingMode::None,
        1,
        true,
    );
    let mut row = OutputMessage::new(85, scope());
    row.update_with_thinking(1, 0, &output(&source), None, &frame)
        .unwrap();
    source.is_provisional = false;
    source.reasoning_ended = true;
    let settled = thinking_frame(
        &source,
        &scope(),
        (2, 0, 9),
        TranscriptThinkingMode::None,
        1,
        false,
    );
    row.update_with_thinking(2, 0, &output(&source), None, &settled)
        .unwrap();
    let root = row.id();
    let ids = row.content_ids()[0];
    let mut host = row.into_host().unwrap();
    assert!(toggle(&mut host, root, ids.disclosure.unwrap()));
    assert_eq!(
        host.text_surface(ids.body).unwrap().input.text,
        "保留的推理中文"
    );
    let reconnected = thinking_frame(
        &source,
        &scope(),
        (3, 1, 1),
        TranscriptThinkingMode::None,
        1,
        false,
    );
    view_mut(&mut host, root)
        .update_with_thinking(3, 1, &output(&source), None, &reconnected)
        .unwrap();
    assert_eq!(view(&host, root).content_ids()[0], ids);
    assert_eq!(
        host.text_surface(ids.body).unwrap().input.text,
        "保留的推理中文"
    );
}

#[test]
fn missing_reasoning_source_is_rejected_and_replaced_thinking_has_no_empty_fallback_row() {
    use tekes_agent_presentation::TranscriptThinkingCall;
    let source = thinking_item();
    let mut row = OutputMessage::new(86, scope());
    let mut facts = TranscriptThinkingInput {
        mode: TranscriptThinkingMode::None,
        is_settled: false,
        is_live_presentation_owner: true,
        has_final_output: false,
        elapsed_seconds: 1,
        items: &[],
        calls: &[],
        narrations: &[],
    };
    let missing = TranscriptThinkingFrame::read(&scope(), "turn", 1, 0, 1, &facts).unwrap();
    assert!(
        row.update_with_thinking(1, 0, &output(&source), None, &missing)
            .is_err()
    );
    assert!(row.content_ids().is_empty());
    let items = [TranscriptThinkingItem {
        id: &source.id,
        arrival_order: 7,
        response_frame_id: Some("frame"),
        is_provisional: true,
        reasoning_has_ended: true,
        has_reasoning: true,
        has_visible_content: false,
    }];
    let calls = [TranscriptThinkingCall {
        group_id: "next-frame",
        arrival_order: 8,
    }];
    facts.items = &items;
    facts.calls = &calls;
    let replaced = TranscriptThinkingFrame::read(&scope(), "turn", 1, 0, 1, &facts).unwrap();
    row.update_with_thinking(1, 0, &output(&source), None, &replaced)
        .unwrap();
    assert!(
        row.content_ids().is_empty(),
        "hidden reasoning must not become missing-content copy"
    );
}
#[test]
fn compact_reasoning_end_keeps_disclosure_editor_selection_and_focus() {
    let mut source = thinking_item();
    let frame = thinking_frame(
        &source,
        &scope(),
        (1, 0, 1),
        TranscriptThinkingMode::Compact,
        1,
        true,
    );
    let mut row = OutputMessage::new(87, scope());
    row.update_with_thinking(1, 0, &output(&source), None, &frame)
        .unwrap();
    let root = row.id();
    let ids = row.content_ids()[0];
    let mut host = row.into_host().unwrap();
    assert!(toggle(&mut host, root, ids.disclosure.unwrap()));
    assert!(host.focus(Some(ids.body), true));
    host.get_mut(ids.body)
        .unwrap()
        .component_mut::<TextFieldContent>()
        .unwrap()
        .surface
        .input
        .select_range(0..2);
    source.reasoning_ended = true;
    let ended = thinking_frame(
        &source,
        &scope(),
        (2, 0, 1),
        TranscriptThinkingMode::Compact,
        2,
        true,
    );
    view_mut(&mut host, root)
        .update_with_thinking(2, 0, &output(&source), None, &ended)
        .unwrap();
    assert_eq!(view(&host, root).content_ids()[0], ids);
    assert_eq!(host.focused(), Some(ids.body));
    assert_eq!(host.text_surface(ids.body).unwrap().input.selection(), 0..2);
    assert_eq!(
        host.get(ids.disclosure.unwrap())
            .unwrap()
            .component::<tekes_ui_controls::composition::ButtonContent>()
            .unwrap()
            .label,
        "收起 · 思考过程"
    );
}

#[test]
fn a_new_thinking_revision_cannot_smuggle_changed_embedded_tool_details() {
    use tekes_agent_presentation::TranscriptThinkingCall;
    let mut source = thinking_item();
    source.content_blocks.as_mut().unwrap().push(block(
        1,
        Block::ToolCall {
            tool_id: "tool".into(),
        },
    ));
    let mut tool = tool();
    let items = [TranscriptThinkingItem {
        id: &source.id,
        arrival_order: 7,
        response_frame_id: Some("frame"),
        is_provisional: true,
        reasoning_has_ended: false,
        has_reasoning: true,
        has_visible_content: false,
    }];
    let calls = [TranscriptThinkingCall {
        group_id: "frame",
        arrival_order: 8,
    }];
    let mut facts = TranscriptThinkingInput {
        mode: TranscriptThinkingMode::Compact,
        is_settled: false,
        is_live_presentation_owner: true,
        has_final_output: false,
        elapsed_seconds: 1,
        items: &items,
        calls: &calls,
        narrations: &[],
    };
    let compact = TranscriptThinkingFrame::read(&scope(), "turn", 1, 0, 1, &facts).unwrap();
    facts.mode = TranscriptThinkingMode::None;
    let hidden = TranscriptThinkingFrame::read(&scope(), "turn", 1, 0, 2, &facts).unwrap();
    let mut row = OutputMessage::new(88, scope());
    row.update_with_thinking(1, 0, &output(&source), Some(&section(&tool)), &compact)
        .unwrap();
    let before = row.content_ids();
    tool.arguments = json!({"path":"unversioned-replacement"});
    assert!(
        row.update_with_thinking(1, 0, &output(&source), Some(&section(&tool)), &hidden)
            .is_err()
    );
    assert_eq!(row.content_ids(), before);
}
