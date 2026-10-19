//! SharedAgent inventory/menu adaptation. No independent pin store or action policy.
use serde_json::{Value, json};
use std::sync::Arc;
use tekes_agent_components_ui::{SidebarInput, SidebarRow};
use tekes_agent_contract::{
    Result, SessionAddress, ThreadActionDefinition, ThreadActionOutcome, ThreadActionRequest,
    require, unsupported,
};
use tekes_agent_web::{SharedAgent, SharedEndpoint};
use tokio::sync::{mpsc, watch};

#[derive(Clone, Debug, PartialEq)]
pub struct SidebarPublication {
    pub revision: u64,
    pub rows: Vec<SidebarRow>,
    pub loaded: bool,
    pub error: Option<String>,
}
impl SidebarPublication {
    /// Host navigation supplies selection; the inventory observer never selects a row.
    pub fn input(&self, selected: Option<SessionAddress>) -> SidebarInput {
        SidebarInput {
            revision: self.revision,
            rows: self.rows.clone(),
            selected,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum SidebarActionOutcome {
    /// The host executes the user's clipboard gesture using this fresh shared value.
    Copy {
        address: SessionAddress,
        text: String,
    },
    Mutation {
        address: SessionAddress,
        outcome: ThreadActionOutcome,
    },
}

/// Observes the same SharedAgent endpoint used by RunBinding. Drop cancels only
/// the inventory observer, never remote execution or the endpoint's runtime.
pub struct SidebarBinding {
    agent: Arc<SharedAgent>,
    endpoint: Arc<SharedEndpoint>,
    publications: watch::Receiver<Arc<SidebarPublication>>,
    observer: tokio::task::JoinHandle<()>,
}
impl Drop for SidebarBinding {
    fn drop(&mut self) {
        self.observer.abort();
    }
}
impl SidebarBinding {
    pub fn new(agent: Arc<SharedAgent>, endpoint_id: &str) -> Result<Self> {
        let endpoint = agent.endpoint(endpoint_id)?;
        let executor = tokio::runtime::Handle::try_current()
            .map_err(|_| unsupported("SidebarBinding requires an async runtime"))?;
        let (sender, publications) = watch::channel(Arc::new(SidebarPublication {
            revision: 1,
            rows: vec![],
            loaded: false,
            error: None,
        }));
        let owner = agent.clone();
        let source = endpoint.clone();
        let observer = executor.spawn(async move {
            let (output, mut frames) = mpsc::channel(8);
            let request = json!({"endpoint":source.runtime.inspect(|state| state.endpoint_id.clone()), "target":"state"});
            let follow = owner.follow(&request, output);
            tokio::pin!(follow);
            let result: Result<()> = loop {
                tokio::select! {
                    result = &mut follow => break result.and_then(|()| Err(unsupported("inventory observation ended"))),
                    frame = frames.recv() => {
                        let Some(frame) = frame else { break Err(unsupported("inventory channel ended")); };
                        match rows(&owner, &source, &frame) {
                            Ok(rows) => {
                                let revision = match sender.borrow().revision.checked_add(1) { Some(revision) => revision, None => break Err(unsupported("inventory revision exhausted")) };
                                sender.send_replace(Arc::new(SidebarPublication { revision, rows, loaded: true, error: None }));
                            }
                            Err(error) => break Err(error),
                        }
                    }
                }
            };
            if let Err(error) = result {
                // Keep the last readable inventory; a failed observation is not absence.
                let mut last = (**sender.borrow()).clone();
                last.revision = last.revision.saturating_add(1);
                last.error = Some(error.to_string());
                for row in &mut last.rows { row.activity.is_stale = true; }
                sender.send_replace(Arc::new(last));
            }
        });
        Ok(Self {
            agent,
            endpoint,
            publications,
            observer,
        })
    }
    pub fn snapshot(&self) -> Arc<SidebarPublication> {
        self.publications.borrow().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<Arc<SidebarPublication>> {
        self.publications.clone()
    }
    pub fn runtime(&self) -> &Arc<tekes_agent_session::SessionRuntime> {
        &self.endpoint.runtime
    }

    /// Reuse the published shared dispatcher and the caller's stable RPC identity.
    /// Outcomes describe navigation/selection changes; this observer never applies them.
    pub async fn perform(
        &self,
        address: &SessionAddress,
        definition: &ThreadActionDefinition,
        title: Option<String>,
        rpc_id: &str,
    ) -> Result<SidebarActionOutcome> {
        require(
            address.endpoint_id
                == self
                    .endpoint
                    .runtime
                    .inspect(|state| state.endpoint_id.clone()),
            "sidebar endpoint mismatch",
        )?;
        require(
            definition.enabled,
            definition
                .disabled_reason
                .as_deref()
                .unwrap_or("thread action unavailable"),
        )?;
        if definition.interaction == "copy" {
            let menu = self
                .agent
                .thread_actions(&self.endpoint, &address.session_id)?;
            let current = menu
                .items
                .iter()
                .find(|item| item.id == definition.id)
                .ok_or_else(|| unsupported("thread action no longer exists"))?;
            require(
                current.enabled && current.interaction == "copy",
                "copy action changed",
            )?;
            let text = current
                .copy_text
                .clone()
                .ok_or_else(|| unsupported("missing shared copy value"))?;
            return Ok(SidebarActionOutcome::Copy {
                address: address.clone(),
                text,
            });
        }
        require(
            !rpc_id.trim().is_empty(),
            "thread mutation requires RPC identity",
        )?;
        let request = ThreadActionRequest {
            session_id: address.session_id.clone(),
            action_id: definition.id.clone(),
            title,
        };
        let outcome = serde_json::from_value(self.agent.request(&json!({"endpoint":address.endpoint_id,"op":"call","method":"thread.performAction","payload":request,"rpcId":rpc_id})).await?)?;
        Ok(SidebarActionOutcome::Mutation {
            address: address.clone(),
            outcome,
        })
    }
}
fn rows(agent: &SharedAgent, endpoint: &SharedEndpoint, frame: &Value) -> Result<Vec<SidebarRow>> {
    let endpoint_id = endpoint.runtime.inspect(|state| state.endpoint_id.clone());
    require(
        frame["description"]["id"] == endpoint_id,
        "inventory endpoint mismatch",
    )?;
    let items = frame["sessions"]
        .as_array()
        .ok_or_else(|| unsupported("invalid inventory rows"))?;
    let mut addresses = std::collections::BTreeSet::new();
    items
        .iter()
        .map(|item| {
            let id = tekes_agent_contract::string(item, "sessionId")?;
            require(
                !id.is_empty() && addresses.insert(id),
                "duplicate or empty inventory identity",
            )?;
            Ok(SidebarRow {
                address: SessionAddress {
                    endpoint_id: endpoint_id.clone(),
                    session_id: id.into(),
                },
                title: item["summary"]["title"]
                    .as_str()
                    .or_else(|| item["summary"]["name"].as_str())
                    .unwrap_or(id)
                    .into(),
                activity: serde_json::from_value(item["activity"].clone())?,
                pin: item
                    .get("pin")
                    .filter(|value| !value.is_null())
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()?,
                actions: agent.thread_actions(endpoint, id)?,
            })
        })
        .collect()
}
