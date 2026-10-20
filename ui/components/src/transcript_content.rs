//! A canonical mixed message content tree. Scrolling/reading transactions belong
//! to TekesUI; this component neither invents a scroll state nor adjusts pixels.
use crate::{
    InputMessage, OutputMessage, OutputMessageLabels, ToolActivity, ToolActivityLabels,
    ToolActivityPlacement, TranscriptMessageIdentity, TurnFootnote, TurnFootnoteIntent,
    TurnFootnoteLabels,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};
use tekes_agent_presentation::{
    RunScope, TranscriptBlockContent, TranscriptDelivery, TranscriptDocument, TranscriptInput,
    TranscriptItemKind, TranscriptMessageKey, TranscriptOrderedEvent, TranscriptOutput,
    TranscriptSection, TranscriptSnapshot, TranscriptSourcePosition, TranscriptToolCall,
    TranscriptTurn,
};
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    extended::{Axis, Stack},
    layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::{Spacing, proposal};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TranscriptContentRowKey {
    Message(TranscriptMessageIdentity),
    Tool(String),
    TurnFootnote(String),
    Unplaced(String),
    SessionNotices,
    Diagnostics,
    Stale,
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
    pub footnote: TurnFootnoteLabels,
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
            footnote: Default::default(),
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
/// Transcript viewport or RunView. See capabilities()/the upstream #22/#25 gates.
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
    last: Option<TranscriptSnapshot>,
    labels: TranscriptContentLabels,
}
impl TranscriptContent {
    /// The host grants an exclusive row namespace range (one namespace per row).
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
            last: None,
            labels,
        })
    }
    pub fn id(&self) -> ComponentId {
        self.root
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
        self.keys
            .iter()
            .position(|value| value == key)
            .map(|i| self.rows.children[i].id())
    }
    pub fn message_id(&self, key: TranscriptMessageKey<'_>) -> Option<ComponentId> {
        self.row_id(&TranscriptContentRowKey::Message(key.into()))
    }
    pub fn row_keys(&self) -> &[TranscriptContentRowKey] {
        &self.keys
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
        let available = available && !snapshot.is_stale;
        if self.epoch == epoch && self.last.as_ref() == Some(snapshot) {
            for node in &mut self.rows.children {
                if let Some(footer) = node.component_mut::<TurnFootnote>() {
                    footer.set_available(available);
                    footer.refresh();
                }
            }
            self.publication = publication;
            self.refresh_detached();
            return Ok(());
        }
        let document =
            TranscriptDocument::read(snapshot).map_err(|_| "invalid transcript document")?;
        let plan = plan(&document, &self.labels)?;
        self.validate_plan(&plan, publication, epoch, available)?;
        let desired: BTreeSet<_> = plan.iter().map(|row| row.key.clone()).collect();
        // All source/identity/content/namespace checks precede tree mutation.
        let old = std::mem::take(&mut self.rows.children);
        let mut existing: BTreeMap<_, _> = std::mem::take(&mut self.keys)
            .into_iter()
            .zip(old)
            .collect();
        existing.append(&mut self.detached);
        for row in &plan {
            let retained = existing.remove(&row.key).or_else(|| {
                row.alias
                    .as_ref()
                    .filter(|key| !desired.contains(*key))
                    .and_then(|key| existing.remove(key))
            });
            let mut node = retained.unwrap_or_else(|| {
                let namespace = self.next;
                self.next += 1;
                self.new_row(namespace, row)
            });
            self.update_row(&mut node, row, publication, epoch, available)?;
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
        self.last = Some(snapshot.clone());
        Ok(())
    }
    pub(crate) fn namespaces_disjoint(&self, namespaces: &[u32]) -> bool {
        namespaces
            .iter()
            .all(|ns| *ns as u64 != self.root.0 >> 32 && !(self.start..self.end).contains(ns))
    }
    pub(crate) fn validate_update(
        &self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        snapshot: &TranscriptSnapshot,
        available: bool,
    ) -> Result<(), &'static str> {
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
        let document =
            TranscriptDocument::read(snapshot).map_err(|_| "invalid transcript document")?;
        let plan = plan(&document, &self.labels)?;
        self.validate_plan(&plan, publication, epoch, available && !snapshot.is_stale)
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
        let mut allocations = 0_u64;
        let mut claimed = BTreeSet::new();
        for row in plan {
            let original = if self.find(&row.key, &index).is_some() {
                Some(&row.key)
            } else {
                row.alias
                    .as_ref()
                    .filter(|key| !desired.contains(*key) && self.find(key, &index).is_some())
            };
            if let Some(key) = original
                && !claimed.insert(key.clone())
            {
                return Err("ambiguous transcript reconciliation receipt");
            }
            match original.and_then(|key| self.find(key, &index)) {
                Some(content) => self.check_row(content, row, publication, epoch, available)?,
                None => {
                    allocations += 1;
                    let candidate = self.new_row(0, row);
                    self.check_row(&candidate, row, publication, epoch, available)?;
                }
            }
        }
        if allocations > u64::from(self.end - self.next) {
            return Err("transcript row namespaces exhausted");
        }
        Ok(())
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
            PlannedContent::Tool(_, _) => {
                let view =
                    ToolActivity::new(namespace, self.scope.clone(), self.labels.tool.clone());
                Content::new(view.id(), view)
            }
            PlannedContent::Footnote(..) => {
                let view =
                    TurnFootnote::new(namespace, self.scope.clone(), self.labels.footnote.clone());
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
                node.component::<OutputMessage>()
                    .ok_or("transcript message role changed")?
                    .check_update(&self.scope, publication, epoch, output, *section)?;
            }
            PlannedContent::Tool(tool, placement) => {
                node.component::<ToolActivity>()
                    .ok_or("transcript tool role changed")?
                    .check_update(&self.scope, publication, epoch, tool, *placement)?;
            }
            PlannedContent::Footnote(turn, output) => {
                node.component::<TurnFootnote>()
                    .ok_or("transcript footnote role changed")?
                    .check_update_with_output(
                        &self.scope,
                        publication,
                        epoch,
                        turn,
                        available,
                        *output,
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
                node.component_mut::<InputMessage>().unwrap().update_with(
                    &self.scope,
                    publication,
                    epoch,
                    input,
                    self.labels.input_information,
                )?
            }
            PlannedContent::Output(output, section) => node
                .component_mut::<OutputMessage>()
                .unwrap()
                .update(&self.scope, publication, epoch, output, *section)?,
            PlannedContent::Tool(tool, placement) => node
                .component_mut::<ToolActivity>()
                .unwrap()
                .update(&self.scope, publication, epoch, tool, *placement)?,
            PlannedContent::Footnote(turn, output) => node
                .component_mut::<TurnFootnote>()
                .unwrap()
                .update_with_output(&self.scope, publication, epoch, turn, available, *output)?,
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
            } else if let Some(tool) = node.component_mut::<ToolActivity>() {
                tool.handle_actions(actions);
            } else if let Some(footer) = node.component_mut::<TurnFootnote>() {
                result.extend(footer.take_intents(actions));
            }
        }
        self.refresh_detached();
        result
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
        let measured = self.rows.measure(text, available);
        MeasuredChild::new(Size::new(available.width, measured.size.height))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.rows.arrange(text, frame, clip);
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.group)
    }
}
struct Planned<'a> {
    key: TranscriptContentRowKey,
    alias: Option<TranscriptContentRowKey>,
    content: PlannedContent<'a>,
}
enum PlannedContent<'a> {
    Input(TranscriptInput<'a>),
    Output(TranscriptOutput<'a>, Option<&'a TranscriptSection<'a>>),
    Tool(&'a TranscriptToolCall, ToolActivityPlacement),
    Footnote(&'a TranscriptTurn, Option<TranscriptOutput<'a>>),
    Label(String),
}
fn label(key: TranscriptContentRowKey, text: String) -> Planned<'static> {
    Planned {
        key,
        alias: None,
        content: PlannedContent::Label(text),
    }
}
fn plan<'a>(
    document: &'a TranscriptDocument<'a>,
    labels: &TranscriptContentLabels,
) -> Result<Vec<Planned<'a>>, &'static str> {
    use TranscriptContentRowKey as Key;
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
            result.push(Planned {
                key: Key::TurnFootnote(turn.id.clone()),
                alias: None,
                content: PlannedContent::Footnote(turn, section.final_output()),
            });
        }
    }
    Ok(result)
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
                    alias: None,
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
                content: PlannedContent::Tool(
                    tool,
                    if tool.source_position.is_some() {
                        ToolActivityPlacement::Call
                    } else {
                        ToolActivityPlacement::Unplaced
                    },
                ),
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
                content: PlannedContent::Tool(
                    tool,
                    if tool.result_position.is_some() {
                        ToolActivityPlacement::ResultOnly
                    } else {
                        ToolActivityPlacement::Unplaced
                    },
                ),
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
