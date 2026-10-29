//! Read-only actionable facts and exact shared response shapes. No transport or UI state.
use crate::RunScope;
use serde_json::{Value, json};
use tekes_agent_contract::{Result, integer, require, string};
#[derive(Clone, Debug, PartialEq)]
pub struct ActionableChoice {
    pub id: String,
    pub label: String,
    outcome: Value,
}
impl ActionableChoice {
    pub fn outcome(&self) -> &Value {
        &self.outcome
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ActionablePrompt {
    scope: RunScope,
    source: Value,
    id: String,
    revision: i64,
    pub title: String,
    pub detail: String,
    pub choices: Vec<ActionableChoice>,
}
impl ActionablePrompt {
    pub fn read(scope: RunScope, source: Value) -> Result<Self> {
        scope.validate()?;
        let id = string(&source, "id")?.to_owned();
        let revision = integer(&source, "revision")?;
        require(
            !id.is_empty() && revision >= 0 && source["sessionId"] == scope.session_id,
            "invalid actionable scope or identity",
        )?;
        let kind = string(&source, "kind")?;
        let payload = &source["payload"];
        require(
            payload
                .get("sessionId")
                .is_none_or(|id| id == &scope.session_id),
            "actionable payload scope mismatch",
        )?;
        let mut title = kind.to_owned();
        let mut detail = String::new();
        let mut choices = vec![];
        match kind {
            "approval" => {
                if let (Some(approval), Some(tool)) = (
                    payload["approvalId"].as_str().filter(|s| !s.is_empty()),
                    payload["toolName"]
                        .as_str()
                        .filter(|s| !s.trim().is_empty()),
                ) {
                    title = tool.into();
                    detail = payload["reason"].as_str().unwrap_or("").into();
                    for (id, label, outcome) in [
                        ("deny", "拒绝", "rejected"),
                        ("allow", "仅允许本次", "allowed-once"),
                    ] {
                        choices.push(ActionableChoice{id:id.into(),label:label.into(),outcome:json!({"sessionId":scope.session_id,"approvalId":approval,"outcome":outcome})});
                    }
                }
            }
            "question" => {
                if let Some(rows) = payload["questions"]
                    .as_array()
                    .filter(|rows| rows.len() == 1)
                {
                    let question = &rows[0];
                    title = question["question"].as_str().unwrap_or(kind).into();
                    detail = question["detail"].as_str().unwrap_or("").into();
                    if ["multiSelect", "isSecret"].iter().all(|field| {
                        question
                            .get(*field)
                            .is_none_or(|flag| flag.is_null() || flag == false)
                    }) && question["question"]
                        .as_str()
                        .is_some_and(|text| !text.trim().is_empty())
                        && let (Some(qid), Some(options)) = (
                            question["id"].as_str().filter(|s| !s.is_empty()),
                            question["options"].as_array(),
                        )
                    {
                        let labels: Option<Vec<_>> = options
                            .iter()
                            .map(|option| option["label"].as_str().filter(|s| !s.trim().is_empty()))
                            .collect();
                        if let Some(labels) = labels
                            && labels
                                .iter()
                                .collect::<std::collections::BTreeSet<_>>()
                                .len()
                                == labels.len()
                        {
                            for (index, label) in labels.into_iter().enumerate() {
                                choices.push(ActionableChoice{id:format!("option-{index}"),label:label.into(),outcome:json!({"sessionId":scope.session_id,"answer":{"answers":[{"id":qid,"selected":[label]}]}})});
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(Self {
            scope,
            source,
            id,
            revision,
            title,
            detail,
            choices,
        })
    }
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    pub fn source(&self) -> &Value {
        &self.source
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn revision(&self) -> i64 {
        self.revision
    }
    pub fn choice(&self, id: &str) -> Option<&ActionableChoice> {
        self.choices.iter().find(|choice| choice.id == id)
    }
}
