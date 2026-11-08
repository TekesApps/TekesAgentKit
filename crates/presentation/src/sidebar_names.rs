//! Retained canonical session names used as an input to Sidebar search.
//! This snapshot deliberately contains no display membership or transcript data.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use tekes_agent_contract::{Result, SessionAddress, require};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarNameDocument {
    pub address: SessionAddress,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarNameCorpus {
    pub endpoint_id: String,
    pub connection_epoch: u64,
    pub inventory_ready: bool,
    pub endpoint_domain: Vec<String>,
    pub documents: Vec<SidebarNameDocument>,
}

impl SidebarNameCorpus {
    /// Checks the corpus shape and its fixed endpoint domain without asserting authority.
    pub fn validate(&self) -> Result<()> {
        require(
            !self.endpoint_id.is_empty(),
            "empty Sidebar name corpus endpoint",
        )?;
        require(
            !self.endpoint_domain.is_empty(),
            "empty Sidebar name corpus endpoint domain",
        )?;
        let mut endpoints = BTreeSet::new();
        for endpoint in &self.endpoint_domain {
            require(
                !endpoint.is_empty(),
                "empty Sidebar name corpus domain entry",
            )?;
            require(
                endpoints.insert(endpoint),
                "duplicate Sidebar name corpus domain entry",
            )?;
        }
        require(
            endpoints.contains(&self.endpoint_id),
            "Sidebar name corpus domain omits its endpoint",
        )?;

        let mut addresses = BTreeSet::new();
        for document in &self.documents {
            require(
                !document.address.endpoint_id.is_empty() && !document.address.session_id.is_empty(),
                "invalid Sidebar name document address",
            )?;
            require(
                document.address.endpoint_id == self.endpoint_id,
                "Sidebar name document belongs to another endpoint",
            )?;
            require(
                addresses.insert(&document.address),
                "duplicate Sidebar name document address",
            )?;
        }
        Ok(())
    }
}

/// Returns the canonical row title, including authoritative projected clears.
pub fn canonical_session_title(summary: &Value, projected_title: Option<&Value>) -> String {
    let projected = projected_title.or_else(|| summary["projections"]["values"].get("title"));
    match projected {
        Some(value) => trimmed_title(value).unwrap_or("New conversation"),
        None => trimmed_title(&summary["title"])
            .or_else(|| trimmed_title(&summary["name"]))
            .unwrap_or("New conversation"),
    }
    .to_owned()
}

fn trimmed_title(value: &Value) -> Option<&str> {
    value
        .as_str()
        .map(str::trim)
        .filter(|title| !title.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn title_projection_clear_and_legacy_fallback_match_sidebar_rows() {
        assert_eq!(
            canonical_session_title(&json!({"title":"old","name":"legacy"}), Some(&Value::Null)),
            "New conversation"
        );
        assert_eq!(
            canonical_session_title(&json!({"title":"old"}), Some(&json!("  new  "))),
            "new"
        );
        assert_eq!(
            canonical_session_title(&json!({"title":"  " ,"name":" legacy "}), None),
            "legacy"
        );
        assert_eq!(
            canonical_session_title(&json!({}), None),
            "New conversation"
        );
    }

    #[test]
    fn corpus_validates_domain_addresses_and_duplicates() {
        let mut corpus = SidebarNameCorpus {
            endpoint_id: "one".into(),
            connection_epoch: 0,
            inventory_ready: false,
            endpoint_domain: vec!["one".into(), "two".into()],
            documents: vec![SidebarNameDocument {
                address: SessionAddress {
                    endpoint_id: "one".into(),
                    session_id: "s".into(),
                },
                title: "title".into(),
            }],
        };
        corpus.validate().unwrap();
        corpus.endpoint_domain.push("two".into());
        assert!(corpus.validate().is_err());
        corpus.endpoint_domain.pop();
        corpus.documents.push(corpus.documents[0].clone());
        assert!(corpus.validate().is_err());
        corpus.documents.pop();
        corpus.documents[0].address.endpoint_id = "two".into();
        assert!(corpus.validate().is_err());

        corpus.documents[0].address.endpoint_id = "one".into();
        corpus.endpoint_domain = vec!["two".into()];
        assert!(corpus.validate().is_err());
        corpus.endpoint_domain.clear();
        assert!(corpus.validate().is_err());
    }

    #[test]
    fn corpus_wire_fields_are_camel_case_and_epoch_zero_is_valid() {
        let corpus = SidebarNameCorpus {
            endpoint_id: "one".into(),
            connection_epoch: 0,
            inventory_ready: false,
            endpoint_domain: vec!["one".into()],
            documents: vec![],
        };
        corpus.validate().unwrap();
        let value = serde_json::to_value(corpus).unwrap();
        assert!(value.get("endpointId").is_some());
        assert!(value.get("connectionEpoch").is_some());
        assert!(value.get("inventoryReady").is_some());
        assert!(value.get("endpointDomain").is_some());
    }
}
