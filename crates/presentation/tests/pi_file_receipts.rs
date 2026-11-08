use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tekes_agent_contract::*;
use tekes_agent_presentation::*;
use tekes_agent_session::*;

fn receipt(custom_type: &str, id: &str, rpc_id: &str, files: Option<Value>) -> Value {
    receipt_for_text(custom_type, id, rpc_id, files, "same input")
}

fn receipt_for_text(
    custom_type: &str,
    id: &str,
    rpc_id: &str,
    files: Option<Value>,
    text: &str,
) -> Value {
    let mut data = json!({
        "rpcId": rpc_id,
        "timestamp": 1,
        "hash": format!("{:x}", Sha256::digest(text.as_bytes())),
    });
    if let Some(files) = files {
        data["files"] = files;
    }
    json!({"type":"custom","id":id,"customType":custom_type,"data":data})
}

fn file(content_index: u64, preceding_image_count: u64, id: &str, name: &str, bytes: u64) -> Value {
    json!({
        "contentIndex":content_index,
        "precedingImageCount":preceding_image_count,
        "file":{"attachmentId":id,"name":name,"bytes":bytes}
    })
}

fn user(id: &str, content: Value) -> Value {
    json!({"type":"message","id":id,"message":{"role":"user","timestamp":1,"content":content}})
}

fn project(entries: Vec<Value>) -> TranscriptSnapshot {
    let session = SessionProjection {
        journal_ready: true,
        native_conversation: Some(NativeConversationSnapshot {
            session_id: "s".into(),
            generation: "g".into(),
            leaf_id: entries
                .last()
                .and_then(|entry| entry["id"].as_str())
                .map(str::to_owned),
            entries,
            has_more_before: false,
            is_streaming: false,
            state: json!({}),
        }),
        ..Default::default()
    };
    TranscriptSnapshot::read(&RunScope::main("e", "s"), &session).unwrap()
}

#[test]
fn both_receipt_namespaces_admit_files_without_rewriting_native_source() {
    for custom_type in ["agentkit.inputReceipt.v1", "tekes.inputReceipt.v1"] {
        let content = json!([
            {"type":"text","text":"same input"},
            {"type":"image","mimeType":"image/png","data":"AA=="},
            {"type":"text","text":"attached file: do not parse"}
        ]);
        let original = user("u", content.clone());
        let entries = vec![
            receipt_for_text(
                custom_type,
                "r",
                "rpc",
                Some(json!([file(
                    2,
                    0,
                    "A0B1C2D3-0000-4000-8000-000000000001",
                    "a.txt",
                    4
                )])),
                "same input\nattached file: do not parse",
            ),
            original.clone(),
        ];
        let snapshot = project(entries);
        let baseline = project(vec![
            receipt_for_text(
                custom_type,
                "r",
                "rpc",
                None,
                "same input\nattached file: do not parse",
            ),
            original,
        ]);
        let row = &snapshot.items[0];
        let baseline_row = &baseline.items[0];
        assert_eq!(row.id, baseline_row.id);
        assert_eq!(row.record_id.as_deref(), Some("u"));
        assert_eq!(row.source_position, baseline_row.source_position);
        assert_eq!(row.content_blocks, baseline_row.content_blocks);
        assert_eq!(row.attachments, baseline_row.attachments);
        assert_eq!(row.reconciliation_id.as_deref(), Some("rpc"));
        assert_eq!(row.attachments, vec![content[1].clone()]);
        assert_eq!(row.content_blocks.as_ref().unwrap()[0].index, 0);
        assert_eq!(row.content_blocks.as_ref().unwrap()[1].index, 1);
        assert_eq!(row.content_blocks.as_ref().unwrap()[2].index, 2);
        assert_eq!(row.metadata["admittedFileReceipt"]["entryId"], "r");
        assert_eq!(
            row.metadata["admittedFileReceipt"]["customType"],
            custom_type
        );
        assert_eq!(row.admitted_file_attachments()[0].file.name, "a.txt");
        assert_eq!(
            row.admitted_file_attachments()[0].file.attachment_id,
            "A0B1C2D3-0000-4000-8000-000000000001"
        );
    }
}

#[test]
fn legacy_and_invalid_file_groups_keep_rpc_reconciliation_but_admit_no_files() {
    let mut valid = file(0, 0, "a0b1c2d3-0000-4000-8000-000000000001", "ok.txt", 1);
    valid.as_object_mut().unwrap().remove("precedingImageCount");
    let bad_groups = [
        json!([file(0, 0, "not-a-uuid", "bad", 1)]),
        json!([file(
            0,
            0,
            "a0b1c2d3-0000-4000-8000-000000000001",
            "bad\nname",
            1
        )]),
        json!([file(
            0,
            0,
            "a0b1c2d3-0000-4000-8000-000000000001",
            "../escape",
            1
        )]),
        json!([file(
            0,
            0,
            "a0b1c2d3-0000-4000-8000-000000000001",
            "zero",
            0
        )]),
        json!([file(
            0,
            1,
            "a0b1c2d3-0000-4000-8000-000000000001",
            "too-late",
            1
        )]),
        json!([file(
            0,
            0,
            "a0b1c2d3-0000-4000-8000-000000000001",
            "too-large",
            8 * 1024 * 1024 + 1
        )]),
        json!([
            file(
                0,
                0,
                "a0b1c2d3-0000-4000-8000-000000000001",
                "a",
                8 * 1024 * 1024
            ),
            file(
                1,
                0,
                "a0b1c2d3-0000-4000-8000-000000000002",
                "b",
                8 * 1024 * 1024
            ),
            file(2, 0, "a0b1c2d3-0000-4000-8000-000000000003", "c", 1)
        ]),
        json!([
            file(0, 0, "a0b1c2d3-0000-4000-8000-000000000001", "1", 1),
            file(1, 0, "a0b1c2d3-0000-4000-8000-000000000002", "2", 1),
            file(2, 0, "a0b1c2d3-0000-4000-8000-000000000003", "3", 1),
            file(3, 0, "a0b1c2d3-0000-4000-8000-000000000004", "4", 1),
            file(4, 0, "a0b1c2d3-0000-4000-8000-000000000005", "5", 1),
            file(5, 0, "a0b1c2d3-0000-4000-8000-000000000006", "6", 1),
            file(6, 0, "a0b1c2d3-0000-4000-8000-000000000007", "7", 1),
            file(7, 0, "a0b1c2d3-0000-4000-8000-000000000008", "8", 1),
            file(8, 0, "a0b1c2d3-0000-4000-8000-000000000009", "9", 1)
        ]),
        json!([
            file(0, 0, "a0b1c2d3-0000-4000-8000-000000000001", "first", 1),
            file(0, 0, "a0b1c2d3-0000-4000-8000-000000000002", "duplicate", 1)
        ]),
    ];
    let bad_group_count = bad_groups.len();
    let mut entries = vec![receipt(
        "agentkit.inputReceipt.v1",
        "legacy",
        "rpc-legacy",
        None,
    )];
    entries.push(user("legacy-user", json!("same input")));
    for (index, bad) in bad_groups.into_iter().enumerate() {
        entries.push(receipt(
            "tekes.inputReceipt.v1",
            &format!("bad-{index}"),
            &format!("rpc-{index}"),
            Some(bad),
        ));
        entries.push(user(&format!("bad-user-{index}"), json!("same input")));
    }
    entries.push(receipt(
        "agentkit.inputReceipt.v1",
        "valid",
        "rpc-valid",
        Some(json!([valid])),
    ));
    entries.push(user("valid-user", json!("same input")));
    let snapshot = project(entries);
    assert_eq!(snapshot.items.len(), bad_group_count + 2);
    assert_eq!(
        snapshot.items[0].reconciliation_id.as_deref(),
        Some("rpc-legacy")
    );
    assert!(snapshot.items[0].admitted_file_attachments().is_empty());
    for index in 0..bad_group_count {
        let row = &snapshot.items[index + 1];
        let expected_rpc = format!("rpc-{index}");
        assert_eq!(
            row.reconciliation_id.as_deref(),
            Some(expected_rpc.as_str())
        );
        assert!(row.admitted_file_attachments().is_empty());
    }
    let valid_row = snapshot.items.last().unwrap();
    assert_eq!(valid_row.reconciliation_id.as_deref(), Some("rpc-valid"));
    assert_eq!(valid_row.admitted_file_attachments().len(), 1);
}

#[test]
fn same_timestamp_and_hash_receipts_are_fifo_and_keep_their_own_files() {
    let entries = vec![
        receipt(
            "agentkit.inputReceipt.v1",
            "first",
            "rpc-first",
            Some(json!([file(
                0,
                0,
                "a0b1c2d3-0000-4000-8000-000000000001",
                "first.txt",
                1
            )])),
        ),
        receipt(
            "tekes.inputReceipt.v1",
            "second",
            "rpc-second",
            Some(json!([file(
                0,
                0,
                "a0b1c2d3-0000-4000-8000-000000000002",
                "second.txt",
                2
            )])),
        ),
        user("one", json!("same input")),
        user("two", json!("same input")),
    ];
    let snapshot = project(entries);
    assert_eq!(
        snapshot.items[0].reconciliation_id.as_deref(),
        Some("rpc-first")
    );
    assert_eq!(
        snapshot.items[0].admitted_file_attachments()[0].file.name,
        "first.txt"
    );
    assert_eq!(
        snapshot.items[1].reconciliation_id.as_deref(),
        Some("rpc-second")
    );
    assert_eq!(
        snapshot.items[1].admitted_file_attachments()[0].file.name,
        "second.txt"
    );
}

#[test]
fn display_order_interleaves_files_around_images_without_changing_native_indices() {
    let content = json!([
        {"type":"text","text":"same input"},
        {"type":"image","mimeType":"image/png","data":"AA=="},
        {"type":"file","name":"native-before-second"},
        {"type":"image","mimeType":"image/png","data":"AQ=="},
        {"type":"text","text":"attached file: pretend.txt"}
    ]);
    let files = json!([
        file(
            1,
            0,
            "a0b1c2d3-0000-4000-8000-000000000001",
            "before-first",
            1
        ),
        file(
            3,
            1,
            "a0b1c2d3-0000-4000-8000-000000000002",
            "before-second-a",
            2
        ),
        file(
            4,
            1,
            "a0b1c2d3-0000-4000-8000-000000000003",
            "before-second-b",
            3
        ),
        file(
            5,
            2,
            "a0b1c2d3-0000-4000-8000-000000000004",
            "after-second",
            4
        )
    ]);
    let snapshot = project(vec![
        receipt_for_text(
            "agentkit.inputReceipt.v1",
            "r",
            "rpc",
            Some(files),
            "same input\nattached file: pretend.txt",
        ),
        user("u", content.clone()),
    ]);
    let row = &snapshot.items[0];
    let order = row.attachment_display_order();
    assert_eq!(
        order,
        vec![
            TranscriptAttachmentDisplayItem::AdmittedFile { file_index: 0 },
            TranscriptAttachmentDisplayItem::MessageAttachment {
                attachment_index: 0
            },
            TranscriptAttachmentDisplayItem::MessageAttachment {
                attachment_index: 1
            },
            TranscriptAttachmentDisplayItem::AdmittedFile { file_index: 1 },
            TranscriptAttachmentDisplayItem::AdmittedFile { file_index: 2 },
            TranscriptAttachmentDisplayItem::MessageAttachment {
                attachment_index: 2
            },
            TranscriptAttachmentDisplayItem::AdmittedFile { file_index: 3 },
        ]
    );
    assert_eq!(
        row.content_blocks
            .as_ref()
            .unwrap()
            .iter()
            .map(|block| block.index)
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4]
    );
    assert_eq!(
        row.attachments,
        vec![content[1].clone(), content[2].clone(), content[3].clone()]
    );
}

#[test]
fn receiptless_attached_file_prose_does_not_create_admitted_file_facts() {
    let snapshot = project(vec![user(
        "u",
        json!([{"type":"text","text":"same input\n\nattached file: fake.txt"}]),
    )]);
    let row = &snapshot.items[0];
    assert!(row.admitted_file_attachments().is_empty());
    assert!(row.attachment_display_order().is_empty());
}

#[test]
fn accessor_fails_closed_for_tampered_metadata() {
    let mut snapshot = project(vec![
        receipt(
            "agentkit.inputReceipt.v1",
            "r",
            "rpc",
            Some(json!([file(
                0,
                0,
                "a0b1c2d3-0000-4000-8000-000000000001",
                "ok.txt",
                1
            )])),
        ),
        user("u", json!("same input")),
    ]);
    assert_eq!(snapshot.items[0].admitted_file_attachments().len(), 1);
    snapshot.items[0].metadata["admittedFileAttachments"][0]["file"]["bytes"] = json!(0);
    assert!(snapshot.items[0].admitted_file_attachments().is_empty());
}
