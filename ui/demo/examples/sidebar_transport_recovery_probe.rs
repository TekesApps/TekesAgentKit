//! Real endpoint close -> runtime failure -> original component reconnect -> fresh Sidebar.
//! No window, screenshot, private session storage, or business mutation is used.
#[path = "sidebar_transport_recovery/gate.rs"]
mod gate;

use gate::Gate;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    env, fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tekes_agent_components_ui::Sidebar;
use tekes_agent_contract::{Error, Result, require, unsupported};
use tekes_agent_presentation::{ComposerDestination, RunScope, SidebarObserverPhase};
use tekes_agent_session::{ConnectionState, SessionRuntime};
use tekes_agent_ui_bindings::{PaneSnapshot, RunBinding, SidebarActionOutcome};
use tekes_agent_ui_demo::{
    component_demo::{ComponentDemo, DemoCommand, RECONNECT},
    endpoint_config::HostEndpointConfiguration,
    sidebar_host::{SidebarHostBinding, SidebarHostPublication},
};
use tekes_agent_web::SharedAgent;
use tekes_ui_controls::composition::AccessibilityAction;

const ENDPOINTS: [&str; 3] = ["native", "pi", "dsh"];
const WAIT: Duration = Duration::from_secs(40);
type Runtimes = BTreeMap<String, Arc<SessionRuntime>>;

fn args() -> Result<(PathBuf, PathBuf, Vec<String>)> {
    let mut args = env::args().skip(1);
    let mut config = None;
    let mut receipt = None;
    let mut selected = Vec::new();
    while let Some(arg) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| unsupported("missing probe argument value"))?;
        match arg.as_str() {
            "--endpoint-config" if config.is_none() => config = Some(PathBuf::from(value)),
            "--receipt" if receipt.is_none() => receipt = Some(PathBuf::from(value)),
            "--endpoint" if ENDPOINTS.contains(&value.as_str()) && !selected.contains(&value) => {
                selected.push(value)
            }
            _ => return Err(unsupported("invalid/duplicate probe argument")),
        }
    }
    let config = config.ok_or_else(|| unsupported("missing endpoint configuration"))?;
    let receipt = receipt.ok_or_else(|| unsupported("missing receipt"))?;
    require(
        config.is_absolute() && receipt.is_absolute() && !receipt.exists(),
        "probe requires absolute inputs and a new receipt",
    )?;
    if selected.is_empty() {
        selected = ENDPOINTS.iter().map(|id| id.to_string()).collect();
    }
    Ok((config, receipt, selected))
}

fn sidebar(view: &ComponentDemo) -> Result<&Sidebar> {
    view.tree
        .get(view.sidebar_id())
        .and_then(|node| node.component::<Sidebar>())
        .ok_or_else(|| unsupported("original Sidebar not mounted"))
}

async fn handle_reads(view: &mut ComponentDemo, commands: Vec<DemoCommand>) -> Result<()> {
    for command in commands {
        match command {
            DemoCommand::ReadInitialFeedback(request) => {
                view.complete_initial_feedback(request.execute().await)?
            }
            _ => return Err(unsupported("unexpected command in read-only recovery host")),
        }
    }
    Ok(())
}

async fn sync(view: &mut ComponentDemo, host: &SidebarHostBinding) -> Result<()> {
    view.set_sidebar_host_source(host.snapshot())?;
    view.sync()?;
    let commands = view.take_background_commands();
    handle_reads(view, commands).await
}

async fn wait_source(
    host: &SidebarHostBinding,
    predicate: impl Fn(&SidebarHostPublication) -> bool,
) -> Result<()> {
    let mut updates = host.subscribe();
    tokio::time::timeout(WAIT, async {
        loop {
            if predicate(&host.snapshot()) {
                return Ok(());
            }
            updates.changed().await.map_err(|_| Error::Disconnected)?;
        }
    })
    .await
    .map_err(|_| Error::Timeout)?
}

fn fresh_source(source: &SidebarHostPublication, runtimes: &Runtimes) -> bool {
    let Some(multi) = source.as_multi_publication() else {
        return false;
    };
    multi.rows.len() == 9
        && ENDPOINTS.iter().all(|id| {
            multi.name_corpora.get(*id).is_some_and(|corpus| {
                corpus.inventory_ready
                    && corpus.connection_epoch == runtimes[*id].inspect(|s| s.connection_epoch)
            }) && multi.endpoints.get(*id).is_some_and(|status| {
                status.loaded
                    && !status.stale
                    && status.error.is_none()
                    && status.observation.phase == SidebarObserverPhase::Observing
            })
        })
}

async fn drafts(bindings: &[Arc<RunBinding>]) -> Result<Vec<Value>> {
    let mut result = Vec::new();
    for binding in bindings {
        let PaneSnapshot::Run { snapshot, .. } = binding.snapshot().await? else {
            return Err(unsupported("recovery pane is not writable Run"));
        };
        result.push(json!({"scope":binding.scope(),"text":snapshot.draft.text}));
    }
    Ok(result)
}

async fn stage(
    view: &mut ComponentDemo,
    host: &SidebarHostBinding,
    runtimes: &Runtimes,
    scopes: &BTreeMap<String, RunScope>,
    bindings: &[Arc<RunBinding>],
    gate: &Gate,
) -> Result<Value> {
    let source = host.snapshot();
    view.set_sidebar_host_source(source.clone())?;
    view.sync()?;
    let commands = view.take_background_commands();
    handle_reads(view, commands).await?;
    let delivery = view.sidebar_delivery_receipt()?;
    let source_actions: Vec<_> = source
        .as_publication()
        .rows
        .iter()
        .map(|row| {
            json!({
                "address":{"endpointId":row.address.endpoint_id,"sessionId":row.address.session_id},
                "actions":row.actions
            })
        })
        .collect();
    let mounts: Vec<_> = delivery["host"]["mountedRows"].as_array().ok_or_else(|| unsupported("missing mounted rows"))?.iter().map(|row| json!({"address":row["address"],"rowId":row["rowId"],"mounted":row["presentInContentHost"]})).collect();
    require(
        mounts.len() == 9 && mounts.iter().all(|row| row["mounted"] == true),
        "all original source rows must be mounted",
    )?;
    let runtime: BTreeMap<_, _> = runtimes.iter().map(|(id, runtime)| {
        let scope = &scopes[id];
        (id.clone(), runtime.inspect(|state| json!({
            "connectionPhase":match state.connection {ConnectionState::Ready => "ready",ConnectionState::Failed(_) => "failed",_ => "connecting"},
            "connectionEpoch":state.connection_epoch,"inventoryReady":state.inventory_ready,
            "selectedJournalReady":state.sessions.get(&scope.session_id).is_some_and(|session| session.journal_ready)
        })))
    }).collect();
    let retry: Vec<_> = ENDPOINTS
        .iter()
        .filter(|id| sidebar(view).is_ok_and(|s| s.observer_retry_id(id).is_some()))
        .copied()
        .collect();
    Ok(
        json!({"delivery":delivery,"sourceActions":source_actions,"runtime":runtime,"mounts":mounts,
        "selectedScope":view.current_binding().ok_or_else(|| unsupported("selected pane missing"))?.scope(),
        "drafts":drafts(bindings).await?,"observerRetryEndpoints":retry,"gate":gate.snapshot()}),
    )
}

async fn wait_failed(runtime: &SessionRuntime) -> Result<()> {
    let started = Instant::now();
    while !runtime.inspect(|state| matches!(state.connection, ConnectionState::Failed(_))) {
        require(
            started.elapsed() < WAIT,
            "real closed transport did not exhaust failure budget",
        )?;
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    Ok(())
}

async fn execute(config: Value, selected: &[String]) -> Result<Value> {
    let configured = HostEndpointConfiguration::parse(&config, |key| env::var(key).ok())?
        .into_connectors()
        .await?;
    require(
        configured.len() == 3
            && configured
                .iter()
                .map(|c| c.scopes[0].endpoint_id.as_str())
                .eq(ENDPOINTS),
        "probe requires original Native/Pi/DSH endpoint order",
    )?;
    let mut agent = SharedAgent::default();
    agent.set_session_url_scheme("agentkit-recovery")?;
    let mut gates = BTreeMap::new();
    let mut all_scopes = Vec::new();
    let mut scopes = BTreeMap::new();
    for config in configured {
        let id = config.scopes[0].endpoint_id.clone();
        let gate = Arc::new(Gate::new(config.connector));
        agent.register(id.clone(), config.kind.into(), id.clone(), gate.clone())?;
        gates.insert(id.clone(), gate);
        scopes.insert(id, config.scopes[0].clone());
        all_scopes.extend(config.scopes);
    }
    let agent = Arc::new(agent);
    let runtimes: Runtimes = ENDPOINTS
        .iter()
        .map(|id| Ok((id.to_string(), agent.endpoint(id)?.runtime.clone())))
        .collect::<Result<_>>()?;
    let result = async {
        for runtime in runtimes.values() { runtime.wait_ready(WAIT).await?; }
        let mut bindings = Vec::new();
        for (index, scope) in all_scopes.iter().enumerate() {
            let binding = Arc::new(RunBinding::open_main(runtimes[&scope.endpoint_id].clone(), scope.clone()).await?);
            binding.edit_text(&ComposerDestination::run(scope.clone()), format!("恢复草稿 {} 会话 {}", scope.endpoint_id, index))?;
            bindings.push(binding);
        }
        let host = Arc::new(SidebarHostBinding::new(agent.clone(), ENDPOINTS)?);
        wait_source(&host, |source| fresh_source(source, &runtimes)).await?;
        let mut view = ComponentDemo::new(bindings.clone())?;
        view.set_sidebar_host_reader(host.clone())?;
        sync(&mut view, &host).await?;
        let mut page_actions = 0;
        while let Some(id) = sidebar(&view)?.page_button_id() {
            let commands = view.accessibility_action(id, None, AccessibilityAction::Press)?;
            handle_reads(&mut view, commands).await?;
            sync(&mut view, &host).await?;
            page_actions += 1;
            require(page_actions <= 3, "Show more did not consume the original page request")?;
        }
        let baseline_drafts = drafts(&bindings).await?;
        let mut cases = Vec::new();
        for endpoint in selected {
            eprintln!("SIDEBAR_TRANSPORT_RECOVERY_PHASE {endpoint} before");
            view.select(ENDPOINTS.iter().position(|id| id == endpoint).ok_or_else(|| unsupported("unknown target"))? * 2)?;
            sync(&mut view, &host).await?;
            let gate = &gates[endpoint];
            let runtime = &runtimes[endpoint];
            let address = scopes[endpoint].address();
            let original_endpoint = gate.current().ok_or(Error::Disconnected)?;
            let old_reads: BTreeMap<_, _> = ENDPOINTS.iter().map(|id| Ok((id.to_string(), host.read_selected(&scopes[*id].address(), 1)?))).collect::<Result<_>>()?;
            let row = host.snapshot().as_publication().rows.into_iter().find(|row| row.address == address).ok_or_else(|| unsupported("original target row missing"))?;
            let definition = row.actions.items.iter().find(|item| item.enabled && item.interaction == "copy").cloned().ok_or_else(|| unsupported("host has no enabled canonical Copy"))?;
            let expected_text = definition.copy_text.clone().ok_or_else(|| unsupported("missing canonical Copy text"))?;
            let before = stage(&mut view, &host, &runtimes, &scopes, &bindings, gate).await?;
            println!("{}", json!({"endpointId":endpoint,"stage":"before","receipt":before}));
            gate.interrupt().await?;
            wait_failed(runtime).await?;
            let mut healthy_refresh = BTreeMap::new();
            for id in ENDPOINTS.iter().filter(|id| **id != endpoint) {
                let epoch_before = runtimes[*id].inspect(|s| s.connection_epoch);
                let revision_before = host.snapshot().as_multi_publication().ok_or_else(|| unsupported("multi source missing"))?.endpoints[*id].revision;
                runtimes[*id].refresh_inventory_at_epoch(epoch_before).await?;
                wait_source(&host, |source| source.as_multi_publication().is_some_and(|p| p.endpoints[*id].revision > revision_before)).await?;
                let revision_after = host.snapshot().as_multi_publication().ok_or_else(|| unsupported("multi source missing"))?.endpoints[*id].revision;
                healthy_refresh.insert(id.to_string(), json!({"epochBefore":epoch_before,"epochAfter":runtimes[*id].inspect(|s|s.connection_epoch),"revisionBefore":revision_before,"revisionAfter":revision_after}));
            }
            wait_source(&host, |source| source.as_multi_publication().is_some_and(|p| {
                p.rows.iter().filter(|row| row.address.endpoint_id == *endpoint).count() == 3
                    && p.rows.iter().all(|row| row.activity.is_stale == (row.address.endpoint_id == *endpoint))
                    && p.name_corpora.get(endpoint).is_some_and(|corpus| !corpus.inventory_ready && corpus.connection_epoch == runtime.inspect(|s|s.connection_epoch))
            })).await?;
            let failed = stage(&mut view, &host, &runtimes, &scopes, &bindings, gate).await?;
            println!("{}", json!({"endpointId":endpoint,"stage":"failed","receipt":failed}));
            require(host.perform_action(&address, &definition, None, "").await.is_err(), "stale Copy definition admitted while transport failed")?;
            gate.reopen();
            require(view.tree.get(RECONNECT).is_some(), "original Reconnect control not mounted")?;
            let commands = view.accessibility_action(RECONNECT, None, AccessibilityAction::Press)?;
            let mut command_count = 0;
            for command in commands {
                match command {
                    DemoCommand::Reconnect => {
                        command_count += 1;
                        require(view.current_binding().is_some_and(|binding| binding.shares_runtime(runtime)), "Reconnect re-bound to another runtime")?;
                        runtime.reconnect().await?;
                        runtime.wait_ready(WAIT).await?;
                    }
                    command => handle_reads(&mut view, vec![command]).await?,
                }
            }
            require(command_count == 1, "original Reconnect command count differs")?;
            let started = Instant::now();
            while !runtime.inspect(|state| all_scopes.iter().filter(|s| s.endpoint_id == *endpoint).all(|scope| state.sessions.get(&scope.session_id).is_some_and(|s| s.journal_ready))) {
                require(started.elapsed() < WAIT, "reconnect did not restore original journals")?;
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            wait_source(&host, |source| fresh_source(source, &runtimes)).await?;
            require(!Arc::ptr_eq(&original_endpoint, &gate.current().ok_or(Error::Disconnected)?), "reconnect did not replace the real endpoint")?;
            let recovered = stage(&mut view, &host, &runtimes, &scopes, &bindings, gate).await?;
            println!("{}", json!({"endpointId":endpoint,"stage":"recovered","receipt":recovered}));
            let mut read_results = BTreeMap::new();
            for id in ENDPOINTS {
                let mut display = host.snapshot().as_publication().input(Some(scopes[id].address()));
                display.rows.retain(|row| row.address != scopes[id].address());
                let accepted = old_reads[id].project_into(&host, &mut display, 1);
                require(accepted == (id != endpoint), "selected-read source fencing differs")?;
                read_results.insert(id.to_string(), accepted);
            }
            let SidebarActionOutcome::Copy { address: actual_address, text } = host.perform_action(&address, &definition, None, "").await? else { return Err(unsupported("Copy callback returned mutation")); };
            require(actual_address == address && text == expected_text, "recovered Copy changed canonical target/value")?;
            require(drafts(&bindings).await? == baseline_drafts, "recovery changed another draft")?;
            cases.push(json!({"endpointId":endpoint,"address":address,"before":before,"failed":failed,"recovered":recovered,
                "healthyRefresh":healthy_refresh,"reconnect":{"commandCount":command_count,"endpointId":endpoint},
                "selectedReads":{"selectionGeneration":1,"selectionGenerationOwner":"probe-selected-read-scope","faultAcceptedAfterRecovery":read_results[endpoint],"healthyAcceptedAfterRecovery":read_results.iter().filter(|(id,_)|*id!=endpoint).collect::<BTreeMap<_,_>>()},
                "capturedDefinitionCopy":{"definitionId":definition.id,"expectedText":expected_text,"actualText":text,"address":actual_address,"rejectedWhileFailed":true,"executionsAfterRecovery":1}}));
            eprintln!("SIDEBAR_TRANSPORT_RECOVERY_PHASE {endpoint} recovered");
        }
        Ok(json!({"schemaVersion":1,"status":"completed","evidenceScope":"real-client-transport-close-runtime-reconnect","desktopInteraction":false,"endpoints":ENDPOINTS,"bootstrap":{"pageActions":page_actions,"mountedCount":9},"cases":cases}))
    }.await;
    for runtime in runtimes.values() {
        runtime.stop().await?;
    }
    result
}

#[tokio::main]
async fn main() -> Result<()> {
    let (config, receipt, selected) = args()?;
    let config: Value = serde_json::from_slice(
        &fs::read(config).map_err(|_| unsupported("endpoint configuration unreadable"))?,
    )?;
    let result = execute(config, &selected).await?;
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(receipt)
        .map_err(|_| unsupported("new receipt cannot be created"))?;
    serde_json::to_writer_pretty(file, &result)?;
    println!("SIDEBAR_TRANSPORT_RECOVERY_COMPLETED desktop=false");
    Ok(())
}
