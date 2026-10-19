use serde_json::json;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tekes_agent_components_ui::*;
use tekes_agent_contract::PromptMode;
use tekes_agent_presentation::{
    ComposerAnnotation, ComposerArtifact, ComposerContextBaseline, ComposerDestination, RunScope,
};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::RunBinding;
use tekes_agent_ui_demo::{
    component_demo::{ComponentDemo, SELECT_A, SELECT_B},
    fixture::Fixture,
};
use tekes_ui_controls::composition::*;
fn reference() -> ComposerArtifact {
    ComposerArtifact::Reference {
        id: "ref".into(),
        mention: Some("设计说明".into()),
        path: Some("/host/design.md".into()),
        excerpt: Some("中文引用 😀".into()),
    }
}
fn annotation() -> ComposerAnnotation {
    ComposerAnnotation {
        id: "note".into(),
        source: "回答 · turn 1".into(),
        excerpt: "保留这段中文 👩🏽‍💻".into(),
        comment: "请解释".into(),
        source_locator: None,
    }
}
async fn setup() -> (Arc<SessionRuntime>, [Arc<RunBinding>; 2]) {
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
    (runtime, [a, b])
}
fn capture(binding: &RunBinding) -> ComposerContextBaseline {
    ComposerContextBaseline::read(&binding.current_run().unwrap()).unwrap()
}
fn composer(host: &ContentHost, root: ComponentId) -> &Composer {
    host.get(root).unwrap().component::<Composer>().unwrap()
}
fn drain(host: &mut ContentHost, root: ComponentId) -> Vec<ComposerIntent> {
    let actions = host.drain_actions();
    host.get_mut(root)
        .unwrap()
        .component_mut::<Composer>()
        .unwrap()
        .take_intents(&actions)
}
#[tokio::test]
async fn whole_draft_text_edits_do_not_advance_context_but_content_aba_does() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let baseline = ComposerContextBaseline::read(&controller.snapshot()).unwrap();
    let mut draft = controller.snapshot().draft.clone();
    draft.text = "完整草稿中的正文更新".into();
    controller
        .edit(&baseline.selection.destination, draft)
        .unwrap();
    assert_eq!(
        controller.snapshot().draft_context_revision,
        baseline.context_revision
    );
    assert!(
        controller
            .patch_context(&baseline, vec![reference()], vec![])
            .unwrap()
    );
    let saved = ComposerContextBaseline::read(&controller.snapshot()).unwrap();
    controller
        .select(ComposerDestination::run(RunScope::main("fixture", "b")))
        .unwrap();
    assert!(
        !controller.patch_context(&saved, vec![], vec![]).unwrap(),
        "a different destination cannot receive a captured edit"
    );
    controller
        .select(saved.selection.destination.clone())
        .unwrap();
    let mut draft = controller.snapshot().draft.clone();
    draft.artifacts.clear();
    controller
        .edit(&saved.selection.destination, draft.clone())
        .unwrap();
    draft.artifacts = vec![reference()];
    controller
        .edit(&saved.selection.destination, draft)
        .unwrap();
    assert!(!controller.patch_context(&saved, vec![], vec![]).unwrap());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn atomic_context_patches_preserve_new_text_and_settings_but_reject_send_and_selection_changes()
 {
    let (runtime, bindings) = setup().await;
    let a = &bindings[0];
    let b = &bindings[1];
    let destination = ComposerDestination::run(a.scope().clone());
    let request = a.begin_settings(&destination).unwrap();
    let baseline = capture(a);
    a.edit_text(&destination, "后来输入的正文".into()).unwrap();
    request
        .commit(BTreeMap::from([("explicit".into(), json!("setting"))]))
        .unwrap();
    assert!(
        a.patch_context(&baseline, vec![reference()], vec![annotation()])
            .unwrap()
    );
    assert_eq!(a.current_run().unwrap().draft.text, "后来输入的正文");
    assert_eq!(
        a.current_run().unwrap().draft.settings["explicit"],
        "setting"
    );
    assert!(b.current_run().unwrap().draft.artifacts.is_empty());
    let mut legacy = serde_json::to_value(a.current_run().unwrap().as_ref()).unwrap();
    legacy
        .as_object_mut()
        .unwrap()
        .remove("draftContextRevision");
    let legacy: tekes_agent_presentation::RunSnapshot = serde_json::from_value(legacy).unwrap();
    assert_eq!(
        legacy.draft_context_revision, 0,
        "older serialized snapshots remain readable"
    );
    assert!(
        !a.patch_context(
            &ComposerContextBaseline::read(&legacy).unwrap(),
            vec![],
            vec![]
        )
        .unwrap(),
        "an unversioned legacy context cannot overwrite newer live content"
    );
    assert!(
        !a.patch_context(&baseline, vec![], vec![]).unwrap(),
        "a changed context does not accept an earlier callback"
    );
    let baseline = capture(a);
    assert!(a.patch_context(&baseline, vec![], vec![]).unwrap());
    assert!(
        a.patch_context(
            &capture(a),
            baseline.artifacts.clone(),
            baseline.annotations.clone()
        )
        .unwrap()
    );
    assert!(
        !a.patch_context(&baseline, vec![], vec![]).unwrap(),
        "an old callback cannot remove a re-added identical context"
    );
    let baseline = capture(a);
    let ticket = a.prepare_send(&destination, PromptMode::Queue).unwrap();
    assert!(
        !a.patch_context(&baseline, vec![reference()], vec![annotation()])
            .unwrap(),
        "Send invalidates a pending content capture"
    );
    a.cancel_prepared(ticket).unwrap();
    let child = tekes_agent_ui_bindings::ChildPaneBinding::for_run(
        a,
        "child-a",
        tekes_agent_presentation::ChildHistoryLayout::IndependentSessions,
        None,
    )
    .await
    .unwrap();
    child.refresh_controls().await.unwrap();
    let child_baseline = capture(child.binding());
    child.retire();
    assert!(
        child
            .binding()
            .patch_context(&child_baseline, vec![], vec![])
            .is_err(),
        "retired child cannot accept a delayed context patch"
    );
    assert!(
        !a.patch_context(&baseline, vec![reference()], vec![annotation()])
            .unwrap(),
        "restoring a failed admission does not revive an old capture"
    );
    let baseline = capture(a);
    drop(a.begin_settings(&destination).unwrap());
    assert!(
        a.patch_context(&baseline, vec![], vec![]).unwrap(),
        "opening a model/settings gesture in the same Run does not invalidate independent context work"
    );
    // Even an initially empty context is invalid after a text-only Send.
    assert!(a.patch_context(&capture(a), vec![], vec![]).unwrap());
    let empty = capture(a);
    let ticket = a.prepare_send(&destination, PromptMode::Queue).unwrap();
    assert!(!a.patch_context(&empty, vec![reference()], vec![]).unwrap());
    a.cancel_prepared(ticket).unwrap();
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn embedded_context_keeps_payloads_private_uses_captured_items_and_obeys_ime_barrier() {
    let (runtime, bindings) = setup().await;
    let a = &bindings[0];
    let attachment = ComposerArtifact::Attachment {
        id: "file".into(),
        name: "设计.png".into(),
        content: json!({"type":"image","data":"PRIVATE_MEDIA_PAYLOAD"}),
        facts: Some(tekes_agent_presentation::AttachmentFacts {
            byte_count: 123,
            media_type: Some("image/png".into()),
        }),
    };
    a.patch_context(
        &capture(a),
        vec![attachment.clone(), reference()],
        vec![annotation()],
    )
    .unwrap();
    let ids = ComposerIds::new(7000);
    let mut view = Composer::new(ids)
        .with_context(92, Default::default())
        .unwrap();
    view.set_context_affordances(ComposerContextAffordances {
        preview_artifacts: true,
        reveal_annotations: true,
        edit_annotations: false,
    });
    view.update(&a.current_run().unwrap(), Default::default())
        .unwrap();
    let mut host = view.into_host().unwrap();
    assert!(
        host.semantics()
            .iter()
            .any(|n| n.semantics.name.contains("123 B"))
    );
    assert!(
        host.semantics()
            .iter()
            .any(|n| n.semantics.name.contains("设计.png"))
    );
    assert!(
        host.semantics()
            .iter()
            .any(|n| n.semantics.name.contains("保留这段中文"))
    );
    assert!(
        !host
            .semantics()
            .iter()
            .any(|n| n.semantics.name.contains("PRIVATE_MEDIA_PAYLOAD"))
    );
    assert!(
        !host
            .semantics()
            .iter()
            .any(|n| n.semantics.name.contains("中文引用 😀")),
        "reference preview metadata does not masquerade as submitted quoted text when an explicit mention takes precedence"
    );
    let item = ComposerContextItem::Artifact(attachment.clone());
    let remove = composer(&host, ids.root)
        .context()
        .unwrap()
        .action_id(&item, ComposerContextAction::Remove)
        .unwrap();
    let preview = composer(&host, ids.root)
        .context()
        .unwrap()
        .action_id(&item, ComposerContextAction::Open)
        .unwrap();
    host.focus(Some(ids.editor), true);
    host.event(&ContentEvent::Preedit {
        text: "组字".into(),
        selection: 2..2,
        clauses: vec![],
        replacement: None,
    });
    drain(&mut host, ids.root);
    host.accessibility_action(remove, None, AccessibilityAction::Press);
    assert!(drain(&mut host, ids.root).is_empty());
    host.accessibility_action(preview, None, AccessibilityAction::Press);
    assert!(drain(&mut host, ids.root).is_empty());
    assert!(composer(&host, ids.root).is_composing());
    host.event(&ContentEvent::Commit {
        text: "中文正文".into(),
        replacement: None,
    });
    for intent in drain(&mut host, ids.root) {
        if let ComposerIntent::Edit { destination, text } = intent {
            a.edit_text(&destination, text).unwrap();
        }
    }
    host.get_mut(ids.root)
        .unwrap()
        .component_mut::<Composer>()
        .unwrap()
        .update(&a.current_run().unwrap(), Default::default())
        .unwrap();
    host.accessibility_action(preview, None, AccessibilityAction::Press);
    let result = drain(&mut host, ids.root);
    let ComposerIntent::Context(intent) = &result[0] else {
        panic!("context preview")
    };
    assert_eq!(intent.item, item);
    assert_eq!(intent.action, ComposerContextAction::Open);
    let before = intent.baseline.clone();
    a.edit_text(
        &ComposerDestination::run(a.scope().clone()),
        "后来输入的正文".into(),
    )
    .unwrap();
    host.get_mut(ids.root)
        .unwrap()
        .component_mut::<Composer>()
        .unwrap()
        .update(&a.current_run().unwrap(), Default::default())
        .unwrap();
    assert_eq!(
        composer(&host, ids.root)
            .context()
            .unwrap()
            .action_id(&item, ComposerContextAction::Open),
        Some(preview)
    );
    host.accessibility_action(preview, None, AccessibilityAction::Press);
    let next = drain(&mut host, ids.root);
    let ComposerIntent::Context(next) = &next[0] else {
        panic!("preview")
    };
    assert!(
        Arc::ptr_eq(&before, &next.baseline),
        "text/stream publications do not duplicate retained attachment payloads or rebuild card identities"
    );
    host.accessibility_action(remove, None, AccessibilityAction::Press);
    let result = drain(&mut host, ids.root);
    let ComposerIntent::Context(intent) = &result[0] else {
        panic!("remove")
    };
    assert!(a.remove_context(intent).unwrap());
    assert_eq!(a.current_run().unwrap().draft.text, "后来输入的正文");
    assert_eq!(a.current_run().unwrap().draft.artifacts, vec![reference()]);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn demo_retains_destination_cards_and_ignores_retired_row_callbacks() {
    let (runtime, bindings) = setup().await;
    let a = bindings[0].clone();
    a.patch_context(&capture(&a), vec![reference()], vec![annotation()])
        .unwrap();
    let mut view = ComponentDemo::new(bindings).unwrap();
    let item = ComposerContextItem::Artifact(reference());
    let id = view
        .tree
        .get(view.ids().root)
        .unwrap()
        .component::<Composer>()
        .unwrap()
        .context()
        .unwrap()
        .action_id(&item, ComposerContextAction::Remove)
        .unwrap();
    view.accessibility_action(SELECT_B, None, AccessibilityAction::Press)
        .unwrap();
    assert!(
        view.accessibility_action(id, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty()
    );
    assert_eq!(a.current_run().unwrap().draft.artifacts.len(), 1);
    view.accessibility_action(SELECT_A, None, AccessibilityAction::Press)
        .unwrap();
    view.accessibility_action(id, None, AccessibilityAction::Press)
        .unwrap();
    assert!(a.current_run().unwrap().draft.artifacts.is_empty());
    // Re-add the same DTO: row identity must still change, so an old AX action
    // cannot remove the new instance after the view observes removal/re-addition.
    a.patch_context(&capture(&a), vec![reference()], vec![annotation()])
        .unwrap();
    view.sync().unwrap();
    view.accessibility_action(id, None, AccessibilityAction::Press)
        .unwrap();
    assert_eq!(a.current_run().unwrap().draft.artifacts.len(), 1);
    runtime.stop().await.unwrap();
}
