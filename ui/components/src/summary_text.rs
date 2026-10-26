//! Summary owns displayed facts; the original SDK read-only field owns selection/copy.
use tekes_ui_controls::{
    DraftRevision, ExternalSelection, ExternalUndo, ExternalValueResult,
    composition::{ComponentId, TextFieldContent},
};

pub(crate) fn update_text(
    field: &mut TextFieldContent,
    id: ComponentId,
    revision: u64,
    value: String,
) {
    let baseline = field.surface.input.external_baseline();
    let result = field.surface.apply_external_value(
        &baseline,
        &DraftRevision {
            identity: format!("summary-text:{}", id.0),
            version: revision,
            payload: value.clone(),
        },
        ExternalSelection::Preserve,
        ExternalUndo::Preserve,
    );
    // The field is exclusively read-only: no composing/writable update can
    // compete with this Summary publication. Rejecting would be an invariant
    // failure, rather than permission to mutate its underlying input directly.
    assert!(matches!(
        result,
        ExternalValueResult::Applied { .. } | ExternalValueResult::Unchanged
    ));
    field.field.label = value;
}
