use serde_json::json;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tekes_agent_components_ui::*;
use tekes_agent_presentation::{ChildHistoryLayout, ComposerDestination, RunScope};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{ChildPaneBinding, RunBinding};
use tekes_agent_ui_demo::fixture::Fixture;
use tekes_ui_controls::composition::*;

fn destination(id: &str) -> ComposerDestination {
    ComposerDestination::run(RunScope::main("fixture", id))
}
fn choice(id: &str, enabled: bool) -> ComposerChoice {
    ComposerChoice {
        id: id.into(),
        title: format!("模型 {id}"),
        detail: "供应商提供的能力说明".into(),
        enabled,
        disabled_reason: (!enabled).then(|| "当前会话不可用".into()),
    }
}
fn catalog(revision: u64) -> ComposerChoiceCatalog {
    ComposerChoiceCatalog {
        revision,
        phase: ComposerCatalogPhase::Ready,
        choices: vec![choice("a", true), choice("b", false)],
        selected: None,
    }
}
fn panel(host: &ContentHost, root: ComponentId) -> &ComposerChoices {
    host.get(root)
        .unwrap()
        .component::<ComposerChoices>()
        .unwrap()
}
fn panel_mut(host: &mut ContentHost, root: ComponentId) -> &mut ComposerChoices {
    host.get_mut(root)
        .unwrap()
        .component_mut::<ComposerChoices>()
        .unwrap()
}
fn activate(
    host: &mut ContentHost,
    root: ComponentId,
    target: ComponentId,
) -> Vec<ComposerChoiceIntent> {
    host.accessibility_action(target, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    panel_mut(host, root).take_intents(&actions)
}

#[test]
fn choice_catalog_retains_unknown_selection_and_restricts_disabled_pending_and_failed_reads() {
    let mut view = ComposerChoices::new(90, Default::default());
    let root = view.id();
    let request = view
        .open(destination("a"), ComposerChoiceKind::Model)
        .unwrap();
    let mut host = view.into_host().unwrap();
    assert!(panel(&host, root).choice_id("a").is_none());
    let mut input = catalog(1);
    input.selected = Some("provider-model-no-longer-listed".into());
    panel_mut(&mut host, root).update(&request, input).unwrap();
    assert!(
        host.semantics().iter().any(|node| node
            .semantics
            .name
            .contains("provider-model-no-longer-listed")),
        "an unknown current value stays visible without selecting the first row"
    );
    let unavailable = panel(&host, root).choice_id("b").unwrap();
    assert!(activate(&mut host, root, unavailable).is_empty());
    let target = panel(&host, root).choice_id("a").unwrap();
    // A synthetic double AX activation uses one captured generation and admits
    // only one local selection even when both events arrive before draining.
    host.accessibility_action(target, None, AccessibilityAction::Press);
    host.accessibility_action(target, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let result = panel_mut(&mut host, root).take_intents(&actions);
    assert_eq!(result.len(), 1);
    let ComposerChoiceIntent::Select(selection) = result.into_iter().next().unwrap() else {
        panic!("selection")
    };
    assert_eq!(selection.request, request);
    assert_eq!(selection.choice.id, "a");
    assert_eq!(selection.catalog_revision, 1);
    assert!(activate(&mut host, root, target).is_empty());
    assert!(panel_mut(&mut host, root).complete(&selection, Err("选择未应用".into())));
    assert!(
        host.semantics()
            .iter()
            .any(|n| n.semantics.name.contains("选择未应用"))
    );
    // Catalog read failure retains readable facts but does not enable writes.
    let mut failed = catalog(2);
    failed.phase = ComposerCatalogPhase::Failed("目录读取失败".into());
    panel_mut(&mut host, root).update(&request, failed).unwrap();
    let target = panel(&host, root).choice_id("a").unwrap();
    assert!(activate(&mut host, root, target).is_empty());
    panel_mut(&mut host, root)
        .update(&request, catalog(3))
        .unwrap();
    let target = panel(&host, root).choice_id("a").unwrap();
    let result = activate(&mut host, root, target);
    let ComposerChoiceIntent::Select(selection) = &result[0] else {
        panic!("selection")
    };
    assert!(panel_mut(&mut host, root).complete(selection, Ok(())));
    assert!(panel(&host, root).request().is_none());
}

#[test]
fn catalog_and_callbacks_are_fenced_across_scope_aba_versions_and_dismissal() {
    let mut view = ComposerChoices::new(91, Default::default());
    let root = view.id();
    let first = view
        .open(destination("a"), ComposerChoiceKind::Model)
        .unwrap();
    view.update(&first, catalog(1)).unwrap();
    let old_button = view.choice_id("a").unwrap();
    let old_dismiss = view.dismiss_id();
    let mut host = view.into_host().unwrap();
    let intents = activate(&mut host, root, old_button);
    let ComposerChoiceIntent::Select(old_selection) = &intents[0] else {
        panic!("selection")
    };
    panel_mut(&mut host, root)
        .open(destination("b"), ComposerChoiceKind::Preset)
        .unwrap();
    let latest = panel_mut(&mut host, root)
        .open(destination("a"), ComposerChoiceKind::Model)
        .unwrap();
    assert_ne!(latest, first);
    assert!(
        !panel_mut(&mut host, root)
            .update(&first, catalog(50))
            .unwrap()
    );
    panel_mut(&mut host, root)
        .update(&latest, catalog(2))
        .unwrap();
    assert!(activate(&mut host, root, old_button).is_empty());
    assert!(activate(&mut host, root, old_dismiss).is_empty());
    assert!(!panel_mut(&mut host, root).complete(old_selection, Ok(())));
    assert!(
        !panel_mut(&mut host, root)
            .update(&latest, catalog(1))
            .unwrap()
    );
    let mut duplicate = catalog(3);
    duplicate.choices.push(choice("a", true));
    assert!(
        panel_mut(&mut host, root)
            .update(&latest, duplicate)
            .is_err()
    );
    let target = panel(&host, root).choice_id("a").unwrap();
    assert!(host.get(target).is_some());
    panel_mut(&mut host, root)
        .update(&latest, catalog(3))
        .unwrap();
    assert!(
        activate(&mut host, root, target).is_empty(),
        "obsolete row callback"
    );
    let dismiss = panel(&host, root).dismiss_id();
    assert_eq!(
        activate(&mut host, root, dismiss),
        vec![ComposerChoiceIntent::Dismiss(latest)]
    );
    assert!(panel(&host, root).request().is_none());
}

#[tokio::test]
async fn settings_use_shared_generation_preserve_text_and_retire_with_their_original_owner() {
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default().with_children()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let a = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let b = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "b"))
            .await
            .unwrap(),
    );
    a.edit_text(&destination("a"), "未提交的中文草稿".into())
        .unwrap();
    let obsolete = a.begin_settings(&destination("a")).unwrap();
    let current = a.begin_settings(&destination("a")).unwrap();
    assert!(
        !obsolete
            .commit(BTreeMap::from([("model".into(), json!("obsolete"))]))
            .unwrap()
    );
    assert_eq!(current.destination(), &destination("a"));
    assert!(
        current
            .commit(BTreeMap::from([(
                "model".into(),
                json!("explicit-provider/model")
            )]))
            .unwrap()
    );
    assert_eq!(a.current_run().unwrap().draft.text, "未提交的中文草稿");
    assert_eq!(
        a.current_run().unwrap().draft.settings["model"],
        "explicit-provider/model"
    );
    let prepared = a
        .prepare_send(&destination("a"), tekes_agent_contract::PromptMode::Queue)
        .unwrap();
    assert_eq!(
        prepared.settings()["model"],
        "explicit-provider/model",
        "Send freezes the shared settings instead of consulting a later visible choice"
    );
    a.cancel_prepared(prepared).unwrap();
    assert!(b.current_run().unwrap().draft.settings.is_empty());
    assert!(a.begin_settings(&destination("b")).is_err());
    drop(a.begin_settings(&destination("a")).unwrap());
    assert_eq!(
        a.current_run().unwrap().draft.settings["model"],
        "explicit-provider/model",
        "dismiss does not clear settings"
    );

    let child =
        ChildPaneBinding::for_run(&a, "child-a", ChildHistoryLayout::IndependentSessions, None)
            .await
            .unwrap();
    child.refresh_controls().await.unwrap();
    let request = child
        .binding()
        .begin_settings(&ComposerDestination::run(child.binding().scope().clone()))
        .unwrap();
    child.retire();
    assert!(
        request.commit(BTreeMap::new()).is_err(),
        "late result cannot mutate a retired child"
    );
    let bounded = ChildPaneBinding::for_run(&b, "bounded-b", ChildHistoryLayout::SameSession, None)
        .await
        .unwrap();
    assert!(
        bounded
            .binding()
            .begin_settings(&ComposerDestination::run(bounded.binding().scope().clone()))
            .is_err()
    );
    runtime.stop().await.unwrap();
}
