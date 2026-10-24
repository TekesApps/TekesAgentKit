//! Read-only parser for public Task/child-family seed manifests.
//!
//! These values are expected identities for later comparison with the host's
//! actual public projections. Parsing them never grants binding or control
//! authority and does not establish harness or UI acceptance.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use tekes_agent_presentation::{AgentTask, RunScope};

const ENDPOINTS: [&str; 3] = ["native", "pi", "dsh"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FamilySeedCase {
    pub endpoint_id: String,
    pub parent: RunScope,
    pub child_id: String,
    pub task: Option<AgentTask>,
    pub summary_subagent_id: Option<String>,
    pub catalog_mode: Option<String>,
}

/// Parse exactly two conservative seed manifests for each supported endpoint.
///
/// Unknown DTO fields are intentionally ignored for forward compatibility.
/// All identities are parsed from the supplied JSON only; this function does
/// not perform file or harness I/O.
pub fn parse_family_seeds(value: &Value) -> Result<Vec<FamilySeedCase>, String> {
    let rows = value
        .as_array()
        .ok_or_else(|| "family seed manifests must be a JSON array".to_owned())?;
    if rows.len() != 6 {
        return Err("exactly six family seed manifests are required".to_owned());
    }

    let mut counts = BTreeMap::<String, usize>::new();
    let mut parents = BTreeSet::<(String, String, String)>::new();
    let mut children = BTreeSet::<(String, String)>::new();
    let mut parsed = Vec::with_capacity(rows.len());

    for (index, row) in rows.iter().enumerate() {
        let endpoint_id = required_text(row, "endpoint")?;
        if !ENDPOINTS.contains(&endpoint_id.as_str()) {
            return Err(format!("manifest {index} has unsupported endpoint"));
        }
        *counts.entry(endpoint_id.clone()).or_default() += 1;

        if integer_field(row, "schemaVersion")? != 1
            || text_field(row, "evidenceScope")? != "paired-public-seed-dtos"
            || bool_field(row, "realHarnessAcceptance")? != Some(false)
            || bool_field(row, "uiAcceptance")? != Some(false)
            || bool_field(row, "pairedPublicSourceMatched")? != Some(true)
        {
            return Err(format!(
                "manifest {index} has unsupported or elevated evidence scope"
            ));
        }

        let parent_value = field(row, "parent")?;
        let parent = serde_json::from_value::<RunScope>(field(parent_value, "scope")?.clone())
            .map_err(|_| format!("manifest {index} has an invalid parent scope"))?;
        if parent.endpoint_id != endpoint_id
            || parent.thread_id.trim().is_empty()
            || parent.session_id.trim().is_empty()
            || parent.thread_id != parent.session_id
        {
            return Err(format!(
                "manifest {index} parent scope conflicts with its endpoint"
            ));
        }
        let parent_key = (
            endpoint_id.clone(),
            parent.thread_id.clone(),
            parent.session_id.clone(),
        );
        if !parents.insert(parent_key) {
            return Err(format!(
                "manifest {index} repeats an endpoint-qualified parent"
            ));
        }

        let child = field(row, "child")?;
        let address = field(child, "address")?;
        let child_endpoint = required_text(address, "endpointId")?;
        let child_id = required_text(address, "sessionId")?;
        if child_endpoint != endpoint_id || child_id == parent.session_id {
            return Err(format!(
                "manifest {index} child address conflicts with its parent"
            ));
        }
        if !children.insert((endpoint_id.clone(), child_id.clone())) {
            return Err(format!(
                "manifest {index} repeats an endpoint-qualified child"
            ));
        }

        let child_parent = serde_json::from_value::<RunScope>(field(child, "parentScope")?.clone())
            .map_err(|_| format!("manifest {index} has an invalid child parent scope"))?;
        if child_parent != parent {
            return Err(format!(
                "manifest {index} child belongs to a different parent"
            ));
        }

        let catalog = field(child, "catalogEntry")?;
        if required_text(catalog, "id")? != child_id || required_text(catalog, "kind")? != "child" {
            return Err(format!(
                "manifest {index} catalog entry conflicts with child identity"
            ));
        }
        if bool_field(child, "controlAuthorityGrantedByManifest")? != Some(false) {
            return Err(format!("manifest {index} claims child control authority"));
        }

        let task_value = field(row, "task")?;
        let task = if task_value.is_null() {
            None
        } else {
            let parsed_task =
                serde_json::from_value::<AgentTask>(field(task_value, "row")?.clone())
                    .map_err(|_| format!("manifest {index} has an invalid task DTO"))?;
            if endpoint_id != "native"
                || parsed_task.kind != "task"
                || parsed_task.scope != parent
                || parsed_task.id.trim().is_empty()
            {
                return Err(format!(
                    "manifest {index} task is outside its Native parent scope"
                ));
            }
            Some(parsed_task)
        };
        if endpoint_id == "native" && task.is_none() {
            return Err(format!(
                "manifest {index} Native seed is missing its parent Task"
            ));
        }
        if endpoint_id != "native" && !task_value.is_null() {
            return Err(format!(
                "manifest {index} may not infer a Task for this endpoint"
            ));
        }

        let summary_subagent_id = match child.get("summaryRow") {
            None => None,
            Some(summary) => {
                let id = required_text(summary, "id")?;
                if endpoint_id != "pi" || required_text(summary, "sessionId")? != child_id {
                    return Err(format!(
                        "manifest {index} summary row conflicts with child identity"
                    ));
                }
                Some(id)
            }
        };
        if endpoint_id == "pi" && summary_subagent_id.is_none() {
            return Err(format!(
                "manifest {index} Pi seed is missing its summary subagent row"
            ));
        }

        let catalog_mode = match catalog.get("mode") {
            None | Some(Value::Null) => None,
            Some(Value::String(mode)) if !mode.trim().is_empty() => Some(mode.clone()),
            _ => return Err(format!("manifest {index} has an invalid catalog mode")),
        };

        if endpoint_id == "native"
            && (bool_field(child, "readOnly")? != Some(true)
                || bool_field(child, "independentRun")? != Some(false))
        {
            return Err(format!(
                "manifest {index} Native bounded child semantics conflict"
            ));
        }

        parsed.push(FamilySeedCase {
            endpoint_id,
            parent,
            child_id,
            task,
            summary_subagent_id,
            catalog_mode,
        });
    }

    for endpoint in ENDPOINTS {
        if counts.get(endpoint) != Some(&2) {
            return Err(format!(
                "exactly two {endpoint} seed manifests are required"
            ));
        }
    }
    Ok(parsed)
}

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value, String> {
    value
        .as_object()
        .and_then(|object| object.get(key))
        .ok_or_else(|| format!("missing {key}"))
}

fn required_text(value: &Value, key: &str) -> Result<String, String> {
    let text = field(value, key)?
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| format!("{key} must be non-empty text"))?;
    Ok(text.to_owned())
}

fn text_field(value: &Value, key: &str) -> Result<String, String> {
    field(value, key)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{key} must be text"))
}

fn integer_field(value: &Value, key: &str) -> Result<u64, String> {
    field(value, key)?
        .as_u64()
        .ok_or_else(|| format!("{key} must be an unsigned integer"))
}

fn bool_field(value: &Value, key: &str) -> Result<Option<bool>, String> {
    Ok(field(value, key)?.as_bool())
}
