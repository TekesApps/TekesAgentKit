use super::*;
use tekes_ui_controls::extended::{Axis, Stack};

pub(super) fn call() -> TranscriptToolCall {
    TranscriptToolCall {
        id: "owned-call".into(),
        turn_id: "owned-turn".into(),
        name: "read".into(),
        phase: "doing".into(),
        arguments: "\"原输入\\n\"".parse().unwrap(),
        result: Some(r#"{"text":"中文输出\n"}"#.parse().unwrap()),
        call_view: Some(r#"{"view":{"title":"原调用"}}"#.parse().unwrap()),
        result_view: None,
        result_meta: None,
        result_error: None,
        source_position: None,
        result_position: None,
    }
}
fn activity(host: &ContentHost, root: ComponentId) -> &ToolActivity {
    host.get(root).unwrap().component::<ToolActivity>().unwrap()
}
fn activity_mut(host: &mut ContentHost, root: ComponentId) -> &mut ToolActivity {
    host.get_mut(root)
        .unwrap()
        .component_mut::<ToolActivity>()
        .unwrap()
}
fn activate(host: &mut ContentHost, root: ComponentId, target: ComponentId) -> Vec<ContentAction> {
    host.accessibility_action(target, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    activity_mut(host, root).handle_actions(&actions);
    actions
}
fn opened(namespace: u32) -> (ContentHost, ComponentId, RunScope, TranscriptToolCall) {
    let scope = RunScope::main("second-product", "tool-sections");
    let call = call();
    let mut view = ToolActivity::new(namespace, scope.clone(), Default::default());
    view.update(&scope, 1, 1, &call, ToolActivityPlacement::Call)
        .unwrap();
    let root = view.id();
    let toggle = view.content_ids().unwrap().disclosure.unwrap();
    let mut host = view.into_host().unwrap();
    assert!(activity(&host, root).copy_action_ids().is_empty());
    activate(&mut host, root, toggle);
    (host, root, scope, call)
}
#[test]
fn independent_tool_sections_have_distinct_owners_and_exact_copy_sources() {
    let (mut host, root, scope, call) = opened(64401);
    let ids = activity(&host, root).tool_detail_sections();
    assert_eq!(
        ids.iter().map(|id| id.kind).collect::<Vec<_>>(),
        vec![
            TranscriptToolDetailKind::Input,
            TranscriptToolDetailKind::Output,
            TranscriptToolDetailKind::CallView
        ]
    );
    assert_ne!(ids[0].body, ids[1].body);
    assert_eq!(host.text_surface(ids[0].body).unwrap().input.text, "原输入");
    assert_eq!(
        host.text_surface(ids[1].body).unwrap().input.text,
        "中文输出"
    );
    assert!(ids[0].divider.is_none());
    for (width, scale, backing_scale, theme) in [
        (640., 1., 1., Theme::LIGHT),
        (260., 1.4, 2., Theme::DARK),
        (640., 1., 2., Theme::LIGHT),
    ] {
        let mut text = Text::new(scale, true);
        text.backing_scale = backing_scale;
        let frame = Rect::new(0., 0., width, 10000.);
        let mut scene = Scene::new();
        host.render(&mut scene, &mut text, frame, frame, theme.into())
            .unwrap();
        for id in &ids {
            let header = host.get(id.header).unwrap().frame();
            let body = host.get(id.body).unwrap().frame();
            assert!((body.min_x() - frame.min_x()).abs() < 0.5);
            assert!(header.max_y() <= body.min_y() + 0.5);
            assert!(host.get(id.copy).unwrap().frame().max_x() <= frame.max_x() + 0.5);
        }
        for (previous, section) in ids.windows(2).map(|pair| (&pair[0], &pair[1])) {
            let divider = host
                .get(section.divider.expect("non-first tool section divider"))
                .unwrap();
            assert!(divider.component::<SeparatorContent>().is_some());
            let line = divider.frame();
            assert!(line.size.width > 0. && line.size.height > 0.);
            assert!(line.min_x() >= frame.min_x() && line.max_x() <= frame.max_x());
            assert!(host.get(previous.body).unwrap().frame().max_y() <= line.min_y());
            assert!(line.max_y() <= host.get(section.header).unwrap().frame().min_y());
            let mut expected = Scene::new();
            tekes_ui_controls::layout_view::SeparatorView::default().render_at_scale(
                &mut expected,
                line,
                Axis::Horizontal,
                Some(frame),
                theme.into(),
                backing_scale,
            );
            let ink: Vec<_> = expected
                .primitives()
                .iter()
                .filter(|p| matches!(p, tekes_ui_scene::Primitive::Quad(_)))
                .collect();
            assert!(!ink.is_empty());
            assert!(
                ink.into_iter().all(|p| scene.primitives().contains(p)),
                "real ToolInput/Output must paint the SDK separator with its theme and backing scale"
            );
        }
        assert!(
            host.get(ids[0].body).unwrap().frame().max_y()
                <= host.get(ids[1].header).unwrap().frame().min_y()
        );
    }
    let actions = activate(&mut host, root, ids[1].copy);
    let copies = activity(&host, root).take_copy_intents(&actions);
    assert_eq!(copies.len(), 1);
    assert_eq!(copies[0].scope(), &scope);
    assert_eq!(
        copies[0].source(),
        &TranscriptCopySource::ToolDetail {
            tool_id: call.id,
            turn_id: call.turn_id,
            section: TranscriptToolDetailKind::Output
        }
    );
    assert_eq!(copies[0].text(), "中文输出\n");
    assert!(copies[0].message().is_none());
    assert!(copies[0].claim());
    assert!(!copies[0].clone().claim());
}
#[test]
fn section_append_reconnect_and_view_reappearance_retain_text_but_retire_copy_targets() {
    let (mut host, root, scope, mut call) = opened(64402);
    let ids = activity(&host, root).tool_detail_sections();
    let owner = host.text_surface(ids[1].body).unwrap() as *const _ as usize;
    assert!(host.focus(Some(ids[0].body), true));
    host.get_mut(ids[0].body)
        .unwrap()
        .component_mut::<TextFieldContent>()
        .unwrap()
        .surface
        .input
        .select_range(0..1);
    let actions = activate(&mut host, root, ids[1].copy);
    let captured = activity(&host, root)
        .take_copy_intents(&actions)
        .pop()
        .unwrap();
    call.result = Some(r#"{"text":"新的输出\n"}"#.parse().unwrap());
    activity_mut(&mut host, root)
        .update(&scope, 2, 1, &call, ToolActivityPlacement::Call)
        .unwrap();
    let next = activity(&host, root).tool_detail_sections();
    assert_eq!(ids[0], next[0]);
    assert_eq!(ids[1].body, next[1].body);
    assert_ne!(ids[1].copy, next[1].copy);
    assert_eq!(
        host.text_surface(next[1].body).unwrap() as *const _ as usize,
        owner
    );
    assert_eq!(host.focused(), Some(ids[0].body));
    assert_eq!(
        host.text_surface(ids[0].body).unwrap().input.selection(),
        0..1
    );
    assert!(activity(&host, root).take_copy_intents(&actions).is_empty());
    assert_eq!(captured.text(), "中文输出\n");
    let original_view = next[2];
    call.call_view = None;
    activity_mut(&mut host, root)
        .update(&scope, 3, 2, &call, ToolActivityPlacement::Call)
        .unwrap();
    assert!(host.get(original_view.body).is_none());
    assert_ne!(
        activity(&host, root).tool_detail_sections()[0].copy,
        ids[0].copy
    );
    call.call_view = Some(r#"{"view":{"title":"再次回传"}}"#.parse().unwrap());
    activity_mut(&mut host, root)
        .update(&scope, 4, 2, &call, ToolActivityPlacement::Call)
        .unwrap();
    assert_ne!(
        activity(&host, root).tool_detail_sections()[2].body,
        original_view.body
    );
    let toggle = activity(&host, root)
        .content_ids()
        .unwrap()
        .disclosure
        .unwrap();
    activate(&mut host, root, toggle);
    assert!(activity(&host, root).copy_action_ids().is_empty());
    assert!(activity(&host, root).take_copy_intents(&actions).is_empty());
    let before = activity(&host, root).tool_detail_sections();
    assert!(
        activity_mut(&mut host, root)
            .update(&scope, 3, 2, &call, ToolActivityPlacement::Call)
            .is_err()
    );
    assert_eq!(activity(&host, root).tool_detail_sections(), before);
}
#[test]
fn original_section_copy_and_background_append_do_not_touch_composer_marked_owner() {
    let scope = RunScope::main("second-product", "ime");
    let mut call = call();
    let mut view = ToolActivity::new(64403, scope.clone(), Default::default());
    view.update(&scope, 1, 1, &call, ToolActivityPlacement::Call)
        .unwrap();
    let root = view.id();
    let toggle = view.content_ids().unwrap().disclosure.unwrap();
    let composer = tekes_agent_ui_consumer_check::new_composer(64404);
    let composer_root = composer.ids().root;
    let editor = composer.ids().editor;
    let mut host = ContentHost::new(Content::new(
        ComponentId(17),
        StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: 0.,
            },
            flexible_child: None,
            children: vec![
                Content::new(composer_root, composer),
                Content::new(root, view),
            ],
        },
    ))
    .unwrap();
    host.set_action_events(true);
    activate(&mut host, root, toggle);
    let target = activity(&host, root).tool_detail_sections()[0].copy;
    let mut text = Text::new(1., true);
    let frame = Rect::new(0., 0., 600., 3000.);
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
        text: "正在中文组字".into(),
        selection: 1..3,
        clauses: vec![],
        replacement: None,
    });
    let owner = host.text_surface(editor).unwrap() as *const _ as usize;
    let baseline = host.text_surface(editor).unwrap().input.external_baseline();
    let point = host.get(target).unwrap().visible().unwrap().center();
    host.event(&ContentEvent::PointerDown(point));
    host.event(&ContentEvent::PointerUp(point));
    let actions = host.drain_actions();
    assert_eq!(activity(&host, root).take_copy_intents(&actions).len(), 1);
    assert!(
        host.get_mut(composer_root)
            .unwrap()
            .component_mut::<Composer>()
            .unwrap()
            .take_intents(&actions)
            .is_empty()
    );
    call.result = Some(r#"{"text":"后台追加"}"#.parse().unwrap());
    activity_mut(&mut host, root)
        .update(&scope, 2, 1, &call, ToolActivityPlacement::Call)
        .unwrap();
    for width in [600., 280., 600.] {
        let frame = Rect::new(0., 0., width, 3000.);
        host.render(
            &mut Scene::new(),
            &mut text,
            frame,
            frame,
            Theme::DARK.into(),
        )
        .unwrap();
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
}
#[test]
fn invalid_section_callback_is_rejected_before_mounting_and_legacy_text_optout_is_explicit() {
    fn duplicate(
        _: &RunScope,
        call: &TranscriptToolCall,
        _: OutputMessageLabels,
    ) -> TranscriptToolDetails {
        let mut facts = call.presentation_details(Default::default());
        facts.sections.push(facts.sections[0].clone());
        facts
    }
    let scope = RunScope::main("second-product", "invalid");
    let call = call();
    let mut labels = ToolActivityLabels::default();
    labels.details.tool_sections = Some(duplicate);
    let mut view = ToolActivity::new(64405, scope.clone(), labels);
    assert!(
        view.update(&scope, 1, 1, &call, ToolActivityPlacement::Call)
            .is_err()
    );
    assert!(view.content_ids().is_none());
    let mut labels = ToolActivityLabels::default();
    labels.details.tool_sections = None;
    labels.details.tool = |_| ("Host title".into(), "Host legacy body".into());
    let mut view = ToolActivity::new(64406, scope.clone(), labels);
    view.update(&scope, 1, 1, &call, ToolActivityPlacement::Call)
        .unwrap();
    let root = view.id();
    let ids = view.content_ids().unwrap();
    let mut host = view.into_host().unwrap();
    activate(&mut host, root, ids.disclosure.unwrap());
    assert_eq!(
        host.text_surface(ids.body).unwrap().input.text,
        "Host legacy body"
    );
    assert!(activity(&host, root).tool_detail_sections().is_empty());
}
