//! Run Summary surface projecting `SummaryFacts` (Usage, Tasks/Todos, Subagents, Artifacts)
//! and optional Host custom key-value sections across both `OthersTab::List` card and `Tab` modes.

use crate::ResponsiveWidthMode;
use crate::ids::*;
use std::collections::BTreeMap;
use tekes_agent_presentation::{RunScope, SummaryAction, SummaryFacts};
use tekes_ui_controls::{
    ButtonVariant, ScrollArea,
    composition::{
        Button, ComponentId, Content, ContentActionKind, ContentActionPayload, ContentActionRecord,
        ContentHost, DisclosureContent, KeyValueTableContent, ListDisplayUpdate, ListIntentKind,
        ListRowDisplay, ScrollContent, SelectableListContent, StackContent, TextContent,
    },
    extended::{
        Axis, KeyedListItem, KeyedListSnapshot, ListBudget, ListDatasetKey, ListItemKey, Stack,
    },
};
use tekes_ui_tokens::Spacing;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SummaryIntent {
    OpenArtifact {
        scope: RunScope,
        path: String,
    },
    OpenSubagent {
        parent: RunScope,
        child_id: String,
        title: String,
    },
    OpenSideChat {
        parent: RunScope,
        title: String,
    },
}

#[derive(Clone, Debug)]
enum SummaryTarget {
    Artifact {
        scope: RunScope,
        path: String,
    },
    Subagent {
        parent: RunScope,
        child_id: String,
        title: String,
    },
}

#[derive(Debug)]
pub struct SummarySurface {
    usage_expanded: bool,
    tasks_expanded: bool,
    next_key: u64,
    key_map: BTreeMap<String, ListItemKey>,
    targets: BTreeMap<ListItemKey, SummaryTarget>,
    selected_key: Option<ListItemKey>,
    last_list_sig: String,
    last_tasks_sig: String,
    last_host_sig: String,
    current_scope: Option<RunScope>,
}

impl Default for SummarySurface {
    fn default() -> Self {
        Self::new()
    }
}

impl SummarySurface {
    pub fn new() -> Self {
        Self {
            usage_expanded: true,
            tasks_expanded: true,
            next_key: 1,
            key_map: BTreeMap::new(),
            targets: BTreeMap::new(),
            selected_key: None,
            last_list_sig: String::new(),
            last_tasks_sig: String::new(),
            last_host_sig: String::new(),
            current_scope: None,
        }
    }

    fn budget() -> ListBudget {
        ListBudget {
            max_items: 256,
            max_display_items: 256,
            max_utf8_bytes: 256_000,
            max_staging_units: 32_000,
        }
    }

    pub fn build_content() -> Content {
        let usage_rows = [
            ("总 Token", "unavailable", 10),
            ("输入", "unavailable", 12),
            ("输出", "unavailable", 14),
            ("缓存命中", "unavailable", 16),
            ("命中率", "unavailable", 18),
            ("已知费用", "unavailable", 20),
        ]
        .into_iter()
        .map(|(k, v, offset)| {
            (
                Content::new(ComponentId(SUMMARY_KV_BASE + offset), TextContent::new(k)),
                Content::new(
                    ComponentId(SUMMARY_KV_BASE + offset + 1),
                    TextContent::new(v),
                ),
            )
        });
        let usage_disc = Content::new(
            SUMMARY_USAGE_DISC_ID,
            DisclosureContent::new(
                SUMMARY_USAGE_TRIGGER_ID,
                Content::new(ComponentId(713), TextContent::new("用量统计 (Usage)")),
                Content::new(
                    SUMMARY_USAGE_TABLE_ID,
                    KeyValueTableContent::new(usage_rows),
                ),
                true,
            )
            .with_section_header_layout(),
        );

        let tasks_disc = Content::new(
            SUMMARY_TASKS_DISC_ID,
            DisclosureContent::new(
                SUMMARY_TASKS_TRIGGER_ID,
                Content::new(ComponentId(723), TextContent::new("任务与子代理清单")),
                Content::new(
                    SUMMARY_TASKS_TABLE_ID,
                    KeyValueTableContent::new([(
                        Content::new(ComponentId(SUMMARY_KV_BASE + 100), TextContent::new("状态")),
                        Content::new(
                            ComponentId(SUMMARY_KV_BASE + 101),
                            TextContent::new("暂无任务"),
                        ),
                    )]),
                ),
                true,
            )
            .with_section_header_layout(),
        );

        let initial_snap = KeyedListSnapshot {
            dataset: ListDatasetKey(2),
            items: Vec::new(),
            display_items: Vec::new(),
        };
        let mut initial_rows = Vec::new();
        let nav_list = SelectableListContent::keyed(
            "产物与子代理导航",
            &initial_snap,
            &mut initial_rows,
            Self::budget(),
        )
        .expect("empty summary list");

        let mut open_artifact = Button::new("查看产物");
        open_artifact.button.state.disabled = true;
        let mut open_subagent = Button::new("打开子代理轨迹");
        open_subagent.button.state.disabled = true;
        let mut open_sidechat = Button::new("发起 SideChat");
        open_sidechat.variant = ButtonVariant::Quiet;

        let actions_row = Content::new(
            SUMMARY_ACTIONS_ROW_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![
                    Content::new(SUMMARY_OPEN_ARTIFACT_BTN_ID, open_artifact),
                    Content::new(SUMMARY_OPEN_SUBAGENT_BTN_ID, open_subagent),
                    Content::new(SUMMARY_OPEN_SIDECHAT_BTN_ID, open_sidechat),
                ],
            },
        );

        let host_sections = Content::new(
            SUMMARY_HOST_SECTIONS_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: Vec::new(),
            },
        );

        let stack = Content::new(
            SUMMARY_STACK_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![
                    Content::new(SUMMARY_HEADER_ID, TextContent::new("会话摘要 · 等待数据")),
                    usage_disc,
                    tasks_disc,
                    Content::new(SUMMARY_LIST_ID, nav_list),
                    actions_row,
                    host_sections,
                ],
            },
        );

        Content::new(
            SUMMARY_SCROLL_ID,
            ScrollContent::new(ScrollArea::default(), stack),
        )
    }

    fn key_for(&mut self, token: &str) -> ListItemKey {
        if let Some(existing) = self.key_map.get(token) {
            return *existing;
        }
        let key = ListItemKey(self.next_key);
        self.next_key += 1;
        self.key_map.insert(token.to_owned(), key);
        key
    }

    pub fn sync(
        &mut self,
        host: &mut ContentHost,
        facts: Option<&SummaryFacts>,
        host_sections: &[(String, Vec<(String, String)>)],
        _width_mode: ResponsiveWidthMode,
    ) {
        self.current_scope = facts.map(|f| f.scope.clone());

        if let Some(header) = host
            .get_mut(SUMMARY_HEADER_ID)
            .and_then(|c| c.component_mut::<TextContent>())
        {
            header.text = match facts {
                Some(f) => format!(
                    "摘要 · {} · 任务 {} / 子代理 {} / 产物 {}",
                    f.scope.session_id,
                    f.tasks.len(),
                    f.subagents.len(),
                    f.artifacts.len()
                ),
                None => "摘要 · 暂无当前会话摘要".to_owned(),
            };
        }

        // Update 6 usage value cells cleanly in place.
        let usage_vals: [String; 6] = if let Some(u) = facts.and_then(|f| f.usage.as_ref()) {
            let hit = u
                .cache_hit_rate()
                .map(|r| format!("{:.1}%", r * 100.0))
                .unwrap_or_else(|| "unavailable".to_owned());
            let cost = u
                .known_cost_usd()
                .map(|c| format!("${c:.4}"))
                .unwrap_or_else(|| "unavailable".to_owned());
            [
                u.total_tokens().to_string(),
                u.input_tokens.to_string(),
                u.output_tokens.to_string(),
                u.cached_input_tokens.to_string(),
                hit,
                cost,
            ]
        } else {
            [
                "unavailable".into(),
                "unavailable".into(),
                "unavailable".into(),
                "unavailable".into(),
                "unavailable".into(),
                "unavailable".into(),
            ]
        };

        for (i, val) in usage_vals.into_iter().enumerate() {
            let cell_id = ComponentId(SUMMARY_KV_BASE + 10 + (i as u64) * 2 + 1);
            if let Some(cell) = host
                .get_mut(cell_id)
                .and_then(|c| c.component_mut::<TextContent>())
            {
                cell.text = val;
            }
        }
        if let Some(disc) = host
            .get_mut(SUMMARY_USAGE_DISC_ID)
            .and_then(|c| c.component_mut::<DisclosureContent>())
        {
            disc.set_expanded(self.usage_expanded);
        }

        let tasks_text = if let Some(f) = facts {
            if f.tasks.is_empty() && f.subagents.is_empty() {
                "暂无任务或子代理".to_owned()
            } else {
                let mut parts = Vec::new();
                for t in &f.tasks {
                    parts.push(format!("{} [{}]", t.name, t.canonical_status()));
                }
                for s in &f.subagents {
                    let st = if s.status.is_empty() {
                        "known"
                    } else {
                        s.status.as_str()
                    };
                    parts.push(format!("子代理: {} [{}]", s.title, st));
                }
                parts.join(" · ")
            }
        } else {
            "等待选择会话".to_owned()
        };
        if tasks_text != self.last_tasks_sig {
            self.last_tasks_sig = tasks_text.clone();
            if let Some(cell) = host
                .get_mut(ComponentId(SUMMARY_KV_BASE + 101))
                .and_then(|c| c.component_mut::<TextContent>())
            {
                cell.text = tasks_text;
            }
        }
        if let Some(disc) = host
            .get_mut(SUMMARY_TASKS_DISC_ID)
            .and_then(|c| c.component_mut::<DisclosureContent>())
        {
            disc.set_expanded(self.tasks_expanded);
        }

        self.sync_nav_list(host, facts);
        self.sync_action_buttons(host);
        self.sync_host_sections(host, host_sections);
    }

    fn sync_nav_list(&mut self, host: &mut ContentHost, facts: Option<&SummaryFacts>) {
        let mut items = Vec::new();
        let mut display_items = Vec::new();
        let mut rows = Vec::new();
        let mut sig = String::new();
        let mut next_targets = BTreeMap::new();

        if let Some(f) = facts {
            for section in f.sections(&f.scope) {
                for row in section.rows {
                    let Some(action) = row.action else {
                        continue;
                    };
                    let (token, label, target) = match action {
                        SummaryAction::OpenArtifact { scope, path } => (
                            format!("artifact:{path}"),
                            format!("[产物] {path}"),
                            SummaryTarget::Artifact { scope, path },
                        ),
                        SummaryAction::OpenSubagent { parent, child_id } => (
                            format!("subagent:{child_id}"),
                            format!("[子代理] {} ({})", row.title, row.value),
                            SummaryTarget::Subagent {
                                parent,
                                child_id,
                                title: row.title,
                            },
                        ),
                        SummaryAction::OpenTask { task_id, scope } => (
                            format!("task:{task_id}"),
                            format!("[任务] {}", row.title),
                            SummaryTarget::Subagent {
                                parent: scope.clone(),
                                child_id: scope.session_id,
                                title: row.title,
                            },
                        ),
                    };
                    let key = self.key_for(&token);
                    next_targets.insert(key, target);
                    sig.push_str(&format!("{}:{}|", key.0, label));
                    items.push(KeyedListItem { key, name: label });
                    display_items.push(key);
                    rows.push(ListRowDisplay {
                        key,
                        row_id: ComponentId(SUMMARY_ROW_BASE + key.0),
                        display: ListDisplayUpdate::DefaultLabel,
                    });
                }
            }
        }
        self.targets = next_targets;

        if sig != self.last_list_sig {
            let snap = KeyedListSnapshot {
                dataset: ListDatasetKey(2),
                items,
                display_items,
            };
            if host
                .replace_keyed_list(SUMMARY_LIST_ID, &snap, &mut rows, None)
                .is_ok()
            {
                self.last_list_sig = sig;
            }
        }
    }

    fn sync_action_buttons(&mut self, host: &mut ContentHost) {
        let selected_target = self.selected_key.and_then(|k| self.targets.get(&k));
        let is_artifact = matches!(selected_target, Some(SummaryTarget::Artifact { .. }));
        let is_subagent = matches!(selected_target, Some(SummaryTarget::Subagent { .. }));
        if let Some(btn) = host
            .get_mut(SUMMARY_OPEN_ARTIFACT_BTN_ID)
            .and_then(|c| c.component_mut::<Button>())
        {
            btn.button.state.disabled = !is_artifact;
        }
        if let Some(btn) = host
            .get_mut(SUMMARY_OPEN_SUBAGENT_BTN_ID)
            .and_then(|c| c.component_mut::<Button>())
        {
            btn.button.state.disabled = !is_subagent;
        }
        if let Some(btn) = host
            .get_mut(SUMMARY_OPEN_SIDECHAT_BTN_ID)
            .and_then(|c| c.component_mut::<Button>())
        {
            btn.button.state.disabled = self.current_scope.is_none();
        }
    }

    fn sync_host_sections(
        &mut self,
        host: &mut ContentHost,
        host_sections: &[(String, Vec<(String, String)>)],
    ) {
        let mut sig = String::new();
        for (title, pairs) in host_sections {
            sig.push_str(title);
            sig.push(':');
            for (k, v) in pairs {
                sig.push_str(k);
                sig.push('=');
                sig.push_str(v);
                sig.push(';');
            }
        }
        if sig == self.last_host_sig {
            return;
        }
        self.last_host_sig = sig;

        let mut next_id = 760_u64;
        let mut alloc = || {
            let id = ComponentId(next_id);
            next_id += 1;
            id
        };
        let mut children = Vec::new();
        for (title, pairs) in host_sections {
            let header_id = alloc();
            let table_id = alloc();
            let rows = pairs.iter().map(|(k, v)| {
                (
                    Content::new(alloc(), TextContent::new(k.clone())),
                    Content::new(alloc(), TextContent::new(v.clone())),
                )
            });
            children.push(Content::new(header_id, TextContent::new(title.clone())));
            children.push(Content::new(table_id, KeyValueTableContent::new(rows)));
        }
        replace_stack_children(host, SUMMARY_HOST_SECTIONS_ID, children);
    }

    pub fn handle_actions(
        &mut self,
        host: &mut ContentHost,
        records: &[ContentActionRecord],
    ) -> Vec<SummaryIntent> {
        let mut out = Vec::new();
        for record in records {
            if let Some(ContentActionPayload::DisclosureExpansion(intent)) = &record.payload {
                if let Some(disc) = host
                    .get_mut(record.action.target)
                    .and_then(|c| c.component_mut::<DisclosureContent>())
                {
                    let _ = disc.confirm(intent, true);
                }
                match record.action.target {
                    id if id == SUMMARY_USAGE_DISC_ID => self.usage_expanded = intent.expanded(),
                    id if id == SUMMARY_TASKS_DISC_ID => self.tasks_expanded = intent.expanded(),
                    _ => {}
                }
                continue;
            }
            if let Some(ContentActionPayload::ListIntent(list)) = &record.payload
                && record.action.target == SUMMARY_LIST_ID
            {
                self.selected_key = Some(list.key);
                self.sync_action_buttons(host);
                if matches!(list.kind, ListIntentKind::Activated)
                    && let Some(target) = self.targets.get(&list.key)
                {
                    out.push(target_to_intent(target));
                }
                continue;
            }
            if matches!(record.action.kind, ContentActionKind::Activate) {
                match record.action.target {
                    id if id == SUMMARY_OPEN_ARTIFACT_BTN_ID => {
                        if let Some(target @ SummaryTarget::Artifact { .. }) =
                            self.selected_key.and_then(|k| self.targets.get(&k))
                        {
                            out.push(target_to_intent(target));
                        }
                    }
                    id if id == SUMMARY_OPEN_SUBAGENT_BTN_ID => {
                        if let Some(target @ SummaryTarget::Subagent { .. }) =
                            self.selected_key.and_then(|k| self.targets.get(&k))
                        {
                            out.push(target_to_intent(target));
                        }
                    }
                    id if id == SUMMARY_OPEN_SIDECHAT_BTN_ID => {
                        if let Some(scope) = &self.current_scope {
                            out.push(SummaryIntent::OpenSideChat {
                                parent: scope.clone(),
                                title: format!("SideChat · {}", scope.session_id),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        out
    }
}

fn target_to_intent(target: &SummaryTarget) -> SummaryIntent {
    match target {
        SummaryTarget::Artifact { scope, path } => SummaryIntent::OpenArtifact {
            scope: scope.clone(),
            path: path.clone(),
        },
        SummaryTarget::Subagent {
            parent,
            child_id,
            title,
        } => SummaryIntent::OpenSubagent {
            parent: parent.clone(),
            child_id: child_id.clone(),
            title: title.clone(),
        },
    }
}
