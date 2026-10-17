use crate::{AgentTask, RunScope, TranscriptSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use tekes_agent_contract::{Result, require};
use tekes_agent_session::SessionProjection;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SummaryArtifact {
    pub id: String,
    pub source: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SummarySubagent {
    pub id: String,
    pub title: String,
    pub session_id: Option<String>,
    pub status: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SummaryUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: Option<f64>,
    pub is_cost_complete: bool,
}
impl SummaryUsage {
    pub fn total_tokens(&self) -> u64 {
        [
            self.input_tokens,
            self.cached_input_tokens,
            self.cache_creation_input_tokens,
            self.output_tokens,
        ]
        .into_iter()
        .fold(0, u64::saturating_add)
    }
    pub fn cache_hit_rate(&self) -> Option<f64> {
        let denominator = self.input_tokens as f64
            + self.cached_input_tokens as f64
            + self.cache_creation_input_tokens as f64;
        (denominator > 0.0).then(|| self.cached_input_tokens as f64 / denominator)
    }
    pub fn known_cost_usd(&self) -> Option<f64> {
        self.cost_usd
            .filter(|v| self.is_cost_complete && v.is_finite() && *v >= 0.0)
    }
}
/// Normalized authoritative facts may also be supplied by an explicit host projection. Artifacts
/// are newest first. Producer task names are retained; result briefs are never task titles.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SummaryFacts {
    pub scope: RunScope,
    pub tasks: Vec<AgentTask>,
    pub subagents: Vec<SummarySubagent>,
    pub artifacts: Vec<SummaryArtifact>,
    pub usage: Option<SummaryUsage>,
}
impl SummaryFacts {
    pub fn read(
        scope: &RunScope,
        session: &SessionProjection,
        transcript: &TranscriptSnapshot,
    ) -> Result<Self> {
        let mut facts = Self {
            scope: scope.clone(),
            tasks: vec![],
            subagents: vec![],
            artifacts: vec![],
            usage: None,
        };
        // A typed projection survives truncated transcript windows. Only events newer than its
        // cut can replace it; null explicitly clears membership, including a previous page.
        let projection = session.projections.get("todos");
        let mut todos = projection.map(|v| v.value.clone());
        let cut = projection.map(|v| v.sequence).unwrap_or(-1);
        for entry in session
            .entries
            .values()
            .filter(|e| e.event.seq > cut && e.event.kind == "todo/write")
        {
            todos = Some(entry.event.data["todos"].clone());
        }
        if let Some(todos) = todos.filter(|v| !v.is_null()) {
            let rows = todos.as_array().ok_or_else(|| {
                tekes_agent_contract::Error::Protocol("invalid todos projection".into())
            })?;
            for (index, row) in rows.iter().enumerate() {
                let name = row["content"].as_str().ok_or_else(|| {
                    tekes_agent_contract::Error::Protocol("invalid todo content".into())
                })?;
                let status = row["status"].as_str().ok_or_else(|| {
                    tekes_agent_contract::Error::Protocol("invalid todo status".into())
                })?;
                facts.tasks.push(AgentTask {
                    id: format!("dsh-todo-{index}"),
                    scope: scope.clone(),
                    name: name.into(),
                    status: status.into(),
                    kind: "todo".into(),
                });
            }
        }
        if let Some(value) = session
            .projections
            .get("tokenUsage")
            .map(|v| &v.value)
            .filter(|v| !v.is_null())
        {
            let count = |key: &str| {
                value[key].as_u64().ok_or_else(|| {
                    tekes_agent_contract::Error::Protocol(format!("invalid tokenUsage.{key}"))
                })
            };
            facts.usage = Some(SummaryUsage {
                input_tokens: count("uncachedInputTokens")?,
                cached_input_tokens: count("cacheReadTokens")?,
                cache_creation_input_tokens: count("cacheWriteTokens")?,
                output_tokens: count("outputTokens")?,
                cost_usd: None,
                is_cost_complete: false,
            });
        }
        if let Some(usage) = &mut facts.usage {
            usage.is_cost_complete = usage.total_tokens() == 0;
        }
        for tool in &transcript.tools {
            if tool.name == "task"
                && let Some(name) = tool.arguments["task_name"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
            {
                // A tool call alone grants no independently navigable session pointer.
                facts.tasks.push(AgentTask {
                    id: tool.id.clone(),
                    scope: scope.clone(),
                    name: name.into(),
                    status: tool.phase.clone(),
                    kind: "task".into(),
                });
            }
            if matches!(tool.name.as_str(), "subagent" | "subagents") {
                let title = ["brief", "description"]
                    .into_iter()
                    .find_map(|key| {
                        tool.arguments[key]
                            .as_str()
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                    })
                    .unwrap_or("Subagent");
                facts.subagents.push(SummarySubagent {
                    id: tool.id.clone(),
                    title: title.into(),
                    session_id: tool.result.as_ref().and_then(subagent_result_id),
                    status: tool.phase.clone(),
                });
            }
        }
        Ok(facts)
    }
    pub fn sections(&self, current: &RunScope) -> Vec<SummarySection> {
        if &self.scope != current {
            return vec![];
        }
        let mut sections = vec![];
        let tasks = self
            .tasks
            .iter()
            .filter(|t| {
                t.scope.endpoint_id == current.endpoint_id && t.scope.thread_id == current.thread_id
            })
            .map(|task| SummaryRow {
                id: task.id.clone(),
                title: task.name.clone(),
                value: Value::String(task.canonical_status().into()),
                action: task.independent_run().map(|scope| SummaryAction::OpenTask {
                    task_id: task.id.clone(),
                    scope,
                }),
            })
            .collect();
        sections.push(SummarySection {
            id: "tasks".into(),
            rows: tasks,
        });
        sections.push(SummarySection {
            id: "subagents".into(),
            rows: self
                .subagents
                .iter()
                .map(|s| SummaryRow {
                    id: s.id.clone(),
                    title: s.title.clone(),
                    value: Value::String(s.status.clone()),
                    action: s
                        .session_id
                        .as_ref()
                        .map(|child| SummaryAction::OpenSubagent {
                            parent: current.clone(),
                            child_id: child.clone(),
                        }),
                })
                .collect(),
        });
        let sources: BTreeSet<_> = self
            .artifacts
            .iter()
            .filter_map(|a| a.source.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        sections.push(SummarySection {
            id: "sources".into(),
            rows: sources
                .into_iter()
                .map(|s| SummaryRow {
                    id: s.into(),
                    title: s.into(),
                    value: Value::Null,
                    action: None,
                })
                .collect(),
        });
        let mut seen = BTreeSet::new();
        sections.push(SummarySection {
            id: "artifacts".into(),
            rows: self
                .artifacts
                .iter()
                .filter(|a| seen.insert(&a.id))
                .map(|a| SummaryRow {
                    id: a.id.clone(),
                    title: a.id.rsplit('/').next().unwrap_or(&a.id).into(),
                    value: Value::Null,
                    action: Some(SummaryAction::OpenArtifact {
                        scope: current.clone(),
                        path: a.id.clone(),
                    }),
                })
                .collect(),
        });
        if let Some(usage) = &self.usage {
            let rows = [
                ("total", serde_json::json!(usage.total_tokens())),
                ("input", serde_json::json!(usage.input_tokens)),
                ("output", serde_json::json!(usage.output_tokens)),
                ("cache", serde_json::json!(usage.cached_input_tokens)),
                ("hit-rate", serde_json::json!(usage.cache_hit_rate())),
                ("cost", serde_json::json!(usage.known_cost_usd())),
            ]
            .into_iter()
            .map(|(id, value)| SummaryRow {
                id: id.into(),
                title: id.into(),
                value,
                action: None,
            })
            .collect();
            sections.push(SummarySection {
                id: "usage".into(),
                rows,
            });
        }
        sections.retain(|s| !s.rows.is_empty());
        sections
    }
    pub fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        require(
            self.tasks.iter().all(|t| {
                t.scope.endpoint_id == self.scope.endpoint_id
                    && t.scope.thread_id == self.scope.thread_id
            }),
            "foreign task scope",
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SummaryAction {
    OpenTask { task_id: String, scope: RunScope },
    OpenSubagent { parent: RunScope, child_id: String },
    OpenArtifact { scope: RunScope, path: String },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SummaryRow {
    pub id: String,
    pub title: String,
    pub value: Value,
    pub action: Option<SummaryAction>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SummarySection {
    pub id: String,
    pub rows: Vec<SummaryRow>,
}
/// Host sections (for example Git) are explicit additions in caller order, with empty sections omitted.
pub fn append_summary_sections(
    sections: &mut Vec<SummarySection>,
    host: impl IntoIterator<Item = SummarySection>,
) {
    sections.extend(host.into_iter().filter(|s| !s.rows.is_empty()));
}

// ClientKit resolves the first native startup UUID in the tool result. Keep it raw here: endpoint
// identity is a separate scope, never a synthetic storage prefix.
fn subagent_result_id(value: &Value) -> Option<String> {
    fn texts(value: &Value, out: &mut String) {
        match value {
            Value::Array(values) => {
                for value in values {
                    texts(value, out);
                }
            }
            Value::Object(values) => {
                if let Some(text) = values.get("text").and_then(Value::as_str) {
                    out.push_str(text);
                    out.push(' ');
                }
                if let Some(content) = values.get("content") {
                    texts(content, out);
                }
            }
            Value::String(text) => {
                out.push_str(text);
                out.push(' ');
            }
            _ => (),
        }
    }
    let mut text = String::new();
    texts(value, &mut text);
    text.as_bytes().windows(36).find_map(|bytes| {
        std::str::from_utf8(bytes)
            .ok()
            .filter(|s| uuid::Uuid::parse_str(s).is_ok())
            .map(str::to_owned)
    })
}
/// Async host query state, scoped and generation-fenced like TaskPanelState.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SummaryLoadToken {
    scope: RunScope,
    generation: u64,
}
#[derive(Clone, Debug, Default)]
pub struct SummaryState {
    current: Option<SummaryLoadToken>,
    generation: u64,
    pub facts: Option<SummaryFacts>,
    pub error: Option<String>,
}
impl SummaryState {
    pub fn begin(&mut self, scope: RunScope) -> SummaryLoadToken {
        self.generation += 1;
        let token = SummaryLoadToken {
            scope,
            generation: self.generation,
        };
        self.current = Some(token.clone());
        self.facts = None;
        self.error = None;
        token
    }
    pub fn commit(
        &mut self,
        token: &SummaryLoadToken,
        result: std::result::Result<SummaryFacts, String>,
    ) -> bool {
        if self.current.as_ref() != Some(token) {
            return false;
        }
        match result {
            Ok(facts) if facts.scope == token.scope && facts.validate().is_ok() => {
                self.facts = Some(facts);
                self.error = None;
            }
            Ok(_) => return false,
            Err(error) => {
                self.facts = None;
                self.error = Some(error);
            }
        }
        true
    }
    pub fn sections(&self, scope: &RunScope) -> Vec<SummarySection> {
        self.facts
            .as_ref()
            .map(|f| f.sections(scope))
            .unwrap_or_default()
    }
}
