//! RunSurface test-harness history, mapped into the upstream single-text reading contract.
//!
//! This concatenated text is not the reusable Transcript component. The production
//! Transcript needs separate input bubbles, output/activity content and footnote
//! lanes with stable mixed-content reading anchors; see docs/rust-agent-ui-contract.md.
use tekes_agent_presentation::{TranscriptItemKind, TranscriptSnapshot};
use tekes_ui_controls::{
    ReadingUpdateError, TextInput, TextRangeMapping, composition::TextFieldContent,
};

#[derive(Debug, Default)]
pub(super) struct History {
    items: Vec<(String, String)>,
}
impl History {
    pub(super) fn sync(
        &mut self,
        field: &mut TextFieldContent,
        key: &str,
        snapshot: &TranscriptSnapshot,
    ) -> Result<(), ReadingUpdateError> {
        let visible: Vec<_> = snapshot.items.iter().filter(|i| i.is_visible).collect();
        let items: Vec<_> = visible
            .iter()
            .enumerate()
            .map(|(n, i)| {
                let role = if i.kind == TranscriptItemKind::User {
                    "你"
                } else {
                    "Agent"
                };
                (
                    i.id.clone(),
                    format!(
                        "{role}\n{}{}",
                        i.text,
                        if n + 1 < visible.len() { "\n\n" } else { "" }
                    ),
                )
            })
            .collect();
        let text: String = items.iter().map(|(_, text)| text.as_str()).collect();
        let reading = field.reading.as_mut().expect("transcript reading policy");
        match reading.document_version().cloned() {
            Some(version) if version.key() == key => {
                if items == self.items {
                    return Ok(());
                }
                let mappings = retained(&self.items, &items);
                reading.update_document(
                    &mut field.surface,
                    &version,
                    version
                        .revision()
                        .checked_add(1)
                        .ok_or(ReadingUpdateError::InvalidDocument)?,
                    &text,
                    &mappings,
                )?;
            }
            _ => {
                reading.set_document(&mut field.surface, key, 1, &text)?;
            }
        }
        self.items = items;
        Ok(())
    }
}
fn retained(old: &[(String, String)], new: &[(String, String)]) -> Vec<TextRangeMapping> {
    let mut result: Vec<TextRangeMapping> = vec![];
    let mut old_offset = 0;
    for (id, text) in old {
        let mut new_offset = 0;
        // A duplicate key is ambiguous: never map it to another displayed message.
        if old.iter().filter(|(key, _)| key == id).count() == 1
            && new.iter().filter(|(key, _)| key == id).count() == 1
        {
            for (new_id, next) in new {
                if new_id == id {
                    let old_input = TextInput::new(text, true);
                    let new_input = TextInput::new(next, true);
                    let common = text
                        .chars()
                        .zip(next.chars())
                        .take_while(|(a, b)| a == b)
                        .map(|(c, _)| c.len_utf16())
                        .sum::<usize>();
                    let next_boundaries = new_input.boundaries();
                    let len = old_input
                        .boundaries()
                        .into_iter()
                        .filter(|i| *i <= common && next_boundaries.contains(i))
                        .max()
                        .unwrap_or(0);
                    if len > 0 && result.last().is_none_or(|last| last.new.end <= new_offset) {
                        if let Some(last) = result
                            .last_mut()
                            .filter(|last| last.old.end == old_offset && last.new.end == new_offset)
                        {
                            last.old.end += len;
                            last.new.end += len;
                        } else {
                            result.push(TextRangeMapping {
                                old: old_offset..old_offset + len,
                                new: new_offset..new_offset + len,
                            });
                        }
                    }
                    break;
                }
                new_offset += next.encode_utf16().count();
            }
        }
        old_offset += text.encode_utf16().count();
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_ids_prevent_identical_deleted_text_from_rebinding() {
        let old = vec![("deleted".into(), "same".into())];
        let new = vec![("other".into(), "same".into())];
        assert!(retained(&old, &new).is_empty());
    }
    #[test]
    fn streaming_prefix_never_splits_new_grapheme() {
        let old = vec![("a".into(), "e".into())];
        let new = vec![("a".into(), "e\u{301}".into())];
        assert!(retained(&old, &new).is_empty());
    }
}
