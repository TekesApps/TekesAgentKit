//! Continuous tool disclosure; original member controls are moved, never rebuilt.
use crate::{ToolActivity, ToolActivityLabels, ToolActivityPlacement, TranscriptCopyIntent};
use std::{collections::BTreeMap, ops::Range};
use tekes_agent_presentation::{RunScope, TranscriptToolCall, TranscriptToolRunSummary};
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    extended::{Axis, Stack},
    layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::Spacing;

#[derive(Clone, Copy, Debug)]
pub enum ToolRunText {
    Group,
    Expand,
    Collapse,
}
#[derive(Clone, Copy, Debug)]
pub struct ToolRunLabels {
    pub text: fn(ToolRunText) -> String,
    pub count: fn(usize) -> String,
    pub failed: fn(usize) -> String,
    pub stopped: fn(usize) -> String,
    pub completed: fn(&str, usize) -> String,
    pub name: fn(&str) -> String,
    pub preview: fn(&RunScope, &TranscriptToolCall) -> String,
}
impl Default for ToolRunLabels {
    fn default() -> Self {
        Self {
            text: |kind| {
                match kind {
                    ToolRunText::Group => "连续工具",
                    ToolRunText::Expand => "展开",
                    ToolRunText::Collapse => "收起",
                }
                .into()
            },
            count: |count| format!("{count} 个工具"),
            failed: |count| format!("{count} 项失败"),
            stopped: |count| format!("{count} 项停止"),
            completed: |name, count| format!("{name} ×{count}"),
            name: crate::output_message::tool_display_name,
            preview: |_, tool| {
                let label = crate::output_message::tool_display_name(&tool.name);
                tool.continuous_summary(&label, None, &label)
            },
        }
    }
}
impl ToolRunLabels {
    pub(crate) fn summary(
        &self,
        scope: &RunScope,
        tools: &[&TranscriptToolCall],
    ) -> Result<TranscriptToolRunSummary, &'static str> {
        TranscriptToolRunSummary::read(tools, self.name, |tool| (self.preview)(scope, tool))
    }
    fn title(&self, model: &TranscriptToolRunSummary) -> String {
        let mut parts = vec![(self.count)(model.tool_ids.len())];
        if model.failed_count > 0 {
            parts.push((self.failed)(model.failed_count));
        }
        if model.stopped_count > 0 {
            parts.push((self.stopped)(model.stopped_count));
        }
        parts.extend(model.active_summaries.iter().cloned());
        parts.extend(
            model
                .completed
                .iter()
                .map(|entry| (self.completed)(&entry.name, entry.count)),
        );
        parts.join(" · ")
    }
}
#[derive(Debug)]
pub struct ToolRun {
    root: ComponentId,
    scope: RunScope,
    labels: ToolRunLabels,
    tool_labels: ToolActivityLabels,
    model: Option<TranscriptToolRunSummary>,
    publication: u64,
    epoch: u64,
    rows: StackContent,
    hidden: Vec<Content>,
    header: Option<Content>,
    header_id: Option<ComponentId>,
    expanded: bool,
    flattened: bool,
    header_text: String,
    namespaces: Option<Range<u32>>,
    next: u32,
    next_header: u64,
    last_tools: Vec<TranscriptToolCall>,
}
impl ToolRun {
    /// A standalone owner reserves one namespace for itself and an exclusive
    /// range for original ToolActivity members. Removed namespaces never recycle.
    pub fn new(
        namespace: u32,
        members: Range<u32>,
        scope: RunScope,
        labels: ToolRunLabels,
        tool_labels: ToolActivityLabels,
    ) -> Result<Self, &'static str> {
        scope.validate().map_err(|_| "invalid tool run scope")?;
        if members.is_empty() || members.contains(&namespace) {
            return Err("invalid tool run namespaces");
        }
        let mut view = Self::at_root(ComponentId((namespace as u64) << 32), scope, labels);
        view.tool_labels = tool_labels;
        view.next = members.start;
        view.namespaces = Some(members);
        Ok(view)
    }
    pub(crate) fn at_root(root: ComponentId, scope: RunScope, labels: ToolRunLabels) -> Self {
        Self {
            root,
            scope,
            labels,
            tool_labels: Default::default(),
            model: None,
            publication: 0,
            epoch: 0,
            rows: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: None,
                children: vec![],
            },
            hidden: vec![],
            header: None,
            header_id: None,
            expanded: false,
            flattened: false,
            header_text: String::new(),
            namespaces: None,
            next: 0,
            next_header: 16,
            last_tools: vec![],
        }
    }
    pub(crate) fn members_visible(&self) -> bool {
        self.flattened || !self.grouped() || self.expanded
    }
    pub(crate) fn set_flattened(&mut self, flattened: bool) {
        if flattened == self.flattened {
            return;
        }
        let members = self.take_members();
        self.flattened = flattened;
        if !self.grouped() {
            self.rows.children = members;
            return;
        }
        if flattened {
            self.rows.children = members;
        } else {
            self.rows
                .children
                .push(self.header.take().expect("retained original tool header"));
            if self.expanded {
                self.rows.children.extend(members);
            } else {
                self.hidden = members;
            }
        }
        self.refresh_header();
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    pub fn summary(&self) -> Option<&TranscriptToolRunSummary> {
        self.model.as_ref()
    }
    pub fn is_expanded(&self) -> bool {
        self.expanded
    }
    pub fn disclosure_id(&self) -> Option<ComponentId> {
        (self.grouped() && !self.flattened)
            .then_some(self.header_id)
            .flatten()
    }
    fn grouped(&self) -> bool {
        self.model
            .as_ref()
            .is_some_and(|model| model.tool_ids.len() > 1)
    }
    pub(crate) fn member_nodes(&self) -> impl Iterator<Item = &Content> {
        self.rows
            .children
            .iter()
            .skip(usize::from(self.grouped() && !self.flattened))
            .chain(&self.hidden)
    }
    pub(crate) fn member_nodes_mut(&mut self) -> impl Iterator<Item = &mut Content> {
        let skip = usize::from(self.grouped() && !self.flattened);
        self.rows
            .children
            .iter_mut()
            .skip(skip)
            .chain(&mut self.hidden)
    }
    pub(crate) fn take_members(&mut self) -> Vec<Content> {
        if self.grouped() && !self.flattened && !self.rows.children.is_empty() {
            self.header = Some(self.rows.children.remove(0));
        }
        let mut members = std::mem::take(&mut self.rows.children);
        members.append(&mut self.hidden);
        members
    }
    pub(crate) fn check_install(
        &self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        model: &TranscriptToolRunSummary,
    ) -> Result<(), &'static str> {
        if scope != &self.scope {
            return Err("tool run scope mismatch");
        }
        if publication == 0 || publication < self.publication || epoch < self.epoch {
            return Err("stale tool run publication");
        }
        if self.next_header >= u32::MAX as u64 {
            return Err("tool group header namespace exhausted");
        }
        if publication == self.publication
            && (epoch != self.epoch
                || self.model.as_ref() != Some(model)
                || self.header_text != self.labels.title(model))
        {
            return Err("conflicting tool run publication");
        }
        Ok(())
    }
    /// Parent preflight already validates member DTOs and exclusive namespaces.
    pub(crate) fn install(
        &mut self,
        publication: u64,
        epoch: u64,
        model: TranscriptToolRunSummary,
        members: Vec<Content>,
        alloc_header: &mut dyn FnMut() -> ComponentId,
    ) {
        let _old = self.take_members();
        if self
            .model
            .as_ref()
            .map(|m| (&m.turn_id, m.tool_ids.first()))
            != Some((&model.turn_id, model.tool_ids.first()))
        {
            self.header = None;
            self.header_id = None;
            self.expanded = false;
        }
        self.header_text = self.labels.title(&model);
        self.model = Some(model);
        self.publication = publication;
        self.epoch = epoch;
        if self.grouped() {
            let header = self.header.take().unwrap_or_else(|| {
                let id = alloc_header();
                self.header_id = Some(id);
                Content::new(id, Button::new(""))
                    .with_label_layout(tekes_ui_text::LabelOptions::single_line(
                        tekes_ui_text::LabelOverflow::TailEllipsis,
                    ))
                    .expect("original group header")
                    .preserving_focus()
            });
            if self.flattened {
                self.header = Some(header);
                self.rows.children.extend(members);
            } else {
                self.rows.children.push(header);
                if self.expanded {
                    self.rows.children.extend(members);
                } else {
                    self.hidden = members;
                }
            }
            self.refresh_header();
        } else {
            self.rows.children = members;
        }
    }
    pub(crate) fn install_owned_header(
        &mut self,
        publication: u64,
        epoch: u64,
        model: TranscriptToolRunSummary,
        members: Vec<Content>,
    ) {
        let base = self.root.0;
        let mut next = self.next_header;
        self.install(publication, epoch, model, members, &mut || {
            let id = ComponentId(base + next);
            next += 1;
            id
        });
        self.next_header = next;
    }
    fn refresh_header(&mut self) {
        if !self.grouped() {
            return;
        }
        let title = &self.header_text;
        let header = if self.flattened {
            self.header.as_mut().unwrap()
        } else {
            &mut self.rows.children[0]
        };
        let button = header.component_mut::<Button>().unwrap();
        button.label = format!(
            "{} · {title}",
            (self.labels.text)(if self.expanded {
                ToolRunText::Collapse
            } else {
                ToolRunText::Expand
            })
        );
        button.button.state.selected = self.expanded;
    }
    pub(crate) fn handle_disclosure(&mut self, actions: &[ContentAction]) -> bool {
        if !self.grouped() || self.flattened {
            return false;
        }
        let mut changed = false;
        for _ in actions
            .iter()
            .filter(|a| a.kind == ContentActionKind::Activate && Some(a.target) == self.header_id)
        {
            if self.expanded {
                self.hidden = self.rows.children.split_off(1);
            } else {
                self.rows.children.append(&mut self.hidden);
            }
            self.expanded = !self.expanded;
            changed = true;
        }
        self.refresh_header();
        changed
    }
    pub fn tool_activity(&self, id: &str) -> Option<&ToolActivity> {
        let index = self
            .model
            .as_ref()?
            .tool_ids
            .iter()
            .position(|value| value == id)?;
        self.member_nodes()
            .nth(index)
            .and_then(|node| node.component::<ToolActivity>())
            .or_else(|| {
                self.member_nodes()
                    .filter_map(|node| node.component::<ToolRun>())
                    .find_map(|group| group.tool_activity(id))
            })
    }
    pub fn update(
        &mut self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        tools: &[TranscriptToolCall],
    ) -> Result<(), &'static str> {
        if scope != &self.scope {
            return Err("tool run scope mismatch");
        }
        if publication == 0 || publication < self.publication || epoch < self.epoch {
            return Err("stale tool run publication");
        }
        let range = self
            .namespaces
            .clone()
            .ok_or("tool run is parent-managed")?;
        let model = self
            .labels
            .summary(scope, &tools.iter().collect::<Vec<_>>())?;
        self.check_install(scope, publication, epoch, &model)?;
        if publication == self.publication && self.last_tools != tools {
            return Err("conflicting tool run source");
        }
        let mut required = 0;
        for tool in tools {
            if let Some(view) = self.tool_activity(&tool.id) {
                view.check_update(scope, publication, epoch, tool, ToolActivityPlacement::Call)?;
            } else {
                required += 1;
                ToolActivity::new(0, scope.clone(), self.tool_labels.clone()).check_update(
                    scope,
                    publication,
                    epoch,
                    tool,
                    ToolActivityPlacement::Call,
                )?;
            }
        }
        if required > range.end - self.next || self.next_header >= u32::MAX as u64 {
            return Err("tool run namespaces exhausted");
        }
        let old_ids = self
            .model
            .as_ref()
            .map(|m| m.tool_ids.clone())
            .unwrap_or_default();
        let mut old: BTreeMap<_, _> = old_ids.into_iter().zip(self.take_members()).collect();
        let mut members = vec![];
        for tool in tools {
            let mut node = old.remove(&tool.id).unwrap_or_else(|| {
                let view = ToolActivity::new(self.next, scope.clone(), self.tool_labels.clone());
                self.next += 1;
                Content::new(view.id(), view)
            });
            node.component_mut::<ToolActivity>().unwrap().update(
                scope,
                publication,
                epoch,
                tool,
                ToolActivityPlacement::Call,
            )?;
            members.push(node);
        }
        let base = self.root.0;
        let mut next = self.next_header;
        self.install(publication, epoch, model, members, &mut || {
            let id = ComponentId(base + next);
            next += 1;
            id
        });
        self.next_header = next;
        self.last_tools = tools.to_vec();
        Ok(())
    }
    pub fn handle_actions(&mut self, actions: &[ContentAction]) -> bool {
        let mut changed = self.handle_disclosure(actions);
        if self.members_visible() {
            for node in self.member_nodes_mut() {
                if let Some(tool) = node.component_mut::<ToolActivity>() {
                    changed |= tool.handle_actions(actions);
                } else if let Some(group) = node.component_mut::<ToolRun>() {
                    changed |= group.handle_actions(actions);
                } else if let Some(fragment) = node.component_mut::<crate::OutputMessageFragment>()
                {
                    changed |= fragment.handle_actions(actions);
                }
            }
        }
        changed
    }
    pub fn copy_action_ids(&self) -> Vec<ComponentId> {
        if !self.members_visible() {
            return vec![];
        }
        self.member_nodes()
            .flat_map(|node| {
                if let Some(tool) = node.component::<ToolActivity>() {
                    tool.copy_action_ids()
                } else if let Some(group) = node.component::<ToolRun>() {
                    group.copy_action_ids()
                } else if let Some(fragment) = node.component::<crate::OutputMessageFragment>() {
                    fragment.copy_action_ids()
                } else {
                    vec![]
                }
            })
            .collect()
    }
    pub fn take_copy_intents(&self, actions: &[ContentAction]) -> Vec<TranscriptCopyIntent> {
        if !self.members_visible() {
            return vec![];
        }
        self.member_nodes()
            .flat_map(|node| {
                if let Some(tool) = node.component::<ToolActivity>() {
                    tool.take_copy_intents(actions)
                } else if let Some(group) = node.component::<ToolRun>() {
                    group.take_copy_intents(actions)
                } else if let Some(fragment) = node.component::<crate::OutputMessageFragment>() {
                    fragment.take_copy_intents(actions)
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
impl Component for ToolRun {
    fn children(&self) -> &[Content] {
        &self.rows.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.rows.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.rows.measure(text, available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.rows.arrange(text, frame, clip);
    }
    fn semantics(&self) -> ContentSemantics {
        let mut value = ContentSemantics::new(
            ContentRole::Group,
            if self.grouped() && !self.flattened {
                (self.labels.text)(ToolRunText::Group)
            } else {
                String::new()
            },
        );
        if self.grouped() && !self.flattened {
            value.expanded = Some(self.expanded);
        }
        value
    }
}
