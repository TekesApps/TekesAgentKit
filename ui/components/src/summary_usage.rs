//! ClientKit Summary usage placement: title on the left, reported value on the
//! right. This composes library text/layout; it owns no counters or editor.
use crate::SummaryRow;
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    extended::Axis,
    layout::{AlignmentOverride, ContainerLayout, Insets, MeasuredChild, layout_measured_children},
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::{Spacing, proposal};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SummaryUsageRowIds {
    pub row: ComponentId,
    pub title: ComponentId,
    pub value: ComponentId,
}

/// Product-formatting seam for the existing Summary facts. Both strings remain
/// complete in the library text controls and accessibility tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SummaryUsageRowText {
    pub title: String,
    pub value: String,
}
impl SummaryUsageRowText {
    /// Default Chinese copy, matching ClientKit's six usage rows and formatting
    /// precision. No estimated costs, percentages, or token totals are added.
    pub fn read(row: &SummaryRow) -> Self {
        let title = match row.id.as_str() {
            "total" => "总计",
            "input" => "输入",
            "output" => "输出",
            "cache" => "缓存",
            "hit-rate" => "命中率",
            "cost" => "成本",
            _ => &row.title,
        }
        .to_owned();
        let value = match row.id.as_str() {
            "total" | "input" | "output" | "cache" => row.value.as_u64().map(grouped_count),
            "hit-rate" if row.value.is_null() => Some("—".into()),
            "hit-rate" => row
                .value
                .as_f64()
                .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
                .map(|v| format!("{:.1}%", v * 100.)),
            "cost" => row
                .value
                .as_f64()
                .filter(|v| v.is_finite() && *v >= 0.)
                .map(|v| {
                    let v = if v == 0. { 0. } else { v };
                    let mut value = format!("{v:.6}");
                    while value.ends_with('0') && value.len() - value.find('.').unwrap() > 3 {
                        value.pop();
                    }
                    let (integer, fraction) = value.split_once('.').unwrap();
                    format!("US${}.{}", grouped_digits(integer), fraction)
                }),
            _ if row.value.is_null() => None,
            _ => Some(
                row.value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| row.value.to_string()),
            ),
        }
        .unwrap_or_else(|| "未知".into());
        Self { title, value }
    }
}
fn grouped_count(value: u64) -> String {
    grouped_digits(&value.to_string())
}
fn grouped_digits(digits: &str) -> String {
    let mut output = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            output.push(',');
        }
        output.push(digit);
    }
    output
}

#[derive(Debug)]
pub(crate) struct SummaryUsageRow {
    children: Vec<Content>,
}
impl SummaryUsageRow {
    pub fn new(title: ComponentId, value: ComponentId) -> Self {
        Self {
            children: vec![
                Content::new(
                    title,
                    TextFieldContent::read_only_content("", "", proposal::TextRole::UiBody),
                ),
                Content::new(
                    value,
                    TextContent {
                        text: String::new(),
                        role: proposal::TextRole::UiCaption,
                    },
                ),
            ],
        }
    }
    pub fn update(&mut self, revision: u64, text: SummaryUsageRowText) {
        let id = self.children[0].id();
        crate::summary_text::update_text(
            self.children[0]
                .component_mut::<TextFieldContent>()
                .unwrap(),
            id,
            revision,
            text.title,
        );
        self.children[1]
            .component_mut::<TextContent>()
            .unwrap()
            .text = text.value;
    }
    fn measurements(
        &mut self,
        text: &mut dyn ButtonTextRenderer,
        width: f32,
    ) -> [MeasuredChild; 2] {
        let gap = Spacing::Regular.value().min(width.max(0.));
        let mut value = self.children[1].measure(text, Size::new(width.max(0.), 1e8));
        value.size.width = value.size.width.min(width.max(0.));
        let title_width = (width - value.size.width - gap).max(0.);
        let mut title = self.children[0].measure(text, Size::new(title_width, 1e8));
        title.size.width = title.size.width.min(title_width);
        [title, value]
    }
}
impl Component for SummaryUsageRow {
    fn children(&self) -> &[Content] {
        &self.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        let inset = Spacing::Regular.value();
        let sizes = self.measurements(text, (available.width - 2. * inset).max(0.));
        MeasuredChild::new(Size::new(
            available.width,
            sizes[0].size.height.max(sizes[1].size.height) + 2. * inset,
        ))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let bounds = ContainerLayout::new(
            frame,
            Insets::ZERO,
            Insets::new(
                Spacing::Regular.value(),
                Spacing::Regular.value(),
                Spacing::Regular.value(),
                Spacing::Regular.value(),
            ),
            Some(clip),
        )
        .content();
        let sizes = self.measurements(text, bounds.size.width);
        // A long title can wrap. Keep the value beside its first line rather
        // than centering it against the entire multi-line title.
        let line = text
            .measure("Mg", proposal::TextRole::UiBody, bounds.size.width)
            .height;
        let placement_bounds = Rect::new(
            bounds.min_x(),
            bounds.min_y(),
            bounds.size.width,
            line.max(sizes[1].size.height),
        );
        let positions = layout_measured_children(
            placement_bounds,
            Axis::Horizontal,
            &sizes,
            Spacing::Regular.value(),
            proposal::layout::ROW,
            AlignmentOverride {
                main_axis: Some(proposal::layout::MainAxisDistribution::SpaceBetween),
                cross_axis: Some(proposal::layout::CrossAxisAlignment::Leading),
            },
        );
        for (i, (node, position)) in self.children.iter_mut().zip(positions).enumerate() {
            let mut frame = position.frame;
            if i == 1 {
                frame.origin.y += ((line - frame.size.height) / 2.).max(0.);
            }
            let visible = frame.intersection(&clip).unwrap_or_default();
            node.arrange(text, frame, visible);
        }
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, "")
    }
}
