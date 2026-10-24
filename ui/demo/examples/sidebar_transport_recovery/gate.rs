//! Owned test-host availability gate. Returned endpoints remain the real adapters.
use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tekes_agent_contract::{EndpointConnector, Error, Result, SessionEndpoint};

#[derive(Default)]
struct State {
    blocked: bool,
    generation: u64,
    attempts: u64,
    successes: u64,
    rejections: u64,
    close_count: u64,
    endpoint: Option<Arc<dyn SessionEndpoint>>,
}

pub struct Gate {
    inner: Arc<dyn EndpointConnector>,
    state: Mutex<State>,
}

impl Gate {
    pub fn new(inner: Arc<dyn EndpointConnector>) -> Self {
        Self {
            inner,
            state: Mutex::new(State::default()),
        }
    }
    pub fn snapshot(&self) -> Value {
        let state = self.state.lock().expect("owned gate lock");
        json!({"attempts":state.attempts,"successes":state.successes,
            "rejections":state.rejections,"closeCount":state.close_count,
            "blocked":state.blocked,"connectionGeneration":state.successes,
            "activeEndpoint":state.endpoint.is_some()})
    }
    pub fn current(&self) -> Option<Arc<dyn SessionEndpoint>> {
        self.state.lock().expect("owned gate lock").endpoint.clone()
    }
    pub async fn interrupt(&self) -> Result<()> {
        let endpoint = {
            let mut state = self.state.lock().expect("owned gate lock");
            state.blocked = true;
            state.generation += 1;
            state.endpoint.take().ok_or(Error::Disconnected)?
        };
        // Counts this gate's actual explicit close invocation, not runtime cleanup calls.
        endpoint.close().await;
        self.state.lock().expect("owned gate lock").close_count += 1;
        Ok(())
    }
    pub fn reopen(&self) {
        let mut state = self.state.lock().expect("owned gate lock");
        state.blocked = false;
        state.generation += 1;
    }
}

#[async_trait]
impl EndpointConnector for Gate {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        let generation = {
            let mut state = self.state.lock().expect("owned gate lock");
            state.attempts += 1;
            if state.blocked {
                state.rejections += 1;
                return Err(Error::Transport(
                    "owned acceptance connection gate closed".into(),
                ));
            }
            state.generation
        };
        let endpoint = self.inner.connect().await?;
        let accepted = {
            let mut state = self.state.lock().expect("owned gate lock");
            if state.blocked || state.generation != generation {
                state.rejections += 1;
                false
            } else {
                state.successes += 1;
                state.endpoint = Some(endpoint.clone());
                true
            }
        };
        if !accepted {
            endpoint.close().await;
            return Err(Error::Disconnected);
        }
        Ok(endpoint)
    }
}
