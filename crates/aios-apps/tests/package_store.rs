#![allow(clippy::expect_used, clippy::panic)]

//! Integration tests for S12.2 `PackageStore` + `VersionChain`.
//!
//! Covers `register`/`lookup`/`list`/`verify_signature`/`compute_content_hash` for
//! `InMemoryPackageStore` and `append`/`current_active`/`rollback_to` for
//! `VersionChain`.

use std::collections::HashMap;

use aios_apps::*;
use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey;
use rand_core::OsRng;

// ============================================================================
// Test helpers
// ============================================================================

fn make_signing_key() -> SigningKey {
    SigningKey::generate(&mut OsRng)
}

fn make_package(
    name: &str,
    version: &str,
    signing_key: &SigningKey,
    manifest_bytes: &[u8],
) -> AppPackage {
    let sig = signing_key.sign(manifest_bytes);
    AppPackage {
        package_id: PackageId(format!(
            "pkg_{}",
            ulid::Ulid::new().to_string().to_lowercase()
        )),
        name: name.into(),
        version: version.into(),
        manifest_bytes: manifest_bytes.to_vec(),
        content_hash_blake3: blake3::hash(manifest_bytes).to_hex().to_string(),
        ed25519_signature: sig.to_bytes().to_vec(),
        signer_public_key: signing_key.verifying_key().to_bytes().to_vec(),
        registered_at: chrono::Utc::now(),
    }
}

fn make_store_with_key(signing_key: &SigningKey) -> InMemoryPackageStore {
    let mut authorities = HashMap::new();
    authorities.insert(
        signing_key.verifying_key().to_bytes().to_vec(),
        "test-authority".into(),
    );
    InMemoryPackageStore::new(authorities)
}

// ============================================================================
// PackageStore: register valid
// ============================================================================

#[tokio::test]
async fn test_59_register_valid_package_succeeds() {
    let sk = make_signing_key();
    let store = make_store_with_key(&sk);
    let pkg = make_package("firefox", "1.0.0", &sk, b"manifest-content");

    let result = store.register_package(pkg).await;
    let id = result.expect("register should succeed");
    assert!(id.0.starts_with("pkg_"));
    assert_eq!(store.package_count().await, 1);
}

// ============================================================================
// PackageStore: register bad signature
// ============================================================================

#[tokio::test]
async fn test_60_register_bad_signature_rejected() {
    let sk = make_signing_key();
    let store = make_store_with_key(&sk);
    let mut pkg = make_package("firefox", "1.0.0", &sk, b"manifest-content");

    // Corrupt the signature
    if !pkg.ed25519_signature.is_empty() {
        pkg.ed25519_signature[0] ^= 0xff;
    }

    let result = store.register_package(pkg).await;
    match &result {
        Err(e) => assert!(
            e.to_string().contains("signature invalid"),
            "expected signature invalid, got: {e}"
        ),
        Ok(_pkg) => panic!("expected error, got Ok"),
    }
}

// ============================================================================
// PackageStore: register unknown authority
// ============================================================================

#[tokio::test]
async fn test_61_register_unknown_authority_rejected() {
    let sk = make_signing_key();
    // Store with NO trusted authorities
    let store = InMemoryPackageStore::new(HashMap::new());
    let pkg = make_package("firefox", "1.0.0", &sk, b"manifest-content");

    let result = store.register_package(pkg).await;
    match &result {
        Err(e) => assert!(
            e.to_string().contains("unknown authority"),
            "expected unknown authority, got: {e}"
        ),
        Ok(_pkg) => panic!("expected error, got Ok"),
    }
}

// ============================================================================
// PackageStore: register wrong content hash
// ============================================================================

#[tokio::test]
async fn test_62_register_wrong_content_hash_rejected() {
    let sk = make_signing_key();
    let store = make_store_with_key(&sk);
    let mut pkg = make_package("firefox", "1.0.0", &sk, b"manifest-content");

    // Corrupt the content hash
    pkg.content_hash_blake3 =
        "0000000000000000000000000000000000000000000000000000000000000000".into();

    let result = store.register_package(pkg).await;
    match &result {
        Err(e) => assert!(
            e.to_string().contains("hash mismatch"),
            "expected hash mismatch, got: {e}"
        ),
        Ok(_pkg) => panic!("expected error, got Ok"),
    }
}

// ============================================================================
// PackageStore: lookup known
// ============================================================================

#[tokio::test]
async fn test_63_lookup_known_package_returns_app_package() {
    let sk = make_signing_key();
    let store = make_store_with_key(&sk);
    let pkg = make_package("firefox", "1.0.0", &sk, b"manifest-content");
    let id = pkg.package_id.clone();

    store
        .register_package(pkg)
        .await
        .expect("register should succeed");

    let found = store
        .lookup_package(&id)
        .await
        .expect("lookup should succeed");
    assert_eq!(found.package_id, id);
    assert_eq!(found.name, "firefox");
    assert_eq!(found.version, "1.0.0");
}

// ============================================================================
// PackageStore: lookup unknown
// ============================================================================

#[tokio::test]
async fn test_64_lookup_unknown_package_returns_not_found() {
    let sk = make_signing_key();
    let store = make_store_with_key(&sk);
    let fake_id = PackageId("pkg_nonexistent".into());

    let result = store.lookup_package(&fake_id).await;
    assert!(result.is_err());
    match result {
        Err(AppsError::PackageNotFound(_)) => {}
        other => panic!("expected PackageNotFound, got {other:?}"),
    }
}

// ============================================================================
// PackageStore: list_packages
// ============================================================================

#[tokio::test]
async fn test_65_list_packages_returns_all_registered() {
    let sk = make_signing_key();
    let store = make_store_with_key(&sk);

    let pkg1 = make_package("firefox", "1.0.0", &sk, b"m1");
    let pkg2 = make_package("chrome", "2.0.0", &sk, b"m2");

    store.register_package(pkg1).await.expect("register 1");
    store.register_package(pkg2).await.expect("register 2");

    let all = store.list_packages().await.expect("list should succeed");
    assert_eq!(all.len(), 2);
    let names: Vec<&str> = all.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"firefox"));
    assert!(names.contains(&"chrome"));
}

// ============================================================================
// PackageStore: list_versions_of
// ============================================================================

#[tokio::test]
async fn test_66_list_versions_of_returns_ordered_chain() {
    let sk = make_signing_key();
    let store = make_store_with_key(&sk);

    let pkg1 = make_package("firefox", "1.0.0", &sk, b"v1");
    let pkg2 = make_package("firefox", "1.1.0", &sk, b"v2");
    let id1 = pkg1.package_id.clone();
    let id2 = pkg2.package_id.clone();

    store.register_package(pkg1).await.expect("register v1");
    store.register_package(pkg2).await.expect("register v2");

    let versions = store
        .list_versions_of("firefox")
        .await
        .expect("list versions should succeed");
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0], id1);
    assert_eq!(versions[1], id2);
}

// ============================================================================
// VersionChain: append valid chain
// ============================================================================

#[test]
fn test_67_version_chain_append_valid_chain() {
    let mut chain = VersionChain::new();
    let e1 = VersionChainEntry {
        package_id: PackageId("pkg_v1".into()),
        version: "1.0.0".into(),
        registered_at: chrono::Utc::now(),
        parent_version: None,
        state: PackageState::Active,
    };
    chain.append(e1).expect("first append should succeed");

    let e2 = VersionChainEntry {
        package_id: PackageId("pkg_v2".into()),
        version: "1.1.0".into(),
        registered_at: chrono::Utc::now(),
        parent_version: Some("1.0.0".into()),
        state: PackageState::Inactive,
    };
    chain.append(e2).expect("second append should succeed");

    assert_eq!(chain.len(), 2);
    assert_eq!(chain.entries()[0].version, "1.0.0");
    assert_eq!(chain.entries()[1].version, "1.1.0");
}

// ============================================================================
// VersionChain: append wrong parent
// ============================================================================

#[test]
fn test_68_version_chain_append_wrong_parent_rejected() {
    let mut chain = VersionChain::new();
    let e1 = VersionChainEntry {
        package_id: PackageId("pkg_v1".into()),
        version: "1.0.0".into(),
        registered_at: chrono::Utc::now(),
        parent_version: None,
        state: PackageState::Active,
    };
    chain.append(e1).expect("first append");

    let e2 = VersionChainEntry {
        package_id: PackageId("pkg_v2".into()),
        version: "1.1.0".into(),
        registered_at: chrono::Utc::now(),
        parent_version: Some("9.9.9".into()), // Wrong parent
        state: PackageState::Inactive,
    };
    let result = chain.append(e2);
    match &result {
        Err(e) => assert!(e.to_string().contains("parent mismatch")),
        Ok(_pkg) => panic!("expected error, got Ok"),
    }
}

// ============================================================================
// VersionChain: current_active
// ============================================================================

#[test]
fn test_69_current_active_returns_last_active() {
    let mut chain = VersionChain::new();

    chain
        .append(VersionChainEntry {
            package_id: PackageId("pkg_v1".into()),
            version: "1.0.0".into(),
            registered_at: chrono::Utc::now(),
            parent_version: None,
            state: PackageState::Active,
        })
        .expect("append v1");

    chain
        .append(VersionChainEntry {
            package_id: PackageId("pkg_v2".into()),
            version: "1.1.0".into(),
            registered_at: chrono::Utc::now(),
            parent_version: Some("1.0.0".into()),
            state: PackageState::Inactive,
        })
        .expect("append v2");

    chain
        .append(VersionChainEntry {
            package_id: PackageId("pkg_v3".into()),
            version: "1.2.0".into(),
            registered_at: chrono::Utc::now(),
            parent_version: Some("1.1.0".into()),
            state: PackageState::Active,
        })
        .expect("append v3");

    let active = chain.current_active().expect("should find active entry");
    assert_eq!(active.version, "1.2.0");
}

// ============================================================================
// VersionChain: rollback_to success
// ============================================================================

#[test]
fn test_70_rollback_to_flips_active_state() {
    let mut chain = VersionChain::new();

    chain
        .append(VersionChainEntry {
            package_id: PackageId("pkg_v1".into()),
            version: "1.0.0".into(),
            registered_at: chrono::Utc::now(),
            parent_version: None,
            state: PackageState::Inactive,
        })
        .expect("append v1");

    chain
        .append(VersionChainEntry {
            package_id: PackageId("pkg_v2".into()),
            version: "1.1.0".into(),
            registered_at: chrono::Utc::now(),
            parent_version: Some("1.0.0".into()),
            state: PackageState::Active,
        })
        .expect("append v2");

    chain.rollback_to("1.0.0").expect("rollback should succeed");

    // v1 should now be Active
    assert_eq!(chain.entries()[0].state, PackageState::Active);
    // v2 should be RollbackRequired
    assert_eq!(chain.entries()[1].state, PackageState::RollbackRequired);
    // current_active should be v1
    let active = chain.current_active().expect("should find active");
    assert_eq!(active.version, "1.0.0");
}

// ============================================================================
// VersionChain: rollback_to unknown
// ============================================================================

#[test]
fn test_71_rollback_to_unknown_version_returns_error() {
    let mut chain = VersionChain::new();

    chain
        .append(VersionChainEntry {
            package_id: PackageId("pkg_v1".into()),
            version: "1.0.0".into(),
            registered_at: chrono::Utc::now(),
            parent_version: None,
            state: PackageState::Active,
        })
        .expect("append v1");

    let result = chain.rollback_to("9.9.9");
    match &result {
        Err(e) => assert!(e.to_string().contains("not found")),
        Ok(_pkg) => panic!("expected error, got Ok"),
    }
}

// ============================================================================
// PackageStore: concurrent register from 3 tasks
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_72_concurrent_register_three_tasks_no_panic() {
    let sk = make_signing_key();
    let store = std::sync::Arc::new(make_store_with_key(&sk));

    let mut handles = Vec::new();
    for i in 0..3u8 {
        let _store = store.clone();
        let task_sk = make_signing_key();
        handles.push(tokio::spawn(async move {
            let mut auth = HashMap::new();
            auth.insert(task_sk.verifying_key().to_bytes().to_vec(), "test".into());
            let local_store = InMemoryPackageStore::new(auth);

            let pkg = make_package(
                &format!("app_{i}"),
                "1.0.0",
                &task_sk,
                &format!("manifest_{i}").into_bytes(),
            );
            local_store
                .register_package(pkg)
                .await
                .expect("register should succeed")
        }));
    }

    for handle in handles {
        handle.await.expect("task should not panic");
    }
    // Shared store remains empty (each task used its own local store)
    assert_eq!(store.package_count().await, 0);
}

// ============================================================================
// PackageStore: verify_signature + compute_content_hash unit tests
// ============================================================================

#[tokio::test]
async fn test_73_verify_signature_valid() {
    let sk = make_signing_key();
    let store = make_store_with_key(&sk);
    let pkg = make_package("test", "1.0.0", &sk, b"manifest");

    let valid = store
        .verify_signature(&pkg)
        .await
        .expect("verify should succeed");
    assert!(valid);
}

#[tokio::test]
async fn test_74_compute_content_hash_deterministic() {
    let sk = make_signing_key();
    let store = make_store_with_key(&sk);
    let data = b"deterministic-manifest-content";

    let h1 = store
        .compute_content_hash(data)
        .await
        .expect("hash should succeed");
    let h2 = store
        .compute_content_hash(data)
        .await
        .expect("hash should succeed");

    assert_eq!(h1, h2);
    assert_eq!(h1.len(), 64); // BLAKE3 hex is 64 chars
}

// ============================================================================
// PackageStore: empty list_versions_of
// ============================================================================

#[tokio::test]
async fn test_75_list_versions_of_unknown_name_returns_empty() {
    let sk = make_signing_key();
    let store = make_store_with_key(&sk);

    let versions = store
        .list_versions_of("nonexistent")
        .await
        .expect("should succeed");
    assert!(versions.is_empty());
}

// ============================================================================
// PackageStore: trait object usability
// ============================================================================

#[tokio::test]
async fn test_76_package_store_trait_object_usable() {
    let sk = make_signing_key();
    let store: std::sync::Arc<dyn PackageStore> = std::sync::Arc::new(make_store_with_key(&sk));

    let pkg = make_package("test-app", "1.0.0", &sk, b"manifest");
    let id = store
        .register_package(pkg)
        .await
        .expect("register via trait object");
    assert!(id.0.starts_with("pkg_"));

    let found = store
        .lookup_package(&id)
        .await
        .expect("lookup via trait object");
    assert_eq!(found.name, "test-app");

    let all = store.list_packages().await.expect("list via trait object");
    assert_eq!(all.len(), 1);
}

// ============================================================================
// VersionChain: empty chain current_active returns None
// ============================================================================

#[test]
fn test_77_empty_chain_current_active_is_none() {
    let chain = VersionChain::new();
    assert!(chain.current_active().is_none());
}

// ============================================================================
// VersionChain: empty chain len + is_empty
// ============================================================================

#[test]
fn test_78_empty_chain_len_zero() {
    let chain = VersionChain::new();
    assert_eq!(chain.len(), 0);
    assert!(chain.is_empty());
}

// ============================================================================
// PackageState: wire format
// ============================================================================

#[test]
fn test_79_package_state_wire_format_screaming_snake_case() {
    let json = serde_json::to_string(&PackageState::Active).expect("serialize");
    assert_eq!(json, r#""ACTIVE""#);

    let json = serde_json::to_string(&PackageState::Inactive).expect("serialize");
    assert_eq!(json, r#""INACTIVE""#);

    let json = serde_json::to_string(&PackageState::RollbackRequired).expect("serialize");
    assert_eq!(json, r#""ROLLBACK_REQUIRED""#);
}
