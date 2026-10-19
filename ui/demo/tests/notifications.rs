use std::{sync::Arc, time::Duration};
use tekes_agent_contract::PromptMode;
use tekes_agent_presentation::{ComposerDestination, RunScope, RunSnapshot};

async fn receive(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<Arc<RunSnapshot>>,
    predicate: impl Fn(&RunSnapshot) -> bool,
) -> Arc<RunSnapshot> {
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let s = rx.recv().await.expect("live notification subscription");
            if predicate(&s) {
                return s;
            }
        }
    })
    .await
    .expect("publication must notify without a polling frame")
}
#[tokio::test]
async fn existing_presentation_notifies_stream_cancel_and_connection_without_polling() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let c = controller.clone();
    let task = tokio::spawn(tekes_agent_ui_demo::notifications::forward(
        controller.subscribe(),
        move || tx.send(c.snapshot()).is_ok(),
    ));
    receive(&mut rx, |s| s.can_submit).await;
    // Once startup settles, idle has no periodic business publication.
    while rx.try_recv().is_ok() {}
    assert!(
        tokio::time::timeout(Duration::from_millis(150), rx.recv())
            .await
            .is_err()
    );
    let scope = RunScope::main("fixture", "a");
    let destination = ComposerDestination::run(scope.clone());
    let mut draft = controller.snapshot().draft.clone();
    draft.text = "通知流式测试".into();
    controller.edit(&destination, draft).unwrap();
    let ticket = controller.prepare(&destination, PromptMode::Queue).unwrap();
    controller.dispatch(ticket, None).await.unwrap();
    receive(&mut rx, |s| {
        s.transcript
            .items
            .iter()
            .any(|i| i.is_provisional && !i.text.is_empty())
    })
    .await;
    controller.cancel(&scope).await.unwrap();
    receive(&mut rx, |s| {
        s.run.as_ref().is_some_and(|r| !r.can_cancel)
            && !s.transcript.items.iter().any(|i| i.is_provisional)
    })
    .await;
    let old = controller.snapshot().connection.phase.clone();
    runtime.stop().await.unwrap();
    receive(&mut rx, |s| s.connection.phase != old).await;
    task.abort();
}
#[tokio::test]
async fn notification_during_delivery_is_not_lost_and_closed_target_stops() {
    let (tx, rx) = tokio::sync::watch::channel(0u64);
    let changes = tx.clone();
    let (seen, mut received) = tokio::sync::mpsc::unbounded_channel();
    let mut calls = 0;
    let worker = tokio::spawn(tekes_agent_ui_demo::notifications::forward(rx, move || {
        calls += 1;
        seen.send(*changes.borrow()).unwrap();
        if calls == 1 {
            changes.send_replace(42);
        }
        calls < 2
    }));
    worker.await.unwrap();
    assert_eq!(received.recv().await, Some(0));
    assert_eq!(received.recv().await, Some(42));
    assert!(received.recv().await.is_none());
}
