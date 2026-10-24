#[path = "../src/native_child_error_dispatch.rs"]
mod dispatch;

use std::{sync::Arc, time::Duration};
use tekes_agent_presentation::RunScope;
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::RunBinding;
use tekes_agent_ui_demo::{component_demo::ComponentDemo, fixture::Fixture};

async fn setup() -> (Arc<SessionRuntime>, ComponentDemo) {
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let first = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let second = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "b"))
            .await
            .unwrap(),
    );
    (runtime, ComponentDemo::new([first, second]).unwrap())
}

#[tokio::test]
async fn native_child_open_error_notice_is_delivered_only_for_current_request() {
    let (runtime, mut view) = setup().await;
    let parent = view.binding().scope().clone();
    let current = view.begin_child_open(&parent).unwrap();

    assert_eq!(
        dispatch::current_child_open_error(&view, &current, "child read failed".into()),
        Some("child read failed".into()),
        "the native HostResult branch forwards an admitted request to Host::failure"
    );

    view.select(1).unwrap();
    assert_eq!(
        dispatch::current_child_open_error(&view, &current, "stale child read failed".into()),
        None,
        "a late error from the old parent must not create a host notice"
    );
    runtime.stop().await.unwrap();
}
