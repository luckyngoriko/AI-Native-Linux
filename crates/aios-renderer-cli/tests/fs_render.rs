//! Tests for AIOS-FS renderable implementations.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_fs::{
    AiosPath, NamespaceClass, Object, ObjectId, ObjectInit, ObjectKind, ObjectMetadata, Pointer,
    PointerId, PointerKind, PrivacyClass, ScopeBinding, ScopeKind, SubjectRef, TransactionId,
    Version, VersionId, VersionState,
};
use aios_renderer_cli::{OutputFormat, RenderContext, Renderable};
use chrono::{TimeZone, Utc};

const CONTENT_HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn ctx(color: bool) -> RenderContext {
    RenderContext {
        color,
        width: Some(240),
        redact_secrets: true,
        verbose: false,
        locale: "en_US.UTF-8".to_owned(),
    }
}

const fn formats() -> [OutputFormat; 4] {
    [
        OutputFormat::Text,
        OutputFormat::Json,
        OutputFormat::Tree,
        OutputFormat::Table,
    ]
}

fn object_id() -> ObjectId {
    ObjectId::parse("obj_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("valid object id")
}

fn pointer_id() -> PointerId {
    PointerId::parse("ptr_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("valid pointer id")
}

fn version_id() -> VersionId {
    VersionId::parse("ver_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("valid version id")
}

fn transaction_id() -> TransactionId {
    TransactionId::parse("txn_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("valid transaction id")
}

fn object() -> Object {
    Object::new(ObjectInit {
        object_id: object_id(),
        kind: ObjectKind::File,
        created_at: Utc
            .with_ymd_and_hms(2026, 5, 25, 9, 15, 0)
            .single()
            .expect("valid timestamp"),
        created_by: SubjectRef("human:operator".to_owned()),
        current_pointer_id: pointer_id(),
        metadata: ObjectMetadata {
            name: "nginx.conf".to_owned(),
            labels: vec!["infra".to_owned(), "nginx".to_owned()],
            mime: "text/plain".to_owned(),
            extra: serde_json::json!({"service": "nginx"}),
        },
        privacy_class: PrivacyClass::Internal,
        scope_binding: ScopeBinding {
            scope_kind: ScopeKind::Group,
            group_id: Some("ops".to_owned()),
            user_id: None,
        },
    })
}

fn version(state: VersionState) -> Version {
    Version {
        version_id: version_id(),
        object_id: object_id(),
        parent_version_ids: Vec::new(),
        chunk_refs: Vec::new(),
        content_hash: CONTENT_HASH.to_owned(),
        metadata_delta: serde_json::json!({"name": "nginx.conf"}),
        created_by_action_id: None,
        created_by_transaction_id: Some(transaction_id()),
        created_at: Utc
            .with_ymd_and_hms(2026, 5, 25, 9, 20, 0)
            .single()
            .expect("valid timestamp"),
        state,
        quarantined_at: None,
        quarantine_reason: None,
    }
}

fn pointer() -> Pointer {
    Pointer {
        pointer_id: pointer_id(),
        object_id: object_id(),
        kind: PointerKind::Current,
        current_version_id: version_id(),
        last_promoted_at: Utc
            .with_ymd_and_hms(2026, 5, 25, 9, 25, 0)
            .single()
            .expect("valid timestamp"),
        last_promoted_by_transaction_id: transaction_id(),
    }
}

#[test]
fn object_renders_core_identity_and_metadata_in_all_formats() {
    let object = object();

    for format in formats() {
        let rendered = object.render(format, &ctx(false)).expect("render object");
        assert!(rendered.contains("obj_01HXY8K2JPQ7N3M4R5S6T7V8W9"));
        assert!(rendered.contains("File") || rendered.contains("FILE"));
        assert!(rendered.contains("Internal") || rendered.contains("INTERNAL"));
        assert!(rendered.contains("ptr_01HXY8K2JPQ7N3M4R5S6T7V8W9"));
        assert!(rendered.contains("nginx.conf"));
    }
}

#[test]
fn object_active_lifecycle_is_green_when_color_enabled() {
    let rendered = object()
        .render(OutputFormat::Text, &ctx(true))
        .expect("render colored object");

    assert!(rendered.contains("\u{1b}[32mActive\u{1b}[0m"));
}

#[test]
fn version_renders_truncated_content_hash_in_all_formats() {
    let version = version(VersionState::Verified);

    for format in formats() {
        let rendered = version.render(format, &ctx(false)).expect("render version");
        assert!(rendered.contains("ver_01HXY8K2JPQ7N3M4R5S6T7V8W9"));
        assert!(rendered.contains("Verified") || rendered.contains("VERIFIED"));
        assert!(rendered.contains("0123456789ab"));
        assert!(!rendered.contains(CONTENT_HASH), "{rendered}");
    }
}

#[test]
fn version_quarantined_state_is_yellow_when_color_enabled() {
    let rendered = version(VersionState::Quarantined)
        .render(OutputFormat::Text, &ctx(true))
        .expect("render quarantined version");

    assert!(rendered.contains("\u{1b}[33mQuarantined\u{1b}[0m"));
}

#[test]
fn pointer_renders_current_version_in_all_formats() {
    let pointer = pointer();

    for format in formats() {
        let rendered = pointer.render(format, &ctx(false)).expect("render pointer");
        assert!(rendered.contains("ptr_01HXY8K2JPQ7N3M4R5S6T7V8W9"));
        assert!(rendered.contains("Current") || rendered.contains("CURRENT"));
        assert!(rendered.contains("ver_01HXY8K2JPQ7N3M4R5S6T7V8W9"));
    }
}

#[test]
fn aios_path_renders_namespace_classification() {
    let path = AiosPath::new("/aios/groups/ops/users/alice/home");

    for format in formats() {
        let rendered = path.render(format, &ctx(false)).expect("render path");
        assert!(rendered.contains("/aios/groups/ops/users/alice/home"));
        assert!(rendered.contains("UserHome") || rendered.contains("USER_HOME"));
    }
}

#[test]
fn namespace_class_renders_policy_flags_and_evidence_floor() {
    let rendered = NamespaceClass::SystemPolicy
        .render(OutputFormat::Text, &ctx(false))
        .expect("render namespace class");

    assert!(rendered.contains("SystemPolicy"));
    assert!(rendered.contains("recovery_only_mutation: true"));
    assert!(rendered.contains("evidence_grade_floor: E4"));
}
