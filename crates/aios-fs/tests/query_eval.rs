//! Integration tests for the AIOS-FS query evaluator and view materializer.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use chrono::{TimeZone, Utc};

use aios_fs::{
    evaluate_query, materialize_view, parse_query, AiosFs, ChunkId, ChunkRef, ConsistencyClass,
    FsContext, InMemoryAiosFs, NamespaceClass, Object, ObjectId, ObjectInit, ObjectKind,
    ObjectMetadata, ObjectWriteRequest, Pointer, PointerId, PointerKind, PrivacyClass,
    QueryEvalContext, QueryEvalError, ScopeBinding, ScopeKind, SubjectRef, TransactionId, Version,
    VersionId, VersionState,
};

fn subject(id: &str) -> SubjectRef {
    SubjectRef(id.to_owned())
}

fn fs_context(id: &str) -> FsContext {
    FsContext {
        subject: subject(id),
        action_id: None,
        expected_snapshot_id: None,
        consistency_class: ConsistencyClass::Snapshot,
    }
}

fn chunk_ref(bytes: &[u8]) -> ChunkRef {
    ChunkRef(ChunkId::from_hash_bytes(bytes))
}

fn write_request(name: &str) -> ObjectWriteRequest {
    ObjectWriteRequest {
        object_id: None,
        parent_version_ids: Vec::new(),
        chunks: vec![chunk_ref(name.as_bytes())],
        metadata_delta: serde_json::json!({
            "name": name,
            "labels": ["query", name],
            "mime": "text/plain"
        }),
        action_id: None,
        subject: subject("family:alice"),
    }
}

fn fixture_records() -> (Object, Version, Pointer) {
    let object_id = ObjectId::new();
    let pointer_id = PointerId::new();
    let version_id = VersionId::new();
    let created_at = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
    let transaction_id = TransactionId::new();

    let object = Object::new(ObjectInit {
        object_id: object_id.clone(),
        kind: ObjectKind::Project,
        created_at,
        created_by: subject("family:alice"),
        current_pointer_id: pointer_id.clone(),
        metadata: ObjectMetadata {
            name: "renderer-core".to_owned(),
            labels: vec!["sdf".to_owned(), "renderer".to_owned()],
            mime: "text/plain".to_owned(),
            extra: serde_json::json!({}),
        },
        privacy_class: PrivacyClass::Internal,
        scope_binding: ScopeBinding {
            scope_kind: ScopeKind::System,
            group_id: None,
            user_id: None,
        },
    })
    .with_policy_tags(vec!["sdf".to_owned(), "stable".to_owned()]);

    let version = Version {
        version_id: version_id.clone(),
        object_id: object_id.clone(),
        parent_version_ids: Vec::new(),
        chunk_refs: Vec::new(),
        content_hash: "hash".to_owned(),
        metadata_delta: serde_json::json!({}),
        created_by_action_id: None,
        created_by_transaction_id: Some(transaction_id.clone()),
        created_at,
        state: VersionState::Verified,
        quarantined_at: None,
        quarantine_reason: None,
    };

    let pointer = Pointer {
        pointer_id,
        object_id,
        kind: PointerKind::Current,
        current_version_id: version_id,
        last_promoted_at: created_at,
        last_promoted_by_transaction_id: transaction_id,
    };

    (object, version, pointer)
}

const fn fixture_context<'a>(
    object: &'a Object,
    version: &'a Version,
    pointer: &'a Pointer,
) -> QueryEvalContext<'a> {
    QueryEvalContext {
        object,
        version,
        pointer,
        namespace_class: Some(NamespaceClass::System),
    }
}

#[test]
fn evaluate_eq_and_neq_on_object_kind() {
    let (object, version, pointer) = fixture_records();
    let ctx = fixture_context(&object, &version, &pointer);

    assert!(evaluate_query(&parse_query("object.kind = PROJECT").unwrap(), &ctx).unwrap());
    assert!(evaluate_query(&parse_query("object.kind != FILE").unwrap(), &ctx).unwrap());
}

#[test]
fn evaluate_ordering_on_version_created_at() {
    let (object, version, pointer) = fixture_records();
    let ctx = fixture_context(&object, &version, &pointer);

    for source in [
        "version.created_at > \"2026-01-01T00:00:00Z\"",
        "version.created_at >= \"2026-01-02T03:04:05Z\"",
        "version.created_at < \"2026-01-03T00:00:00Z\"",
        "version.created_at <= \"2026-01-02T03:04:05Z\"",
    ] {
        assert!(evaluate_query(&parse_query(source).unwrap(), &ctx).unwrap());
    }
}

#[test]
fn evaluate_in_contains_and_matches() {
    let (object, version, pointer) = fixture_records();
    let ctx = fixture_context(&object, &version, &pointer);

    assert!(evaluate_query(
        &parse_query("object.kind in [FILE, PROJECT]").unwrap(),
        &ctx
    )
    .unwrap());
    assert!(evaluate_query(
        &parse_query("object.policy_tags contains \"sdf\"").unwrap(),
        &ctx
    )
    .unwrap());
    assert!(evaluate_query(
        &parse_query("object.metadata.name matches \"renderer*\"").unwrap(),
        &ctx
    )
    .unwrap());
}

#[test]
fn evaluate_and_short_circuits_before_type_error() {
    let (object, version, pointer) = fixture_records();
    let ctx = fixture_context(&object, &version, &pointer);
    let query = parse_query("object.kind = FILE and object.metadata.name > 7").unwrap();

    assert!(!evaluate_query(&query, &ctx).unwrap());
}

#[test]
fn mismatched_value_type_returns_query_eval_error_not_panic() {
    let (object, version, pointer) = fixture_records();
    let ctx = fixture_context(&object, &version, &pointer);
    let query = parse_query("object.kind = true").unwrap();

    let result = evaluate_query(&query, &ctx);

    assert!(matches!(result, Err(QueryEvalError::TypeMismatch { .. })));
}

#[tokio::test]
async fn materialize_view_filters_inmemory_objects() {
    let fs = InMemoryAiosFs::new();
    let keep_a = fs
        .write_object(write_request("keep-renderer"), &fs_context("family:alice"))
        .await
        .expect("write keep a");
    let _drop = fs
        .write_object(write_request("drop-dataset"), &fs_context("family:alice"))
        .await
        .expect("write drop");
    let keep_b = fs
        .write_object(write_request("keep-workspace"), &fs_context("family:alice"))
        .await
        .expect("write keep b");
    let query = parse_query("object.metadata.name contains \"keep\"").unwrap();

    let view = materialize_view(&query, &fs, None).await.expect("view");

    let matched: Vec<_> = view
        .matched
        .iter()
        .map(|object_ref| object_ref.object_id.clone())
        .collect();
    let mut expected = vec![keep_a.object_id, keep_b.object_id];
    expected.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    assert_eq!(matched, expected);
}

#[tokio::test]
async fn materialize_view_stable_hash_and_matches_for_same_state() {
    let fs = InMemoryAiosFs::new();
    let _first = fs
        .write_object(write_request("stable-one"), &fs_context("family:alice"))
        .await
        .expect("write first");
    let _second = fs
        .write_object(write_request("stable-two"), &fs_context("family:alice"))
        .await
        .expect("write second");
    let snapshot_id = fs.snapshot().snapshot_id;
    let query = parse_query("object.metadata.name contains \"stable\"").unwrap();

    let left = materialize_view(&query, &fs, Some(&snapshot_id))
        .await
        .expect("left view");
    let right = materialize_view(&query, &fs, Some(&snapshot_id))
        .await
        .expect("right view");

    assert_eq!(left.query_hash, right.query_hash);
    assert_eq!(left.matched, right.matched);
    assert_eq!(left.snapshot_id, snapshot_id);
}

#[tokio::test]
async fn materialize_view_is_read_only_no_snapshot_mutation_observable() {
    let fs = InMemoryAiosFs::new();
    let written = fs
        .write_object(
            write_request("readonly-target"),
            &fs_context("family:alice"),
        )
        .await
        .expect("write object");
    let before = fs
        .get_snapshot(&written.snapshot_id_after)
        .await
        .expect("snapshot before");
    let query = parse_query("object.metadata.name contains \"readonly\"").unwrap();

    let _view = materialize_view(&query, &fs, Some(&written.snapshot_id_after))
        .await
        .expect("view");
    let after = fs
        .get_snapshot(&written.snapshot_id_after)
        .await
        .expect("snapshot after");

    assert_eq!(before.object_count, after.object_count);
    assert_eq!(before.pointer_count, after.pointer_count);
    assert_eq!(before.snapshot_id, after.snapshot_id);
}

#[test]
fn parse_roundtrip_determinism_same_source_100x() {
    let source = "object.kind = PROJECT and object.policy_tags contains \"sdf\"";
    let first = parse_query(source).unwrap();

    for _ in 0..100 {
        assert_eq!(parse_query(source).unwrap(), first);
    }
}
