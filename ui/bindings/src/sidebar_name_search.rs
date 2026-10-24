//! Rebuildable, in-memory title membership for Sidebar queries.
//!
//! This index is derived from a public inventory snapshot. It is not a session
//! store and never reads harness-private files.

use std::collections::BTreeMap;

use rusqlite::{Connection, params};
use tekes_agent_contract::{Error, Result, SessionAddress, require};

use tekes_agent_presentation::{SidebarNameCorpus, SidebarObserverSourceKey};

/// Keep the historical binding path while sharing the canonical DTO.
pub use tekes_agent_presentation::SidebarNameDocument;

/// A rebuildable SQLite FTS5 index of titles from one inventory source.
pub struct SidebarNameIndex {
    connection: Connection,
    source: Option<SidebarObserverSourceKey>,
    revision: Option<u64>,
    snapshot: BTreeMap<SessionAddress, String>,
    corpus_metadata: Option<(BTreeMap<String, u64>, Vec<String>, bool)>,
}

impl SidebarNameIndex {
    /// Create an empty in-memory index. This fails if SQLite was built without
    /// FTS5, rather than silently substituting a different search algorithm.
    pub fn new() -> Result<Self> {
        let connection = Connection::open_in_memory()
            .map_err(|error| Error::Protocol(format!("open Sidebar title index: {error}")))?;
        connection
            .execute_batch(
                "CREATE VIRTUAL TABLE sidebar_title_fts USING fts5(
                    endpoint_id UNINDEXED,
                    session_id UNINDEXED,
                    title,
                    tokenize = 'unicode61 remove_diacritics 2'
                );",
            )
            .map_err(|error| {
                Error::Protocol(format!("create Sidebar FTS5 title index: {error}"))
            })?;

        Ok(Self {
            connection,
            source: None,
            revision: None,
            snapshot: BTreeMap::new(),
            corpus_metadata: None,
        })
    }

    /// Atomically replace the complete inventory snapshot for `source`.
    ///
    /// Revisions are monotonic within a source. Equal revisions are accepted
    /// only when they describe the identical address/title map; input order is
    /// irrelevant. A new source starts a new revision sequence.
    pub fn replace_snapshot(
        &mut self,
        source: &SidebarObserverSourceKey,
        revision: u64,
        documents: &[SidebarNameDocument],
    ) -> Result<()> {
        self.replace_snapshot_inner(source, revision, documents, None)
    }

    /// Atomically replace the canonical retained-name corpus and its fencing metadata.
    pub fn replace_corpus_snapshot(
        &mut self,
        source: &SidebarObserverSourceKey,
        revision: u64,
        corpus: &SidebarNameCorpus,
    ) -> Result<()> {
        corpus.validate()?;
        let epochs = BTreeMap::from([(corpus.endpoint_id.clone(), corpus.connection_epoch)]);
        self.replace_ranked_snapshot(
            source,
            revision,
            &corpus.documents,
            epochs,
            corpus.endpoint_domain.clone(),
            corpus.inventory_ready,
        )
    }

    pub(crate) fn replace_ranked_snapshot(
        &mut self,
        source: &SidebarObserverSourceKey,
        revision: u64,
        documents: &[SidebarNameDocument],
        epochs: BTreeMap<String, u64>,
        domain: Vec<String>,
        ready: bool,
    ) -> Result<()> {
        let metadata = Some((epochs, domain, ready));
        self.replace_snapshot_inner(source, revision, documents, metadata)
    }

    fn replace_snapshot_inner(
        &mut self,
        source: &SidebarObserverSourceKey,
        revision: u64,
        documents: &[SidebarNameDocument],
        metadata: Option<(BTreeMap<String, u64>, Vec<String>, bool)>,
    ) -> Result<()> {
        require(
            revision > 0,
            "sidebar name snapshot revision must be nonzero",
        )?;

        let mut next_snapshot = BTreeMap::new();
        for document in documents {
            require(
                !document.address.endpoint_id.trim().is_empty()
                    && !document.address.session_id.trim().is_empty(),
                "sidebar name document requires a complete session address",
            )?;
            require(
                next_snapshot
                    .insert(document.address.clone(), document.title.clone())
                    .is_none(),
                "sidebar name snapshot contains a duplicate session address",
            )?;
        }

        let same_source = self.source.as_ref() == Some(source);
        if same_source {
            let current_revision = self.revision.ok_or_else(|| {
                Error::Protocol("sidebar name source exists without a revision".into())
            })?;
            require(
                revision >= current_revision,
                "sidebar name snapshot revision moved backwards",
            )?;
            if revision == current_revision {
                require(
                    next_snapshot == self.snapshot && metadata == self.corpus_metadata,
                    "sidebar name snapshot or metadata conflicts at the same revision",
                )?;
                return Ok(());
            }

            // Advancing only the watermark is safe when the canonical dataset
            // is unchanged, and avoids rewriting an identical FTS table.
            if next_snapshot == self.snapshot {
                self.corpus_metadata = metadata;
                self.revision = Some(revision);
                return Ok(());
            }
        }

        let transaction = self.connection.transaction().map_err(|error| {
            Error::Protocol(format!("begin Sidebar title snapshot replacement: {error}"))
        })?;
        transaction
            .execute("DELETE FROM sidebar_title_fts", [])
            .map_err(|error| Error::Protocol(format!("clear Sidebar title snapshot: {error}")))?;
        for (address, title) in &next_snapshot {
            transaction
                .execute(
                    "INSERT INTO sidebar_title_fts(endpoint_id, session_id, title)
                     VALUES (?1, ?2, ?3)",
                    params![address.endpoint_id, address.session_id, title],
                )
                .map_err(|error| {
                    Error::Protocol(format!("insert Sidebar title document: {error}"))
                })?;
        }
        transaction.commit().map_err(|error| {
            Error::Protocol(format!(
                "commit Sidebar title snapshot replacement: {error}"
            ))
        })?;

        self.source = Some(source.clone());
        self.revision = Some(revision);
        self.snapshot = next_snapshot;
        self.corpus_metadata = metadata;
        Ok(())
    }

    /// Return full-address title matches using quoted token-prefix AND terms.
    ///
    /// Every Unicode-whitespace-separated query segment is escaped as an FTS5
    /// quoted phrase, suffixed with `*`, and joined by `AND`. Query text is
    /// always passed as a SQL parameter; operators and quotes remain literal
    /// input to the FTS parser.
    pub fn matches(&self, query: &str) -> Result<Vec<SessionAddress>> {
        Ok(self
            .matches_with_scores(query)?
            .into_iter()
            .map(|(address, _)| address)
            .collect())
    }

    /// Return matches with SQLite FTS5 bm25 values (lower values rank first).
    pub fn matches_with_scores(&self, query: &str) -> Result<Vec<(SessionAddress, f64)>> {
        require(
            self.revision.is_some(),
            "sidebar name index has no published snapshot",
        )?;
        let terms: Vec<_> = query.split_whitespace().collect();
        if terms.is_empty() {
            return Ok(Vec::new());
        }

        let expression = terms
            .into_iter()
            .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ");
        let mut statement = self
            .connection
            .prepare(
                "SELECT endpoint_id, session_id, bm25(sidebar_title_fts)
                 FROM sidebar_title_fts
                 WHERE sidebar_title_fts MATCH ?1
                 ORDER BY bm25(sidebar_title_fts), endpoint_id, session_id",
            )
            .map_err(|error| Error::Protocol(format!("prepare Sidebar title query: {error}")))?;
        let rows = statement
            .query_map([expression], |row| {
                Ok((
                    SessionAddress {
                        endpoint_id: row.get(0)?,
                        session_id: row.get(1)?,
                    },
                    row.get::<_, f64>(2)?,
                ))
            })
            .map_err(|error| Error::Protocol(format!("query Sidebar titles: {error}")))?;
        rows.map(|row| {
            row.map_err(|error| Error::Protocol(format!("read Sidebar title result: {error}")))
        })
        .collect()
    }
}

pub(crate) fn legacy_documents(
    rows: &[tekes_agent_components_ui::SidebarRow],
) -> Vec<SidebarNameDocument> {
    rows.iter()
        .map(|row| SidebarNameDocument {
            address: row.address.clone(),
            title: row.title.clone(),
        })
        .collect()
}
