//! Shared Agent text delegate; all editing and reading algorithms remain upstream.
use crate::transcript;
use tekes_ui_controls::{
    ButtonTextRenderer, ExternalSelection, ExternalUndo, ExternalValueResult, ReadingController,
    TextEditBaseline, TextSurface, composition::*, layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Scene, Size};
/// Agent-selected arguments for a controlled draft synchronization. Validate
/// against the original TekesUI surface before a reading transaction; this
/// value is not a generic editor validator or a cross-event reservation.
#[derive(Debug, PartialEq, Eq)]
pub struct ComposerDraftUpdate {
    expected: TextEditBaseline,
    value: tekes_ui_controls::DraftRevision,
    selection: ExternalSelection,
    undo: ExternalUndo,
    clearing: bool,
}
impl ComposerDraftUpdate {
    pub fn expected(&self) -> &TextEditBaseline {
        &self.expected
    }
    pub fn value(&self) -> &tekes_ui_controls::DraftRevision {
        &self.value
    }
    pub fn selection(&self) -> &ExternalSelection {
        &self.selection
    }
    pub fn undo(&self) -> ExternalUndo {
        self.undo
    }
}
/// Agent layout role. Editing, selection, IME and scrolling delegate to TekesUI unchanged.
#[derive(Debug)]
pub struct AgentText {
    pub field: TextFieldContent,
    pub(super) history: transcript::History,
    pub(super) clear_baseline: Option<TextEditBaseline>,
}
impl AgentText {
    pub(super) fn composer() -> Self {
        let mut text = Self::new(false);
        text.field.surface.presentation = tekes_ui_controls::TextSurfacePresentation::Content;
        text.field.viewport = TextViewport::Grow {
            min: 0.,
            max: text.field.surface.role.style().line_height * 10.,
        };
        text
    }
    pub(super) fn new(transcript: bool) -> Self {
        let mut field = TextFieldContent::new(if transcript { "会话记录" } else { "消息" }, "");
        field.surface = TextSurface::new("", true, transcript);
        if transcript {
            field.viewport = TextViewport::Fill;
            field.reading = Some(ReadingController::default());
        } else {
            let min = field.surface.minimum_outer_height(1.);
            field.viewport = TextViewport::Grow {
                min,
                max: field.surface.role.style().line_height * 10.
                    + 2. * tekes_ui_tokens::proposal::SCROLL_CONTENT_PADDING,
            };
            field.tab_policy = TextTabPolicy::Insert;
        }
        Self {
            field,
            history: transcript::History::default(),
            clear_baseline: None,
        }
    }
}
impl AgentText {
    pub(super) fn prepare_sync_draft(
        &self,
        key: &str,
        revision: u64,
        text: &str,
    ) -> Result<Option<ComposerDraftUpdate>, &'static str> {
        let current = self.field.surface.input.external_value();
        if current.is_some_and(|value| value.identity == key && value.payload == text)
            && (self.field.surface.input.text != text || self.field.surface.input.marked.is_some())
        {
            // A newer publication of the previous authority value cannot erase
            // unacknowledged local input or preedit. The envelope revision still
            // fences old snapshots; unchanged editor values acknowledge newer
            // revisions through the public transaction without resetting state.
            return Ok(None);
        }
        let first = current.is_none_or(|value| value.identity != key);
        let local_pending =
            current.is_some_and(|value| self.field.surface.input.text != value.payload);
        let clearing = !self.field.surface.input.text.is_empty() && text.is_empty();
        if local_pending && self.field.surface.input.text != text && !clearing {
            return Err("authoritative draft conflicts with unacknowledged local editing");
        }
        let expected = if clearing {
            self.clear_baseline
                .clone()
                .unwrap_or_else(|| self.field.surface.input.external_baseline())
        } else {
            self.field.surface.input.external_baseline()
        };
        if local_pending && self.field.surface.input.text != text && self.clear_baseline.is_none() {
            return Err("external clear has no current editor receipt");
        }
        let selection = if first || clearing {
            ExternalSelection::Reset
        } else {
            ExternalSelection::Preserve
        };
        let undo = if first {
            ExternalUndo::Reset
        } else if clearing {
            ExternalUndo::Transaction
        } else {
            ExternalUndo::Preserve
        };
        Ok(Some(ComposerDraftUpdate {
            expected,
            value: tekes_ui_controls::DraftRevision {
                identity: key.into(),
                version: revision,
                payload: text.into(),
            },
            selection,
            undo,
            clearing,
        }))
    }
    pub(super) fn sync_draft(
        &mut self,
        key: &str,
        revision: u64,
        text: &str,
    ) -> Result<bool, &'static str> {
        let Some(update) = self.prepare_sync_draft(key, revision, text)? else {
            return Ok(false);
        };
        let result = self.field.surface.apply_external_value(
            &update.expected,
            &update.value,
            update.selection,
            update.undo,
        );
        match result {
            ExternalValueResult::Applied { .. } => {
                if update.clearing {
                    self.clear_baseline = None;
                }
                Ok(true)
            }
            ExternalValueResult::Unchanged => Ok(false),
            ExternalValueResult::Conflict(_) => {
                Err("controlled editor update rejected; local draft retained")
            }
        }
    }
}
impl Component for AgentText {
    fn action_kinds(&self, result: &ContentResponse) -> Vec<ContentActionKind> {
        self.field.action_kinds(result)
    }

    fn prepare_text_input(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.field.prepare_text_input(text, frame, clip);
    }

    fn measure(&mut self, t: &mut dyn ButtonTextRenderer, a: Size) -> MeasuredChild {
        self.field.measure(t, a)
    }
    fn arrange(&mut self, t: &mut dyn ButtonTextRenderer, f: Rect, c: Rect) {
        self.field.arrange(t, f, c)
    }
    fn paint(
        &mut self,
        s: &mut Scene,
        t: &mut dyn ButtonTextRenderer,
        f: Rect,
        c: Rect,
        p: ContentTheme,
    ) {
        self.field.paint(s, t, f, c, p)
    }
    fn event(&mut self, e: &ContentEvent) -> ContentResponse {
        self.field.event(e)
    }
    fn pointer_event(&mut self, pointer: &PointerEvent) -> ContentResponse {
        self.field.pointer_event(pointer)
    }
    fn key_event(&mut self, key: &ContentKeyEvent) -> ContentResponse {
        self.field.key_event(key)
    }
    fn focusable(&self) -> bool {
        self.field.focusable()
    }
    fn set_focus(&mut self, f: bool, v: bool) {
        self.field.set_focus(f, v)
    }
    fn composing(&self) -> bool {
        self.field.composing()
    }
    fn requires_text_services(&self) -> bool {
        true
    }
    fn text_surface(&self) -> Option<&TextSurface> {
        Some(&self.field.surface)
    }
    fn semantic_frame(&self, f: Rect) -> Rect {
        self.field.semantic_frame(f)
    }
    fn semantics(&self) -> ContentSemantics {
        self.field.semantics()
    }
    fn accessibility_action(
        &mut self,
        p: Option<usize>,
        a: &AccessibilityAction,
    ) -> ContentResponse {
        self.field.accessibility_action(p, a)
    }
}
