//! Renderer-independent multi-endpoint search projection over the original
//! aggregate inventory. Search publications remain endpoint-owned.
use crate::{SidebarMultiPublication, SidebarPublication};
use std::collections::{BTreeMap, BTreeSet};
use tekes_agent_components_ui::SidebarInput;
use tekes_agent_contract::SessionAddress;
use tekes_agent_presentation::SidebarSearchPublication;
pub use tekes_agent_presentation::{
    SidebarMultiSearchChild, SidebarMultiSearchController as SidebarMultiSearchBinding,
    SidebarMultiSearchInput, SidebarMultiSearchIntent, SidebarMultiSearchKey,
    SidebarMultiSearchPublication,
};

/// One child keeps its original single-endpoint state and aggregate query key.
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarMultiSearchChildProjection {
    pub aggregate_key: Option<SidebarMultiSearchKey>,
    pub publication: SidebarSearchPublication,
}

/// Display projection of endpoint-owned search results joined to the original
/// fixed-set inventory. `input.search_help` stays empty because one
/// `SidebarSearchInput` cannot represent the independent child states.
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarMultiSearchProjection {
    pub input: SidebarInput,
    pub aggregate_input: SidebarMultiSearchInput,
    pub query: String,
    pub children: BTreeMap<String, SidebarMultiSearchChildProjection>,
    pub snippets: BTreeMap<SessionAddress, String>,
    pub unresolved: Vec<SessionAddress>,
}

impl SidebarMultiPublication {
    /// Preserve global row order, selection, folder filter, load and error state
    /// while applying each endpoint's original single-search projection.
    pub fn project_multi_search_for_display(
        &self,
        search: &SidebarMultiSearchPublication,
        display: &SidebarInput,
    ) -> Result<SidebarMultiSearchProjection, &'static str> {
        if self.revision == 0 || display.revision == 0 {
            return Err("invalid multi-search display source");
        }
        if let Some(key) = &display.folder_filter {
            key.validate()
                .map_err(|_| "invalid Sidebar folder filter")?;
        }
        validate_multi_search(search, &self.endpoints)?;

        let endpoint_ids: BTreeSet<_> = self.endpoints.keys().cloned().collect();
        let mut permitted = BTreeSet::new();
        let mut snippets = BTreeMap::new();
        let mut unresolved = Vec::new();
        let mut children = BTreeMap::new();

        for child in &search.children {
            let endpoint_id = child.endpoint_id.as_str();
            let inventory = SidebarPublication {
                revision: self.revision,
                rows: self
                    .rows
                    .iter()
                    .filter(|row| row.address.endpoint_id == endpoint_id)
                    .cloned()
                    .collect(),
                activity_at: self
                    .activity_at
                    .iter()
                    .filter(|(address, _)| address.endpoint_id == endpoint_id)
                    .map(|(address, activity)| (address.clone(), *activity))
                    .collect(),
                loaded: self.endpoints[endpoint_id].loaded,
                error: self.endpoints[endpoint_id].error.clone(),
            };
            let projection =
                inventory.project_search_for_display(endpoint_id, &child.publication, display)?;
            permitted.extend(projection.input.rows.iter().map(|row| row.address.clone()));
            snippets.extend(projection.snippets);
            unresolved.extend(projection.unresolved);
            children.insert(
                child.endpoint_id.clone(),
                SidebarMultiSearchChildProjection {
                    aggregate_key: search.input.key.clone(),
                    publication: child.publication.clone(),
                },
            );
        }

        // The global inventory is authoritative for sorting and full address
        // identity. Endpoint projections only decide membership and snippets.
        let mut input = self.input(display.selected.clone());
        input.revision = display.revision;
        input.folder_filter = display.folder_filter.clone();
        input.rows.retain(|row| permitted.contains(&row.address));
        input.search_help = None;

        // Guard against malformed aggregate inventory rows/activity omitted
        // from their endpoint partition rather than silently reordering them.
        if endpoint_ids.is_empty()
            || self.rows.iter().any(|row| {
                !endpoint_ids.contains(&row.address.endpoint_id)
                    || !self.activity_at.contains_key(&row.address)
            })
        {
            return Err("invalid multi-endpoint Sidebar inventory");
        }

        Ok(SidebarMultiSearchProjection {
            input,
            aggregate_input: search.input.clone(),
            query: search.query.clone(),
            children,
            snippets,
            unresolved,
        })
    }
}

fn validate_multi_search(
    search: &SidebarMultiSearchPublication,
    endpoints: &BTreeMap<String, crate::SidebarEndpointStatus>,
) -> Result<(), &'static str> {
    search
        .validate()
        .map_err(|_| "invalid multi-search publication")?;
    let expected: BTreeSet<_> = endpoints.keys().cloned().collect();
    if expected.is_empty() {
        return Err("multi-search inventory has no endpoints");
    }
    let mut actual = BTreeSet::new();
    for child in &search.children {
        actual.insert(child.endpoint_id.clone());
    }
    if actual != expected {
        return Err("multi-search endpoints differ from fixed inventory set");
    }
    Ok(())
}
