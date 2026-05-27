//! T-149 cross-crate integration tests — Web renderer ↔ CLI + KDE + apps.
//!
//! Validates three-way parity proofs, node-tree compilation, and end-to-end
//! WebAppsBridge against an in-process `AppsService` gRPC server.

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

use aios_renderer_web::{
    apps_package_envelope_to_web_render_tree, assert_three_way_parity_for_apps_domain, NodeKind,
    WebAppsBridge, WebRenderTree, WebRenderTreeEntry,
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
    let content_hash_blake3 = aios_apps::blake3_hex(&manifest_bytes);
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

/// 1. assert_three_way_parity_for_apps_domain returns at least one entry.
#[test]
fn assert_three_way_parity_for_apps_domain_returns_at_least_one_type() {
    let parity = assert_three_way_parity_for_apps_domain().expect("parity check");
    assert!(
        !parity.entries.is_empty(),
        "must register at least one domain type"
    );
    let entry = &parity.entries[0];
    assert_eq!(entry.type_name, "AppPackage");
    assert!(entry.parses_in_cli);
    assert!(entry.parses_in_kde);
    assert!(entry.parses_in_web);
}

/// 2. apps_package_envelope_to_web_render_tree uses Card kind for root.
#[test]
fn apps_package_envelope_to_web_render_tree_uses_card_kind_for_root() {
    let parity = assert_three_way_parity_for_apps_domain().unwrap();
    let pkg: AppPackage =
        serde_json::from_str(&parity.entries[0].json_sample).expect("deserialize AppPackage");
    let tree = apps_package_envelope_to_web_render_tree(&pkg);
    assert_eq!(tree.root.kind, NodeKind::Card);
    assert_eq!(tree.root.dom_tag, "div");
    assert_eq!(tree.root.children.len(), 2);
    assert_eq!(tree.root.children[0].kind, NodeKind::Text);
    assert_eq!(tree.root.children[0].dom_tag, "span");
    assert_eq!(tree.root.children[1].kind, NodeKind::Text);
    assert_eq!(tree.root.children[1].dom_tag, "span");
}

/// 3. KDE and Web trees have the same root NodeKind for the same envelope.
#[test]
fn kde_and_web_trees_have_same_root_kind_for_same_envelope() {
    let parity = assert_three_way_parity_for_apps_domain().unwrap();
    let pkg: AppPackage =
        serde_json::from_str(&parity.entries[0].json_sample).expect("deserialize AppPackage");
    let kde_tree = aios_renderer_kde::integration::apps_package_envelope_to_kde_node_tree(&pkg);
    let web_tree = apps_package_envelope_to_web_render_tree(&pkg);
    assert_eq!(
        kde_tree.root.kind, web_tree.root.kind,
        "KDE and Web root NodeKind must be identical"
    );
}

/// 4. KDE and Web trees have the same children count for the same envelope.
#[test]
fn kde_and_web_trees_have_same_children_count_for_same_envelope() {
    let parity = assert_three_way_parity_for_apps_domain().unwrap();
    let pkg: AppPackage =
        serde_json::from_str(&parity.entries[0].json_sample).expect("deserialize AppPackage");
    let kde_tree = aios_renderer_kde::integration::apps_package_envelope_to_kde_node_tree(&pkg);
    let web_tree = apps_package_envelope_to_web_render_tree(&pkg);
    assert_eq!(
        kde_tree.root.children.len(),
        web_tree.root.children.len(),
        "KDE and Web children count must be identical"
    );
}

/// 5. WebRenderTree round-trips through JSON serialization.
#[test]
fn web_render_tree_serialization_round_trip() {
    let entry = WebRenderTreeEntry {
        kind: NodeKind::Card,
        dom_tag: "div",
        label: "test-card".into(),
        children: vec![WebRenderTreeEntry {
            kind: NodeKind::Text,
            dom_tag: "span",
            label: "child".into(),
            children: vec![],
        }],
    };
    let tree = WebRenderTree { root: entry };
    let json = serde_json::to_string(&tree).expect("serialize");
    let back: WebRenderTree = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.root.kind, NodeKind::Card);
    assert_eq!(back.root.dom_tag, "div");
    assert_eq!(back.root.label, "test-card");
    assert_eq!(back.root.children.len(), 1);
    assert_eq!(back.root.children[0].kind, NodeKind::Text);
    assert_eq!(back.root.children[0].dom_tag, "span");
}

/// 6. WebAppsBridge::connect with empty endpoint returns error.
#[tokio::test]
async fn web_apps_bridge_connect_empty_endpoint_returns_error() {
    let result = WebAppsBridge::connect("").await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, aios_renderer_web::WebRendererError::Internal(_)),
        "expected Internal error, got {err:?}"
    );
    assert!(
        err.to_string().contains("empty endpoint"),
        "error must mention empty endpoint: {err}"
    );
}

/// 7. Every entry in a WebRenderTree has a legal NodeKind (one of the 19).
#[test]
fn web_render_tree_has_only_legal_node_kinds() {
    let tree = WebRenderTree {
        root: WebRenderTreeEntry {
            kind: NodeKind::List,
            dom_tag: "ul",
            label: "root".into(),
            children: vec![WebRenderTreeEntry {
                kind: NodeKind::Card,
                dom_tag: "div",
                label: "card".into(),
                children: vec![
                    WebRenderTreeEntry {
                        kind: NodeKind::Text,
                        dom_tag: "span",
                        label: "text".into(),
                        children: vec![],
                    },
                    WebRenderTreeEntry {
                        kind: NodeKind::Heading,
                        dom_tag: "h2",
                        label: "heading".into(),
                        children: vec![],
                    },
                ],
            }],
        },
    };

    fn walk(entry: &WebRenderTreeEntry) {
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
// End-to-end WebAppsBridge tests (in-process AppsServer)
// ---------------------------------------------------------------------------

/// 8. WebAppsBridge connects to a live in-process server.
#[tokio::test]
async fn web_apps_bridge_connect_to_local_apps_server_succeeds() {
    let (endpoint, _shutdown) = setup_apps_server().await;
    let bridge = WebAppsBridge::connect(endpoint).await;
    assert!(bridge.is_ok(), "must connect to local server");
}

/// 9. render_package_list empty → List root with zero children.
#[tokio::test]
async fn web_apps_bridge_render_package_list_empty_returns_empty_list_node() {
    let (endpoint, _shutdown) = setup_apps_server().await;
    let mut bridge = WebAppsBridge::connect(endpoint)
        .await
        .expect("connect to server");
    let tree = bridge
        .render_package_list_as_web_tree()
        .await
        .expect("list RPC");
    assert_eq!(tree.root.kind, NodeKind::List);
    assert_eq!(tree.root.dom_tag, "ul");
    assert_eq!(tree.root.label, "Packages");
    assert!(tree.root.children.is_empty());
}

/// 10. Register one package → list returns one Card child.
#[tokio::test]
async fn web_apps_bridge_render_package_list_after_register_returns_one_card_child() {
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
    register_via_client(&mut raw_client, &signing_key, "firefox", "125.0.1").await;

    let mut bridge = WebAppsBridge::connect(ep).await.expect("connect bridge");
    let tree = bridge
        .render_package_list_as_web_tree()
        .await
        .expect("list RPC");
    assert_eq!(tree.root.kind, NodeKind::List);
    assert_eq!(tree.root.dom_tag, "ul");
    assert_eq!(tree.root.children.len(), 1);
    let card = &tree.root.children[0];
    assert_eq!(card.kind, NodeKind::Card);
    assert_eq!(card.dom_tag, "div");
    assert!(card.label.contains("firefox"), "label: {}", card.label);
    assert!(card.label.contains("125.0.1"), "label: {}", card.label);
}

/// 11. render_package_show returns Card with version child.
#[tokio::test]
async fn web_apps_bridge_render_package_show_returns_card_with_version_child() {
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

    let mut bridge = WebAppsBridge::connect(ep).await.expect("connect bridge");
    let tree = bridge
        .render_package_show_as_web_tree(&pkg_id)
        .await
        .expect("show RPC");
    assert_eq!(tree.root.kind, NodeKind::Card);
    assert_eq!(tree.root.dom_tag, "div");
    assert!(tree.root.label.contains("thunderbird"));
    let version_child = &tree.root.children[0];
    assert_eq!(version_child.kind, NodeKind::Text);
    assert_eq!(version_child.dom_tag, "span");
    assert!(
        version_child.label.contains("115.0"),
        "version label: {}",
        version_child.label
    );
}
