//! Receipt-backed Native file cards are consumed through the public transcript
//! and InputMessage APIs by an independent UI host.
use serde_json::{Value, json};
use tekes_agent_components_ui::*;
use tekes_ui_controls::composition::*;

const IMAGE_A: &str = "image-a.png";
const IMAGE_B: &str = "image-b.png";
const REPEATED_FILE: &str = "same-name.txt";

fn image(name: &str, data: &str) -> Value {
    json!({"type":"image", "mimeType":"image/png", "name":name, "data":data})
}

fn file(attachment_id: &str, content_index: usize, name: &str, bytes: u64) -> Value {
    json!({
        "contentIndex": content_index,
        "precedingImageCount": 1,
        "file": {
            "attachmentId": attachment_id,
            "name": name,
            "bytes": bytes
        }
    })
}

fn native_input() -> TranscriptItem {
    let image_a = image(IMAGE_A, "image-a-source-bytes");
    let image_b = image(IMAGE_B, "image-b-source-bytes");
    TranscriptItem {
        id: "native-user-entry-17".into(),
        record_id: Some("native-entry-17".into()),
        reconciliation_id: Some("input-rpc-17".into()),
        turn_id: "native-turn-4".into(),
        kind: TranscriptItemKind::User,
        text: "前文\n中间原文\n后文".into(),
        reasoning: String::new(),
        attachments: vec![image_a.clone(), image_b.clone()],
        // Keep the original Native block positions and payloads intact. Receipt
        // file indices describe attachment-relative placement only.
        content_blocks: Some(vec![
            TranscriptContentBlock {
                index: 0,
                content: TranscriptBlockContent::Text {
                    text: "前文".into(),
                },
            },
            TranscriptContentBlock {
                index: 1,
                content: TranscriptBlockContent::Attachment { content: image_a },
            },
            TranscriptContentBlock {
                index: 2,
                content: TranscriptBlockContent::Text {
                    text: "\n中间原文\n".into(),
                },
            },
            TranscriptContentBlock {
                index: 3,
                content: TranscriptBlockContent::Attachment { content: image_b },
            },
            TranscriptContentBlock {
                index: 4,
                content: TranscriptBlockContent::Text {
                    text: "后文".into(),
                },
            },
        ]),
        is_visible: true,
        is_final: false,
        is_provisional: false,
        reasoning_ended: true,
        source_sequence: None,
        source_position: Some(TranscriptSourcePosition::Native {
            entry_id: "native-entry-17".into(),
            entry_ordinal: 17,
            content_index: None,
        }),
        time: Some(1_761_400_000_000.0),
        metadata: Default::default(),
    }
}

fn with_files(mut item: TranscriptItem, files: Value) -> TranscriptItem {
    item.metadata = json!({
        "admittedFileAttachments": files,
        "admittedFileReceipt": {
            "entryId": "native-receipt-17",
            "customType": "agentkit.inputReceipt.v1"
        }
    });
    item
}

fn valid_files() -> Value {
    json!([
        file(
            "11111111-1111-4111-8111-111111111111",
            2,
            REPEATED_FILE,
            120
        ),
        file(
            "22222222-2222-4222-8222-222222222222",
            3,
            REPEATED_FILE,
            240
        )
    ])
}

fn update_message(
    host: &mut ContentHost,
    ids: InputMessageIds,
    scope: &RunScope,
    publication: u64,
    item: &TranscriptItem,
) {
    host.get_mut(ids.root)
        .unwrap()
        .component_mut::<InputMessage>()
        .unwrap()
        .update_with(
            scope,
            publication,
            1,
            &TranscriptInput {
                key: TranscriptMessageKey::NativeEntry("native-entry-17"),
                item,
                footnote: TranscriptDelivery::Recorded { time: item.time },
            },
            |_| "已发送 · 原始时间".into(),
        )
        .unwrap();
}

fn mounted(item: &TranscriptItem, publication: u64) -> (ContentHost, RunScope, InputMessageIds) {
    let scope = RunScope::main("consumer", "admitted-files");
    let input = TranscriptInput {
        key: TranscriptMessageKey::NativeEntry("native-entry-17"),
        item,
        footnote: TranscriptDelivery::Recorded { time: item.time },
    };
    let mut message = InputMessage::new(64_520, scope.clone());
    message
        .update_with(&scope, publication, 1, &input, |_| {
            "已发送 · 原始时间".into()
        })
        .unwrap();
    let ids = message.ids();
    (message.into_host().unwrap(), scope, ids)
}

fn detail_names(host: &ContentHost, ids: &[OutputContentIds]) -> Vec<String> {
    let semantics = host.semantics();
    ids.iter()
        .map(|detail| {
            semantics
                .iter()
                .find(|node| node.id == detail.root)
                .unwrap_or_else(|| panic!("missing public semantic node for {:?}", detail.root))
                .semantics
                .name
                .clone()
        })
        .collect()
}

fn detail_roots_named(
    host: &ContentHost,
    ids: &[OutputContentIds],
    needle: &str,
) -> Vec<ComponentId> {
    let semantics = host.semantics();
    ids.iter()
        .filter_map(|detail| {
            semantics
                .iter()
                .find(|node| node.id == detail.root)
                .filter(|node| node.semantics.name.contains(needle))
                .map(|node| node.id)
        })
        .collect()
}

fn expand_detail(host: &mut ContentHost, root: ComponentId, detail: OutputContentIds) {
    let disclosure = detail.disclosure.expect("attachment card disclosure");
    host.accessibility_action(disclosure, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<InputMessage>()
            .unwrap()
            .handle_actions(&actions)
    );
}

#[test]
fn native_files_interleave_by_image_slot_in_stable_content_order_without_rewriting_source() {
    let source = with_files(native_input(), valid_files());
    let original_attachments = source.attachments.clone();
    let original_blocks = source.content_blocks.clone();
    let original_text = source.text.clone();
    let original_id = source.id.clone();

    let admitted = source.admitted_file_attachments();
    assert_eq!(admitted.len(), 2);
    assert_eq!(admitted[0].content_index, 2);
    assert_eq!(admitted[1].content_index, 3);
    assert_eq!(admitted[0].file.name, REPEATED_FILE);
    assert_eq!(admitted[1].file.name, REPEATED_FILE);
    assert_ne!(
        admitted[0].file.attachment_id,
        admitted[1].file.attachment_id
    );
    assert_eq!(
        source.attachment_display_order(),
        vec![
            TranscriptAttachmentDisplayItem::MessageAttachment {
                attachment_index: 0
            },
            TranscriptAttachmentDisplayItem::AdmittedFile { file_index: 0 },
            TranscriptAttachmentDisplayItem::AdmittedFile { file_index: 1 },
            TranscriptAttachmentDisplayItem::MessageAttachment {
                attachment_index: 1
            },
        ]
    );

    let (mut host, _, ids) = mounted(&source, 1);
    let details = host
        .get(ids.root)
        .unwrap()
        .component::<InputMessage>()
        .unwrap()
        .detail_ids();
    assert_eq!(details.len(), 4);
    assert!(
        details
            .iter()
            .all(|detail| detail.kind == OutputContentKind::Attachment)
    );
    let names = detail_names(&host, &details);
    assert!(names[0].contains(IMAGE_A), "{names:?}");
    assert!(names[1].contains(REPEATED_FILE), "{names:?}");
    assert!(names[2].contains(REPEATED_FILE), "{names:?}");
    assert!(names[3].contains(IMAGE_B), "{names:?}");
    expand_detail(&mut host, ids.root, details[1]);
    expand_detail(&mut host, ids.root, details[2]);
    let first_file_text = &host.text_surface(details[1].body).unwrap().input.text;
    let second_file_text = &host.text_surface(details[2].body).unwrap().input.text;
    assert!(
        first_file_text.contains("11111111-1111-4111-8111-111111111111"),
        "{first_file_text}"
    );
    assert!(
        second_file_text.contains("22222222-2222-4222-8222-222222222222"),
        "{second_file_text}"
    );
    assert_eq!(
        details
            .iter()
            .map(|detail| detail.root)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        details.len(),
        "repeated filenames remain distinct card identities"
    );
    assert_eq!(
        host.text_surface(ids.body).unwrap().input.text,
        original_text
    );
    let displayed = host
        .get(ids.root)
        .unwrap()
        .component::<InputMessage>()
        .unwrap();
    assert_eq!(displayed.information_text(), "已发送 · 原始时间");
    assert_eq!(displayed.ids().root, ids.root);
    assert_eq!(source.id, original_id);
    assert_eq!(source.attachments, original_attachments);
    assert_eq!(source.content_blocks, original_blocks);
}

#[test]
fn receipt_publication_keeps_message_and_image_card_ids_and_allocates_a_file_card_id() {
    let mut source = native_input();
    let (mut host, scope, ids) = mounted(&source, 1);
    let old_details = host
        .get(ids.root)
        .unwrap()
        .component::<InputMessage>()
        .unwrap()
        .detail_ids();
    assert_eq!(old_details.len(), 2);
    let before_names = detail_names(&host, &old_details);
    let image_a_id = old_details[0].root;
    let image_b_id = old_details[1].root;
    let message_id = source.id.clone();

    source = with_files(
        source,
        json!([file(
            "33333333-3333-4333-8333-333333333333",
            2,
            "new-file.txt",
            64
        )]),
    );
    update_message(&mut host, ids, &scope, 2, &source);

    let view = host
        .get(ids.root)
        .unwrap()
        .component::<InputMessage>()
        .unwrap();
    assert_eq!(view.ids().footnote, ids.footnote);
    assert_eq!(view.ids().bubble, ids.bubble);
    assert_eq!(view.ids().body, ids.body);
    assert_eq!(source.id, message_id);
    let new_details = view.detail_ids();
    assert_eq!(new_details.len(), 3);
    assert_eq!(new_details[0].root, image_a_id);
    assert_eq!(new_details[2].root, image_b_id);
    let new_file_ids = detail_roots_named(&host, &new_details, "new-file.txt");
    assert_eq!(new_file_ids.len(), 1);
    assert_eq!(new_details[1].root, new_file_ids[0]);
    assert!(![image_a_id, image_b_id].contains(&new_file_ids[0]));
    assert!(before_names[0].contains(IMAGE_A));
    assert!(before_names[1].contains(IMAGE_B));
    assert_eq!(host.text_surface(ids.body).unwrap().input.text, source.text);
    assert_eq!(
        host.get(ids.root)
            .unwrap()
            .component::<InputMessage>()
            .unwrap()
            .information_text(),
        "已发送 · 原始时间"
    );
}

#[test]
fn invalid_receipt_files_are_suppressed_while_original_text_and_images_remain() {
    let source = with_files(
        native_input(),
        json!([file(
            "44444444-4444-4444-8444-444444444444",
            2,
            "../not-admitted.txt",
            100
        )]),
    );
    assert!(source.admitted_file_attachments().is_empty());
    assert!(
        source
            .attachment_display_order()
            .iter()
            .all(|item| matches!(
                item,
                TranscriptAttachmentDisplayItem::MessageAttachment { .. }
            ))
    );
    let original_attachments = source.attachments.clone();
    let original_blocks = source.content_blocks.clone();
    let (host, _, ids) = mounted(&source, 1);
    let details = host
        .get(ids.root)
        .unwrap()
        .component::<InputMessage>()
        .unwrap()
        .detail_ids();
    assert_eq!(details.len(), 2);
    let names = detail_names(&host, &details);
    assert!(names[0].contains(IMAGE_A), "{names:?}");
    assert!(names[1].contains(IMAGE_B), "{names:?}");
    assert_eq!(host.text_surface(ids.body).unwrap().input.text, source.text);
    assert_eq!(source.attachments, original_attachments);
    assert_eq!(source.content_blocks, original_blocks);
}

#[test]
fn admitted_receipt_metadata_is_scoped_to_original_native_user_source() {
    let source = with_files(native_input(), valid_files());
    assert_eq!(source.admitted_file_attachments().len(), 2);

    let mut missing_receipt_id = source.clone();
    missing_receipt_id.metadata["admittedFileReceipt"]["entryId"] = json!("");
    assert!(missing_receipt_id.admitted_file_attachments().is_empty());

    let mut unknown_receipt_type = source.clone();
    unknown_receipt_type.metadata["admittedFileReceipt"]["customType"] =
        json!("unrecognized.receipt.v9");
    assert!(unknown_receipt_type.admitted_file_attachments().is_empty());

    let mut assistant = source.clone();
    assistant.kind = TranscriptItemKind::Assistant;
    assert!(assistant.admitted_file_attachments().is_empty());

    let mut mismatched_record = source.clone();
    mismatched_record.record_id = Some("different-native-entry".into());
    assert!(mismatched_record.admitted_file_attachments().is_empty());

    let mut journal = source.clone();
    journal.source_position = Some(TranscriptSourcePosition::Journal {
        sequence: 17,
        content_index: None,
    });
    assert!(journal.admitted_file_attachments().is_empty());

    let mut empty_rpc = source.clone();
    empty_rpc.reconciliation_id = Some(String::new());
    assert!(empty_rpc.admitted_file_attachments().is_empty());

    let mut provisional = source.clone();
    provisional.is_provisional = true;
    assert!(provisional.admitted_file_attachments().is_empty());

    let mut hidden = source;
    hidden.is_visible = false;
    // Visibility is a row selection fact, not permission to erase metadata.
    assert_eq!(hidden.admitted_file_attachments().len(), 2);
}
