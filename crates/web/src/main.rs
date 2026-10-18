//! Private JSON-lines transport for an embedding host. No network/authentication policy here.
use futures_util::FutureExt;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::PathBuf,
    sync::Arc,
};
use tekes_agent_contract::*;
use tekes_agent_endpoint_dsh::{DshConfiguration, DshConnector};
use tekes_agent_endpoint_kernel::KernelConnector;
use tekes_agent_endpoint_pi::{PiConfiguration, PiConnector};
use tekes_agent_web::SharedAgent;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    let (input, mut receiver) = mpsc::channel::<String>(64);
    std::thread::spawn(move || {
        // macOS pipe flags may become nonblocking when child RPC pipes are registered.
        // A temporary EAGAIN is not EOF; retain partial JSON lines across reads.
        let stdin = std::io::stdin();
        let mut reader = stdin.lock();
        let mut pending = Vec::new();
        let mut buffer = [0_u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    pending.extend_from_slice(&buffer[..count]);
                    while let Some(cut) = pending.iter().position(|b| *b == b'\n') {
                        if cut > 16 * 1024 * 1024 {
                            return;
                        }
                        let line = match String::from_utf8(pending.drain(..=cut).collect()) {
                            Ok(line) => line,
                            Err(_) => return,
                        };
                        if input.blocking_send(line).is_err() {
                            return;
                        }
                    }
                    if pending.len() > 16 * 1024 * 1024 {
                        return;
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(_) => {
                    eprintln!("AgentKit host input closed");
                    break;
                }
            }
        }
    });
    let config: Value = serde_json::from_str(&receiver.recv().await.ok_or(Error::Disconnected)?)?;
    let mut agent = SharedAgent::default();
    for endpoint in array(&config, "endpoints")? {
        let id = string(endpoint, "id")?;
        let kind = string(endpoint, "kind")?;
        let connector: Arc<dyn EndpointConnector> = match kind {
            "tekes" => Arc::new(KernelConnector::new(
                id,
                string(endpoint, "url")?,
                string(endpoint, "bearer")?.into(),
            )?),
            "pi" => {
                let mut pi = PiConfiguration::new(
                    PathBuf::from(string(endpoint, "executable")?),
                    PathBuf::from(string(endpoint, "dataRoot")?),
                );
                pi.identity = id.into();
                pi.arguments = serde_json::from_value(
                    endpoint.get("arguments").cloned().unwrap_or(json!([])),
                )?;
                pi.environment = serde_json::from_value(
                    endpoint.get("environment").cloned().unwrap_or(json!({})),
                )?;
                pi.extensions = serde_json::from_value(
                    endpoint.get("extensions").cloned().unwrap_or(json!([])),
                )?;
                pi.session_roots = serde_json::from_value(
                    endpoint.get("sessionRoots").cloned().unwrap_or(json!([])),
                )?;
                Arc::new(PiConnector::new(pi).await?)
            }
            "dsh" => {
                let mut dsh = DshConfiguration::new(id, string(endpoint, "url")?)?;
                if let Some(cookie) = endpoint["cookie"].as_str() {
                    dsh.authorization = Some(("Cookie".into(), cookie.into()));
                } else if let Some(token) = endpoint["bearer"].as_str() {
                    dsh.authorization = Some(("Authorization".into(), format!("Bearer {token}")));
                }
                Arc::new(DshConnector::new(dsh))
            }
            _ => return Err(unsupported("endpoint kind")),
        };
        agent.register(
            id.into(),
            kind.into(),
            endpoint["name"].as_str().unwrap_or(kind).into(),
            connector,
        )?;
    }
    let agent = Arc::new(agent);
    let (output, mut messages) = mpsc::channel::<Value>(128);
    let writer = tokio::task::spawn_blocking(move || {
        let mut stdout = std::io::stdout().lock();
        while let Some(value) = messages.blocking_recv() {
            let mut bytes = match serde_json::to_vec(&value) {
                Ok(bytes) => bytes,
                Err(_) => return,
            };
            bytes.push(b'\n');
            let mut written = 0;
            while written < bytes.len() {
                match stdout.write(&bytes[written..]) {
                    Ok(0) => return,
                    Ok(count) => written += count,
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                        ) =>
                    {
                        std::thread::sleep(std::time::Duration::from_millis(5))
                    }
                    Err(_) => return,
                }
            }
            loop {
                match stdout.flush() {
                    Ok(()) => break,
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                        ) =>
                    {
                        std::thread::sleep(std::time::Duration::from_millis(5))
                    }
                    Err(_) => return,
                }
            }
        }
    });
    let mut tasks = BTreeMap::<String, tokio::task::AbortHandle>::new();
    while let Some(line) = receiver.recv().await {
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let id = string(&request, "id")?.to_owned();
        if request["op"] == "cancelObservation" {
            if let Some(task) = tasks.remove(&id) {
                task.abort();
            }
            continue;
        }
        // Finished requests are removed without ever retrying an uncertain mutation.
        tasks.retain(|_, task| !task.is_finished());
        require(!tasks.contains_key(&id), "duplicate request id")?;
        let agent = agent.clone();
        let output = output.clone();
        let task_id = id.clone();
        let task = tokio::spawn(async move {
            let result=std::panic::AssertUnwindSafe(async {
                if request["op"]=="configureDSH" {
                    let identity=string(&request,"endpoint")?;
                    require(agent.endpoint(identity)?.kind=="dsh","DSH endpoint required")?;
                    let mut config=DshConfiguration::new(identity,string(&request,"url")?)?;
                    config.authorization=Some(("Cookie".into(),string(&request,"cookie")?.into()));
                    agent.replace_connector(identity,Arc::new(DshConnector::new(config))).await?;
                    let _=output.send(json!({"id":task_id,"value":{}})).await;
                    return Ok(());
                }
                if request["op"]=="follow" {
                    let (tx,mut rx)=mpsc::channel(16);
                    let follow=agent.follow(&request,tx);
                    tokio::pin!(follow);
                    loop {tokio::select! {
                        value=rx.recv()=>if let Some(value)=value && output.send(json!({"id":task_id,"value":value})).await.is_err(){return Ok(())},
                        result=&mut follow=>return result,
                    }}
                } else {
                    let value=agent.request(&request).await?;
                    let _=output.send(json!({"id":task_id,"value":value})).await;
                    Ok(())
                }
            }).catch_unwind().await;
            match result {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    let _ = output
                        .send(json!({"id":task_id,"error":error.to_string()}))
                        .await;
                }
                Err(_) => {
                    let _ = output
                        .send(json!({"id":task_id,"error":"service operation failed"}))
                        .await;
                }
            }
            let _ = output.send(json!({"id":task_id,"done":true})).await;
        });
        tasks.insert(id, task.abort_handle());
    }
    eprintln!("AgentKit host shutting down: input channel ended");
    for task in tasks.values() {
        task.abort();
    }
    drop(output);
    let _ = writer.await;
    Ok(())
}
