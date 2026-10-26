//! The demo host delegates label layout to the fixed public SDK and the same
//! font registry/style used for its normal text. No clipping/truncation engine.
use std::rc::Rc;
use tekes_ui_text::{
    LabelLayout, LabelOptions, coretext::CoreTextShaper, model::ShapeOptions, style::TextStyle,
};
use tekes_ui_tokens::proposal;
pub fn text_style(role: proposal::TextRole, scale: f32) -> TextStyle {
    let m = role.style();
    let style = if proposal::text_family(role.id()) == Some("mono") {
        TextStyle::named("Menlo", m.size * scale)
    } else {
        TextStyle::system(m.size * scale)
    };
    style.with_weight(m.weight)
}
pub fn shape_label(
    shaper: &mut CoreTextShaper,
    text: &str,
    role: proposal::TextRole,
    width: f32,
    scale: f32,
    options: LabelOptions,
) -> Rc<LabelLayout> {
    Rc::new(shaper.shape_label(
        text,
        &text_style(role, scale),
        &ShapeOptions {
            width: Some(f64::from(width)),
            line_spacing: 0.,
        },
        options,
    ))
}
