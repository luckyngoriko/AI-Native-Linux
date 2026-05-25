//! Tests for evidence renderable implementations.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_evidence::{EvidenceReceipt, ReceiptBuilder, RecordType, RetentionClass};
use aios_renderer_cli::{EvidenceChainView, OutputFormat, RenderContext, Renderable};
use serde_json::{json, Value};

const SECRET_MARKER: &str = "AIOS_TEST_SECRET_MARKER";

fn ctx(color: bool, redact_secrets: bool) -> RenderContext {
    RenderContext {
        color,
        width: Some(200),
        redact_secrets,
        verbose: false,
        locale: "en_US.UTF-8".to_owned(),
    }
}

fn receipt(record_type: RecordType, previous: Option<&EvidenceReceipt>) -> EvidenceReceipt {
    ReceiptBuilder::new(record_type, RetentionClass::Standard24M, "human:operator-1")
        .with_payload(json!({
            "message": "routine evidence",
            "sequence": previous.map_or(0, |_| 1),
        }))
        .seal(previous)
        .expect("seal receipt")
}

fn receipt_with_payload(payload: Value) -> EvidenceReceipt {
    ReceiptBuilder::new(
        RecordType::ActionReceived,
        RetentionClass::Standard24M,
        "human:operator-1",
    )
    .with_payload(payload)
    .seal(None)
    .expect("seal receipt")
}

fn receipt_with_signature(signature_hex: &str) -> EvidenceReceipt {
    let receipt = receipt(RecordType::PolicyDecision, None);
    let mut value = serde_json::to_value(&receipt).expect("serialize receipt");
    value["signature"] = json!(signature_hex);
    serde_json::from_value(value).expect("deserialize signed receipt fixture")
}

fn chain3() -> Vec<EvidenceReceipt> {
    let first = receipt(RecordType::ActionReceived, None);
    let second = receipt(RecordType::ExecutionStarted, Some(&first));
    let third = receipt(RecordType::ExecutionCompleted, Some(&second));
    vec![first, second, third]
}

fn broken_chain2() -> Vec<EvidenceReceipt> {
    let first = receipt(RecordType::ActionReceived, None);
    let second = receipt(RecordType::ExecutionStarted, Some(&first));
    let mut value = serde_json::to_value(&second).expect("serialize second receipt");
    value["previous_receipt_hash"] = json!("0".repeat(32));
    let broken_second = serde_json::from_value(value).expect("deserialize broken receipt");
    vec![first, broken_second]
}

fn render_all(value: &impl Renderable, redact_secrets: bool) -> Vec<String> {
    [
        OutputFormat::Text,
        OutputFormat::Json,
        OutputFormat::Tree,
        OutputFormat::Table,
    ]
    .into_iter()
    .map(|format| {
        value
            .render(format, &ctx(false, redact_secrets))
            .expect("render evidence value")
    })
    .collect()
}

#[test]
fn evidence_receipt_text_includes_receipt_id_record_type_and_timestamp() {
    let receipt = receipt(RecordType::ActionReceived, None);

    let rendered = receipt
        .render(OutputFormat::Text, &ctx(false, true))
        .expect("render receipt text");

    assert!(rendered.contains(receipt.receipt_id().as_str()));
    assert!(rendered.contains("ACTION_RECEIVED"));
    assert!(rendered.contains(&receipt.recorded_at().to_rfc3339()));
}

#[test]
fn evidence_receipt_json_round_trips_through_serde_json_when_debug_unredacted() {
    let receipt = receipt(RecordType::PolicyDecision, None);

    let rendered = receipt
        .render(OutputFormat::Json, &ctx(false, false))
        .expect("render receipt json");
    let reparsed: EvidenceReceipt = serde_json::from_str(&rendered).expect("parse receipt json");

    assert_eq!(reparsed, receipt);
}

#[test]
fn evidence_receipt_tree_shows_receipt_as_tree_node() {
    let receipt = receipt(RecordType::ExecutionStarted, None);

    let rendered = receipt
        .render(OutputFormat::Tree, &ctx(false, true))
        .expect("render receipt tree");

    assert!(rendered.starts_with(&format!("receipt {}", receipt.receipt_id().as_str())));
    assert!(rendered.contains("record_type: EXECUTION_STARTED"));
}

#[test]
fn evidence_receipt_table_produces_one_data_row() {
    let receipt = receipt(RecordType::ExecutionCompleted, None);

    let rendered = receipt
        .render(OutputFormat::Table, &ctx(false, true))
        .expect("render receipt table");

    assert!(rendered.contains("receipt_id"));
    assert!(rendered.contains("type"));
    assert!(rendered.contains("timestamp"));
    assert!(rendered.contains("subject"));
    assert_eq!(rendered.matches(receipt.receipt_id().as_str()).count(), 1);
}

#[test]
fn record_type_representative_variants_render_wire_names() {
    let cases = [
        (RecordType::ActionReceived, "ACTION_RECEIVED"),
        (RecordType::SegmentSealed, "SEGMENT_SEALED"),
        (RecordType::VaultCapabilityIssued, "VAULT_CAPABILITY_ISSUED"),
        (
            RecordType::ModelInvocationStarted,
            "MODEL_INVOCATION_STARTED",
        ),
        (
            RecordType::AgentLifecycleTransitioned,
            "AGENT_LIFECYCLE_TRANSITIONED",
        ),
    ];

    for (record_type, expected) in cases {
        for format in [
            OutputFormat::Text,
            OutputFormat::Json,
            OutputFormat::Tree,
            OutputFormat::Table,
        ] {
            let rendered = record_type
                .render(format, &ctx(false, true))
                .expect("render record type");
            assert!(
                rendered.contains(expected),
                "{format:?} output must contain {expected}: {rendered}"
            );
        }
    }
}

#[test]
fn evidence_chain_view_with_three_receipts_shows_all_three_in_each_format() {
    let receipts = chain3();
    let receipt_ids = receipts
        .iter()
        .map(|receipt| receipt.receipt_id().as_str().to_owned())
        .collect::<Vec<_>>();
    let view = EvidenceChainView::new(receipts);

    for rendered in render_all(&view, true) {
        for receipt_id in &receipt_ids {
            assert!(
                rendered.contains(receipt_id),
                "missing {receipt_id}: {rendered}"
            );
        }
    }
}

#[test]
fn evidence_chain_view_empty_renders_no_receipts_marker() {
    let view = EvidenceChainView::new(Vec::new());

    for rendered in render_all(&view, true) {
        assert!(rendered.contains("(no receipts)"), "{rendered}");
    }
}

#[test]
fn inv015_redacted_payload_marker_does_not_leak_in_any_format() {
    let receipt = receipt_with_payload(json!({
        "operator_note": SECRET_MARKER,
        "bytes": [65, 73, 79, 83],
    }));

    for rendered in render_all(&receipt, true) {
        assert!(!rendered.contains(SECRET_MARKER), "{rendered}");
    }
}

#[test]
fn inv015_redacted_signature_shows_byte_count_not_hex() {
    let signature = "ab".repeat(64);
    let receipt = receipt_with_signature(&signature);

    let rendered = receipt
        .render(OutputFormat::Text, &ctx(false, true))
        .expect("render redacted signed receipt");

    assert!(rendered.contains("signature: <64 bytes>"));
    assert!(!rendered.contains(&signature));
}

#[test]
fn inv015_debug_signature_shows_full_hex() {
    let signature = "cd".repeat(64);
    let receipt = receipt_with_signature(&signature);

    let rendered = receipt
        .render(OutputFormat::Text, &ctx(false, false))
        .expect("render debug signed receipt");

    assert!(rendered.contains(&signature));
}

#[test]
fn inv018_key_material_never_renders_with_byte_values() {
    let receipt = receipt_with_payload(json!({
        "key_material": "00112233445566778899aabbccddeeff",
        "nested": {
            "key_material": [1, 2, 3, 4]
        }
    }));

    for rendered in render_all(&receipt, false) {
        assert!(!rendered.contains("00112233445566778899aabbccddeeff"));
        assert!(!rendered.contains("[1,2,3,4]"));
        if rendered.contains("key_material") {
            assert!(
                rendered.contains("\"key_material\":\"<redacted>\"")
                    || rendered.contains("key_material: <redacted>")
            );
        }
    }
}

#[test]
fn tree_format_renders_prev_receipt_hash_truncated_to_twelve_chars() {
    let receipts = chain3();
    let expected = receipts[1].previous_receipt_hash().expect("prev hash")[..12].to_owned();
    let view = EvidenceChainView::new(receipts);

    let rendered = view
        .render(OutputFormat::Tree, &ctx(false, true))
        .expect("render chain tree");

    assert!(rendered.contains(&expected), "{rendered}");
    assert!(!rendered.contains(&format!("{expected}{}", "0".repeat(20))));
}

#[test]
fn chain_link_rendering_valid_hash_is_green_when_color_enabled() {
    let receipts = chain3();
    let expected = receipts[1].previous_receipt_hash().expect("prev hash")[..12].to_owned();
    let view = EvidenceChainView::new(receipts);

    let rendered = view
        .render(OutputFormat::Tree, &ctx(true, true))
        .expect("render chain tree");

    assert!(
        rendered.contains(&format!("\u{1b}[32m→ {expected}\u{1b}[0m")),
        "{rendered}"
    );
}

#[test]
fn chain_link_rendering_mismatched_hash_is_red_when_color_enabled() {
    let receipts = broken_chain2();
    let expected = "000000000000";
    let view = EvidenceChainView::new(receipts);

    let rendered = view
        .render(OutputFormat::Tree, &ctx(true, true))
        .expect("render broken chain tree");

    assert!(
        rendered.contains(&format!("\u{1b}[31m→ {expected}\u{1b}[0m")),
        "{rendered}"
    );
}
