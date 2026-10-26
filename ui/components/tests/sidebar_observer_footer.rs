use std::sync::Arc;

use tekes_agent_components_ui::*;
use tekes_agent_presentation::{
    SidebarObservation, SidebarObserverEndpointInput, SidebarObserverKey, SidebarObserverPhase,
    SidebarObserverSourceKey, SidebarObserverStatusInput,
};
use tekes_ui_controls::{ButtonTextAlign, ButtonTextRenderer, composition::*};
use tekes_ui_scene::{Rect, Scene, Size};
use tekes_ui_tokens::{Rgba, Theme, proposal};

struct Text;

impl ButtonTextRenderer for Text {
    fn measure(&mut self, text: &str, _: proposal::TextRole, max_width: f32) -> Size {
        Size::new((text.chars().count() as f32 * 8.0).min(max_width), 20.0)
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
}

fn observer_input(
    source_owner: &Arc<()>,
    child_owner: &Arc<()>,
    revision: u64,
    generation: u64,
    phase: SidebarObserverPhase,
) -> SidebarObserverStatusInput {
    SidebarObserverStatusInput {
        source: SidebarObserverSourceKey::from_owner(source_owner),
        revision,
        endpoints: vec![SidebarObserverEndpointInput {
            key: SidebarObserverKey::from_owners(source_owner, child_owner, "native", generation)
                .unwrap(),
            revision,
            observation: SidebarObservation {
                generation,
                phase,
                converged: true,
            },
            error: (phase == SidebarObserverPhase::Failed).then(|| "observer failure".to_owned()),
        }],
    }
}

fn retry_action(id: ComponentId) -> ContentAction {
    ContentAction {
        target: id,
        kind: ContentActionKind::Activate,
        source: ContentActionSource::Pointer,
    }
}

#[test]
fn observer_footer_coexists_with_search_and_retry_fits_the_72px_viewport() {
    let mut sidebar = Sidebar::new(41, SidebarLabels::default());
    sidebar
        .update(&SidebarInput {
            folder_filter: None,
            search_help: None,
            revision: 1,
            loaded: true,
            error: Some("inventory failure".into()),
            rows: vec![],
            selected: None,
        })
        .unwrap();
    sidebar
        .update_search(&SidebarSearchInput {
            revision: 1,
            key: Some(SidebarSearchKey {
                owner: "search-owner".into(),
                endpoint_id: "native".into(),
                connection_epoch: 1,
                generation: 1,
                query: "中文".into(),
            }),
            phase: SidebarSearchPhase::Failed {
                message: "search failure".into(),
                has_more: false,
                retryable: true,
            },
        })
        .unwrap();

    let source = Arc::new(());
    let child = Arc::new(());
    let status = observer_input(&source, &child, 1, 1, SidebarObserverPhase::Failed);
    sidebar
        .bind_observer_source(&status, SidebarObserverLabels::default())
        .unwrap();
    let root = sidebar.id();
    let search_footer = sidebar.search_footer_id();
    let observer_footer = sidebar.observer_footer_id();
    let retry = sidebar.observer_retry_id("native").unwrap();
    let search_retry = sidebar.search_retry_id().unwrap();
    assert!((root.0 + 0xFE00_0000..root.0 + 0xFF00_0000).contains(&retry.0));
    assert!((root.0 + 0xFF00_0000..root.0 + u32::MAX as u64).contains(&search_retry.0));

    let mut host = sidebar.into_host().unwrap();
    let mut scene = Scene::new();
    let bounds = Rect::new(0.0, 0.0, 280.0, 72.0);
    host.render(&mut scene, &mut Text, bounds, bounds, Theme::LIGHT.into())
        .unwrap();

    let sidebar = host.get(root).unwrap().component::<Sidebar>().unwrap();
    let children = sidebar.children();
    assert_eq!(children.len(), 3);
    assert_eq!(children[1].id(), search_footer);
    assert_eq!(children[2].id(), observer_footer);
    let search = host.get(search_footer).unwrap().frame();
    let observer = host.get(observer_footer).unwrap().frame();
    assert!(search.max_y() <= observer.min_y());
    let button = host.get(retry).unwrap();
    assert_eq!(
        button.visible(),
        Some(button.frame()),
        "72px viewport must contain the complete Retry button, not just an intersection"
    );
    let semantic_names: Vec<_> = host
        .semantics()
        .into_iter()
        .map(|node| node.semantics.name)
        .collect();
    assert!(
        semantic_names
            .iter()
            .any(|name| name.contains("observer failure"))
    );
    assert!(
        semantic_names
            .iter()
            .any(|name| name == "inventory failure"),
        "independently supplied inventory errors must not be hidden by observer status"
    );
}

#[test]
fn source_clear_and_replacement_retire_old_retry_ids_without_reuse() {
    let mut sidebar = Sidebar::new(42, SidebarLabels::default());
    let first_source = Arc::new(());
    let first_child = Arc::new(());
    let first = observer_input(
        &first_source,
        &first_child,
        1,
        1,
        SidebarObserverPhase::Failed,
    );
    sidebar
        .bind_observer_source(&first, SidebarObserverLabels::default())
        .unwrap();
    let retired = sidebar.observer_retry_id("native").unwrap();
    let old_action = retry_action(retired);
    assert_eq!(
        sidebar
            .take_observer_intents(std::slice::from_ref(&old_action))
            .len(),
        1
    );

    sidebar
        .bind_observer_source(&first, SidebarObserverLabels::default())
        .unwrap();
    assert!(
        sidebar.observer_retry_id("native").is_none(),
        "re-binding the same source cannot clear pending"
    );
    assert!(
        sidebar
            .take_observer_intents(std::slice::from_ref(&old_action))
            .is_empty()
    );

    sidebar.clear_observer_source().unwrap();
    assert!(
        sidebar
            .take_observer_intents(std::slice::from_ref(&old_action))
            .is_empty()
    );

    let second_source = Arc::new(());
    let second_child = Arc::new(());
    let second = observer_input(
        &second_source,
        &second_child,
        1,
        1,
        SidebarObserverPhase::Failed,
    );
    sidebar
        .bind_observer_source(&second, SidebarObserverLabels::default())
        .unwrap();
    let current = sidebar.observer_retry_id("native").unwrap();
    assert!(current > retired);
    assert!(sidebar.take_observer_intents(&[old_action]).is_empty());
    assert_eq!(
        sidebar
            .take_observer_intents(&[retry_action(current)])
            .len(),
        1
    );
}
