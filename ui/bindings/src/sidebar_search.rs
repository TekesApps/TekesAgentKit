//! Thin public binding name for the shared, renderer-independent search reader.
pub use tekes_agent_presentation::{
    SidebarSearchController as SidebarSearchBinding, SidebarSearchMatch, SidebarSearchMessages,
    SidebarSearchPublication,
};

use crate::{SidebarNameIndex, SidebarPublication};
use std::collections::{BTreeMap, BTreeSet};
use tekes_agent_components_ui::{SidebarInput, SidebarSearchPhase};
use tekes_agent_contract::SessionAddress;
pub use tekes_agent_contract::{SessionSearchHit, SessionSearchResults};
use tekes_agent_presentation::{
    SidebarNameCorpus, SidebarObserverPhase, SidebarObserverSourceKey, SidebarSort,
};

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

    pub fn project_search_for_display_with_sort(
        &self,
        endpoint: &str,
        search: &SidebarSearchPublication,
        display: &SidebarInput,
        sort: SidebarSort,
    ) -> Result<SidebarSearchProjection, &'static str> {
        let owner = std::sync::Arc::new(());
        SidebarSearchProjector::default().project_for_display_with_sort(
            &SidebarObserverSourceKey::from_owner(&owner),
            self,
            endpoint,
            search,
            display,
            sort,
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
    pub(crate) index: Option<SidebarNameIndex>,
}
struct LocalNameMatches {
    addresses: BTreeSet<SessionAddress>,
    scores: BTreeMap<SessionAddress, f64>,
    rank_ready: bool,
}
impl SidebarSearchProjector {
    pub fn project_for_display(
        &mut self,
        source: &SidebarObserverSourceKey,
        inventory: &SidebarPublication,
        endpoint: &str,
        search: &SidebarSearchPublication,
        display: &SidebarInput,
    ) -> Result<SidebarSearchProjection, &'static str> {
        self.project_for_display_with_sort(
            source,
            inventory,
            endpoint,
            search,
            display,
            // Preserve the historical membership-only convenience API.
            SidebarSort::TitleAscending,
        )
    }

    /// Explicit-sort variant used by hosts that already own Sidebar preferences.
    pub fn project_for_display_with_sort(
        &mut self,
        source: &SidebarObserverSourceKey,
        inventory: &SidebarPublication,
        endpoint: &str,
        search: &SidebarSearchPublication,
        display: &SidebarInput,
        sort: SidebarSort,
    ) -> Result<SidebarSearchProjection, &'static str> {
        let LocalNameMatches {
            addresses: local,
            scores,
            rank_ready,
        } = self.local_ranked(
            source,
            inventory.revision,
            &inventory.rows,
            inventory.name_corpus.as_ref(),
            &search.query,
        )?;
        let mut projection = project_with_local(inventory, endpoint, search, display, &local)?;
        if inventory.name_corpus.is_some()
            && let Some(help) = projection.input.search_help.as_mut()
        {
            help.local_results_ready &= rank_ready;
        }
        if sort == SidebarSort::RecentActivity && rank_ready {
            let known: BTreeSet<_> = inventory
                .rows
                .iter()
                .map(|row| row.address.clone())
                .collect();
            let provider_only: BTreeSet<_> = search
                .hits
                .iter()
                .filter(|hit| known.contains(&hit.address) && !scores.contains_key(&hit.address))
                .map(|hit| hit.address.clone())
                .collect();
            order_recent(
                &mut projection.input.rows,
                &inventory.activity_at,
                &scores,
                &provider_only,
            );
        }
        Ok(projection)
    }

    fn local_ranked(
        &mut self,
        source: &SidebarObserverSourceKey,
        revision: u64,
        rows: &[tekes_agent_components_ui::SidebarRow],
        corpus: Option<&SidebarNameCorpus>,
        query: &str,
    ) -> Result<LocalNameMatches, &'static str> {
        if self.index.is_none() {
            self.index =
                Some(SidebarNameIndex::new().map_err(|_| "cannot create Sidebar title FTS index")?);
        }
        let index = self.index.as_mut().unwrap();
        let rank_ready = if let Some(corpus) = corpus {
            index
                .replace_corpus_snapshot(source, revision, corpus)
                .map_err(|_| "invalid Sidebar retained-name corpus")?;
            corpus.inventory_ready && corpus.endpoint_domain == [corpus.endpoint_id.clone()]
        } else {
            let docs = crate::sidebar_name_search::legacy_documents(rows);
            index
                .replace_snapshot(source, revision, &docs)
                .map_err(|_| "invalid Sidebar title index snapshot")?;
            false
        };
        let matches = index
            .matches_with_scores(query)
            .map_err(|_| "cannot query Sidebar title FTS index")?;
        let scores = matches.iter().cloned().collect::<BTreeMap<_, _>>();
        Ok(LocalNameMatches {
            addresses: scores.keys().cloned().collect(),
            scores,
            rank_ready,
        })
    }
}

pub(crate) fn order_recent(
    rows: &mut [tekes_agent_components_ui::SidebarRow],
    activity: &BTreeMap<SessionAddress, f64>,
    scores: &BTreeMap<SessionAddress, f64>,
    provider_only: &BTreeSet<SessionAddress>,
) {
    rows.sort_by(|left, right| {
        let left_rank = scores.get(&left.address).copied().unwrap_or_else(|| {
            if provider_only.contains(&left.address) {
                1_000_000.0
            } else {
                f64::INFINITY
            }
        });
        let right_rank = scores.get(&right.address).copied().unwrap_or_else(|| {
            if provider_only.contains(&right.address) {
                1_000_000.0
            } else {
                f64::INFINITY
            }
        });
        left_rank
            .total_cmp(&right_rank)
            .then_with(|| {
                activity
                    .get(&right.address)
                    .copied()
                    .unwrap_or(0.0)
                    .total_cmp(&activity.get(&left.address).copied().unwrap_or(0.0))
            })
            .then_with(|| {
                client_sidebar_identity(&left.address).cmp(&client_sidebar_identity(&right.address))
            })
    });
}

// ClientMirrorStore's final SQL tie is the durable thread ID, not a tuple
// ordered by endpoint spelling. Keep the Rust address public; only compare the
// equivalent key here (SessionEndpointStorageIdentity.thread).
fn client_sidebar_identity(address: &SessionAddress) -> String {
    if address.endpoint_id == "legacy-tekes-appserver" {
        return address.session_id.clone();
    }
    format!(
        "{}:{}7:session{}",
        address.endpoint_id.len(),
        address.endpoint_id,
        address.session_id
    )
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
    if let Some(corpus) = &inventory.name_corpus {
        corpus
            .validate()
            .map_err(|_| "invalid Sidebar retained-name corpus")?;
        if corpus.endpoint_id != endpoint || corpus.connection_epoch != key.connection_epoch {
            return Err("sidebar search and retained-name corpus epoch mismatch");
        }
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

#[cfg(test)]
mod identity_tests {
    use super::*;

    #[test]
    fn recent_rank_tie_uses_client_storage_identity_instead_of_endpoint_tuple() {
        let address = |endpoint: &str, session: &str| SessionAddress {
            endpoint_id: endpoint.into(),
            session_id: session.into(),
        };
        assert_eq!(
            client_sidebar_identity(&address("ab", "s")),
            "2:ab7:sessions"
        );
        assert_eq!(
            client_sidebar_identity(&address("中", "s")),
            "3:中7:sessions"
        );
        assert_eq!(
            client_sidebar_identity(&address("legacy-tekes-appserver", "native-id")),
            "native-id"
        );
        assert!(address("ab", "s") < address("z", "s"));
        assert!(
            client_sidebar_identity(&address("z", "s"))
                < client_sidebar_identity(&address("ab", "s"))
        );
    }
}
