//! Run Summary surface aligned with Tekes (`SummaryFacts`: usage caption, status-icon Tasks,
//! SubAgents/Artifacts clickable navigation list, and optional host metadata sections).

use crate::ResponsiveWidthMode;
use crate::ids::*;
use crate::ui_helpers::{caption_text, heading_text, icon_row, secondary_text};
use std::collections::BTreeMap;
use tekes_agent_presentation::{RunScope, SummaryAction, SummaryFacts};
use tekes_ui_controls::{
    ScrollArea,
    composition::{
        ComponentId, Content, ContentActionPayload, ContentActionRecord, ContentHost,
        ListDisplayUpdate, ListIntentKind, ListRowDisplay, ScrollContent, SelectableListContent,
        StackContent, TextContent,
    },
    extended::{
        Axis, KeyedListItem, KeyedListSnapshot, ListBudget, ListDatasetKey, ListItemKey, Stack,
    },
};
use tekes_ui_tokens::{Spacing, proposal::layout::MainAxisDistribution};

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
    next_key: u64,
    key_map: BTreeMap<String, ListItemKey>,
    targets: BTreeMap<ListItemKey, SummaryTarget>,
    selected_key: Option<ListItemKey>,
    last_list_sig: String,
    last_tasks_sig: String,
    last_usage_sig: String,
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
            next_key: 1,
            key_map: BTreeMap::new(),
            targets: BTreeMap::new(),
            selected_key: None,
            last_list_sig: String::new(),
            last_tasks_sig: String::new(),
            last_usage_sig: String::new(),
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
        let header = Content::new(SUMMARY_HEADER_ID, heading_text("Summary"));
        let usage_caption = Content::new(SUMMARY_USAGE_CAPTION_ID, caption_text(""));

        let tasks_header = Content::new(SUMMARY_TASKS_HEADER_ID, caption_text("Tasks"));
        let tasks_stack = Content::new(
            SUMMARY_TASKS_STACK_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: Vec::new(),
            },
        );

        let nav_header = Content::new(SUMMARY_NAV_HEADER_ID, caption_text("SubAgents & Artifacts"));

        let initial_snap = KeyedListSnapshot {
            dataset: ListDatasetKey(2),
            items: Vec::new(),
            display_items: Vec::new(),
        };
        let mut initial_rows = Vec::new();
        let mut nav_list = SelectableListContent::keyed(
            "SubAgents and Artifacts",
            &initial_snap,
            &mut initial_rows,
            Self::budget(),
        )
        .expect("empty summary list");
        nav_list.set_navigation_layout(true);
        nav_list.set_navigation_selection_fill_only(true);

        let host_sections = Content::new(
            SUMMARY_HOST_SECTIONS_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
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
                    gap: Spacing::Relaxed.value(),
                },
                flexible_child: None,
                children: vec![
                    header,
                    usage_caption,
                    tasks_header,
                    tasks_stack,
                    nav_header,
                    Content::new(SUMMARY_LIST_ID, nav_list),
                    host_sections,
                ],
            },
        );

        Content::new(
            SUMMARY_SCROLL_ID,
            ScrollContent::new(ScrollArea::default(), stack)
                .scrollbar(true)
                .content_padding(Spacing::Loose.value()),
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

        let usage_text = if let Some(u) = facts.and_then(|f| f.usage.as_ref()) {
            let total = u.total_tokens();
            if let Some(rate) = u.cache_hit_rate() {
                format!("{total} tokens · {:.0}% cache hit", rate * 100.0)
            } else if total > 0 {
                format!("{total} tokens")
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        if usage_text != self.last_usage_sig {
            self.last_usage_sig = usage_text.clone();
            if let Some(caption) = host
                .get_mut(SUMMARY_USAGE_CAPTION_ID)
                .and_then(|c| c.component_mut::<TextContent>())
            {
                caption.text = usage_text;
            }
        }

        self.sync_tasks_section(host, facts);
        self.sync_nav_list(host, facts);
        self.sync_host_sections(host, host_sections);
    }

    fn sync_tasks_section(&mut self, host: &mut ContentHost, facts: Option<&SummaryFacts>) {
        let mut sig = String::new();
        let mut rows = Vec::new();
        if let Some(f) = facts {
            for (idx, t) in f.tasks.iter().enumerate() {
                let status = t.canonical_status();
                sig.push_str(&format!("{}:{}:{}|", idx, status, t.name));
                let icon_name = match status {
                    "completed" => "tekes-goal-completed",
                    "failed" => "tekes-error",
                    _ => "tekes-pending-task",
                };
                let base = SUMMARY_TASK_ROW_BASE + (idx as u64) * 4;
                rows.push(icon_row(
                    ComponentId(base),
                    ComponentId(base + 1),
                    ComponentId(base + 2),
                    icon_name,
                    &t.name,
                ));
            }
        }
        if rows.is_empty() {
            sig = "empty".to_owned();
            rows.push(Content::new(
                ComponentId(SUMMARY_TASK_ROW_BASE),
                secondary_text("No active tasks"),
            ));
        }
        if sig != self.last_tasks_sig {
            self.last_tasks_sig = sig;
            replace_stack_children(host, SUMMARY_TASKS_STACK_ID, rows);
        }
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
                    let (token, icon_name, label, target) = match action {
                        SummaryAction::OpenArtifact { scope, path } => (
                            format!("artifact:{path}"),
                            "tekes-file-code",
                            path.clone(),
                            SummaryTarget::Artifact { scope, path },
                        ),
                        SummaryAction::OpenSubagent { parent, child_id } => (
                            format!("subagent:{child_id}"),
                            "tekes-task",
                            row.title.clone(),
                            SummaryTarget::Subagent {
                                parent,
                                child_id,
                                title: row.title,
                            },
                        ),
                        SummaryAction::OpenTask { task_id, scope } => (
                            format!("task:{task_id}"),
                            "tekes-task",
                            row.title.clone(),
                            SummaryTarget::Subagent {
                                parent: scope.clone(),
                                child_id: scope.session_id,
                                title: row.title,
                            },
                        ),
                    };
                    let key = self.key_for(&token);
                    next_targets.insert(key, target);
                    sig.push_str(&format!("{}:{icon_name}:{label}|", key.0));
                    items.push(KeyedListItem {
                        key,
                        name: label.clone(),
                    });
                    display_items.push(key);
                    let decor_base = SUMMARY_ROW_DECOR_BASE + key.0 * 4;
                    rows.push(ListRowDisplay {
                        key,
                        row_id: ComponentId(SUMMARY_ROW_BASE + key.0),
                        display: ListDisplayUpdate::Replace(icon_row(
                            ComponentId(decor_base),
                            ComponentId(decor_base + 1),
                            ComponentId(decor_base + 2),
                            icon_name,
                            label,
                        )),
                    });
                }
            }
        }
        self.targets = next_targets;
        let nav_title = if items.is_empty() {
            ""
        } else {
            "SubAgents & Artifacts"
        };
        if let Some(hdr) = host
            .get_mut(SUMMARY_NAV_HEADER_ID)
            .and_then(|c| c.component_mut::<TextContent>())
            && hdr.text != nav_title
        {
            hdr.text = nav_title.to_owned();
        }

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
            children.push(Content::new(header_id, caption_text(title.clone())));
            for (k, v) in pairs {
                let row_id = alloc();
                let key_id = alloc();
                let val_id = alloc();
                let row = StackContent {
                    model: Stack {
                        axis: Axis::Horizontal,
                        gap: Spacing::Tight.value(),
                    },
                    flexible_child: Some(key_id),
                    children: vec![
                        Content::new(key_id, secondary_text(k.clone())),
                        Content::new(val_id, TextContent::new(v.clone())),
                    ],
                }
                .into_content_with_distribution(row_id, MainAxisDistribution::SpaceBetween);
                children.push(row);
            }
        }
        replace_stack_children(host, SUMMARY_HOST_SECTIONS_ID, children);
    }

    pub fn handle_actions(
        &mut self,
        _host: &mut ContentHost,
        records: &[ContentActionRecord],
    ) -> Vec<SummaryIntent> {
        let mut out = Vec::new();
        for record in records {
            if let Some(ContentActionPayload::ListIntent(list)) = &record.payload
                && record.action.target == SUMMARY_LIST_ID
            {
                self.selected_key = Some(list.key);
                if matches!(list.kind, ListIntentKind::Activated)
                    && let Some(target) = self.targets.get(&list.key)
                {
                    out.push(target_to_intent(target));
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
