//! Offscreen public-consumer driver for the Native per-call approval denial.
//! It uses AgentKit's real SessionRuntime, RunController, and SummaryView;
//! it deliberately does not start a window, GPU renderer, or ComponentDemo.
use std::{io::Read, sync::Arc, time::Duration};

use serde_json::{Value, json};
use tekes_agent_components_ui::{SummaryInput, SummaryView};
use tekes_agent_contract::PromptMode;
use tekes_agent_endpoint_kernel::KernelConnector;
use tekes_agent_presentation::{ComposerDestination, RunController, RunScope};
use tekes_agent_session::SessionRuntime;
use tekes_ui_controls::composition::{ComponentId, ContentHost, TextFieldContent};

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("native_parent_approval: {message}");
    std::process::exit(2)
}

fn task_tool(snapshot: &tekes_agent_presentation::RunSnapshot, phase: &str) -> Value {
    snapshot
        .transcript
        .tools
        .iter()
        .find(|tool| tool.name == "task" && tool.phase == phase)
        .and_then(|tool| serde_json::to_value(tool).ok())
        .unwrap_or(Value::Null)
}

async fn wait_task(controller: &RunController, phase: &str, timeout: Duration) -> Value {
    let mut changes = controller.subscribe();
    let wait: Result<Value, &'static str> = tokio::time::timeout(timeout, async {
        loop {
            let tool = task_tool(&controller.snapshot(), phase);
            if !tool.is_null() {
                return Ok(tool);
            }
            changes
                .changed()
                .await
                .map_err(|_| "RunController closed")?;
        }
    })
    .await
    .unwrap_or_else(|_| fail(format!("timed out waiting for task phase {phase}")));
    wait.unwrap_or_else(|error| fail(error))
}

async fn wait_approval(runtime: &SessionRuntime, session: &str) -> (String, Value) {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Some((id, value)) = runtime
                .snapshot()
                .actionables
                .iter()
                .find(|(_, value)| {
                    value["sessionId"] == session && value["payload"]["approvalId"].is_string()
                })
                .map(|(id, value)| (id.clone(), value.clone()))
            {
                return (id, value);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| fail("timed out waiting for per-call approval actionable"))
}

struct MountedSummary {
    root: ComponentId,
    host: ContentHost,
}

fn capture(
    controller: &RunController,
    _runtime: &SessionRuntime,
    scope: &RunScope,
    summary: &mut Option<SummaryView>,
    mounted_summary: &mut Option<MountedSummary>,
    phase: &str,
) -> Value {
    let source = controller.refresh_with_source_witness();
    let snapshot = source.snapshot();
    if !source.projection_succeeded() {
        fail("RunController published a degraded projection");
    }
    if snapshot.run.as_ref().is_none_or(|run| run.scope != *scope) {
        fail("selected Run scope changed");
    }
    let task = task_tool(snapshot, phase);
    if task.is_null() {
        fail(format!("same Run has no task at phase {phase}"));
    }
    let summary_facts = snapshot
        .summary
        .clone()
        .unwrap_or_else(|| fail("Summary facts unavailable"));
    let task_id = task["id"]
        .as_str()
        .unwrap_or_else(|| fail("task id missing"))
        .to_owned();
    let input = SummaryInput {
        revision: snapshot.revision,
        scope: scope.clone(),
        facts: Some(summary_facts),
        host_sections: vec![],
    };
    if let Some(view) = summary.as_mut() {
        view.update(&input).unwrap_or_else(|error| fail(error));
        let root = view.id();
        let host = summary
            .take()
            .unwrap()
            .into_host()
            .unwrap_or_else(|error| fail(error));
        *mounted_summary = Some(MountedSummary { root, host });
    } else {
        let mounted = mounted_summary
            .as_mut()
            .unwrap_or_else(|| fail("SummaryView mount missing"));
        mounted
            .host
            .get_mut(mounted.root)
            .and_then(|node| node.component_mut::<SummaryView>())
            .unwrap_or_else(|| fail("same SummaryView instance disappeared"))
            .update(&input)
            .unwrap_or_else(|error| fail(error));
    }
    let mounted = mounted_summary
        .as_ref()
        .unwrap_or_else(|| fail("SummaryView mount missing"));
    let root = mounted.root;
    let mounted_view = mounted
        .host
        .get(root)
        .and_then(|node| node.component::<SummaryView>())
        .unwrap_or_else(|| fail("same SummaryView instance is not mounted"));
    let displayed = mounted_view
        .displayed_input()
        .unwrap_or_else(|| fail("mounted Summary has no displayed input"));
    if displayed != &input {
        fail("mounted Summary input differs from this source revision/scope/facts");
    }
    let view_instance = mounted_view as *const SummaryView as usize;
    let row_id = mounted_view
        .passive_text_id("tasks", &task_id)
        .unwrap_or_else(|| fail("original parent Task title is not mounted"));
    let title = mounted
        .host
        .get(row_id)
        .and_then(|node| node.component::<TextFieldContent>())
        .unwrap_or_else(|| fail("mounted parent Task title field is missing"));
    let title_instance = title as *const TextFieldContent as usize;
    let authority = serde_json::to_value(source.authority()).unwrap_or_else(|error| fail(error));
    let snapshot_json = serde_json::to_value(snapshot.as_ref()).unwrap_or_else(|error| fail(error));
    let tool: Value = task;
    let source_position = tool["sourcePosition"].clone();
    let position = if source_position["kind"] == "journal" {
        let sequence = source_position["sequence"]
            .as_u64()
            .unwrap_or_else(|| fail("journal sequence missing"));
        authority["sessions"][&scope.session_id]["entries"][sequence.to_string()]["event"].clone()
    } else {
        let ordinal = source_position["entryOrdinal"]
            .as_u64()
            .unwrap_or_else(|| fail("native ordinal missing")) as usize;
        let index = source_position["contentIndex"]
            .as_u64()
            .unwrap_or_else(|| fail("native content index missing")) as usize;
        authority["sessions"][&scope.session_id]["native_conversation"]["entries"][ordinal]["message"]["content"][index].clone()
    };
    json!({
        "available": true, "publication": snapshot.revision, "connectionEpoch": snapshot.connection_epoch,
        "scope": scope, "taskId": task_id, "title": title.field.label.as_str(),
        "rawStatus": tool["phase"], "canonicalStatus": if phase == "doing" {"in_progress"} else {"failed"},
        "adaptedStatus": if phase == "doing" {"in_progress"} else {"error"},
        "rowId": row_id.0, "viewInstance": view_instance,
        "titleInstance": title_instance, "mounted": true,
        "tool": tool, "sourceCallId": source_position_call_id(&position), "sourcePosition": source_position,
        "sourceEntry": if source_position["kind"] == "journal" { json!({"event": position}) } else {position},
        "draft": snapshot_json["draft"].clone(),
        "witness": {"snapshot": snapshot_json, "authority": authority,
            "projectionSucceeded": source.projection_succeeded(),
            "evidenceScope": "original-publication-and-same-memory-input",
            "serverTransactionalFreshnessProven": false}
    })
}

fn source_position_call_id(source: &Value) -> Value {
    if source["type"] == "tool/call" {
        source["data"]["callId"].clone()
    } else {
        source["id"].clone()
    }
}

#[tokio::main]
async fn main() {
    let mut input = String::new();
    std::io::stdin()
        .take(64 * 1024)
        .read_to_string(&mut input)
        .unwrap_or_else(|error| fail(error));
    let config: Value = serde_json::from_str(&input).unwrap_or_else(|error| fail(error));
    let connector = KernelConnector::new(
        "native",
        config["url"].as_str().unwrap_or_default(),
        config["bearer"].as_str().unwrap_or_default().to_owned(),
    )
    .unwrap_or_else(|error| fail(error));
    let runtime = Arc::new(SessionRuntime::spawn("native", Arc::new(connector.clone())));
    runtime
        .wait_ready(Duration::from_secs(20))
        .await
        .unwrap_or_else(|error| fail(error));
    let cwd = config["cwd"]
        .as_str()
        .unwrap_or_else(|| fail("cwd required"));
    let created = runtime
        .call(
            "session.create",
            json!({"execution":{
                "cwd":cwd,"read_roots":[],"writable_roots":[],"toolchain_roots":[],
                "network":false,"allowed_tools":["task"],"mcp":[]
            }}),
            "agentkit-parent-approval-create",
        )
        .await
        .unwrap_or_else(|error| fail(error));
    let session = created["sessionId"]
        .as_str()
        .unwrap_or_else(|| fail("created Native session id missing"))
        .to_owned();
    runtime
        .call(
            "session.selectModel",
            json!({"sessionId":session,"provider":config["provider"],"model":config["model"]}),
            "agentkit-parent-approval-model",
        )
        .await
        .unwrap_or_else(|error| fail(error));
    runtime
        .call(
            "approvals.select",
            json!({"sessionId":session,"mode":"workspace-write"}),
            "agentkit-parent-approval-policy",
        )
        .await
        .unwrap_or_else(|error| fail(error));
    let scope = RunScope::main("native", session.clone());
    let destination = ComposerDestination::run(scope.clone());
    let controller = RunController::new(runtime.clone()).unwrap_or_else(|error| fail(error));
    controller
        .open(scope.clone())
        .await
        .unwrap_or_else(|error| fail(error));
    controller.edit_text(&destination, "AGENTKIT_NATIVE_PARENT_APPROVAL: invoke task exactly once with the valid inline contract.".into())
        .unwrap_or_else(|error| fail(error));
    let ticket = controller
        .prepare(&destination, PromptMode::Queue)
        .unwrap_or_else(|error| fail(error));
    controller
        .dispatch(ticket, None)
        .await
        .unwrap_or_else(|error| fail(error));
    let doing = wait_task(&controller, "doing", Duration::from_secs(45)).await;
    controller
        .edit_text(
            &destination,
            "Keep this parent draft unchanged across approval rejection.".into(),
        )
        .unwrap_or_else(|error| fail(error));
    let (actionable_id, actionable) = wait_approval(&runtime, &session).await;
    let approval_id = actionable["payload"]["approvalId"]
        .as_str()
        .unwrap_or_else(|| fail("approval id missing"));
    let revision = actionable["revision"]
        .as_i64()
        .unwrap_or_else(|| fail("actionable revision missing"));
    let mut summary = Some(SummaryView::new(0x4150_5052, Default::default()));
    let mut mounted_summary = None;
    let mut running = capture(
        &controller,
        &runtime,
        &scope,
        &mut summary,
        &mut mounted_summary,
        "doing",
    );
    if doing["id"] != running["taskId"] {
        fail("doing tool and captured Summary row differ");
    }
    if actionable["payload"]["callId"] != running["sourceCallId"]
        || actionable["payload"]["toolName"] != "task"
        || actionable["sessionId"] != session
    {
        fail("approval actionable does not identify the original parent Task call");
    }
    let approval_capture = json!({"actionableId": actionable_id, "approvalId": approval_id,
        "revision": revision, "actionable": actionable});
    running["approval"] = approval_capture.clone();
    let outcome = json!({"sessionId":session,"approvalId": approval_id, "outcome":"rejected"});
    controller
        .respond(&scope, &actionable_id, revision, outcome.clone())
        .await
        .unwrap_or_else(|error| fail(error));
    let failed_tool = wait_task(&controller, "error", Duration::from_secs(45)).await;
    let failed = capture(
        &controller,
        &runtime,
        &scope,
        &mut summary,
        &mut mounted_summary,
        "error",
    );
    if failed_tool["id"] != failed["taskId"] {
        fail("failed tool and captured Summary row differ");
    }
    let mut failed = failed;
    failed["approval"] = approval_capture;
    if controller.snapshot().draft.text
        != "Keep this parent draft unchanged across approval rejection."
    {
        fail("parent draft was not preserved");
    }
    let catalog = runtime
        .call(
            "subagents.list",
            json!({"parentSessionId":session}),
            "agentkit-parent-approval-catalog",
        )
        .await
        .unwrap_or_else(|error| fail(error));
    runtime.stop().await.unwrap_or_else(|error| fail(error));
    println!(
        "{}",
        json!({"running":running,"failed":failed,"actionableId":actionable_id,
        "approvalId":approval_id,"expectedRevision":revision,"outcome":outcome,
        "response":{"accepted":true,"outcome":"rejected","actionableId":actionable_id,
            "approvalId":approval_id,"expectedRevision":revision}, "catalog":catalog})
    );
}
