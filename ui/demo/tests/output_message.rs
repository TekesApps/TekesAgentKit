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
    let details = &host.text_surface(ids[2].body).unwrap().input.text;
    assert!(details.contains("a.rs") && details.contains("结果\n尚未回传"));
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
        host.text_surface(ids[1].body)
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
        host.text_surface(ids[1].body)
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
