//! Read-only, UI-free Sidebar delivery receipt for consumer validation.
//!
//! The caller supplies the exact source publication and component instance it
//! just applied. This module never snapshots a runtime or reconstructs search.
//! Raw observer error strings are represented by presence only, avoiding any
//! accidental serialization of credential material embedded in an error.
use crate::sidebar_host::SidebarHostPublication;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use tekes_agent_components_ui::{Sidebar, SidebarInput, SidebarRow};
use tekes_agent_contract::SessionAddress;
use tekes_agent_presentation::{
    SidebarMultiSearchPublication, SidebarObserverPhase, SidebarObserverStatusInput,
    SidebarSearchPhase,
};
use tekes_agent_ui_bindings::{SidebarMultiPublication, SidebarPublication};

/// Serialize the exact host publication and already-mounted Sidebar state.
///
/// `applied_multi_search` must be the same value passed to the component's
/// multi-search update. It is compared with the component's public displayed
/// getter before any search facts are serialized.
pub fn sidebar_delivery_receipt(
    applied: &SidebarHostPublication,
    sidebar: &Sidebar,
    applied_multi_search: Option<&SidebarMultiSearchPublication>,
) -> Result<Value, &'static str> {
    let publication = applied.as_publication();
    let multi = applied.as_multi_publication();
    let catalog = applied
        .folder_catalog()
        .map_err(|_| "invalid Sidebar folder catalog")?;
    serialize_delivery_receipt(
        &publication,
        applied.observer_status(),
        applied.endpoint_count(),
        multi.as_ref(),
        &catalog,
        sidebar,
        applied_multi_search,
    )
}

/// Pure serializer used by the host wrapper and fixture-based unit tests.
/// Equality checks establish that the supplied display rows are represented by
/// these source facts; equal numeric revisions alone are never treated as proof
/// of source authority.
fn serialize_delivery_receipt(
    publication: &SidebarPublication,
    observer: &SidebarObserverStatusInput,
    endpoint_count: usize,
    multi: Option<&SidebarMultiPublication>,
    catalog: &tekes_agent_ui_bindings::SidebarFolderCatalog,
    sidebar: &Sidebar,
    applied_multi_search: Option<&SidebarMultiSearchPublication>,
) -> Result<Value, &'static str> {
    observer
        .validate()
        .map_err(|_| "invalid Sidebar observer status")?;
    validate_source(publication, observer, endpoint_count, multi, catalog)?;

    let displayed = sidebar
        .displayed_input()
        .ok_or("Sidebar has no applied input")?;
    validate_display(publication, displayed)?;

    let displayed_search = sidebar.displayed_multi_search();
    match (displayed_search, applied_multi_search) {
        (None, None) => {}
        (Some(current), Some(applied)) if current == applied => {
            applied
                .validate()
                .map_err(|_| "invalid applied Sidebar multi-search publication")?;
            validate_search_source(multi, applied)?;
        }
        _ => return Err("Sidebar multi-search source differs from applied publication"),
    }

    let raw_rows = publication
        .rows
        .iter()
        .map(row_json)
        .collect::<Result<Vec<_>, _>>()?;
    let activity_at = publication
        .activity_at
        .iter()
        .map(|(address, at)| {
            if !at.is_finite() {
                return Err("invalid Sidebar activity timestamp");
            }
            Ok(json!({"address": address_json(address), "at": at}))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // row_id is a component getter, not a parent ContentHost lookup or a paint
    // receipt. The host-level probe must verify actual mounting separately.
    let row_ids = displayed
        .rows
        .iter()
        .filter_map(|row| {
            sidebar.row_id(&row.address).map(|id| {
                json!({
                    "address": address_json(&row.address),
                    "rowId": id.0
                })
            })
        })
        .collect::<Vec<_>>();
    let visibility_target = sidebar.visibility_target().map(|target| {
        json!({
            "address": address_json(target.address()),
            "itemId": target.item().0,
            "navigationRevision": target.navigation_revision(),
            "inventoryRevision": target.inventory_revision()
        })
    });

    let name_corpus = publication
        .name_corpus
        .as_ref()
        .map(single_corpus_json)
        .transpose()?
        .unwrap_or(Value::Null);

    let folders = catalog
        .folders
        .iter()
        .map(|folder| {
            json!({
                "key": &folder.key,
                "path": &folder.path
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "schemaVersion": 1,
        "evidenceScope": "source-and-component-getters",
        "nativeInteraction": false,
        "source": {
            "revision": publication.revision,
            "loaded": publication.loaded,
            "loadedMeansAllEndpointsHealthy": false,
            "errorPresent": publication.error.is_some(),
            "endpointCount": endpoint_count,
            "rows": raw_rows,
            "activityAt": activity_at,
            "classification": classification_json(publication, multi)?,
            "nameCorpus": name_corpus,
            "nameCorpora": multi.map(multi_name_corpora_json).unwrap_or(Value::Null),
            "multiEndpoints": multi.map(multi_endpoints_json).unwrap_or(Value::Null),
            "folderCatalog": {
                "revision": catalog.revision,
                "loaded": catalog.loaded,
                "errorPresent": catalog.error.is_some(),
                "folders": folders
            },
            "observer": observer_json(observer, publication, multi)
        },
        "display": {
            "revision": displayed.revision,
            "loaded": displayed.loaded,
            "errorPresent": displayed.error.is_some(),
            "selected": displayed.selected.as_ref().map(address_json),
            "rows": displayed.rows.iter().map(row_json).collect::<Result<Vec<_>, _>>()?,
            "paging": {
                "limit": sidebar.paging().limit,
                "ordinaryCount": sidebar.paging().ordinary_count,
                "hasMore": sidebar.paging().has_more,
                "selectedSupplemented": sidebar.paging().selected_supplemented
            },
            "rowIds": row_ids,
            "rowIdsProveMount": false,
            "visibilityTarget": visibility_target,
            "multiSearch": applied_multi_search.map(search_json).transpose()?.unwrap_or(Value::Null)
        }
    }))
}

/// Serialize already captured DTOs without runtime access. This lower-level
/// entry point is useful for Rust fixture tests; it is descriptive only and
/// does not establish that the caller owns or captured the source authority.
#[allow(clippy::too_many_arguments)]
pub fn sidebar_delivery_receipt_from_facts(
    publication: &SidebarPublication,
    observer: &SidebarObserverStatusInput,
    endpoint_count: usize,
    multi: Option<&SidebarMultiPublication>,
    catalog: &tekes_agent_ui_bindings::SidebarFolderCatalog,
    sidebar: &Sidebar,
    applied_multi_search: Option<&SidebarMultiSearchPublication>,
) -> Result<Value, &'static str> {
    serialize_delivery_receipt(
        publication,
        observer,
        endpoint_count,
        multi,
        catalog,
        sidebar,
        applied_multi_search,
    )
}

fn validate_source(
    publication: &SidebarPublication,
    observer: &SidebarObserverStatusInput,
    endpoint_count: usize,
    multi: Option<&SidebarMultiPublication>,
    catalog: &tekes_agent_ui_bindings::SidebarFolderCatalog,
) -> Result<(), &'static str> {
    if publication.revision == 0 || endpoint_count == 0 || catalog.revision != publication.revision
    {
        return Err("invalid Sidebar source revision or endpoint count");
    }
    if observer.endpoints.len() != endpoint_count {
        return Err("Sidebar observer endpoint count differs from publication");
    }
    let endpoint_ids: BTreeSet<_> = observer
        .endpoints
        .iter()
        .map(|endpoint| endpoint.key.endpoint_id())
        .collect();
    if endpoint_ids.len() != endpoint_count {
        return Err("duplicate Sidebar observer endpoint");
    }
    let mut addresses = BTreeSet::new();
    for row in &publication.rows {
        if !endpoint_ids.contains(row.address.endpoint_id.as_str())
            || row.address.session_id.is_empty()
            || !addresses.insert(row.address.clone())
            || !publication.activity_at.contains_key(&row.address)
        {
            return Err("invalid or duplicate Sidebar source row");
        }
    }
    if publication.activity_at.iter().any(|(address, at)| {
        !endpoint_ids.contains(address.endpoint_id.as_str())
            || address.session_id.is_empty()
            || !at.is_finite()
    }) {
        return Err("invalid Sidebar activity source");
    }
    match (endpoint_count, multi) {
        (1, None) => {}
        (count, Some(source)) if count > 1 => {
            if source.revision != publication.revision
                || source.loaded != publication.loaded
                || source.error != publication.error
                || source.rows != publication.rows
                || source.activity_at != publication.activity_at
                || source.endpoints.len() != count
            {
                return Err("Sidebar multi-source facts differ from applied publication");
            }
            let endpoint_ids: BTreeSet<_> = source.endpoints.keys().map(String::as_str).collect();
            if observer
                .endpoints
                .iter()
                .any(|endpoint| !endpoint_ids.contains(endpoint.key.endpoint_id()))
            {
                return Err("Sidebar observer and inventory endpoint sets differ");
            }
        }
        _ => return Err("Sidebar source shape differs from endpoint count"),
    }
    if let Some(corpus) = &publication.name_corpus {
        corpus
            .validate()
            .map_err(|_| "invalid Sidebar name corpus")?;
        if endpoint_count != 1
            || observer.endpoints[0].key.endpoint_id() != corpus.endpoint_id.as_str()
        {
            return Err("Sidebar name corpus endpoint differs from observer");
        }
    }
    if let Some(source) = multi {
        for (endpoint_id, corpus) in &source.name_corpora {
            corpus
                .validate()
                .map_err(|_| "invalid Sidebar multi name corpus")?;
            if endpoint_id != &corpus.endpoint_id || !source.endpoints.contains_key(endpoint_id) {
                return Err("Sidebar name corpus endpoint differs from inventory");
            }
        }
    }
    Ok(())
}

fn validate_display(
    publication: &SidebarPublication,
    displayed: &SidebarInput,
) -> Result<(), &'static str> {
    // Host view revisions also advance for query/selection/preferences. They
    // need not equal the original inventory revision; validate the actual
    // source row facts below rather than treating a number as source identity.
    if displayed.revision == 0
        || displayed.loaded != publication.loaded
        || displayed.error != publication.error
    {
        return Err("Sidebar displayed status differs from applied publication");
    }
    let source: BTreeMap<_, _> = publication
        .rows
        .iter()
        .map(|row| (&row.address, row))
        .collect();
    let mut seen = BTreeSet::new();
    if displayed.rows.iter().any(|row| {
        !seen.insert(&row.address)
            || source
                .get(&row.address)
                .is_none_or(|source_row| *source_row != row)
    }) {
        return Err("Sidebar display rows differ from applied source facts");
    }
    Ok(())
}

fn validate_search_source(
    multi: Option<&SidebarMultiPublication>,
    search: &SidebarMultiSearchPublication,
) -> Result<(), &'static str> {
    let inventory = multi.ok_or("multi-search has no multi-endpoint inventory")?;
    let expected: BTreeSet<_> = inventory.endpoints.keys().map(String::as_str).collect();
    let actual: BTreeSet<_> = search
        .children
        .iter()
        .map(|child| child.endpoint_id.as_str())
        .collect();
    if expected != actual {
        return Err("multi-search endpoint domain differs from inventory");
    }
    Ok(())
}

fn row_json(row: &SidebarRow) -> Result<Value, &'static str> {
    let activity =
        serde_json::to_value(&row.activity).map_err(|_| "cannot serialize Sidebar row activity")?;
    Ok(json!({
        "address": address_json(&row.address),
        "title": &row.title,
        "primaryFolderId": &row.primary_folder_id,
        "folderPath": &row.folder_path,
        "activity": activity
    }))
}

fn address_json(address: &SessionAddress) -> Value {
    json!({"endpointId": &address.endpoint_id, "sessionId": &address.session_id})
}

fn single_corpus_json(
    corpus: &tekes_agent_presentation::SidebarNameCorpus,
) -> Result<Value, &'static str> {
    Ok(json!({
        "endpointId": &corpus.endpoint_id,
        "connectionEpoch": corpus.connection_epoch,
        "endpointDomain": &corpus.endpoint_domain,
        "ready": corpus.inventory_ready
    }))
}

fn multi_name_corpora_json(source: &SidebarMultiPublication) -> Value {
    Value::Array(
        source
            .name_corpora
            .values()
            .map(|corpus| {
                json!({
                    "endpointId": &corpus.endpoint_id,
                    "connectionEpoch": corpus.connection_epoch,
                    "endpointDomain": &corpus.endpoint_domain,
                    "ready": corpus.inventory_ready
                })
            })
            .collect(),
    )
}

fn multi_endpoints_json(source: &SidebarMultiPublication) -> Value {
    Value::Array(
        source
            .endpoints
            .values()
            .map(|endpoint| {
                json!({
                    "endpointId": endpoint.endpoint_id,
                    "revision": endpoint.revision,
                    "loaded": endpoint.loaded,
                    "stale": endpoint.stale,
                    "terminalError": endpoint.terminal_error,
                    "errorPresent": endpoint.error.is_some(),
                    "observation": observation_json(endpoint.observation)
                })
            })
            .collect(),
    )
}

fn observer_json(
    observer: &SidebarObserverStatusInput,
    publication: &SidebarPublication,
    multi: Option<&SidebarMultiPublication>,
) -> Value {
    Value::Array(
        observer
            .endpoints
            .iter()
            .map(|endpoint| {
                let id = endpoint.key.endpoint_id();
                let multi_status = multi.and_then(|source| source.endpoints.get(id));
                let row_stale = publication
                    .rows
                    .iter()
                    .any(|row| row.address.endpoint_id == id && row.activity.is_stale);
                json!({
                    "endpointId": id,
                    "generation": endpoint.key.generation(),
                    "revision": endpoint.revision,
                    "loaded": multi_status.map(|status| status.loaded).unwrap_or(publication.loaded),
                    "stale": multi_status.map(|status| status.stale).unwrap_or(row_stale),
                    "errorPresent": endpoint.error.is_some() || multi_status.is_some_and(|status| status.error.is_some()),
                    "observation": observation_json(endpoint.observation)
                })
            })
            .collect(),
    )
}

fn observation_json(observation: tekes_agent_presentation::SidebarObservation) -> Value {
    let phase = match observation.phase {
        SidebarObserverPhase::Observing => "observing",
        SidebarObserverPhase::Restarting => "restarting",
        SidebarObserverPhase::Failed => "failed",
    };
    json!({
        "generation": observation.generation,
        "phase": phase,
        "converged": observation.converged
    })
}

fn search_json(search: &SidebarMultiSearchPublication) -> Result<Value, &'static str> {
    let children = search
        .children
        .iter()
        .map(|child| {
            let publication = &child.publication;
            let hits = publication
                .hits
                .iter()
                .map(|hit| address_json(&hit.address))
                .collect::<Vec<_>>();
            let unresolved = publication
                .unresolved
                .iter()
                .map(address_json)
                .collect::<Vec<_>>();
            Ok(json!({
                "endpointId": &child.endpoint_id,
                "revision": publication.input.revision,
                "phase": search_phase_json(&publication.input.phase),
                "hits": hits,
                "unresolved": unresolved
            }))
        })
        .collect::<Result<Vec<_>, &'static str>>()?;
    Ok(json!({
        "query": &search.query,
        "unresolvedScope": "reader-publication-not-display-projection",
        "generation": search.input.generation,
        "revision": search.input.revision,
        "children": children
    }))
}

fn search_phase_json(phase: &SidebarSearchPhase) -> Value {
    match phase {
        SidebarSearchPhase::Idle => json!({"state": "idle"}),
        SidebarSearchPhase::Searching => json!({"state": "searching"}),
        SidebarSearchPhase::Complete { has_more } => {
            json!({"state": "complete", "hasMore": has_more})
        }
        SidebarSearchPhase::Failed {
            has_more,
            retryable,
            ..
        } => json!({
            "state": "failed",
            "hasMore": has_more,
            "retryable": retryable
        }),
    }
}

fn fact_json<T>(
    fact: &tekes_agent_ui_bindings::SidebarClassificationFact<T>,
    known: impl Fn(&T) -> Value,
) -> Value {
    use tekes_agent_ui_bindings::SidebarClassificationFact::*;
    match fact {
        Missing => json!({"state":"missing"}),
        Known(value) => json!({"state":"known","value":known(value)}),
        Unknown(value) => json!({"state":"unknown","value":value}),
    }
}

/// No runtime read: all facts are from the original inventory packet.
fn classification_json(
    publication: &SidebarPublication,
    multi: Option<&SidebarMultiPublication>,
) -> Result<Value, &'static str> {
    let sources: Vec<_> = match multi {
        Some(multi) => multi.classification.values().collect(),
        None => publication.classification.iter().collect(),
    };
    let mut evidence = Vec::new();
    for source in sources {
        let endpoint = &source.endpoint_id;
        let (current_revision, observation) = match multi {
            Some(multi) => {
                let status = multi
                    .endpoints
                    .get(endpoint)
                    .ok_or("classification endpoint missing")?;
                (status.revision, status.observation)
            }
            None => (publication.revision, publication.observation),
        };
        if source.source_revision == 0
            || source.source_revision > current_revision
            || source.observer_generation == 0
            || source.observer_generation > observation.generation
        {
            return Err("classification source fence is newer than original packet");
        }
        if observation.phase == SidebarObserverPhase::Observing
            && (source.source_revision != current_revision
                || source.observer_generation != observation.generation)
        {
            return Err("classification belongs to a different inventory frame");
        }
        let active: BTreeSet<_> = publication
            .activity_at
            .keys()
            .filter(|key| &key.endpoint_id == endpoint)
            .collect();
        let recorded: BTreeSet<_> = source.records.keys().collect();
        if active != recorded {
            return Err("classification does not cover original active inventory");
        }
        let included: BTreeSet<_> = source
            .records
            .iter()
            .filter(|(_, value)| value.included)
            .map(|(key, _)| key)
            .collect();
        let displayed: BTreeSet<_> = publication
            .rows
            .iter()
            .filter(|row| &row.address.endpoint_id == endpoint)
            .map(|row| &row.address)
            .collect();
        if included != displayed {
            return Err("classification membership differs from original rows");
        }
        let records: Vec<_> = source.records.iter().map(|(address, record)| {
            let facts = &record.facts;
            json!({"address":address_json(address),"included":record.included,
                "origin":fact_json(&facts.origin, |origin| match origin {
                    tekes_agent_ui_bindings::SidebarClassificationOrigin::Fork => json!("fork"),
                    tekes_agent_ui_bindings::SidebarClassificationOrigin::Subagent => json!("subagent"),
                }),
                "parentSessionId":fact_json(&facts.parent_session_id, |value| json!(value)),
                "ephemeral":fact_json(&facts.ephemeral, |value| json!(value))})
        }).collect();
        let mut frame = json!({"endpointId":endpoint,"sourceRevision":source.source_revision,
            "connectionEpoch":source.connection_epoch,"observerGeneration":source.observer_generation,
            "inventoryReady":source.inventory_ready,"records":records});
        if let Some(raw) = &source.archive_facts {
            frame
                .as_object_mut()
                .unwrap()
                .insert("archiveFacts".into(), raw.clone());
        }
        evidence.push(frame);
    }
    Ok(json!(evidence))
}
