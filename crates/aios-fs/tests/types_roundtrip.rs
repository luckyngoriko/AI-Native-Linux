//! T-036 round-trip + invariant tests for the `aios-fs` skeleton.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use chrono::{TimeZone, Utc};
use strum::{EnumCount, IntoEnumIterator};

use aios_action::ActionId;
use aios_fs::{
    AiosPath, ChunkId, ChunkRef, ConsistencyClass, FsError, LifecycleState, NamespaceClass, Object,
    ObjectId, ObjectInit, ObjectKind, ObjectMetadata, Pointer, PointerId, PointerKind,
    PrivacyClass, ScopeBinding, ScopeKind, SubjectRef, Transaction, TransactionId,
    TransactionState, Version, VersionId, VersionState,
};

#[test]
fn version_state_has_spec_variants_including_quarantined() {
    let count = VersionState::iter().count();
    assert!(
        count >= 4,
        "S1.3 §6 has at least the four active version states"
    );
    assert!(VersionState::iter().any(|s| s == VersionState::Quarantined));
}

#[test]
fn pointer_kind_has_exactly_five_active_variants() {
    assert_eq!(PointerKind::COUNT, 5);
    assert_eq!(PointerKind::iter().count(), 5);
}

#[test]
fn object_round_trips_through_serde_json() {
    let object = sample_object();

    let json = serde_json::to_string(&object).expect("serialise Object");
    let back: Object = serde_json::from_str(&json).expect("deserialise Object");

    assert_eq!(object, back);
    assert!(json.contains("\"privacy_class\":\"SENSITIVE\""));
}

#[test]
fn version_round_trips_through_serde_json() {
    let version = sample_version();

    let json = serde_json::to_string(&version).expect("serialise Version");
    let back: Version = serde_json::from_str(&json).expect("deserialise Version");

    assert_eq!(version, back);
    assert!(json.contains("\"state\":\"QUARANTINED\""));
}

#[test]
fn pointer_round_trips_through_serde_json() {
    let pointer = sample_pointer();

    let json = serde_json::to_string(&pointer).expect("serialise Pointer");
    let back: Pointer = serde_json::from_str(&json).expect("deserialise Pointer");

    assert_eq!(pointer, back);
    assert!(json.contains("\"kind\":\"CURRENT\""));
}

#[test]
fn transaction_round_trips_through_serde_json() {
    let transaction = sample_transaction();

    let json = serde_json::to_string(&transaction).expect("serialise Transaction");
    let back: Transaction = serde_json::from_str(&json).expect("deserialise Transaction");

    assert_eq!(transaction, back);
    assert!(json.contains("\"state\":\"COMMITTED\""));

    let consistency = serde_json::to_string(&ConsistencyClass::Snapshot).expect("serialise");
    assert_eq!(consistency, "\"SNAPSHOT\"");
}

#[test]
fn aios_path_classifies_system_namespace() {
    let path = AiosPath::new("/aios/system/foo");

    assert_eq!(path.namespace_class(), Some(NamespaceClass::System));
}

#[test]
fn aios_path_rejects_non_aios_path() {
    let path = AiosPath::new("/etc/passwd");

    assert_eq!(path.namespace_class(), None);
}

#[test]
fn lifecycle_state_retired_is_terminal() {
    assert!(LifecycleState::Retired.is_terminal());
}

#[test]
fn fs_error_display_strings_are_non_empty() {
    let errors = [
        FsError::ObjectNotFound(ObjectId::parse("obj_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("id")),
        FsError::VersionNotFound(VersionId::parse("ver_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("id")),
        FsError::PointerNotFound(PointerId::parse("ptr_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("id")),
        FsError::InvalidPath("/etc/passwd".to_owned()),
        FsError::QuarantineViolation("recovery subject required".to_owned()),
        FsError::InvalidTransition {
            from: VersionState::Verified,
            to: VersionState::Staged,
        },
        FsError::Internal("metadata index unavailable".to_owned()),
    ];

    for err in errors {
        assert!(
            !err.to_string().is_empty(),
            "Display must be non-empty for {err:?}"
        );
    }
}

fn sample_time() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 24, 12, 0, 0)
        .single()
        .expect("fixture timestamp is valid")
}

fn sample_object() -> Object {
    Object::new(ObjectInit {
        object_id: ObjectId::parse("obj_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("object id"),
        kind: ObjectKind::Project,
        created_at: sample_time(),
        created_by: SubjectRef("family:alice".to_owned()),
        current_pointer_id: PointerId::parse("ptr_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("pointer id"),
        metadata: ObjectMetadata {
            name: "household docs".to_owned(),
            labels: vec!["docs".to_owned(), "family".to_owned()],
            mime: "inode/directory".to_owned(),
            extra: serde_json::json!({ "source": "fixture" }),
        },
        privacy_class: PrivacyClass::Sensitive,
        scope_binding: ScopeBinding {
            scope_kind: ScopeKind::Group,
            group_id: Some("family".to_owned()),
            user_id: None,
        },
    })
    .with_policy_tags(vec!["group:family".to_owned()])
    .with_index_hints(vec!["fulltext".to_owned()])
}

fn sample_version() -> Version {
    Version {
        version_id: VersionId::parse("ver_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("version id"),
        object_id: ObjectId::parse("obj_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("object id"),
        parent_version_ids: vec![
            VersionId::parse("ver_01HXY8K2JPQ7N3M4R5S6T7V8W0").expect("parent id")
        ],
        chunk_refs: vec![ChunkRef(ChunkId::from_hash_bytes(b"fixture chunk"))],
        content_hash: blake3::hash(b"fixture chunk").to_hex().to_string(),
        metadata_delta: serde_json::json!({ "name": "household docs" }),
        created_by_action_id: Some(
            ActionId::parse("act_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("action id"),
        ),
        created_by_transaction_id: Some(
            TransactionId::parse("txn_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("txn id"),
        ),
        created_at: sample_time(),
        state: VersionState::Quarantined,
        quarantined_at: Some(sample_time()),
        quarantine_reason: Some("chunk_integrity_failure".to_owned()),
    }
}

fn sample_pointer() -> Pointer {
    Pointer {
        pointer_id: PointerId::parse("ptr_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("pointer id"),
        object_id: ObjectId::parse("obj_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("object id"),
        kind: PointerKind::Current,
        current_version_id: VersionId::parse("ver_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("version id"),
        last_promoted_at: sample_time(),
        last_promoted_by_transaction_id: TransactionId::parse("txn_01HXY8K2JPQ7N3M4R5S6T7V8W9")
            .expect("txn id"),
    }
}

fn sample_transaction() -> Transaction {
    Transaction {
        transaction_id: TransactionId::parse("txn_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("txn id"),
        subject: SubjectRef("family:alice".to_owned()),
        action_id: Some(ActionId::parse("act_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("action id")),
        started_at: sample_time(),
        completed_at: Some(sample_time()),
        state: TransactionState::Committed,
        writes: Vec::new(),
        pointer_moves: Vec::new(),
        evidence_receipt_id: Some("evr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned()),
    }
}
