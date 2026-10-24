//! Agent tab controls for a host toolbar; no window or navigation state machine.
use crate::{RunScope, SidePanelInput, SidePanelIntent, SidePanelTab, SidePanelTabKind};
use std::collections::BTreeMap;
use tekes_ui_controls::{
    ButtonTextRenderer, composition::*, layout::MeasuredChild, paint::ButtonVariant,
};
use tekes_ui_scene::{Rect, Size};

#[derive(Clone, Debug)]
pub struct SidePanelNavigationLabels {
    pub group: String,
    pub side_chat: String,
    pub new_side_chat: String,
    pub close: String,
}
impl Default for SidePanelNavigationLabels {
    fn default() -> Self {
        Self {
            group: "侧面板标签".into(),
            side_chat: "SideChat".into(),
            new_side_chat: "新建 SideChat".into(),
            close: "关闭当前标签".into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Tab(SidePanelTabKind),
    Chats,
    Close,
}
#[derive(Clone, Debug, PartialEq)]
enum Source {
    Action {
        intent: SidePanelIntent,
        enabled: bool,
    },
    Chats {
        parent: RunScope,
        tabs: Vec<SidePanelTab>,
        selected: Option<SidePanelTabKind>,
        create: bool,
        enabled: bool,
    },
}
#[derive(Clone, Debug)]
struct Control {
    id: ComponentId,
    source: Source,
}

/// Consumes the same controlled SidePanelInput as the pane body. Selection and
/// close/create return existing SidePanelIntent callbacks. A host allocates its
/// window toolbar/ProcessContainer and keeps IME/navigation adoption barriers.
/// The original SDK Content toolbar and menu own layout, popup, focus and paint.
#[derive(Debug)]
pub struct SidePanelNavigation {
    root: ComponentId,
    toolbar: ToolbarContent,
    labels: SidePanelNavigationLabels,
    input: Option<SidePanelInput>,
    controls: BTreeMap<Key, Control>,
    next: u64,
}
impl SidePanelNavigation {
    pub fn new(namespace: u32, labels: SidePanelNavigationLabels) -> Self {
        let root = ComponentId((namespace as u64) << 32);
        Self {
            root,
            toolbar: ToolbarContent::new(vec![
                Content::new(
                    ComponentId(root.0 + 1),
                    ToolbarGroup::new(ToolbarPlacement::Navigation, vec![]),
                ),
                Content::new(
                    ComponentId(root.0 + 2),
                    ToolbarGroup::new(ToolbarPlacement::PrimaryAction, vec![]),
                ),
            ]),
            labels,
            input: None,
            controls: BTreeMap::new(),
            next: root.0 + 16,
        }
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    pub fn displayed_input(&self) -> Option<&SidePanelInput> {
        self.input.as_ref()
    }
    pub fn tab_button_id(&self, tab: &SidePanelTabKind) -> Option<ComponentId> {
        self.controls.get(&Key::Tab(tab.clone())).map(|c| c.id)
    }
    pub fn side_chat_menu_id(&self) -> Option<ComponentId> {
        self.controls.get(&Key::Chats).map(|c| c.id)
    }
    pub fn close_id(&self) -> Option<ComponentId> {
        self.controls.get(&Key::Close).map(|c| c.id)
    }
    fn group_mut(&mut self, index: usize) -> &mut Vec<Content> {
        &mut self.toolbar.children[index]
            .component_mut::<ToolbarGroup>()
            .unwrap()
            .children
    }
    fn presented(&self) -> bool {
        !self.controls.is_empty()
    }
    /// `enabled` is the host's interaction input, not an endpoint capability.
    /// Unready projections cannot navigate/open a pane or create/discard a chat.
    pub fn update(&mut self, input: &SidePanelInput, enabled: bool) -> Result<(), &'static str> {
        crate::side_panel::validate_panel_navigation_input(input, self.input.as_ref())?;
        let enabled = enabled && input.projection_ready;
        let mut declarations = vec![];
        for tab in &input.tabs {
            if !matches!(tab.kind, SidePanelTabKind::SideChat { .. }) {
                declarations.push((
                    Key::Tab(tab.kind.clone()),
                    Source::Action {
                        intent: SidePanelIntent::Select {
                            parent: input.parent.clone(),
                            tab: tab.clone(),
                        },
                        enabled,
                    },
                ));
            }
        }
        let chats: Vec<_> = input
            .tabs
            .iter()
            .filter(|tab| matches!(tab.kind, SidePanelTabKind::SideChat { .. }))
            .cloned()
            .collect();
        if !chats.is_empty() {
            declarations.push((
                Key::Chats,
                Source::Chats {
                    parent: input.parent.clone(),
                    tabs: chats,
                    selected: input
                        .selected
                        .clone()
                        .filter(|tab| matches!(tab, SidePanelTabKind::SideChat { .. })),
                    create: enabled
                        && input.supports_side_chat == Some(true)
                        && !input.side_chats.is_creating_for(&input.parent)
                        && input
                            .side_chats
                            .unknown_creation_for(&input.parent)
                            .is_none(),
                    enabled,
                },
            ));
        }
        if let Some(tab) = &input.selected {
            let close = enabled
                && match tab {
                    SidePanelTabKind::SideChat { session_id } => {
                        input.side_chats.chats.get(session_id).is_some_and(|chat| {
                            chat.phase == tekes_agent_presentation::SideChatPhase::Ready
                        })
                    }
                    _ => true,
                };
            declarations.push((
                Key::Close,
                Source::Action {
                    intent: SidePanelIntent::Close {
                        parent: input.parent.clone(),
                        tab: tab.clone(),
                    },
                    enabled: close,
                },
            ));
        }
        let needed = declarations
            .iter()
            .filter(|(key, source)| {
                self.controls
                    .get(key)
                    .is_none_or(|old| old.source != *source)
            })
            .count() as u64;
        self.next
            .checked_add(needed)
            .filter(|next| *next <= self.root.0 | u32::MAX as u64)
            .ok_or("panel navigation namespace exhausted")?;
        let mut old: BTreeMap<_, _> = (0..2)
            .flat_map(|index| std::mem::take(self.group_mut(index)))
            .map(|node| (node.id(), node))
            .collect();
        let mut controls = BTreeMap::new();
        for (key, source) in declarations {
            let retained = self.controls.get(&key).filter(|old| old.source == source);
            let id = retained.map(|old| old.id).unwrap_or_else(|| {
                let id = ComponentId(self.next);
                self.next += 1;
                id
            });
            let mut node =
                retained
                    .and_then(|_| old.remove(&id))
                    .unwrap_or_else(|| match &source {
                        Source::Action { .. } => {
                            Content::new(id, ButtonContent::new("")).preserving_focus()
                        }
                        Source::Chats {
                            tabs,
                            selected,
                            create,
                            enabled,
                            ..
                        } => {
                            let mut menu = MenuContent::new(&self.labels.side_chat, vec![]);
                            let mut entries = vec![
                                MenuEntry::Item {
                                    value: "new".into(),
                                    label: self.labels.new_side_chat.clone(),
                                    checked: None,
                                    disabled: !create,
                                },
                                MenuEntry::Separator,
                            ];
                            entries.extend(tabs.iter().map(|tab| MenuEntry::Item {
                                value: match &tab.kind {
                                    SidePanelTabKind::SideChat { session_id } => {
                                        format!("select:{session_id}")
                                    }
                                    _ => unreachable!(),
                                },
                                label: self.labels.side_chat.clone(),
                                checked: Some(selected.as_ref() == Some(&tab.kind)),
                                disabled: !enabled,
                            }));
                            menu.set_entries(entries)
                                .expect("unique validated SideChat entries");
                            menu.trigger.button.state.disabled = !enabled;
                            menu.trigger.button.state.selected = selected.is_some();
                            Content::new(id, menu).preserving_focus()
                        }
                    });
            if let Source::Action { intent, enabled } = &source {
                let button = node.component_mut::<ButtonContent>().unwrap();
                button.variant = ButtonVariant::Quiet;
                button.button.state.disabled = !enabled;
                match intent {
                    SidePanelIntent::Select { tab, .. } => {
                        button.label = tab.title.clone();
                        button.button.state.selected = input.selected.as_ref() == Some(&tab.kind);
                    }
                    SidePanelIntent::Close { .. } => {
                        button.label = self.labels.close.clone();
                        button.button.state.selected = false;
                    }
                    _ => unreachable!(),
                }
            }
            self.group_mut(usize::from(key == Key::Close)).push(node);
            controls.insert(key, Control { id, source });
        }
        self.controls = controls;
        self.input = Some(input.clone());
        Ok(())
    }
    /// Capture before publishing new input. Inspect each original event payload,
    /// never the menu's last selected index. The SDK mount rejects same-ID remounts.
    /// Host must also gate its current pane and native Composer/feedback preedit.
    pub fn take_intents(
        &self,
        records: &[ContentActionRecord],
        host: &ContentHost,
    ) -> Vec<SidePanelIntent> {
        if host
            .get(self.root)
            .and_then(|node| node.component::<Self>())
            .is_none_or(|mounted| !std::ptr::eq(self, mounted))
        {
            return vec![];
        }
        records.iter().filter_map(|record| {
            if record.action.kind != ContentActionKind::Activate || host.semantic_mount(record.action.target) != Some(record.mount) { return None; }
            let control = self.controls.values().find(|control| control.id == record.action.target)?;
            match &control.source {
                Source::Action { intent, enabled: true } => Some(intent.clone()),
                Source::Chats { parent, tabs, create, enabled: true, .. } => {
                    let Some(ContentActionPayload::MenuSelection(selection)) = &record.payload else { return None; };
                    let menu = self.toolbar.children[0].component::<ToolbarGroup>()?.children.iter().find(|node| node.id() == control.id)?.component::<MenuContent>()?;
                    if menu.model_revision() != Some(selection.model_revision()) { return None; }
                    match selection.value_path()? {
                        [value] if value == "new" && *create => Some(SidePanelIntent::CreateSideChat { parent: parent.clone() }),
                        [value] => {
                            let id = value.strip_prefix("select:")?;
                            tabs.iter().find(|tab| matches!(&tab.kind, SidePanelTabKind::SideChat { session_id } if session_id == id)).map(|tab| SidePanelIntent::Select { parent: parent.clone(), tab: tab.clone() })
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        }).collect()
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let root = self.root;
        let mut host = ContentHost::new(Content::new(root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
}
impl Component for SidePanelNavigation {
    fn children(&self) -> &[Content] {
        if self.presented() {
            &self.toolbar.children
        } else {
            &[]
        }
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        self.presented().then_some(&mut self.toolbar.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        if self.presented() {
            self.toolbar.measure(text, available)
        } else {
            MeasuredChild::new(Size::default())
        }
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        if self.presented() {
            self.toolbar.arrange(text, frame, clip);
        }
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.group)
    }
}
