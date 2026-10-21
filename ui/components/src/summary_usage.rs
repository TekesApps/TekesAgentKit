//! ClientKit Summary usage placement: title on the left, reported value on the
//! right. This composes library text/layout; it owns no counters or editor.
use crate::SummaryRow;
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    layout::{ContainerLayout, Insets, MeasuredChild},
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
                )
                .with_label_layout(tekes_ui_text::LabelOptions::single_line(
                    tekes_ui_text::LabelOverflow::Clip,
                ))
                .expect("original single-line Summary metadata"),
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
        let inset = crate::summary_row_layout::title_inset();
        let sizes = self.measurements(text, (available.width - 2. * inset).max(0.));
        MeasuredChild::new(Size::new(
            available.width,
            sizes[0]
                .size
                .height
                .max(crate::summary_row_layout::body_line(text))
                + 2. * Spacing::Regular.value(),
        ))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let bounds = ContainerLayout::new(
            frame,
            Insets::ZERO,
            Insets::new(
                Spacing::Regular.value(),
                crate::summary_row_layout::title_inset(),
                Spacing::Regular.value(),
                crate::summary_row_layout::title_inset(),
            ),
            Some(clip),
        )
        .content();
        let sizes = self.measurements(text, bounds.size.width);
        let title_width =
            (bounds.size.width - sizes[1].size.width - Spacing::Regular.value()).max(0.);
        let top = frame.min_y() + (frame.size.height - sizes[0].size.height) / 2.;
        let line = crate::summary_row_layout::body_line(text);
        let frames = [
            Rect::new(bounds.min_x(), top, title_width, sizes[0].size.height),
            Rect::new(
                bounds.max_x() - sizes[1].size.width,
                top + (line - sizes[1].size.height) / 2.,
                sizes[1].size.width,
                sizes[1].size.height,
            ),
        ];
        for (node, frame) in self.children.iter_mut().zip(frames) {
            node.arrange(text, frame, frame.intersection(&clip).unwrap_or_default());
        }
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, "")
    }
}
