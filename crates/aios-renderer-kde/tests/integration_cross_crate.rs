//! T-137 cross-crate integration tests — KDE renderer ↔ renderer-cli + apps.
//!
//! Validates parity proofs, node-tree compilation, and end-to-end AppsBridge
//! against an in-process `AppsService` gRPC server.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::unwrap_in_result,
    clippy::doc_markdown,
    clippy::cast_possible_wrap,
    clippy::items_after_statements,
    clippy::significant_drop_tightening,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey};
use rand_core::OsRng;
use tonic::Request;

use aios_apps::compatibility_orchestrator::CompatibilityOrchestrator;
use aios_apps::knowledge_db::CompatibilityKnowledgeDB;
use aios_apps::package::PackageId;
use aios_apps::package_store::{AppPackage, InMemoryPackageStore, PackageStore};
use aios_apps::service::proto::RegisterPackageRequest;
use aios_apps::service::{build_router, AppsServer};
use aios_apps::session_driver::{InMemorySessionDriver, SessionDriver};
use aios_apps::update_driver::{InMemoryUpdateDriver, UpdateRollbackDriver};

use aios_renderer_kde::{
    apps_package_envelope_to_kde_node_tree, assert_parity_for_apps_domain, AppsBridge, KdeNodeTree,
    KdeNodeTreeEntry, NodeKind,
};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Convert an AppPackage into the proto wire type for RegisterPackage RPCs.
fn app_pkg_to_proto(pkg: &AppPackage) -> aios_apps::service::proto::PackageEnvelopeProto {
    aios_apps::service::proto::PackageEnvelopeProto {
        package_id: pkg.package_id.0.clone(),
        name: pkg.name.clone(),
        version: pkg.version.clone(),
        manifest_bytes: pkg.manifest_bytes.clone(),
        content_hash_blake3: pkg.content_hash_blake3.clone(),
        ed25519_signature: pkg.ed25519_signature.clone(),
        signer_public_key: pkg.signer_public_key.clone(),
        registered_at: Some(prost_types::Timestamp {
            seconds: pkg.registered_at.timestamp(),
            nanos: pkg.registered_at.timestamp_subsec_nanos() as i32,
        }),
    }
}

/// Generate a validly signed test package.
fn test_package(signing_key: &SigningKey, name: &str, version: &str) -> AppPackage {
    let verifying_key = signing_key.verifying_key();
    let manifest_json = format!(r#"{{"name":"{name}","version":"{version}"}}"#);
    let manifest_bytes = manifest_json.into_bytes();
    let content_hash_blake3 = blake3::hash(&manifest_bytes).to_hex().to_string();
    let sig = signing_key.sign(&manifest_bytes);
    AppPackage {
        package_id: PackageId(format!(
            "pkg_{}",
            ulid::Ulid::new().to_string().to_lowercase()
        )),
        name: name.to_string(),
        version: version.to_string(),
        manifest_bytes,
        content_hash_blake3,
        ed25519_signature: sig.to_bytes().to_vec(),
        signer_public_key: verifying_key.to_bytes().to_vec(),
        registered_at: chrono::Utc::now(),
    }
}

/// Boot an in-process AppsServer and return (endpoint_string, shutdown_handle).
async fn setup_apps_server() -> (String, tokio::task::JoinHandle<()>) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let mut trusted = HashMap::new();
    trusted.insert(verifying_key.to_bytes().to_vec(), "test-authority".into());

    let store = Arc::new(InMemoryPackageStore::new(trusted.clone()));
    let knowledge = Arc::new(CompatibilityKnowledgeDB::with_fixtures());
    let orchestrator = Arc::new(CompatibilityOrchestrator::new_with_defaults());
    let sessions = Arc::new(InMemorySessionDriver::new_with_defaults());
    let updates = Arc::new(InMemoryUpdateDriver::new());

    let svc = AppsServer::new(
        store.clone() as Arc<dyn PackageStore>,
        knowledge.clone(),
        sessions.clone() as Arc<dyn SessionDriver>,
        updates.clone() as Arc<dyn UpdateRollbackDriver>,
        orchestrator.clone(),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint = format!("http://{addr}");

    let router = build_router(svc);
    let shutdown = tokio::spawn(async move {
        router
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    // Give the server a moment to start accepting connections.
    tokio::time::sleep(Duration::from_millis(50)).await;

    (endpoint, shutdown)
}

/// Register a package against a connected AppsServiceClient.
async fn register_via_client(
    client: &mut aios_apps::service::proto::apps_service_client::AppsServiceClient<
        tonic::transport::Channel,
    >,
    signing_key: &SigningKey,
    name: &str,
    version: &str,
) -> String {
    let pkg = test_package(signing_key, name, version);
    let proto_pkg = app_pkg_to_proto(&pkg);
    let resp = client
        .register_package(Request::new(RegisterPackageRequest {
            package: Some(proto_pkg),
        }))
        .await
        .expect("register RPC");
    resp.into_inner().package_id
}

// ---------------------------------------------------------------------------
// Parity tests
// ---------------------------------------------------------------------------

/// 1. assert_parity_for_apps_domain returns at least one entry.
#[test]
fn assert_parity_for_apps_domain_returns_at_least_one_type() {
    let parity = assert_parity_for_apps_domain().expect("parity check");
    assert!(
        !parity.entries.is_empty(),
        "must register at least one domain type"
    );
    let entry = &parity.entries[0];
    assert_eq!(entry.type_name, "AppPackage");
    assert!(entry.parses_in_cli);
    assert!(entry.parses_in_kde);
}

/// 2. apps_package_envelope_to_kde_node_tree uses Card kind for root.
#[test]
fn apps_package_envelope_to_kde_node_tree_uses_card_kind_for_root() {
    let parity = assert_parity_for_apps_domain().unwrap();
    let pkg: AppPackage =
        serde_json::from_str(&parity.entries[0].json_sample).expect("deserialize AppPackage");
    let tree = apps_package_envelope_to_kde_node_tree(&pkg);
    assert_eq!(tree.root.kind, NodeKind::Card);
    assert_eq!(tree.root.children.len(), 2);
    assert_eq!(tree.root.children[0].kind, NodeKind::Text);
    assert_eq!(tree.root.children[1].kind, NodeKind::Text);
}

/// 3. KdeNodeTree round-trips through JSON.
#[test]
fn kde_node_tree_serialization_round_trip() {
    let entry = KdeNodeTreeEntry {
        kind: NodeKind::Card,
        label: "test-card".into(),
        children: vec![KdeNodeTreeEntry {
            kind: NodeKind::Text,
            label: "child".into(),
            children: vec![],
        }],
    };
    let tree = KdeNodeTree { root: entry };
    let json = serde_json::to_string(&tree).expect("serialize");
    let back: KdeNodeTree = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.root.kind, NodeKind::Card);
    assert_eq!(back.root.label, "test-card");
    assert_eq!(back.root.children.len(), 1);
}

/// 4. AppsBridge::connect with empty endpoint returns error.
#[tokio::test]
async fn apps_bridge_connect_empty_endpoint_returns_error() {
    let result = AppsBridge::connect("").await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    // Should be WaylandConnectError per spec
    assert!(
        matches!(
            err,
            aios_renderer_kde::KdeRendererError::WaylandConnectError(_)
        ),
        "expected WaylandConnectError, got {err:?}"
    );
}

/// 5. Every entry in a KdeNodeTree has a legal NodeKind (one of the 19).
#[test]
fn kde_node_tree_has_only_legal_node_kinds() {
    // Build a tree that exercises several kinds.
    let tree = KdeNodeTree {
        root: KdeNodeTreeEntry {
            kind: NodeKind::List,
            label: "root".into(),
            children: vec![KdeNodeTreeEntry {
                kind: NodeKind::Card,
                label: "card".into(),
                children: vec![
                    KdeNodeTreeEntry {
                        kind: NodeKind::Text,
                        label: "text".into(),
                        children: vec![],
                    },
                    KdeNodeTreeEntry {
                        kind: NodeKind::Heading,
                        label: "heading".into(),
                        children: vec![],
                    },
                ],
            }],
        },
    };

    // Walk the tree recursively and check every kind.
    fn walk(entry: &KdeNodeTreeEntry) {
        assert!(
            NodeKind::ALL.contains(&entry.kind),
            "illegal NodeKind in tree: {entry:?}"
        );
        for child in &entry.children {
            walk(child);
        }
    }
    walk(&tree.root);
}

// ---------------------------------------------------------------------------
// End-to-end AppsBridge tests (in-process AppsServer)
// ---------------------------------------------------------------------------

/// 6. AppsBridge connects to a live in-process server.
#[tokio::test]
async fn apps_bridge_connect_to_local_apps_server_succeeds() {
    let (endpoint, _shutdown) = setup_apps_server().await;
    let bridge = AppsBridge::connect(endpoint).await;
    assert!(bridge.is_ok(), "must connect to local server");
}

/// 7. render_package_list empty → List root with zero children.
#[tokio::test]
async fn apps_bridge_render_package_list_empty_returns_empty_list_node() {
    let (endpoint, _shutdown) = setup_apps_server().await;
    let mut bridge = AppsBridge::connect(endpoint)
        .await
        .expect("connect to server");
    let tree = bridge
        .render_package_list_as_kde_tree()
        .await
        .expect("list RPC");
    assert_eq!(tree.root.kind, NodeKind::List);
    assert_eq!(tree.root.label, "Packages");
    assert!(tree.root.children.is_empty());
}

/// 8. Register one package → list returns one Card child.
#[tokio::test]
async fn apps_bridge_render_package_list_after_register_returns_one_card_child() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let mut trusted = HashMap::new();
    trusted.insert(verifying_key.to_bytes().to_vec(), "test-authority".into());
    let store = Arc::new(InMemoryPackageStore::new(trusted));
    let knowledge = Arc::new(CompatibilityKnowledgeDB::with_fixtures());
    let orchestrator = Arc::new(CompatibilityOrchestrator::new_with_defaults());
    let sessions = Arc::new(InMemorySessionDriver::new_with_defaults());
    let updates = Arc::new(InMemoryUpdateDriver::new());

    let svc = AppsServer::new(
        store.clone() as Arc<dyn PackageStore>,
        knowledge,
        sessions as Arc<dyn SessionDriver>,
        updates as Arc<dyn UpdateRollbackDriver>,
        orchestrator,
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let ep = format!("http://{addr}");

    let router = build_router(svc);
    let _jh = tokio::spawn(async move {
        router
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect a raw client and register one package.
    let mut raw_client =
        aios_apps::service::proto::apps_service_client::AppsServiceClient::connect(ep.clone())
            .await
            .unwrap();
    register_via_client(&mut raw_client, &signing_key, "firefox", "125.0.1").await;

    // Now connect via AppsBridge and list.
    let mut bridge = AppsBridge::connect(ep).await.expect("connect bridge");
    let tree = bridge
        .render_package_list_as_kde_tree()
        .await
        .expect("list RPC");
    assert_eq!(tree.root.kind, NodeKind::List);
    assert_eq!(tree.root.children.len(), 1);
    let card = &tree.root.children[0];
    assert_eq!(card.kind, NodeKind::Card);
    assert!(card.label.contains("firefox"), "label: {}", card.label);
    assert!(card.label.contains("125.0.1"), "label: {}", card.label);
}

/// 9. render_package_show returns Card with version child.
#[tokio::test]
async fn apps_bridge_render_package_show_returns_card_with_version_child() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let mut trusted = HashMap::new();
    trusted.insert(verifying_key.to_bytes().to_vec(), "test-authority".into());
    let store = Arc::new(InMemoryPackageStore::new(trusted));
    let knowledge = Arc::new(CompatibilityKnowledgeDB::with_fixtures());
    let orchestrator = Arc::new(CompatibilityOrchestrator::new_with_defaults());
    let sessions = Arc::new(InMemorySessionDriver::new_with_defaults());
    let updates = Arc::new(InMemoryUpdateDriver::new());

    let svc = AppsServer::new(
        store.clone() as Arc<dyn PackageStore>,
        knowledge,
        sessions as Arc<dyn SessionDriver>,
        updates as Arc<dyn UpdateRollbackDriver>,
        orchestrator,
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let ep = format!("http://{addr}");

    let router = build_router(svc);
    let _jh = tokio::spawn(async move {
        router
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut raw_client =
        aios_apps::service::proto::apps_service_client::AppsServiceClient::connect(ep.clone())
            .await
            .unwrap();
    let pkg_id = register_via_client(&mut raw_client, &signing_key, "thunderbird", "115.0").await;

    let mut bridge = AppsBridge::connect(ep).await.expect("connect bridge");
    let tree = bridge
        .render_package_show_as_kde_tree(&pkg_id)
        .await
        .expect("show RPC");
    assert_eq!(tree.root.kind, NodeKind::Card);
    assert!(tree.root.label.contains("thunderbird"));
    let version_child = &tree.root.children[0];
    assert_eq!(version_child.kind, NodeKind::Text);
    assert!(
        version_child.label.contains("115.0"),
        "version label: {}",
        version_child.label
    );
}

/// 10. render_package_show for unknown package → Internal error.
#[tokio::test]
async fn apps_bridge_render_package_show_unknown_returns_internal_error() {
    let (endpoint, _shutdown) = setup_apps_server().await;
    let mut bridge = AppsBridge::connect(endpoint).await.expect("connect bridge");
    let result = bridge
        .render_package_show_as_kde_tree("pkg_nonexistent_0000000000")
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, aios_renderer_kde::KdeRendererError::Internal(_)),
        "unknown package must return Internal, got {err:?}"
    );
}
