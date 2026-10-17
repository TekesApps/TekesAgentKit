use serde_json::json;
use std::{env, sync::Arc, time::Duration};
use tekes_agent_contract::PromptMode;
use tekes_agent_endpoint_kernel::KernelConnector;
use tekes_agent_presentation::{ComposerDestination, ComposerDraft, RunController, RunScope};
use tekes_agent_session::SessionRuntime;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let connector = KernelConnector::new(
        "native",
        &env::var("AGENTKIT_KERNEL_URL")?,
        env::var("AGENTKIT_KERNEL_BEARER")?,
    )?;
    let runtime = Arc::new(SessionRuntime::spawn("native", Arc::new(connector)));
    runtime.wait_ready(Duration::from_secs(30)).await?;
    let session = runtime
        .create_session(
            &env::var("AGENTKIT_WORKSPACE")?,
            &uuid::Uuid::new_v4().to_string(),
        )
        .await?;
    runtime
        .call(
            "session.selectModel",
            json!({
                "sessionId": session,
                "provider": env::var("AGENTKIT_PROVIDER")?,
                "model": env::var("AGENTKIT_MODEL")?
            }),
            &uuid::Uuid::new_v4().to_string(),
        )
        .await?;

    let controller = RunController::new(runtime.clone())?;
    let scope = RunScope::main("native", session);
    controller.open(scope.clone()).await?;
    let destination = ComposerDestination::run(scope);
    controller.edit(
        &destination,
        ComposerDraft {
            text: "Describe this workspace briefly.".into(),
            ..Default::default()
        },
    )?;
    let ticket = controller.prepare(&destination, PromptMode::Queue)?;
    // An optimistic input is already present here. Admission does not mean the Agent finished.
    controller.dispatch(ticket, None).await?;
    let mut changes = controller.subscribe();
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let snapshot = changes.borrow_and_update().clone();
            if snapshot.transcript.items.iter().any(|item| item.is_final) {
                break;
            }
            changes
                .changed()
                .await
                .map_err(|_| tekes_agent_contract::Error::Disconnected)?;
        }
        Ok::<_, tekes_agent_contract::Error>(())
    })
    .await??;
    runtime.stop().await?;
    Ok(())
}
