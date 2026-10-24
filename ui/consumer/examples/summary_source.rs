//! Public consumer receipt for an original headless `presentationSource` result.
//! This mounts AgentKit's Summary component from one captured frame; it does not
//! reconnect to a runtime, recalculate Summary facts, or claim rendered pixels.
use std::{env, fs};

use tekes_agent_components_ui::{
    RunSnapshot, SummaryActionRow, SummaryInput, SummarySubagentIndicator, SummaryView,
};
use tekes_ui_controls::composition::ContentHost;

fn fail(message: impl AsRef<str>) -> ! {
    eprintln!("summary_source: {}", message.as_ref());
    std::process::exit(2);
}

fn main() {
    let path = env::args_os().nth(1).unwrap_or_else(|| {
        fail("usage: cargo run -p tekes-agent-ui-consumer-check --example summary_source -- <presentationSource.json>")
    });
    let source: serde_json::Value = serde_json::from_slice(
        &fs::read(path).unwrap_or_else(|error| fail(format!("read input: {error}"))),
    )
    .unwrap_or_else(|error| fail(format!("parse input: {error}")));

    require(
        source["projectionSucceeded"] == true,
        "projection did not succeed",
    );
    require(
        source["evidenceScope"] == "original-publication-and-same-memory-input",
        "unexpected evidence scope",
    );
    require(
        source["serverTransactionalFreshnessProven"] == false,
        "source must not claim server transactional freshness",
    );

    let snapshot: RunSnapshot = serde_json::from_value(
        source
            .get("snapshot")
            .cloned()
            .unwrap_or_else(|| fail("missing snapshot")),
    )
    .unwrap_or_else(|error| fail(format!("decode public RunSnapshot: {error}")));
    let authority = source
        .get("authority")
        .unwrap_or_else(|| fail("missing authority"));
    let run = snapshot
        .run
        .as_ref()
        .unwrap_or_else(|| fail("snapshot has no Run"));
    let selection = snapshot
        .selection
        .as_ref()
        .unwrap_or_else(|| fail("snapshot has no selection"));
    require(snapshot.summary.is_some(), "snapshot has no Summary facts");
    require(
        !snapshot.transcript.is_stale && authority["connection"] == "Ready",
        "stale or unavailable source",
    );
    require(
        snapshot.connection_epoch == authority["connection_epoch"].as_u64().unwrap_or(u64::MAX),
        "snapshot and authority epochs differ",
    );
    require(
        run.scope.endpoint_id == authority["endpoint_id"].as_str().unwrap_or_default(),
        "Run and authority endpoints differ",
    );
    require(
        authority["sessions"].get(&run.scope.session_id).is_some(),
        "selected Run session is absent from authority",
    );
    let selection_json = serde_json::to_value(selection)
        .unwrap_or_else(|error| fail(format!("encode selection: {error}")));
    let run_scope_json = serde_json::to_value(&run.scope)
        .unwrap_or_else(|error| fail(format!("encode Run scope: {error}")));
    require(
        selection_json["destination"]["kind"] == "run"
            && selection_json["destination"]["scope"] == run_scope_json,
        "selection and Run scopes differ",
    );
    require(
        snapshot.summary.as_ref().unwrap().scope == run.scope,
        "Summary facts and Run scopes differ",
    );

    // Keep the typed SummaryFacts from the captured RunSnapshot unchanged
    // at the component boundary; this consumer has no SummaryFacts algorithm.
    let input = SummaryInput {
        revision: snapshot.revision,
        scope: run.scope.clone(),
        facts: snapshot.summary.clone(),
        host_sections: vec![],
    };
    let facts = input.facts.as_ref().unwrap();
    let expected_rows = facts.sections(&input.scope);
    let mut summary = SummaryView::new(0x5355_4d4d, Default::default());
    summary
        .update(&input)
        .unwrap_or_else(|error| fail(format!("publish original SummaryInput: {error}")));
    let root = summary.id();
    let host: ContentHost = summary
        .into_host()
        .unwrap_or_else(|error| fail(format!("mount original SummaryView: {error}")));
    let mounted = host
        .get(root)
        .and_then(|content| content.component::<SummaryView>())
        .unwrap_or_else(|| fail("original SummaryView is not mounted"));
    require(
        mounted.displayed_input() == Some(&input),
        "mounted SummaryInput differs from captured input",
    );
    let displayed_context = mounted
        .displayed_context_input()
        .unwrap_or_else(|| fail("canonical displayed Summary context is missing"));
    require(
        displayed_context.owner.run_scope() == Some(&input.scope)
            && displayed_context.facts == input.facts,
        "displayed Summary context differs from captured facts",
    );

    let mut child_rows = Vec::new();
    for subagent in &facts.subagents {
        let expected = expected_rows
            .iter()
            .find(|section| section.id == "subagents")
            .and_then(|section| section.rows.iter().find(|row| row.id == subagent.id))
            .unwrap_or_else(|| fail(format!("original subagent row {} is missing", subagent.id)));
        let Some(action) = expected.action.as_ref() else {
            // Unknown or unavailable child pointers are intentionally not made actionable.
            continue;
        };
        let row_id = mounted
            .action_id("subagents", &subagent.id)
            .unwrap_or_else(|| {
                fail(format!(
                    "original action row {} is not mounted",
                    subagent.id
                ))
            });
        let row = host
            .get(row_id)
            .and_then(|content| content.component::<SummaryActionRow>())
            .unwrap_or_else(|| {
                fail(format!(
                    "original SummaryActionRow {} is absent",
                    subagent.id
                ))
            });
        require(
            row.button().label == expected.title,
            "mounted child label differs from source title",
        );
        let indicator = match subagent.indicator() {
            Some(SummarySubagentIndicator::Running) => "running",
            Some(SummarySubagentIndicator::Failed) => "failed",
            None => "none",
        };
        child_rows.push(serde_json::json!({
            "id": subagent.id,
            "label": row.button().label,
            "title": expected.title,
            "actionMounted": true,
            "actionScopeMatches": action_scope_matches(action, &input.scope, subagent.session_id.as_deref()),
            "status": subagent.status,
            "indicatorPolicy": indicator,
            "glyphRendered": false
        }));
    }
    require(
        child_rows
            .iter()
            .all(|row| row["actionScopeMatches"] == true),
        "mounted child action scope differs from original facts",
    );
    require(
        !child_rows.is_empty(),
        "no actionable child row was mounted",
    );
    let receipt = serde_json::json!({
        "result": "mounted",
        "snapshotRevision": snapshot.revision,
        "connectionEpoch": snapshot.connection_epoch,
        "scope": run.scope,
        "summaryInputMatchesCapturedFacts": true,
        "summarySections": mounted.section_order(),
        "childRows": child_rows,
        "nativeInteraction": false,
        "physicalIme": false,
        "desktopVisibility": false,
        "rendered": false
    });
    println!("{}", serde_json::to_string(&receipt).unwrap());
}

fn action_scope_matches(
    action: &tekes_agent_components_ui::SummaryAction,
    scope: &tekes_agent_components_ui::RunScope,
    child_id: Option<&str>,
) -> bool {
    matches!(action,
        tekes_agent_components_ui::SummaryAction::OpenSubagent { parent, child_id: action_child }
            if parent == scope && Some(action_child.as_str()) == child_id
    )
}

fn require(condition: bool, message: impl AsRef<str>) {
    if !condition {
        fail(message);
    }
}
