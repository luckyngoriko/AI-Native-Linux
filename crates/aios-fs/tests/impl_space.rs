//! T-041 integration tests for S2.2 implementation-space bindings.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::{TimeZone, Utc};

use aios_fs::{
    ChunkId, FsError, ImplSpace, ImplSpaceBinding, ImplSpaceSource, ImplSpaceTarget,
    InMemoryImplSpace, IntegrityState, ObjectId, SubjectRef, VersionId,
};

fn subject(id: &str) -> SubjectRef {
    SubjectRef(id.to_owned())
}

fn sample_time() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 24, 12, 0, 0)
        .single()
        .expect("fixture timestamp is valid")
}

fn object_id() -> ObjectId {
    ObjectId::parse("obj_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("object id")
}

fn second_object_id() -> ObjectId {
    ObjectId::parse("obj_01HXY8K2JPQ7N3M4R5S6T7V8W8").expect("object id")
}

fn version_id() -> VersionId {
    VersionId::parse("ver_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("version id")
}

fn chunk_id() -> ChunkId {
    ChunkId::from_hash_bytes(b"impl-space chunk")
}

fn binding(binding_id: &str, source: ImplSpaceSource, target: ImplSpaceTarget) -> ImplSpaceBinding {
    ImplSpaceBinding {
        binding_id: binding_id.to_owned(),
        object_or_chunk_id: source,
        target,
        created_at: sample_time(),
        created_by: subject("family:alice"),
        last_verified_at: None,
        integrity_state: IntegrityState::Verified,
    }
}

#[tokio::test]
async fn record_binding_then_resolve_returns_the_binding() {
    let impl_space = InMemoryImplSpace::new();
    let source = ImplSpaceSource::Object(object_id());
    let recorded = binding(
        "ispb_01HXY8K2JPQ7N3M4R5S6T7V8W9",
        source.clone(),
        ImplSpaceTarget::LocalFile {
            path: "/aios/store/objects/demo".to_owned(),
        },
    );

    impl_space
        .record_binding(recorded.clone())
        .await
        .expect("record binding");

    assert_eq!(
        impl_space.resolve(&source).await.expect("resolve source"),
        vec![recorded]
    );
}

#[tokio::test]
async fn resolve_unknown_source_returns_empty_vec() {
    let impl_space = InMemoryImplSpace::new();

    let resolved = impl_space
        .resolve(&ImplSpaceSource::Object(object_id()))
        .await
        .expect("resolve unknown source");

    assert!(resolved.is_empty());
}

#[tokio::test]
async fn record_binding_rejects_duplicate_binding_id() {
    let impl_space = InMemoryImplSpace::new();
    let first = binding(
        "ispb_01HXY8K2JPQ7N3M4R5S6T7V8W9",
        ImplSpaceSource::Object(object_id()),
        ImplSpaceTarget::LocalFile {
            path: "/aios/store/objects/demo".to_owned(),
        },
    );
    let duplicate = binding(
        "ispb_01HXY8K2JPQ7N3M4R5S6T7V8W9",
        ImplSpaceSource::Object(second_object_id()),
        ImplSpaceTarget::AiosFsManaged {
            handle: "managed/demo".to_owned(),
        },
    );

    impl_space
        .record_binding(first)
        .await
        .expect("record first binding");
    let err = impl_space
        .record_binding(duplicate)
        .await
        .expect_err("duplicate binding id must fail");

    assert!(matches!(err, FsError::Internal(_)));
    assert!(err.to_string().contains("duplicate impl-space binding"));
}

#[tokio::test]
async fn list_for_with_multiple_bindings_on_same_source_returns_all() {
    let impl_space = InMemoryImplSpace::new();
    let source = ImplSpaceSource::Object(object_id());
    let local = binding(
        "ispb_01HXY8K2JPQ7N3M4R5S6T7V8W9",
        source.clone(),
        ImplSpaceTarget::LocalFile {
            path: "/aios/store/objects/demo".to_owned(),
        },
    );
    let remote = binding(
        "ispb_01HXY8K2JPQ7N3M4R5S6T7V8W8",
        source.clone(),
        ImplSpaceTarget::RemoteBlob {
            url: "https://objects.example.invalid/obj".to_owned(),
            etag: Some("etag-1".to_owned()),
        },
    );

    impl_space
        .record_binding(local.clone())
        .await
        .expect("record local binding");
    impl_space
        .record_binding(remote.clone())
        .await
        .expect("record remote binding");

    let bindings = impl_space.list_for(&source).await.expect("list source");
    assert_eq!(bindings.len(), 2);
    assert!(bindings.contains(&local));
    assert!(bindings.contains(&remote));
}

#[tokio::test]
async fn verify_on_verified_binding_returns_verified_state() {
    let impl_space = InMemoryImplSpace::new();
    let recorded = binding(
        "ispb_01HXY8K2JPQ7N3M4R5S6T7V8W9",
        ImplSpaceSource::Chunk(chunk_id()),
        ImplSpaceTarget::EncryptedBlob {
            blob_id: "blob/demo".to_owned(),
            key_capability_id: "cap/key/demo".to_owned(),
        },
    );
    impl_space
        .record_binding(recorded.clone())
        .await
        .expect("record binding");

    assert_eq!(
        impl_space
            .verify(&recorded.binding_id)
            .await
            .expect("verify binding"),
        IntegrityState::Verified
    );
}

#[tokio::test]
async fn verify_on_unknown_binding_id_returns_not_found() {
    let impl_space = InMemoryImplSpace::new();
    let missing = "ispb_01HXY8K2JPQ7N3M4R5S6T7V8W9";

    let err = impl_space
        .verify(missing)
        .await
        .expect_err("unknown binding id must fail");

    assert_eq!(err, FsError::ImplSpaceBindingNotFound(missing.to_owned()));
}

#[test]
fn local_file_target_round_trips_through_serde_json() {
    let target = ImplSpaceTarget::LocalFile {
        path: "/aios/store/objects/demo".to_owned(),
    };

    let json = serde_json::to_string(&target).expect("serialise target");
    let back: ImplSpaceTarget = serde_json::from_str(&json).expect("deserialise target");

    assert_eq!(back, target);
}

#[test]
fn encrypted_blob_target_round_trips_through_serde_json() {
    let target = ImplSpaceTarget::EncryptedBlob {
        blob_id: "blob/demo".to_owned(),
        key_capability_id: "cap/key/demo".to_owned(),
    };

    let json = serde_json::to_string(&target).expect("serialise target");
    let back: ImplSpaceTarget = serde_json::from_str(&json).expect("deserialise target");

    assert_eq!(back, target);
}

#[test]
fn remote_blob_target_round_trips_through_serde_json() {
    let target = ImplSpaceTarget::RemoteBlob {
        url: "https://objects.example.invalid/obj".to_owned(),
        etag: Some("etag-1".to_owned()),
    };

    let json = serde_json::to_string(&target).expect("serialise target");
    let back: ImplSpaceTarget = serde_json::from_str(&json).expect("deserialise target");

    assert_eq!(back, target);
}

#[test]
fn aios_fs_managed_target_round_trips_through_serde_json() {
    let target = ImplSpaceTarget::AiosFsManaged {
        handle: "managed/demo".to_owned(),
    };

    let json = serde_json::to_string(&target).expect("serialise target");
    let back: ImplSpaceTarget = serde_json::from_str(&json).expect("deserialise target");

    assert_eq!(back, target);
}

#[test]
fn object_source_round_trips_through_serde_json() {
    let source = ImplSpaceSource::Object(object_id());

    let json = serde_json::to_string(&source).expect("serialise source");
    let back: ImplSpaceSource = serde_json::from_str(&json).expect("deserialise source");

    assert_eq!(back, source);
}

#[test]
fn chunk_source_round_trips_through_serde_json() {
    let source = ImplSpaceSource::Chunk(chunk_id());

    let json = serde_json::to_string(&source).expect("serialise source");
    let back: ImplSpaceSource = serde_json::from_str(&json).expect("deserialise source");

    assert_eq!(back, source);
}

#[test]
fn version_source_round_trips_through_serde_json() {
    let source = ImplSpaceSource::Version(version_id());

    let json = serde_json::to_string(&source).expect("serialise source");
    let back: ImplSpaceSource = serde_json::from_str(&json).expect("deserialise source");

    assert_eq!(back, source);
}

#[test]
fn integrity_state_verified_round_trips_with_screaming_snake_case_wire_form() {
    let state = IntegrityState::Verified;

    let json = serde_json::to_string(&state).expect("serialise state");
    let back: IntegrityState = serde_json::from_str(&json).expect("deserialise state");

    assert_eq!(json, "\"VERIFIED\"");
    assert_eq!(back, state);
}

#[test]
fn integrity_state_stale_round_trips_with_screaming_snake_case_wire_form() {
    let state = IntegrityState::Stale;

    let json = serde_json::to_string(&state).expect("serialise state");
    let back: IntegrityState = serde_json::from_str(&json).expect("deserialise state");

    assert_eq!(json, "\"STALE\"");
    assert_eq!(back, state);
}

#[test]
fn integrity_state_integrity_failed_round_trips_with_screaming_snake_case_wire_form() {
    let state = IntegrityState::IntegrityFailed;

    let json = serde_json::to_string(&state).expect("serialise state");
    let back: IntegrityState = serde_json::from_str(&json).expect("deserialise state");

    assert_eq!(json, "\"INTEGRITY_FAILED\"");
    assert_eq!(back, state);
}

#[test]
fn integrity_state_unknown_round_trips_with_screaming_snake_case_wire_form() {
    let state = IntegrityState::Unknown;

    let json = serde_json::to_string(&state).expect("serialise state");
    let back: IntegrityState = serde_json::from_str(&json).expect("deserialise state");

    assert_eq!(json, "\"UNKNOWN\"");
    assert_eq!(back, state);
}

#[test]
fn trait_usage_via_arc_dyn_impl_space_compiles() {
    let _impl_space: Arc<dyn ImplSpace> = Arc::new(InMemoryImplSpace::new());
}

#[tokio::test]
async fn with_fixtures_loads_three_canonical_bindings() {
    let impl_space = InMemoryImplSpace::with_fixtures();

    let local = impl_space
        .list_for(&ImplSpaceSource::Object(object_id()))
        .await
        .expect("list object fixture");
    let encrypted = impl_space
        .list_for(&ImplSpaceSource::Chunk(chunk_id()))
        .await
        .expect("list chunk fixture");
    let managed = impl_space
        .list_for(&ImplSpaceSource::Version(version_id()))
        .await
        .expect("list version fixture");

    assert_eq!(local.len(), 1);
    assert_eq!(encrypted.len(), 1);
    assert_eq!(managed.len(), 1);
    assert!(matches!(local[0].target, ImplSpaceTarget::LocalFile { .. }));
    assert!(matches!(
        encrypted[0].target,
        ImplSpaceTarget::EncryptedBlob { .. }
    ));
    assert!(matches!(
        managed[0].target,
        ImplSpaceTarget::AiosFsManaged { .. }
    ));
}

#[tokio::test]
async fn end_to_end_trait_resolve_returns_expected_target_type() {
    let impl_space: Arc<dyn ImplSpace> = Arc::new(InMemoryImplSpace::with_fixtures());

    let bindings = impl_space
        .resolve(&ImplSpaceSource::Object(object_id()))
        .await
        .expect("resolve object fixture");

    assert_eq!(bindings.len(), 1);
    assert!(matches!(
        bindings[0].target,
        ImplSpaceTarget::LocalFile { .. }
    ));
}
