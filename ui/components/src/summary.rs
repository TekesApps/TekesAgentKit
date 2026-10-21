//! Agent summary UI. Membership, totals, costs and domain actions come from SummaryFacts.
use crate::summary_section::{Expansion, SummaryDocument, SummarySectionView};
use crate::summary_usage::{SummaryUsageRow, SummaryUsageRowIds, SummaryUsageRowText};
use std::collections::{BTreeMap, BTreeSet};
use std::{cell::RefCell, rc::Rc};
use tekes_agent_presentation::{RunScope, append_summary_sections};
pub use tekes_agent_presentation::{
    SummaryAction, SummaryArtifact, SummaryFacts, SummaryRow, SummarySection, SummarySubagent,
    SummaryUsage,
};
use tekes_ui_controls::{ButtonTextRenderer, ScrollArea, composition::*, layout::MeasuredChild};
use tekes_ui_scene::{Rect, Size};

#[derive(Clone, Debug, PartialEq)]
pub struct SummaryInput {
    pub revision: u64,
    pub scope: RunScope,
    pub facts: Option<SummaryFacts>,
    /// Host additions (e.g. Git) precede default providers, in caller order.
    pub host_sections: Vec<SummarySection>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SummaryIntent {
    pub scope: RunScope,
    pub action: SummaryAction,
}
#[derive(Clone, Debug)]
pub struct SummaryLabels {
    pub name: String,
    pub unavailable: String,
    pub empty: String,
    pub section: fn(&str) -> String,
    pub row: fn(&str, &SummaryRow) -> String,
    /// Usage uses distinct title/value slots, as in ClientKit. Product localization
    /// formats these facts; it must not recompute usage or infer missing costs.
    pub usage_row: fn(&SummaryRow) -> SummaryUsageRowText,
}
impl Default for SummaryLabels {
    fn default() -> Self {
        Self {
            name: "Summary".into(),
            unavailable: "Summary 暂不可用".into(),
            empty: "暂无 Summary 内容".into(),
            section: |id| {
                match id {
                    "tasks" => "任务",
                    "subagents" => "子代理",
                    "sources" => "来源",
                    "artifacts" => "产物",
                    "usage" => "用量",
                    _ => id,
                }
                .into()
            },
            usage_row: SummaryUsageRowText::read,
            row: |section, row| {
                if row.value.is_null() {
                    if section == "usage" {
                        format!("{} · 未知", row.title)
                    } else {
                        row.title.clone()
                    }
                } else {
                    format!(
                        "{} · {}",
                        row.title,
                        row.value
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| row.value.to_string())
                    )
                }
            },
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Section(RunScope, String),
    Header(RunScope, String),
    HeaderLabel(RunScope, String),
    Body(RunScope, String),
    Row(RunScope, String, String, ActionIdentity),
    UsageTitle(RunScope, String),
    UsageValue(RunScope, String),
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ActionIdentity {
    Passive,
    Task(String, RunScope),
    Subagent(RunScope, String),
    Artifact(RunScope, String),
}
impl ActionIdentity {
    fn read(action: Option<&SummaryAction>) -> Self {
        match action {
            None => Self::Passive,
            Some(SummaryAction::OpenTask { task_id, scope }) => {
                Self::Task(task_id.clone(), scope.clone())
            }
            Some(SummaryAction::OpenSubagent { parent, child_id }) => {
                Self::Subagent(parent.clone(), child_id.clone())
            }
            Some(SummaryAction::OpenArtifact { scope, path }) => {
                Self::Artifact(scope.clone(), path.clone())
            }
        }
    }
}
#[derive(Debug)]
pub struct SummaryView {
    root: ComponentId,
    scroll: ScrollContent,
    labels: SummaryLabels,
    ids: BTreeMap<Key, ComponentId>,
    next: u64,
    revision: u64,
    scope: Option<RunScope>,
    bindings: BTreeMap<ComponentId, (String, SummaryIntent)>,
    expansions: BTreeMap<String, Expansion>,
    last: Option<SummaryInput>,
}
impl SummaryView {
    pub fn new(namespace: u32, labels: SummaryLabels) -> Self {
        let base = (namespace as u64) << 32;
        Self {
            root: ComponentId(base),
            scroll: ScrollContent::new(
                ScrollArea::default(),
                Content::new(
                    ComponentId(base + 1),
                    SummaryDocument {
                        list: ListContent::new(
                            &labels.name,
                            vec![Content::new(
                                ComponentId(base + 2),
                                TextContent::new(&labels.unavailable),
                            )],
                        ),
                    },
                ),
            ),
            labels,
            ids: BTreeMap::new(),
            next: base + 16,
            revision: 0,
            scope: None,
            bindings: BTreeMap::new(),
            expansions: BTreeMap::new(),
            last: None,
        }
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    pub fn displayed_input(&self) -> Option<&SummaryInput> {
        self.last.as_ref()
    }
    pub fn row_id(&self, section: &str, row: &str) -> Option<ComponentId> {
        let scope = self.scope.as_ref()?;
        self.ids.iter().find_map(|(key, id)| match key {
            Key::Row(owner, section_id, row_id, _)
                if owner == scope && section_id == section && row_id == row =>
            {
                Some(*id)
            }
            _ => None,
        })
    }
    /// Retained passive text slots of a usage row. IDs remain stable through
    /// coherent publications and disclosure toggles; a new run gets new IDs.
    pub fn usage_row_ids(&self, row: &str) -> Option<SummaryUsageRowIds> {
        let scope = self.scope.as_ref()?;
        Some(SummaryUsageRowIds {
            row: self.row_id("usage", row)?,
            title: *self.ids.get(&Key::UsageTitle(scope.clone(), row.into()))?,
            value: *self.ids.get(&Key::UsageValue(scope.clone(), row.into()))?,
        })
    }
    /// Original selectable text for a passive row (the title slot for usage).
    /// Activatable rows retain their button and do not expose an editor here.
    pub fn passive_text_id(&self, section: &str, row: &str) -> Option<ComponentId> {
        let id = self.row_id(section, row)?;
        if self.bindings.contains_key(&id) {
            return None;
        }
        if section == "usage" {
            self.usage_row_ids(row).map(|ids| ids.title)
        } else {
            Some(id)
        }
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let mut host = ContentHost::new(Content::new(self.root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
    pub fn action_id(&self, section: &str, row: &str) -> Option<ComponentId> {
        self.row_id(section, row)
            .filter(|id| self.bindings.contains_key(id))
    }
    pub fn section_header_id(&self, section: &str) -> Option<ComponentId> {
        self.ids
            .get(&Key::Header(self.scope.as_ref()?.clone(), section.into()))
            .copied()
    }
    pub fn section_expanded(&self, section: &str) -> Option<bool> {
        self.section_header_id(section)?;
        Some(self.expansions.get(section)?.borrow().open)
    }
    pub fn section_order(&self) -> Vec<&str> {
        self.scroll.children()[0]
            .component::<SummaryDocument>()
            .unwrap()
            .list
            .rows
            .children
            .iter()
            .filter_map(|node| {
                node.component::<SummarySectionView>()
                    .map(|block| block.section.as_str())
            })
            .collect()
    }
    pub fn section_layouts(&self) -> Vec<&crate::SummarySectionLayout> {
        let mut layouts = vec![];
        for node in &self.scroll.children()[0]
            .component::<SummaryDocument>()
            .unwrap()
            .list
            .rows
            .children
        {
            if let Some(block) = node.component::<SummarySectionView>() {
                let Some(layout) = block.arranged() else {
                    return vec![];
                };
                layouts.push(layout);
            }
        }
        layouts
    }
    pub fn update(&mut self, input: &SummaryInput) -> Result<(), &'static str> {
        if input.revision == 0
            || input.revision < self.revision
            || (input.revision == self.revision && self.last.as_ref() != Some(input))
        {
            return Err("stale or unversioned summary input");
        }
        input
            .scope
            .validate()
            .map_err(|_| "invalid summary scope")?;
        if let Some(facts) = &input.facts
            && (facts.scope != input.scope || facts.validate().is_err())
        {
            return Err("summary facts belong to another run");
        }
        let providers = input
            .facts
            .as_ref()
            .map(|facts| facts.sections(&input.scope))
            .unwrap_or_default();
        let mut sections = vec![];
        append_summary_sections(&mut sections, input.host_sections.clone());
        sections.extend(providers);
        let mut section_ids = BTreeSet::new();
        let mut keys = BTreeSet::new();
        for section in &sections {
            if section.id.is_empty() || !section_ids.insert(section.id.clone()) {
                return Err("invalid or duplicate summary section identity");
            }
            keys.insert(Key::Section(input.scope.clone(), section.id.clone()));
            keys.insert(Key::Header(input.scope.clone(), section.id.clone()));
            keys.insert(Key::HeaderLabel(input.scope.clone(), section.id.clone()));
            keys.insert(Key::Body(input.scope.clone(), section.id.clone()));
            let mut row_ids = BTreeSet::new();
            for row in &section.rows {
                if row.id.is_empty() || !row_ids.insert(&row.id) {
                    return Err("invalid or duplicate summary row identity");
                }
                if let Some(action) = &row.action {
                    let valid = match action {
                        SummaryAction::OpenTask { task_id, scope } => {
                            !task_id.is_empty()
                                && scope.validate().is_ok()
                                && scope.endpoint_id == input.scope.endpoint_id
                                && scope.thread_id == input.scope.thread_id
                        }
                        SummaryAction::OpenSubagent { parent, child_id } => {
                            parent == &input.scope
                                && !child_id.is_empty()
                                && child_id != &input.scope.session_id
                        }
                        SummaryAction::OpenArtifact { scope, path } => {
                            scope == &input.scope && !path.is_empty()
                        }
                    };
                    if !valid {
                        return Err("summary action belongs to another run");
                    }
                }
                if section.id == "usage" && row.action.is_none() {
                    keys.insert(Key::UsageTitle(input.scope.clone(), row.id.clone()));
                    keys.insert(Key::UsageValue(input.scope.clone(), row.id.clone()));
                }
                keys.insert(Key::Row(
                    input.scope.clone(),
                    section.id.clone(),
                    row.id.clone(),
                    ActionIdentity::read(row.action.as_ref()),
                ));
            }
        }
        let needed = keys
            .iter()
            .filter(|key| !self.ids.contains_key(*key))
            .count() as u64;
        let next = self
            .next
            .checked_add(needed)
            .filter(|next| *next <= (self.root.0 | u32::MAX as u64))
            .ok_or("summary ID namespace exhausted")?;
        let mut assigned = self.next;
        let mut ids = self.ids.clone();
        ids.retain(|key, _| keys.contains(key));
        for key in keys {
            ids.entry(key).or_insert_with(|| {
                let id = ComponentId(assigned);
                assigned += 1;
                id
            });
        }
        if self.scope.as_ref() != Some(&input.scope) {
            self.scroll.scroll = ScrollArea::default();
        }
        let list = self.scroll.children_mut().expect("summary content")[0]
            .component_mut::<SummaryDocument>()
            .expect("summary sections");
        let list = &mut list.list;
        list.rows.model.gap = tekes_ui_tokens::Spacing::Section.value();
        let old = std::mem::take(&mut list.rows.children);
        let mut old: BTreeMap<_, _> = old
            .into_iter()
            .map(|content| (content.id(), content))
            .collect();
        let mut bindings = BTreeMap::new();
        for section in sections {
            let id = ids[&Key::Section(input.scope.clone(), section.id.clone())];
            let expansion = self
                .expansions
                .entry(section.id.clone())
                .or_insert_with(|| {
                    Rc::new(RefCell::new(tekes_ui_controls::extended::Disclosure {
                        open: true,
                        disabled: false,
                    }))
                })
                .clone();
            let mut block = old.remove(&id).unwrap_or_else(|| {
                Content::new(
                    id,
                    SummarySectionView::new(
                        section.id.clone(),
                        ids[&Key::Header(input.scope.clone(), section.id.clone())],
                        ids[&Key::HeaderLabel(input.scope.clone(), section.id.clone())],
                        ids[&Key::Body(input.scope.clone(), section.id.clone())],
                        expansion,
                    ),
                )
            });
            let view = block.component_mut::<SummarySectionView>().unwrap();
            view.layout = None;
            view.header_mut()
                .set_title((self.labels.section)(&section.id));
            let mut old_rows: BTreeMap<_, _> = std::mem::take(view.rows_mut())
                .into_iter()
                .map(|node| (node.id(), node))
                .collect();
            for row in section.rows {
                let id = ids[&Key::Row(
                    input.scope.clone(),
                    section.id.clone(),
                    row.id.clone(),
                    ActionIdentity::read(row.action.as_ref()),
                )];
                let content = if let Some(action) = row.action.clone() {
                    let label = (self.labels.row)(&section.id, &row);
                    let mut content = old_rows
                        .remove(&id)
                        .unwrap_or_else(|| Content::new(id, ButtonContent::new("")));
                    content
                        .component_mut::<ButtonContent>()
                        .expect("summary action")
                        .label = label;
                    bindings.insert(
                        id,
                        (
                            section.id.clone(),
                            SummaryIntent {
                                scope: input.scope.clone(),
                                action,
                            },
                        ),
                    );
                    content
                } else if section.id == "usage" {
                    let title_id = ids[&Key::UsageTitle(input.scope.clone(), row.id.clone())];
                    let value_id = ids[&Key::UsageValue(input.scope.clone(), row.id.clone())];
                    let mut content = old_rows.remove(&id).unwrap_or_else(|| {
                        Content::new(id, SummaryUsageRow::new(title_id, value_id))
                    });
                    content
                        .component_mut::<SummaryUsageRow>()
                        .expect("summary usage row")
                        .update(input.revision, (self.labels.usage_row)(&row));
                    content
                } else {
                    let label = (self.labels.row)(&section.id, &row);
                    let mut content = old_rows.remove(&id).unwrap_or_else(|| {
                        Content::new(
                            id,
                            TextFieldContent::read_only_content(
                                "",
                                "",
                                tekes_ui_tokens::proposal::TextRole::UiBody,
                            ),
                        )
                    });
                    crate::summary_text::update_text(
                        content
                            .component_mut::<TextFieldContent>()
                            .expect("summary passive text"),
                        id,
                        input.revision,
                        label,
                    );
                    content
                };
                view.rows_mut().push(content);
            }
            list.rows.children.push(block);
        }
        if list.rows.children.is_empty() {
            list.rows.children.push(Content::new(
                ComponentId(self.root.0 + 2),
                TextContent::new(if input.facts.is_some() {
                    &self.labels.empty
                } else {
                    &self.labels.unavailable
                }),
            ));
        }
        self.ids = ids;
        self.next = next;
        self.revision = input.revision;
        self.scope = Some(input.scope.clone());
        self.bindings = bindings;
        // ClientKit remembers collapsed section IDs even while absent; expanded
        // absent sections need no retained model or row tree.
        self.expansions
            .retain(|section, model| section_ids.contains(section) || !model.borrow().open);
        self.last = Some(input.clone());
        Ok(())
    }
    pub fn take_intents(&self, actions: &[ContentAction]) -> Vec<SummaryIntent> {
        actions
            .iter()
            .filter(|action| action.kind == ContentActionKind::Activate)
            .filter_map(|action| self.bindings.get(&action.target))
            .filter(|(section, _)| {
                self.expansions
                    .get(section)
                    .is_some_and(|model| model.borrow().open)
            })
            .map(|(_, intent)| intent.clone())
            .collect()
    }
}
impl Component for SummaryView {
    fn children(&self) -> &[Content] {
        self.scroll.children()
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        self.scroll.children_mut()
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.scroll.measure(text, available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.scroll.arrange(text, frame, clip);
    }
    fn event(&mut self, event: &ContentEvent) -> ContentResponse {
        self.scroll.event(event)
    }
    fn interactive(&self) -> bool {
        true
    }
    fn reveal(&mut self, descendant: Rect) -> Rect {
        self.scroll.reveal(descendant)
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.name)
    }
}
