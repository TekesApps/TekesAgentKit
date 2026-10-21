//! Thin public binding name for the shared, renderer-independent search reader.
pub use tekes_agent_presentation::{
    SidebarSearchController as SidebarSearchBinding, SidebarSearchMatch, SidebarSearchMessages,
    SidebarSearchPublication,
};

use crate::SidebarPublication;
use std::collections::{BTreeMap, BTreeSet};
use tekes_agent_components_ui::{SidebarInput, SidebarSearchPhase};
use tekes_agent_contract::SessionAddress;
pub use tekes_agent_contract::{SessionSearchHit, SessionSearchResults};

/// Display projection of provider search hits joined to the original inventory.
/// Inventory order remains authoritative (including the host's sort preference).
/// This does not implement local Mirror FTS or manufacture rows for unknown IDs.
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarSearchProjection {
    pub input: SidebarInput,
    /// Query-scoped row help; consume with an original library tooltip when the
    /// fixed SDK provides that declaration, rather than adding a subtitle row.
    pub snippets: BTreeMap<SessionAddress, String>,
    pub unresolved: Vec<SessionAddress>,
}
impl SidebarPublication {
    pub fn project_search(
        &self,
        endpoint: &str,
        search: &SidebarSearchPublication,
        selected: Option<SessionAddress>,
        display_revision: u64,
    ) -> Result<SidebarSearchProjection, &'static str> {
        if self.revision == 0
            || endpoint.is_empty()
            || display_revision == 0
            || search.input.revision == 0
            || self
                .rows
                .iter()
                .any(|row| row.address.endpoint_id != endpoint)
        {
            return Err("invalid sidebar search inventory source");
        }
        let mut input = self.input(selected);
        input.revision = display_revision;
        let Some(key) = &search.input.key else {
            if !search.query.trim().is_empty()
                || !search.hits.is_empty()
                || !search.unresolved.is_empty()
                || search.input.phase != SidebarSearchPhase::Idle
                || search.result.is_some()
            {
                return Err("cleared sidebar search retains results");
            }
            return Ok(SidebarSearchProjection {
                input,
                snippets: BTreeMap::new(),
                unresolved: vec![],
            });
        };
        if key.owner.is_empty()
            || key.endpoint_id != endpoint
            || key.generation == 0
            || key.query != search.query
            || key.query.trim().is_empty()
            || search.input.phase == SidebarSearchPhase::Idle
        {
            return Err("sidebar search result source mismatch");
        }
        let mut snippets = BTreeMap::new();
        for item in &search.hits {
            if item.address.endpoint_id != endpoint
                || item.address.session_id.is_empty()
                || item.address.session_id != item.hit.session_id
                || snippets
                    .insert(item.address.clone(), item.hit.snippet.clone())
                    .is_some()
            {
                return Err("invalid or duplicate sidebar search hit");
            }
        }
        if search
            .unresolved
            .iter()
            .any(|address| !snippets.contains_key(address))
        {
            return Err("invalid unresolved sidebar search hit");
        }
        // Search filters ordinary sessions, never the independent global pins.
        // Determine coherence against the full original inventory before filtering.
        let known: BTreeSet<_> = input.rows.iter().map(|row| row.address.clone()).collect();
        let unresolved: Vec<_> = search
            .hits
            .iter()
            .filter(|hit| !known.contains(&hit.address))
            .map(|hit| hit.address.clone())
            .collect();
        let results_ready =
            unresolved.is_empty() && !matches!(search.input.phase, SidebarSearchPhase::Searching);
        input.rows.retain(|row| {
            row.pin.as_ref().is_some_and(|pin| pin.pinned)
                || (results_ready && snippets.contains_key(&row.address))
        });
        snippets.retain(|address, _| results_ready && known.contains(address));
        // loaded still means inventory restored. A pending search cannot retire
        // original pinned controls or grant them a new mount/action identity.
        input.loaded = self.loaded;
        input.search_help = Some(tekes_agent_components_ui::SidebarSearchHelp {
            results_ready,
            source: search.input.clone(),
            snippets: snippets.clone(),
        });
        Ok(SidebarSearchProjection {
            input,
            snippets,
            unresolved,
        })
    }
}
