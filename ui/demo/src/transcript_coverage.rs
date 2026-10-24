//! Source-to-visible-owner conservation for the offscreen acceptance driver.
//! A source message is not necessarily a visible body row.
use std::collections::BTreeSet;
use tekes_agent_components_ui::*;

pub fn verify(
    content: &TranscriptContent,
    document: &TranscriptDocument<'_>,
) -> Result<(usize, usize), &'static str> {
    let visible: BTreeSet<_> = content
        .row_keys()
        .iter()
        .filter_map(|key| match key {
            TranscriptContentRowKey::Message(owner)
            | TranscriptContentRowKey::MessageFragment(owner, _)
            | TranscriptContentRowKey::MessageToolFragment(owner, _) => Some(owner.clone()),
            _ => None,
        })
        .collect();
    let mut expected = BTreeSet::new();
    let mut empty = 0;
    for section in &document.sections {
        for input in &section.inputs {
            let owner: TranscriptMessageIdentity = input.key.into();
            if !expected.insert(owner.clone()) || !visible.contains(&owner) {
                return Err("missing or duplicate canonical input");
            }
        }
    }
    let outputs = document
        .session_notices
        .iter()
        .map(|output| (output, None))
        .chain(document.sections.iter().flat_map(|section| {
            section
                .outputs
                .iter()
                .map(move |output| (output, Some(section)))
        }));
    for (output, section) in outputs {
        let owner: TranscriptMessageIdentity = output.key.into();
        if !expected.insert(owner.clone()) {
            return Err("duplicate canonical output");
        }
        if visible.contains(&owner) {
            continue;
        }
        let item = output.item;
        let known_empty = item.text.is_empty()
            && item.display_reasoning().is_empty()
            && item.attachments.is_empty()
            && item.failure_message().is_none()
            && item.system_prompt().is_none()
            && item.content_blocks.as_ref().is_some_and(|blocks| {
                blocks.iter().all(|block| match &block.content {
                    TranscriptBlockContent::Text { text } => text.is_empty(),
                    TranscriptBlockContent::ToolCall { tool_id } => {
                        section.is_some_and(|section| {
                            section.tools.iter().any(|call| &call.id == tool_id)
                                && content.retained_tool_activity(tool_id).is_some()
                        })
                    }
                    _ => false,
                })
            });
        if !known_empty || !content.message_content_ids(&owner).is_empty() {
            return Err("canonical output has undisplayed content");
        }
        empty += 1;
    }
    if !visible.is_subset(&expected) || visible.len() + empty != expected.len() {
        return Err("visible transcript owners differ from canonical source");
    }
    Ok((visible.len(), empty))
}
