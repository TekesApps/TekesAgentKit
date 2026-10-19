//! Agent summary UI. Membership, totals, costs and domain actions come from SummaryFacts.
use std::collections::{BTreeMap, BTreeSet};
use tekes_agent_presentation::{
    RunScope, SummaryAction, SummaryFacts, SummaryRow, SummarySection, append_summary_sections,
};
use tekes_ui_controls::{ButtonTextRenderer, ScrollArea, composition::*, layout::MeasuredChild};
use tekes_ui_scene::{Rect, Size};

#[derive(Clone, Debug, PartialEq)]
pub struct SummaryInput {
    pub revision: u64,
    pub scope: RunScope,
    pub facts: Option<SummaryFacts>,
    /// Host additions (e.g. Git) in caller order, through the existing public section contract.
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
    Row(RunScope, String, String, bool),
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
    bindings: BTreeMap<ComponentId, SummaryIntent>,
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
                    ListContent::new(
                        &labels.name,
                        vec![Content::new(
                            ComponentId(base + 2),
                            TextContent::new(&labels.unavailable),
                        )],
                    ),
                ),
            ),
            labels,
            ids: BTreeMap::new(),
            next: base + 16,
            revision: 0,
            scope: None,
            bindings: BTreeMap::new(),
        }
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let mut host = ContentHost::new(Content::new(self.root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
    pub fn action_id(&self, section: &str, row: &str) -> Option<ComponentId> {
        self.ids
            .get(&Key::Row(
                self.scope.as_ref()?.clone(),
                section.into(),
                row.into(),
                true,
            ))
            .copied()
    }
    pub fn update(&mut self, input: &SummaryInput) -> Result<(), &'static str> {
        if input.revision == 0 || input.revision < self.revision {
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
        let mut sections = input
            .facts
            .as_ref()
            .map(|facts| facts.sections(&input.scope))
            .unwrap_or_default();
        append_summary_sections(&mut sections, input.host_sections.clone());
        let mut section_ids = BTreeSet::new();
        let mut keys = BTreeSet::new();
        for section in &sections {
            if section.id.is_empty() || !section_ids.insert(&section.id) {
                return Err("invalid or duplicate summary section identity");
            }
            keys.insert(Key::Section(input.scope.clone(), section.id.clone()));
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
                keys.insert(Key::Row(
                    input.scope.clone(),
                    section.id.clone(),
                    row.id.clone(),
                    row.action.is_some(),
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
            .component_mut::<ListContent>()
            .expect("summary sections");
        let old = std::mem::take(&mut list.rows.children);
        let mut old: BTreeMap<_, _> = old
            .into_iter()
            .map(|content| (content.id(), content))
            .collect();
        let mut bindings = BTreeMap::new();
        for section in sections {
            let id = ids[&Key::Section(input.scope.clone(), section.id.clone())];
            let mut title = old
                .remove(&id)
                .unwrap_or_else(|| Content::new(id, TextContent::new("")));
            title
                .component_mut::<TextContent>()
                .expect("summary section label")
                .text = (self.labels.section)(&section.id);
            list.rows.children.push(title);
            for row in section.rows {
                let id = ids[&Key::Row(
                    input.scope.clone(),
                    section.id.clone(),
                    row.id.clone(),
                    row.action.is_some(),
                )];
                let label = (self.labels.row)(&section.id, &row);
                let content = if let Some(action) = row.action.clone() {
                    let mut content = old
                        .remove(&id)
                        .unwrap_or_else(|| Content::new(id, ButtonContent::new("")));
                    content
                        .component_mut::<ButtonContent>()
                        .expect("summary action")
                        .label = label;
                    bindings.insert(
                        id,
                        SummaryIntent {
                            scope: input.scope.clone(),
                            action,
                        },
                    );
                    content
                } else {
                    let mut content = old
                        .remove(&id)
                        .unwrap_or_else(|| Content::new(id, TextContent::new("")));
                    content
                        .component_mut::<TextContent>()
                        .expect("summary value")
                        .text = label;
                    content
                };
                list.rows.children.push(content);
            }
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
        Ok(())
    }
    pub fn take_intents(&self, actions: &[ContentAction]) -> Vec<SummaryIntent> {
        actions
            .iter()
            .filter(|action| action.kind == ContentActionKind::Activate)
            .filter_map(|action| self.bindings.get(&action.target).cloned())
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
