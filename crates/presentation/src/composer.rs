use crate::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
use tekes_agent_contract::{Error, PromptMode, Result, require};
use tekes_agent_session::MemoryProjectionStore;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ComposerArtifact {
    Attachment {
        id: String,
        name: String,
        content: Value,
    },
    Command {
        id: String,
        name: String,
        supports_attachments: bool,
    },
    Skill {
        id: String,
        name: String,
        skill_id: String,
    },
    Reference {
        id: String,
        mention: Option<String>,
        path: Option<String>,
        excerpt: Option<String>,
    },
}
impl ComposerArtifact {
    pub fn id(&self) -> &str {
        match self {
            Self::Attachment { id, .. }
            | Self::Command { id, .. }
            | Self::Skill { id, .. }
            | Self::Reference { id, .. } => id,
        }
    }
    fn is_attachment(&self) -> bool {
        matches!(self, Self::Attachment { .. })
    }
}
pub use tekes_agent_contract::ComposerAnnotation;
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComposerDraft {
    pub text: String,
    pub artifacts: Vec<ComposerArtifact>,
    pub annotations: Vec<ComposerAnnotation>,
    /// Model/preset/permission choices remain host-owned explicit inputs, never inferred from prose.
    pub settings: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SubmissionPlan {
    Prompt {
        content: Vec<Value>,
        display_text: String,
    },
    Command {
        name: String,
        arguments: String,
        attachments: Vec<Value>,
    },
}
impl ComposerDraft {
    pub fn compile(&self, allows_commands: bool) -> Result<SubmissionPlan> {
        let commands: Vec<_> = self
            .artifacts
            .iter()
            .filter(|a| matches!(a, ComposerArtifact::Command { .. }))
            .collect();
        require(commands.len() <= 1, "only one command can be sent")?;
        let attachments: Vec<_> = self
            .artifacts
            .iter()
            .filter_map(|a| {
                if let ComposerArtifact::Attachment { content, .. } = a {
                    Some(content.clone())
                } else {
                    None
                }
            })
            .collect();
        for attachment in &attachments {
            require(
                matches!(attachment["type"].as_str(), Some("file" | "image")),
                "invalid attachment input",
            )?;
        }
        if let Some(ComposerArtifact::Command {
            name,
            supports_attachments,
            ..
        }) = commands.first()
        {
            require(allows_commands, "commands unavailable in this Run")?;
            require(
                !name.is_empty() && !name.contains(char::is_whitespace) && !name.starts_with('/'),
                "invalid command name",
            )?;
            require(
                self.annotations.is_empty()
                    && (self.artifacts.len() == 1
                        || (*supports_attachments
                            && attachments.len() + 1 == self.artifacts.len())),
                "command has incompatible artifacts",
            )?;
            return Ok(SubmissionPlan::Command {
                name: name.clone(),
                arguments: self.text.trim().into(),
                attachments,
            });
        }
        let mut blocks = Vec::new();
        let skills: Vec<_> = self
            .artifacts
            .iter()
            .filter_map(|a| {
                if let ComposerArtifact::Skill { name, skill_id, .. } = a {
                    Some(format!("- Use the skill {name} (id: {skill_id})"))
                } else {
                    None
                }
            })
            .collect();
        if !skills.is_empty() {
            blocks.push(format!(
                "<requested_skills>\n{}\n</requested_skills>",
                skills.join("\n")
            ));
        }
        let mut references = Vec::new();
        for artifact in &self.artifacts {
            if let ComposerArtifact::Reference {
                id,
                mention,
                path,
                excerpt,
            } = artifact
            {
                if let Some(mention) = mention {
                    blocks.push(mention.clone());
                } else if let Some(path) = path {
                    references.push(format!("- Workspace file: {path}"));
                } else if let Some(excerpt) = excerpt {
                    references.push(format!("- Turn {id}:\n\"\"\"\n{excerpt}\n\"\"\""));
                }
            }
        }
        if !references.is_empty() {
            blocks.push(format!(
                "<user_references>\n{}\n</user_references>",
                references.join("\n")
            ));
        }
        let annotated = crate::compile_annotation_input(self.text.trim(), &self.annotations)?;
        if !annotated.is_empty() {
            blocks.push(annotated);
        }
        require(
            !blocks.is_empty() || !attachments.is_empty(),
            "empty submission",
        )?;
        let mut content = Vec::new();
        if !blocks.is_empty() {
            content.push(json!({"type":"text","text":blocks.join("\n\n")}));
        }
        content.extend(attachments);
        Ok(SubmissionPlan::Prompt {
            content,
            display_text: self.text.clone(),
        })
    }
    /// New typing wins. Restore only attachments from older artifacts when newer intent exists.
    fn restore(&mut self, submitted: &Self) {
        let restore_text =
            self.text.is_empty() && self.artifacts.iter().all(ComposerArtifact::is_attachment);
        if restore_text {
            self.text = submitted.text.clone();
        }
        let mut artifacts: Vec<_> = submitted
            .artifacts
            .iter()
            .filter(|a| restore_text || a.is_attachment())
            .filter(|a| !self.artifacts.iter().any(|current| current.id() == a.id()))
            .cloned()
            .collect();
        artifacts.append(&mut self.artifacts);
        self.artifacts = artifacts;
        let mut annotations: Vec<_> = submitted
            .annotations
            .iter()
            .filter(|a| !self.annotations.iter().any(|current| current.id == a.id))
            .cloned()
            .collect();
        annotations.append(&mut self.annotations);
        self.annotations = annotations;
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SubmissionPhase {
    Prepared,
    Dispatched,
    Admitted,
    Failed,
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubmissionState {
    pub id: String,
    pub destination: ComposerDestination,
    pub resolved_scope: Option<RunScope>,
    pub phase: SubmissionPhase,
    pub operation: String,
    pub message: Option<String>,
    pub reconciled: bool,
}
#[derive(Clone, Debug)]
pub struct PreparedSubmission {
    pub(crate) id: String,
    pub(crate) destination: ComposerDestination,
    pub(crate) plan: SubmissionPlan,
    pub(crate) mode: PromptMode,
    pub(crate) settings: BTreeMap<String, Value>,
}
impl PreparedSubmission {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn destination(&self) -> &ComposerDestination {
        &self.destination
    }
    pub fn plan(&self) -> &SubmissionPlan {
        &self.plan
    }
    pub fn settings(&self) -> &BTreeMap<String, Value> {
        &self.settings
    }
}
#[derive(Clone)]
struct SubmissionRecord {
    state: SubmissionState,
    draft: ComposerDraft,
    plan: SubmissionPlan,
    claimed: bool,
}
#[derive(Default)]
pub struct PresentationStore {
    pub(crate) selected: Option<ComposerDestination>,
    generation: u64,
    snapshot_revision: u64,
    drafts: BTreeMap<ComposerDestination, ComposerDraft>,
    submissions: BTreeMap<String, SubmissionRecord>,
    pub(crate) events: VecDeque<SubmissionState>,
    was_connected: bool,
    aliases: BTreeMap<ComposerDestination, ComposerDestination>,
    connection_epoch: u64,
    child_catalog: Option<(SelectionToken, tekes_agent_contract::SubagentCatalog)>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunSnapshot {
    /// Monotonic canonical publication order for this PresentationStore. Zero
    /// denotes a legacy unversioned serialized snapshot, not a UI arrival clock.
    #[serde(default)]
    pub revision: u64,
    pub selection: Option<SelectionToken>,
    pub connection: ConnectionPresentation,
    pub run: Option<RunPresentation>,
    pub transcript: TranscriptSnapshot,
    pub summary: Option<SummaryFacts>,
    pub draft: ComposerDraft,
    pub can_submit: bool,
    /// Derived from the same catalog authority that gates child controls. None
    /// for a main run; older serialized snapshots may omit this presentation.
    #[serde(default)]
    pub child_availability: Option<ChildComposerAvailability>,
    pub submissions: Vec<SubmissionState>,
    pub events: Vec<SubmissionState>,
}
impl PresentationStore {
    pub(crate) fn next_snapshot_revision(&mut self) -> u64 {
        self.snapshot_revision = self
            .snapshot_revision
            .checked_add(1)
            .expect("snapshot revision exhausted");
        self.snapshot_revision
    }
    fn resolve(&self, destination: &ComposerDestination) -> ComposerDestination {
        self.aliases.get(destination).unwrap_or(destination).clone()
    }
    fn owner(&self, record: &SubmissionRecord) -> ComposerDestination {
        record
            .state
            .resolved_scope
            .as_ref()
            .map(|scope| ComposerDestination::run(scope.clone()))
            .unwrap_or_else(|| self.resolve(&record.state.destination))
    }
    pub fn cancel_prepared(&mut self, ticket: &PreparedSubmission) -> Result<()> {
        let record = self
            .submissions
            .get(&ticket.id)
            .ok_or_else(|| Error::Protocol("unknown submission".into()))?;
        require(
            record.state.phase == SubmissionPhase::Prepared && !record.claimed,
            "submission dispatch already claimed",
        )?;
        self.finish(&ticket.id, Err("submission preparation cancelled".into()))
    }
    pub fn select(&mut self, destination: ComposerDestination) -> Result<SelectionToken> {
        destination.validate()?;
        // Acquiring a new-thread destination again starts the next draft, never an alias to the last Run.
        self.aliases.remove(&destination);
        self.aliases.retain(|_, target| target != &destination);
        self.generation = self.generation.wrapping_add(1);
        self.selected = Some(destination);
        Ok(SelectionToken {
            destination: self.selected.clone().unwrap(),
            generation: self.generation,
        })
    }
    pub fn selection(&self) -> Option<SelectionToken> {
        self.selected.as_ref().map(|d| SelectionToken {
            destination: d.clone(),
            generation: self.generation,
        })
    }
    pub fn draft(&self, destination: &ComposerDestination) -> ComposerDraft {
        self.drafts
            .get(&self.resolve(destination))
            .cloned()
            .unwrap_or_default()
    }
    pub fn edit(&mut self, destination: &ComposerDestination, draft: ComposerDraft) -> Result<()> {
        destination.validate()?;
        self.drafts.insert(self.resolve(destination), draft);
        Ok(())
    }
    /// Apply only authored text. A delayed editor callback cannot erase newer host attachments,
    /// annotations or settings, and retains its captured destination across selection changes.
    pub fn edit_text(&mut self, destination: &ComposerDestination, text: String) -> Result<()> {
        destination.validate()?;
        self.drafts
            .entry(self.resolve(destination))
            .or_default()
            .text = text;
        Ok(())
    }
    /// Asynchronous model/catalog reads must carry both scope and generation (A -> B -> A is stale).
    pub fn commit_settings(
        &mut self,
        token: &SelectionToken,
        settings: BTreeMap<String, Value>,
        cancelled: bool,
    ) -> bool {
        if cancelled || self.selection().as_ref() != Some(token) {
            return false;
        }
        self.drafts
            .entry(token.destination.clone())
            .or_default()
            .settings = settings;
        true
    }
    fn event(&mut self, state: SubmissionState) {
        if self.events.len() == 256 {
            self.events.pop_front();
        }
        self.events.push_back(state);
    }
    pub fn prepare(
        &mut self,
        destination: &ComposerDestination,
        mode: PromptMode,
        authority: &MemoryProjectionStore,
    ) -> Result<PreparedSubmission> {
        let destination = self.resolve(destination);
        destination.validate()?;
        require(
            destination.endpoint_id() == authority.endpoint_id,
            "submission endpoint mismatch",
        )?;
        let allowed = match &destination {
            ComposerDestination::Run { scope } => {
                let child_allowed = scope.thread_id == scope.session_id
                    || (self.connection_epoch == authority.connection_epoch
                        && authority.endpoint_capabilities.supports("subagents.prompt")
                        && self.child_catalog.as_ref().is_some_and(|(token, catalog)| {
                            self.selection().as_ref() == Some(token)
                                && token.destination == destination
                                && catalog.require_continuable(&scope.session_id).is_ok()
                        }));
                child_allowed
                    && RunPresentation::read(authority, scope, self.was_connected).can_submit
            }
            ComposerDestination::NewThread { .. } => {
                ConnectionPresentation::read(authority, None, self.was_connected).can_submit
            }
        };
        require(allowed, "Run is not ready for input")?;
        require(
            !self.submissions.values().any(|r| {
                self.owner(r) == destination
                    && (matches!(
                        r.state.phase,
                        SubmissionPhase::Prepared
                            | SubmissionPhase::Dispatched
                            | SubmissionPhase::Unknown
                    ) && !r.state.reconciled)
            }),
            "submission requires reconciliation",
        )?;
        let draft = self.draft(&destination);
        let plan = draft.compile(
            authority.endpoint_capabilities.supports("commands/run")
                || authority.endpoint_capabilities.supports("commands.execute"),
        )?;
        require(
            match &plan {
                SubmissionPlan::Prompt { .. } => {
                    authority.endpoint_capabilities.supports("session.prompt")
                }
                SubmissionPlan::Command { .. } => true,
            },
            "submission capability unavailable",
        )?;
        let id = uuid::Uuid::new_v4().to_string();
        let state = SubmissionState {
            id: id.clone(),
            destination: destination.clone(),
            resolved_scope: None,
            phase: SubmissionPhase::Prepared,
            operation: if matches!(plan, SubmissionPlan::Prompt { .. }) {
                "prompt"
            } else {
                "command"
            }
            .into(),
            message: None,
            reconciled: false,
        };
        self.submissions.insert(
            id.clone(),
            SubmissionRecord {
                state: state.clone(),
                draft,
                plan: plan.clone(),
                claimed: false,
            },
        );
        let settings = self.draft(&destination).settings;
        self.drafts.insert(
            destination.clone(),
            ComposerDraft {
                settings: settings.clone(),
                ..Default::default()
            },
        );
        self.event(state);
        Ok(PreparedSubmission {
            id,
            destination,
            plan,
            mode,
            settings,
        })
    }
    pub(crate) fn claim(&mut self, ticket: &PreparedSubmission) -> Result<()> {
        let record = self
            .submissions
            .get_mut(&ticket.id)
            .ok_or_else(|| Error::Protocol("unknown submission".into()))?;
        require(
            record.state.phase == SubmissionPhase::Prepared && !record.claimed,
            "submission dispatch already claimed",
        )?;
        record.claimed = true;
        Ok(())
    }
    pub fn mark_dispatched(&mut self, ticket: &PreparedSubmission) -> Result<()> {
        let record = self
            .submissions
            .get_mut(&ticket.id)
            .ok_or_else(|| Error::Protocol("unknown submission".into()))?;
        require(
            record.state.phase == SubmissionPhase::Prepared,
            "submission already dispatched",
        )?;
        record.state.phase = SubmissionPhase::Dispatched;
        Ok(())
    }
    pub fn finish(&mut self, id: &str, outcome: std::result::Result<(), String>) -> Result<()> {
        let destination = self.owner(
            self.submissions
                .get(id)
                .ok_or_else(|| Error::Protocol("unknown submission".into()))?,
        );
        let record = self.submissions.get_mut(id).expect("owned submission");
        if matches!(
            record.state.phase,
            SubmissionPhase::Admitted | SubmissionPhase::Failed | SubmissionPhase::Unknown
        ) {
            return Ok(());
        }
        record.state.phase = match &outcome {
            Ok(()) => SubmissionPhase::Admitted,
            Err(_) if record.state.phase == SubmissionPhase::Dispatched => SubmissionPhase::Unknown,
            Err(_) => SubmissionPhase::Failed,
        };
        if let Err(message) = outcome {
            record.state.message = Some(message);
            if !record.state.reconciled {
                self.drafts
                    .entry(destination)
                    .or_default()
                    .restore(&record.draft);
            }
        }
        let state = record.state.clone();
        self.event(state);
        Ok(())
    }
    pub fn materialize(&mut self, ticket: &PreparedSubmission, scope: RunScope) -> Result<()> {
        scope.validate()?;
        require(
            scope.endpoint_id == ticket.destination.endpoint_id(),
            "materialized endpoint mismatch",
        )?;
        let target = ComposerDestination::run(scope.clone());
        let record = self
            .submissions
            .get_mut(&ticket.id)
            .ok_or_else(|| Error::Protocol("unknown submission".into()))?;
        require(
            record.state.resolved_scope.is_none(),
            "submission already materialized",
        )?;
        if ticket.destination != target {
            require(
                matches!(ticket.destination, ComposerDestination::NewThread { .. }),
                "cannot redirect a Run submission",
            )?;
            require(
                !self.drafts.contains_key(&target),
                "materialized draft already exists",
            )?;
            record.state.resolved_scope = Some(scope.clone());
            let draft = self.drafts.remove(&ticket.destination).unwrap_or_default();
            self.drafts.insert(target.clone(), draft);
            self.aliases
                .insert(ticket.destination.clone(), target.clone());
            if self.selected.as_ref() == Some(&ticket.destination) {
                self.generation = self.generation.wrapping_add(1);
                self.selected = Some(target);
            }
        }
        self.submissions
            .get_mut(&ticket.id)
            .expect("owned submission")
            .state
            .resolved_scope = Some(scope);
        Ok(())
    }
    /// Explicit host reconciliation after checking native history; never inferred from matching prose.
    pub fn acknowledge_reconciliation(&mut self, id: &str) -> Result<()> {
        self.submissions
            .get_mut(id)
            .ok_or_else(|| Error::Protocol("unknown submission".into()))?
            .state
            .reconciled = true;
        Ok(())
    }
    pub fn commit_child_catalog(
        &mut self,
        token: &SelectionToken,
        connection_epoch: u64,
        result: std::result::Result<tekes_agent_contract::SubagentCatalog, String>,
    ) -> bool {
        if self.selection().as_ref() != Some(token) || self.connection_epoch != connection_epoch {
            return false;
        }
        self.child_catalog = result.ok().map(|catalog| (token.clone(), catalog));
        true
    }
    /// Call only after authoritative discard/absence. Late completions cannot resurrect a draft.
    pub fn forget_destination(&mut self, destination: &ComposerDestination) {
        let destination = self.resolve(destination);
        self.drafts.remove(&destination);
        let ids: Vec<_> = self
            .submissions
            .iter()
            .filter(|(_, r)| self.owner(r) == destination)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            let record = self.submissions.get_mut(&id).expect("owned record");
            record.state.reconciled = true;
            if record.state.phase == SubmissionPhase::Prepared {
                record.state.phase = SubmissionPhase::Failed;
            }
        }
        self.aliases
            .retain(|key, value| key != &destination && value != &destination);
        if self.selected.as_ref() == Some(&destination) {
            self.selected = None;
            self.generation += 1;
        }
    }
    pub fn refresh(&mut self, authority: &MemoryProjectionStore) -> Result<RunSnapshot> {
        if let Some(destination) = &self.selected {
            require(
                destination.endpoint_id() == authority.endpoint_id,
                "selected endpoint mismatch",
            )?;
        }
        let selected = self.selected.clone();
        let scope = selected.as_ref().and_then(|d| {
            if let ComposerDestination::Run { scope } = d {
                Some(scope)
            } else {
                None
            }
        });
        if self.connection_epoch != authority.connection_epoch {
            self.child_catalog = None;
            self.connection_epoch = authority.connection_epoch;
        }
        if !matches!(
            authority.connection,
            tekes_agent_session::ConnectionState::Ready
        ) {
            self.child_catalog = None;
        }
        let mut run = scope.map(|s| RunPresentation::read(authority, s, self.was_connected));
        let mut child_availability = None;
        if let Some(run) = &mut run
            && run.scope.thread_id != run.scope.session_id
        {
            let catalog = self
                .child_catalog
                .as_ref()
                .filter(|(token, _)| Some(token.clone()) == self.selection())
                .map(|(_, catalog)| catalog);
            let availability = ChildComposerAvailability::resolve(catalog, &run.scope.session_id);
            let available = availability == ChildComposerAvailability::Available;
            child_availability = Some(availability);
            run.can_submit &=
                available && authority.endpoint_capabilities.supports("subagents.prompt");
            run.can_cancel &=
                available && authority.endpoint_capabilities.supports("subagents.cancel");
        }
        let mut transcript = scope
            .and_then(|s| {
                authority
                    .sessions
                    .get(&s.session_id)
                    .map(|session| TranscriptSnapshot::read(s, session))
            })
            .transpose()?
            .unwrap_or_default();
        let mut reconciliation_ids: Vec<_> =
            transcript.reconciliation_ids().map(String::from).collect();
        if let Some(run) = &run {
            reconciliation_ids.extend(
                run.queue
                    .iter()
                    .filter_map(|q| {
                        q["rpcId"]
                            .as_str()
                            .or_else(|| q["message"]["rpcId"].as_str())
                    })
                    .map(String::from),
            );
        }
        for id in reconciliation_ids {
            if let Some(record) = self.submissions.get_mut(&id) {
                let owner = record
                    .state
                    .resolved_scope
                    .as_ref()
                    .map(|s| ComposerDestination::run(s.clone()))
                    .unwrap_or_else(|| record.state.destination.clone());
                if selected.as_ref() == Some(&owner) {
                    record.state.reconciled = true;
                }
            }
        }
        let submissions: Vec<_> = self
            .submissions
            .values()
            .filter(|r| selected.as_ref() == Some(&self.owner(r)))
            .map(|r| r.state.clone())
            .collect();
        for record in self
            .submissions
            .values()
            .filter(|r| !r.state.reconciled && selected.as_ref() == Some(&self.owner(r)))
        {
            if let SubmissionPlan::Prompt { display_text, .. } = &record.plan {
                let turn_id = format!("echo-turn:{}", record.state.id);
                transcript.turns.push(TranscriptTurn {
                    id: turn_id.clone(),
                    status: if matches!(
                        record.state.phase,
                        SubmissionPhase::Failed | SubmissionPhase::Unknown
                    ) {
                        "error"
                    } else {
                        "running"
                    }
                    .into(),
                    input: display_text.clone(),
                    started_at: None,
                    ended_at: None,
                    usage: TokenUsage::default(),
                    fork_boundary: None,
                    footnote_actions: Vec::new(),
                    live_usage: None,
                    reported_usage: None,
                });
                transcript.items.push(TranscriptItem {
                    id: format!("echo-item:{}", record.state.id),
                    record_id: None,
                    reconciliation_id: Some(record.state.id.clone()),
                    turn_id,
                    kind: TranscriptItemKind::User,
                    text: display_text.clone(),
                    reasoning: String::new(),
                    attachments: record.draft.artifacts.iter().filter_map(|artifact| {
                        if let ComposerArtifact::Attachment { content, .. } = artifact { Some(content.clone()) }
                        else { None }
                    }).collect(),
                    is_visible: true,
                    is_final: false,
                    is_provisional: true,
                    reasoning_ended: true,
                    source_sequence: None,
                    time: None,
                    metadata: json!({"submissionPhase":record.state.phase,"message":record.state.message}),
                });
            }
        }
        let connection = ConnectionPresentation::read(
            authority,
            scope.map(|s| s.session_id.as_str()),
            self.was_connected,
        );
        if connection.phase == ConnectionPhase::Connected {
            self.was_connected = true;
        }
        transcript.is_stale |= connection.history_is_stale;
        let blocked = submissions.iter().any(|s| {
            !s.reconciled
                && matches!(
                    s.phase,
                    SubmissionPhase::Prepared
                        | SubmissionPhase::Dispatched
                        | SubmissionPhase::Unknown
                )
        });
        let summary = match scope
            .and_then(|scope| {
                authority
                    .sessions
                    .get(&scope.session_id)
                    .map(|session| SummaryFacts::read(scope, session, &transcript))
            })
            .transpose()
        {
            Ok(summary) => summary,
            Err(error) => {
                transcript
                    .diagnostics
                    .push(format!("Summary unavailable: {error}"));
                None
            }
        };
        let revision = self.next_snapshot_revision();
        Ok(RunSnapshot {
            revision,
            selection: self.selection(),
            connection: connection.clone(),
            run: run.clone(),
            transcript,
            summary,
            child_availability,
            draft: selected.as_ref().map(|d| self.draft(d)).unwrap_or_default(),
            can_submit: selected.is_some()
                && run
                    .as_ref()
                    .map(|r| r.can_submit)
                    .unwrap_or(connection.can_submit)
                && !blocked,
            submissions,
            events: self.events.iter().cloned().collect(),
        })
    }
}
