use serde_json::json;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tekes_agent_components_ui::*;
use tekes_agent_presentation::{
    ComposerAnnotation, ComposerArtifact, ComposerContextBaseline, ComposerDestination, RunScope,
};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{ModelSelectionPreparation, RunBinding};
use tekes_agent_ui_demo::{
    component_demo::{ComponentDemo, DemoCommand, NEW_SESSION, SELECT_B},
    fixture::Fixture,
};
use tekes_ui_controls::composition::*;

fn note() -> ComposerAnnotation {
    ComposerAnnotation {id:"note".into(),source:"回答 · turn 1".into(),excerpt:"引用中文 😀".into(),comment:"原评论".into(),source_locator:Some(serde_json::from_value(json!({"endpoint":"fixture","sessionId":"a","itemId":"message","turnId":"turn","kind":"assistant","start":0,"end":7,"fingerprint":"KEEP_SOURCE_DIGEST"})).unwrap())}
}
async fn setup() -> (Arc<SessionRuntime>, [Arc<RunBinding>; 2]) {
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default()),
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
    let baseline = ComposerContextBaseline::read(&a.current_run().unwrap()).unwrap();
    assert!(
        a.patch_context(
            &baseline,
            vec![ComposerArtifact::Reference {
                id: "ref".into(),
                mention: Some("保留引用".into()),
                path: None,
                excerpt: None
            }],
            vec![note()]
        )
        .unwrap()
    );
    (runtime, [a, b])
}
fn request(binding: &RunBinding) -> ComposerContextIntent {
    let baseline =
        Arc::new(ComposerContextBaseline::read(&binding.current_run().unwrap()).unwrap());
    ComposerContextIntent {
        item: ComposerContextItem::Annotation(baseline.annotations[0].clone()),
        baseline,
        action: ComposerContextAction::EditAnnotation,
    }
}
fn editor(host: &mut ContentHost, root: ComponentId) -> &mut AnnotationEditor {
    host.get_mut(root)
        .unwrap()
        .component_mut::<AnnotationEditor>()
        .unwrap()
}
fn standalone(binding: &RunBinding) -> (ContentHost, ComponentId, AnnotationEditorIds) {
    let mut panel = AnnotationEditor::new(96, Default::default());
    panel.open(&request(binding)).unwrap();
    panel
        .update_authority(&binding.current_run().unwrap(), true)
        .unwrap();
    let ids = panel.ids();
    let mut host = panel.into_host().unwrap();
    assert!(host.focus(Some(ids.editor), true));
    host.accessibility_action(
        ids.editor,
        None,
        AccessibilityAction::SetSelection(0..note().comment.encode_utf16().count()),
    );
    host.drain_actions();
    (host, ids.root, ids)
}
fn deliver(
    host: &mut ContentHost,
    root: ComponentId,
    event: ContentEvent,
) -> Vec<AnnotationEditorIntent> {
    host.event(&event);
    let actions = host.drain_actions();
    editor(host, root).take_intents(&actions)
}
fn change(mut intents: Vec<AnnotationEditorIntent>) -> AnnotationEditChange {
    assert_eq!(intents.len(), 1);
    let AnnotationEditorIntent::Change(change) = intents.remove(0) else {
        panic!("annotation change")
    };
    *change
}
fn commit(host: &mut ContentHost, root: ComponentId, text: &str) -> Vec<AnnotationEditorIntent> {
    deliver(
        host,
        root,
        ContentEvent::Commit {
            text: text.into(),
            replacement: None,
        },
    )
}
#[tokio::test]
async fn committed_comment_keeps_provenance_prose_settings_and_editor_selection() {
    let (runtime, [a, _]) = setup().await;
    let (mut host, root, ids) = standalone(&a);
    host.focus(Some(ids.excerpt), true);
    assert!(commit(&mut host, root, "不能改写引用").is_empty());
    assert_eq!(
        host.get(ids.excerpt)
            .unwrap()
            .component::<TextFieldContent>()
            .unwrap()
            .surface
            .input
            .text,
        note().excerpt
    );
    host.focus(Some(ids.editor), true);
    let destination = ComposerDestination::run(a.scope().clone());
    a.edit_text(&destination, "较新的正文".into()).unwrap();
    assert!(
        a.begin_settings(&destination)
            .unwrap()
            .commit(BTreeMap::from([("host-setting".into(), json!(true))]))
            .unwrap()
    );
    assert!(
        deliver(
            &mut host,
            root,
            ContentEvent::Preedit {
                text: "评论组字".into(),
                selection: 4..4,
                clauses: vec![],
                replacement: None
            }
        )
        .is_empty()
    );
    assert!(editor(&mut host, root).is_composing());
    host.accessibility_action(ids.close, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    assert!(editor(&mut host, root).take_intents(&actions).is_empty());
    assert_eq!(
        a.current_run().unwrap().draft.annotations[0].comment,
        "原评论"
    );
    let text = "  评论\n中文 😀  ";
    let changed = change(commit(&mut host, root, text));
    let selection = editor(&mut host, root).editor().unwrap().input.selection();
    let result = a.edit_annotation(&changed).unwrap();
    // A canonical publication may arrive before the host acknowledges its write.
    editor(&mut host, root)
        .update_authority(&a.current_run().unwrap(), true)
        .unwrap();
    assert!(editor(&mut host, root).complete(&changed, Ok(result)));
    assert_eq!(
        editor(&mut host, root).editor().unwrap().input.selection(),
        selection
    );
    assert_eq!(editor(&mut host, root).editor().unwrap().input.text, text);
    let saved = a.current_run().unwrap();
    let mut expected = note();
    expected.comment = text.into();
    assert_eq!(saved.draft.annotations, vec![expected]);
    assert_eq!(saved.draft.text, "较新的正文");
    assert_eq!(saved.draft.artifacts.len(), 1);
    assert_eq!(saved.draft.settings["host-setting"], true);
    assert!(
        !host
            .semantics()
            .iter()
            .any(|n| n.semantics.name.contains("KEEP_SOURCE_DIGEST"))
    );
    assert!(matches!(
        commit(&mut host, root, "后续").as_slice(),
        [AnnotationEditorIntent::Change(_)]
    ));
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn pending_newer_typing_and_upstream_undo_are_not_reset_by_acknowledgements() {
    let (runtime, [a, _]) = setup().await;
    let (mut host, root, _) = standalone(&a);
    let first = change(commit(&mut host, root, "第一段"));
    assert!(commit(&mut host, root, "第二段").is_empty());
    let latest = editor(&mut host, root).editor().unwrap().input.text.clone();
    let publication_before_ack = a.current_run().unwrap();
    let accepted = a.edit_annotation(&first).unwrap();
    assert!(editor(&mut host, root).complete(&first, Ok(accepted)));
    assert!(
        editor(&mut host, root)
            .update_authority(&publication_before_ack, true)
            .is_err(),
        "an acknowledged write must fence older publications even if no notification preceded its receipt"
    );
    assert_eq!(editor(&mut host, root).editor().unwrap().input.text, latest);
    let second = change(editor(&mut host, root).take_intents(&[]));
    assert!(second.baseline.context_revision > first.baseline.context_revision);
    assert!(editor(&mut host, root).complete(&second, Ok(a.edit_annotation(&second).unwrap())));
    assert!(!editor(&mut host, root).complete(&first, Err("late".into())));
    let undo = change(deliver(
        &mut host,
        root,
        ContentEvent::TextCommand("undo:".into()),
    ));
    assert_ne!(undo.comment, latest);
    assert!(editor(&mut host, root).complete(&undo, Ok(a.edit_annotation(&undo).unwrap())));
    assert_eq!(
        a.current_run().unwrap().draft.annotations[0].comment,
        editor(&mut host, root).editor().unwrap().input.text
    );
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn context_aba_stales_the_capture_without_retry_and_old_ids_cannot_close_reopened_editor() {
    let (runtime, [a, _]) = setup().await;
    let (mut host, root, old) = standalone(&a);
    let changed = change(commit(&mut host, root, "保留本地编辑"));
    let baseline = ComposerContextBaseline::read(&a.current_run().unwrap()).unwrap();
    assert!(
        a.patch_context(&baseline, baseline.artifacts.clone(), vec![])
            .unwrap()
    );
    let empty = ComposerContextBaseline::read(&a.current_run().unwrap()).unwrap();
    assert!(
        a.patch_context(&empty, empty.artifacts.clone(), vec![note()])
            .unwrap()
    );
    assert!(a.edit_annotation(&changed).unwrap().is_none());
    assert!(editor(&mut host, root).complete(&changed, Ok(None)));
    assert!(editor(&mut host, root).take_intents(&[]).is_empty());
    assert_eq!(
        editor(&mut host, root).editor().unwrap().input.text,
        "保留本地编辑"
    );
    editor(&mut host, root).open(&request(&a)).unwrap();
    let fresh = editor(&mut host, root).ids();
    assert_ne!(fresh.editor, old.editor);
    host.accessibility_action(old.close, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    assert!(editor(&mut host, root).take_intents(&actions).is_empty());
    assert!(!editor(&mut host, root).complete(&changed, Err("late".into())));
    assert!(editor(&mut host, root).is_open());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn permission_invalidation_overlong_comments_and_malformed_ack_do_not_write_or_replace_text()
{
    let (runtime, [a, _]) = setup().await;
    let (mut host, root, ids) = standalone(&a);
    let too_long = "😀".repeat(50_001);
    assert!(commit(&mut host, root, &too_long).is_empty());
    assert_eq!(
        editor(&mut host, root).editor().unwrap().input.text,
        too_long
    );
    assert_eq!(
        a.current_run().unwrap().draft.annotations[0].comment,
        "原评论"
    );
    host.accessibility_action(
        ids.editor,
        None,
        AccessibilityAction::SetSelection(0..100_002),
    );
    host.drain_actions();
    let changed = change(commit(&mut host, root, "有效评论"));
    let accepted = a.edit_annotation(&changed).unwrap();
    editor(&mut host, root)
        .update_authority(&a.current_run().unwrap(), false)
        .unwrap();
    assert!(editor(&mut host, root).complete(&changed, Ok(accepted)));
    assert!(commit(&mut host, root, "权限变化后的文本").is_empty());
    assert_eq!(
        a.current_run().unwrap().draft.annotations[0].comment,
        "有效评论"
    );
    editor(&mut host, root).open(&request(&a)).unwrap();
    let fresh = editor(&mut host, root).ids();
    host.focus(Some(fresh.editor), true);
    let changed = change(commit(&mut host, root, "新内容"));
    let mut forged = (*a.current_run().unwrap()).clone();
    forged.draft.annotations[0].comment = changed.comment.clone();
    assert!(editor(&mut host, root).complete(&changed, Ok(Some(Arc::new(forged)))));
    assert!(editor(&mut host, root).take_intents(&[]).is_empty());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn demo_card_edits_commit_into_original_draft_and_send_uses_shared_annotation_compilation() {
    let (runtime, bindings) = setup().await;
    let a = bindings[0].clone();
    let mut view = ComponentDemo::new(bindings).unwrap();
    let item = ComposerContextItem::Annotation(note());
    let target = view
        .tree
        .get(view.ids().root)
        .unwrap()
        .component::<Composer>()
        .unwrap()
        .context()
        .unwrap()
        .action_id(&item, ComposerContextAction::EditAnnotation)
        .unwrap();
    assert!(
        view.accessibility_action(target, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty()
    );
    let ids = view.annotation_editor_ids().unwrap();
    view.accessibility_action(
        ids.editor,
        None,
        AccessibilityAction::SetSelection(0..note().comment.encode_utf16().count()),
    )
    .unwrap();
    view.event(&ContentEvent::Preedit {
        text: "批注组字".into(),
        selection: 4..4,
        clauses: vec![],
        replacement: None,
    })
    .unwrap();
    assert!(
        view.accessibility_action(SELECT_B, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty()
    );
    assert!(
        view.accessibility_action(NEW_SESSION, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty()
    );
    assert!(
        view.accessibility_action(view.ids().send, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        a.current_run().unwrap().draft.annotations[0].comment,
        "原评论"
    );
    view.event(&ContentEvent::Commit {
        text: "更新后的批注\n第二行".into(),
        replacement: None,
    })
    .unwrap();
    assert_eq!(
        a.current_run().unwrap().draft.annotations[0].comment,
        "更新后的批注\n第二行"
    );
    view.select(1).unwrap();
    assert!(view.annotation_editor_ids().is_none());
    view.select(0).unwrap();
    a.edit_text(
        &ComposerDestination::run(a.scope().clone()),
        "提交正文".into(),
    )
    .unwrap();
    view.sync().unwrap();
    let mut commands = view
        .accessibility_action(view.ids().send, None, AccessibilityAction::Press)
        .unwrap();
    let DemoCommand::Send { binding, ticket } = commands.remove(0) else {
        panic!("send")
    };
    binding
        .dispatch_prepared_with(ticket, &ModelSelectionPreparation)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let s = binding.current_run().unwrap();
            if s.can_submit
                && s.transcript.items.iter().any(|i| {
                    i.kind == tekes_agent_presentation::TranscriptItemKind::Assistant
                        && i.text.contains("你好")
                })
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let snapshot = binding.current_run().unwrap();
    assert_eq!(
        snapshot
            .transcript
            .items
            .iter()
            .filter(
                |i| i.kind == tekes_agent_presentation::TranscriptItemKind::User
                    && i.text.contains("更新后的批注")
                    && i.text.contains("提交正文")
            )
            .count(),
        1
    );
    runtime.stop().await.unwrap();
}
