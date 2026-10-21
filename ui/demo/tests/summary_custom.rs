use tekes_agent_components_ui::*;
use tekes_ui_controls::{
    composition::*,
    extended::{Axis, Stack},
};
fn input(revision: u64) -> SummaryContextInput {
    SummaryContextInput {
        revision,
        owner: SummaryOwner::Workspace {
            id: "host-workspace".into(),
        },
        facts: None,
        ready: true,
        host_sections: vec![SummarySection {
            id: "repository".into(),
            rows: vec![SummaryRow {
                id: "status".into(),
                title: "仓库".into(),
                value: serde_json::json!("正在读取"),
                action: None,
            }],
        }],
        custom_rows: vec![SummaryCustomRowSpec {
            section: "repository".into(),
            row: "status".into(),
            content_key: "git-controls-v1".into(),
        }],
    }
}
fn view(host: &ContentHost, root: ComponentId) -> &SummaryView {
    host.get(root).unwrap().component().unwrap()
}
fn view_mut(host: &mut ContentHost, root: ComponentId) -> &mut SummaryView {
    host.get_mut(root).unwrap().component_mut().unwrap()
}
fn token(host: &ContentHost, root: ComponentId) -> SummaryCustomRowToken {
    view(host, root)
        .custom_row_token("repository", "status")
        .unwrap()
}
#[test]
fn original_host_control_survives_publications_and_disclosure_aba_retires_callbacks() {
    let mut model = input(1);
    let mut summary = SummaryView::new(40400, Default::default());
    summary.update_context(&model).unwrap();
    let root = summary.id();
    let mut host = summary.into_host().unwrap();
    let first = token(&host, root);
    assert!(
        view(&host, root)
            .passive_text_id("repository", "status")
            .is_none()
    );
    let button = ComponentId(40401u64 << 32);
    tekes_agent_ui_consumer_check::install_summary_host_content(
        &mut host,
        root,
        &first,
        Content::new(button, ButtonContent::new("原宿主控件")),
    )
    .unwrap();
    host.accessibility_action(button, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    assert_eq!(actions.len(), 1);
    view(&host, root)
        .validate_custom_row_action(&first)
        .unwrap();
    assert!(
        view(&host, root).take_intents(&actions).is_empty(),
        "host action must not become a session intent"
    );
    model.revision += 1;
    model.host_sections[0].rows[0].value = serde_json::json!("新快照");
    view_mut(&mut host, root).update_context(&model).unwrap();
    assert!(
        view(&host, root)
            .validate_custom_row_action(&first)
            .is_err()
    );
    let refreshed = token(&host, root);
    assert_eq!(refreshed.row_id(), first.row_id());
    assert_eq!(
        view(&host, root)
            .custom_row_component::<ButtonContent>(&refreshed)
            .unwrap()
            .label,
        "原宿主控件"
    );
    let header = view(&host, root).section_header_id("repository").unwrap();
    host.accessibility_action(header, None, AccessibilityAction::Press);
    let closed = token(&host, root);
    assert!(
        view(&host, root)
            .validate_custom_row_action(&closed)
            .is_err()
    );
    assert!(!host.semantics().iter().any(|node| node.id == button));
    host.accessibility_action(header, None, AccessibilityAction::Press);
    assert!(
        view(&host, root)
            .validate_custom_row_action(&refreshed)
            .is_err(),
        "close/open must not resurrect a held callback"
    );
    assert!(
        view(&host, root)
            .validate_custom_row_action(&closed)
            .is_err()
    );
    let reopened = token(&host, root);
    view(&host, root)
        .validate_custom_row_action(&reopened)
        .unwrap();
    assert_eq!(
        view(&host, root)
            .custom_row_component::<ButtonContent>(&reopened)
            .unwrap()
            .label,
        "原宿主控件"
    );
    model.revision += 1;
    model.custom_rows[0].content_key = "different-product-control".into();
    assert!(view_mut(&mut host, root).update_context(&model).is_err());
    publish_summary_context_update(&mut host, root, &model).unwrap();
    assert!(
        view(&host, root)
            .validate_custom_row_action(&reopened)
            .is_err()
    );
    assert_ne!(token(&host, root).row_id(), first.row_id());
    assert!(host.get(button).is_none());
}
#[test]
fn invalid_specs_and_sdk_duplicate_ids_leave_the_original_slot_unmodified() {
    let model = input(1);
    let mut summary = SummaryView::new(40402, Default::default());
    summary.update_context(&model).unwrap();
    let root = summary.id();
    let mut host = summary.into_host().unwrap();
    let original = token(&host, root);
    let mut invalid = model.clone();
    invalid.revision += 1;
    invalid.custom_rows.push(invalid.custom_rows[0].clone());
    assert!(view_mut(&mut host, root).update_context(&invalid).is_err());
    invalid.custom_rows.truncate(1);
    invalid.custom_rows[0].row = "missing".into();
    assert!(view_mut(&mut host, root).update_context(&invalid).is_err());
    assert_eq!(view(&host, root).displayed_context_input(), Some(&model));
    let header = view(&host, root).section_header_id("repository").unwrap();
    let duplicate = Content::new(
        ComponentId(40403u64 << 32),
        StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: 0.,
            },
            flexible_child: None,
            children: vec![Content::new(header, TextContent::new("重复"))],
        },
    );
    assert_eq!(
        install_summary_custom_row(&mut host, root, &original, duplicate),
        Err(SummaryCustomRowError::Composition(
            CompositionError::DuplicateId(header)
        ))
    );
    assert!(
        view(&host, root)
            .custom_row_component::<ButtonContent>(&original)
            .is_err()
    );
    assert_eq!(view(&host, root).displayed_context_input(), Some(&model));
    let button = ComponentId(40404u64 << 32);
    install_summary_custom_row(
        &mut host,
        root,
        &original,
        Content::new(button, ButtonContent::new("保留")),
    )
    .unwrap();
    assert!(
        install_summary_custom_row(
            &mut host,
            root,
            &original,
            Content::new(ComponentId(button.0 + 1), ButtonContent::new("替换"))
        )
        .is_err()
    );
    assert_eq!(
        view(&host, root)
            .custom_row_component::<ButtonContent>(&original)
            .unwrap()
            .label,
        "保留"
    );
    let mut foreign = model.clone();
    foreign.revision += 1;
    foreign.owner = SummaryOwner::Workspace { id: "other".into() };
    publish_summary_context_update(&mut host, root, &foreign).unwrap();
    assert!(
        install_summary_custom_row(
            &mut host,
            root,
            &original,
            Content::new(ComponentId(button.0 + 2), ButtonContent::new("晚到"))
        )
        .is_err()
    );
}
#[test]
fn identical_numeric_ids_in_another_view_and_original_host_remount_cannot_reuse_a_token() {
    let model = input(1);
    let mut a = SummaryView::new(40405, Default::default());
    a.update_context(&model).unwrap();
    let root = a.id();
    let before = a.custom_row_token("repository", "status").unwrap();
    let mut b = SummaryView::new(40405, Default::default());
    b.update_context(&model).unwrap();
    let mut foreign = b.into_host().unwrap();
    assert!(
        install_summary_custom_row(
            &mut foreign,
            root,
            &before,
            Content::new(ComponentId(40406u64 << 32), ButtonContent::new("外来"))
        )
        .is_err()
    );
    let container = ComponentId(40407u64 << 32);
    let mut host = ContentHost::new(Content::new(
        container,
        StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: 0.,
            },
            flexible_child: None,
            children: vec![Content::new(root, a)],
        },
    ))
    .unwrap();
    let button = ComponentId(40408u64 << 32);
    install_summary_custom_row(
        &mut host,
        root,
        &before,
        Content::new(button, ButtonContent::new("原实例")),
    )
    .unwrap();
    let retained = host.remove(root).unwrap();
    host.insert(container, retained).unwrap();
    assert!(
        view(&host, root)
            .validate_custom_row_action(&before)
            .is_err()
    );
    let mounted = token(&host, root);
    view(&host, root)
        .validate_custom_row_action(&mounted)
        .unwrap();
    assert_eq!(
        view(&host, root)
            .custom_row_component::<ButtonContent>(&mounted)
            .unwrap()
            .label,
        "原实例"
    );
}

#[test]
fn retirement_is_preflighted_and_uses_original_sdk_unmount_lifecycle() {
    use std::{cell::Cell, rc::Rc};
    #[derive(Debug)]
    struct Life(Rc<Cell<u64>>);
    impl Component for Life {
        fn measure(
            &mut self,
            text: &mut dyn tekes_ui_controls::ButtonTextRenderer,
            size: tekes_ui_scene::Size,
        ) -> tekes_ui_controls::layout::MeasuredChild {
            TextContent::new("Host lifecycle probe").measure(text, size)
        }
        fn lifecycle(&mut self, event: Lifecycle) {
            if event == Lifecycle::Unmounted {
                self.0.set(self.0.get() + 1);
            }
        }
        fn semantics(&self) -> ContentSemantics {
            TextContent::new("Host lifecycle probe").semantics()
        }
    }
    let model = input(1);
    let mut summary = SummaryView::new(40410, Default::default());
    summary.update_context(&model).unwrap();
    let root = summary.id();
    let mut host = summary.into_host().unwrap();
    let source = token(&host, root);
    let unmounted = Rc::new(Cell::new(0));
    install_summary_custom_row(
        &mut host,
        root,
        &source,
        Content::new(ComponentId(40411u64 << 32), Life(unmounted.clone())),
    )
    .unwrap();
    let mut invalid = input(2);
    invalid.host_sections[0].rows.clear();
    assert!(publish_summary_context_update(&mut host, root, &invalid).is_err());
    assert_eq!(unmounted.get(), 0);
    assert_eq!(view(&host, root).displayed_context_input(), Some(&model));
    let mut next = input(2);
    next.custom_rows[0].content_key = "replacement".into();
    assert!(view_mut(&mut host, root).update_context(&next).is_err());
    assert_eq!(unmounted.get(), 0);
    publish_summary_context_update(&mut host, root, &next).unwrap();
    assert_eq!(unmounted.get(), 1);
    assert!(
        view(&host, root)
            .validate_custom_row_action(&source)
            .is_err()
    );
    assert_eq!(view(&host, root).displayed_context_input(), Some(&next));
}
