//! Original shared session actions and local pin ordering over the SDK menu.
use crate::{
    SessionAddress, Sidebar, SidebarIntent, SidebarPreferenceChange, ThreadActionsPresentation,
};
use std::collections::BTreeSet;
use tekes_ui_controls::{ButtonTextRenderer, composition::*, layout::MeasuredChild};
use tekes_ui_scene::{Rect, Size};

#[derive(Clone, Debug)]
pub struct SidebarSessionMenuLabels {
    pub name: String,
    pub move_up: String,
    pub move_down: String,
}
impl Default for SidebarSessionMenuLabels {
    fn default() -> Self {
        Self {
            name: "会话操作".into(),
            move_up: "上移置顶".into(),
            move_down: "下移置顶".into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
struct Source {
    address: SessionAddress,
    actions: ThreadActionsPresentation,
    pinned: bool,
    pending: BTreeSet<String>,
    writable: bool,
    up: Option<SidebarPreferenceChange>,
    down: Option<SidebarPreferenceChange>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum SidebarSessionMenuIntent {
    Shared(SidebarIntent),
    Preferences(SidebarPreferenceChange),
}
/// A host may mount this beside its Sidebar, or use Sidebar::with_session_menu.
/// Input, popup, keyboard and semantics are delegated to original MenuContent.
#[derive(Debug)]
pub struct SidebarSessionMenu {
    root: ComponentId,
    labels: SidebarSessionMenuLabels,
    children: Vec<Content>,
    source: Option<Source>,
}
impl SidebarSessionMenu {
    pub fn new(namespace: u32, labels: SidebarSessionMenuLabels) -> Self {
        Self::at(ComponentId((namespace as u64) << 32), labels)
    }
    pub(crate) fn at(root: ComponentId, labels: SidebarSessionMenuLabels) -> Self {
        let menu = MenuContent::new(&labels.name, vec![]);
        Self {
            root,
            labels,
            children: vec![Content::new(ComponentId(root.0 + 1), menu)],
            source: None,
        }
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    pub fn menu_id(&self) -> Option<ComponentId> {
        self.source.as_ref().map(|_| self.children[0].id())
    }
    pub fn update_from_sidebar(&mut self, sidebar: &Sidebar) -> Result<(), &'static str> {
        let source = sidebar
            .displayed_input()
            .filter(|input| input.loaded)
            .and_then(|input| {
                input
                    .selected
                    .as_ref()
                    .and_then(|selected| input.rows.iter().find(|row| &row.address == selected))
            })
            .filter(|row| sidebar.row_id(&row.address).is_some())
            .map(|row| Source {
                address: row.address.clone(),
                actions: row.actions.clone(),
                pinned: row.pin.as_ref().is_some_and(|pin| pin.pinned),
                pending: row
                    .actions
                    .items
                    .iter()
                    .filter(|action| sidebar.is_pending(&row.address, &action.id))
                    .map(|action| action.id.clone())
                    .collect(),
                writable: sidebar.preferences_writable(),
                up: sidebar.move_pin_change(&row.address, -1),
                down: sidebar.move_pin_change(&row.address, 1),
            });
        if source == self.source {
            return Ok(());
        }
        let mut entries = vec![];
        if let Some(source) = &source {
            let mut seen = BTreeSet::new();
            if source.actions.endpoint != source.address.endpoint_id
                || source.actions.session_id != source.address.session_id
            {
                return Err("foreign Sidebar session menu actions");
            }
            for action in &source.actions.items {
                if action.id.is_empty() || !seen.insert(&action.id) {
                    return Err("invalid Sidebar session menu identity");
                }
                let label = if !action.enabled {
                    action
                        .disabled_reason
                        .as_ref()
                        .map(|reason| format!("{} · {reason}", action.label))
                        .unwrap_or_else(|| action.label.clone())
                } else {
                    action.label.clone()
                };
                entries.push(MenuEntry::Item {
                    value: format!("shared:{}", action.id),
                    label,
                    checked: None,
                    disabled: !action.enabled || source.pending.contains(&action.id),
                });
            }
            if source.pinned {
                // ClientKit offers both commands at either boundary; a move
                // outside the stored order is a no-op, never a new RPC.
                let at = source
                    .actions
                    .items
                    .iter()
                    .position(|a| matches!(a.id.as_str(), "archive" | "unarchive"))
                    .unwrap_or(entries.len());
                entries.splice(
                    at..at,
                    [
                        MenuEntry::Item {
                            value: "pin:up".into(),
                            label: self.labels.move_up.clone(),
                            checked: None,
                            disabled: !source.writable,
                        },
                        MenuEntry::Item {
                            value: "pin:down".into(),
                            label: self.labels.move_down.clone(),
                            checked: None,
                            disabled: !source.writable,
                        },
                    ],
                );
            }
        }
        let mut menu = MenuContent::new(&self.labels.name, vec![]);
        menu.set_entries(entries)
            .map_err(|_| "invalid Sidebar session menu labels")?;
        // A changed source retires its actual Content mount and SDK model.
        // A captured value never adopts a later selection or permission set.
        self.children = vec![Content::new(ComponentId(self.root.0 + 1), menu)];
        self.source = source;
        Ok(())
    }
    pub fn take_intents(&self, records: &[ContentActionRecord]) -> Vec<SidebarSessionMenuIntent> {
        let Some(source) = &self.source else {
            return vec![];
        };
        let Some(menu) = self.children[0].component::<MenuContent>() else {
            return vec![];
        };
        records
            .iter()
            .filter_map(|record| {
                if Some(record.action.target) != self.menu_id()
                    || record.action.kind != ContentActionKind::Activate
                {
                    return None;
                }
                let Some(ContentActionPayload::MenuSelection(selection)) = &record.payload else {
                    return None;
                };
                if Some(selection.model_revision()) != menu.model_revision() {
                    return None;
                }
                let [value] = selection.value_path()? else {
                    return None;
                };
                if let Some(id) = value.strip_prefix("shared:") {
                    let action = source.actions.items.iter().find(|action| {
                        action.id == id && action.enabled && !source.pending.contains(id)
                    })?;
                    return Some(SidebarSessionMenuIntent::Shared(SidebarIntent::Action {
                        address: source.address.clone(),
                        definition: action.clone(),
                    }));
                }
                if !source.writable {
                    return None;
                }
                match value.as_str() {
                    "pin:up" => source.up.clone().map(SidebarSessionMenuIntent::Preferences),
                    "pin:down" => source
                        .down
                        .clone()
                        .map(SidebarSessionMenuIntent::Preferences),
                    _ => None,
                }
            })
            .collect()
    }
}
impl Component for SidebarSessionMenu {
    fn active(&self) -> bool {
        self.source.is_some()
    }
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
