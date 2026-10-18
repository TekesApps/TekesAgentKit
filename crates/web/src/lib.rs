//! Shared AgentKit service. Hosts own its authentication, listener and process lifetime.
//! A disconnected view drops only its history lease; it never stops a runtime or Harness.
use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};
use tekes_agent_contract::*;
use tekes_agent_presentation::{RunPresentation, RunScope, TranscriptSnapshot};
use tekes_agent_session::{ConnectionState, MemoryProjectionStore, SessionRuntime};
use tokio::sync::mpsc;

fn active_inventory(state: &MemoryProjectionStore) -> Vec<Value> {
    state
        .sessions
        .iter()
        .filter(|(id, _)| !state.archived_session_ids.contains(*id))
        .filter_map(|(id, session)| {
            session
                .summary
                .as_ref()
                .map(|summary| json!({"sessionId":id,"summary":summary}))
        })
        .collect()
}

/// Typed public read projection; the durable original text is retained for other consumers.
pub fn present_annotations(transcript: &mut Value) {
    if let Some(items) = transcript.get_mut("items").and_then(Value::as_array_mut) {
        for item in items {
            if item["kind"] == "user" {
                if let Some(presentation) = item["text"]
                    .as_str()
                    .and_then(tekes_agent_presentation::annotation_input_presentation)
                {
                    item["inputPresentation"] =
                        serde_json::to_value(presentation).expect("annotation presentation");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tekes_agent_session::SessionProjection;

    struct Offline;
    #[async_trait]
    impl EndpointConnector for Offline {
        async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
            Err(Error::Disconnected)
        }
    }
    #[tokio::test]
    async fn public_annotation_compilation_is_local_and_typed_not_a_harness_rpc() {
        let mut agent = SharedAgent::default();
        agent
            .register(
                "offline".into(),
                "tekes".into(),
                "Offline".into(),
                Arc::new(Offline),
            )
            .unwrap();
        let value = agent.request(&json!({"op":"call","endpoint":"offline","method":"composer.compileAnnotations","rpcId":"stable","payload":{"text":"body","annotations":[{"id":"a","source":"Reply","excerpt":"quoted","comment":"explain"}]}})).await.unwrap();
        let compiled: AnnotationCompilation = serde_json::from_value(value).unwrap();
        let input =
            tekes_agent_presentation::annotation_input_presentation(&compiled.text).unwrap();
        assert_eq!(input.text, "body");
        assert_eq!(input.annotations[0].excerpt, "quoted");
        let mut transcript = json!({"items":[{"id":"user","kind":"user","text":compiled.text},{"id":"assistant","kind":"assistant","text":compiled.text}]});
        present_annotations(&mut transcript);
        assert_eq!(transcript["items"][0]["inputPresentation"]["text"], "body");
        assert!(transcript["items"][1].get("inputPresentation").is_none());
        let mut absent = Value::Null;
        present_annotations(&mut absent);
        assert!(absent.is_null());
    }

    #[test]
    fn archived_workspace_authority_excludes_retained_summary_without_erasing_history() {
        let mut store = MemoryProjectionStore::new("dsh");
        let archived = SessionProjection {
            summary: Some(json!({"sessionId":"archived","title":"Retained"})),
            ..Default::default()
        };
        let active = SessionProjection {
            summary: Some(json!({"sessionId":"active"})),
            ..Default::default()
        };
        store.sessions.insert("archived".into(), archived);
        store.sessions.insert("active".into(), active);
        store.archived_session_ids.push("archived".into());
        assert_eq!(
            active_inventory(&store),
            vec![json!({"sessionId":"active","summary":{"sessionId":"active"}})]
        );
        assert!(store.sessions["archived"].summary.is_some());
        store.archived_session_ids.clear();
        assert_eq!(active_inventory(&store).len(), 2);
    }
}

struct TrackedConnector {
    inner: Arc<RwLock<Arc<dyn EndpointConnector>>>,
    endpoint: Arc<RwLock<Option<Arc<dyn SessionEndpoint>>>>,
}
#[async_trait]
impl EndpointConnector for TrackedConnector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        let connector = self.inner.read().expect("connector lock").clone();
        let endpoint = connector.connect().await?;
        *self.endpoint.write().expect("endpoint lock") = Some(endpoint.clone());
        Ok(endpoint)
    }
}
pub struct SharedEndpoint {
    pub kind: String,
    pub name: String,
    pub runtime: Arc<SessionRuntime>,
    connector: Arc<RwLock<Arc<dyn EndpointConnector>>>,
    endpoint: Arc<RwLock<Option<Arc<dyn SessionEndpoint>>>>,
}
impl SharedEndpoint {
    fn connected(&self) -> Result<Arc<dyn SessionEndpoint>> {
        self.endpoint
            .read()
            .expect("endpoint lock")
            .clone()
            .ok_or(Error::Disconnected)
    }
    pub fn description(&self) -> Value {
        let state = self.runtime.snapshot();
        let mut description = json!({"id":state.endpoint_id,"kind":self.kind,"name":self.name,
            "connection":match &state.connection {ConnectionState::Connecting=>json!("connecting"),ConnectionState::Ready=>json!("ready"),ConnectionState::Failed(e)=>json!({"failed":e}),ConnectionState::Stopped=>json!("stopped")},"host":state.host,
            "capabilities":self.connected().ok().map(|e| e.capabilities())});
        if let Some(methods) = description["capabilities"]["methods"].as_array_mut() {
            methods.push(json!("composer.compileAnnotations"));
        }
        description
    }
}
#[derive(Default)]
pub struct SharedAgent {
    endpoints: BTreeMap<String, Arc<SharedEndpoint>>,
}
impl SharedAgent {
    pub fn register(
        &mut self,
        id: String,
        kind: String,
        name: String,
        connector: Arc<dyn EndpointConnector>,
    ) -> Result<()> {
        require(
            !self.endpoints.contains_key(&id),
            "duplicate endpoint identity",
        )?;
        let endpoint = Arc::new(RwLock::new(None));
        let connector = Arc::new(RwLock::new(connector));
        let runtime = Arc::new(SessionRuntime::spawn(
            &id,
            Arc::new(TrackedConnector {
                inner: connector.clone(),
                endpoint: endpoint.clone(),
            }),
        ));
        self.endpoints.insert(
            id,
            Arc::new(SharedEndpoint {
                kind,
                name,
                runtime,
                connector,
                endpoint,
            }),
        );
        Ok(())
    }
    pub fn endpoint(&self, id: &str) -> Result<Arc<SharedEndpoint>> {
        self.endpoints
            .get(id)
            .cloned()
            .ok_or_else(|| unsupported("unknown endpoint"))
    }
    /// The embedding host may refresh credentials without changing endpoint or session identity.
    pub async fn replace_connector(
        &self,
        id: &str,
        connector: Arc<dyn EndpointConnector>,
    ) -> Result<()> {
        let endpoint = self.endpoint(id)?;
        *endpoint.connector.write().expect("connector lock") = connector;
        *endpoint.endpoint.write().expect("endpoint lock") = None;
        endpoint.runtime.reconnect().await
    }
    pub fn descriptions(&self) -> Value {
        json!(
            self.endpoints
                .values()
                .map(|e| e.description())
                .collect::<Vec<_>>()
        )
    }
    pub async fn request(&self, request: &Value) -> Result<Value> {
        if request["op"] == "endpoints" {
            return Ok(self.descriptions());
        }
        let shared = self.endpoint(string(request, "endpoint")?)?;
        let runtime = &shared.runtime;
        match string(request, "op")? {
            "describe" => Ok(shared.description()),
            "snapshot" => Ok(serde_json::to_value(runtime.snapshot().as_ref())?),
            "call" => {
                if request["method"] == "composer.compileAnnotations" {
                    let input: AnnotationInput =
                        serde_json::from_value(request["payload"].clone())?;
                    let text = tekes_agent_presentation::compile_annotation_input(
                        &input.text,
                        &input.annotations,
                    )?;
                    return Ok(serde_json::to_value(AnnotationCompilation { text })?);
                }
                runtime
                    .call(
                        string(request, "method")?,
                        request["payload"].clone(),
                        string(request, "rpcId")?,
                    )
                    .await
            }
            "history" => Ok(serde_json::to_value(
                shared
                    .connected()?
                    .history_page(serde_json::from_value(request["payload"].clone())?)
                    .await?,
            )?),
            "respond" => {
                runtime
                    .respond(
                        string(request, "actionableId")?,
                        integer(request, "expectedRevision")?,
                        request["outcome"].clone(),
                    )
                    .await?;
                Ok(json!({}))
            }
            "earlier" => {
                Ok(json!({"loaded":runtime.load_earlier(string(request,"sessionId")?).await?}))
            }
            _ => Err(unsupported("unknown service operation")),
        }
    }
    /// Each observer gets its own lease and generation. No browser owns execution lifetime.
    pub async fn follow(&self, request: &Value, output: mpsc::Sender<Value>) -> Result<()> {
        let shared = self.endpoint(string(request, "endpoint")?)?;
        if request["target"] == "view" || request["target"] == "state" {
            let runtime = &shared.runtime;
            let session = request["sessionId"].as_str();
            let _lease = if let Some(id) = session {
                Some(runtime.acquire_history(id).await?)
            } else {
                None
            };
            let mut observer = runtime.subscribe();
            loop {
                let state = runtime.snapshot();
                let value = if let Some(id) = session {
                    let scope = RunScope::main(&state.endpoint_id, id);
                    let transcript = state
                        .sessions
                        .get(id)
                        .map(|s| TranscriptSnapshot::read(&scope, s))
                        .transpose()?;
                    let mut transcript = serde_json::to_value(transcript)?;
                    present_annotations(&mut transcript);
                    json!({"endpoint":state.endpoint_id,"sessionId":id,"run":RunPresentation::read(&state,&scope,true),"transcript":transcript})
                } else {
                    json!({"description":shared.description(),"sessions":active_inventory(&state)})
                };
                if output.send(value).await.is_err() {
                    break;
                }
                tokio::select! { result = observer.changed() => {if result.is_err(){break}}, _ = output.closed() => break }
            }
        } else {
            let mut stream = shared
                .connected()?
                .follow(serde_json::from_value(request["target"].clone())?)
                .await?;
            loop {
                tokio::select! {
                    frame = stream.next() => match frame {Some(frame)=> {if output.send(frame?).await.is_err(){break}}, None=>break},
                    _ = output.closed() => break,
                }
            }
        }
        Ok(())
    }
}
