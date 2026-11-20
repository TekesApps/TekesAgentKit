//! Deterministic multi-session SEP v4 endpoint fixture for TekesAgentKit AppHost UI.
//! Supports inventory, control (queues & subagent jobs), actionables, paged history,
//! streaming responses, cancellation, forks/sidechats, and normalized settings.

use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tekes_agent_contract::*;
use tokio::sync::broadcast;

pub const DEFAULT_MAIN_SESSION_ID: &str = "session-alpha";
pub const SECONDARY_SESSION_ID: &str = "session-beta";
pub const CHILD_SUBAGENT_SESSION_ID: &str = "11111111-1111-4111-8111-111111111111";

#[derive(Clone, Debug)]
struct SessionMeta {
    title: String,
    folder: String,
    running: bool,
    status: String,
    updated_at: f64,
    parent_session_id: Option<String>,
}

struct SessionLine {
    meta: SessionMeta,
    rows: Vec<HistoryEntry>,
    sender: broadcast::Sender<Value>,
    cancel: Arc<AtomicBool>,
    turn: i64,
    todos: Value,
    token_usage: Value,
}

#[derive(Clone, Debug)]
struct FixtureQueueItem {
    id: String,
    text: String,
}

struct FixtureState {
    sessions: BTreeMap<String, SessionLine>,
    pinned_ids: Vec<String>,
    archived_ids: Vec<String>,
    queues: BTreeMap<String, Vec<FixtureQueueItem>>,
    jobs: BTreeMap<String, Vec<Value>>,
    actionables: BTreeMap<String, Value>,
    settings_revision: u64,
    settings_values: BTreeMap<String, Value>,
    settings_read_only: bool,
    inventory_tx: broadcast::Sender<Value>,
    control_tx: broadcast::Sender<Value>,
    actionables_tx: broadcast::Sender<Value>,
    next_id: u64,
}

fn entry(seq: i64, kind: &str, data: Value) -> HistoryEntry {
    serde_json::from_value(json!({
        "event": {
            "seq": seq,
            "time": 1_760_000_000.0 + seq as f64,
            "type": kind,
            "data": data
        }
    }))
    .expect("valid fixture HistoryEntry")
}

fn append_row(line: &mut SessionLine, kind: &str, data: Value) {
    let seq = line.rows.len() as i64;
    let row = entry(seq, kind, data);
    line.rows.push(row.clone());
    let _ = line.sender.send(json!({
        "type": "event",
        "entry": row,
    }));
}

fn summary_value(id: &str, line: &SessionLine) -> Value {
    let mut obj = json!({
        "sessionId": id,
        "title": line.meta.title,
        "folder": line.meta.folder,
        "running": line.meta.running,
        "blank": line.rows.is_empty(),
        "status": line.meta.status,
        "updatedAt": line.meta.updated_at,
        "projections": {
            "asOfSeq": (line.rows.len() as i64).saturating_sub(1),
            "values": {
                "todos": line.todos,
                "tokenUsage": line.token_usage,
            }
        }
    });
    if let Some(parent) = &line.meta.parent_session_id {
        obj["parentSessionId"] = json!(parent);
    }
    obj
}

#[derive(Clone)]
pub struct FixtureSessionEndpoint {
    inner: Arc<Mutex<FixtureState>>,
    connects: Arc<AtomicU64>,
    connect_delay: Duration,
    chunk_delay: Duration,
}

impl Default for FixtureSessionEndpoint {
    fn default() -> Self {
        Self::new()
    }
}

impl FixtureSessionEndpoint {
    pub fn new() -> Self {
        Self::with_timing(Duration::from_millis(20), Duration::from_millis(12))
    }

    pub fn with_timing(connect_delay: Duration, chunk_delay: Duration) -> Self {
        let (inventory_tx, _) = broadcast::channel(128);
        let (control_tx, _) = broadcast::channel(128);
        let (actionables_tx, _) = broadcast::channel(128);
        let mut sessions = BTreeMap::new();

        // 1. Primary session with 26 turns (> 80 entries) so pagination + tool report + markdown render cleanly.
        {
            let (sender, _) = broadcast::channel(256);
            let mut line = SessionLine {
                meta: SessionMeta {
                    title: "AppHost 视图与会话工作台".into(),
                    folder: "TekesAgentKit".into(),
                    running: false,
                    status: "active".into(),
                    updated_at: 1_760_000_260.0,
                    parent_session_id: None,
                },
                rows: Vec::new(),
                sender,
                cancel: Arc::new(AtomicBool::new(false)),
                turn: 26,
                todos: json!([
                    { "content": "对齐 TekesUI AppHost 容器默认边距与无边框阅读流", "status": "completed" },
                    { "content": "验证 WindowWake 流式刷新与长历史前置锚点", "status": "in_progress" }
                ]),
                token_usage: json!({
                    "uncachedInputTokens": 1240,
                    "cacheReadTokens": 640,
                    "cacheWriteTokens": 120,
                    "outputTokens": 380
                }),
            };
            for n in 1..=25 {
                append_row(&mut line, "turn/start", json!({ "turn": n }));
                append_row(
                    &mut line,
                    "user/message",
                    json!({
                        "turn": n,
                        "id": format!("alpha-u-{n}"),
                        "content": [{ "type": "text", "text": format!("检查模块 #{n:02}：确认 AppHost 容器切换与中文输入法组合态。") }]
                    }),
                );
                append_row(
                    &mut line,
                    "assistant/message",
                    json!({
                        "turn": n,
                        "step": 1,
                        "message": {
                            "id": format!("alpha-a-{n}"),
                            "content": [{ "type": "text", "text": format!("已完成模块 #{n:02} 巡检：容器实例与输入状态均保持稳定。") }],
                            "usage": { "inputTokens": 48, "outputTokens": 24 }
                        }
                    }),
                );
                append_row(
                    &mut line,
                    "turn/end",
                    json!({ "turn": n, "reason": "completed" }),
                );
            }
            // Rich turn 26 with thinking, tool call, tool result, markdown + Rust code fence.
            append_row(&mut line, "turn/start", json!({ "turn": 26 }));
            append_row(
                &mut line,
                "user/message",
                json!({
                    "turn": 26,
                    "id": "alpha-u-26",
                    "content": [{
                        "type": "text",
                        "text": "检查 AppHost 容器切换与长会话分页性能，并总结本次改动。"
                    }]
                }),
            );
            append_row(
                &mut line,
                "tool/call",
                json!({
                    "turn": 26,
                    "callId": "call-check-apphost",
                    "name": "read",
                    "arguments": { "path": "ui/apphost/src/container.rs" }
                }),
            );
            append_row(
                &mut line,
                "tool/result",
                json!({
                    "turn": 26,
                    "message": {
                        "source": { "callId": "call-check-apphost" },
                        "content": [{
                            "type": "text",
                            "text": "Verified ManagedAgentContainer preserves mount_identity across promote() (+42 -7 lines)."
                        }]
                    }
                }),
            );
            append_row(
                &mut line,
                "assistant/message",
                json!({
                    "turn": 26,
                    "step": 1,
                    "sessionFinal": true,
                    "message": {
                        "id": "alpha-a-26",
                        "content": [
                            {
                                "type": "thinking",
                                "thinking": "正在核对 WindowWake 驱动的 prepare_for_render、Inactive-Tab 延期同步，以及 ReadingController 前置加载锚点..."
                            },
                            {
                                "type": "text",
                                "text": "## AppHost 阶段进展\n\n已完成 Core Trio 容器投影，支持 Focus 与 Others 零重建切换。\n\n- ReadingController 保持前置分页阅读位置\n- TextFieldContent 在组合态保护中文 preedit\n\n```rust\nstate.promote(\"agent-run\")?;\n```\n\n详情见 [架构设计](doc:docs/tekesui-apphost-architecture.md)。"
                            }
                        ],
                        "usage": { "inputTokens": 320, "outputTokens": 148 }
                    }
                }),
            );
            append_row(
                &mut line,
                "turn/end",
                json!({ "turn": 26, "reason": "completed" }),
            );
            sessions.insert(DEFAULT_MAIN_SESSION_ID.into(), line);
        }

        // 2. Secondary session with subagent call pointing to CHILD_SUBAGENT_SESSION_ID + question prompt.
        {
            let (sender, _) = broadcast::channel(256);
            let mut line = SessionLine {
                meta: SessionMeta {
                    title: "验证工作区 Diff 与 Citation 引用".into(),
                    folder: "workspace-qa".into(),
                    running: false,
                    status: "active".into(),
                    updated_at: 1_760_000_200.0,
                    parent_session_id: None,
                },
                rows: Vec::new(),
                sender,
                cancel: Arc::new(AtomicBool::new(false)),
                turn: 1,
                todos: json!([
                    { "content": "确认 ReadOnlyFsWorkspaceEndpoint cap-std 边界", "status": "completed" }
                ]),
                token_usage: json!({
                    "uncachedInputTokens": 420,
                    "cacheReadTokens": 120,
                    "cacheWriteTokens": 0,
                    "outputTokens": 96
                }),
            };
            append_row(&mut line, "turn/start", json!({ "turn": 1 }));
            append_row(
                &mut line,
                "user/message",
                json!({
                    "turn": 1,
                    "id": "beta-u-1",
                    "content": [{ "type": "text", "text": "请派发只读子代理检查工作区服务权限，并汇总摘要。" }]
                }),
            );
            append_row(
                &mut line,
                "tool/call",
                json!({
                    "turn": 1,
                    "callId": "call-subagent-audit",
                    "name": "subagent",
                    "arguments": { "brief": "审计 WorkspaceServiceEndpoint 只读边界" }
                }),
            );
            append_row(
                &mut line,
                "tool/result",
                json!({
                    "turn": 1,
                    "message": {
                        "source": { "callId": "call-subagent-audit" },
                        "content": [{
                            "type": "text",
                            "text": format!("Subagent child session ready: {CHILD_SUBAGENT_SESSION_ID}")
                        }]
                    }
                }),
            );
            append_row(
                &mut line,
                "assistant/message",
                json!({
                    "turn": 1,
                    "step": 1,
                    "sessionFinal": true,
                    "message": {
                        "id": "beta-a-1",
                        "content": [{
                            "type": "text",
                            "text": "子代理已启动并完成初步边界扫描，可在 Summary 或 Tasks 中点击查看只读执行轨迹。"
                        }],
                        "usage": { "inputTokens": 420, "outputTokens": 96 }
                    }
                }),
            );
            append_row(
                &mut line,
                "turn/end",
                json!({ "turn": 1, "reason": "completed" }),
            );
            sessions.insert(SECONDARY_SESSION_ID.into(), line);
        }

        // 3. Child subagent session with paginated history (32 turns) for ChildRunContainer.
        {
            let (sender, _) = broadcast::channel(256);
            let mut line = SessionLine {
                meta: SessionMeta {
                    title: "子代理：审计 WorkspaceServiceEndpoint 只读边界".into(),
                    folder: "subagents".into(),
                    running: false,
                    status: "completed".into(),
                    updated_at: 1_760_000_190.0,
                    parent_session_id: Some(SECONDARY_SESSION_ID.into()),
                },
                rows: Vec::new(),
                sender,
                cancel: Arc::new(AtomicBool::new(false)),
                turn: 32,
                todos: json!([]),
                token_usage: json!({
                    "uncachedInputTokens": 210,
                    "cacheReadTokens": 0,
                    "cacheWriteTokens": 0,
                    "outputTokens": 72
                }),
            };
            for n in 1..=32 {
                append_row(&mut line, "turn/start", json!({ "turn": n }));
                append_row(
                    &mut line,
                    "user/message",
                    json!({
                        "turn": n,
                        "id": format!("child-u-{n}"),
                        "content": [{ "type": "text", "text": format!("子任务检查点 #{n:02}") }]
                    }),
                );
                append_row(
                    &mut line,
                    "assistant/message",
                    json!({
                        "turn": n,
                        "step": 1,
                        "sessionFinal": true,
                        "message": {
                            "id": format!("child-a-{n}"),
                            "content": [{ "type": "text", "text": format!("检查点 #{n:02} 通过：路径规范化与 cap-std 目录逃逸拦截正常。") }]
                        }
                    }),
                );
                append_row(
                    &mut line,
                    "turn/end",
                    json!({ "turn": n, "reason": "completed" }),
                );
            }
            sessions.insert(CHILD_SUBAGENT_SESSION_ID.into(), line);
        }

        let mut actionables = BTreeMap::new();
        actionables.insert(
            "q-beta-1".into(),
            json!({
                "id": "q-beta-1",
                "sessionId": SECONDARY_SESSION_ID,
                "kind": "question",
                "revision": 1,
                "payload": {
                    "sessionId": SECONDARY_SESSION_ID,
                    "questions": [{
                        "id": "scope",
                        "question": "请选择子代理审计输出的详细程度：",
                        "detail": "摘要模式适合侧栏卡片；完整模式会包含每个文件的行级检查结果。",
                        "options": [
                            { "label": "仅关键边界发现" },
                            { "label": "完整逐文件报告" }
                        ]
                    }]
                }
            }),
        );

        let mut queues = BTreeMap::new();
        queues.insert(
            DEFAULT_MAIN_SESSION_ID.into(),
            vec![FixtureQueueItem {
                id: "queue-1".into(),
                text: "补充 Files / Changes / Artifact 容器验收".into(),
            }],
        );

        let mut jobs = BTreeMap::new();
        jobs.insert(
            SECONDARY_SESSION_ID.into(),
            vec![json!({
                "id": "job-audit-1",
                "sessionId": SECONDARY_SESSION_ID,
                "childSessionId": CHILD_SUBAGENT_SESSION_ID,
                "name": "审计 WorkspaceServiceEndpoint 只读边界",
                "status": "completed",
                "kind": "subagent"
            })],
        );

        let mut settings_values = BTreeMap::new();
        settings_values.insert(
            "execution".into(),
            json!({
                "approvalPolicy": "ask",
                "sandboxMode": "workspace-write",
                "maxTurns": 24,
                "apiToken": "sk-fixture-secret-998877"
            }),
        );

        Self {
            inner: Arc::new(Mutex::new(FixtureState {
                sessions,
                pinned_ids: vec![DEFAULT_MAIN_SESSION_ID.into()],
                archived_ids: Vec::new(),
                queues,
                jobs,
                actionables,
                settings_revision: 1,
                settings_values,
                settings_read_only: false,
                inventory_tx,
                control_tx,
                actionables_tx,
                next_id: 1,
            })),
            connects: Arc::new(AtomicU64::new(0)),
            connect_delay,
            chunk_delay,
        }
    }

    pub fn set_settings_read_only(&self, read_only: bool) {
        self.inner.lock().expect("fixture lock").settings_read_only = read_only;
    }

    pub fn expire_child_session(&self, child_session_id: &str) {
        let mut guard = self.inner.lock().expect("fixture lock");
        guard.sessions.remove(child_session_id);
        let _ = guard.inventory_tx.send(json!({
            "type": "remove",
            "sessionId": child_session_id,
        }));
    }

    fn queue_json_for(guard: &FixtureState, session_id: &str) -> Vec<Value> {
        guard
            .queues
            .get(session_id)
            .map(|items| {
                items
                    .iter()
                    .map(|item| {
                        json!({
                            "id": item.id,
                            "text": item.text,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

struct ConnectedFixtureEndpoint {
    fixture: FixtureSessionEndpoint,
    host: HostDescription,
    generation: String,
}

#[async_trait]
impl EndpointConnector for FixtureSessionEndpoint {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        if !self.connect_delay.is_zero() {
            tokio::time::sleep(self.connect_delay).await;
        }
        let generation = format!(
            "fixture-gen-{}",
            self.connects.fetch_add(1, Ordering::SeqCst)
        );
        let host: HostDescription = serde_json::from_value(json!({
            "protocolVersion": 4,
            "product": { "name": "Local SEP Fixture", "version": "1.0.0" },
            "capabilities": [
                "workspace-sync",
                "session-inventory-sync",
                "session-journal",
                "session-control-sync",
                "actionable-sync"
            ],
            "cwd": "/fixture/tekes-agentkit",
            "home": "/fixture",
            "attachedSessions": 2,
            "canOpenPath": false
        }))?;
        Ok(Arc::new(ConnectedFixtureEndpoint {
            fixture: self.clone(),
            host,
            generation,
        }))
    }
}

#[async_trait]
impl SessionEndpoint for ConnectedFixtureEndpoint {
    fn identity(&self) -> &str {
        "fixture"
    }

    fn host(&self) -> &HostDescription {
        &self.host
    }

    fn capabilities(&self) -> EndpointCapabilities {
        let mut caps = EndpointCapabilities::methods(&[
            "session.prompt",
            "session.cancel",
            "session.create",
            "session.rename",
            "session.archive",
            "session.unarchive",
            "session.fork",
            "session.discard",
            "session.updateQueue",
            "session.search",
            "settings.describe",
            "settings.mutate",
        ]);
        caps.supports_archive_session = true;
        caps.supports_unarchive_session = true;
        caps.supports_ephemeral_fork = true;
        caps.supports_question_cancellation = true;
        caps
    }

    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        let g = self.generation.clone();
        match target {
            StreamTarget::WorkspaceInventory => {
                let (pinned, archived) = {
                    let guard = self.fixture.inner.lock().expect("fixture lock");
                    (guard.pinned_ids.clone(), guard.archived_ids.clone())
                };
                let baseline = json!({
                    "type": "baseline",
                    "generation": g,
                    "snapshot": {
                        "items": [{
                            "workspaceId": "ws-main",
                            "path": "/fixture/tekes-agentkit",
                            "name": "TekesAgentKit"
                        }],
                        "archivedSessionIds": archived,
                        "pinnedSessionIds": pinned,
                    }
                });
                Ok(Box::pin(
                    stream::once(async { Ok(baseline) }).chain(stream::pending()),
                ))
            }
            StreamTarget::SessionInventory => {
                let (baseline, rx) = {
                    let guard = self.fixture.inner.lock().expect("fixture lock");
                    let items: Vec<Value> = guard
                        .sessions
                        .iter()
                        .map(|(id, line)| summary_value(id, line))
                        .collect();
                    (
                        json!({
                            "type": "baseline",
                            "generation": g,
                            "snapshot": { "items": items }
                        }),
                        guard.inventory_tx.subscribe(),
                    )
                };
                let updates = stream::unfold(rx, move |mut rx| {
                    let gen_id = g.clone();
                    async move {
                        let res = rx
                            .recv()
                            .await
                            .map(|mut frame| {
                                frame["generation"] = json!(gen_id);
                                frame
                            })
                            .map_err(|_| Error::Disconnected);
                        Some((res, rx))
                    }
                });
                Ok(Box::pin(
                    stream::once(async { Ok(baseline) }).chain(updates),
                ))
            }
            StreamTarget::SessionControl => {
                let (baseline, rx) = {
                    let guard = self.fixture.inner.lock().expect("fixture lock");
                    let queues: Vec<Value> = guard
                        .sessions
                        .keys()
                        .map(|id| {
                            json!({
                                "sessionId": id,
                                "items": FixtureSessionEndpoint::queue_json_for(&guard, id),
                            })
                        })
                        .collect();
                    let jobs: Vec<Value> = guard
                        .jobs
                        .iter()
                        .map(|(id, list)| {
                            json!({
                                "sessionId": id,
                                "jobs": list,
                            })
                        })
                        .collect();
                    (
                        json!({
                            "type": "baseline",
                            "generation": g,
                            "snapshot": {
                                "queues": queues,
                                "jobs": jobs,
                                "projections": [],
                            }
                        }),
                        guard.control_tx.subscribe(),
                    )
                };
                let updates = stream::unfold(rx, move |mut rx| {
                    let gen_id = g.clone();
                    async move {
                        let res = rx
                            .recv()
                            .await
                            .map(|mut frame| {
                                frame["generation"] = json!(gen_id);
                                frame
                            })
                            .map_err(|_| Error::Disconnected);
                        Some((res, rx))
                    }
                });
                Ok(Box::pin(
                    stream::once(async { Ok(baseline) }).chain(updates),
                ))
            }
            StreamTarget::Actionables => {
                let (initial_frames, rx) = {
                    let guard = self.fixture.inner.lock().expect("fixture lock");
                    let mut frames = vec![json!({ "type": "reset", "generation": g })];
                    for value in guard.actionables.values() {
                        frames.push(json!({
                            "type": "upsert",
                            "generation": g,
                            "actionable": value,
                        }));
                    }
                    (frames, guard.actionables_tx.subscribe())
                };
                let updates = stream::unfold(rx, move |mut rx| {
                    let gen_id = g.clone();
                    async move {
                        let res = rx
                            .recv()
                            .await
                            .map(|mut frame| {
                                frame["generation"] = json!(gen_id);
                                frame
                            })
                            .map_err(|_| Error::Disconnected);
                        Some((res, rx))
                    }
                });
                Ok(Box::pin(
                    stream::iter(initial_frames.into_iter().map(Ok)).chain(updates),
                ))
            }
            StreamTarget::SessionJournal { address, .. } => {
                let guard = self.fixture.inner.lock().expect("fixture lock");
                let line = guard
                    .sessions
                    .get(&address.session_id)
                    .ok_or_else(|| unsupported("fixture session"))?;
                let rx = line.sender.subscribe();
                let limit = 40usize;
                let start = line.rows.len().saturating_sub(limit);
                let through = line.rows.len() as i64 - 1;
                let baseline = json!({
                    "type": "snapshot",
                    "snapshot": {
                        "sessionId": address.session_id,
                        "generation": g,
                        "throughSequence": through,
                        "windowLimit": limit,
                        "entries": &line.rows[start..],
                        "hasMoreBefore": start > 0,
                        "projections": {
                            "asOfSeq": through.max(0),
                            "values": {
                                "todos": line.todos,
                                "tokenUsage": line.token_usage,
                            }
                        }
                    }
                });
                let sid = address.session_id;
                let gen_id = g;
                let updates = stream::unfold(rx, move |mut rx| {
                    let gen_id = gen_id.clone();
                    let sid = sid.clone();
                    async move {
                        let res = rx
                            .recv()
                            .await
                            .map(|mut frame| {
                                frame["generation"] = json!(gen_id);
                                frame["sessionId"] = json!(sid);
                                frame
                            })
                            .map_err(|_| Error::Disconnected);
                        Some((res, rx))
                    }
                });
                Ok(Box::pin(
                    stream::once(async { Ok(baseline) }).chain(updates),
                ))
            }
            StreamTarget::SessionConversation { .. } => {
                Err(unsupported("fixture native conversation stream"))
            }
        }
    }

    async fn history_page(&self, request: HistoryRequest) -> Result<HistoryPage> {
        request.validate()?;
        let guard = self.fixture.inner.lock().expect("fixture lock");
        let line = guard
            .sessions
            .get(&request.session_id)
            .ok_or_else(|| unsupported("fixture session"))?;
        let end = request
            .before_sequence
            .unwrap_or(request.through_sequence + 1)
            .max(0) as usize;
        require(end <= line.rows.len(), "fixture history cut")?;
        let start = end.saturating_sub(request.max_messages);
        Ok(HistoryPage {
            session_id: request.session_id,
            through_sequence: request.through_sequence,
            entries: line.rows[start..end].to_vec(),
            has_more_before: start > 0,
        })
    }

    async fn respond(
        &self,
        actionable_id: &str,
        expected_revision: i64,
        _outcome: Value,
    ) -> Result<()> {
        let mut guard = self.fixture.inner.lock().expect("fixture lock");
        let current = guard
            .actionables
            .get(actionable_id)
            .ok_or_else(|| Error::Protocol("unknown actionable".into()))?;
        let rev = integer(current, "revision")?;
        require(rev == expected_revision, "stale actionable revision")?;
        guard.actionables.remove(actionable_id);
        let _ = guard.actionables_tx.send(json!({
            "type": "resolved",
            "actionableId": actionable_id,
            "revision": expected_revision,
        }));
        Ok(())
    }

    async fn call(&self, method: &str, payload: Value, rpc_id: &str) -> Result<Value> {
        match method {
            "session.create" => {
                let mut guard = self.fixture.inner.lock().expect("fixture lock");
                guard.next_id += 1;
                let id = payload["sessionId"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("session-new-{}", guard.next_id));
                let (sender, _) = broadcast::channel(256);
                let line = SessionLine {
                    meta: SessionMeta {
                        title: format!("新建对话 {}", guard.next_id),
                        folder: "tekes-agentkit".into(),
                        running: false,
                        status: "active".into(),
                        updated_at: 1_760_000_300.0 + guard.next_id as f64,
                        parent_session_id: None,
                    },
                    rows: Vec::new(),
                    sender,
                    cancel: Arc::new(AtomicBool::new(false)),
                    turn: 0,
                    todos: json!([]),
                    token_usage: json!({
                        "uncachedInputTokens": 0,
                        "cacheReadTokens": 0,
                        "cacheWriteTokens": 0,
                        "outputTokens": 0
                    }),
                };
                let summary = summary_value(&id, &line);
                guard.sessions.insert(id.clone(), line);
                let _ = guard.inventory_tx.send(json!({
                    "type": "upsert",
                    "session": summary,
                }));
                Ok(json!({ "sessionId": id }))
            }
            "session.fork" => {
                let parent_id = string(&payload, "sessionId")?.to_owned();
                let mut guard = self.fixture.inner.lock().expect("fixture lock");
                guard.next_id += 1;
                let fork_id = format!("sidechat-{}", guard.next_id);
                let parent_title = guard
                    .sessions
                    .get(&parent_id)
                    .map(|l| l.meta.title.clone())
                    .unwrap_or_else(|| parent_id.clone());
                let (sender, _) = broadcast::channel(256);
                let mut line = SessionLine {
                    meta: SessionMeta {
                        title: format!("SideChat · {parent_title}"),
                        folder: "sidechats".into(),
                        running: false,
                        status: "active".into(),
                        updated_at: 1_760_000_400.0 + guard.next_id as f64,
                        parent_session_id: Some(parent_id),
                    },
                    rows: Vec::new(),
                    sender,
                    cancel: Arc::new(AtomicBool::new(false)),
                    turn: 1,
                    todos: json!([]),
                    token_usage: json!({
                        "uncachedInputTokens": 120,
                        "cacheReadTokens": 0,
                        "cacheWriteTokens": 0,
                        "outputTokens": 45
                    }),
                };
                append_row(&mut line, "turn/start", json!({ "turn": 1 }));
                append_row(
                    &mut line,
                    "user/message",
                    json!({
                        "turn": 1,
                        "id": format!("{fork_id}-u1"),
                        "content": [{ "type": "text", "text": "分支讨论已就绪（与主对话 Draft 隔离）" }]
                    }),
                );
                append_row(
                    &mut line,
                    "assistant/message",
                    json!({
                        "turn": 1,
                        "step": 1,
                        "sessionFinal": true,
                        "message": {
                            "id": format!("{fork_id}-a1"),
                            "content": [{ "type": "text", "text": "这里可以在不影响主会话运行与 Draft 的前提下做临时追问。" }]
                        }
                    }),
                );
                append_row(
                    &mut line,
                    "turn/end",
                    json!({ "turn": 1, "reason": "completed" }),
                );
                let summary = summary_value(&fork_id, &line);
                guard.sessions.insert(fork_id.clone(), line);
                let _ = guard.inventory_tx.send(json!({
                    "type": "upsert",
                    "session": summary,
                }));
                Ok(json!({ "sessionId": fork_id }))
            }
            "session.discard" => {
                let id = string(&payload, "sessionId")?.to_owned();
                let mut guard = self.fixture.inner.lock().expect("fixture lock");
                guard.sessions.remove(&id);
                let _ = guard.inventory_tx.send(json!({
                    "type": "remove",
                    "sessionId": id,
                }));
                Ok(json!({ "discarded": true }))
            }
            "session.rename" => {
                let id = string(&payload, "sessionId")?.to_owned();
                let title = string(&payload, "title")?.to_owned();
                let mut guard = self.fixture.inner.lock().expect("fixture lock");
                let summary = {
                    let line = guard
                        .sessions
                        .get_mut(&id)
                        .ok_or_else(|| unsupported("fixture session"))?;
                    line.meta.title = title;
                    summary_value(&id, line)
                };
                let _ = guard.inventory_tx.send(json!({
                    "type": "upsert",
                    "session": summary,
                }));
                Ok(json!({ "renamed": true }))
            }
            "session.archive" | "session.unarchive" => {
                let id = string(&payload, "sessionId")?.to_owned();
                let archive = method == "session.archive";
                let mut guard = self.fixture.inner.lock().expect("fixture lock");
                if archive {
                    if !guard.archived_ids.contains(&id) {
                        guard.archived_ids.push(id.clone());
                    }
                } else {
                    guard.archived_ids.retain(|x| x != &id);
                }
                let summary = guard.sessions.get_mut(&id).map(|line| {
                    line.meta.status = if archive { "archived" } else { "active" }.into();
                    summary_value(&id, line)
                });
                if let Some(summary) = summary {
                    let _ = guard.inventory_tx.send(json!({
                        "type": "upsert",
                        "session": summary,
                    }));
                }
                Ok(json!({ "archived": archive }))
            }
            "session.updateQueue" => {
                let session_id = string(&payload, "sessionId")?.to_owned();
                let item_id = string(&payload, "itemId")?.to_owned();
                let action_kind = payload["action"]["kind"].as_str().unwrap_or("remove");
                let mut guard = self.fixture.inner.lock().expect("fixture lock");
                let list = guard.queues.entry(session_id.clone()).or_default();
                if action_kind == "remove" {
                    list.retain(|item| item.id != item_id);
                } else if action_kind == "edit" {
                    let text = payload["action"]["content"][0]["text"]
                        .as_str()
                        .unwrap_or("")
                        .to_owned();
                    if let Some(item) = list.iter_mut().find(|item| item.id == item_id) {
                        item.text = text;
                    }
                }
                let items = FixtureSessionEndpoint::queue_json_for(&guard, &session_id);
                let _ = guard.control_tx.send(json!({
                    "type": "queue",
                    "state": {
                        "sessionId": session_id,
                        "items": items,
                    }
                }));
                Ok(json!({ "updated": true }))
            }
            "session.search" => {
                let query = payload["query"].as_str().unwrap_or("").to_lowercase();
                let guard = self.fixture.inner.lock().expect("fixture lock");
                let items: Vec<Value> = guard
                    .sessions
                    .iter()
                    .filter(|(id, line)| {
                        query.is_empty()
                            || id.to_lowercase().contains(&query)
                            || line.meta.title.to_lowercase().contains(&query)
                    })
                    .map(|(id, line)| {
                        json!({
                            "sessionId": id,
                            "title": line.meta.title,
                            "folder": line.meta.folder,
                        })
                    })
                    .collect();
                Ok(json!({ "items": items }))
            }
            "session.cancel" => {
                let id = string(&payload, "sessionId")?.to_owned();
                let guard = self.fixture.inner.lock().expect("fixture lock");
                let line = guard
                    .sessions
                    .get(&id)
                    .ok_or_else(|| unsupported("fixture session"))?;
                line.cancel.store(true, Ordering::SeqCst);
                Ok(json!({ "cancelled": true }))
            }
            "session.prompt" => {
                let id = string(&payload, "sessionId")?.to_owned();
                let content = payload["content"].clone();
                let (turn, cancel, tx) = {
                    let mut guard = self.fixture.inner.lock().expect("fixture lock");
                    let line = guard
                        .sessions
                        .get_mut(&id)
                        .ok_or_else(|| unsupported("fixture session"))?;
                    require(!line.meta.running, "fixture session is busy")?;
                    line.meta.running = true;
                    line.turn += 1;
                    line.cancel = Arc::new(AtomicBool::new(false));
                    let turn = line.turn;
                    append_row(line, "turn/start", json!({ "turn": turn }));
                    append_row(
                        line,
                        "user/message",
                        json!({
                            "turn": turn,
                            "id": rpc_id,
                            "source": { "kind": "user", "rpcId": rpc_id },
                            "content": content
                        }),
                    );
                    let cancel = Arc::clone(&line.cancel);
                    let tx = line.sender.clone();
                    let _ = guard.inventory_tx.send(json!({
                        "type": "status",
                        "sessionId": id,
                        "running": true,
                    }));
                    (turn, cancel, tx)
                };

                let endpoint = self.fixture.clone();
                let chunk_delay = self.fixture.chunk_delay;
                tokio::spawn(async move {
                    let mut text = String::new();
                    for chunk in [
                        "已收到请求，",
                        "正在通过 SessionEndpoint ",
                        "流式推送中文与代码响应。\n\n```rust\nprintln!(\"WindowWake OK\");\n```",
                    ] {
                        if !chunk_delay.is_zero() {
                            tokio::time::sleep(chunk_delay).await;
                        }
                        if cancel.load(Ordering::SeqCst) {
                            let mut guard = endpoint.inner.lock().expect("fixture lock");
                            if let Some(line) = guard.sessions.get_mut(&id) {
                                line.meta.running = false;
                                append_row(
                                    line,
                                    "turn/end",
                                    json!({ "turn": turn, "reason": "aborted" }),
                                );
                            }
                            let _ = guard.inventory_tx.send(json!({
                                "type": "status",
                                "sessionId": id,
                                "running": false,
                            }));
                            return;
                        }
                        text.push_str(chunk);
                        let _ = tx.send(json!({
                            "type": "transient",
                            "event": {
                                "type": "assistant/chunk",
                                "seq": 0,
                                "time": 1_760_000_500.0,
                                "data": {
                                    "turn": turn,
                                    "step": 1,
                                    "chunk": { "type": "text-delta", "text": chunk }
                                }
                            }
                        }));
                    }
                    let mut guard = endpoint.inner.lock().expect("fixture lock");
                    if let Some(line) = guard.sessions.get_mut(&id) {
                        line.meta.running = false;
                        line.meta.updated_at += 1.0;
                        append_row(
                            line,
                            "assistant/message",
                            json!({
                                "turn": turn,
                                "step": 1,
                                "sessionFinal": true,
                                "message": {
                                    "id": format!("{id}-m-{turn}"),
                                    "content": [{ "type": "text", "text": text }],
                                    "usage": { "inputTokens": 42, "outputTokens": 38 }
                                }
                            }),
                        );
                        append_row(
                            line,
                            "turn/end",
                            json!({ "turn": turn, "reason": "completed" }),
                        );
                    }
                    let _ = guard.inventory_tx.send(json!({
                        "type": "status",
                        "sessionId": id,
                        "running": false,
                    }));
                });

                Ok(json!({ "accepted": true }))
            }
            "settings.describe" => {
                let guard = self.fixture.inner.lock().expect("fixture lock");
                Ok(json!({
                    "revision": guard.settings_revision,
                    "readOnly": guard.settings_read_only,
                    "namespaces": [{
                        "id": "execution",
                        "label": "Agent 运行策略 (Harness)",
                        "revision": guard.settings_revision,
                        "readOnly": guard.settings_read_only,
                        "fields": [
                            {
                                "key": "approvalPolicy",
                                "label": "工具审批策略",
                                "kind": "enum",
                                "options": ["ask", "auto-read", "strict"],
                                "value": guard.settings_values["execution"]["approvalPolicy"],
                                "secret": false
                            },
                            {
                                "key": "sandboxMode",
                                "label": "工作区沙盒模式",
                                "kind": "enum",
                                "options": ["workspace-write", "read-only"],
                                "value": guard.settings_values["execution"]["sandboxMode"],
                                "secret": false
                            },
                            {
                                "key": "maxTurns",
                                "label": "单轮最大步数",
                                "kind": "integer",
                                "value": guard.settings_values["execution"]["maxTurns"],
                                "secret": false
                            },
                            {
                                "key": "apiToken",
                                "label": "服务端凭证",
                                "kind": "secret",
                                "value": "••••••••",
                                "secret": true
                            }
                        ]
                    }]
                }))
            }
            "settings.mutate" => {
                let mut guard = self.fixture.inner.lock().expect("fixture lock");
                require(!guard.settings_read_only, "fixture settings are read-only")?;
                let expected = payload["expectedRevision"]
                    .as_u64()
                    .ok_or_else(|| Error::Protocol("missing expectedRevision".into()))?;
                if expected != guard.settings_revision {
                    return Err(Error::Remote(json!({
                        "code": "conflict",
                        "message": "CAS revision mismatch",
                        "currentRevision": guard.settings_revision
                    })));
                }
                let ns = payload["namespace"]
                    .as_str()
                    .unwrap_or("execution")
                    .to_owned();
                if let Some(patch) = payload["values"].as_object() {
                    let target = guard.settings_values.entry(ns).or_insert_with(|| json!({}));
                    for (k, v) in patch {
                        target[k] = v.clone();
                    }
                }
                guard.settings_revision += 1;
                Ok(json!({
                    "accepted": true,
                    "revision": guard.settings_revision
                }))
            }
            _ => Err(unsupported("fixture rpc method")),
        }
    }

    async fn close(&self) {}
}
