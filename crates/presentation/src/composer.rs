use crate::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
use tekes_agent_contract::{Error, PromptMode, Result, require};
use tekes_agent_session::MemoryProjectionStore;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentFacts {
    pub byte_count: u64,
    /// Public read identity from the upload receipt, distinct from receiptId.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
}
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
        /// Client presentation/policy facts, never added to the prompt content.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        facts: Option<AttachmentFacts>,
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
/// Canonical content identity, independent of stream/text/settings publications.
#[derive(Clone, Debug, PartialEq)]
pub struct ComposerContextBaseline {
    pub selection: SelectionToken,
    pub artifacts: Vec<ComposerArtifact>,
    pub annotations: Vec<ComposerAnnotation>,
    pub context_revision: u64,
}
impl ComposerContextBaseline {
    fn matches_selection(&self, selection: Option<&SelectionToken>) -> bool {
        selection.is_some_and(|current| {
            current.destination == self.selection.destination
                && (!matches!(
                    &self.selection.destination,
                    ComposerDestination::NewThread { .. }
                ) || current.generation == self.selection.generation)
        })
    }
    pub fn matches_snapshot(&self, snapshot: &RunSnapshot) -> bool {
        self.matches_selection(snapshot.selection.as_ref())
            && snapshot.draft_context_revision == self.context_revision
            && (self.context_revision > 0
                || (snapshot.draft.artifacts == self.artifacts
                    && snapshot.draft.annotations == self.annotations))
    }
    pub fn read(snapshot: &RunSnapshot) -> Option<Self> {
        Some(Self {
            selection: snapshot.selection.clone()?,
            artifacts: snapshot.draft.artifacts.clone(),
            annotations: snapshot.draft.annotations.clone(),
            context_revision: snapshot.draft_context_revision,
        })
    }
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
pub struct PreparedAttachmentFacts {
    artifact_id: String,
    facts: Option<AttachmentFacts>,
}
impl PreparedAttachmentFacts {
    pub fn artifact_id(&self) -> &str {
        &self.artifact_id
    }
    pub fn facts(&self) -> Option<&AttachmentFacts> {
        self.facts.as_ref()
    }
}
#[derive(Clone, Debug)]
pub struct PreparedSubmission {
    pub(crate) id: String,
    pub(crate) destination: ComposerDestination,
    pub(crate) plan: SubmissionPlan,
    pub(crate) mode: PromptMode,
    pub(crate) settings: BTreeMap<String, Value>,
    attachment_facts: Vec<PreparedAttachmentFacts>,
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
    pub fn attachment_facts(&self) -> &[PreparedAttachmentFacts] {
        &self.attachment_facts
    }
}
#[derive(Clone)]
struct SubmissionRecord {
    state: SubmissionState,
    draft: ComposerDraft,
    plan: SubmissionPlan,
    attachment_facts: Vec<PreparedAttachmentFacts>,
    claimed: bool,
}
#[derive(Default)]
pub struct PresentationStore {
    pub(crate) selected: Option<ComposerDestination>,
    generation: u64,
    snapshot_revision: u64,
    drafts: BTreeMap<ComposerDestination, ComposerDraft>,
    context_clock: u64,
    context_revisions: BTreeMap<ComposerDestination, u64>,
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
    /// Authority epoch read atomically with this publication's connection/run facts.
    /// Zero is the legacy default; consumers must not replace it with a separately
    /// sampled runtime epoch while retaining this snapshot's revision/content.
    #[serde(default)]
    pub connection_epoch: u64,
    pub selection: Option<SelectionToken>,
    pub connection: ConnectionPresentation,
    pub run: Option<RunPresentation>,
    pub transcript: TranscriptSnapshot,
    pub summary: Option<SummaryFacts>,
    /// Read from the same authority/epoch as this Run and Transcript.
    #[serde(default)]
    pub goal: Option<GoalFacts>,
    pub draft: ComposerDraft,
    /// Per-destination content mutation identity. Zero is a legacy/initial value.
    /// Advances on Send/restoration even when the content collections are empty.
    #[serde(default)]
    pub draft_context_revision: u64,
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
    pub(crate) fn context_revision(&self, destination: &ComposerDestination) -> u64 {
        self.context_revisions
            .get(&self.resolve(destination))
            .copied()
            .unwrap_or_default()
    }
    fn advance_context(&mut self, destination: &ComposerDestination) {
        self.context_clock = self
            .context_clock
            .checked_add(1)
            .expect("draft context revision exhausted");
        self.context_revisions
            .insert(destination.clone(), self.context_clock);
    }
    pub fn edit(&mut self, destination: &ComposerDestination, draft: ComposerDraft) -> Result<()> {
        destination.validate()?;
        let destination = self.resolve(destination);
        let changed = self.drafts.get(&destination).map_or(
            !draft.artifacts.is_empty() || !draft.annotations.is_empty(),
            |old| old.artifacts != draft.artifacts || old.annotations != draft.annotations,
        );
        self.drafts.insert(destination.clone(), draft);
        if changed {
            self.advance_context(&destination);
        }
        Ok(())
    }
    /// Atomically replace only context collections. A Send, selection change or
    /// intervening content edit invalidates the capture; text/settings survive.
    pub fn patch_context(
        &mut self,
        expected: &ComposerContextBaseline,
        artifacts: Vec<ComposerArtifact>,
        annotations: Vec<ComposerAnnotation>,
    ) -> Result<bool> {
        let destination = self.resolve(&expected.selection.destination);
        destination.validate()?;
        if !expected.matches_selection(self.selection().as_ref()) {
            return Ok(false);
        }
        let current = self.draft(&destination);
        if self.context_revision(&destination) != expected.context_revision
            || current.artifacts != expected.artifacts
            || current.annotations != expected.annotations
        {
            return Ok(false);
        }
        let draft = self.drafts.entry(destination.clone()).or_default();
        draft.artifacts = artifacts;
        draft.annotations = annotations;
        self.advance_context(&destination);
        Ok(true)
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
        let attachment_facts: Vec<_> = draft
            .artifacts
            .iter()
            .filter_map(|artifact| match artifact {
                ComposerArtifact::Attachment { id, facts, .. } => Some(PreparedAttachmentFacts {
                    artifact_id: id.clone(),
                    facts: facts.clone(),
                }),
                _ => None,
            })
            .collect();
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
                attachment_facts,
                claimed: false,
            },
        );
        let attachment_facts = self
            .submissions
            .get(&id)
            .expect("newly prepared submission")
            .attachment_facts
            .clone();
        let settings = self.draft(&destination).settings;
        self.drafts.insert(
            destination.clone(),
            ComposerDraft {
                settings: settings.clone(),
                ..Default::default()
            },
        );
        self.advance_context(&destination);
        self.event(state);
        Ok(PreparedSubmission {
            id,
            destination,
            plan,
            mode,
            settings,
            attachment_facts,
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
        let mut restored = false;
        if let Err(message) = outcome {
            record.state.message = Some(message);
            if !record.state.reconciled {
                self.drafts
                    .entry(destination.clone())
                    .or_default()
                    .restore(&record.draft);
                restored = true;
            }
        }
        let state = record.state.clone();
        if restored {
            self.advance_context(&destination);
        }
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
                self.selected = Some(target.clone());
            }
            self.context_revisions.remove(&ticket.destination);
            self.advance_context(&target);
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
        self.context_revisions.remove(&destination);
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
                            .or_else(|| {
                                // Native queued inputs retain the imported user source.
                                let message = &q["message"];
                                if message["role"] == "user" && message["source"]["kind"] == "user"
                                {
                                    message["source"]["rpcId"].as_str()
                                } else {
                                    None
                                }
                            })
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
                    content_blocks: None,
                    reasoning_ended: true,
                    source_sequence: None,
                    source_position: None,
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
        if let Some(session) = scope.and_then(|s| authority.sessions.get(&s.session_id)) {
            present_turn_actions(
                &TurnActionContext {
                    capabilities: &authority.endpoint_capabilities,
                    ready: authority.connection == tekes_agent_session::ConnectionState::Ready
                        && run.as_ref().is_none_or(|run| run.can_submit),
                    running: session_has_active_work(session),
                },
                &mut transcript,
            );
        }
        let summary = match scope
            .and_then(|scope| {
                authority
                    .sessions
                    .get(&scope.session_id)
                    .map(|_| SummaryFacts::read_with_store(scope, authority, &transcript))
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
        let goal = match scope
            .map(|scope| GoalFacts::read(authority, scope))
            .transpose()
        {
            Ok(goal) => goal.flatten(),
            Err(error) => {
                transcript
                    .diagnostics
                    .push(format!("Goal unavailable: {error}"));
                None
            }
        };
        let revision = self.next_snapshot_revision();
        Ok(RunSnapshot {
            revision,
            connection_epoch: authority.connection_epoch,
            selection: self.selection(),
            connection: connection.clone(),
            run: run.clone(),
            transcript,
            summary,
            goal,
            child_availability,
            draft: selected.as_ref().map(|d| self.draft(d)).unwrap_or_default(),
            draft_context_revision: selected
                .as_ref()
                .map(|d| self.context_revision(d))
                .unwrap_or_default(),
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

#[cfg(test)]
mod prepared_attachment_facts_tests {
    use super::*;
    use tekes_agent_contract::EndpointCapabilities;
    use tekes_agent_session::{ConnectionState, SessionProjection};

    fn destination() -> ComposerDestination {
        ComposerDestination::run(RunScope::main("facts-endpoint", "facts-session"))
    }

    fn authority() -> MemoryProjectionStore {
        let mut authority = MemoryProjectionStore::new("facts-endpoint");
        authority.connection = ConnectionState::Ready;
        authority.connection_epoch = 3;
        authority.inventory_ready = true;
        authority.workspace_ready = true;
        authority.control_ready = true;
        authority.actionables_ready = true;
        authority.endpoint_capabilities =
            EndpointCapabilities::methods(&["session.prompt", "commands/run"]);
        authority.sessions.insert(
            "facts-session".into(),
            SessionProjection {
                summary: Some(json!({"sessionId":"facts-session","status":"active"})),
                journal_ready: true,
                ..Default::default()
            },
        );
        authority
    }

    fn attachment(id: &str, content: Value, facts: Option<AttachmentFacts>) -> ComposerArtifact {
        ComposerArtifact::Attachment {
            id: id.into(),
            name: format!("{id}.bin"),
            content,
            facts,
        }
    }

    fn facts(byte_count: u64) -> Option<AttachmentFacts> {
        Some(AttachmentFacts {
            byte_count,
            attachment_id: None,
            media_type: None,
        })
    }

    #[test]
    fn prompt_ticket_freezes_ordered_facts_and_failure_restores_through_original_completion() {
        let destination = destination();
        let file = json!({"type":"file","receiptId":"file-r"});
        let image = json!({"type":"image","mediaType":"image/png","data":"YWJj"});
        let legacy_file = json!({"type":"file","receiptId":"legacy-r"});
        let original = ComposerDraft {
            text: "original prompt".into(),
            artifacts: vec![
                attachment("file-a", file.clone(), facts(12)),
                attachment("image-b", image.clone(), facts(3)),
                attachment("legacy-c", legacy_file.clone(), None),
            ],
            ..Default::default()
        };
        let mut core = PresentationStore::default();
        core.select(destination.clone()).unwrap();
        core.edit(&destination, original.clone()).unwrap();
        let ticket = core
            .prepare(&destination, PromptMode::Queue, &authority())
            .unwrap();

        match ticket.plan() {
            SubmissionPlan::Prompt { content, .. } => {
                assert_eq!(content[0]["type"], "text");
                assert_eq!(&content[1..], &[file, image, legacy_file]);
            }
            _ => panic!("expected prompt plan"),
        }
        let prepared = ticket.attachment_facts();
        assert_eq!(
            prepared
                .iter()
                .map(PreparedAttachmentFacts::artifact_id)
                .collect::<Vec<_>>(),
            ["file-a", "image-b", "legacy-c"]
        );
        assert_eq!(prepared[0].facts().unwrap().byte_count, 12);
        assert_eq!(prepared[1].facts().unwrap().byte_count, 3);
        assert!(
            prepared[2].facts().is_none(),
            "legacy missing facts stay missing"
        );

        core.edit(
            &destination,
            ComposerDraft {
                text: "newer text survives failure".into(),
                artifacts: vec![attachment(
                    "later-image",
                    json!({"type":"image","mediaType":"image/webp","data":"eHl6"}),
                    facts(3),
                )],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(ticket.attachment_facts()[0].artifact_id(), "file-a");
        assert_eq!(ticket.attachment_facts()[0].facts().unwrap().byte_count, 12);

        core.finish(&ticket.id, Err("host preparation failed".into()))
            .unwrap();
        let restored = core.draft(&destination);
        assert_eq!(restored.text, "newer text survives failure");
        assert!(restored.artifacts.iter().any(|a| a.id() == "later-image"));
        assert!(restored.artifacts.iter().any(|a| a.id() == "file-a"));
        assert!(restored.artifacts.iter().any(|a| a.id() == "image-b"));
    }

    #[test]
    fn command_ticket_keeps_attachment_facts_in_plan_order() {
        let destination = destination();
        let first = json!({"type":"image","mediaType":"image/jpeg","data":"Zmlyc3Q="});
        let second = json!({"type":"file","receiptId":"second-r"});
        let mut core = PresentationStore::default();
        core.select(destination.clone()).unwrap();
        core.edit(
            &destination,
            ComposerDraft {
                text: "arguments".into(),
                artifacts: vec![
                    ComposerArtifact::Command {
                        id: "command".into(),
                        name: "inspect".into(),
                        supports_attachments: true,
                    },
                    attachment("first", first.clone(), facts(5)),
                    attachment("second", second.clone(), None),
                ],
                ..Default::default()
            },
        )
        .unwrap();
        let ticket = core
            .prepare(&destination, PromptMode::Queue, &authority())
            .unwrap();

        match ticket.plan() {
            SubmissionPlan::Command { attachments, .. } => {
                assert_eq!(attachments, &[first, second]);
            }
            _ => panic!("expected command plan"),
        }
        let prepared = ticket.attachment_facts();
        assert_eq!(prepared.len(), 2);
        assert_eq!(prepared[0].artifact_id(), "first");
        assert_eq!(prepared[0].facts().unwrap().byte_count, 5);
        assert_eq!(prepared[1].artifact_id(), "second");
        assert!(prepared[1].facts().is_none());
    }
}
