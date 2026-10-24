use async_trait::async_trait;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tekes_agent_contract::*;
use tekes_agent_presentation::*;
use tekes_agent_session::*;

#[derive(Clone)]
struct Connector {
    endpoint_id: String,
    supported: bool,
    delay: Duration,
    failures_left: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
    queries: Arc<Mutex<Vec<String>>>,
}

impl Connector {
    fn new(endpoint_id: &str, supported: bool) -> Self {
        Self {
            endpoint_id: endpoint_id.into(),
            supported,
            delay: Duration::ZERO,
            failures_left: Arc::new(AtomicUsize::new(0)),
            calls: Arc::new(AtomicUsize::new(0)),
            queries: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    fn fail_first(&self) {
        self.failures_left.store(1, Ordering::SeqCst);
    }
}

struct Endpoint {
    connector: Connector,
    host: HostDescription,
}

#[async_trait]
impl EndpointConnector for Connector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        let host = serde_json::from_value(json!({
            "protocolVersion": 4,
            "product": {"name": "multi-search-test", "version": "1"},
            "capabilities": ["session-journal", "session-inventory-sync", "session-control-sync", "actionable-sync"],
            "cwd": "/work",
            "home": "/home",
            "attachedSessions": 0,
            "canOpenPath": false
        }))?;
        Ok(Arc::new(Endpoint {
            connector: self.clone(),
            host,
        }))
    }
}

#[async_trait]
impl SessionEndpoint for Endpoint {
    fn identity(&self) -> &str {
        &self.connector.endpoint_id
    }

    fn host(&self) -> &HostDescription {
        &self.host
    }

    fn capabilities(&self) -> EndpointCapabilities {
        let mut methods = vec!["session.prompt"];
        if self.connector.supported {
            methods.push("session.search");
        }
        EndpointCapabilities::methods(&methods)
    }

    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        use futures_util::{StreamExt, stream};
        let value = match target {
            StreamTarget::SessionInventory => {
                json!({"type":"baseline","generation":"g","snapshot":{"items":[]}})
            }
            StreamTarget::SessionControl => json!({
                "type":"baseline","generation":"g","snapshot":{"queues":[],"jobs":[],"projections":[]}
            }),
            StreamTarget::Actionables => json!({"type":"reset","generation":"g"}),
            StreamTarget::SessionJournal { address, .. } => json!({
                "type":"snapshot","snapshot":{"sessionId":address.session_id,"generation":"g","throughSequence":-1,"windowLimit":50,"entries":[],"hasMoreBefore":false}
            }),
            _ => unreachable!(),
        };
        Ok(Box::pin(
            stream::once(async { Ok(value) }).chain(stream::pending()),
        ))
    }

    async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
        unreachable!()
    }

    async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
        Ok(())
    }

    async fn call(&self, method: &str, payload: Value, _: &str) -> Result<Value> {
        if method != "session.search" {
            return Ok(json!({"accepted":true}));
        }
        let call_id = self.connector.calls.fetch_add(1, Ordering::SeqCst) + 1;
        let query = payload["query"].as_str().unwrap_or_default().to_owned();
        self.connector.queries.lock().unwrap().push(query.clone());
        tokio::time::sleep(self.connector.delay).await;
        if self
            .connector
            .failures_left
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                left.checked_sub(1)
            })
            .is_ok()
        {
            return Err(Error::Transport("fixture search failed".into()));
        }
        Ok(json!({
            "items":[{"sessionId":"same-session","snippet":query,"fixtureCallId":call_id}],
            "hasMore":false
        }))
    }

    async fn close(&self) {}
}

async fn runtime(connector: Connector) -> Arc<SessionRuntime> {
    let runtime = Arc::new(SessionRuntime::spawn(
        connector.endpoint_id.clone(),
        Arc::new(connector),
    ));
    runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
    runtime
}

async fn wait_for(
    controller: &SidebarMultiSearchController,
    predicate: impl Fn(&SidebarMultiSearchPublication) -> bool,
) -> Arc<SidebarMultiSearchPublication> {
    let mut changes = controller.subscribe();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let snapshot = controller.snapshot();
            if predicate(&snapshot) {
                return snapshot;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .expect("multi-search publication timed out")
}

#[tokio::test]
async fn rejects_empty_and_duplicate_endpoint_sets() {
    assert!(SidebarMultiSearchController::new(vec![]).is_err());
    let first = runtime(Connector::new("same", true)).await;
    let second = runtime(Connector::new("same", true)).await;
    assert!(SidebarMultiSearchController::new(vec![first.clone(), second.clone()]).is_err());
    first.stop().await.unwrap();
    second.stop().await.unwrap();
}

#[tokio::test]
async fn identical_query_is_not_reissued_and_publications_keep_each_endpoint() {
    let left = Connector::new("left", true);
    let right = Connector::new("right", true);
    let left_runtime = runtime(left.clone()).await;
    let right_runtime = runtime(right.clone()).await;
    let controller =
        SidebarMultiSearchController::new(vec![left_runtime.clone(), right_runtime.clone()])
            .unwrap();

    controller.set_query("中文 search").unwrap();
    let done = wait_for(&controller, |publication| {
        publication.children.iter().all(|child| {
            matches!(
                child.publication.input.phase,
                SidebarSearchPhase::Complete { .. }
            )
        })
    })
    .await;
    let generation = done.input.key.as_ref().unwrap().generation;
    controller.set_query("中文 search").unwrap();
    tokio::time::sleep(Duration::from_millis(350)).await;
    assert_eq!(left.calls.load(Ordering::SeqCst), 1);
    assert_eq!(right.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        controller.snapshot().input.key.as_ref().unwrap().generation,
        generation
    );
    assert_eq!(done.children.len(), 2);
    assert_eq!(
        done.children[0].publication.hits[0].address.session_id,
        done.children[1].publication.hits[0].address.session_id
    );
    assert_ne!(
        done.children[0].publication.hits[0].address.endpoint_id,
        done.children[1].publication.hits[0].address.endpoint_id
    );
    assert!(
        done.children
            .iter()
            .all(|child| child.publication.hits.len() == 1)
    );
    left_runtime.stop().await.unwrap();
    right_runtime.stop().await.unwrap();
}

#[tokio::test]
async fn aba_and_clear_retire_old_attempts_and_old_retry_tickets() {
    let slow = Connector::new("slow", true).with_delay(Duration::from_millis(400));
    let runtime = runtime(slow.clone()).await;
    let controller = SidebarMultiSearchController::new(vec![runtime.clone()]).unwrap();

    controller.set_query("A").unwrap();
    let first = controller.snapshot().input.key.clone().unwrap();
    tokio::time::sleep(Duration::from_millis(290)).await;
    controller.set_query("B").unwrap();
    controller.set_query("A").unwrap();
    let current = controller.snapshot().input.key.clone().unwrap();
    assert_ne!(first.generation, current.generation);
    let returned = wait_for(&controller, |publication| {
        publication.children[0]
            .publication
            .hits
            .first()
            .is_some_and(|hit| hit.hit.snippet == "A")
    })
    .await;
    assert_eq!(returned.input.key.as_ref(), Some(&current));
    assert_eq!(
        returned.children[0].publication.hits[0].hit.extra["fixtureCallId"],
        json!(2),
        "the old A response cannot supply the newer A query result"
    );

    controller.set_query("late result").unwrap();
    tokio::time::sleep(Duration::from_millis(290)).await;
    controller.set_query("").unwrap();
    let cleared = controller.snapshot();
    assert!(cleared.input.key.is_none());
    assert!(cleared.children.iter().all(|child| {
        child.publication.input.key.is_none() && child.publication.hits.is_empty()
    }));
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        controller.snapshot().children[0]
            .publication
            .hits
            .is_empty()
    );

    let old_ticket = SidebarMultiSearchIntent::Retry {
        aggregate_key: first,
        child_key: SidebarSearchKey {
            owner: "stale".into(),
            endpoint_id: "slow".into(),
            connection_epoch: 1,
            generation: 1,
            query: "A".into(),
        },
    };
    assert!(controller.retry(&old_ticket).is_err());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn unsupported_endpoint_keeps_its_status_without_erasing_other_hits() {
    let supported = Connector::new("supported", true);
    let unsupported = Connector::new("unsupported", false);
    let supported_runtime = runtime(supported.clone()).await;
    let unsupported_runtime = runtime(unsupported.clone()).await;
    let controller = SidebarMultiSearchController::new(vec![
        supported_runtime.clone(),
        unsupported_runtime.clone(),
    ])
    .unwrap();

    controller.set_query("query").unwrap();
    let done = wait_for(&controller, |publication| {
        publication.children.iter().any(|child| {
            child.endpoint_id == "supported"
                && matches!(
                    child.publication.input.phase,
                    SidebarSearchPhase::Complete { .. }
                )
        }) && publication.children.iter().any(|child| {
            child.endpoint_id == "unsupported"
                && matches!(
                    child.publication.input.phase,
                    SidebarSearchPhase::Failed { .. }
                )
        })
    })
    .await;
    let good = done
        .children
        .iter()
        .find(|child| child.endpoint_id == "supported")
        .unwrap();
    let unavailable = done
        .children
        .iter()
        .find(|child| child.endpoint_id == "unsupported")
        .unwrap();
    assert_eq!(good.publication.hits.len(), 1);
    assert!(matches!(
        unavailable.publication.input.phase,
        SidebarSearchPhase::Failed {
            retryable: false,
            ..
        }
    ));
    supported_runtime.stop().await.unwrap();
    unsupported_runtime.stop().await.unwrap();
}

#[tokio::test]
async fn retry_uses_both_keys_and_only_retries_the_original_failed_endpoint() {
    let failed = Connector::new("failed", true);
    failed.fail_first();
    let good = Connector::new("good", true);
    let failed_runtime = runtime(failed.clone()).await;
    let good_runtime = runtime(good.clone()).await;
    let controller =
        SidebarMultiSearchController::new(vec![failed_runtime.clone(), good_runtime.clone()])
            .unwrap();

    controller.set_query("retry me").unwrap();
    let complete = wait_for(&controller, |publication| {
        publication.children.iter().all(|child| {
            matches!(
                child.publication.input.phase,
                SidebarSearchPhase::Complete { .. } | SidebarSearchPhase::Failed { .. }
            )
        }) && publication.children.iter().any(|child| {
            child.endpoint_id == "failed"
                && matches!(
                    child.publication.input.phase,
                    SidebarSearchPhase::Failed {
                        retryable: true,
                        ..
                    }
                )
        })
    })
    .await;
    let agg = complete.input.key.clone().unwrap();
    let child = complete
        .children
        .iter()
        .find(|child| child.endpoint_id == "failed")
        .unwrap()
        .publication
        .input
        .key
        .clone()
        .unwrap();
    let good_calls = good.calls.load(Ordering::SeqCst);
    controller
        .retry(&SidebarMultiSearchIntent::Retry {
            aggregate_key: agg.clone(),
            child_key: child.clone(),
        })
        .unwrap();
    wait_for(&controller, |publication| {
        publication.children.iter().any(|child| {
            child.endpoint_id == "failed"
                && matches!(
                    child.publication.input.phase,
                    SidebarSearchPhase::Complete { .. }
                )
        })
    })
    .await;
    assert_eq!(failed.calls.load(Ordering::SeqCst), 2);
    assert_eq!(good.calls.load(Ordering::SeqCst), good_calls);

    let mut wrong_child = child.clone();
    wrong_child.generation += 100;
    assert!(
        controller
            .retry(&SidebarMultiSearchIntent::Retry {
                aggregate_key: controller.snapshot().input.key.clone().unwrap(),
                child_key: wrong_child,
            })
            .is_err()
    );
    let foreign =
        SidebarMultiSearchController::new(vec![failed_runtime.clone(), good_runtime.clone()])
            .unwrap();
    foreign.set_query("retry me").unwrap();
    wait_for(&foreign, |publication| {
        publication.children.iter().all(|child| {
            matches!(
                child.publication.input.phase,
                SidebarSearchPhase::Complete { .. } | SidebarSearchPhase::Failed { .. }
            )
        })
    })
    .await;
    assert!(
        foreign
            .retry(&SidebarMultiSearchIntent::Retry {
                aggregate_key: agg,
                child_key: child,
            })
            .is_err()
    );
    failed_runtime.stop().await.unwrap();
    good_runtime.stop().await.unwrap();
}

#[tokio::test]
async fn reconnect_rekeys_only_the_original_endpoint_and_rejects_its_old_retry_key() {
    let left = Connector::new("left", true);
    let right = Connector::new("right", true);
    let left_runtime = runtime(left.clone()).await;
    let right_runtime = runtime(right.clone()).await;
    let controller =
        SidebarMultiSearchController::new(vec![left_runtime.clone(), right_runtime.clone()])
            .unwrap();
    controller.set_query("kept query").unwrap();
    let before = wait_for(&controller, |p| {
        p.children.iter().all(|c| {
            matches!(
                c.publication.input.phase,
                SidebarSearchPhase::Complete { .. }
            )
        })
    })
    .await;
    let left_key = before.children[0].publication.input.key.clone().unwrap();
    let right_key = before.children[1].publication.input.key.clone().unwrap();
    let aggregate_key = before.input.key.clone().unwrap();
    left_runtime.reconnect().await.unwrap();
    left_runtime
        .wait_ready(Duration::from_secs(2))
        .await
        .unwrap();
    let after = wait_for(&controller, |p| {
        p.children[0]
            .publication
            .input
            .key
            .as_ref()
            .is_some_and(|key| key.connection_epoch > left_key.connection_epoch)
            && p.children.iter().all(|c| {
                matches!(
                    c.publication.input.phase,
                    SidebarSearchPhase::Complete { .. }
                )
            })
    })
    .await;
    assert_eq!(after.input.key.as_ref(), Some(&aggregate_key));
    assert_eq!(
        after.children[1].publication.input.key.as_ref(),
        Some(&right_key)
    );
    assert_eq!(left.calls.load(Ordering::SeqCst), 2);
    assert_eq!(right.calls.load(Ordering::SeqCst), 1);
    assert!(
        controller
            .retry(&SidebarMultiSearchIntent::Retry {
                aggregate_key,
                child_key: left_key
            })
            .is_err()
    );
    left_runtime.stop().await.unwrap();
    right_runtime.stop().await.unwrap();
}

#[tokio::test]
async fn dropping_the_owner_publishes_retirement_and_late_io_cannot_restore_hits() {
    let source = Connector::new("slow", true).with_delay(Duration::from_millis(400));
    let runtime = runtime(source.clone()).await;
    let controller = SidebarMultiSearchController::new(vec![runtime.clone()]).unwrap();
    let mut updates = controller.subscribe();
    controller.set_query("retire").unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while source.calls.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    updates.borrow_and_update();
    drop(controller);
    updates.changed().await.unwrap();
    let retired = updates.borrow_and_update().clone();
    assert!(retired.input.key.is_none());
    assert!(
        retired
            .children
            .iter()
            .all(|c| c.publication.input.key.is_none() && c.publication.hits.is_empty())
    );
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(updates.borrow().as_ref(), retired.as_ref());
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
    runtime.stop().await.unwrap();
}
