//! Deterministic, local SEP authority fixture. No model, credentials or harness-private reads.
use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tekes_agent_contract::*;
use tekes_agent_presentation::{RunController, RunScope};
use tekes_agent_session::SessionRuntime;
use tokio::sync::broadcast;
#[derive(Clone)]
pub struct Fixture {
    sessions: Arc<Mutex<BTreeMap<String, Line>>>,
    connects: Arc<AtomicUsize>,
    forks: Arc<AtomicUsize>,
    side_chats: bool,
}
struct Line {
    rows: Vec<HistoryEntry>,
    sender: broadcast::Sender<Value>,
    cancel: Arc<AtomicBool>,
    busy: bool,
    turn: i64,
    ephemeral: bool,
}
fn entry(seq: i64, kind: &str, data: Value) -> HistoryEntry {
    serde_json::from_value(json!({"event":{"seq":seq,"time":seq+1,"type":kind,"data":data}}))
        .unwrap()
}
fn append(line: &mut Line, kind: &str, data: Value) {
    let row = entry(line.rows.len() as i64, kind, data);
    line.rows.push(row.clone());
    let _ = line.sender.send(json!({"type":"event","entry":row}));
}
impl Default for Fixture {
    fn default() -> Self {
        let mut sessions = BTreeMap::new();
        for id in ["a", "b"] {
            let (sender, _) = broadcast::channel(256);
            let mut line = Line {
                rows: vec![],
                sender,
                cancel: Arc::new(AtomicBool::new(false)),
                busy: false,
                turn: 30,
                ephemeral: false,
            };
            for n in 1..=30 {
                append(&mut line, "turn/start", json!({"turn":n}));
                append(
                    &mut line,
                    "user/message",
                    json!({"turn":n,"id":format!("{id}-u{n}"),"content":[{"type":"text","text":format!("会话 {id} · 历史问题 {n}")}]}),
                );
                append(
                    &mut line,
                    "assistant/message",
                    json!({"turn":n,"step":1,"message":{"id":format!("{id}-m{n}"),"content":[{"type":"text","text":format!("历史回答 {n}：这是一段用于验证阅读位置与分页的中文记录。")}]}}),
                );
                append(
                    &mut line,
                    "turn/end",
                    json!({"turn":n,"reason":"completed"}),
                );
            }
            sessions.insert(id.into(), line);
        }
        Self {
            sessions: Arc::new(Mutex::new(sessions)),
            connects: Arc::new(AtomicUsize::new(0)),
            forks: Arc::new(AtomicUsize::new(0)),
            side_chats: false,
        }
    }
}
impl Fixture {
    pub fn with_side_chats(mut self) -> Self {
        self.side_chats = true;
        self
    }
    /// Explicit owned demo data, emitted through the same journal contract.
    pub fn with_demo_summary(self) -> Self {
        {
            let mut sessions = self.sessions.lock().unwrap();
            for (id, line) in sessions.iter_mut() {
                append(
                    line,
                    "todo/write",
                    json!({"todos":[
                        {"content":format!("验证会话 {id} 的数据回传"),"status":"completed"},
                        {"content":"接入正式 Transcript","status":"pending"}
                    ]}),
                );
            }
        }
        self
    }
}
struct Endpoint {
    fixture: Fixture,
    host: HostDescription,
    generation: String,
}
#[async_trait]
impl EndpointConnector for Fixture {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok(Arc::new(Endpoint {
            fixture: self.clone(),
            generation: format!("fixture-{}", self.connects.fetch_add(1, Ordering::SeqCst)),
            host: serde_json::from_value(
                json!({"protocolVersion":4,"product":{"name":"Local SEP fixture","version":"1"},"capabilities":["session-journal","session-inventory-sync","session-control-sync","actionable-sync"],"cwd":"/fixture","home":"/fixture","attachedSessions":0,"canOpenPath":false}),
            )?,
        }))
    }
}
#[async_trait]
impl SessionEndpoint for Endpoint {
    fn identity(&self) -> &str {
        "fixture"
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn capabilities(&self) -> EndpointCapabilities {
        let mut caps = EndpointCapabilities::methods(&["session.prompt", "session.cancel"]);
        if self.fixture.side_chats {
            caps.methods
                .extend(["session.fork".into(), "session.discard".into()]);
            caps.supports_ephemeral_fork = true;
        }
        caps
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        let g = &self.generation;
        let baseline = match target {
            StreamTarget::SessionInventory => {
                let sessions = self.fixture.sessions.lock().unwrap();
                let items: Vec<_> = sessions.iter().filter(|(_, line)| !line.ephemeral).map(|(id,line)| json!({"sessionId":id,"title":format!("会话 {id}"),"running":line.busy,"blank":line.rows.is_empty(),"updatedAt":line.rows.len()})).collect();
                json!({"type":"baseline","generation":g,"snapshot":{"items":items}})
            }
            StreamTarget::SessionControl => {
                json!({"type":"baseline","generation":g,"snapshot":{"queues":[],"jobs":[],"projections":[]}})
            }
            StreamTarget::Actionables => json!({"type":"reset","generation":g}),
            StreamTarget::SessionJournal { address, .. } => {
                let sessions = self.fixture.sessions.lock().unwrap();
                let line = sessions
                    .get(&address.session_id)
                    .ok_or_else(|| unsupported("fixture session"))?;
                let receiver = line.sender.subscribe();
                let start = line.rows.len().saturating_sub(40);
                let baseline = json!({"type":"snapshot","snapshot":{"sessionId":address.session_id,"generation":g,"throughSequence":line.rows.len() as i64-1,"windowLimit":40,"entries":line.rows[start..],"hasMoreBefore":start>0}});
                let id = address.session_id;
                let generation = g.clone();
                let updates = stream::unfold(receiver, move |mut receiver| {
                    let generation = generation.clone();
                    let id = id.clone();
                    async move {
                        let result = receiver
                            .recv()
                            .await
                            .map(|mut v| {
                                v["generation"] = json!(generation);
                                v["sessionId"] = json!(id);
                                v
                            })
                            .map_err(|_| Error::Disconnected);
                        Some((result, receiver))
                    }
                });
                return Ok(Box::pin(
                    stream::once(async { Ok(baseline) }).chain(updates),
                ));
            }
            _ => return Err(unsupported("fixture stream")),
        };
        Ok(Box::pin(
            stream::once(async { Ok(baseline) }).chain(stream::pending()),
        ))
    }
    async fn history_page(&self, r: HistoryRequest) -> Result<HistoryPage> {
        r.validate()?;
        let sessions = self.fixture.sessions.lock().unwrap();
        let line = sessions
            .get(&r.session_id)
            .ok_or_else(|| unsupported("fixture session"))?;
        let end = r.before_sequence.unwrap_or(r.through_sequence + 1).max(0) as usize;
        require(end <= line.rows.len(), "fixture history cut")?;
        let start = end.saturating_sub(r.max_messages);
        Ok(HistoryPage {
            session_id: r.session_id,
            through_sequence: r.through_sequence,
            entries: line.rows[start..end].to_vec(),
            has_more_before: start > 0,
        })
    }
    async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
        Err(unsupported("fixture actionable"))
    }
    async fn call(&self, method: &str, p: Value, rpc: &str) -> Result<Value> {
        let id = string(&p, "sessionId")?.to_owned();
        let mut sessions = self.fixture.sessions.lock().unwrap();
        let line = sessions
            .get_mut(&id)
            .ok_or_else(|| unsupported("fixture session"))?;
        if method == "session.fork" && self.fixture.side_chats {
            let ephemeral = p["ephemeral"].as_bool().unwrap_or(false);
            let end = if let Some(sequence) = p["atSeq"].as_u64() {
                (sequence as usize + 1).min(line.rows.len())
            } else if line.busy {
                line.rows
                    .iter()
                    .rposition(|row| row.event.kind == "turn/end")
                    .map_or(0, |index| index + 1)
            } else {
                line.rows.len()
            };
            let rows = line.rows[..end].to_vec();
            let turn = line.turn;
            let next = self.fixture.forks.fetch_add(1, Ordering::SeqCst) + 1;
            let child = format!("{}-{next}", if ephemeral { "scratch" } else { "fork" });
            require(
                !sessions.contains_key(&child),
                "fixture fork identity collision",
            )?;
            sessions.insert(
                child.clone(),
                Line {
                    rows,
                    sender: broadcast::channel(256).0,
                    cancel: Arc::new(AtomicBool::new(false)),
                    busy: false,
                    turn,
                    ephemeral,
                },
            );
            return Ok(json!({"sessionId":child}));
        }
        if method == "session.discard" && self.fixture.side_chats {
            require(line.ephemeral, "fixture only discards ephemeral sessions")?;
            if line.busy {
                return Err(Error::Remote(json!({"code":"session-running"})));
            }
            sessions.remove(&id);
            return Ok(json!({"discarded":true}));
        }
        if method == "session.cancel" {
            line.cancel.store(true, Ordering::SeqCst);
            return Ok(json!({"cancelled":true}));
        }
        require(method == "session.prompt", "fixture method")?;
        require(!line.busy, "fixture is busy; one active input only")?;
        let content = p["content"].clone();
        line.busy = true;
        line.turn += 1;
        line.cancel = Arc::new(AtomicBool::new(false));
        let cancel = line.cancel.clone();
        let turn = line.turn;
        append(line, "turn/start", json!({"turn":turn}));
        append(
            line,
            "user/message",
            json!({"turn":turn,"id":rpc,"source":{"kind":"user","rpcId":rpc},"content":content}),
        );
        let fixture = self.fixture.clone();
        tokio::spawn(async move {
            let mut text = String::new();
            for chunk in [
                "你好，",
                "这是 ",
                "AgentKit ",
                "通过 SEP ",
                "逐段回传的 ",
                "中文响应。",
                "\n可在回传中切换会话、向上阅读或停止。",
            ] {
                tokio::time::sleep(Duration::from_millis(200)).await;
                if cancel.load(Ordering::SeqCst) {
                    break;
                }
                text.push_str(chunk);
                let sessions = fixture.sessions.lock().unwrap();
                let line = &sessions[&id];
                let _=line.sender.send(json!({"type":"transient","event":{"seq":0,"time":1,"type":"assistant/chunk","data":{"turn":turn,"step":1,"chunk":{"type":"text-delta","text":chunk}}}}));
            }
            let mut sessions = fixture.sessions.lock().unwrap();
            let line = sessions.get_mut(&id).unwrap();
            append(
                line,
                "assistant/message",
                json!({"turn":turn,"step":1,"message":{"id":format!("{id}-m{turn}"),"content":[{"type":"text","text":text}]}}),
            );
            append(
                line,
                "turn/end",
                json!({"turn":turn,"reason":if cancel.load(Ordering::SeqCst){"interrupted"}else{"completed"}}),
            );
            line.busy = false;
        });
        Ok(json!({"accepted":true}))
    }
    async fn close(&self) {}
}
pub async fn setup() -> Result<(Arc<SessionRuntime>, Arc<RunController>)> {
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await?;
    let controller = Arc::new(RunController::new(runtime.clone())?);
    controller.open(RunScope::main("fixture", "a")).await?;
    tokio::time::sleep(Duration::from_millis(40)).await;
    Ok((runtime, controller))
}
