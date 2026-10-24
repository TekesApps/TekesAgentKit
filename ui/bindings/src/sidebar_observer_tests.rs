use super::*;
use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tekes_agent_contract::{
    EndpointCapabilities, EndpointConnector, FrameStream, HistoryPage, HistoryRequest,
    HostDescription, SessionEndpoint, StreamTarget,
};

struct Fixture {
    id: String,
    host: HostDescription,
    connects: AtomicUsize,
    follows: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
}
#[async_trait]
impl EndpointConnector for Fixture {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        self.connects.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(Endpoint {
            id: self.id.clone(),
            host: self.host.clone(),
            follows: self.follows.clone(),
            calls: self.calls.clone(),
        }))
    }
}
struct Endpoint {
    id: String,
    host: HostDescription,
    follows: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
}
#[async_trait]
impl SessionEndpoint for Endpoint {
    fn identity(&self) -> &str {
        &self.id
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn capabilities(&self) -> EndpointCapabilities {
        EndpointCapabilities::default()
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        self.follows.fetch_add(1, Ordering::SeqCst);
        let frame = match target {
            StreamTarget::SessionInventory => {
                json!({"type":"baseline","generation":"observer-fixture","snapshot":{"items":[{"sessionId":"a","title":"Owned thread","running":true,"blank":false,"updatedAt":1.0}]}})
            }
            StreamTarget::SessionControl => {
                json!({"type":"baseline","generation":"observer-fixture","snapshot":{"queues":[],"jobs":[],"projections":[]}})
            }
            StreamTarget::Actionables => {
                json!({"type":"reset","generation":"observer-fixture"})
            }
            _ => {
                return Err(unsupported(
                    "no history or workspace lease in observer fixture",
                ));
            }
        };
        Ok(Box::pin(
            stream::once(async move { Ok(frame) }).chain(stream::pending()),
        ))
    }
    async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
        Err(unsupported("no journal reads"))
    }
    async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
        Err(unsupported("no response"))
    }
    async fn call(&self, _: &str, _: Value, _: &str) -> Result<Value> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(unsupported("no mutations"))
    }
    async fn close(&self) {}
}
fn fixture(id: &str) -> Arc<Fixture> {
    Arc::new(Fixture {
        id: id.into(),
        host: serde_json::from_value(json!({"protocolVersion":4,"product":{"name":"Owned fixture","version":"1"},"capabilities":[],"cwd":"/fixture","home":"/fixture","attachedSessions":1,"canOpenPath":false})).unwrap(),
        connects: AtomicUsize::new(0), follows: Arc::new(AtomicUsize::new(0)), calls: Arc::new(AtomicUsize::new(0)),
    })
}
async fn setup() -> (Arc<SharedAgent>, Arc<Fixture>, Arc<SidebarBinding>) {
    let fixture = fixture("owned-observer-fixture");
    let mut agent = SharedAgent::default();
    agent
        .register(
            "owned-observer-fixture".into(),
            "native".into(),
            "Owned".into(),
            fixture.clone(),
        )
        .unwrap();
    let agent = Arc::new(agent);
    agent
        .endpoint("owned-observer-fixture")
        .unwrap()
        .runtime
        .wait_ready(Duration::from_secs(3))
        .await
        .unwrap();
    let binding = Arc::new(SidebarBinding::new(agent.clone(), "owned-observer-fixture").unwrap());
    wait_for(&binding, |p| p.loaded).await;
    (agent, fixture, binding)
}
async fn wait_for(binding: &SidebarBinding, accept: impl Fn(&SidebarPublication) -> bool) {
    let mut changes = binding.subscribe();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if accept(&binding.snapshot()) {
                return;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}
fn fail(binding: &SidebarBinding) {
    publish_observer_failure(
        &binding.observation,
        binding.observer_generation(),
        unsupported("owned terminal observation injection"),
    );
}
#[tokio::test]
async fn original_subscriber_recovers_and_old_frames_receipts_and_retry_tokens_are_retired() {
    let (agent, fixture, binding) = setup().await;
    let mut original_subscriber = binding.subscribe();
    let baseline = binding.snapshot();
    assert_eq!(baseline.rows.len(), 1);
    let address = baseline.rows[0].address.clone();
    let old_read = binding.read_selected(&address, 7).unwrap();
    assert!(old_read.is_current(&binding));
    let connects = fixture.connects.load(Ordering::SeqCst);
    let follows = fixture.follows.load(Ordering::SeqCst);
    fail(&binding);
    let retry = binding.capture_observer_retry().unwrap();
    let duplicate = binding.capture_observer_retry().unwrap();
    let later_stale = binding.capture_observer_retry().unwrap();
    assert_eq!(
        binding.snapshot().observation.phase,
        SidebarObserverPhase::Failed
    );
    assert!(binding.snapshot().rows[0].activity.is_stale);
    let before = binding.snapshot().revision;
    binding.restart_observer(retry).unwrap();
    let pending = binding.snapshot();
    assert!(pending.revision > before);
    assert_eq!(pending.observation.phase, SidebarObserverPhase::Restarting);
    assert!(pending.loaded && pending.observation.converged);
    assert_eq!(pending.rows.len(), baseline.rows.len());
    assert_eq!(pending.activity_at, baseline.activity_at);
    assert!(!old_read.is_current(&binding));
    assert!(binding.restart_observer(duplicate).is_err());
    let old = pending.observation.generation - 1;
    let frame =
        json!({"description":{"id":"owned-observer-fixture"},"sessions":[],"inventoryReady":true});
    assert!(
        !publish_inventory_frame(&binding.observation, &agent, &binding.endpoint, old, &frame)
            .unwrap()
    );
    publish_observer_failure(&binding.observation, old, unsupported("late old failure"));
    assert_eq!(binding.snapshot(), pending);
    let partial =
        json!({"description":{"id":"owned-observer-fixture"},"sessions":[],"inventoryReady":false});
    assert!(
        publish_inventory_frame(
            &binding.observation,
            &agent,
            &binding.endpoint,
            pending.observation.generation,
            &partial
        )
        .unwrap()
    );
    assert_eq!(
        binding.snapshot(),
        pending,
        "partial baseline cannot clear retained rows"
    );
    // Resume the original subscriber; production follow reads the real retained
    // runtime baseline. Only terminal failure/late-frame injection is private.
    wait_for(&binding, |p| {
        p.observation.phase == SidebarObserverPhase::Observing
    })
    .await;
    original_subscriber.changed().await.unwrap();
    let recovered = original_subscriber.borrow_and_update().clone();
    assert!(recovered.revision > pending.revision);
    assert_eq!(recovered.rows.len(), 1);
    assert!(!recovered.rows[0].activity.is_stale);
    assert_eq!(recovered.activity_at, baseline.activity_at);
    assert!(recovered.error.is_none());
    assert!(
        binding
            .read_selected(&address, 7)
            .unwrap()
            .is_current(&binding)
    );
    assert!(binding.capture_observer_retry().is_err());
    fail(&binding);
    assert!(binding.restart_observer(later_stale).is_err());
    assert_eq!(
        fixture.connects.load(Ordering::SeqCst),
        connects,
        "state-follow restart must not reconnect transport"
    );
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        fixture.follows.load(Ordering::SeqCst),
        follows,
        "state follow must not add provider streams or journal leases"
    );
    binding.runtime().stop().await.unwrap();
}
#[tokio::test]
async fn duplicate_and_cross_instance_retries_cannot_restart_another_attempt() {
    let (agent, _, binding) = setup().await;
    let other = SidebarBinding::new(agent, "owned-observer-fixture").unwrap();
    wait_for(&other, |p| p.loaded).await;
    fail(&binding);
    assert!(
        other
            .restart_observer(binding.capture_observer_retry().unwrap())
            .is_err()
    );
    let first = binding.capture_observer_retry().unwrap();
    let second = binding.capture_observer_retry().unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let results = std::thread::scope(|scope| {
        let b = barrier.clone();
        let source = binding.clone();
        let a = scope.spawn(move || {
            b.wait();
            source.restart_observer(first).is_ok()
        });
        let b = barrier.clone();
        let source = binding.clone();
        let c = scope.spawn(move || {
            b.wait();
            source.restart_observer(second).is_ok()
        });
        barrier.wait();
        (a.join().unwrap(), c.join().unwrap())
    });
    assert_ne!(results.0, results.1, "exactly one retry is admitted");
    assert_eq!(binding.observer_generation(), 2);
    wait_for(&binding, |p| {
        p.observation.phase == SidebarObserverPhase::Observing
    })
    .await;
    let abort = binding
        .observation
        .lock()
        .unwrap()
        .task
        .as_ref()
        .unwrap()
        .abort_handle();
    let runtime = binding.runtime().clone();
    drop(binding);
    tokio::time::timeout(Duration::from_secs(3), async {
        while !abort.is_finished() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn multi_retry_preserves_healthy_tickets_and_retires_only_the_failed_partition() {
    use crate::SidebarMultiEndpointBinding;
    let fixtures = [fixture("endpoint-a"), fixture("endpoint-b")];
    let mut agent = SharedAgent::default();
    for fixture in &fixtures {
        agent
            .register(
                fixture.id.clone(),
                "native".into(),
                fixture.id.clone(),
                fixture.clone(),
            )
            .unwrap();
    }
    let agent = Arc::new(agent);
    for fixture in &fixtures {
        agent
            .endpoint(&fixture.id)
            .unwrap()
            .runtime
            .wait_ready(Duration::from_secs(3))
            .await
            .unwrap();
    }
    let source =
        SidebarMultiEndpointBinding::new(agent.clone(), ["endpoint-a", "endpoint-b"]).unwrap();
    let a = source.binding_for("endpoint-a").unwrap();
    let b = source.binding_for("endpoint-b").unwrap();
    wait_for(a, |p| p.loaded).await;
    wait_for(b, |p| p.loaded).await;
    let address_a = a.snapshot().rows[0].address.clone();
    let address_b = b.snapshot().rows[0].address.clone();
    let action_a = a.snapshot().rows[0]
        .actions
        .items
        .iter()
        .find(|a| a.enabled && a.interaction == "copy")
        .unwrap()
        .clone();
    let action_b = b.snapshot().rows[0]
        .actions
        .items
        .iter()
        .find(|a| a.enabled && a.interaction == "copy")
        .unwrap()
        .clone();
    let old_a = source.capture_action(&address_a, &action_a).unwrap();
    let valid_b = source.capture_action(&address_b, &action_b).unwrap();
    let read_a = source.read_selected(&address_a, 4).unwrap();
    let read_b = source.read_selected(&address_b, 4).unwrap();
    let mut subscriber = source.subscribe();
    fail(a);
    let displayed = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let packet = source.observer_snapshot().unwrap();
            if packet.status().endpoints.iter().any(|e| {
                e.key.endpoint_id() == "endpoint-a"
                    && e.observation.phase == SidebarObserverPhase::Failed
            }) {
                break packet;
            }
            subscriber.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    let display_key = displayed
        .status()
        .endpoints
        .iter()
        .find(|e| e.key.endpoint_id() == "endpoint-a")
        .unwrap()
        .key
        .clone();
    assert_eq!(
        displayed.publication().revision,
        displayed.status().revision
    );
    for endpoint in &displayed.status().endpoints {
        assert_eq!(
            displayed.publication().endpoints[endpoint.key.endpoint_id()].observation,
            endpoint.observation
        );
    }
    let wrong_owner_token = source.capture_observer_retry("endpoint-a").unwrap();
    let replacement =
        SidebarMultiEndpointBinding::new(agent.clone(), ["endpoint-a", "endpoint-b"]).unwrap();
    assert!(replacement.restart_observer(wrong_owner_token).is_err());
    assert!(
        replacement
            .capture_observer_retry_for(&display_key)
            .is_err()
    );
    let retry = source.capture_observer_retry_for(&display_key).unwrap();
    let previous_b = b.snapshot().revision;
    agent.invalidate_thread_inventory();
    wait_for(b, |p| p.revision > previous_b).await;
    source.restart_observer(retry).unwrap();
    assert!(source.capture_observer_retry_for(&display_key).is_err());
    assert!(!read_a.is_current(&source));
    assert!(read_b.is_current(&source));
    wait_for(a, |p| {
        p.observation.generation == 2 && p.observation.phase == SidebarObserverPhase::Observing
    })
    .await;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if source
                .snapshot()
                .endpoints
                .get("endpoint-a")
                .is_some_and(|status| {
                    status.observation.generation == 2
                        && status.observation.phase == SidebarObserverPhase::Observing
                })
            {
                break;
            }
            subscriber.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(source.snapshot().rows.len(), 2);
    assert!(source.perform_action(old_a, None, "").await.is_err());
    assert!(
        matches!(source.perform_action(valid_b, None, "").await.unwrap(), SidebarActionOutcome::Copy { address, .. } if address == address_b)
    );
    let fresh_a = source.capture_action(&address_a, &action_a).unwrap();
    assert!(
        matches!(source.perform_action(fresh_a, None, "").await.unwrap(), SidebarActionOutcome::Copy { address, .. } if address == address_a)
    );
    assert!(source.capture_observer_retry("missing-endpoint").is_err());
    for fixture in &fixtures {
        assert_eq!(fixture.connects.load(Ordering::SeqCst), 1);
        assert_eq!(fixture.follows.load(Ordering::SeqCst), 3);
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
        agent
            .endpoint(&fixture.id)
            .unwrap()
            .runtime
            .stop()
            .await
            .unwrap();
    }
}
#[tokio::test]
async fn failure_before_initial_baseline_does_not_fabricate_loaded_during_restart() {
    let (agent, _, initial) = setup().await;
    let source = SidebarBinding::new(agent, "owned-observer-fixture").unwrap();
    // Single-thread runtime: the newly scheduled observer has not run yet.
    fail(&source);
    assert!(!source.snapshot().loaded);
    assert!(source.snapshot().observation.converged);
    source
        .restart_observer(source.capture_observer_retry().unwrap())
        .unwrap();
    let pending = source.snapshot();
    assert!(!pending.loaded);
    assert!(pending.observation.converged);
    assert_eq!(pending.observation.phase, SidebarObserverPhase::Restarting);
    wait_for(&source, |p| {
        p.loaded && p.observation.phase == SidebarObserverPhase::Observing
    })
    .await;
    initial.runtime().stop().await.unwrap();
}
#[tokio::test]
async fn counters_exhaust_without_repeated_publications_or_restarts() {
    let (_, _, binding) = setup().await;
    fail(&binding);
    {
        let mut state = binding.observation.lock().unwrap();
        state.generation = u64::MAX;
        let mut value = (**state.sender.as_ref().unwrap().borrow()).clone();
        value.observation.generation = u64::MAX;
        state.sender.as_ref().unwrap().send_replace(Arc::new(value));
    }
    assert!(
        binding
            .restart_observer(binding.capture_observer_retry().unwrap())
            .is_err()
    );
    let mut changes = binding.subscribe();
    {
        let mut state = binding.observation.lock().unwrap();
        let mut value = (**state.sender.as_ref().unwrap().borrow()).clone();
        value.revision = u64::MAX;
        state.sender.as_ref().unwrap().send_replace(Arc::new(value));
        changes.borrow_and_update();
        assert!(state.next_revision().is_err());
        assert!(state.sender.is_none());
    }
    assert!(
        changes.changed().await.is_err(),
        "exhaustion closes the publisher instead of repeating a revision"
    );
    assert!(binding.capture_observer_retry().is_err());
    binding.runtime().stop().await.unwrap();
}

#[derive(Default)]
struct ReentrantHost {
    binding: Mutex<std::sync::Weak<SidebarBinding>>,
    observed: std::sync::atomic::AtomicBool,
    locked: std::sync::atomic::AtomicBool,
}
#[async_trait]
impl tekes_agent_contract::ThreadActionHost for ReentrantHost {
    fn methods(&self) -> std::collections::BTreeSet<String> {
        if let Some(binding) = self.binding.lock().unwrap().upgrade() {
            self.observed.store(true, Ordering::SeqCst);
            let locked = binding.observation.try_lock().is_err();
            self.locked.store(locked, Ordering::SeqCst);
            if !locked {
                assert!(binding.observer_generation() > 0);
            }
        }
        Default::default()
    }
    fn pin_state(&self, _: &str, _: &str) -> Option<tekes_agent_contract::ThreadPinState> {
        None
    }
    async fn perform(
        &self,
        _: &str,
        _: &tekes_agent_contract::ThreadActionRequest,
        _: &str,
    ) -> Result<tekes_agent_contract::ThreadActionOutcome> {
        Err(unsupported("no fixture mutation"))
    }
}
#[tokio::test]
async fn inventory_host_callbacks_can_read_observer_state_without_reentering_its_lock() {
    let fixture = fixture("owned-observer-fixture");
    let host = Arc::new(ReentrantHost::default());
    let mut agent = SharedAgent::default();
    agent
        .register(
            fixture.id.clone(),
            "native".into(),
            "Owned".into(),
            fixture.clone(),
        )
        .unwrap();
    agent.install_thread_action_host(host.clone());
    let agent = Arc::new(agent);
    agent
        .endpoint(&fixture.id)
        .unwrap()
        .runtime
        .wait_ready(Duration::from_secs(3))
        .await
        .unwrap();
    let binding = Arc::new(SidebarBinding::new(agent, &fixture.id).unwrap());
    *host.binding.lock().unwrap() = Arc::downgrade(&binding);
    wait_for(&binding, |p| p.loaded).await;
    assert!(host.observed.load(Ordering::SeqCst));
    assert!(!host.locked.load(Ordering::SeqCst));
    binding.runtime().stop().await.unwrap();
}

#[tokio::test]
async fn display_packet_keys_cannot_be_reminted_or_reused_for_a_later_failure() {
    use tekes_agent_presentation::SidebarObserverKey;
    let (agent, _, binding) = setup().await;
    let initial = binding.observer_snapshot().unwrap();
    assert_eq!(initial.publication().revision, initial.status().revision);
    assert_eq!(
        initial.publication().observation,
        initial.status().endpoints[0].observation
    );
    assert!(
        binding
            .capture_observer_retry_for(&initial.status().endpoints[0].key)
            .is_err()
    );
    fail(&binding);
    let displayed = binding.observer_snapshot().unwrap();
    let key = displayed.status().endpoints[0].key.clone();
    assert_eq!(
        key.generation(),
        displayed.publication().observation.generation
    );
    let replacement = SidebarBinding::new(agent, "owned-observer-fixture").unwrap();
    assert!(replacement.capture_observer_retry_for(&key).is_err());
    let fake_owner = Arc::new(9u8);
    let forged = SidebarObserverKey::from_owners(
        &fake_owner,
        &fake_owner,
        key.endpoint_id(),
        key.generation(),
    )
    .unwrap();
    assert!(binding.capture_observer_retry_for(&forged).is_err());
    // Mutating a public copy does not mint a new key or change the packet.
    let mut edited = displayed.publication().as_ref().clone();
    edited.observation.generation += 1;
    assert_ne!(
        edited.observation,
        displayed.status().endpoints[0].observation
    );
    binding
        .restart_observer(binding.capture_observer_retry_for(&key).unwrap())
        .unwrap();
    wait_for(&binding, |p| {
        p.observation.phase == SidebarObserverPhase::Observing
    })
    .await;
    fail(&binding);
    assert!(binding.capture_observer_retry_for(&key).is_err());
    let next = binding.observer_snapshot().unwrap();
    assert!(next.status().endpoints[0].key.generation() > key.generation());
    assert!(
        binding
            .capture_observer_retry_for(&next.status().endpoints[0].key)
            .is_ok()
    );
}
