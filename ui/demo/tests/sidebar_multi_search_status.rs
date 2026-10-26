use tekes_agent_components_ui::{SidebarMultiSearchStatus, SidebarSearchLabels};
use tekes_agent_presentation::{
    SidebarMultiSearchChild, SidebarMultiSearchInput, SidebarMultiSearchKey,
    SidebarMultiSearchPublication, SidebarSearchInput, SidebarSearchKey, SidebarSearchPhase,
    SidebarSearchPublication,
};
use tekes_ui_controls::composition::*;

struct Text;
impl tekes_ui_controls::ButtonTextRenderer for Text {
    fn measure(
        &mut self,
        text: &str,
        _: tekes_ui_tokens::proposal::TextRole,
        _: f32,
    ) -> tekes_ui_scene::Size {
        tekes_ui_scene::Size::new(text.chars().count() as f32 * 10., 20.)
    }
    fn draw(
        &mut self,
        _: &mut tekes_ui_scene::Scene,
        _: &str,
        _: tekes_ui_tokens::proposal::TextRole,
        _: tekes_ui_scene::Rect,
        _: tekes_ui_controls::ButtonTextAlign,
        _: tekes_ui_tokens::Rgba,
    ) {
    }
}
fn paint(host: &mut ContentHost) {
    let frame = tekes_ui_scene::Rect::new(0., 0., 400., 400.);
    host.render(
        &mut tekes_ui_scene::Scene::default(),
        &mut Text,
        frame,
        frame,
        tekes_ui_tokens::Theme::LIGHT.into(),
    )
    .unwrap();
}

fn publication(
    owner: &str,
    revision: u64,
    query: &str,
    generation: u64,
    child_generation: u64,
    first: SidebarSearchPhase,
    second: SidebarSearchPhase,
) -> SidebarMultiSearchPublication {
    let aggregate_key = (!query.is_empty()).then(|| SidebarMultiSearchKey {
        owner: owner.into(),
        generation,
        query: query.into(),
    });
    let children = [("alpha", first), ("beta", second)]
        .into_iter()
        .map(|(endpoint_id, phase)| SidebarMultiSearchChild {
            endpoint_id: endpoint_id.into(),
            publication: SidebarSearchPublication {
                input: SidebarSearchInput {
                    revision,
                    key: aggregate_key.as_ref().map(|_| SidebarSearchKey {
                        owner: format!("reader-{endpoint_id}"),
                        endpoint_id: endpoint_id.into(),
                        connection_epoch: 1,
                        generation: child_generation,
                        query: query.into(),
                    }),
                    phase,
                },
                query: query.into(),
                hits: vec![],
                unresolved: vec![],
                result: None,
            },
        })
        .collect();
    SidebarMultiSearchPublication {
        input: SidebarMultiSearchInput {
            owner: owner.into(),
            generation: if query.is_empty() { 0 } else { generation },
            revision,
            key: aggregate_key,
        },
        query: query.into(),
        children,
    }
}

fn status_text(host: &ContentHost, _: ComponentId) -> Vec<String> {
    host.semantics()
        .into_iter()
        .map(|node| node.semantics.name)
        .collect()
}

fn status(host: &mut ContentHost, root: ComponentId) -> &mut SidebarMultiSearchStatus {
    host.get_mut(root)
        .unwrap()
        .component_mut::<SidebarMultiSearchStatus>()
        .unwrap()
}

fn publish(
    host: &mut ContentHost,
    root: ComponentId,
    owner: &str,
    publication: &SidebarMultiSearchPublication,
) {
    status(host, root).update_from(owner, publication).unwrap();
    paint(host);
}

fn failed(has_more: bool, retryable: bool) -> SidebarSearchPhase {
    SidebarSearchPhase::Failed {
        message: "alpha offline".into(),
        has_more,
        retryable,
    }
}

#[test]
fn shows_endpoint_owned_failure_searching_and_more_without_claiming_global_success() {
    let source = publication(
        "owner-a",
        1,
        "needle",
        1,
        1,
        failed(true, true),
        SidebarSearchPhase::Searching,
    );
    let mut component = SidebarMultiSearchStatus::new(201, SidebarSearchLabels::default());
    component.replace_source("owner-a", &source).unwrap();
    let root = component.id();
    let mut host = component.into_host().unwrap();
    let text = status_text(&host, root);
    assert!(
        text.iter()
            .any(|line| line.contains("alpha: alpha offline"))
    );
    assert!(text.iter().any(|line| line.contains("还有更多匹配结果")));
    assert!(text.iter().any(|line| line.contains("beta: 正在搜索")));
    assert!(status(&mut host, root).retry_id("alpha").is_some());
}

#[test]
fn captures_original_ax_button_once_and_keeps_pending_across_other_endpoint_updates() {
    let source = publication(
        "owner-a",
        1,
        "needle",
        1,
        1,
        failed(false, true),
        SidebarSearchPhase::Searching,
    );
    let mut component = SidebarMultiSearchStatus::new(202, SidebarSearchLabels::default());
    component.replace_source("owner-a", &source).unwrap();
    let root = component.id();
    let mut host = component.into_host().unwrap();
    let alpha_retry = status(&mut host, root).retry_id("alpha").unwrap();
    host.accessibility_action(alpha_retry, None, AccessibilityAction::Press);
    let raw = host.drain_actions();
    let intents = status(&mut host, root).take_intents(&raw);
    assert_eq!(intents.len(), 1);
    assert!(matches!(
        &intents[0],
        tekes_agent_presentation::SidebarMultiSearchIntent::Retry { .. }
    ));
    assert!(status(&mut host, root).take_intents(&raw).is_empty());
    paint(&mut host);
    assert!(
        host.get(alpha_retry)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap()
            .button
            .state
            .disabled
    );

    let next = publication(
        "owner-a",
        2,
        "needle",
        1,
        1,
        failed(false, true),
        SidebarSearchPhase::Complete { has_more: true },
    );
    publish(&mut host, root, "owner-a", &next);
    assert_eq!(status(&mut host, root).retry_id("alpha"), Some(alpha_retry));
    assert!(status(&mut host, root).take_intents(&raw).is_empty());
    assert!(
        host.get(alpha_retry)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap()
            .button
            .state
            .disabled
    );
    assert!(
        status_text(&host, root)
            .iter()
            .any(|line| line.contains("beta: 还有更多匹配结果"))
    );
}

#[test]
fn retires_raw_actions_on_child_attempt_query_and_owner_replacement() {
    let source = publication(
        "owner-a",
        1,
        "needle",
        1,
        1,
        failed(false, true),
        SidebarSearchPhase::Searching,
    );
    let mut component = SidebarMultiSearchStatus::new(203, SidebarSearchLabels::default());
    component.replace_source("owner-a", &source).unwrap();
    let root = component.id();
    let mut host = component.into_host().unwrap();
    let mut old_button = status(&mut host, root).retry_id("alpha").unwrap();
    host.accessibility_action(old_button, None, AccessibilityAction::Press);
    let stale = host.drain_actions();

    let attempt = publication(
        "owner-a",
        2,
        "needle",
        1,
        2,
        failed(false, true),
        SidebarSearchPhase::Searching,
    );
    publish(&mut host, root, "owner-a", &attempt);
    assert!(status(&mut host, root).take_intents(&stale).is_empty());
    let child_attempt_button = status(&mut host, root).retry_id("alpha").unwrap();
    assert_ne!(child_attempt_button, old_button);
    old_button = child_attempt_button;

    host.accessibility_action(old_button, None, AccessibilityAction::Press);
    let stale = host.drain_actions();
    let query = publication(
        "owner-a",
        3,
        "another query",
        2,
        3,
        failed(false, true),
        SidebarSearchPhase::Searching,
    );
    publish(&mut host, root, "owner-a", &query);
    assert!(status(&mut host, root).take_intents(&stale).is_empty());
    let query_button = status(&mut host, root).retry_id("alpha").unwrap();
    assert_ne!(query_button, old_button);

    host.accessibility_action(query_button, None, AccessibilityAction::Press);
    let stale = host.drain_actions();
    let replacement = publication(
        "owner-b",
        1,
        "another query",
        1,
        1,
        failed(false, true),
        SidebarSearchPhase::Searching,
    );
    status(&mut host, root)
        .replace_source("owner-b", &replacement)
        .unwrap();
    paint(&mut host);
    assert!(status(&mut host, root).take_intents(&stale).is_empty());
    assert_eq!(status(&mut host, root).source_owner(), Some("owner-b"));
    assert_ne!(
        status(&mut host, root).retry_id("alpha").unwrap(),
        query_button
    );
}

#[test]
fn source_validation_and_revision_fences_leave_existing_controls_untouched() {
    let source = publication(
        "owner-a",
        1,
        "needle",
        1,
        1,
        failed(false, true),
        SidebarSearchPhase::Searching,
    );
    let mut component = SidebarMultiSearchStatus::new(204, SidebarSearchLabels::default());
    component.replace_source("owner-a", &source).unwrap();
    let original_id = component.retry_id("alpha").unwrap();

    let mut invalid = source.clone();
    invalid.children[0].endpoint_id = "wrong".into();
    assert!(component.update_from("owner-a", &invalid).is_err());
    assert_eq!(component.displayed(), Some(&source));
    assert_eq!(component.retry_id("alpha"), Some(original_id));

    let mut same_revision_different = source.clone();
    same_revision_different.children[0].publication.input.phase = failed(true, true);
    assert!(
        component
            .update_from("owner-a", &same_revision_different)
            .is_err()
    );
    let older = publication(
        "owner-a",
        0,
        "needle",
        1,
        1,
        failed(false, true),
        SidebarSearchPhase::Searching,
    );
    assert!(component.update_from("owner-a", &older).is_err());
    assert_eq!(component.displayed(), Some(&source));
    assert_eq!(component.retry_id("alpha"), Some(original_id));
}

#[test]
fn sidebar_single_multi_mode_changes_never_reuse_a_retired_retry_id() {
    use tekes_agent_components_ui::{Sidebar, SidebarInput, SidebarLabels};
    let source = publication(
        "aggregate",
        1,
        "query",
        1,
        1,
        failed(false, true),
        SidebarSearchPhase::Searching,
    );
    let mut sidebar = Sidebar::new(205, SidebarLabels::default());
    sidebar
        .update(&SidebarInput {
            revision: 1,
            loaded: true,
            error: None,
            rows: vec![],
            selected: None,
            folder_filter: None,
            search_help: None,
        })
        .unwrap();
    sidebar
        .replace_multi_search_source("aggregate", &source)
        .unwrap();
    let root = sidebar.id();
    let footer = sidebar.search_footer_id();
    let mut host = sidebar.into_host().unwrap();
    let old = host
        .get(footer)
        .unwrap()
        .component::<SidebarMultiSearchStatus>()
        .unwrap()
        .retry_id("alpha")
        .unwrap();
    host.accessibility_action(old, None, AccessibilityAction::Press);
    let raw = host.drain_actions();
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .replace_search_source(
            "single",
            &SidebarSearchInput {
                revision: 1,
                key: None,
                phase: SidebarSearchPhase::Idle,
            },
        )
        .unwrap();
    paint(&mut host);
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .replace_multi_search_source("aggregate", &source)
        .unwrap();
    paint(&mut host);
    let fresh = host
        .get(footer)
        .unwrap()
        .component::<SidebarMultiSearchStatus>()
        .unwrap()
        .retry_id("alpha")
        .unwrap();
    assert_ne!(fresh, old);
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .take_multi_search_intents(&raw)
            .is_empty()
    );
    host.accessibility_action(fresh, None, AccessibilityAction::Press);
    let current = host.drain_actions();
    assert_eq!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<Sidebar>()
            .unwrap()
            .take_multi_search_intents(&current)
            .len(),
        1
    );
}

#[test]
fn a_higher_display_revision_cannot_restore_retired_aggregate_or_child_generations() {
    let current = publication(
        "owner",
        10,
        "query",
        3,
        5,
        failed(false, true),
        SidebarSearchPhase::Searching,
    );
    let mut component = SidebarMultiSearchStatus::new(206, SidebarSearchLabels::default());
    component.replace_source("owner", &current).unwrap();
    let old_query = publication(
        "owner",
        11,
        "old query",
        2,
        4,
        failed(false, true),
        SidebarSearchPhase::Searching,
    );
    old_query.validate().unwrap();
    assert!(component.update_from("owner", &old_query).is_err());
    let old_child = publication(
        "owner",
        11,
        "query",
        3,
        4,
        failed(false, true),
        SidebarSearchPhase::Searching,
    );
    old_child.validate().unwrap();
    assert!(component.update_from("owner", &old_child).is_err());
    assert_eq!(component.displayed(), Some(&current));
}
