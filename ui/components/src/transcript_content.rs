//! A canonical mixed message content tree. Scrolling/reading transactions belong
//! to TekesUI; this component neither invents a scroll state nor adjusts pixels.
use crate::{
    FeedbackGesture, FeedbackLabels, InputMessage, OutputMessage, OutputMessageLabels,
    ToolActivity, ToolActivityLabels, ToolActivityPlacement, ToolRun, ToolRunLabels,
    TranscriptMessageIdentity, TurnFeedback, TurnFootnote, TurnFootnoteIntent, TurnFootnoteLabels,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};
use tekes_agent_presentation::{
    RunScope, TranscriptBlockContent, TranscriptDelivery, TranscriptDocument, TranscriptInput,
    TranscriptItemKind, TranscriptMessageKey, TranscriptOrderedEvent, TranscriptOutput,
    TranscriptSection, TranscriptSnapshot, TranscriptSourcePosition, TranscriptToolCall,
    TranscriptTurn, TurnFeedbackTarget,
};
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    extended::{Axis, Stack},
    layout::{ContainerLayout, Insets, MeasuredChild},
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::{Spacing, proposal};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TranscriptContentRowKey {
    Message(TranscriptMessageIdentity),
    Tool(String),
    TurnFootnote(String),
    /// Compatibility lookup for the original nested feedback control. This is
    /// no longer a separate reading row; use feedback_controls() for callbacks.
    TurnFeedback(String),
    Unplaced(String),
    SessionNotices,
    Diagnostics,
    Stale,
}
/// A view-local, borrowed publication plan. Row IDs are predicted without
/// reserving namespaces or changing controls. This is not endpoint/read authority.
#[derive(Debug)]
pub struct TranscriptContentUpdate<'a> {
    root: ComponentId,
    scope: RunScope,
    base_publication: u64,
    base_epoch: u64,
    base_next: u32,
    publication: u64,
    epoch: u64,
    snapshot: &'a TranscriptSnapshot,
    available: bool,
    timing: crate::TranscriptFootnoteTiming,
    base_timing: crate::TranscriptFootnoteTiming,
    base_historical_generation: u64,
    base_header_revision: (u64, u64),
    header_revision: (u64, u64),
    rows: Vec<(TranscriptContentRowKey, ComponentId)>,
    thinking_input: Option<crate::TranscriptThinkingDisplayInput>,
    base_applied_thinking: Option<crate::TranscriptThinkingDisplayInput>,
}
impl TranscriptContentUpdate<'_> {
    pub fn rows(&self) -> &[(TranscriptContentRowKey, ComponentId)] {
        &self.rows
    }
    pub fn item_ids(&self) -> Vec<ComponentId> {
        self.rows.iter().map(|(_, id)| *id).collect()
    }
    pub fn publication(&self) -> u64 {
        self.publication
    }
    pub fn connection_epoch(&self) -> u64 {
        self.epoch
    }
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
}
#[derive(Clone, Debug)]
pub struct TranscriptContentLabels {
    pub group: String,
    pub unplaced: String,
    pub notices: String,
    pub stale: String,
    pub input_details: OutputMessageLabels,
    pub input_information: fn(TranscriptDelivery) -> String,
    pub input_copy: String,
    pub output: OutputMessageLabels,
    pub tool: ToolActivityLabels,
    pub tool_run: ToolRunLabels,
    pub footnote: TurnFootnoteLabels,
    pub feedback: Option<FeedbackLabels>,
}
impl TranscriptContentLabels {
    /// One explicit report source for standalone and message-embedded tools.
    /// Input-message disclosures and non-tool labels retain their existing copy.
    pub fn with_tool_header(mut self, adapter: crate::ToolHeaderAdapter) -> Self {
        self.output.tool_header = Some(adapter);
        self.tool.details.tool_header = Some(adapter);
        self
    }
}
impl Default for TranscriptContentLabels {
    fn default() -> Self {
        Self {
            group: "会话内容".into(),
            unplaced: "部分事件尚未提供公开位置".into(),
            notices: "会话提示".into(),
            stale: "历史待同步".into(),
            input_details: InputMessage::default_details_labels(),
            input_copy: "复制输入".into(),
            input_information: |delivery| match delivery {
                TranscriptDelivery::Pending => "发送中".into(),
                TranscriptDelivery::Recorded { .. } => "已发送".into(),
            },
            output: Default::default(),
            tool: Default::default(),
            tool_run: Default::default(),
            footnote: Default::default(),
            feedback: None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptContentCapabilities {
    pub mixed_content: bool,
    pub keyed_reading_transactions: bool,
    pub rich_content: bool,
    pub structured_user_content: bool,
}
/// Real content composition, independently mountable. It is not the complete
/// Transcript viewport or RunPort. See capabilities()/the upstream #22/#25 gates.
#[derive(Debug)]
pub struct TranscriptContent {
    root: ComponentId,
    scope: RunScope,
    rows: StackContent,
    keys: Vec<TranscriptContentRowKey>,
    /// Only unresolved operations survive clipping; not a second session cache.
    detached: BTreeMap<TranscriptContentRowKey, Content>,
    start: u32,
    next: u32,
    end: u32,
    publication: u64,
    epoch: u64,
    header_revision: (u64, u64),
    last: Option<TranscriptSnapshot>,
    labels: TranscriptContentLabels,
    timing: crate::TranscriptFootnoteTiming,
    controls_dirty: bool,
    run_layout: Option<RunTranscriptLayout>,
    arranged_revision: Option<(u64, u64)>,
    turn_target_owner: std::sync::Arc<()>,
    historical_control: Option<crate::HistoricalInputControl>,
    historical_generation: u64,
    thinking_input: Option<crate::TranscriptThinkingDisplayInput>,
    applied_thinking: Option<crate::TranscriptThinkingDisplayInput>,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct RunTranscriptLayout {
    pub column_x: f32,
    pub column_width: f32,
    pub top: f32,
    pub bottom: f32,
}
impl TranscriptContent {
    fn current_header_revision(&self) -> (u64, u64) {
        (
            crate::output_message::tool_header_revision(self.labels.output, &self.scope),
            crate::output_message::tool_header_revision(self.labels.tool.details, &self.scope),
        )
    }
    fn checked_header_revision(&self, epoch: u64) -> Result<(u64, u64), &'static str> {
        let next = self.current_header_revision();
        if epoch == self.epoch
            && (next.0 < self.header_revision.0 || next.1 < self.header_revision.1)
        {
            return Err("stale transcript tool header revision");
        }
        Ok(next)
    }
    pub(crate) fn set_run_layout(&mut self, layout: RunTranscriptLayout) {
        self.run_layout = Some(layout);
    }
    /// The host grants an exclusive row namespace range. Output asides reserve
    /// two namespaces (footer and original feedback); other rows reserve one.
    /// IDs never recycle after removal. Reserve this range across all Kit controls.
    pub fn new(
        root_namespace: u32,
        row_namespaces: Range<u32>,
        scope: RunScope,
        labels: TranscriptContentLabels,
    ) -> Result<Self, &'static str> {
        if row_namespaces.is_empty() || row_namespaces.contains(&root_namespace) {
            return Err("invalid transcript namespace range");
        }
        scope.validate().map_err(|_| "invalid transcript scope")?;
        Ok(Self {
            root: ComponentId((root_namespace as u64) << 32),
            scope,
            rows: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: None,
                children: vec![],
            },
            keys: vec![],
            detached: BTreeMap::new(),
            start: row_namespaces.start,
            next: row_namespaces.start,
            end: row_namespaces.end,
            publication: 0,
            epoch: 0,
            header_revision: (0, 0),
            last: None,
            labels,
            timing: Default::default(),
            controls_dirty: false,
            run_layout: None,
            arranged_revision: None,
            turn_target_owner: std::sync::Arc::new(()),
            historical_control: None,
            historical_generation: 0,
            thinking_input: None,
            applied_thinking: None,
        })
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    /// Controlled display preference/tick, independent of Run authority. Call
    /// before preparing the original reading transaction, then publish normally.
    /// Once enabled it cannot silently fall back to uncontrolled reasoning.
    pub fn set_thinking_display(
        &mut self,
        input: crate::TranscriptThinkingDisplayInput,
    ) -> Result<(), &'static str> {
        if input.display_revision == 0 {
            return Err("thinking display revision must be nonzero");
        }
        if let Some(old) = self.thinking_input {
            if input.display_revision < old.display_revision {
                return Err("stale thinking display input");
            }
            if input.display_revision == old.display_revision && input != old {
                return Err("conflicting thinking display input");
            }
        }
        self.thinking_input = Some(input);
        Ok(())
    }
    pub fn thinking_display(&self) -> Option<crate::TranscriptThinkingDisplayInput> {
        self.thinking_input
    }
    fn plan_with_thinking<'a>(
        &self,
        snapshot: &TranscriptSnapshot,
        document: &'a TranscriptDocument<'a>,
        timing: &crate::TranscriptFootnoteTiming,
        publication: u64,
        epoch: u64,
        display: Option<crate::TranscriptThinkingDisplayInput>,
    ) -> Result<Vec<Planned<'a>>, &'static str> {
        let mut rows = plan(snapshot, document, &self.labels, timing)?;
        if let Some(input) = display {
            let mut frames = BTreeMap::new();
            for row in &mut rows {
                if let PlannedContent::Output(output, Some(section)) = &row.content
                    && output.item.kind == TranscriptItemKind::Assistant
                {
                    if !frames.contains_key(section.id) {
                        let frame = snapshot
                            .thinking_frame(&self.scope, section.id, publication, epoch, input)
                            .map_err(|_| "transcript thinking source unavailable")?;
                        frames.insert(section.id, frame);
                    }
                    row.thinking = frames.get(section.id).cloned();
                }
            }
        }
        // No empty layout gap or false tool-run boundary for reasoning whose
        // controlled display has no nodes. Only proven reasoning-only content
        // may disappear; failures, tools, unknown/legacy content stay explicit.
        rows.retain(|row| !matches!((&row.content, &row.thinking),
            (PlannedContent::Output(output, _), Some(frame))
                if output.item.failure_message().is_none()
                    && output.item.attachments.is_empty()
                    && output.item.content_blocks.as_ref().is_some_and(|blocks|
                        !blocks.is_empty() && blocks.iter().all(|block|
                            matches!(&block.content, TranscriptBlockContent::Reasoning { .. })))
                    && (output.item.display_reasoning().trim().is_empty()
                        || frame.display_for(&output.item.id) == Some(crate::TranscriptReasoningDisplay::Hidden))));
        Ok(group_tools(rows))
    }
    pub fn footnote_timing(&self) -> &crate::TranscriptFootnoteTiming {
        &self.timing
    }
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    pub fn capabilities(&self) -> TranscriptContentCapabilities {
        TranscriptContentCapabilities {
            mixed_content: true,
            keyed_reading_transactions: false,
            rich_content: false,
            structured_user_content: true,
        }
    }
    pub fn row_id(&self, key: &TranscriptContentRowKey) -> Option<ComponentId> {
        if let TranscriptContentRowKey::TurnFeedback(turn) = key {
            return self
                .rows
                .children
                .iter()
                .filter_map(|node| node.component::<TurnFootnote>())
                .find(|footer| footer.facts().is_some_and(|facts| &facts.turn_id == turn))
                .and_then(TurnFootnote::feedback)
                .map(|control| control.ids().root);
        }
        self.keys
            .iter()
            .position(|value| value == key)
            .map(|i| self.rows.children[i].id())
            .or_else(|| {
                if let TranscriptContentRowKey::Tool(id) = key {
                    self.tool_run(id).map(ToolRun::id)
                } else {
                    None
                }
            })
    }
    pub fn message_id(&self, key: TranscriptMessageKey<'_>) -> Option<ComponentId> {
        self.row_id(&TranscriptContentRowKey::Message(key.into()))
    }
    pub fn row_keys(&self) -> &[TranscriptContentRowKey] {
        &self.keys
    }
    /// Rail facts from the installed canonical publication and original rows.
    /// Empty-input turns are omitted exactly as in ClientKit's rail; orphan and
    /// session-wide rows never acquire an invented turn.
    pub fn turn_targets(&self) -> Result<Vec<crate::TranscriptTurnTarget>, &'static str> {
        let Some(snapshot) = &self.last else {
            return Ok(vec![]);
        };
        let document =
            TranscriptDocument::read(snapshot).map_err(|_| "invalid transcript document")?;
        let planned = self.plan_with_thinking(
            snapshot,
            &document,
            &self.timing,
            self.publication,
            self.epoch,
            self.applied_thinking,
        )?;
        let mounted: BTreeMap<_, _> = self.keys.iter().zip(&self.rows.children).collect();
        let mut rows: BTreeMap<&str, (Vec<ComponentId>, bool)> = BTreeMap::new();
        for row in &planned {
            let turn = match &row.content {
                PlannedContent::Input(input) => Some(input.item.turn_id.as_str()),
                PlannedContent::Output(output, section) => {
                    section.map(|_| output.item.turn_id.as_str())
                }
                PlannedContent::Tools(tools) => {
                    tools.first().map(|(tool, _)| tool.turn_id.as_str())
                }
                PlannedContent::Footnote(turn, _, _, _) => Some(turn.id.as_str()),
                PlannedContent::Label(_) => match &row.key {
                    TranscriptContentRowKey::Unplaced(turn) => Some(turn.as_str()),
                    _ => None,
                },
            };
            if let Some(turn) = turn
                && let Some(node) = mounted.get(&row.key)
            {
                let (ids, visible) = rows.entry(turn).or_default();
                ids.push(node.id());
                *visible |= node
                    .visible()
                    .is_some_and(|r| r.size.width > 0. && r.size.height > 0.);
            }
        }
        let arranged = self.arranged_revision == Some((self.publication, self.epoch));
        // Match the ClientKit AgentKit bridge: assistant items in canonical
        // source order, including empty items. Reasoning/tools/errors are not
        // promoted to the turn's output preview. Group once for all targets.
        let mut outputs: BTreeMap<&str, Vec<&tekes_agent_presentation::TranscriptItem>> =
            BTreeMap::new();
        for item in &snapshot.items {
            if item.kind == TranscriptItemKind::Assistant {
                outputs.entry(&item.turn_id).or_default().push(item);
            }
        }
        Ok(snapshot
            .turns
            .iter()
            .filter(|turn| !turn.input.trim().is_empty())
            .map(|turn| {
                let (ids, visible) = rows.remove(turn.id.as_str()).unwrap_or_default();
                let visible = arranged.then_some(visible);
                crate::TranscriptTurnTarget {
                    owner: self.turn_target_owner.clone(),
                    scope: self.scope.clone(),
                    publication: self.publication,
                    epoch: self.epoch,
                    turn_id: turn.id.clone(),
                    preview: crate::TranscriptTurnPreview::read(
                        turn,
                        outputs
                            .get(turn.id.as_str())
                            .map(Vec::as_slice)
                            .unwrap_or_default(),
                    ),
                    status: turn.status.clone(),
                    rows: ids,
                    visible,
                }
            })
            .collect())
    }
    /// Recheck a captured local target before calling a public library reveal
    /// service. This check does not perform scrolling or grant endpoint authority.
    pub fn validate_turn_target(
        &self,
        target: &crate::TranscriptTurnTarget,
    ) -> Result<(), &'static str> {
        if !std::sync::Arc::ptr_eq(&target.owner, &self.turn_target_owner)
            || target.scope != self.scope
            || target.publication != self.publication
            || target.epoch != self.epoch
        {
            return Err("stale or foreign transcript turn target");
        }
        if !self.turn_targets()?.iter().any(|current| {
            current.turn_id == target.turn_id
                && current.preview == target.preview
                && current.status == target.status
                && current.rows == target.rows
                && !current.rows.is_empty()
        }) {
            return Err("transcript turn target is no longer present");
        }
        Ok(())
    }
    pub fn retained_operation_count(&self) -> usize {
        self.detached.len()
    }
    pub fn update(
        &mut self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        snapshot: &TranscriptSnapshot,
        available: bool,
    ) -> Result<(), &'static str> {
        self.update_with_timing(
            scope,
            publication,
            epoch,
            snapshot,
            available,
            &Default::default(),
        )
    }
    pub fn update_with_timing(
        &mut self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        snapshot: &TranscriptSnapshot,
        available: bool,
        timing: &crate::TranscriptFootnoteTiming,
    ) -> Result<(), &'static str> {
        self.check_timing(publication, epoch, snapshot, timing)?;

        if scope != &self.scope {
            return Err("transcript content scope mismatch");
        }
        if publication == 0 || publication < self.publication || epoch < self.epoch {
            return Err("stale transcript content publication");
        }
        if publication == self.publication
            && (epoch != self.epoch || self.last.as_ref() != Some(snapshot))
        {
            return Err("conflicting transcript content publication");
        }
        let header_revision = self.checked_header_revision(epoch)?;
        let available = available && !snapshot.is_stale;
        if !self.controls_dirty
            && self.historical_control.is_none()
            && self.epoch == epoch
            && self.last.as_ref() == Some(snapshot)
            && &self.timing == timing
            && self.header_revision == header_revision
            && self.applied_thinking == self.thinking_input
            && self.publication == publication
        {
            for node in &mut self.rows.children {
                if let Some(footer) = node.component_mut::<TurnFootnote>() {
                    footer.set_available(available);
                    footer.refresh();
                }
                if snapshot.is_stale
                    && let Some(control) = node
                        .component_mut::<TurnFootnote>()
                        .and_then(TurnFootnote::feedback_mut)
                {
                    control.invalidate_authority();
                }
            }
            self.publication = publication;
            self.refresh_detached();
            return Ok(());
        }
        let document =
            TranscriptDocument::read(snapshot).map_err(|_| "invalid transcript document")?;
        let plan = self.plan_with_thinking(
            snapshot,
            &document,
            timing,
            publication,
            epoch,
            self.thinking_input,
        )?;
        self.validate_plan(&plan, publication, epoch, available)?;
        if header_revision != self.current_header_revision() {
            return Err("tool header inputs changed during preparation");
        }
        let desired: BTreeSet<_> = plan.iter().map(|row| row.key.clone()).collect();
        // All source/identity/content/namespace checks precede tree mutation.
        let index: BTreeMap<_, _> = self
            .keys
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, key)| (key, i))
            .collect();
        let mut claimed = BTreeSet::new();
        let retained_keys: Vec<_> = plan
            .iter()
            .map(|row| {
                let key = self.retained_key(row, &desired, &index, &claimed);
                if let Some(key) = &key {
                    claimed.insert(key.clone());
                }
                key
            })
            .collect();
        let old = std::mem::take(&mut self.rows.children);
        let mut existing: BTreeMap<_, _> = std::mem::take(&mut self.keys)
            .into_iter()
            .zip(old)
            .collect();
        existing.append(&mut self.detached);
        let mut members = BTreeMap::new();
        for node in existing.values_mut() {
            if let Some(group) = node.component_mut::<ToolRun>() {
                let model = group.summary().unwrap().clone();
                for (id, member) in model.tool_ids.into_iter().zip(group.take_members()) {
                    members.insert((model.turn_id.clone(), id), member);
                }
            }
        }
        for (row, retained_key) in plan.iter().zip(retained_keys) {
            let mut node = retained_key
                .and_then(|key| existing.remove(&key))
                .unwrap_or_else(|| {
                    let namespace = self.next;
                    self.next += row.namespaces();
                    self.new_row(namespace, row)
                });
            if let PlannedContent::Tools(tools) = &row.content {
                let mut children = vec![];
                for (tool, placement) in tools {
                    let mut child = members
                        .remove(&(tool.turn_id.clone(), tool.id.clone()))
                        .unwrap_or_else(|| {
                            let view = ToolActivity::new(
                                self.next,
                                self.scope.clone(),
                                self.labels.tool.clone(),
                            );
                            self.next += 1;
                            Content::new(view.id(), view)
                        });
                    child.component_mut::<ToolActivity>().unwrap().update(
                        &self.scope,
                        publication,
                        epoch,
                        tool,
                        *placement,
                    )?;
                    children.push(child);
                }
                let model = self.labels.tool_run.summary(
                    &self.scope,
                    &tools.iter().map(|(tool, _)| *tool).collect::<Vec<_>>(),
                )?;
                node.component_mut::<ToolRun>()
                    .unwrap()
                    .install_owned_header(publication, epoch, model, children);
            } else {
                self.update_row(&mut node, row, publication, epoch, available)?;
            }
            self.keys.push(row.key.clone());
            self.rows.children.push(node);
        }
        for (key, mut node) in existing {
            if let Some(footer) = node.component_mut::<TurnFootnote>() {
                footer.refresh();
                footer.set_available(false);
                if footer.has_unresolved_action() {
                    self.detached.insert(key, node);
                }
            }
        }
        self.publication = publication;
        self.epoch = epoch;
        if snapshot.is_stale {
            for node in &mut self.rows.children {
                if let Some(control) = node
                    .component_mut::<TurnFootnote>()
                    .and_then(TurnFootnote::feedback_mut)
                {
                    control.invalidate_authority();
                }
            }
        }
        self.header_revision = header_revision;
        self.applied_thinking = self.thinking_input;
        self.last = Some(snapshot.clone());
        self.timing = timing.clone();
        self.arranged_revision = None;
        self.controls_dirty = false;
        Ok(())
    }
    pub(crate) fn namespaces_disjoint(&self, namespaces: &[u32]) -> bool {
        namespaces
            .iter()
            .all(|ns| *ns as u64 != self.root.0 >> 32 && !(self.start..self.end).contains(ns))
    }
    /// Validate the complete message update and expose its ordered row identities
    /// before a library reading transaction. Use immediately on the same UI turn;
    /// retain the library's original request token separately for async reads.
    pub fn prepare_update<'a>(
        &self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        snapshot: &'a TranscriptSnapshot,
        available: bool,
    ) -> Result<TranscriptContentUpdate<'a>, &'static str> {
        self.prepare_update_with_timing(
            scope,
            publication,
            epoch,
            snapshot,
            available,
            &Default::default(),
        )
    }
    pub fn prepare_update_with_timing<'a>(
        &self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        snapshot: &'a TranscriptSnapshot,
        available: bool,
        timing: &crate::TranscriptFootnoteTiming,
    ) -> Result<TranscriptContentUpdate<'a>, &'static str> {
        self.validate_update_with_timing(scope, publication, epoch, snapshot, available, timing)?;
        let header_revision = self.checked_header_revision(epoch)?;
        let document =
            TranscriptDocument::read(snapshot).map_err(|_| "invalid transcript document")?;
        let planned = self.plan_with_thinking(
            snapshot,
            &document,
            timing,
            publication,
            epoch,
            self.thinking_input,
        )?;
        let desired: BTreeSet<_> = planned.iter().map(|row| row.key.clone()).collect();
        let index: BTreeMap<_, _> = self
            .keys
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, key)| (key, i))
            .collect();
        let mut next = self.next;
        let mut claimed = BTreeSet::new();
        let rows = planned
            .iter()
            .map(|row| {
                let key = self.retained_key(row, &desired, &index, &claimed);
                if let Some(key) = &key {
                    claimed.insert(key.clone());
                }
                let retained = key.and_then(|key| self.find(&key, &index));
                let id = retained.map(Content::id).unwrap_or_else(|| {
                    let id = ComponentId((next as u64) << 32);
                    next += row.namespaces();
                    id
                });
                if let PlannedContent::Tools(tools) = &row.content {
                    next += tools
                        .iter()
                        .filter(|(tool, _)| self.find_tool(&tool.turn_id, &tool.id).is_none())
                        .count() as u32;
                }
                (row.key.clone(), id)
            })
            .collect();
        if header_revision != self.current_header_revision() {
            return Err("tool header inputs changed during preparation");
        }
        Ok(TranscriptContentUpdate {
            root: self.root,
            scope: scope.clone(),
            base_publication: self.publication,
            base_epoch: self.epoch,
            base_next: self.next,
            publication,
            epoch,
            snapshot,
            available,
            timing: timing.clone(),
            base_timing: self.timing.clone(),
            base_historical_generation: self.historical_generation,
            base_header_revision: self.header_revision,
            header_revision,
            rows,
            thinking_input: self.thinking_input,
            base_applied_thinking: self.applied_thinking,
        })
    }
    /// Recheck before committing. Stale plans cannot change the tree, controls
    /// or namespace allocation, even when a detached action settled meanwhile.
    pub fn validate_prepared_update(
        &self,
        prepared: &TranscriptContentUpdate<'_>,
    ) -> Result<(), &'static str> {
        if self.root != prepared.root
            || self.scope != prepared.scope
            || self.publication != prepared.base_publication
            || self.epoch != prepared.base_epoch
            || self.next != prepared.base_next
            || self.timing != prepared.base_timing
            || self.historical_generation != prepared.base_historical_generation
            || self.header_revision != prepared.base_header_revision
            || self.thinking_input != prepared.thinking_input
            || self.applied_thinking != prepared.base_applied_thinking
        {
            return Err("stale or foreign transcript update plan");
        }
        let current = self.prepare_update_with_timing(
            &prepared.scope,
            prepared.publication,
            prepared.epoch,
            prepared.snapshot,
            prepared.available,
            &prepared.timing,
        )?;
        if current.rows != prepared.rows || current.header_revision != prepared.header_revision {
            return Err("transcript update plan identities changed");
        }
        Ok(())
    }
    pub fn apply_prepared_update(
        &mut self,
        prepared: &TranscriptContentUpdate<'_>,
    ) -> Result<(), &'static str> {
        self.validate_prepared_update(prepared)?;
        self.update_with_timing(
            &prepared.scope,
            prepared.publication,
            prepared.epoch,
            prepared.snapshot,
            prepared.available,
            &prepared.timing,
        )
    }
    fn check_timing(
        &self,
        publication: u64,
        epoch: u64,
        snapshot: &TranscriptSnapshot,
        timing: &crate::TranscriptFootnoteTiming,
    ) -> Result<(), &'static str> {
        timing.validate(snapshot)?;
        if timing
            .goal
            .as_ref()
            .is_some_and(|goal| goal.scope != self.scope)
        {
            return Err("footnote goal scope mismatch");
        }
        if epoch == self.epoch
            && (timing.revision < self.timing.revision
                || (publication == self.publication
                    && timing.revision == self.timing.revision
                    && timing != &self.timing))
        {
            return Err("stale or conflicting Transcript footnote timing");
        }
        Ok(())
    }
    pub(crate) fn validate_update_with_timing(
        &self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        snapshot: &TranscriptSnapshot,
        available: bool,
        timing: &crate::TranscriptFootnoteTiming,
    ) -> Result<(), &'static str> {
        self.check_timing(publication, epoch, snapshot, timing)?;

        if scope != &self.scope {
            return Err("transcript content scope mismatch");
        }
        if publication == 0 || publication < self.publication || epoch < self.epoch {
            return Err("stale transcript content publication");
        }
        if publication == self.publication
            && (epoch != self.epoch || self.last.as_ref() != Some(snapshot))
        {
            return Err("conflicting transcript content publication");
        }
        let header_revision = self.checked_header_revision(epoch)?;
        let document =
            TranscriptDocument::read(snapshot).map_err(|_| "invalid transcript document")?;
        let plan = self.plan_with_thinking(
            snapshot,
            &document,
            timing,
            publication,
            epoch,
            self.thinking_input,
        )?;
        self.validate_plan(&plan, publication, epoch, available && !snapshot.is_stale)?;
        if header_revision != self.current_header_revision() {
            return Err("tool header inputs changed during preparation");
        }
        Ok(())
    }
    fn validate_plan(
        &self,
        plan: &[Planned<'_>],
        publication: u64,
        epoch: u64,
        available: bool,
    ) -> Result<(), &'static str> {
        let index: BTreeMap<_, _> = self
            .keys
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, key)| (key, i))
            .collect();
        let desired: BTreeSet<_> = plan.iter().map(|row| row.key.clone()).collect();
        if desired.len() != plan.len() {
            return Err("duplicate transcript content row");
        }
        for (key, node) in self.keys.iter().zip(&self.rows.children) {
            if let Some(footer) = node.component::<TurnFootnote>() {
                let target =
                    plan.iter()
                        .find(|row| &row.key == key)
                        .and_then(|row| match &row.content {
                            PlannedContent::Footnote(_, _, _, target) => target.map(|target| {
                                target
                                    .capture(&self.scope)
                                    .expect("validated feedback scope")
                            }),
                            _ => None,
                        });
                footer.check_feedback(target.as_ref())?;
            }
        }
        let mut allocations = 0_u64;
        let mut claimed = BTreeSet::new();
        for row in plan {
            let original = self.retained_key(row, &desired, &index, &claimed);
            if let Some(key) = &original
                && !claimed.insert(key.clone())
            {
                return Err("ambiguous transcript reconciliation receipt");
            }
            match original.as_ref().and_then(|key| self.find(key, &index)) {
                Some(content) => self.check_row(content, row, publication, epoch, available)?,
                None => {
                    allocations += u64::from(row.namespaces());
                    let candidate = self.new_row(0, row);
                    self.check_row(&candidate, row, publication, epoch, available)?;
                }
            }
            if let PlannedContent::Tools(tools) = &row.content {
                allocations += tools
                    .iter()
                    .filter(|(tool, _)| self.find_tool(&tool.turn_id, &tool.id).is_none())
                    .count() as u64;
            }
        }
        if allocations > u64::from(self.end - self.next) {
            return Err("transcript row namespaces exhausted");
        }
        Ok(())
    }
    fn find_tool(&self, turn: &str, id: &str) -> Option<&ToolActivity> {
        self.rows
            .children
            .iter()
            .filter_map(|node| node.component::<ToolRun>())
            .filter(|group| group.summary().is_some_and(|m| m.turn_id == turn))
            .find_map(|group| group.tool_activity(id))
    }
    pub fn tool_activity_id(&self, id: &str) -> Option<ComponentId> {
        self.rows
            .children
            .iter()
            .filter_map(|node| node.component::<ToolRun>())
            .find_map(|group| group.tool_activity(id))
            .map(ToolActivity::id)
    }
    pub fn tool_run(&self, id: &str) -> Option<&ToolRun> {
        self.rows
            .children
            .iter()
            .filter_map(|node| node.component::<ToolRun>())
            .find(|group| {
                group
                    .summary()
                    .is_some_and(|m| m.tool_ids.iter().any(|value| value == id))
            })
    }
    fn retained_key(
        &self,
        row: &Planned<'_>,
        desired: &BTreeSet<TranscriptContentRowKey>,
        index: &BTreeMap<TranscriptContentRowKey, usize>,
        claimed: &BTreeSet<TranscriptContentRowKey>,
    ) -> Option<TranscriptContentRowKey> {
        if let PlannedContent::Tools(tools) = &row.content {
            let matches = |node: &Content| {
                node.component::<ToolRun>().is_some_and(|group| {
                    group
                        .summary()
                        .is_some_and(|m| m.turn_id == tools[0].0.turn_id)
                })
            };
            if self.find(&row.key, index).is_some_and(matches) {
                return Some(row.key.clone());
            }
            return self
                .keys
                .iter()
                .zip(&self.rows.children)
                .find(|(key, node)| {
                    !desired.contains(*key)
                        && !claimed.contains(*key)
                        && matches(node)
                        && node
                            .component::<ToolRun>()
                            .unwrap()
                            .summary()
                            .unwrap()
                            .tool_ids
                            .iter()
                            .any(|id| tools.iter().any(|(tool, _)| &tool.id == id))
                })
                .map(|(key, _)| key.clone());
        }
        if self.find(&row.key, index).is_some() {
            Some(row.key.clone())
        } else {
            row.alias
                .as_ref()
                .filter(|key| !desired.contains(*key) && self.find(key, index).is_some())
                .cloned()
        }
    }
    fn find<'a>(
        &'a self,
        key: &TranscriptContentRowKey,
        index: &BTreeMap<TranscriptContentRowKey, usize>,
    ) -> Option<&'a Content> {
        index
            .get(key)
            .map(|i| &self.rows.children[*i])
            .or_else(|| self.detached.get(key))
    }
    fn new_row(&self, namespace: u32, row: &Planned<'_>) -> Content {
        let root = ComponentId((namespace as u64) << 32);
        match &row.content {
            PlannedContent::Input(_) => {
                let mut view = InputMessage::with_details_labels(
                    namespace,
                    self.scope.clone(),
                    self.labels.input_details,
                );
                view.set_copy_label(self.labels.input_copy.clone());
                Content::new(view.ids().root, view)
            }
            PlannedContent::Output(_, _) => {
                let view =
                    OutputMessage::with_labels(namespace, self.scope.clone(), self.labels.output);
                Content::new(view.id(), view)
            }
            PlannedContent::Tools(_) => Content::new(
                root,
                ToolRun::at_root(root, self.scope.clone(), self.labels.tool_run),
            ),
            PlannedContent::Footnote(..) => {
                let mut view =
                    TurnFootnote::new(namespace, self.scope.clone(), self.labels.footnote.clone());
                view.reserve_feedback_namespace(namespace + 1);
                Content::new(view.id(), view)
            }
            PlannedContent::Label(text) => Content::new(
                root,
                TextContent {
                    text: text.clone(),
                    role: proposal::TextRole::UiCaption,
                },
            ),
        }
    }
    fn check_row(
        &self,
        node: &Content,
        row: &Planned<'_>,
        publication: u64,
        epoch: u64,
        available: bool,
    ) -> Result<(), &'static str> {
        match &row.content {
            PlannedContent::Input(input) => {
                node.component::<InputMessage>()
                    .ok_or("transcript message role changed")?
                    .check_update(&self.scope, publication, epoch, input)?;
            }
            PlannedContent::Output(output, section) => {
                let view = node
                    .component::<OutputMessage>()
                    .ok_or("transcript message role changed")?;
                if let Some(thinking) = &row.thinking {
                    view.check_update_with_thinking(
                        publication,
                        epoch,
                        output,
                        *section,
                        thinking,
                    )?;
                } else {
                    view.check_update(&self.scope, publication, epoch, output, *section)?;
                }
            }
            PlannedContent::Tools(tools) => {
                let model = self.labels.tool_run.summary(
                    &self.scope,
                    &tools.iter().map(|(tool, _)| *tool).collect::<Vec<_>>(),
                )?;
                node.component::<ToolRun>()
                    .ok_or("transcript tool run role changed")?
                    .check_install(&self.scope, publication, epoch, &model)?;
                for (tool, placement) in tools {
                    if let Some(member) = self.find_tool(&tool.turn_id, &tool.id) {
                        member.check_update(&self.scope, publication, epoch, tool, *placement)?;
                    } else {
                        ToolActivity::new(0, self.scope.clone(), self.labels.tool.clone())
                            .check_update(&self.scope, publication, epoch, tool, *placement)?;
                    }
                }
            }
            PlannedContent::Footnote(turn, output, display, target) => {
                let footer = node
                    .component::<TurnFootnote>()
                    .ok_or("transcript footnote role changed")?;
                footer.check_feedback(
                    target
                        .map(|target| {
                            target
                                .capture(&self.scope)
                                .expect("validated feedback scope")
                        })
                        .as_ref(),
                )?;
                footer.check_update_with_turn_copy(
                    &self.scope,
                    publication,
                    epoch,
                    turn,
                    available,
                    output.as_ref(),
                    display,
                )?;
            }
            PlannedContent::Label(_) => {
                if node.component::<TextContent>().is_none() {
                    return Err("transcript label role changed");
                }
            }
        }
        Ok(())
    }
    fn update_row(
        &self,
        node: &mut Content,
        row: &Planned<'_>,
        publication: u64,
        epoch: u64,
        available: bool,
    ) -> Result<(), &'static str> {
        match &row.content {
            PlannedContent::Input(input) => {
                let view = node.component_mut::<InputMessage>().unwrap();
                view.update_with(
                    &self.scope,
                    publication,
                    epoch,
                    input,
                    self.labels.input_information,
                )?;
                view.configure_historical_input(if available {
                    self.historical_control.clone()
                } else {
                    None
                })?;
            }
            PlannedContent::Output(output, section) => {
                let view = node.component_mut::<OutputMessage>().unwrap();
                if let Some(thinking) = &row.thinking {
                    view.update_with_thinking(publication, epoch, output, *section, thinking)?;
                } else {
                    view.update(&self.scope, publication, epoch, output, *section)?;
                }
            }
            PlannedContent::Tools(_) => {
                unreachable!("parent installs preflighted original tool members")
            }
            PlannedContent::Footnote(turn, output, display, target) => {
                let footer = node.component_mut::<TurnFootnote>().unwrap();
                footer.update_with_turn_copy(
                    &self.scope,
                    publication,
                    epoch,
                    turn,
                    available,
                    output.as_ref(),
                    display,
                )?;
                footer.configure_feedback(
                    target.map(|target| {
                        target
                            .capture(&self.scope)
                            .expect("validated feedback scope")
                    }),
                    self.labels.feedback.as_ref(),
                );
                if epoch != self.epoch
                    && let Some(control) = footer.feedback_mut()
                {
                    control.invalidate_authority();
                }
            }
            PlannedContent::Label(text) => {
                node.component_mut::<TextContent>().unwrap().text = text.clone()
            }
        }
        Ok(())
    }
    fn refresh_detached(&mut self) {
        self.detached.retain(|_, node| {
            let footer = node.component_mut::<TurnFootnote>().unwrap();
            footer.refresh();
            footer.has_unresolved_action()
        });
    }
    /// Host forwards the original drained action batch, then renders. Local tool
    /// disclosure is handled here; mutations return immutable original-turn intents.
    pub fn handle_actions(&mut self, actions: &[ContentAction]) -> Vec<TurnFootnoteIntent> {
        let mut result = vec![];
        for node in &mut self.rows.children {
            if let Some(input) = node.component_mut::<InputMessage>() {
                input.handle_actions(actions);
            } else if let Some(output) = node.component_mut::<OutputMessage>() {
                output.handle_actions(actions);
            } else if let Some(tool) = node.component_mut::<ToolRun>() {
                tool.handle_actions(actions);
            } else if let Some(footer) = node.component_mut::<TurnFootnote>() {
                result.extend(footer.take_intents(actions));
            }
        }
        self.refresh_detached();
        result
    }
    pub fn feedback_enabled(&self) -> bool {
        self.labels.feedback.is_some()
    }
    pub(crate) fn historical_input_control(&self) -> Option<&crate::HistoricalInputControl> {
        self.historical_control.as_ref()
    }
    /// Register a host editing flow only for an advertised source. None is the
    /// default, including bounded panes. Apply through update before rendering.
    pub fn configure_historical_input(
        &mut self,
        control: Option<crate::HistoricalInputControl>,
    ) -> Result<(), &'static str> {
        if self.historical_control == control {
            return Ok(());
        }
        if self.composing_feedback_editor().is_some() {
            return Err("finish feedback composition before changing controls");
        }
        self.historical_generation = self
            .historical_generation
            .checked_add(1)
            .ok_or("historical input generation exhausted")?;
        self.historical_control = control;
        self.controls_dirty = true;
        self.arranged_revision = None;
        self.turn_target_owner = std::sync::Arc::new(());
        Ok(())
    }
    pub fn take_historical_input_intents(
        &self,
        actions: &[ContentAction],
    ) -> Vec<crate::HistoricalInputIntent> {
        if self.historical_control.is_none() || self.controls_dirty {
            return vec![];
        }
        self.rows
            .children
            .iter()
            .filter_map(|node| node.component::<InputMessage>())
            .flat_map(|input| input.take_historical_input_intents(actions))
            .filter(|intent| {
                self.historical_control
                    .as_ref()
                    .is_some_and(|control| control.action == intent.action())
            })
            .collect()
    }
    pub fn configure_feedback(
        &mut self,
        labels: Option<FeedbackLabels>,
    ) -> Result<(), &'static str> {
        if self.labels.feedback == labels {
            return Ok(());
        }
        if self.composing_feedback_editor().is_some() {
            return Err("finish feedback composition before changing controls");
        }
        self.labels.feedback = labels;
        self.controls_dirty = true;
        Ok(())
    }
    pub fn take_feedback_intents(
        &mut self,
        actions: &[ContentAction],
    ) -> Vec<(ComponentId, FeedbackGesture)> {
        self.rows
            .children
            .iter_mut()
            .filter_map(|node| {
                node.component_mut::<TurnFootnote>()
                    .and_then(TurnFootnote::feedback_mut)
                    .map(|control| (control.ids().root, control.take_intents(actions)))
            })
            .flat_map(|(id, gestures)| gestures.into_iter().map(move |gesture| (id, gesture)))
            .collect()
    }
    pub fn feedback_controls(&self) -> impl Iterator<Item = (ComponentId, &TurnFeedback)> {
        self.rows
            .children
            .iter()
            .filter_map(|node| {
                node.component::<TurnFootnote>()
                    .and_then(TurnFootnote::feedback)
            })
            .map(|control| (control.ids().root, control))
    }
    pub fn feedback_mut(&mut self, id: ComponentId) -> Option<&mut TurnFeedback> {
        self.rows
            .children
            .iter_mut()
            .filter_map(|node| {
                node.component_mut::<TurnFootnote>()
                    .and_then(TurnFootnote::feedback_mut)
            })
            .find(|control| control.ids().root == id)
    }
    pub fn composing_feedback_editor(&self) -> Option<ComponentId> {
        self.feedback_controls()
            .find_map(|(_, control)| control.is_composing().then(|| control.ids().editor))
    }
    /// Original copy actions only. Hosts may admit these focus-preserving
    /// business controls during preedit; SDK routing/geometry still decides hits.
    /// IDs are not clipboard or mutation authority and must not be cached.
    pub fn copy_action_ids(&self) -> Vec<ComponentId> {
        self.rows
            .children
            .iter()
            .flat_map(|node| {
                if let Some(input) = node.component::<InputMessage>() {
                    input.copy_button_id().into_iter().collect()
                } else if let Some(output) = node.component::<OutputMessage>() {
                    output.copy_action_ids()
                } else if let Some(tool) = node.component::<ToolRun>() {
                    tool.copy_action_ids()
                } else {
                    node.component::<TurnFootnote>()
                        .and_then(TurnFootnote::copy_button_id)
                        .into_iter()
                        .collect::<Vec<_>>()
                }
            })
            .collect()
    }
    pub fn take_copy_intents(&self, actions: &[ContentAction]) -> Vec<crate::TranscriptCopyIntent> {
        self.rows
            .children
            .iter()
            .flat_map(|node| {
                if let Some(input) = node.component::<InputMessage>() {
                    input.take_copy_intents(actions)
                } else if let Some(footnote) = node.component::<TurnFootnote>() {
                    footnote.take_copy_intents(actions)
                } else if let Some(output) = node.component::<OutputMessage>() {
                    output.take_copy_intents(actions)
                } else if let Some(tool) = node.component::<ToolRun>() {
                    tool.take_copy_intents(actions)
                } else {
                    vec![]
                }
            })
            .collect()
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let root = self.root;
        let mut host = ContentHost::new(Content::new(root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
}
impl Component for TranscriptContent {
    fn children(&self) -> &[Content] {
        &self.rows.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.rows.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        let layout = self.run_layout;
        let width = layout.map_or(available.width, |layout| layout.column_width);
        let measured = self.rows.measure(text, Size::new(width, available.height));
        let clearance = layout.map_or(0., |layout| layout.top + layout.bottom);
        MeasuredChild::new(Size::new(available.width, measured.size.height + clearance))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let content = if let Some(layout) = self.run_layout {
            // The Run supplies its pane-space column. The library owns the
            // ScrollContent focus gutter, scrolling origin, clip and placement.
            // Only vertical document clearance belongs to this Agent composition.
            ContainerLayout::new(
                Rect::new(
                    layout.column_x,
                    frame.min_y(),
                    layout.column_width,
                    frame.size.height,
                ),
                Insets::ZERO,
                Insets {
                    top: layout.top,
                    bottom: layout.bottom,
                    ..Insets::ZERO
                },
                None,
            )
            .content()
        } else {
            frame
        };
        self.rows.arrange(text, content, clip);
        self.arranged_revision = Some((self.publication, self.epoch));
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.group)
    }
}
struct Planned<'a> {
    key: TranscriptContentRowKey,
    alias: Option<TranscriptContentRowKey>,
    content: PlannedContent<'a>,
    thinking: Option<crate::TranscriptThinkingFrame>,
}
impl Planned<'_> {
    fn namespaces(&self) -> u32 {
        if matches!(self.content, PlannedContent::Footnote(..)) {
            2
        } else {
            1
        }
    }
}
enum PlannedContent<'a> {
    Input(TranscriptInput<'a>),
    Output(TranscriptOutput<'a>, Option<&'a TranscriptSection<'a>>),
    Tools(Vec<(&'a TranscriptToolCall, ToolActivityPlacement)>),
    Footnote(
        &'a TranscriptTurn,
        Option<crate::TranscriptTurnCopy>,
        crate::TurnFootnoteDisplay,
        Option<TurnFeedbackTarget<'a>>,
    ),
    Label(String),
}
fn label(key: TranscriptContentRowKey, text: String) -> Planned<'static> {
    Planned {
        key,
        alias: None,
        thinking: None,
        content: PlannedContent::Label(text),
    }
}
fn plan<'a>(
    snapshot: &TranscriptSnapshot,
    document: &'a TranscriptDocument<'a>,
    labels: &TranscriptContentLabels,
    timing: &crate::TranscriptFootnoteTiming,
) -> Result<Vec<Planned<'a>>, &'static str> {
    use TranscriptContentRowKey as Key;
    let mut copies = crate::TranscriptTurnCopy::catalog(snapshot);
    let goal_turn_id = timing
        .goal
        .as_ref()
        .and_then(|goal| goal.terminal_turn_id(snapshot));
    let mut result = vec![];
    if document.is_stale {
        result.push(label(Key::Stale, labels.stale.clone()));
    }
    if !document.diagnostics.is_empty() {
        result.push(label(Key::Diagnostics, document.diagnostics.join("\n")));
    }
    if !document.session_notices.is_empty() {
        result.push(label(Key::SessionNotices, labels.notices.clone()));
        for output in &document.session_notices {
            result.push(Planned {
                key: Key::Message(output.key.into()),
                alias: None,
                thinking: None,
                content: PlannedContent::Output(*output, None),
            });
        }
    }
    for section in &document.sections {
        let order = section
            .event_order()
            .map_err(|_| "ambiguous transcript event order")?;
        let embedded = embedded_tools(section);
        let mut tools = BTreeSet::new();
        for event in &order.ordered {
            if let Some(row) = event_row(*event, section, &embedded, &mut tools)? {
                result.push(row);
            }
        }
        if !order.unplaced.is_empty() {
            result.push(label(
                Key::Unplaced(section.id.into()),
                labels.unplaced.clone(),
            ));
        }
        for event in &order.unplaced {
            if let Some(row) = event_row(*event, section, &embedded, &mut tools)? {
                result.push(row);
            }
        }
        if let Some(turn) = section.turn {
            let output = copies.remove(&turn.id);
            let display = timing.display(
                turn,
                &labels.footnote,
                section.outputs.iter().any(|output| {
                    output.item.kind == TranscriptItemKind::Assistant
                        && output.item.is_final
                        && output.item.is_visible
                        && !output.item.is_provisional
                        && (!output.item.text.is_empty()
                            || !output.item.attachments.is_empty()
                            || output.item.content_blocks.as_ref().is_some_and(|blocks| {
                                blocks.iter().any(|block| match &block.content {
                                    TranscriptBlockContent::Text { text } => !text.is_empty(),
                                    TranscriptBlockContent::Attachment { .. }
                                    | TranscriptBlockContent::Unknown { .. } => true,
                                    TranscriptBlockContent::Reasoning { .. }
                                    | TranscriptBlockContent::ToolCall { .. } => false,
                                })
                            }))
                }),
                goal_turn_id,
            );
            let feedback_visible =
                display.shows_aside(turn, output.is_some()) && !display.is_live_owner;
            let target = (labels.feedback.is_some() && feedback_visible)
                .then(|| TurnFeedbackTarget::read(section))
                .flatten();
            result.push(Planned {
                key: Key::TurnFootnote(turn.id.clone()),
                alias: None,
                thinking: None,
                content: PlannedContent::Footnote(turn, output, display, target),
            });
        }
    }
    Ok(group_tools(result))
}
fn group_tools<'a>(result: Vec<Planned<'a>>) -> Vec<Planned<'a>> {
    let mut grouped: Vec<Planned<'a>> = vec![];
    for row in result {
        if let PlannedContent::Tools(current) = &row.content
            && current
                .iter()
                .all(|(_, p)| *p != ToolActivityPlacement::Unplaced)
            && let Some(previous) = grouped.iter_mut().rev().find(|row| !matches!(&row.content,
                PlannedContent::Output(output, _) if output.item.kind == TranscriptItemKind::Assistant
                    && output.item.text.is_empty() && output.item.reasoning.is_empty() && output.item.attachments.is_empty()
                    && output.item.failure_message().is_none() && output.item.content_blocks.as_ref().is_some_and(Vec::is_empty)))
            && let PlannedContent::Tools(prior) = &mut previous.content
            && prior
                .iter()
                .all(|(_, p)| *p != ToolActivityPlacement::Unplaced)
            && prior[0].0.turn_id == current[0].0.turn_id
        {
            prior.extend(current.iter().copied());
            continue;
        }
        grouped.push(row);
    }
    grouped
}
fn event_row<'a>(
    event: TranscriptOrderedEvent<'a>,
    section: &'a TranscriptSection<'a>,
    embedded: &BTreeSet<&str>,
    tools: &mut BTreeSet<String>,
) -> Result<Option<Planned<'a>>, &'static str> {
    use TranscriptContentRowKey as Key;
    Ok(match event {
        TranscriptOrderedEvent::Message(item) => {
            if item.kind == TranscriptItemKind::User {
                let input = section
                    .inputs
                    .iter()
                    .find(|input| input.item.id == item.id)
                    .ok_or("missing transcript input")?;
                Some(Planned {
                    key: Key::Message(input.key.into()),
                    thinking: None,
                    alias: input
                        .reconciliation_key()
                        .map(|key| Key::Message(key.into())),
                    content: PlannedContent::Input(*input),
                })
            } else {
                let output = section
                    .outputs
                    .iter()
                    .find(|output| output.item.id == item.id)
                    .ok_or("missing transcript output")?;
                Some(Planned {
                    key: Key::Message(output.key.into()),
                    thinking: None,
                    alias: output
                        .reconciliation_key()
                        .map(|key| Key::Message(key.into())),
                    content: PlannedContent::Output(*output, Some(section)),
                })
            }
        }
        TranscriptOrderedEvent::ToolCall(tool)
            if !embedded.contains(tool.id.as_str())
                && (tool.source_position.is_some()
                    || tool.result_position.is_none()
                    || tool.result.is_none())
                && tools.insert(tool.id.clone()) =>
        {
            Some(Planned {
                key: Key::Tool(tool.id.clone()),
                alias: None,
                thinking: None,
                content: PlannedContent::Tools(vec![(
                    tool,
                    if tool.source_position.is_some() {
                        ToolActivityPlacement::Call
                    } else {
                        ToolActivityPlacement::Unplaced
                    },
                )]),
            })
        }
        TranscriptOrderedEvent::ToolResult(tool)
            if !embedded.contains(tool.id.as_str())
                && tool.source_position.is_none()
                && tools.insert(tool.id.clone()) =>
        {
            Some(Planned {
                key: Key::Tool(tool.id.clone()),
                alias: None,
                thinking: None,
                content: PlannedContent::Tools(vec![(
                    tool,
                    if tool.result_position.is_some() {
                        ToolActivityPlacement::ResultOnly
                    } else {
                        ToolActivityPlacement::Unplaced
                    },
                )]),
            })
        }
        _ => None,
    })
}
fn embedded_tools<'a>(section: &'a TranscriptSection<'a>) -> BTreeSet<&'a str> {
    let mut result = BTreeSet::new();
    for output in &section.outputs {
        for block in output.item.content_blocks.as_ref().into_iter().flatten() {
            if let TranscriptBlockContent::ToolCall { tool_id } = &block.content
                && let Some(tool) = section.tool_for_block(block)
            {
                let covered = match (&tool.source_position, &output.item.source_position) {
                    (None, _) => true,
                    (
                        Some(TranscriptSourcePosition::Journal {
                            sequence,
                            content_index,
                        }),
                        Some(TranscriptSourcePosition::Journal {
                            sequence: container,
                            ..
                        }),
                    ) => sequence == container && *content_index == Some(block.index),
                    (
                        Some(TranscriptSourcePosition::Native {
                            entry_id,
                            entry_ordinal,
                            content_index,
                        }),
                        Some(TranscriptSourcePosition::Native {
                            entry_id: container,
                            entry_ordinal: ordinal,
                            ..
                        }),
                    ) => {
                        entry_id == container
                            && entry_ordinal == ordinal
                            && *content_index == Some(block.index)
                    }
                    _ => false,
                };
                if covered {
                    result.insert(tool_id.as_str());
                }
            }
        }
    }
    result
}
