//! Agent summary UI. Membership, totals, costs and domain actions come from SummaryFacts.
use crate::summary_custom::SummaryCustomRow;
use crate::summary_row_layout::SummaryRowBody;
use crate::summary_section::{Expansion, SectionHeaderKind, SummaryDocument, SummarySectionView};
use crate::summary_usage::{SummaryUsageRow, SummaryUsageRowIds, SummaryUsageRowText};
use crate::{SummaryActionRow, SummaryCustomRowSpec, SummaryCustomRowToken};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::{cell::RefCell, rc::Rc};
use tekes_agent_presentation::{RunScope, append_summary_sections};
pub use tekes_agent_presentation::{
    SummaryAction, SummaryArtifact, SummaryFacts, SummaryRow, SummarySection, SummarySubagent,
    SummarySubagentIndicator, SummaryUsage,
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
/// Display ownership is independent of the presence of a session. Hosts provide
/// stable workspace identity; no synthetic RunScope is allocated for host rows.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SummaryOwner {
    Run(RunScope),
    Workspace { id: String },
}
impl SummaryOwner {
    pub fn run_scope(&self) -> Option<&RunScope> {
        match self {
            Self::Run(scope) => Some(scope),
            Self::Workspace { .. } => None,
        }
    }
    fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Run(scope) => scope.validate().map_err(|_| "invalid summary scope"),
            Self::Workspace { id } if !id.trim().is_empty() && !id.contains('\0') => Ok(()),
            _ => Err("invalid summary workspace identity"),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct SummaryContextInput {
    /// Monotonic display publication across owner switches and both update APIs.
    pub revision: u64,
    pub owner: SummaryOwner,
    /// Session facts are valid only for the exact Run owner.
    pub facts: Option<SummaryFacts>,
    pub host_sections: Vec<SummarySection>,
    /// Original host controls replace these passive host rows, in the same order.
    pub custom_rows: Vec<SummaryCustomRowSpec>,
    /// Explicit host readiness. Only selects empty/unavailable when no rows exist.
    pub ready: bool,
}
impl From<&SummaryInput> for SummaryContextInput {
    fn from(input: &SummaryInput) -> Self {
        Self {
            revision: input.revision,
            owner: SummaryOwner::Run(input.scope.clone()),
            facts: input.facts.clone(),
            host_sections: input.host_sections.clone(),
            ready: input.facts.is_some(),
            custom_rows: vec![],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SummaryIntent {
    pub scope: RunScope,
    pub action: SummaryAction,
}
/// Called during publication preflight. Hosts may capture a read-only product
/// projection, and must publish a new Summary revision when that input changes.
pub type SummaryTrailingFormatter = Arc<dyn Fn(&str, &SummaryRow) -> Option<String> + Send + Sync>;
#[derive(Clone)]
pub struct SummaryLabels {
    pub name: String,
    pub unavailable: String,
    pub empty: String,
    pub section: fn(&str) -> String,
    pub row: fn(&str, &SummaryRow) -> String,
    /// Usage uses distinct title/value slots, as in ClientKit. Product localization
    /// formats these facts; it must not recompute usage or infer missing costs.
    pub usage_row: fn(&SummaryRow) -> SummaryUsageRowText,
    /// Optional plain metadata for ordinary default rows (not usage/custom).
    /// Host projection/localization supplies text; never infer it from status.
    pub trailing: SummaryTrailingFormatter,
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
            row: |_, row| row.title.clone(),
            trailing: Arc::new(|section, row| {
                if matches!(
                    section,
                    "tasks" | "subagents" | "sources" | "artifacts" | "usage"
                ) || row.value.is_null()
                {
                    None
                } else {
                    Some(
                        row.value
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| row.value.to_string()),
                    )
                }
            }),
        }
    }
}
impl std::fmt::Debug for SummaryLabels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SummaryLabels")
            .field("name", &self.name)
            .field("unavailable", &self.unavailable)
            .field("empty", &self.empty)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Section(SummaryOwner, String),
    Header(SummaryOwner, String),
    HeaderLabel(SummaryOwner, String),
    Body(SummaryOwner, String),
    Row(SummaryOwner, String, String, ActionIdentity),
    PassiveBody(SummaryOwner, String, String),
    Trailing(SummaryOwner, String, String, ActionIdentity),
    ActionBody(SummaryOwner, String, String, ActionIdentity),
    ActionTitle(SummaryOwner, String, String, ActionIdentity),
    UsageTitle(SummaryOwner, String),
    UsageValue(SummaryOwner, String),
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ActionIdentity {
    Passive,
    Custom(String),
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
pub(crate) struct PreparedSummaryContext {
    labels: BTreeMap<(String, String), String>,
    trailing: BTreeMap<(String, String), Option<String>>,
    sections: Vec<SummarySection>,
    section_ids: BTreeSet<String>,
    custom: BTreeMap<(String, String), SummaryCustomRowSpec>,
    ids: BTreeMap<Key, ComponentId>,
    next: u64,
    pub(crate) retiring: Vec<ComponentId>,
}
#[derive(Debug)]
pub struct SummaryView {
    root: ComponentId,
    scroll: ScrollContent,
    labels: SummaryLabels,
    ids: BTreeMap<Key, ComponentId>,
    next: u64,
    revision: u64,
    owner: Option<SummaryOwner>,
    bindings: BTreeMap<ComponentId, (String, SummaryIntent)>,
    expansions: BTreeMap<String, Expansion>,
    last: Option<SummaryInput>,
    last_context: Option<SummaryContextInput>,
    displayed_labels: BTreeMap<(String, String), String>,
    displayed_trailing: BTreeMap<(String, String), Option<String>>,
    custom_context: Arc<()>,
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
            owner: None,
            bindings: BTreeMap::new(),
            expansions: BTreeMap::new(),
            last: None,
            last_context: None,
            displayed_labels: BTreeMap::new(),
            displayed_trailing: BTreeMap::new(),
            custom_context: Arc::new(()),
        }
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    /// Last legacy Run input; None after an explicit update_context publication.
    pub fn displayed_input(&self) -> Option<&SummaryInput> {
        self.last.as_ref()
    }
    /// Canonical display input, including workspace-only ownership.
    pub fn displayed_context_input(&self) -> Option<&SummaryContextInput> {
        self.last_context.as_ref()
    }

    pub fn row_id(&self, section: &str, row: &str) -> Option<ComponentId> {
        let scope = self.owner.as_ref()?;
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
        let scope = self.owner.as_ref()?;
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
        if self.bindings.contains_key(&id) || self.custom_spec(section, row).is_some() {
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
    /// Actual default row band and its original title slot. Passive row_id /
    /// passive_text_id remain the original selectable field for compatibility.
    /// Usage and custom host rows keep their separate slot contracts.
    pub fn default_row_ids(&self, section: &str, row: &str) -> Option<crate::SummaryDefaultRowIds> {
        if section == "usage" || self.custom_spec(section, row).is_some() {
            return None;
        }
        let id = self.row_id(section, row)?;
        let owner = self.owner.as_ref()?.clone();
        let trailing = *self.ids.get(&Key::Trailing(
            owner.clone(),
            section.into(),
            row.into(),
            ActionIdentity::read(self.bindings.get(&id).map(|(_, intent)| &intent.action)),
        ))?;
        if let Some((_, intent)) = self.bindings.get(&id) {
            Some(crate::SummaryDefaultRowIds {
                row: id,
                trailing,
                title: *self.ids.get(&Key::ActionTitle(
                    owner,
                    section.into(),
                    row.into(),
                    ActionIdentity::read(Some(&intent.action)),
                ))?,
            })
        } else {
            Some(crate::SummaryDefaultRowIds {
                row: *self
                    .ids
                    .get(&Key::PassiveBody(owner, section.into(), row.into()))?,
                title: id,
                trailing,
            })
        }
    }
    pub fn section_header_id(&self, section: &str) -> Option<ComponentId> {
        self.ids
            .get(&Key::Header(self.owner.as_ref()?.clone(), section.into()))
            .copied()
    }
    /// Original decorative label slot. Text remains complete; the SDK shapes its
    /// one-line tail ellipsis, independently of the button's full AX name.
    pub fn section_header_label_id(&self, section: &str) -> Option<ComponentId> {
        self.ids
            .get(&Key::HeaderLabel(
                self.owner.as_ref()?.clone(),
                section.into(),
            ))
            .copied()
    }
    pub fn section_expanded(&self, section: &str) -> Option<bool> {
        self.section_header_id(section)?;
        Some(self.expansions.get(section)?.borrow().open)
    }
    fn arranged_headers(&self) -> impl Iterator<Item = &Content> {
        self.scroll.children()[0]
            .component::<SummaryDocument>()
            .unwrap()
            .list
            .rows
            .children
            .iter()
            .filter_map(|node| node.component::<SummarySectionView>())
            .filter(|section| section.arranged().is_some())
            .map(|section| &section.children[0])
    }
    /// Original, laid-out disclosure only. A host may allow this while a
    /// separate Run Composer is composing; never use it to hide a composing
    /// editor inside a custom Summary row or to permit navigation actions.
    /// The host must also verify this Summary is the active presented pane.
    pub fn is_nonfocusing_disclosure_target(&self, id: ComponentId) -> bool {
        self.arranged_headers().any(|header| {
            header.id() == id
                && header.visible().is_some()
                && !header
                    .component::<crate::summary_section::SummaryHeader>()
                    .unwrap()
                    .semantics()
                    .disabled
        })
    }
    pub fn is_nonfocusing_disclosure_at(&self, point: tekes_ui_scene::Point) -> bool {
        self.arranged_headers().any(|header| {
            header.visible().is_some_and(|frame| frame.contains(point))
                && self.is_nonfocusing_disclosure_target(header.id())
        })
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
    /// Compatibility adapter for existing Run-scoped consumers.
    pub fn update(&mut self, input: &SummaryInput) -> Result<(), &'static str> {
        self.update_context(&SummaryContextInput::from(input))?;
        self.last = Some(input.clone());
        Ok(())
    }
    /// Use one revision authority across Run/workspace owner changes. Reject
    /// foreign session facts/actions before changing any original control.
    pub fn update_context(&mut self, input: &SummaryContextInput) -> Result<(), &'static str> {
        let prepared = self.prepare_context(input)?;
        if !prepared.retiring.is_empty() {
            return Err("retire installed Summary rows through publish_summary_context_update");
        }
        self.apply_context(input, prepared)
    }
    pub(crate) fn prepare_context(
        &self,
        input: &SummaryContextInput,
    ) -> Result<PreparedSummaryContext, &'static str> {
        if input.revision == 0
            || input.revision < self.revision
            || (input.revision == self.revision && self.last_context.as_ref() != Some(input))
        {
            return Err("stale or unversioned summary input");
        }
        input.owner.validate()?;
        if let Some(facts) = &input.facts
            && (input.owner.run_scope() != Some(&facts.scope) || facts.validate().is_err())
        {
            return Err("summary facts belong to another run");
        }
        let providers = input
            .facts
            .as_ref()
            .map(|facts| facts.sections(&facts.scope))
            .unwrap_or_default();
        let mut sections = vec![];
        append_summary_sections(&mut sections, input.host_sections.clone());
        sections.extend(providers);
        let mut labels = BTreeMap::new();
        let mut trailing = BTreeMap::new();
        let mut custom = BTreeMap::new();
        for spec in &input.custom_rows {
            if spec.content_key.trim().is_empty()
                || custom
                    .insert((spec.section.clone(), spec.row.clone()), spec.clone())
                    .is_some()
                || !input.host_sections.iter().any(|section| {
                    section.id == spec.section
                        && section
                            .rows
                            .iter()
                            .any(|row| row.id == spec.row && row.action.is_none())
                })
            {
                return Err("invalid or duplicate Summary custom host row");
            }
        }
        let mut section_ids = BTreeSet::new();
        let mut keys = BTreeSet::new();
        for section in &sections {
            if section.id.is_empty() || !section_ids.insert(section.id.clone()) {
                return Err("invalid or duplicate summary section identity");
            }
            keys.insert(Key::Section(input.owner.clone(), section.id.clone()));
            keys.insert(Key::Header(input.owner.clone(), section.id.clone()));
            keys.insert(Key::HeaderLabel(input.owner.clone(), section.id.clone()));
            keys.insert(Key::Body(input.owner.clone(), section.id.clone()));
            let mut row_ids = BTreeSet::new();
            for row in &section.rows {
                if row.id.is_empty() || !row_ids.insert(&row.id) {
                    return Err("invalid or duplicate summary row identity");
                }
                if let Some(action) = &row.action {
                    let current = input
                        .owner
                        .run_scope()
                        .ok_or("workspace Summary cannot authorize session actions")?;
                    let valid = match action {
                        SummaryAction::OpenTask { task_id, scope } => {
                            !task_id.is_empty()
                                && scope.validate().is_ok()
                                && scope.endpoint_id == current.endpoint_id
                                && scope.thread_id == current.thread_id
                        }
                        SummaryAction::OpenSubagent { parent, child_id } => {
                            parent == current
                                && !child_id.is_empty()
                                && child_id != &current.session_id
                        }
                        SummaryAction::OpenArtifact { scope, path } => {
                            scope == current && !path.is_empty()
                        }
                    };
                    if !valid {
                        return Err("summary action belongs to another run");
                    }
                }
                let slot = custom.get(&(section.id.clone(), row.id.clone()));
                if section.id == "usage" && row.action.is_none() && slot.is_none() {
                    keys.insert(Key::UsageTitle(input.owner.clone(), row.id.clone()));
                    keys.insert(Key::UsageValue(input.owner.clone(), row.id.clone()));
                }
                if slot.is_none() && (row.action.is_some() || section.id != "usage") {
                    keys.insert(Key::Trailing(
                        input.owner.clone(),
                        section.id.clone(),
                        row.id.clone(),
                        ActionIdentity::read(row.action.as_ref()),
                    ));
                    trailing.insert(
                        (section.id.clone(), row.id.clone()),
                        (self.labels.trailing)(&section.id, row),
                    );
                }
                if slot.is_none() && row.action.is_some() {
                    let identity = ActionIdentity::read(row.action.as_ref());
                    keys.insert(Key::ActionBody(
                        input.owner.clone(),
                        section.id.clone(),
                        row.id.clone(),
                        identity.clone(),
                    ));
                    keys.insert(Key::ActionTitle(
                        input.owner.clone(),
                        section.id.clone(),
                        row.id.clone(),
                        identity,
                    ));
                } else if slot.is_none() && section.id != "usage" {
                    keys.insert(Key::PassiveBody(
                        input.owner.clone(),
                        section.id.clone(),
                        row.id.clone(),
                    ));
                }
                if slot.is_some() || row.action.is_some() || section.id != "usage" {
                    let label = (self.labels.row)(&section.id, row);
                    if row.action.is_some() && label.trim().is_empty() {
                        return Err("empty Summary action label");
                    }
                    labels.insert((section.id.clone(), row.id.clone()), label);
                }
                keys.insert(Key::Row(
                    input.owner.clone(),
                    section.id.clone(),
                    row.id.clone(),
                    slot.map_or_else(
                        || ActionIdentity::read(row.action.as_ref()),
                        |spec| ActionIdentity::Custom(spec.content_key.clone()),
                    ),
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
        let desired: BTreeSet<_> = ids.values().copied().collect();
        if input.revision == self.revision
            && (labels != self.displayed_labels || trailing != self.displayed_trailing)
        {
            return Err("Summary formatted content changed without publication");
        }
        let retiring = self.scroll.children()[0]
            .component::<SummaryDocument>()
            .unwrap()
            .list
            .rows
            .children
            .iter()
            .filter_map(|node| node.component::<SummarySectionView>())
            .flat_map(|block| block.rows())
            .filter(|row| !desired.contains(&row.id()))
            .filter_map(|row| row.component::<SummaryCustomRow>())
            .flat_map(|row| row.children.iter().map(Content::id))
            .collect();
        Ok(PreparedSummaryContext {
            labels,
            trailing,
            sections,
            section_ids,
            custom,
            ids,
            next,
            retiring,
        })
    }
    pub(crate) fn apply_context(
        &mut self,
        input: &SummaryContextInput,
        prepared: PreparedSummaryContext,
    ) -> Result<(), &'static str> {
        let PreparedSummaryContext {
            labels,
            trailing,
            sections,
            section_ids,
            custom,
            ids,
            next,
            ..
        } = prepared;
        if self.owner.as_ref() != Some(&input.owner) {
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
        for (index, section) in sections.into_iter().enumerate() {
            let id = ids[&Key::Section(input.owner.clone(), section.id.clone())];
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
                        ids[&Key::Header(input.owner.clone(), section.id.clone())],
                        ids[&Key::HeaderLabel(input.owner.clone(), section.id.clone())],
                        ids[&Key::Body(input.owner.clone(), section.id.clone())],
                        expansion,
                        SectionHeaderKind::Summary,
                    ),
                )
            });
            let view = block.component_mut::<SummarySectionView>().unwrap();
            view.layout = None;
            view.header_mut()
                .set_repository_divider(index != 0 && section.id.starts_with("repository:"));
            view.header_mut()
                .set_title((self.labels.section)(&section.id));
            let mut old_rows: BTreeMap<_, _> = std::mem::take(view.rows_mut())
                .into_iter()
                .map(|node| (node.id(), node))
                .collect();
            for row in section.rows {
                let slot = custom.get(&(section.id.clone(), row.id.clone()));
                let id = ids[&Key::Row(
                    input.owner.clone(),
                    section.id.clone(),
                    row.id.clone(),
                    slot.map_or_else(
                        || ActionIdentity::read(row.action.as_ref()),
                        |spec| ActionIdentity::Custom(spec.content_key.clone()),
                    ),
                )];
                let content = if slot.is_some() {
                    let label = labels[&(section.id.clone(), row.id.clone())].clone();
                    let mut content = old_rows
                        .remove(&id)
                        .unwrap_or_else(|| Content::new(id, SummaryCustomRow::new(label.clone())));
                    content
                        .component_mut::<SummaryCustomRow>()
                        .expect("Summary custom row")
                        .placeholder
                        .text = label;
                    content
                } else if let Some(action) = row.action.clone() {
                    let label = labels[&(section.id.clone(), row.id.clone())].clone();
                    let mut content = old_rows.remove(&id).unwrap_or_else(|| {
                        Content::new(
                            id,
                            SummaryActionRow::new(
                                ids[&Key::ActionBody(
                                    input.owner.clone(),
                                    section.id.clone(),
                                    row.id.clone(),
                                    ActionIdentity::read(Some(&action)),
                                )],
                                ids[&Key::ActionTitle(
                                    input.owner.clone(),
                                    section.id.clone(),
                                    row.id.clone(),
                                    ActionIdentity::read(Some(&action)),
                                )],
                                ids[&Key::Trailing(
                                    input.owner.clone(),
                                    section.id.clone(),
                                    row.id.clone(),
                                    ActionIdentity::read(Some(&action)),
                                )],
                            ),
                        )
                    });
                    content
                        .component_mut::<SummaryActionRow>()
                        .expect("summary action")
                        .update(
                            label,
                            trailing[&(section.id.clone(), row.id.clone())].clone(),
                        );
                    bindings.insert(
                        id,
                        (
                            section.id.clone(),
                            SummaryIntent {
                                scope: input
                                    .owner
                                    .run_scope()
                                    .expect("validated session action")
                                    .clone(),
                                action,
                            },
                        ),
                    );
                    content
                } else if section.id == "usage" {
                    let title_id = ids[&Key::UsageTitle(input.owner.clone(), row.id.clone())];
                    let value_id = ids[&Key::UsageValue(input.owner.clone(), row.id.clone())];
                    let mut content = old_rows.remove(&id).unwrap_or_else(|| {
                        Content::new(id, SummaryUsageRow::new(title_id, value_id))
                    });
                    content
                        .component_mut::<SummaryUsageRow>()
                        .expect("summary usage row")
                        .update(input.revision, (self.labels.usage_row)(&row));
                    content
                } else {
                    let label = labels[&(section.id.clone(), row.id.clone())].clone();
                    let body = ids[&Key::PassiveBody(
                        input.owner.clone(),
                        section.id.clone(),
                        row.id.clone(),
                    )];
                    let mut content = old_rows.remove(&body).unwrap_or_else(|| {
                        Content::new(
                            body,
                            SummaryRowBody::new(
                                Content::new(
                                    id,
                                    TextFieldContent::read_only_content(
                                        "",
                                        "",
                                        tekes_ui_tokens::proposal::TextRole::UiBody,
                                    ),
                                ),
                                ids[&Key::Trailing(
                                    input.owner.clone(),
                                    section.id.clone(),
                                    row.id.clone(),
                                    ActionIdentity::Passive,
                                )],
                            ),
                        )
                    });
                    content
                        .component_mut::<SummaryRowBody>()
                        .unwrap()
                        .update_trailing(trailing[&(section.id.clone(), row.id.clone())].clone());
                    crate::summary_text::update_text(
                        content.component_mut::<SummaryRowBody>().unwrap().children[0]
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
                TextContent::new(if input.ready {
                    &self.labels.empty
                } else {
                    &self.labels.unavailable
                }),
            ));
        }
        self.ids = ids;
        self.next = next;
        self.revision = input.revision;
        self.owner = Some(input.owner.clone());
        self.bindings = bindings;
        // ClientKit remembers collapsed section IDs even while absent; expanded
        // absent sections need no retained model or row tree.
        self.expansions
            .retain(|section, model| section_ids.contains(section) || !model.borrow().open);
        self.displayed_labels = labels;
        self.displayed_trailing = trailing;
        self.last_context = Some(input.clone());
        self.last = None;
        Ok(())
    }
    fn custom_spec(&self, section: &str, row: &str) -> Option<&SummaryCustomRowSpec> {
        self.last_context
            .as_ref()?
            .custom_rows
            .iter()
            .find(|spec| spec.section == section && spec.row == row)
    }
    fn custom_section_epoch(&self, section: &str) -> Option<u64> {
        self.scroll.children()[0]
            .component::<SummaryDocument>()?
            .list
            .rows
            .children
            .iter()
            .filter_map(|node| node.component::<SummarySectionView>())
            .find(|block| block.section == section)
            .map(|block| block.disclosure_epoch.get())
    }
    pub fn custom_row_token(&self, section: &str, row: &str) -> Option<SummaryCustomRowToken> {
        Some(SummaryCustomRowToken {
            summary: self.custom_context.clone(),
            owner: self.owner.clone()?,
            revision: self.revision,
            disclosure_epoch: self.custom_section_epoch(section)?,
            id: self.row_id(section, row)?,
            spec: self.custom_spec(section, row)?.clone(),
        })
    }
    fn check_custom(&self, token: &SummaryCustomRowToken) -> Result<(), &'static str> {
        if !Arc::ptr_eq(&self.custom_context, &token.summary)
            || self.owner.as_ref() != Some(&token.owner)
            || self.revision != token.revision
            || self.custom_section_epoch(&token.spec.section) != Some(token.disclosure_epoch)
            || self.custom_spec(&token.spec.section, &token.spec.row) != Some(&token.spec)
            || self.row_id(&token.spec.section, &token.spec.row) != Some(token.id)
        {
            return Err("retired Summary custom-row source");
        }
        Ok(())
    }
    fn custom_row(&self, id: ComponentId) -> Option<&SummaryCustomRow> {
        self.scroll.children()[0]
            .component::<SummaryDocument>()?
            .list
            .rows
            .children
            .iter()
            .filter_map(|node| node.component::<SummarySectionView>())
            .flat_map(|section| section.rows())
            .find(|row| row.id() == id)?
            .component()
    }
    fn custom_row_mut(&mut self, id: ComponentId) -> Option<&mut SummaryCustomRow> {
        let blocks = &mut self.scroll.children_mut()?[0]
            .component_mut::<SummaryDocument>()?
            .list
            .rows
            .children;
        for block in blocks {
            if let Some(section) = block.component_mut::<SummarySectionView>()
                && section.rows().iter().any(|row| row.id() == id)
            {
                section.layout = None;
                return section
                    .rows_mut()
                    .iter_mut()
                    .find(|row| row.id() == id)?
                    .component_mut();
            }
        }
        None
    }
    pub(crate) fn invalidate_custom_layout(&mut self, id: ComponentId) {
        self.custom_row_mut(id);
    }
    pub(crate) fn check_custom_install(
        &self,
        token: &SummaryCustomRowToken,
        content: ComponentId,
    ) -> Result<(), &'static str> {
        self.check_custom(token)?;
        if content.0 >> 32 == self.root.0 >> 32 {
            return Err("Summary custom content needs a separate namespace");
        }
        if self
            .custom_row(token.id)
            .is_none_or(|slot| !slot.children.is_empty())
        {
            return Err("Summary custom row missing or already installed");
        }
        Ok(())
    }
    pub fn custom_row_component<T: Component>(
        &self,
        token: &SummaryCustomRowToken,
    ) -> Result<&T, &'static str> {
        self.check_custom(token)?;
        self.custom_row(token.id)
            .and_then(|row| row.children.first())
            .and_then(Content::component::<T>)
            .ok_or("Summary custom row component is not installed or has another type")
    }
    /// Configure the retained host component, then request the next ordinary
    /// host render. SDK measurement owns height changes; there is no pixel callback.
    pub fn custom_row_component_mut<T: Component>(
        &mut self,
        token: &SummaryCustomRowToken,
    ) -> Result<&mut T, &'static str> {
        self.check_custom(token)?;
        self.custom_row_mut(token.id)
            .and_then(|row| row.children.first_mut())
            .and_then(Content::component_mut::<T>)
            .ok_or("Summary custom row component is not installed or has another type")
    }
    /// Validate the source captured with a host event, before product dispatch.
    /// Not an endpoint permission, once-only request claim, or async-result fence.
    pub fn validate_custom_row_action(
        &self,
        token: &SummaryCustomRowToken,
    ) -> Result<(), &'static str> {
        self.check_custom(token)?;
        if self.section_expanded(&token.spec.section) != Some(true)
            || self
                .custom_row(token.id)
                .is_none_or(|row| row.children.is_empty())
        {
            return Err("Summary custom row is hidden or not installed");
        }
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
    fn lifecycle(&mut self, event: Lifecycle) {
        if event == Lifecycle::Unmounted {
            self.custom_context = Arc::new(());
        }
    }
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
