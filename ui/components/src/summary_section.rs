//! Agent Summary section composition. Buttons, Disclosure state, focus and
//! input handling remain in TekesUI; no generic accordion engine lives here.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    extended::{Axis, Disclosure, Stack},
    layout::{ContainerLayout, Insets, MeasuredChild},
    layout_view::SeparatorView,
};
use tekes_ui_scene::{Rect, Scene, Size};
use tekes_ui_tokens::Spacing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SectionHeaderKind {
    Summary,
    Sidebar,
}

pub(crate) type Expansion = Rc<RefCell<Disclosure>>;

#[derive(Debug)]
pub(crate) struct SummaryHeader {
    children: Vec<Content>,
    title: Rc<RefCell<String>>,
    shows_divider: Rc<Cell<bool>>,
}
impl SummaryHeader {
    fn new(
        label: ComponentId,
        kind: SectionHeaderKind,
        title: Rc<RefCell<String>>,
        shows_divider: Rc<Cell<bool>>,
    ) -> Self {
        let label = Content::new(label, TextContent::new(""));
        let label = if kind == SectionHeaderKind::Summary {
            label
                .with_label_layout(tekes_ui_text::LabelOptions::single_line(
                    tekes_ui_text::LabelOverflow::TailEllipsis,
                ))
                .expect("published Summary label policy")
        } else {
            label
        };
        Self {
            children: vec![label],
            title,
            shows_divider,
        }
    }
    fn rule_height() -> f32 {
        tekes_ui_tokens::proposal::metric("stroke", "stroke-rule")
            .expect("published separator rule")
    }
    fn divider_reserve(&self) -> f32 {
        if self.shows_divider.get() {
            Self::rule_height() + Spacing::Regular.value()
        } else {
            0.
        }
    }
}
impl Component for SummaryHeader {
    fn children(&self) -> &[Content] {
        &self.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }
    fn fixed_slots(&self) -> bool {
        true
    }
    fn paints_children(&self) -> bool {
        true
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, size: Size) -> MeasuredChild {
        self.children[0]
            .component_mut::<TextContent>()
            .unwrap()
            .text = self.title.borrow().clone();
        let mut measured = self.children[0].measure_intrinsic(text, size);
        measured.size.height += self.divider_reserve();
        measured
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let reserve = self.divider_reserve();
        self.children[0].arrange(
            text,
            Rect::new(
                frame.min_x(),
                frame.min_y() + reserve,
                frame.size.width,
                (frame.size.height - reserve).max(0.),
            ),
            clip,
        );
    }
    fn paint(
        &mut self,
        scene: &mut Scene,
        text: &mut dyn ButtonTextRenderer,
        _: Rect,
        _: Rect,
        theme: ContentTheme,
    ) {
        self.children[0].paint(scene, text, theme);
    }
}

#[derive(Debug)]
pub(crate) struct SummaryRows {
    pub rows: StackContent,
    indent: f32,
    expansion: Expansion,
    disclosure_epoch: Rc<Cell<u64>>,
    arranged: Option<(u64, Vec<ComponentId>)>,
}
impl Component for SummaryRows {
    fn host_reveal_passthrough(&self) -> bool {
        self.active()
            && self.arranged.as_ref().is_some_and(|(epoch, rows)| {
                *epoch == self.disclosure_epoch.get()
                    && rows
                        .iter()
                        .copied()
                        .eq(self.rows.children.iter().map(Content::id))
            })
    }
    fn lifecycle(&mut self, _: Lifecycle) {
        self.arranged = None;
    }
    fn active(&self) -> bool {
        self.expansion.borrow().open
    }
    fn children(&self) -> &[Content] {
        &self.rows.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.rows.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, size: Size) -> MeasuredChild {
        self.arranged = None;
        // Full stable body measurement belongs to the SDK, including closing paint.
        self.rows.measure(
            text,
            Size::new((size.width - self.indent).max(0.), size.height),
        )
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.arranged = None;
        let content = ContainerLayout::new(
            frame,
            Insets::ZERO,
            Insets::new(0., 0., 0., self.indent),
            Some(clip),
        )
        .content();
        self.rows.arrange(text, content, clip);
        if self.active() && !frame.is_empty() && !clip.is_empty() {
            self.arranged = Some((
                self.disclosure_epoch.get(),
                self.rows.children.iter().map(Content::id).collect(),
            ));
        }
    }
}

/// Actual last-arranged section frames. Hidden bodies have no frame; model
/// changes and a disclosure toggle invalidate this query until the next layout.
#[derive(Clone, Debug, PartialEq)]
pub struct SummarySectionLayout {
    pub section_id: String,
    pub header: Rect,
    /// Arranged logical SDK separator band, before ancestor clipping. None for
    /// first/non-repository sections. This is not an ink or pointer target.
    pub divider: Option<Rect>,
    pub body: Option<Rect>,
    pub expanded: bool,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct SummarySectionIds {
    pub header: ComponentId,
    pub title: ComponentId,
    pub label: ComponentId,
    pub body: ComponentId,
}
#[derive(Debug)]
pub(crate) struct SummarySectionView {
    pub body_indent: f32,
    pub section: String,
    control: DisclosureContent,
    kind: SectionHeaderKind,
    title: Rc<RefCell<String>>,
    shows_divider: Rc<Cell<bool>>,
    divider_frame: Option<Rect>,
    divider: SeparatorView,
    pub expansion: Expansion,
    pub layout: Option<SummarySectionLayout>,
    pub disclosure_epoch: Rc<Cell<u64>>,
    arranged_epoch: Option<u64>,
}
impl SummarySectionView {
    pub fn new(
        section: String,
        ids: SummarySectionIds,
        expansion: Expansion,
        kind: SectionHeaderKind,
    ) -> Self {
        let SummarySectionIds {
            header,
            title,
            label,
            body,
        } = ids;
        let epoch = Rc::new(Cell::new(0));
        let model = *expansion.borrow();
        let title_text = Rc::new(RefCell::new(String::new()));
        let shows_divider = Rc::new(Cell::new(false));
        let mut control = DisclosureContent::new(
            header,
            Content::new(
                title,
                SummaryHeader::new(label, kind, title_text.clone(), shows_divider.clone()),
            ),
            Content::new(
                body,
                SummaryRows {
                    rows: StackContent {
                        model: Stack {
                            axis: Axis::Vertical,
                            gap: 0.,
                        },
                        flexible_child: None,
                        children: vec![],
                    },
                    indent: 0.,
                    expansion: expansion.clone(),
                    disclosure_epoch: epoch.clone(),
                    arranged: None,
                },
            ),
            model.open,
        );
        control
            .set_accessible_name("Summary section")
            .expect("nonempty initial name");
        control.set_disabled(model.disabled);
        if kind == SectionHeaderKind::Summary {
            // Preserve the original Summary heading contract through the real
            // SDK owner; Sidebar keeps the default focus and indicator layout.
            control = control
                .with_trigger_focusable(false)
                .with_section_header_layout();
        }
        Self {
            body_indent: 0.,
            section,
            control,
            kind,
            title: title_text,
            shows_divider,
            divider_frame: None,
            divider: SeparatorView::default(),
            expansion,
            layout: None,
            disclosure_epoch: epoch,
            arranged_epoch: None,
        }
    }
    pub fn header(&self) -> &Content {
        &self.control.children()[0]
    }
    fn invalidate(&mut self) {
        self.layout = None;
        self.divider_frame = None;
        self.arranged_epoch = None;
        self.control.children_mut().unwrap()[1]
            .component_mut::<SummaryRows>()
            .unwrap()
            .arranged = None;
    }
    fn project(&mut self) {
        self.expansion.borrow_mut().open = self.control.expanded();
        self.expansion.borrow_mut().disabled = self.control.disabled();
        self.disclosure_epoch.set(
            self.disclosure_epoch
                .get()
                .checked_add(1)
                .expect("Summary disclosure generation exhausted"),
        );
        self.invalidate();
    }
    pub fn set_title(&mut self, title: String) {
        self.control
            .set_accessible_name(title.clone())
            .expect("preflight Summary title");
        *self.title.borrow_mut() = title;
        self.invalidate();
    }
    pub fn set_enabled(&mut self, enabled: bool) {
        self.control.set_disabled(!enabled);
        self.expansion.borrow_mut().disabled = !enabled;
        self.invalidate();
    }
    pub fn set_repository_divider(&mut self, shown: bool) {
        self.shows_divider
            .set(self.kind == SectionHeaderKind::Summary && shown);
        self.invalidate();
    }
    pub fn set_expanded(&mut self, expanded: bool) -> bool {
        if !self.control.set_expanded(expanded) {
            return false;
        }
        self.project();
        true
    }
    pub fn confirm_record(&mut self, record: &ContentActionRecord, accepted: bool) -> bool {
        if record.action.target != self.control.trigger_id()
            || record.action.kind != ContentActionKind::Activate
            || record.mount != self.header().mount_identity()
        {
            return false;
        }
        let Some(ContentActionPayload::DisclosureExpansion(intent)) = &record.payload else {
            return false;
        };
        if !self.control.confirm(intent, accepted) {
            return false;
        }
        if accepted {
            self.project();
        }
        true
    }
    pub fn rows(&self) -> &[Content] {
        &self.control.children()[1]
            .component::<SummaryRows>()
            .unwrap()
            .rows
            .children
    }
    pub fn rows_mut(&mut self) -> &mut Vec<Content> {
        self.invalidate();
        &mut self.control.children_mut().unwrap()[1]
            .component_mut::<SummaryRows>()
            .unwrap()
            .rows
            .children
    }
    pub fn arranged(&self) -> Option<&SummarySectionLayout> {
        self.layout.as_ref().filter(|layout| {
            self.arranged_epoch == Some(self.disclosure_epoch.get())
                && layout.expanded == self.control.expanded()
        })
    }
}
impl Component for SummarySectionView {
    fn lifecycle(&mut self, event: Lifecycle) {
        self.invalidate();
        self.control.lifecycle(event);
    }
    fn validate(&self) -> Result<(), CompositionError> {
        self.control.validate()
    }
    fn children(&self) -> &[Content] {
        self.control.children()
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        self.control.children_mut()
    }
    fn fixed_slots(&self) -> bool {
        self.control.fixed_slots()
    }
    fn paints_children(&self) -> bool {
        self.control.paints_children()
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.invalidate();
        self.control.children_mut().unwrap()[1]
            .component_mut::<SummaryRows>()
            .unwrap()
            .indent = self.body_indent;
        self.control.measure(text, available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.arranged_epoch = None;
        self.control.arrange(text, frame, clip);
        // The separator decorates the full original header band. It is not
        // part of the SDK's narrower, clipped passive title slot.
        let header = self.header().frame();
        self.divider_frame = self.shows_divider.get().then(|| {
            let inset = crate::summary_row_layout::title_inset();
            Rect::new(
                header.min_x() + inset,
                header.min_y(),
                (header.size.width - 2. * inset).max(0.),
                SummaryHeader::rule_height(),
            )
        });
        let body = self.control.body_layout();
        self.layout = Some(SummarySectionLayout {
            section_id: self.section.clone(),
            header: self.header().frame(),
            divider: self.divider_frame,
            body: body.interactive.then(|| {
                ContainerLayout::new(
                    self.control.children()[1].frame(),
                    Insets::ZERO,
                    Insets::new(0., 0., 0., self.body_indent),
                    Some(clip),
                )
                .content()
            }),
            expanded: self.control.expanded(),
        });
        if !frame.is_empty() && !clip.is_empty() {
            self.arranged_epoch = Some(self.disclosure_epoch.get());
            // A fully offscreen body receives an empty actual clip. Its same
            // real SDK arrange can still certify translation via the original
            // owner edge. Carry that qualification, not a reconstructed clip,
            // into the business rows' existing epoch/identity proof.
            if self.control.host_reveal_passthrough() {
                let body = &mut self.control.children_mut().unwrap()[1];
                if !body.frame().is_empty() {
                    let rows = body.component_mut::<SummaryRows>().unwrap();
                    if rows.arranged.is_none() && rows.active() {
                        rows.arranged = Some((
                            self.disclosure_epoch.get(),
                            rows.rows.children.iter().map(Content::id).collect(),
                        ));
                    }
                }
            }
        }
    }
    fn paint(
        &mut self,
        scene: &mut Scene,
        text: &mut dyn ButtonTextRenderer,
        frame: Rect,
        clip: Rect,
        theme: ContentTheme,
    ) {
        self.control.paint(scene, text, frame, clip, theme);
        if let Some(divider) = self.divider_frame {
            self.divider
                .render(scene, divider, Axis::Horizontal, Some(clip), theme.palette);
        }
    }
    fn paint_overlay(
        &mut self,
        scene: &mut Scene,
        text: &mut dyn ButtonTextRenderer,
        frame: Rect,
        clip: Rect,
        theme: ContentTheme,
    ) {
        self.control.paint_overlay(scene, text, frame, clip, theme);
    }
    fn overlay_presented(&self) -> bool {
        self.control.overlay_presented()
    }
    fn host_reveal_passthrough(&self) -> bool {
        // Business arrange/epoch proof is additional to the original SDK owner.
        self.arranged().is_some() && self.control.host_reveal_passthrough()
    }
    fn host_reveal_child_vertical_clip(
        &self,
        child: ComponentId,
    ) -> Option<HostRevealVerticalClip> {
        // Direct children are the unchanged SDK trigger/body Content instances.
        self.control.host_reveal_child_vertical_clip(child)
    }
}

#[derive(Debug)]
pub(crate) struct SummaryDocument {
    pub list: ListContent,
}
impl Component for SummaryDocument {
    fn children(&self) -> &[Content] {
        &self.list.rows.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.list.rows.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        let measured = self.list.measure(text, available);
        MeasuredChild::new(Size::new(
            available.width,
            measured.size.height + 2. * Spacing::Regular.value(),
        ))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let content = ContainerLayout::new(
            frame,
            Insets::ZERO,
            Insets {
                top: Spacing::Regular.value(),
                bottom: Spacing::Regular.value(),
                ..Insets::ZERO
            },
            Some(clip),
        )
        .content();
        self.list.arrange(text, content, clip);
    }
    fn semantics(&self) -> ContentSemantics {
        self.list.semantics()
    }
}

#[cfg(test)]
mod rows_reveal_tests {
    use super::*;
    struct Text;
    impl ButtonTextRenderer for Text {
        fn measure(&mut self, _: &str, _: tekes_ui_tokens::proposal::TextRole, width: f32) -> Size {
            Size::new(width.min(80.), 20.)
        }
        fn draw(
            &mut self,
            _: &mut Scene,
            _: &str,
            _: tekes_ui_tokens::proposal::TextRole,
            _: Rect,
            _: tekes_ui_controls::ButtonTextAlign,
            _: tekes_ui_tokens::Rgba,
        ) {
        }
    }
    // CPU geometry resource seam; no SVG/pixel evidence.
    struct Icons;
    impl tekes_ui_controls::layout_view::IconRenderer for Icons {
        fn prepare_icon(
            &mut self,
            _: &str,
            _: f32,
            _: f32,
            _: tekes_ui_tokens::Rgba,
        ) -> Result<
            tekes_ui_controls::composition::IconImage,
            tekes_ui_controls::composition::IconRenderError,
        > {
            Ok(tekes_ui_controls::composition::IconImage::new(
                tekes_ui_scene::ImageId(99),
                std::rc::Rc::new(()),
            ))
        }
        fn draw_icon(
            &mut self,
            _: &mut tekes_ui_scene::Scene,
            _: &str,
            _: tekes_ui_scene::Rect,
            _: tekes_ui_tokens::Rgba,
        ) {
            panic!("prepared-icon seam required")
        }
    }
    fn render(host: &mut ContentHost) {
        let frame = Rect::new(0., 0., 400., 400.);
        host.render(
            &mut Scene::new(),
            &mut WithIconRenderer::new(&mut Text, &mut Icons),
            frame,
            frame,
            tekes_ui_tokens::Theme::LIGHT.into(),
        )
        .unwrap();
    }
    fn press(host: &mut ContentHost) {
        host.accessibility_action(ComponentId(2), None, AccessibilityAction::Press);
        let records = host.drain_action_records();
        assert_eq!(records.len(), 1);
        assert!(
            host.get_mut(ComponentId(1))
                .unwrap()
                .component_mut::<SummarySectionView>()
                .unwrap()
                .confirm_record(&records[0], true)
        );
    }
    struct MotionText {
        inner: Text,
        now: std::time::Duration,
    }
    impl ButtonTextRenderer for MotionText {
        fn measure(
            &mut self,
            source: &str,
            role: tekes_ui_tokens::proposal::TextRole,
            width: f32,
        ) -> Size {
            self.inner.measure(source, role, width)
        }
        fn draw(
            &mut self,
            scene: &mut Scene,
            source: &str,
            role: tekes_ui_tokens::proposal::TextRole,
            frame: Rect,
            align: tekes_ui_controls::ButtonTextAlign,
            ink: tekes_ui_tokens::Rgba,
        ) {
            self.inner.draw(scene, source, role, frame, align, ink)
        }
        fn motion_context(&self) -> Option<tekes_ui_controls::ControlMotionContext> {
            Some(tekes_ui_controls::ControlMotionContext {
                now: self.now,
                reduced_motion: false,
            })
        }
    }
    fn render_at(host: &mut ContentHost, now: std::time::Duration) {
        let frame = Rect::new(0., 0., 400., 400.);
        let mut text = MotionText { inner: Text, now };
        host.render(
            &mut Scene::new(),
            &mut WithIconRenderer::new(&mut text, &mut Icons),
            frame,
            frame,
            tekes_ui_tokens::Theme::LIGHT.into(),
        )
        .unwrap();
    }
    fn qualified(host: &ContentHost) -> bool {
        host.get(ComponentId(4))
            .unwrap()
            .component::<SummaryRows>()
            .unwrap()
            .host_reveal_passthrough()
    }
    #[test]
    fn sdk_qualification_never_bypasses_unarranged_mid_motion_closed_or_disabled_state() {
        use std::time::Duration;
        let expansion = Rc::new(RefCell::new(Disclosure::default()));
        expansion.borrow_mut().open = false;
        let mut section = SummarySectionView::new(
            "motion".into(),
            SummarySectionIds {
                header: ComponentId(2),
                title: ComponentId(30),
                label: ComponentId(3),
                body: ComponentId(4),
            },
            expansion,
            SectionHeaderKind::Sidebar,
        );
        section.set_title("motion".into());
        section
            .rows_mut()
            .push(Content::new(ComponentId(5), Button::new("original leaf")));
        let mut host = ContentHost::new(Content::new(ComponentId(1), section)).unwrap();
        let ready = |host: &ContentHost| {
            host.get(ComponentId(1))
                .unwrap()
                .component::<SummarySectionView>()
                .unwrap()
                .host_reveal_passthrough()
        };
        assert!(!ready(&host));
        render_at(&mut host, Duration::ZERO);
        assert!(ready(&host));
        assert!(matches!(
            host.plan_host_reveal(ComponentId(5), Default::default(), &[], Default::default()),
            Err(HostRevealError::UnavailableTarget)
        ));
        assert!(
            host.get_mut(ComponentId(1))
                .unwrap()
                .component_mut::<SummarySectionView>()
                .unwrap()
                .set_expanded(true)
        );
        assert!(!ready(&host));
        render_at(&mut host, Duration::ZERO);
        render_at(&mut host, Duration::from_millis(50));
        let section = host
            .get(ComponentId(1))
            .unwrap()
            .component::<SummarySectionView>()
            .unwrap();
        let progress = section.control.body_layout().progress;
        assert!(
            progress > 0. && progress < 1.,
            "must exercise actual SDK intermediate motion: {progress}"
        );
        assert!(!ready(&host));
        assert!(matches!(
            host.plan_host_reveal(ComponentId(5), Default::default(), &[], Default::default()),
            Err(HostRevealError::UnsupportedLayout)
        ));
        render_at(&mut host, Duration::from_secs(2));
        assert!(ready(&host));
        assert!(
            host.plan_host_reveal(ComponentId(5), Default::default(), &[], Default::default())
                .is_ok()
        );
        assert!(
            host.get_mut(ComponentId(1))
                .unwrap()
                .component_mut::<SummarySectionView>()
                .unwrap()
                .set_expanded(false)
        );
        assert!(!ready(&host));
        assert!(matches!(
            host.plan_host_reveal(ComponentId(5), Default::default(), &[], Default::default()),
            Err(HostRevealError::UnavailableTarget)
        ));
        render_at(&mut host, Duration::from_secs(2));
        render_at(&mut host, Duration::from_secs(4));
        assert!(ready(&host));
        host.get_mut(ComponentId(1))
            .unwrap()
            .component_mut::<SummarySectionView>()
            .unwrap()
            .set_enabled(false);
        assert!(!ready(&host));
    }
    #[test]
    fn rows_require_current_arrange_and_reject_changed_rows_and_disclosure_aba() {
        let expansion = Rc::new(RefCell::new(Disclosure::default()));
        expansion.borrow_mut().open = true;
        let mut section = SummarySectionView::new(
            "rows".into(),
            SummarySectionIds {
                header: ComponentId(2),
                title: ComponentId(30),
                label: ComponentId(3),
                body: ComponentId(4),
            },
            expansion,
            SectionHeaderKind::Sidebar,
        );
        section.set_title("section".into());
        section
            .rows_mut()
            .push(Content::new(ComponentId(5), Button::new("first")));
        let mut host = ContentHost::new(Content::new(ComponentId(1), section)).unwrap();
        host.set_action_events(true);
        assert!(!qualified(&host));
        render(&mut host);
        assert!(qualified(&host));
        let plan = host
            .plan_host_reveal(ComponentId(5), Default::default(), &[], Default::default())
            .unwrap();
        host.get_mut(ComponentId(1))
            .unwrap()
            .component_mut::<SummarySectionView>()
            .unwrap()
            .rows_mut()[0] = Content::new(ComponentId(5), Button::new("replacement"));
        assert!(!qualified(&host));
        assert!(host.commit_host_reveal(plan).is_err());
        render(&mut host);
        assert!(qualified(&host));
        let plan = host
            .plan_host_reveal(ComponentId(5), Default::default(), &[], Default::default())
            .unwrap();
        host.get_mut(ComponentId(1))
            .unwrap()
            .component_mut::<SummarySectionView>()
            .unwrap()
            .rows_mut()
            .push(Content::new(ComponentId(6), Button::new("second")));
        assert!(!qualified(&host));
        assert!(host.commit_host_reveal(plan).is_err());
        render(&mut host);
        assert!(qualified(&host));
        // SDK confirmation updates the business projection and arrange epoch.
        // Reopen without arranging: equal open value cannot revive old evidence.
        press(&mut host);
        assert!(!qualified(&host));
        render(&mut host);
        assert!(!qualified(&host));
        press(&mut host);
        assert!(!qualified(&host));
        render(&mut host);
        assert!(qualified(&host));
        press(&mut host);
        press(&mut host);
        assert!(
            host.get(ComponentId(4))
                .unwrap()
                .component::<SummaryRows>()
                .unwrap()
                .active()
        );
        assert!(!qualified(&host));
        render(&mut host);
        assert!(qualified(&host));
        host.get_mut(ComponentId(4))
            .unwrap()
            .component_mut::<SummaryRows>()
            .unwrap()
            .measure(&mut Text, Size::new(300., 400.));
        assert!(!qualified(&host));
        render(&mut host);
        assert!(qualified(&host));
        let frame = Rect::new(0., 0., 400., 400.);
        host.get_mut(ComponentId(1))
            .unwrap()
            .component_mut::<SummarySectionView>()
            .unwrap()
            .arrange(&mut Text, frame, Rect::default());
        assert!(
            !qualified(&host),
            "an empty outer ancestor clip grants no rows proof"
        );
        assert!(
            !host
                .get(ComponentId(1))
                .unwrap()
                .component::<SummarySectionView>()
                .unwrap()
                .host_reveal_passthrough()
        );
        host.get_mut(ComponentId(1))
            .unwrap()
            .component_mut::<SummarySectionView>()
            .unwrap()
            .arrange(&mut Text, Rect::default(), frame);
        assert!(
            !qualified(&host),
            "an empty outer allocation grants no rows proof"
        );
    }
}
