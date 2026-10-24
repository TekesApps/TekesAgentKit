//! Opt-in read-only protocol proof, separate from rating controls and mutations.
use crate::component_demo::ComponentDemo;
use serde_json::{Value, json};
use tekes_agent_contract::{Result, require, unsupported};
use tekes_agent_presentation::TranscriptDocument;
use tekes_agent_ui_bindings::TurnFeedbackTarget;

pub async fn run(view: &ComponentDemo) -> Result<Value> {
    let source = view.binding().clone();
    let snapshot = source
        .current_run()
        .ok_or_else(|| unsupported("feedback run unavailable"))?;
    let document = TranscriptDocument::read(&snapshot.transcript)?;
    let targets = document
        .sections
        .iter()
        .filter_map(TurnFeedbackTarget::read)
        .count();
    require(
        targets > 0,
        "no proven final feedback identity in restored history",
    )?;
    let frame = source.read_feedback().await?;
    require(frame.is_current(&source), "feedback frame became stale")?;
    Ok(
        json!({"readCommitted":true,"originalScope":frame.scope()==source.scope(),
        "publication":frame.publication(),"connectionEpoch":frame.connection_epoch(),
        "records":frame.records().items.len(),"knownEmpty":frame.records().items.is_empty(),
        "finalTargets":targets,"putAdvertised":frame.put_advertised(),
        "deleteAdvertised":frame.delete_advertised(),"mutated":false,
        "ratingControls":false,"nativeInteraction":false}),
    )
}
