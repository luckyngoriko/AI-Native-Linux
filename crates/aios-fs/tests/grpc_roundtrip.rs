//! T-043 integration tests for the `aios.fs.v1alpha1.AiosFs` gRPC surface.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::items_after_statements,
    clippy::result_large_err,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tonic::{Code, Request};

use aios_fs::service::conversions::{fs_error_to_status, json_to_struct};
use aios_fs::service::proto::aios_fs_server::AiosFs;
use aios_fs::service::proto::{
    self, ConsistencyClassProto, GcPassReportProto, MaterializeViewRequestProto,
    ObjectWriteRequestProto, QuarantineObjectRequestProto, QuarantineTriggerProto,
    ReadObjectRequestProto, RunGcPassRequestProto,
};
use aios_fs::service::{AiosFsClient, AiosFsGrpcServer, AiosFsService};
use aios_fs::{
    AiosFs as _, ChunkId, ChunkRef, ConsistencyClass, FsContext, FsError, InMemoryAiosFs, ObjectId,
    ObjectWriteRequest, PointerKind, SnapshotId, SubjectRef, VersionId, VersionState,
};

fn subject(id: &str) -> SubjectRef {
    SubjectRef(id.to_owned())
}

fn context(id: &str) -> FsContext {
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

fn write_request(subject_id: &str, name: &str, chunks: Vec<ChunkRef>) -> ObjectWriteRequest {
    ObjectWriteRequest {
        object_id: None,
        parent_version_ids: Vec::new(),
        chunks,
        metadata_delta: serde_json::json!({
            "name": name,
            "mime": "text/plain"
        }),
        action_id: None,
        subject: subject(subject_id),
    }
}

fn append_request(
    object_id: ObjectId,
    parent_version_id: VersionId,
    subject_id: &str,
    name: &str,
) -> ObjectWriteRequest {
    ObjectWriteRequest {
        object_id: Some(object_id),
        parent_version_ids: vec![parent_version_id],
        chunks: vec![chunk_ref(name.as_bytes())],
        metadata_delta: serde_json::json!({
            "name": name,
            "mime": "text/plain"
        }),
        action_id: None,
        subject: subject(subject_id),
    }
}

fn write_proto(name: &str, chunks: Vec<ChunkRef>) -> ObjectWriteRequestProto {
    ObjectWriteRequestProto {
        object_id: String::new(),
        parent_version_ids: Vec::new(),
        chunk_refs: chunks
            .into_iter()
            .map(|chunk_ref| chunk_ref.0.to_string())
            .collect(),
        metadata_delta: Some(json_to_struct(&serde_json::json!({
            "name": name,
            "mime": "text/plain"
        }))),
        action_id_proto: Vec::new(),
        subject: "family:alice".to_owned(),
        expected_snapshot_id: String::new(),
        consistency_class: i32::from(ConsistencyClassProto::Snapshot),
    }
}

async fn write_one(fs: &InMemoryAiosFs, name: &str) -> aios_fs::ObjectWriteResult {
    fs.write_object(
        write_request("family:alice", name, vec![chunk_ref(name.as_bytes())]),
        &context("family:alice"),
    )
    .await
    .expect("write object")
}

async fn pick_port() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind random port");
    let addr = listener.local_addr().expect("local_addr");
    drop(listener);
    addr
}

async fn spawn_server(
    svc: AiosFsService,
) -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let addr = pick_port().await;
    let (tx, rx) = oneshot::channel::<()>();
    let server = tonic::transport::Server::builder().add_service(AiosFsGrpcServer::new(svc));
    let handle = tokio::spawn(async move {
        server
            .serve_with_shutdown(addr, async move {
                let _ = rx.await;
            })
            .await
            .expect("server task");
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    (addr, tx, handle)
}

#[tokio::test]
async fn read_object_happy_path_returns_object_proto() {
    let fs = Arc::new(InMemoryAiosFs::new());
    let written = write_one(&fs, "read-target").await;
    let svc = AiosFsService::new(Arc::clone(&fs));

    let response = svc
        .read_object(Request::new(ReadObjectRequestProto {
            object_id: written.object_id.to_string(),
            snapshot_id: String::new(),
            consistency_class: i32::from(ConsistencyClassProto::Snapshot),
        }))
        .await
        .expect("read ok")
        .into_inner();

    let object = response.object.expect("object proto");
    assert_eq!(object.object_id, written.object_id.to_string());
    assert_eq!(
        response.version.expect("version proto").version_id,
        written.version_id.to_string()
    );
}

#[tokio::test]
async fn read_object_unknown_maps_to_not_found() {
    let fs = Arc::new(InMemoryAiosFs::new());
    let svc = AiosFsService::new(fs);
    let missing = ObjectId::new();

    let err = svc
        .read_object(Request::new(ReadObjectRequestProto {
            object_id: missing.to_string(),
            snapshot_id: String::new(),
            consistency_class: i32::from(ConsistencyClassProto::Snapshot),
        }))
        .await
        .expect_err("missing object");

    assert_eq!(err.code(), Code::NotFound);
}

#[tokio::test]
async fn write_object_happy_path_returns_result_proto() {
    let fs = Arc::new(InMemoryAiosFs::new());
    let svc = AiosFsService::new(fs);

    let response = svc
        .write_object(Request::new(write_proto(
            "created-through-grpc",
            vec![chunk_ref(b"created-through-grpc")],
        )))
        .await
        .expect("write ok")
        .into_inner();

    assert!(response.object_id.starts_with("obj_"));
    assert!(response.version_id.starts_with("ver_"));
    assert!(response.transaction_id.starts_with("txn_"));
    assert!(response.snapshot_id_after.starts_with("snap_"));
}

#[tokio::test]
async fn write_object_without_parent_on_existing_maps_to_invalid_argument() {
    let fs = Arc::new(InMemoryAiosFs::new());
    let first = write_one(&fs, "first").await;
    let svc = AiosFsService::new(fs);
    let mut request = write_proto("bad-append", vec![chunk_ref(b"bad-append")]);
    request.object_id = first.object_id.to_string();

    let err = svc
        .write_object(Request::new(request))
        .await
        .expect_err("existing object write without parent");

    assert_eq!(err.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn get_snapshot_returns_valid_summary_proto() {
    let fs = Arc::new(InMemoryAiosFs::new());
    let written = write_one(&fs, "snapshotted").await;
    let svc = AiosFsService::new(fs);

    let response = svc
        .get_snapshot(Request::new(proto::GetSnapshotRequestProto {
            snapshot_id: written.snapshot_id_after.to_string(),
        }))
        .await
        .expect("snapshot ok")
        .into_inner();

    assert_eq!(response.snapshot_id, written.snapshot_id_after.to_string());
    assert_eq!(response.object_count, 1);
    assert_eq!(response.pointer_count, 1);
    assert!(response.at.is_some());
}

#[tokio::test]
async fn list_versions_for_known_object_returns_version_protos() {
    let fs = Arc::new(InMemoryAiosFs::new());
    let first = write_one(&fs, "v1").await;
    let second = fs
        .write_object(
            append_request(
                first.object_id.clone(),
                first.version_id.clone(),
                "family:alice",
                "v2",
            ),
            &context("family:alice"),
        )
        .await
        .expect("second write");
    let svc = AiosFsService::new(fs);

    let response = svc
        .list_versions(Request::new(proto::ListVersionsRequestProto {
            object_id: first.object_id.to_string(),
        }))
        .await
        .expect("list ok")
        .into_inner();

    let version_ids: Vec<String> = response
        .versions
        .into_iter()
        .map(|version| version.version_id)
        .collect();
    assert_eq!(
        version_ids,
        vec![first.version_id.to_string(), second.version_id.to_string()]
    );
}

#[tokio::test]
async fn resolve_pointer_returns_pointer_proto() {
    let fs = Arc::new(InMemoryAiosFs::new());
    let written = write_one(&fs, "pointer-target").await;
    let read = fs
        .read_object(&written.object_id, None)
        .await
        .expect("read current");
    let svc = AiosFsService::new(fs);

    let response = svc
        .resolve_pointer(Request::new(proto::ResolvePointerRequestProto {
            pointer_id: read.object.current_pointer_id.to_string(),
        }))
        .await
        .expect("resolve ok")
        .into_inner();

    assert_eq!(response.object_id, written.object_id.to_string());
    assert_eq!(response.current_version_id, written.version_id.to_string());
}

#[tokio::test]
async fn quarantine_object_returns_receipt_and_marks_version_quarantined() {
    let fs = Arc::new(InMemoryAiosFs::new());
    let first = write_one(&fs, "stable").await;
    let second = fs
        .write_object(
            append_request(
                first.object_id.clone(),
                first.version_id.clone(),
                "family:alice",
                "suspect",
            ),
            &context("family:alice"),
        )
        .await
        .expect("second write");
    let _rollback = fs
        .force_pointer_for_harness(&first.object_id, PointerKind::Rollback, &first.version_id)
        .expect("rollback pointer");
    let svc = AiosFsService::new(Arc::clone(&fs));

    let receipt = svc
        .quarantine_object(Request::new(QuarantineObjectRequestProto {
            version_id: second.version_id.to_string(),
            trigger: i32::from(QuarantineTriggerProto::OperatorManual),
            reason: "operator test".to_owned(),
            action_id_proto: Vec::new(),
            subject: "family:alice".to_owned(),
        }))
        .await
        .expect("quarantine ok")
        .into_inner();

    assert!(receipt.quarantine_id.starts_with("qnt_"));
    assert_eq!(receipt.version_id, second.version_id.to_string());

    let versions = fs
        .list_versions(&first.object_id)
        .await
        .expect("list versions");
    let quarantined = versions
        .iter()
        .find(|version| version.version_id == second.version_id)
        .expect("second version present");
    assert_eq!(quarantined.state, VersionState::Quarantined);
}

#[tokio::test]
async fn run_gc_pass_returns_report_with_counts() {
    let fs = Arc::new(InMemoryAiosFs::new());
    let written = write_one(&fs, "retired").await;
    fs.force_version_state_for_harness(&written.version_id, VersionState::RetiredVersion, None)
        .expect("retire version");
    let svc = AiosFsService::new(fs);

    let report: GcPassReportProto = svc
        .run_gc_pass(Request::new(RunGcPassRequestProto {
            max_chunks_per_pass: 100,
            max_versions_per_pass: 100,
        }))
        .await
        .expect("gc ok")
        .into_inner();

    assert!(report.pass_id.starts_with("gcp_"));
    assert_eq!(report.versions_inspected, 1);
    assert_eq!(report.versions_purged, 1);
    assert_eq!(report.chunks_inspected, 1);
    assert_eq!(report.chunks_reclaimed, 1);
}

#[tokio::test]
async fn materialize_view_with_valid_query_returns_matched_object_refs() {
    let fs = Arc::new(InMemoryAiosFs::new());
    let matched = write_one(&fs, "needle").await;
    let _other = write_one(&fs, "other").await;
    let svc = AiosFsService::new(fs);

    let response = svc
        .materialize_view(Request::new(MaterializeViewRequestProto {
            query: Some(proto::QueryProto {
                source: "object.metadata.name = \"needle\"".to_owned(),
                predicates: Vec::new(),
            }),
            snapshot_id: String::new(),
        }))
        .await
        .expect("view ok")
        .into_inner();

    assert_eq!(response.matched.len(), 1);
    assert_eq!(response.matched[0].object_id, matched.object_id.to_string());
    assert!(response.query_hash.len() > 16);
}

#[test]
fn status_code_mapping_snapshot_stale_is_aborted() {
    let status = fs_error_to_status(&FsError::SnapshotStale {
        expected: SnapshotId("snap_expected".to_owned()),
        found: SnapshotId("snap_found".to_owned()),
    });

    assert_eq!(status.code(), Code::Aborted);
}

#[tokio::test]
async fn tonic_in_process_channel_smoke_test() {
    let fs = Arc::new(InMemoryAiosFs::new());
    let svc = AiosFsService::new(fs);
    let (addr, shutdown, handle) = spawn_server(svc).await;
    let mut client = AiosFsClient::connect(format!("http://{addr}"))
        .await
        .expect("client connect");

    let written = client
        .write_object(write_proto(
            "through-client",
            vec![chunk_ref(b"through-client")],
        ))
        .await
        .expect("client write")
        .into_inner();
    let read = client
        .read_object(ReadObjectRequestProto {
            object_id: written.object_id.clone(),
            snapshot_id: String::new(),
            consistency_class: i32::from(ConsistencyClassProto::Snapshot),
        })
        .await
        .expect("client read")
        .into_inner();
    drop(client);

    assert_eq!(read.object.expect("object").object_id, written.object_id);

    let _ = shutdown.send(());
    let _ = handle.await;
}
