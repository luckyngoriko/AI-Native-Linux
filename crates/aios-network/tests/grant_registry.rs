//! Integration tests for `OutboundGrantRegistry` covering INV I7 + INV I8.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::redundant_clone,
    missing_docs,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::Utc;

use aios_network::{
    fingerprint_from_vk, generate_keypair, sign_grant, AllowlistEntry, AllowlistEntryKind,
    NetworkPolicyError, OutboundDirectiveKind, OutboundGrant, OutboundGrantRegistry, PortPolicy,
    ProtocolFamily, SubjectId,
};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn test_allowlist_entry(value: &str) -> AllowlistEntry {
    AllowlistEntry {
        kind: AllowlistEntryKind::HostFqdn,
        value: value.into(),
        port_policy: PortPolicy::OperatorAssigned { port: 443 },
        protocol: ProtocolFamily::Tcp,
    }
}

fn unsigned_grant(id: &str, subject: &str, fp: &str) -> OutboundGrant {
    OutboundGrant {
        grant_id: id.into(),
        subject: SubjectId(subject.into()),
        allowlist: vec![test_allowlist_entry("example.com")],
        directive_kind: OutboundDirectiveKind::AllowListOnly,
        issued_at: Utc::now(),
        expires_at: None,
        signer_fingerprint: fp.into(),
        signature: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// 16 integration tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn append_grant_with_valid_signature_succeeds() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut registry = OutboundGrantRegistry::new();
    registry.register_authority(&fp, vk);

    let mut grant = unsigned_grant("g-1", "human:test", &fp);
    sign_grant(&mut grant, &sk);

    let result = registry.append_grant(grant).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn append_grant_with_invalid_signature_returns_grant_signature_invalid() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut registry = OutboundGrantRegistry::new();
    registry.register_authority(&fp, vk);

    let mut grant = unsigned_grant("g-1", "human:test", &fp);
    // Tamper with the message after signing
    sign_grant(&mut grant, &sk);
    grant.subject = SubjectId("human:evil".into()); // changes signed bytes

    let result = registry.append_grant(grant).await;
    match result {
        Err(NetworkPolicyError::GrantSignatureInvalid { grant_id, reason }) => {
            assert_eq!(grant_id, "g-1");
            assert!(reason.contains("ed25519 verify failed"));
        }
        other => panic!("expected GrantSignatureInvalid, got {other:?}"),
    }
}

#[tokio::test]
async fn append_grant_with_unknown_authority_returns_grant_signature_invalid() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    // Do NOT register the authority.
    let registry = OutboundGrantRegistry::new();

    let mut grant = unsigned_grant("g-1", "human:test", &fp);
    sign_grant(&mut grant, &sk);

    let result = registry.append_grant(grant).await;
    match result {
        Err(NetworkPolicyError::GrantSignatureInvalid { grant_id, reason }) => {
            assert_eq!(grant_id, "g-1");
            assert!(reason.contains("unknown authority"));
        }
        other => panic!("expected GrantSignatureInvalid, got {other:?}"),
    }
}

#[tokio::test]
async fn append_grant_creates_new_manifest_for_subject() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut registry = OutboundGrantRegistry::new();
    registry.register_authority(&fp, vk);

    let mut grant = unsigned_grant("g-1", "human:test", &fp);
    sign_grant(&mut grant, &sk);
    registry.append_grant(grant).await.unwrap();

    let manifest = registry.get_manifest(&SubjectId("human:test".into())).await;
    assert!(manifest.is_some());
    assert_eq!(manifest.unwrap().grant_count(), 1);
}

#[tokio::test]
async fn append_grant_to_existing_manifest_appends_not_replaces() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut registry = OutboundGrantRegistry::new();
    registry.register_authority(&fp, vk);

    let mut g1 = unsigned_grant("g-1", "human:test", &fp);
    sign_grant(&mut g1, &sk);
    registry.append_grant(g1).await.unwrap();

    let mut g2 = unsigned_grant("g-2", "human:test", &fp);
    g2.allowlist = vec![test_allowlist_entry("other.com")];
    sign_grant(&mut g2, &sk);
    registry.append_grant(g2).await.unwrap();

    let manifest = registry
        .get_manifest(&SubjectId("human:test".into()))
        .await
        .unwrap();
    assert_eq!(manifest.grant_count(), 2);
}

#[tokio::test]
async fn append_grant_attempting_to_shrink_expires_at_returns_manifest_mutation_forbidden() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut registry = OutboundGrantRegistry::new();
    registry.register_authority(&fp, vk);

    let t0 = Utc::now();
    // First grant: expires far in the future.
    let mut g1 = unsigned_grant("g-1", "human:test", &fp);
    g1.expires_at = Some(t0 + chrono::Duration::days(365));
    sign_grant(&mut g1, &sk);
    registry.append_grant(g1).await.unwrap();

    // Second grant: expires much earlier — shrink attempt (INV I8).
    let mut g2 = unsigned_grant("g-2", "human:test", &fp);
    g2.expires_at = Some(t0 + chrono::Duration::days(1));
    sign_grant(&mut g2, &sk);

    let result = registry.append_grant(g2).await;
    match result {
        Err(NetworkPolicyError::ManifestMutationForbidden(msg)) => {
            assert!(msg.contains("cannot shrink in-place"));
        }
        other => panic!("expected ManifestMutationForbidden, got {other:?}"),
    }
}

#[tokio::test]
async fn revoke_grant_known_id_returns_tombstone() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut registry = OutboundGrantRegistry::new();
    registry.register_authority(&fp, vk);

    let mut grant = unsigned_grant("g-1", "human:test", &fp);
    sign_grant(&mut grant, &sk);
    registry.append_grant(grant).await.unwrap();

    let tombstone = registry
        .revoke_grant("g-1", SubjectId("human:admin".into()), "test revoke")
        .await
        .unwrap();
    assert_eq!(tombstone.revoked_grant_id, "g-1");
    assert_eq!(tombstone.reason, "test revoke");
}

#[tokio::test]
async fn revoke_grant_then_effective_allowlist_excludes_that_grant_entries() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut registry = OutboundGrantRegistry::new();
    registry.register_authority(&fp, vk);

    let mut grant = unsigned_grant("g-1", "human:test", &fp);
    sign_grant(&mut grant, &sk);
    registry.append_grant(grant).await.unwrap();

    // Before revoke: allowlist has 1 entry.
    let before = registry
        .effective_allowlist(&SubjectId("human:test".into()))
        .await;
    assert_eq!(before.len(), 1);

    registry
        .revoke_grant("g-1", SubjectId("human:admin".into()), "revoked")
        .await
        .unwrap();

    // After revoke: allowlist is empty (grant tombstoned).
    let after = registry
        .effective_allowlist(&SubjectId("human:test".into()))
        .await;
    assert_eq!(after.len(), 0);
}

#[tokio::test]
async fn effective_allowlist_unions_multiple_grants() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut registry = OutboundGrantRegistry::new();
    registry.register_authority(&fp, vk);

    let mut g1 = unsigned_grant("g-1", "human:test", &fp);
    g1.allowlist = vec![test_allowlist_entry("a.com")];
    sign_grant(&mut g1, &sk);
    registry.append_grant(g1).await.unwrap();

    let mut g2 = unsigned_grant("g-2", "human:test", &fp);
    g2.allowlist = vec![test_allowlist_entry("b.com")];
    sign_grant(&mut g2, &sk);
    registry.append_grant(g2).await.unwrap();

    let entries = registry
        .effective_allowlist(&SubjectId("human:test".into()))
        .await;
    assert_eq!(entries.len(), 2);
}

#[tokio::test]
async fn effective_allowlist_for_unknown_subject_returns_empty() {
    let registry = OutboundGrantRegistry::new();
    let entries = registry
        .effective_allowlist(&SubjectId("nobody".into()))
        .await;
    assert!(entries.is_empty());
}

#[tokio::test]
async fn get_manifest_known_subject_returns_manifest() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut registry = OutboundGrantRegistry::new();
    registry.register_authority(&fp, vk);

    let mut grant = unsigned_grant("g-1", "human:test", &fp);
    sign_grant(&mut grant, &sk);
    registry.append_grant(grant).await.unwrap();

    let manifest = registry.get_manifest(&SubjectId("human:test".into())).await;
    assert!(manifest.is_some());
    assert_eq!(manifest.unwrap().subject, SubjectId("human:test".into()));
}

#[tokio::test]
async fn get_manifest_unknown_subject_returns_none() {
    let registry = OutboundGrantRegistry::new();
    let manifest = registry.get_manifest(&SubjectId("nobody".into())).await;
    assert!(manifest.is_none());
}

#[tokio::test]
async fn list_manifests_after_3_subjects_returns_3() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut registry = OutboundGrantRegistry::new();
    registry.register_authority(&fp, vk);

    for (i, subj) in ["human:a", "human:b", "human:c"].iter().enumerate() {
        let mut grant = unsigned_grant(&format!("g-{i}"), subj, &fp);
        sign_grant(&mut grant, &sk);
        registry.append_grant(grant).await.unwrap();
    }

    let manifests = registry.list_manifests().await;
    assert_eq!(manifests.len(), 3);
}

#[tokio::test]
async fn list_tombstones_after_2_revocations_returns_2() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut registry = OutboundGrantRegistry::new();
    registry.register_authority(&fp, vk);

    let mut g1 = unsigned_grant("g-1", "human:a", &fp);
    sign_grant(&mut g1, &sk);
    registry.append_grant(g1).await.unwrap();

    let mut g2 = unsigned_grant("g-2", "human:b", &fp);
    sign_grant(&mut g2, &sk);
    registry.append_grant(g2).await.unwrap();

    registry
        .revoke_grant("g-1", SubjectId("human:admin".into()), "r1")
        .await
        .unwrap();
    registry
        .revoke_grant("g-2", SubjectId("human:admin".into()), "r2")
        .await
        .unwrap();

    assert_eq!(registry.list_tombstones().await.len(), 2);
}

#[tokio::test]
async fn concurrent_append_5_grants_no_panic() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut registry = OutboundGrantRegistry::new();
    registry.register_authority(&fp, vk);

    let registry = Arc::new(registry);
    let mut handles: Vec<tokio::task::JoinHandle<Result<(), NetworkPolicyError>>> = Vec::new();
    for i in 0..5 {
        let r = Arc::clone(&registry);
        let sk_c = sk.clone();
        let fp_c = fp.clone();
        handles.push(tokio::spawn(async move {
            let mut grant = unsigned_grant(&format!("g-{i}"), "human:test", &fp_c);
            sign_grant(&mut grant, &sk_c);
            r.append_grant(grant).await
        }));
    }

    for h in handles {
        assert!(h.await.unwrap().is_ok());
    }

    let manifest = registry
        .get_manifest(&SubjectId("human:test".into()))
        .await
        .unwrap();
    assert_eq!(manifest.grant_count(), 5);
}

#[tokio::test]
async fn inv_i7_signature_binds_subject_id_swap_detected() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut registry = OutboundGrantRegistry::new();
    registry.register_authority(&fp, vk);

    // Sign a grant for subject A.
    let mut grant = unsigned_grant("g-swap", "human:subject-a", &fp);
    sign_grant(&mut grant, &sk);

    // Alter the in-memory subject field to B AFTER signing.
    grant.subject = SubjectId("human:subject-b".into());

    let result = registry.append_grant(grant).await;
    match result {
        Err(NetworkPolicyError::GrantSignatureInvalid { grant_id, .. }) => {
            assert_eq!(grant_id, "g-swap");
        }
        other => panic!("expected GrantSignatureInvalid, got {other:?}"),
    }
}
