//! Sidebar preference declarations over the original SDK menu and action records.
use crate::{SidebarPreferenceChange, SidebarPreferencesSnapshot, SidebarSort};
use tekes_ui_controls::{ButtonTextRenderer, composition::*, layout::MeasuredChild};
use tekes_ui_scene::{Rect, Size};

#[derive(Clone, Debug)]
pub struct SidebarOptionsLabels {
    pub name: String,
    pub sort: String,
    pub recent: String,
    pub ascending: String,
    pub descending: String,
    pub group: String,
    pub ungrouped: String,
    pub folder: String,
}
impl Default for SidebarOptionsLabels {
    fn default() -> Self {
        Self {
            name: "会话列表选项".into(),
            sort: "排序方式".into(),
            recent: "最近活动".into(),
            ascending: "标题 A–Z".into(),
            descending: "标题 Z–A".into(),
            group: "分组方式".into(),
            ungrouped: "无".into(),
            folder: "主文件夹".into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarOptionsIntent {
    pub change: SidebarPreferenceChange,
    /// ClientKit resets the ordinary page even for a repeated Sort by choice.
    pub reset_paging: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Input {
    snapshot: SidebarPreferencesSnapshot,
    writable: bool,
    title_sort_supported: bool,
}
/// The host allocates this compact control beside its shared Sidebar. Menu
/// input, overflow, focus, popup geometry and semantics stay in TekesUI.
#[derive(Debug)]
pub struct SidebarOptions {
    root: ComponentId,
    children: Vec<Content>,
    labels: SidebarOptionsLabels,
    input: Option<Input>,
    generation: u32,
}
impl SidebarOptions {
    pub fn new(namespace: u32, labels: SidebarOptionsLabels) -> Self {
        let root = ComponentId((namespace as u64) << 32);
        let mut menu = MenuContent::new(&labels.name, vec![]);
        menu.trigger.button.state.disabled = true;
        Self {
            root,
            children: vec![Content::new(ComponentId(root.0 + 1), menu)],
            labels,
            input: None,
            generation: 1,
        }
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    pub fn menu_id(&self) -> ComponentId {
        self.children[0].id()
    }
    pub fn update(
        &mut self,
        snapshot: &SidebarPreferencesSnapshot,
        writable: bool,
        title_sort_supported: bool,
    ) -> Result<(), &'static str> {
        snapshot
            .preferences
            .validate()
            .map_err(|_| "invalid Sidebar options preferences")?;
        if snapshot.revision == 0
            || self.input.as_ref().is_some_and(|old| {
                snapshot.revision < old.snapshot.revision
                    || (snapshot.revision == old.snapshot.revision && snapshot != &old.snapshot)
            })
        {
            return Err("stale or conflicting Sidebar options preferences");
        }
        let next = Input {
            snapshot: snapshot.clone(),
            writable,
            title_sort_supported,
        };
        if self.input.as_ref() == Some(&next) {
            return Ok(());
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or("Sidebar options namespace exhausted")?;
        let choice = |value: &str, label: &str, checked: bool, supported: bool| MenuEntry::Item {
            value: value.into(),
            label: label.into(),
            checked: Some(checked),
            disabled: !writable || !supported,
        };
        let preferences = &snapshot.preferences;
        let mut menu = MenuContent::new(&self.labels.name, vec![]);
        menu.set_entries(vec![
            MenuEntry::submenu(
                "sort",
                &self.labels.sort,
                vec![
                    choice(
                        "recentActivity",
                        &self.labels.recent,
                        preferences.sort == SidebarSort::RecentActivity,
                        true,
                    ),
                    choice(
                        "titleAscending",
                        &self.labels.ascending,
                        preferences.sort == SidebarSort::TitleAscending,
                        title_sort_supported,
                    ),
                    choice(
                        "titleDescending",
                        &self.labels.descending,
                        preferences.sort == SidebarSort::TitleDescending,
                        title_sort_supported,
                    ),
                ],
            ),
            MenuEntry::submenu(
                "group",
                &self.labels.group,
                vec![
                    choice(
                        "none",
                        &self.labels.ungrouped,
                        !preferences.group_by_folder,
                        true,
                    ),
                    choice(
                        "folder",
                        &self.labels.folder,
                        preferences.group_by_folder,
                        true,
                    ),
                ],
            ),
        ])
        .map_err(|_| "invalid Sidebar options menu labels")?;
        menu.trigger.button.state.disabled = !writable;
        // Revoke even an unchanged menu declaration when an unrelated preference
        // or capability changes. An old record cannot adopt a new CAS baseline.
        self.children = vec![Content::new(
            ComponentId(self.root.0 + generation as u64),
            menu,
        )];
        self.generation = generation;
        self.input = Some(next);
        Ok(())
    }
    /// Drain records at the originating event boundary, before publishing new
    /// inputs. Never poll MenuContent.selected or consume the legacy queue first.
    pub fn take_intents(&self, records: &[ContentActionRecord]) -> Vec<SidebarOptionsIntent> {
        let Some(input) = &self.input else {
            return vec![];
        };
        if !input.writable {
            return vec![];
        }
        records
            .iter()
            .filter_map(|record| {
                if record.action.target != self.menu_id()
                    || record.action.kind != ContentActionKind::Activate
                {
                    return None;
                }
                let Some(ContentActionPayload::MenuSelection(selection)) = &record.payload else {
                    return None;
                };
                let revision = self.children[0]
                    .component::<MenuContent>()?
                    .model_revision()?;
                if selection.model_revision() != revision {
                    return None;
                }
                let path = selection.value_path()?;
                let mut value = input.snapshot.preferences.clone();
                let reset_paging = match path
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice()
                {
                    ["sort", "recentActivity"] => {
                        value.sort = SidebarSort::RecentActivity;
                        true
                    }
                    ["sort", "titleAscending"] if input.title_sort_supported => {
                        value.sort = SidebarSort::TitleAscending;
                        true
                    }
                    ["sort", "titleDescending"] if input.title_sort_supported => {
                        value.sort = SidebarSort::TitleDescending;
                        true
                    }
                    ["group", "none"] => {
                        value.group_by_folder = false;
                        false
                    }
                    ["group", "folder"] => {
                        value.group_by_folder = true;
                        false
                    }
                    _ => return None,
                };
                Some(SidebarOptionsIntent {
                    change: SidebarPreferenceChange {
                        expected: input.snapshot.clone(),
                        value,
                    },
                    reset_paging,
                })
            })
            .collect()
    }
}
impl Component for SidebarOptions {
    fn children(&self) -> &[Content] {
        &self.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.children[0].measure(text, available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.children[0].arrange(text, frame, clip);
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.name)
    }
}
