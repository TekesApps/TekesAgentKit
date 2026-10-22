//! Agent-owned composition of public TekesUI controls. No endpoint or Run state machine.
mod sidebar_visibility;
mod tool_run;
pub use tekes_agent_presentation::{
    TranscriptReasoningDisclosure, TranscriptThinkingCall, TranscriptThinkingInput,
    TranscriptThinkingItem, TranscriptThinkingMode, TranscriptThinkingNarration,
    TranscriptThinkingPlaceholder, TranscriptThinkingPolicy,
};
pub use tekes_agent_presentation::{TranscriptToolNameCount, TranscriptToolRunSummary};
pub use tool_run::*;
mod tool_detail_content;
pub use sidebar_visibility::*;
pub use tool_detail_content::ToolDetailSectionIds;
mod sidebar_paging;
pub use sidebar_paging::*;
mod feedback;
pub use feedback::*;
mod readonly_reading;
pub use readonly_reading::*;
mod readonly_transcript;
pub use readonly_transcript::*;
mod run_column;
pub use run_column::*;
mod turn_preview;
mod turn_rail;
pub use turn_preview::TranscriptTurnPreview;
mod turn_rail_action;
mod turn_rail_component;
pub use turn_rail::*;
pub use turn_rail_component::*;
mod run_port;
pub use run_port::*;
mod run_reading;
pub use run_reading::*;
mod run_navigation;
pub use run_navigation::*;
mod actionable;
pub use actionable::*;
mod annotation_editor;
mod composer;
mod composer_notice;
pub use composer_notice::*;
mod composer_layout;
pub use composer_layout::{ComposerLayoutGeometry, ComposerPrimaryActionKind};
mod composer_uploads;
pub use annotation_editor::*;
pub use composer_uploads::*;
mod composer_context;
pub use composer_context::*;
mod composer_choices;
pub use composer_choices::*;
mod editor;
#[doc(hidden)]
pub use editor::AgentText;
pub use editor::ComposerDraftUpdate;
mod indicators;
mod side_panel;
mod side_panel_navigation;
pub use side_panel_navigation::*;
mod side_panel_launcher;
pub use side_panel_launcher::*;
mod side_panel_layout;
pub use side_panel_layout::{SidePanelLayout, SidePanelLayoutGeometry};
mod sidebar;
mod sidebar_options;
pub use sidebar_options::*;
mod sidebar_session_menu;
pub use sidebar_session_menu::*;
mod sidebar_search;
pub use sidebar_search::*;
mod summary;
mod summary_action_row;
mod summary_row_layout;
pub use summary_action_row::SummaryActionRow;
pub use summary_row_layout::SummaryDefaultRowIds;
mod summary_section;
mod summary_text;
mod summary_usage;
pub use summary_section::SummarySectionLayout;
pub use summary_usage::{SummaryUsageRowIds, SummaryUsageRowText};
mod thread_rename;
pub use thread_rename::*;
mod transcript_content;
pub use transcript_content::*;
mod transcript_identity;
pub use transcript_identity::*;
mod historical_input;
mod input_message;
pub use historical_input::*;
mod output_message;
mod tool_activity;
pub use output_message::*;
pub use tool_activity::*;
mod transcript;
pub use input_message::*;
mod transcript_paging;
mod turn_footnote;
mod turn_output_copy;
pub use composer::*;
pub use indicators::*;
pub use side_panel::*;
pub use sidebar::*;
use std::collections::BTreeMap;
pub use summary::*;
pub use tekes_agent_presentation::RunSnapshot;
pub use tekes_agent_presentation::{ComposerDestination, RunScope};
pub use tekes_agent_presentation::{
    TranscriptToolChangeCount, TranscriptToolHeader, TranscriptToolHeaderRole,
    TranscriptToolHeaderSpan, TranscriptToolReport, TranscriptToolReportWording,
    TranscriptToolRowInputs,
};
pub use tekes_agent_presentation::{
    TranscriptToolDetailKind, TranscriptToolDetailSection, TranscriptToolDetailWording,
    TranscriptToolDetails,
};
use tekes_ui_controls::{
    ButtonTextRenderer, TextSurface,
    composition::*,
    extended::{Axis, Stack},
};
use tekes_ui_scene::{Rect, Scene};
use tekes_ui_tokens::{Palette, Spacing};
pub use transcript_paging::*;
pub use turn_footnote::*;
pub use turn_output_copy::*;

pub const ROOT: ComponentId = ComponentId(1);
pub const STATUS: ComponentId = ComponentId(2);
pub const TRANSCRIPT: ComponentId = ComponentId(3);
pub const COMPOSER: ComponentId = ComponentId(4);
pub const ACTIONS: ComponentId = ComponentId(5);
pub const SEND: ComponentId = ComponentId(6);
pub const CANCEL: ComponentId = ComponentId(7);
pub const EARLIER: ComponentId = ComponentId(8);
pub const LATEST: ComponentId = ComponentId(9);
pub const RECONNECT: ComponentId = ComponentId(10);
pub const SESSION_A: ComponentId = ComponentId(11);
pub const SESSION_B: ComponentId = ComponentId(12);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    Edit {
        destination: ComposerDestination,
        text: String,
    },
    Send {
        destination: ComposerDestination,
    },
    Cancel {
        scope: RunScope,
    },
    Earlier {
        scope: RunScope,
    },
    Reconnect,
    Select(&'static str),
}

/// Protocol/controls test harness. This is not the reusable Agent RunPort API.
pub struct RunSurface {
    pub tree: ContentHost,
    selected: Option<ComposerDestination>,
    editors: BTreeMap<String, Content>,
    transcripts: BTreeMap<String, Content>,
    pending_intents: Vec<Intent>,
    committed: String,
    source_revision: u64,
}
impl Default for RunSurface {
    fn default() -> Self {
        Self::new()
    }
}
impl RunSurface {
    pub fn new() -> Self {
        let buttons = [
            (SESSION_A, "会话 A"),
            (SESSION_B, "会话 B"),
            (EARLIER, "更早记录"),
            (LATEST, "回到最新"),
            (RECONNECT, "重新连接"),
            (CANCEL, "停止"),
            (SEND, "发送"),
        ];
        let actions = Content::new(
            ACTIONS,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: buttons
                    .into_iter()
                    .map(|(id, label)| Content::new(id, ButtonContent::new(label)))
                    .collect(),
            },
        );
        let mut tree = ContentHost::new(Content::new(
            ROOT,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: Some(TRANSCRIPT),
                children: vec![
                    Content::new(STATUS, TextContent::new("正在连接")),
                    Content::new(TRANSCRIPT, AgentText::new(true)),
                    Content::new(COMPOSER, AgentText::new(false)),
                    actions,
                ],
            },
        ))
        .expect("unique Agent component IDs");
        tree.set_action_events(true);
        Self {
            tree,
            selected: None,
            editors: BTreeMap::new(),
            transcripts: BTreeMap::new(),
            pending_intents: Vec::new(),
            committed: String::new(),
            source_revision: 0,
        }
    }
    pub fn editor(&self) -> &TextSurface {
        &self
            .tree
            .get(COMPOSER)
            .unwrap()
            .component::<AgentText>()
            .unwrap()
            .field
            .surface
    }
    pub fn composing(&self) -> bool {
        self.editor().input.marked.is_some()
    }
    pub fn transcript(&self) -> &AgentText {
        self.tree
            .get(TRANSCRIPT)
            .unwrap()
            .component::<AgentText>()
            .unwrap()
    }
    fn text_mut(&mut self, id: ComponentId) -> &mut AgentText {
        self.tree
            .get_mut(id)
            .unwrap()
            .component_mut::<AgentText>()
            .unwrap()
    }
    fn enabled(&mut self, id: ComponentId, on: bool) {
        self.tree
            .get_mut(id)
            .unwrap()
            .component_mut::<ButtonContent>()
            .unwrap()
            .button
            .state
            .disabled = !on;
    }
    pub fn sync(&mut self, s: &RunSnapshot) -> Result<(), &'static str> {
        if s.revision == 0 || s.revision < self.source_revision {
            return Err("stale or unversioned presentation snapshot");
        }
        // Freeze any low-level queued action against the still-presented
        // destination before applying a newer selection/model snapshot.
        self.collect_pending_intents();
        self.source_revision = s.revision;
        let destination = s.selection.as_ref().map(|v| v.destination.clone());
        if self.selected != destination {
            if self.composing() {
                return Err(
                    "finish or cancel IME composition before selecting a different session",
                );
            }
            self.tree.focus(None, false);
            let editor = self.tree.remove(COMPOSER).expect("mounted editor");
            let transcript = self.tree.remove(TRANSCRIPT).expect("mounted transcript");
            if let Some(old) = &self.selected {
                self.editors.insert(old.storage_key(), editor);
                self.transcripts.insert(old.storage_key(), transcript);
            }
            let key = destination
                .as_ref()
                .map(ComposerDestination::storage_key)
                .unwrap_or_default();
            let editor = self
                .editors
                .remove(&key)
                .unwrap_or_else(|| Content::new(COMPOSER, AgentText::new(false)));
            let transcript = self
                .transcripts
                .remove(&key)
                .unwrap_or_else(|| Content::new(TRANSCRIPT, AgentText::new(true)));
            self.tree.insert(ROOT, transcript).unwrap();
            self.tree.insert(ROOT, editor).unwrap();
            self.tree
                .reorder(ROOT, &[STATUS, TRANSCRIPT, COMPOSER, ACTIONS])
                .unwrap();
            self.selected = destination;
            self.committed = s.draft.text.clone();
        }
        let key = self
            .selected
            .as_ref()
            .map(ComposerDestination::storage_key)
            .unwrap_or_else(|| "unselected".into());
        let sync = self
            .text_mut(COMPOSER)
            .sync_draft(&key, s.revision, &s.draft.text);
        match sync {
            Ok(true) if !self.composing() => self.committed = self.editor().input.text.clone(),
            Ok(_) => {}
            Err(error) => {
                self.enabled(SEND, false);
                return Err(error);
            }
        }
        let transcript = self.text_mut(TRANSCRIPT);
        transcript
            .history
            .sync(&mut transcript.field, &key, &s.transcript)
            .map_err(|_| "transcript document update rejected")?;
        self.tree
            .get_mut(STATUS)
            .unwrap()
            .component_mut::<TextContent>()
            .unwrap()
            .text = format!(
            "连接：{:?}  ·  {}{}",
            s.connection.phase,
            self.selected
                .as_ref()
                .map(|d| match d {
                    ComposerDestination::Run { scope } => scope.session_id.as_str(),
                    _ => "新会话",
                })
                .unwrap_or("未选择"),
            if s.transcript.is_stale {
                " · 历史待同步"
            } else {
                ""
            }
        );
        self.enabled(
            SEND,
            s.can_submit
                && !self.composing()
                && self.editor().input.text == s.draft.text
                && !s.run.as_ref().is_some_and(|r| r.can_cancel),
        );
        self.enabled(CANCEL, s.run.as_ref().is_some_and(|r| r.can_cancel));
        self.enabled(EARLIER, s.transcript.has_more_before);
        self.enabled(SESSION_A, !self.composing());
        self.enabled(SESSION_B, !self.composing());
        Ok(())
    }
    fn begin_input(&mut self, e: &ContentEvent) -> bool {
        self.collect_pending_intents();
        if self.composing()
            && match e {
                ContentEvent::PointerDown(p) => {
                    !self.tree.get(COMPOSER).unwrap().frame().contains(*p)
                }
                ContentEvent::Pointer(p) if p.phase == PointerPhase::Down && p.is_primary() => {
                    !self
                        .tree
                        .get(COMPOSER)
                        .unwrap()
                        .frame()
                        .contains(p.position)
                }
                _ => false,
            }
        {
            return true;
        }
        false
    }
    pub fn event(&mut self, e: &ContentEvent) -> Vec<Intent> {
        if self.begin_input(e) {
            return std::mem::take(&mut self.pending_intents);
        }
        self.tree.event(e);
        self.drain_intents()
    }
    pub fn event_with_text(
        &mut self,
        e: &ContentEvent,
        text: &mut dyn ButtonTextRenderer,
    ) -> Result<Vec<Intent>, CompositionError> {
        if self.begin_input(e) {
            return Ok(std::mem::take(&mut self.pending_intents));
        }
        self.tree.event_with_text(e, text)?;
        Ok(self.drain_intents())
    }
    /// Semantic actions use the same immediate consumer-owned scope capture;
    /// no synthetic pointer move or control counter polling is needed.
    pub fn accessibility_action(
        &mut self,
        id: ComponentId,
        part: Option<usize>,
        action: AccessibilityAction,
    ) -> Vec<Intent> {
        self.collect_pending_intents();
        if self.composing() && id != COMPOSER {
            return std::mem::take(&mut self.pending_intents);
        }
        self.tree.accessibility_action(id, part, action);
        self.drain_intents()
    }
    /// Owned intents may be processed after selection changes without retargeting.
    /// Standard event/AX entry points drain immediately. Low-level tree callers
    /// must drain too; sync freezes their pending actions before changing scope.
    pub fn drain_intents(&mut self) -> Vec<Intent> {
        self.collect_pending_intents();
        std::mem::take(&mut self.pending_intents)
    }
    fn collect_pending_intents(&mut self) {
        if !self.composing() && self.editor().input.text != self.committed {
            self.committed = self.editor().input.text.clone();
            if let Some(destination) = &self.selected {
                self.pending_intents.push(Intent::Edit {
                    destination: destination.clone(),
                    text: self.committed.clone(),
                });
            }
        }
        for action in self.tree.drain_actions() {
            // Editor Submit remains a generic intent; AgentKit's current product
            // contract is explicit-button Send, not a keyboard-send migration.
            if action.kind != ContentActionKind::Activate {
                continue;
            }
            match action.target {
                SEND if !self.composing() => {
                    if let Some(destination) = self.selected.clone() {
                        let receipt = self.editor().input.external_baseline();
                        self.text_mut(COMPOSER).clear_baseline = Some(receipt);
                        self.pending_intents.push(Intent::Send { destination });
                    }
                }
                CANCEL | EARLIER => {
                    if let Some(ComposerDestination::Run { scope }) = &self.selected {
                        self.pending_intents.push(if action.target == CANCEL {
                            Intent::Cancel {
                                scope: scope.clone(),
                            }
                        } else {
                            Intent::Earlier {
                                scope: scope.clone(),
                            }
                        });
                    }
                }
                RECONNECT => self.pending_intents.push(Intent::Reconnect),
                SESSION_A if !self.composing() => self.pending_intents.push(Intent::Select("a")),
                SESSION_B if !self.composing() => self.pending_intents.push(Intent::Select("b")),
                LATEST => self.follow_latest(),
                _ => {}
            }
        }
    }
    pub fn follow_latest(&mut self) {
        let t = self.text_mut(TRANSCRIPT);
        t.field
            .reading
            .as_mut()
            .unwrap()
            .follow_latest(&mut t.field.surface);
    }
    pub fn render(
        &mut self,
        s: &mut Scene,
        t: &mut dyn ButtonTextRenderer,
        f: Rect,
        c: Rect,
        p: Palette,
    ) -> Result<(), CompositionError> {
        self.tree.render(s, t, f, c, p)
    }
}

pub use tekes_agent_presentation::{
    TranscriptBlockContent, TranscriptContentBlock, TranscriptDelivery, TranscriptDocument,
    TranscriptEventOrder, TranscriptFailure, TranscriptInput, TranscriptItem, TranscriptItemKind,
    TranscriptMessageKey, TranscriptOrderedEvent, TranscriptOutput, TranscriptSection,
    TranscriptSnapshot, TranscriptSourcePosition, TranscriptToolCall, TranscriptToolCardModel,
    TranscriptToolDiffHunk, TranscriptToolRead, TranscriptToolReadLine, TranscriptToolRowModel,
    TranscriptToolRowState, TranscriptToolRowWording, TranscriptToolSearch,
    TranscriptToolSearchFile, TranscriptToolSearchMatch, TranscriptToolTerminal, TranscriptToolWeb,
    TranscriptToolWebSource, TranscriptTurn,
};

mod sidebar_preferences;
pub use sidebar_preferences::*;

mod sidebar_folders;
pub use sidebar_folders::*;

mod sidebar_navigation;
pub use sidebar_navigation::SidebarSelectionNavigation;

mod sidebar_catalog;
pub use sidebar_catalog::*;

mod sidebar_reveal;
pub use sidebar_reveal::*;

mod summary_custom;
pub use summary_custom::{
    SummaryCustomRowError, SummaryCustomRowSpec, SummaryCustomRowToken, install_summary_custom_row,
    publish_summary_context_update,
};

mod turn_footnote_timing;
pub use turn_footnote_timing::*;

// Re-export shared read-only goal facts; the UI does not define another lifecycle.
pub use tekes_agent_presentation::{GoalFacts, GoalPhase};
