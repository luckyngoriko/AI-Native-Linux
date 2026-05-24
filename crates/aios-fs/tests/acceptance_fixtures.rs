//! T-045 S1.3/S2.1/S4.1 acceptance fixture coverage.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::too_many_lines,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use ed25519_dalek::SigningKey;

use aios_action::ActionId;
use aios_evidence::RecordType;
use aios_fs::{
    materialize_view, parse_query, AiosFs, AiosPath, ChunkId, ChunkRef, ConsistencyClass,
    FsContext, FsError, FsEvidenceEmitter, GcPassDriver, InMemoryAiosFs, InMemoryFsEvidenceLog,
    MutableAiosFs, NamespaceClass, NamespacePolicy, ObjectId, ObjectWriteRequest, PointerKind,
    QuarantineDriver, QuarantineTrigger, SubjectRef, VersionId, VersionPurgeReason, VersionState,
};

fn subject(id: &str) -> SubjectRef {
    SubjectRef(id.to_owned())
}

fn context(id: &str) -> FsContext {
    FsContext {
        subject: subject(id),
        action_id: Some(ActionId::new()),
        expected_snapshot_id: None,
        consistency_class: ConsistencyClass::Snapshot,
    }
}

fn context_at(id: &str, snapshot_id: aios_fs::SnapshotId) -> FsContext {
    FsContext {
        expected_snapshot_id: Some(snapshot_id),
        ..context(id)
    }
}

fn chunk_ref(bytes: &[u8]) -> ChunkRef {
    ChunkRef(ChunkId::from_hash_bytes(bytes))
}

fn evidence_fixture() -> (Arc<InMemoryFsEvidenceLog>, Arc<FsEvidenceEmitter>) {
    let log = Arc::new(InMemoryFsEvidenceLog::new());
    let emitter = Arc::new(FsEvidenceEmitter::new(
        log.clone(),
        SigningKey::from_bytes(&[46u8; 32]),
        subject("_system:service:aios-fs"),
    ));
    (log, emitter)
}

fn write_request(
    name: &str,
    kind: &str,
    privacy_class: &str,
    policy_tags: &[&str],
    chunks: Vec<ChunkRef>,
) -> ObjectWriteRequest {
    ObjectWriteRequest {
        object_id: None,
        parent_version_ids: Vec::new(),
        chunks,
        metadata_delta: serde_json::json!({
            "name": name,
            "mime": "text/plain",
            "kind": kind,
            "privacy_class": privacy_class,
            "labels": policy_tags,
            "policy_tags": policy_tags,
            "scope": { "kind": "GROUP", "group_id": "family" }
        }),
        action_id: Some(ActionId::new()),
        subject: subject("family:alice"),
    }
}

fn append_request(
    object_id: ObjectId,
    parent_version_id: VersionId,
    name: &str,
    privacy_class: &str,
    chunks: Vec<ChunkRef>,
) -> ObjectWriteRequest {
    ObjectWriteRequest {
        object_id: Some(object_id),
        parent_version_ids: vec![parent_version_id],
        chunks,
        metadata_delta: serde_json::json!({
            "name": name,
            "mime": "text/plain",
            "privacy_class": privacy_class
        }),
        action_id: Some(ActionId::new()),
        subject: subject("family:alice"),
    }
}

async fn write_fixture(
    fs: &InMemoryAiosFs,
    name: &str,
    kind: &str,
    privacy_class: &str,
    tags: &[&str],
) -> aios_fs::ObjectWriteResult {
    fs.write_object(
        write_request(
            name,
            kind,
            privacy_class,
            tags,
            vec![chunk_ref(name.as_bytes())],
        ),
        &context("family:alice"),
    )
    .await
    .expect("write fixture object")
}

#[tokio::test]
async fn s1_3_fixture_write_promote_commits_pointer_and_evidence() {
    let (log, emitter) = evidence_fixture();
    let fs = InMemoryAiosFs::with_evidence_emitter(emitter);

    let written = write_fixture(&fs, "write-promote", "FILE", "SENSITIVE", &["doc"]).await;
    let read = fs
        .read_object(&written.object_id, None)
        .await
        .expect("read promoted object");

    assert_eq!(read.version.version_id, written.version_id);
    assert_eq!(read.chunks[0].ref_count, 1);
    assert_eq!(
        log.receipts().await[0].record_type(),
        RecordType::ActionReceived
    );
}

#[tokio::test]
async fn s1_3_fixture_cas_conflict_fails_closed_and_records_conflict_event() {
    let (log, emitter) = evidence_fixture();
    let fs = InMemoryAiosFs::with_evidence_emitter(emitter.clone());
    let first = write_fixture(&fs, "root", "FILE", "SENSITIVE", &["doc"]).await;
    let stale_snapshot = first.snapshot_id_after.clone();
    let _concurrent = write_fixture(&fs, "concurrent", "FILE", "SENSITIVE", &["doc"]).await;

    let err = fs
        .write_object(
            append_request(
                first.object_id.clone(),
                first.version_id.clone(),
                "candidate",
                "SENSITIVE",
                vec![chunk_ref(b"candidate")],
            ),
            &context_at("family:alice", stale_snapshot),
        )
        .await
        .expect_err("stale snapshot write must fail closed");
    assert!(matches!(err, FsError::SnapshotStale { .. }));

    aios_fs::record_conflict_event(
        &emitter,
        &first.object_id,
        "candidate lost stale snapshot CAS",
        "resolve",
    )
    .await
    .expect("record conflict");
    assert!(log
        .receipts()
        .await
        .iter()
        .any(|receipt| receipt.record_type() == RecordType::ConflictEvent));
}

#[tokio::test]
async fn s1_3_fixture_multi_pointer_atomicity_rolls_back_on_second_failure() {
    let fs = InMemoryAiosFs::new();
    let left = write_fixture(&fs, "left", "FILE", "SENSITIVE", &["doc"]).await;
    let right = write_fixture(&fs, "right", "FILE", "SENSITIVE", &["doc"]).await;

    let err = fs
        .write_object(
            append_request(
                left.object_id.clone(),
                right.version_id,
                "bad-parent",
                "SENSITIVE",
                vec![chunk_ref(b"bad-parent")],
            ),
            &context("family:alice"),
        )
        .await
        .expect_err("wrong-object parent must abort");
    assert!(matches!(err, FsError::VersionNotFound(_)));

    let read = fs
        .read_object(&left.object_id, None)
        .await
        .expect("left pointer unchanged");
    assert_eq!(read.version.version_id, left.version_id);
}

#[tokio::test]
async fn s1_3_fixture_quarantine_integrity_denies_non_recovery_read_and_emits_evidence() {
    let (log, emitter) = evidence_fixture();
    let fs = InMemoryAiosFs::new();
    let driver = QuarantineDriver::with_evidence_emitter(fs.clone(), emitter);
    let first = write_fixture(&fs, "stable", "FILE", "SENSITIVE", &["doc"]).await;
    let second = fs
        .write_object(
            append_request(
                first.object_id.clone(),
                first.version_id.clone(),
                "target",
                "SENSITIVE",
                vec![chunk_ref(b"target")],
            ),
            &context("family:alice"),
        )
        .await
        .expect("write target");
    fs.force_pointer_for_harness(&first.object_id, PointerKind::Rollback, &first.version_id)
        .expect("rollback pointer");
    let quarantine_pointer_id = fs
        .force_pointer_for_harness(
            &first.object_id,
            PointerKind::Quarantine,
            &second.version_id,
        )
        .expect("quarantine pointer");

    driver
        .enter(
            &second.version_id,
            QuarantineTrigger::IntegrityFailure,
            "chunk_integrity_failure",
            &fs,
        )
        .await
        .expect("enter quarantine");
    fs.force_object_current_pointer_for_harness(&first.object_id, &quarantine_pointer_id)
        .expect("force current to quarantine pointer");

    assert!(matches!(
        fs.read_object(&first.object_id, None).await,
        Err(FsError::QuarantineViolation(_))
    ));
    assert!(log
        .receipts()
        .await
        .iter()
        .any(|receipt| receipt.record_type() == RecordType::QuarantineEvent));
}

#[tokio::test]
async fn s1_3_fixture_gc_orphan_reclaims_zero_ref_chunk_with_evidence() {
    let (log, emitter) = evidence_fixture();
    let fs = InMemoryAiosFs::new();
    let written = write_fixture(&fs, "orphan", "FILE", "SENSITIVE", &["doc"]).await;
    fs.purge_version(&written.version_id, VersionPurgeReason::OperatorRequested)
        .expect("purge version");

    let report = GcPassDriver::with_evidence_emitter(1024, 1024, emitter)
        .run_pass(&fs)
        .await
        .expect("run gc");

    assert_eq!(report.chunks_reclaimed, 1);
    assert!(log
        .receipts()
        .await
        .iter()
        .any(|receipt| receipt.record_type() == RecordType::GcPass));
}

#[tokio::test]
async fn s1_3_fixture_privacy_class_monotonic_cannot_lower_persisted_class() {
    let fs = InMemoryAiosFs::new();
    let written = write_fixture(&fs, "secret-doc", "FILE", "SENSITIVE", &["doc"]).await;

    fs.write_object(
        append_request(
            written.object_id.clone(),
            written.version_id,
            "attempted-public",
            "PUBLIC",
            vec![chunk_ref(b"attempted-public")],
        ),
        &context("family:alice"),
    )
    .await
    .expect("append version");
    let read = fs
        .read_object(&written.object_id, None)
        .await
        .expect("read after append");

    assert_eq!(read.object.privacy_class, aios_fs::PrivacyClass::Sensitive);
}

#[tokio::test]
async fn s1_3_fixture_recovery_enumerate_lists_active_and_retired_without_llm() {
    let fs = InMemoryAiosFs::new();
    let active = write_fixture(&fs, "active", "FILE", "SENSITIVE", &["doc"]).await;
    let retired = write_fixture(&fs, "retired", "FILE", "SENSITIVE", &["doc"]).await;
    fs.force_version_state_for_harness(&retired.version_id, VersionState::RetiredVersion, None)
        .expect("retire version");

    let active_versions = fs
        .list_versions(&active.object_id)
        .await
        .expect("list active");
    let retired_versions = fs
        .list_versions(&retired.object_id)
        .await
        .expect("list retired");

    assert_eq!(active_versions[0].state, VersionState::Verified);
    assert_eq!(retired_versions[0].state, VersionState::RetiredVersion);
}

#[tokio::test]
async fn s1_3_fixture_snapshot_read_fails_closed_instead_of_mixing_snapshots() {
    let fs = InMemoryAiosFs::new();
    let left = write_fixture(&fs, "left", "FILE", "SENSITIVE", &["doc"]).await;
    let snapshot_before = left.snapshot_id_after.clone();
    let _right = write_fixture(&fs, "right", "FILE", "SENSITIVE", &["doc"]).await;

    let err = fs
        .read_object(&left.object_id, Some(&snapshot_before))
        .await
        .expect_err("stale snapshot must not produce mixed view");

    assert!(matches!(err, FsError::SnapshotStale { .. }));
}

#[tokio::test]
async fn s2_1_fixture_simple_filter_matches_project_renderer_objects() {
    let fs = InMemoryAiosFs::new();
    let keep = write_fixture(&fs, "renderer-core", "PROJECT", "PUBLIC", &["renderer"]).await;
    let _drop = write_fixture(&fs, "docs", "FILE", "PUBLIC", &["doc"]).await;
    let query = parse_query("object.kind = PROJECT and object.policy_tags contains \"renderer\"")
        .expect("query parses");

    let view = materialize_view(&query, &fs, None)
        .await
        .expect("materialize");

    assert_eq!(view.matched.len(), 1);
    assert_eq!(view.matched[0].object_id, keep.object_id);
}

#[tokio::test]
async fn s2_1_fixture_time_travel_snapshot_predicate_uses_snapshot_consistency() {
    let fs = InMemoryAiosFs::new();
    let written = write_fixture(&fs, "policy", "POLICY", "PUBLIC", &["policy"]).await;
    let query = parse_query(
        "object.kind = POLICY and version.created_at in [\"2020-01-01T00:00:00Z\", \"2100-01-01T00:00:00Z\"]",
    )
    .expect("query parses");

    let view = materialize_view(&query, &fs, Some(&written.snapshot_id_after))
        .await
        .expect("materialize against current snapshot");

    assert_eq!(view.snapshot_id, written.snapshot_id_after);
    assert_eq!(view.matched[0].object_id, written.object_id);
}

#[tokio::test]
async fn s2_1_fixture_privacy_silent_exclusion_by_caller_ceiling_predicate() {
    let fs = InMemoryAiosFs::new();
    let public = write_fixture(&fs, "public-memory", "MEMORY", "PUBLIC", &["memory"]).await;
    let _sensitive =
        write_fixture(&fs, "sensitive-memory", "MEMORY", "SENSITIVE", &["memory"]).await;
    let _classified = write_fixture(
        &fs,
        "classified-memory",
        "MEMORY",
        "CLASSIFIED",
        &["memory"],
    )
    .await;
    let query = parse_query("object.kind = MEMORY and object.privacy_class = PUBLIC")
        .expect("query parses");

    let view = materialize_view(&query, &fs, None)
        .await
        .expect("materialize");

    assert_eq!(view.matched.len(), 1);
    assert_eq!(view.matched[0].object_id, public.object_id);
}

#[test]
fn s2_1_fixture_forbidden_field_rejected_as_invalid_query() {
    let err = parse_query("object.secret_value = \"some_token\"")
        .expect_err("secret_value is outside queryable schema");

    assert!(matches!(err, aios_fs::QueryParseError::UnknownField { .. }));
}

#[tokio::test]
async fn s2_1_fixture_cursor_pagination_order_is_deterministic_under_snapshot() {
    let fs = InMemoryAiosFs::new();
    for name in ["file-a", "file-b", "file-c"] {
        write_fixture(&fs, name, "FILE", "PUBLIC", &["page"]).await;
    }
    let snapshot = fs.snapshot().snapshot_id;
    let query = parse_query("object.kind = FILE").expect("query parses");
    let first = materialize_view(&query, &fs, Some(&snapshot))
        .await
        .expect("first page source");
    let first_two_len = first.matched.iter().take(2).count();
    write_fixture(&fs, "file-d", "FILE", "PUBLIC", &["page"]).await;

    let stale = materialize_view(&query, &fs, Some(&snapshot))
        .await
        .expect_err("stale cursor snapshot fails closed");

    assert_eq!(first_two_len, 2);
    assert!(matches!(stale, FsError::SnapshotStale { .. }));
}

#[tokio::test]
async fn s2_1_fixture_materialized_refresh_rebuilds_after_write() {
    let fs = InMemoryAiosFs::new();
    write_fixture(&fs, "view-a", "FILE", "PUBLIC", &["refresh"]).await;
    let query = parse_query("object.policy_tags contains \"refresh\"").expect("query parses");
    let before = materialize_view(&query, &fs, None)
        .await
        .expect("materialize before");
    write_fixture(&fs, "view-b", "FILE", "PUBLIC", &["refresh"]).await;

    let after = materialize_view(&query, &fs, None)
        .await
        .expect("materialize after");

    assert_eq!(before.query_hash, after.query_hash);
    assert_eq!(before.matched.len(), 1);
    assert_eq!(after.matched.len(), 2);
}

#[tokio::test]
async fn s2_1_fixture_aggregation_rows_per_kind_represented_by_view_counts() {
    let fs = InMemoryAiosFs::new();
    write_fixture(&fs, "file-a", "FILE", "PUBLIC", &["aggregate"]).await;
    write_fixture(&fs, "file-b", "FILE", "PUBLIC", &["aggregate"]).await;
    write_fixture(&fs, "project-a", "PROJECT", "PUBLIC", &["aggregate"]).await;
    let files = materialize_view(&parse_query("object.kind = FILE").unwrap(), &fs, None)
        .await
        .expect("files view");
    let projects = materialize_view(&parse_query("object.kind = PROJECT").unwrap(), &fs, None)
        .await
        .expect("projects view");

    assert_eq!(files.matched.len(), 2);
    assert_eq!(projects.matched.len(), 1);
}

#[tokio::test]
async fn s2_1_fixture_in_clause_matches_project_or_workspace() {
    let fs = InMemoryAiosFs::new();
    let project = write_fixture(&fs, "project", "PROJECT", "PUBLIC", &["in"]).await;
    let workspace = write_fixture(&fs, "workspace", "WORKSPACE", "PUBLIC", &["in"]).await;
    let _file = write_fixture(&fs, "file", "FILE", "PUBLIC", &["in"]).await;
    let query = parse_query("object.kind in [PROJECT, WORKSPACE]").expect("query parses");

    let view = materialize_view(&query, &fs, None)
        .await
        .expect("materialize");
    let matched: Vec<_> = view
        .matched
        .iter()
        .map(|obj| obj.object_id.clone())
        .collect();

    assert_eq!(matched.len(), 2);
    assert!(matched.contains(&project.object_id));
    assert!(matched.contains(&workspace.object_id));
}

#[tokio::test]
async fn s2_1_fixture_budget_exhausted_fails_closed_without_partial_result() {
    let fs = InMemoryAiosFs::new();
    write_fixture(&fs, "staged-like", "FILE", "PUBLIC", &["budget"]).await;
    let query = parse_query("version.state = 5").expect("query parses");

    let err = materialize_view(&query, &fs, None)
        .await
        .expect_err("type mismatch fails closed");

    assert!(matches!(err, FsError::QueryEval(_)));
}

#[test]
fn s4_1_fixture_personal_household_namespace_resolves_expected_paths() {
    assert_eq!(
        AiosPath::new("/aios/system/apps/evidence-viewer").namespace_class(),
        Some(NamespaceClass::SystemApps)
    );
    assert_eq!(
        AiosPath::new("/aios/groups/family/users/alice/home/notes.md").namespace_class(),
        Some(NamespaceClass::UserHome)
    );
    assert_eq!(
        AiosPath::new("/aios/groups/family/inbox").namespace_class(),
        Some(NamespaceClass::GroupInbox)
    );
}

#[test]
fn s4_1_fixture_solo_developer_cross_group_denial() {
    let path = AiosPath::new("/aios/groups/homelab/apps/bg.iconys.proxguard");
    let err = NamespacePolicy::can_mutate(&path, &subject("personal:luckyngoriko"), false, false)
        .expect_err("cross-group mutation must fail");

    assert!(matches!(
        err,
        FsError::NamespaceMutationDenied { path, reason }
            if path == "/aios/groups/homelab/apps/bg.iconys.proxguard"
                && reason.contains("cross-group")
    ));
}

#[test]
fn s4_1_fixture_context_switching_changes_group_access() {
    let personal = AiosPath::new("/aios/groups/personal/users/alice/home/diary.md");
    let work = AiosPath::new("/aios/groups/work-finance/users/alice/home/notes.md");

    assert_eq!(
        NamespacePolicy::can_mutate(&personal, &subject("personal:alice"), false, false),
        Ok(())
    );
    assert!(NamespacePolicy::can_mutate(&work, &subject("personal:alice"), false, false).is_err());
    assert_eq!(
        NamespacePolicy::can_mutate(&work, &subject("work-finance:alice"), false, false),
        Ok(())
    );
    assert!(
        NamespacePolicy::can_mutate(&personal, &subject("work-finance:alice"), false, false)
            .is_err()
    );
}

#[test]
fn s4_1_fixture_path_traversal_and_reserved_ids_are_rejected() {
    for path in [
        "/aios/groups/family/../system/policy",
        "/aios/groups/family//inbox",
        "/aios/groups/family/inbox/.",
        "/etc/passwd",
    ] {
        assert_eq!(AiosPath::new(path).namespace_class(), None, "{path}");
    }

    let err = NamespacePolicy::can_mutate(
        &AiosPath::new("/aios/groups/_system/apps/evil"),
        &subject("family:alice"),
        false,
        false,
    )
    .expect_err("reserved group id must fail");
    assert!(matches!(err, FsError::InvalidPath(_)));
}

#[test]
fn s4_1_fixture_catalog_resolution_is_deterministic_for_same_path() {
    let path = AiosPath::new("/aios/groups/family/inbox");

    assert_eq!(
        path.namespace_class(),
        AiosPath::new(path.as_str()).namespace_class()
    );
}

#[test]
fn s4_1_fixture_inbox_virtual_view_resolves_but_rejects_mutation() {
    let group_inbox = AiosPath::new("/aios/groups/family/inbox");
    let user_inbox = AiosPath::new("/aios/groups/family/users/alice/inbox");

    assert_eq!(
        group_inbox.namespace_class(),
        Some(NamespaceClass::GroupInbox)
    );
    assert_eq!(
        user_inbox.namespace_class(),
        Some(NamespaceClass::UserInbox)
    );
    assert!(
        NamespacePolicy::can_mutate(&group_inbox, &subject("family:alice"), false, false).is_err()
    );
    assert!(
        NamespacePolicy::can_mutate(&user_inbox, &subject("family:alice"), false, false).is_err()
    );
}
