use super::*;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use tekes_ui_controls::extended::{Axis, Stack};

#[test]
fn explicit_report_inputs_are_captured_scoped_and_preserve_composer_and_detail_owners() {
    static REV: AtomicU64 = AtomicU64::new(1);
    static PARTIAL: AtomicBool = AtomicBool::new(false);
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    fn inputs(scope: &RunScope, _: &TranscriptToolCall) -> TranscriptToolRowInputs {
        CALLS.fetch_add(1, Ordering::SeqCst);
        TranscriptToolRowInputs {
            report: Some(TranscriptToolReport {
                verb: "read".into(),
                subject: Some(format!("{}.rs", scope.endpoint_id)),
                result_count: None,
                result_object: None,
                note: None,
                added_lines: Some(0),
                removed_lines: (!PARTIAL.load(Ordering::SeqCst)).then_some(0),
            }),
            workspace_change_count: Some(TranscriptToolChangeCount {
                additions: if scope.endpoint_id == "second-product" {
                    9
                } else {
                    99
                },
                deletions: 10,
            }),
        }
    }
    let labels = ToolActivityLabels {
        details: OutputMessageLabels {
            tool_header: Some(ToolHeaderAdapter {
                revision: |_| REV.load(Ordering::SeqCst),
                inputs,
                name: None,
                stopped: || "Stopped".into(),
                report_wording: TranscriptToolReportWording {
                    subject_phrase: |v, s| format!("{s}: {v}"),
                    verb: str::to_owned,
                    magnitude: |n, o| format!("{n} {o}"),
                    join: |a, b| format!("{a} · {b}"),
                },
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    let scope = RunScope::main("second-product", "header");
    let mut tool = super::tool_sections::call();
    tool.phase = "done".into();
    tool.result_meta = Some(r#"{"addedLines":999,"removedLines":999}"#.parse().unwrap());
    let mut view = ToolActivity::new(65500, scope.clone(), labels.clone());
    view.update(&scope, 1, 1, &tool, ToolActivityPlacement::Call)
        .unwrap();
    assert_eq!(
        view.tool_header().unwrap().text,
        "second-product.rs: read    +0 −0"
    );
    let root = view.id();
    let button = view.content_ids().unwrap().disclosure.unwrap();
    let composer = tekes_agent_ui_consumer_check::new_composer(65501);
    let editor = composer.ids().editor;
    let composer_id = composer.ids().root;
    let mut host = ContentHost::new(Content::new(
        ComponentId(19),
        StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: 0.,
            },
            flexible_child: None,
            children: vec![
                Content::new(composer_id, composer),
                Content::new(root, view),
            ],
        },
    ))
    .unwrap();
    host.set_action_events(true);
    host.accessibility_action(button, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    host.get_mut(root)
        .unwrap()
        .component_mut::<ToolActivity>()
        .unwrap()
        .handle_actions(&actions);
    let detail = host
        .get(root)
        .unwrap()
        .component::<ToolActivity>()
        .unwrap()
        .tool_detail_sections()[0]
        .body;
    let detail_owner = host.text_surface(detail).unwrap() as *const _ as usize;
    let frame = Rect::new(0., 0., 600., 3000.);
    let mut text = Text::new(1.2, true);
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    assert!(host.focus(Some(editor), true));
    host.event(&ContentEvent::Preedit {
        text: "中文组字".into(),
        selection: 1..3,
        clauses: vec![],
        replacement: None,
    });
    let owner = host.text_surface(editor).unwrap() as *const _ as usize;
    let baseline = host.text_surface(editor).unwrap().input.external_baseline();
    PARTIAL.store(true, Ordering::SeqCst);
    let before = CALLS.load(Ordering::SeqCst);
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<ToolActivity>()
            .unwrap()
            .update(
                &RunScope::main("foreign", "header"),
                2,
                1,
                &tool,
                ToolActivityPlacement::Call
            )
            .is_err()
    );
    assert_eq!(CALLS.load(Ordering::SeqCst), before);
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<ToolActivity>()
            .unwrap()
            .update(&scope, 1, 1, &tool, ToolActivityPlacement::Call)
            .is_err()
    );
    assert_eq!(
        host.get(root)
            .unwrap()
            .component::<ToolActivity>()
            .unwrap()
            .tool_header()
            .unwrap()
            .text,
        "second-product.rs: read    +0 −0"
    );
    REV.store(2, Ordering::SeqCst);
    host.get_mut(root)
        .unwrap()
        .component_mut::<ToolActivity>()
        .unwrap()
        .update(&scope, 1, 1, &tool, ToolActivityPlacement::Call)
        .unwrap();
    for (width, theme) in [(280., Theme::DARK), (600., Theme::LIGHT)] {
        let frame = Rect::new(0., 0., width, 3000.);
        host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
            .unwrap();
        let view = host.get(root).unwrap().component::<ToolActivity>().unwrap();
        assert_eq!(
            view.tool_header().unwrap().text,
            "second-product.rs: read    +9 −10"
        );
        assert!(
            host.get(button)
                .unwrap()
                .component::<ButtonContent>()
                .unwrap()
                .label
                .contains("+9 −10")
        );
        assert_eq!(
            host.text_surface(detail).unwrap() as *const _ as usize,
            detail_owner
        );
        assert_eq!(host.focused(), Some(editor));
        assert_eq!(
            host.text_surface(editor).unwrap() as *const _ as usize,
            owner
        );
        assert_eq!(
            host.text_surface(editor).unwrap().input.external_baseline(),
            baseline
        );
    }
    let actions = host.drain_actions();
    assert!(
        host.get_mut(composer_id)
            .unwrap()
            .component_mut::<Composer>()
            .unwrap()
            .take_intents(&actions)
            .is_empty()
    );
    REV.store(1, Ordering::SeqCst);
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<ToolActivity>()
            .unwrap()
            .update(&scope, 1, 1, &tool, ToolActivityPlacement::Call)
            .is_err()
    );
    REV.store(2, Ordering::SeqCst);
    let other = RunScope::main("other", "header");
    let mut other_view = ToolActivity::new(65502, other.clone(), labels);
    other_view
        .update(&other, 1, 1, &tool, ToolActivityPlacement::Call)
        .unwrap();
    assert_eq!(
        other_view.tool_header().unwrap().text,
        "other.rs: read    +99 −10"
    );
}
