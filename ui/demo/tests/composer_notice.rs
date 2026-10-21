use tekes_agent_components_ui::*;
use tekes_ui_controls::composition::*;
fn input(revision: u64) -> ComposerNoticeInput {
    ComposerNoticeInput {
        connection_epoch: 1,
        revision,
        destination: ComposerDestination::run(RunScope::main("fixture", "a")),
        enabled: true,
        notice: Some(ComposerNoticeState {
            text: "连接目录暂不可用，请显式刷新".into(),
            can_refresh: true,
            refreshing: false,
        }),
    }
}
fn notice(host: &ContentHost, root: ComponentId) -> &ComposerNotice {
    host.get(root).unwrap().component().unwrap()
}
fn notice_mut(host: &mut ContentHost, root: ComponentId) -> &mut ComposerNotice {
    host.get_mut(root).unwrap().component_mut().unwrap()
}
#[test]
fn requests_capture_original_destination_reject_late_frames_and_share_one_claim() {
    let mut view = ComposerNotice::new(29001, Default::default());
    let first = input(1);
    view.update(&first).unwrap();
    let root = view.id();
    let refresh = view.refresh_id();
    let mut host = view.into_host().unwrap();
    host.accessibility_action(refresh, None, AccessibilityAction::Press);
    let request = notice_mut(&mut host, root).take_refreshes().remove(0);
    assert_eq!(request.source(), &first);
    notice(&host, root).claim_refresh(&request).unwrap();
    assert!(notice(&host, root).claim_refresh(&request.clone()).is_err());
    host.accessibility_action(refresh, None, AccessibilityAction::Press);
    let late = notice_mut(&mut host, root).take_refreshes().remove(0);
    let mut next = input(2);
    next.notice.as_mut().unwrap().text = "新来源".into();
    notice_mut(&mut host, root).update(&next).unwrap();
    assert!(notice(&host, root).claim_refresh(&late).is_err());
    assert_eq!(
        late.source().notice.as_ref().unwrap().text,
        first.notice.as_ref().unwrap().text
    );
    let mut conflict = next.clone();
    conflict.notice = None;
    assert!(notice_mut(&mut host, root).update(&conflict).is_err());
    assert_eq!(notice(&host, root).displayed_input(), Some(&next));
    next.revision += 1;
    next.notice.as_mut().unwrap().refreshing = true;
    notice_mut(&mut host, root).update(&next).unwrap();
    host.accessibility_action(refresh, None, AccessibilityAction::Press);
    assert!(notice_mut(&mut host, root).take_refreshes().is_empty());
    next.revision += 1;
    next.notice = None;
    notice_mut(&mut host, root).update(&next).unwrap();
    assert!(host.semantics().is_empty());
    assert!(notice(&host, root).claim_refresh(&late).is_err());
}
#[tokio::test]
async fn composer_owns_notice_scope_without_replacing_editor_and_retires_it_on_archive() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let ids = ComposerIds::new(29000);
    let mut snapshot = (*controller.snapshot()).clone();
    let mut view = Composer::new(ids);
    view.update(&snapshot, Default::default()).unwrap();
    view.install_notice(29002, Default::default()).unwrap();
    let mut source = input(1);
    source.destination = view.destination().unwrap().clone();
    source.connection_epoch = snapshot.connection_epoch;
    view.update_notice(&source).unwrap();
    let mut host = view.into_host().unwrap();
    let editor = host
        .get(ids.root)
        .unwrap()
        .component::<Composer>()
        .unwrap()
        .editor() as *const _;
    let refresh = host
        .get(ids.root)
        .unwrap()
        .component::<Composer>()
        .unwrap()
        .notice()
        .unwrap()
        .refresh_id();
    host.accessibility_action(refresh, None, AccessibilityAction::Press);
    let old_request = host
        .get_mut(ids.root)
        .unwrap()
        .component_mut::<Composer>()
        .unwrap()
        .notice_mut()
        .unwrap()
        .take_refreshes()
        .remove(0);
    snapshot.connection_epoch += 1;
    snapshot.revision += 1;
    let composer = host
        .get_mut(ids.root)
        .unwrap()
        .component_mut::<Composer>()
        .unwrap();
    composer.update(&snapshot, Default::default()).unwrap();
    assert!(
        composer
            .notice()
            .unwrap()
            .claim_refresh(&old_request)
            .is_err()
    );
    assert!(composer.notice().unwrap().displayed_input().is_none());
    source.revision = 2;
    assert!(
        composer.update_notice(&source).is_err(),
        "new display revision cannot revive old connection facts"
    );
    source.connection_epoch = snapshot.connection_epoch;
    composer.update_notice(&source).unwrap();
    assert_eq!(composer.editor() as *const _, editor);
    source.revision = 3;
    source.destination = ComposerDestination::run(RunScope::main("fixture", "wrong"));
    assert!(
        host.get_mut(ids.root)
            .unwrap()
            .component_mut::<Composer>()
            .unwrap()
            .update_notice(&source)
            .is_err()
    );
    let mut refreshed = snapshot.run.as_ref().unwrap().clone();
    refreshed.phase = tekes_agent_presentation::RunPhase::Archived;
    snapshot.run = Some(refreshed);
    snapshot.revision += 1;
    let composer = host
        .get_mut(ids.root)
        .unwrap()
        .component_mut::<Composer>()
        .unwrap();
    composer.update(&snapshot, Default::default()).unwrap();
    assert_eq!(composer.editor() as *const _, editor);
    assert!(composer.notice().unwrap().displayed_input().is_none());
    assert!(composer.update_notice(&input(3)).is_err());
    runtime.stop().await.unwrap();
}
