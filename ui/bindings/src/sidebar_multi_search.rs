//! Renderer-independent multi-endpoint search projection over the original
//! aggregate inventory. Search publications remain endpoint-owned.
use crate::{SidebarMultiPublication, SidebarPublication, SidebarSearchProjector};
use std::collections::{BTreeMap, BTreeSet};
use tekes_agent_components_ui::SidebarInput;
use tekes_agent_contract::SessionAddress;
use tekes_agent_presentation::SidebarObserverSourceKey;
use tekes_agent_presentation::SidebarSearchPublication;
pub use tekes_agent_presentation::{
    SidebarMultiSearchChild, SidebarMultiSearchController as SidebarMultiSearchBinding,
    SidebarMultiSearchInput, SidebarMultiSearchIntent, SidebarMultiSearchKey,
    SidebarMultiSearchPublication,
};
use tekes_agent_presentation::{SidebarNameDocument, SidebarSort};

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
    pub help: tekes_agent_components_ui::SidebarMultiSearchHelp,
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
        self.project_multi_search_for_display_with_sort(
            search,
            display,
            // Preserve the historical membership-only convenience API.
            SidebarSort::TitleAscending,
        )
    }

    pub fn project_multi_search_for_display_with_sort(
        &self,
        search: &SidebarMultiSearchPublication,
        display: &SidebarInput,
        sort: SidebarSort,
    ) -> Result<SidebarMultiSearchProjection, &'static str> {
        let owner = std::sync::Arc::new(());
        SidebarSearchProjector::default().project_multi_for_display_with_sort(
            &SidebarObserverSourceKey::from_owner(&owner),
            self,
            search,
            display,
            sort,
        )
    }
}
impl SidebarSearchProjector {
    pub fn project_multi_for_display(
        &mut self,
        source: &SidebarObserverSourceKey,
        inventory: &SidebarMultiPublication,
        search: &SidebarMultiSearchPublication,
        display: &SidebarInput,
    ) -> Result<SidebarMultiSearchProjection, &'static str> {
        self.project_multi_for_display_with_sort(
            source,
            inventory,
            search,
            display,
            // Preserve the historical membership-only convenience API.
            SidebarSort::TitleAscending,
        )
    }

    pub fn project_multi_for_display_with_sort(
        &mut self,
        source: &SidebarObserverSourceKey,
        inventory: &SidebarMultiPublication,
        search: &SidebarMultiSearchPublication,
        display: &SidebarInput,
        sort: SidebarSort,
    ) -> Result<SidebarMultiSearchProjection, &'static str> {
        if inventory.revision == 0 || display.revision == 0 {
            return Err("invalid multi-search display source");
        }
        if let Some(key) = &display.folder_filter {
            key.validate()
                .map_err(|_| "invalid Sidebar folder filter")?;
        }
        validate_multi_search(search, &inventory.endpoints)?;

        let endpoint_ids: BTreeSet<_> = inventory.endpoints.keys().cloned().collect();
        if inventory.activity_at.iter().any(|(address, activity)| {
            !endpoint_ids.contains(&address.endpoint_id)
                || address.session_id.is_empty()
                || !activity.is_finite()
        }) {
            return Err("invalid multi-search active inventory facts");
        }
        let mut documents = Vec::<SidebarNameDocument>::new();
        let mut epochs = BTreeMap::new();
        let mut common_domain: Option<Vec<String>> = None;
        let mut all_corpora_ready = !inventory.name_corpora.is_empty();
        for (endpoint_id, corpus) in &inventory.name_corpora {
            corpus
                .validate()
                .map_err(|_| "invalid multi-endpoint Sidebar name corpus")?;
            if &corpus.endpoint_id != endpoint_id {
                return Err("Sidebar name corpus endpoint key mismatch");
            }
            if let Some(domain) = &common_domain {
                if *domain != corpus.endpoint_domain {
                    return Err("Sidebar name corpus domains conflict");
                }
            } else {
                common_domain = Some(corpus.endpoint_domain.clone());
            }
            epochs.insert(endpoint_id.clone(), corpus.connection_epoch);
            all_corpora_ready &= corpus.inventory_ready;
            documents.extend(corpus.documents.iter().cloned());
        }
        if inventory
            .name_corpora
            .keys()
            .any(|id| !endpoint_ids.contains(id))
        {
            return Err("Sidebar name corpus escaped the inventory domain");
        }
        // Legacy/missing children still contribute their known row names to
        // membership. Their partial corpus is never used for global ranking.
        documents.extend(crate::sidebar_name_search::legacy_documents(
            &inventory
                .rows
                .iter()
                .filter(|row| {
                    !inventory
                        .name_corpora
                        .contains_key(&row.address.endpoint_id)
                })
                .cloned()
                .collect::<Vec<_>>(),
        ));
        let expected_domain: Vec<_> = endpoint_ids.iter().cloned().collect();
        let rank_ready = all_corpora_ready
            && inventory.name_corpora.len() == endpoint_ids.len()
            && common_domain.as_ref() == Some(&expected_domain)
            && epochs.keys().cloned().collect::<BTreeSet<_>>() == endpoint_ids;
        let local_scores = if inventory.name_corpora.is_empty() {
            let documents = crate::sidebar_name_search::legacy_documents(&inventory.rows);
            if self.index.is_none() {
                self.index = Some(
                    crate::SidebarNameIndex::new()
                        .map_err(|_| "cannot create Sidebar title FTS index")?,
                );
            }
            let index = self.index.as_mut().unwrap();
            index
                .replace_snapshot(source, inventory.revision, &documents)
                .map_err(|_| "invalid Sidebar title index snapshot")?;
            index
                .matches_with_scores(&search.query)
                .map_err(|_| "cannot query Sidebar title FTS index")?
        } else {
            if self.index.is_none() {
                self.index = Some(
                    crate::SidebarNameIndex::new()
                        .map_err(|_| "cannot create Sidebar title FTS index")?,
                );
            }
            let index = self.index.as_mut().unwrap();
            index
                .replace_ranked_snapshot(
                    source,
                    inventory.revision,
                    &documents,
                    epochs,
                    common_domain.unwrap_or_default(),
                    all_corpora_ready,
                )
                .map_err(|_| "invalid Sidebar retained-name corpus")?;
            index
                .matches_with_scores(&search.query)
                .map_err(|_| "cannot query Sidebar title FTS index")?
        };
        let scores: BTreeMap<_, _> = local_scores.iter().cloned().collect();
        let local: BTreeSet<_> = scores.keys().cloned().collect();
        let mut local_results_ready = true;
        let mut results_ready = true;
        let mut permitted = BTreeSet::new();
        let mut snippets = BTreeMap::new();
        let mut unresolved = Vec::new();
        let mut children = BTreeMap::new();
        let mut provider_only = BTreeSet::new();

        for child in &search.children {
            let endpoint_id = child.endpoint_id.as_str();
            let child_inventory = SidebarPublication {
                observation: inventory.endpoints[endpoint_id].observation,
                revision: inventory.revision,
                rows: inventory
                    .rows
                    .iter()
                    .filter(|row| row.address.endpoint_id == endpoint_id)
                    .cloned()
                    .collect(),
                activity_at: inventory
                    .activity_at
                    .iter()
                    .filter(|(address, _)| address.endpoint_id == endpoint_id)
                    .map(|(address, activity)| (address.clone(), *activity))
                    .collect(),
                name_corpus: inventory.name_corpora.get(endpoint_id).cloned(),
                loaded: inventory.endpoints[endpoint_id].loaded,
                error: inventory.endpoints[endpoint_id].error.clone(),
            };
            let projection = crate::sidebar_search::project_with_local(
                &child_inventory,
                endpoint_id,
                &child.publication,
                display,
                &local,
            )?;
            if matches!(
                child.publication.input.phase,
                tekes_agent_components_ui::SidebarSearchPhase::Complete { .. }
            ) {
                provider_only.extend(
                    child
                        .publication
                        .hits
                        .iter()
                        .filter(|hit| !scores.contains_key(&hit.address))
                        .map(|hit| hit.address.clone()),
                );
            }
            if let Some(help) = &projection.input.search_help {
                local_results_ready &= help.local_results_ready;
                results_ready &= help.results_ready;
            } else {
                local_results_ready = false;
                results_ready = false;
            }
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
        let mut input = inventory.input(display.selected.clone());
        input.revision = display.revision;
        input.folder_filter = display.folder_filter.clone();
        input.rows.retain(|row| permitted.contains(&row.address));
        if sort == SidebarSort::RecentActivity && rank_ready {
            crate::sidebar_search::order_recent(
                &mut input.rows,
                &inventory.activity_at,
                &scores,
                &provider_only,
            );
        }
        if !inventory.name_corpora.is_empty() {
            local_results_ready &= rank_ready;
        }
        input.search_help = None;

        // Guard against malformed aggregate inventory rows/activity omitted
        // from their endpoint partition rather than silently reordering them.
        if endpoint_ids.is_empty()
            || inventory.rows.iter().any(|row| {
                !endpoint_ids.contains(&row.address.endpoint_id)
                    || !inventory.activity_at.contains_key(&row.address)
            })
        {
            return Err("invalid multi-endpoint Sidebar inventory");
        }

        Ok(SidebarMultiSearchProjection {
            input,
            aggregate_input: search.input.clone(),
            query: search.query.clone(),
            children,
            help: tekes_agent_components_ui::SidebarMultiSearchHelp {
                source: search.input.clone(),
                snippets: snippets.clone(),
                local_results_ready,
                results_ready,
            },
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
