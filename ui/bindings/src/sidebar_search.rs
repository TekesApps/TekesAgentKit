//! Thin public binding name for the shared, renderer-independent search reader.
pub use tekes_agent_presentation::{
    SidebarSearchController as SidebarSearchBinding, SidebarSearchMatch, SidebarSearchMessages,
    SidebarSearchPublication,
};

use crate::{SidebarNameDocument, SidebarNameIndex, SidebarPublication};
use std::collections::{BTreeMap, BTreeSet};
use tekes_agent_components_ui::{SidebarInput, SidebarSearchPhase};
use tekes_agent_contract::SessionAddress;
pub use tekes_agent_contract::{SessionSearchHit, SessionSearchResults};
use tekes_agent_presentation::{SidebarObserverPhase, SidebarObserverSourceKey};

/// Display projection of provider search hits joined to the original inventory.
/// Inventory order remains authoritative (including the host's sort preference).
/// Local canonical-name FTS is unioned with known complete provider hits.
/// Unknown provider IDs never manufacture rows.
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarSearchProjection {
    pub input: SidebarInput,
    /// Query-scoped row help; consume with an original library tooltip when the
    /// fixed SDK provides that declaration, rather than adding a subtitle row.
    pub snippets: BTreeMap<SessionAddress, String>,
    pub unresolved: Vec<SessionAddress>,
}
impl SidebarPublication {
    /// Preserve explicit host selection/filter/view revision while the shared
    /// projection owns query membership for every session row, including pins.
    pub fn project_search_for_display(
        &self,
        endpoint: &str,
        search: &SidebarSearchPublication,
        display: &SidebarInput,
    ) -> Result<SidebarSearchProjection, &'static str> {
        let owner = std::sync::Arc::new(());
        SidebarSearchProjector::default().project_for_display(
            &SidebarObserverSourceKey::from_owner(&owner),
            self,
            endpoint,
            search,
            display,
        )
    }

    pub fn project_search(
        &self,
        endpoint: &str,
        search: &SidebarSearchPublication,
        selected: Option<SessionAddress>,
        display_revision: u64,
    ) -> Result<SidebarSearchProjection, &'static str> {
        let mut display = self.input(selected);
        display.revision = display_revision;
        self.project_search_for_display(endpoint, search, &display)
    }
}

/// Reusable derived index, owned by a consumer alongside original publications.
/// Source keys fence cache reuse; they do not authenticate caller-made snapshots.
#[derive(Default)]
pub struct SidebarSearchProjector {
    index: Option<SidebarNameIndex>,
}
impl SidebarSearchProjector {
    pub(crate) fn local_matches(
        &mut self,
        source: &SidebarObserverSourceKey,
        revision: u64,
        rows: &[tekes_agent_components_ui::SidebarRow],
        query: &str,
    ) -> Result<BTreeSet<SessionAddress>, &'static str> {
        if self.index.is_none() {
            self.index =
                Some(SidebarNameIndex::new().map_err(|_| "cannot create Sidebar title FTS index")?);
        }
        let index = self.index.as_mut().unwrap();
        let documents: Vec<_> = rows
            .iter()
            .map(|row| SidebarNameDocument {
                address: row.address.clone(),
                title: row.title.clone(),
            })
            .collect();
        index
            .replace_snapshot(source, revision, &documents)
            .map_err(|_| "invalid Sidebar title index snapshot")?;
        index
            .matches(query)
            .map(|rows| rows.into_iter().collect())
            .map_err(|_| "cannot query Sidebar title FTS index")
    }

    pub fn project_for_display(
        &mut self,
        source: &SidebarObserverSourceKey,
        inventory: &SidebarPublication,
        endpoint: &str,
        search: &SidebarSearchPublication,
        display: &SidebarInput,
    ) -> Result<SidebarSearchProjection, &'static str> {
        let local =
            self.local_matches(source, inventory.revision, &inventory.rows, &search.query)?;
        project_with_local(inventory, endpoint, search, display, &local)
    }
}

pub(crate) fn project_with_local(
    inventory: &SidebarPublication,
    endpoint: &str,
    search: &SidebarSearchPublication,
    display: &SidebarInput,
    local: &BTreeSet<SessionAddress>,
) -> Result<SidebarSearchProjection, &'static str> {
    if let Some(key) = &display.folder_filter {
        key.validate()
            .map_err(|_| "invalid Sidebar folder filter")?;
    }
    if inventory.revision == 0
        || endpoint.is_empty()
        || display.revision == 0
        || search.input.revision == 0
        || inventory
            .rows
            .iter()
            .any(|row| row.address.endpoint_id != endpoint)
    {
        return Err("invalid sidebar search inventory source");
    }
    let mut input = inventory.input(display.selected.clone());
    input.revision = display.revision;
    input.folder_filter = display.folder_filter.clone();
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
    // Determine coherence against the full original inventory before filtering.
    let known: BTreeSet<_> = input.rows.iter().map(|row| row.address.clone()).collect();
    let unresolved: Vec<_> = search
        .hits
        .iter()
        .filter(|hit| !known.contains(&hit.address))
        .map(|hit| hit.address.clone())
        .collect();
    let provider_complete = matches!(search.input.phase, SidebarSearchPhase::Complete { .. });
    let results_ready = unresolved.is_empty() && provider_complete;
    // A missing provider row does not hide other known provider/local hits.
    input.rows.retain(|row| {
        local.contains(&row.address) || (provider_complete && snippets.contains_key(&row.address))
    });
    snippets.retain(|address, _| provider_complete && known.contains(address));
    input.loaded = inventory.loaded;
    input.search_help = Some(tekes_agent_components_ui::SidebarSearchHelp {
        local_results_ready: inventory.loaded
            && inventory.observation.phase == SidebarObserverPhase::Observing,
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
