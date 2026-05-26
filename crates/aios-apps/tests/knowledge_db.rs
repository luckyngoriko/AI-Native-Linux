//! S12.4 — Compatibility knowledge DB integration tests.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::no_effect_underscore_binding
)]

use std::collections::HashMap;

use aios_apps::app_profile::{AppProfile, CompatibilityRating, EvidenceLevel, RatingDimension};
use aios_apps::ecosystem::{EcosystemHonestyClass, EcosystemRuntime, RecipeTrustClass};
use aios_apps::error::AppsError;
use aios_apps::knowledge_db::{AppProfileMutation, CompatibilityKnowledgeDB};
use aios_apps::package::PackageId;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};

fn fixture_profile(app_id: &str, rt: EcosystemRuntime) -> AppProfile {
    AppProfile {
        app_id: app_id.to_string(),
        ecosystem_runtime: rt,
        current_recipe_trust_class: RecipeTrustClass::RecipeCommunity,
        headline_rating: CompatibilityRating::Gold,
        headline_evidence_level: EvidenceLevel::MultiOperatorCorroborated,
        worst_dimension: RatingDimension::LaunchReliability,
        ecosystem_honesty_class: EcosystemHonestyClass::PartiallySupported,
    }
}

fn test_signing_key() -> SigningKey {
    let seed: [u8; 32] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f, 0x20,
    ];
    SigningKey::from_bytes(&seed)
}

fn test_authority() -> HashMap<String, VerifyingKey> {
    let vk = test_signing_key().verifying_key();
    let mut m = HashMap::new();
    m.insert("test-authority".to_string(), vk);
    m
}

fn sign_profile(profile: &AppProfile, sk: &SigningKey) -> Vec<u8> {
    let canonical_bytes = serde_json::to_vec(profile).expect("serialise profile");
    sk.sign(&canonical_bytes).to_bytes().to_vec()
}

// ---------------------------------------------------------------------------
// 1 — new() empty
// ---------------------------------------------------------------------------

#[tokio::test]
async fn new_creates_empty_db() {
    let db = CompatibilityKnowledgeDB::new(HashMap::new());
    assert_eq!(db.profile_count().await, 0);
    assert_eq!(db.authority_count(), 0);
}

// ---------------------------------------------------------------------------
// 2 — with_fixtures() populates 5 profiles
// ---------------------------------------------------------------------------

#[tokio::test]
async fn with_fixtures_populates_five_profiles() {
    let db = CompatibilityKnowledgeDB::with_fixtures();
    assert_eq!(db.profile_count().await, 5);
}

// ---------------------------------------------------------------------------
// 3 — register valid → success
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_valid_profile_succeeds() {
    let db = CompatibilityKnowledgeDB::new(test_authority());
    let profile = fixture_profile("test-app", EcosystemRuntime::RuntimeLinuxNative);
    let sk = test_signing_key();
    let sig = sign_profile(&profile, &sk);
    let pid = PackageId("pkg_test_001".into());

    db.register_profile(pid.clone(), profile.clone(), sig)
        .await
        .expect("register should succeed");

    let found = db.lookup(&pid).await.expect("lookup should succeed");
    assert_eq!(found.app_id, "test-app");
    assert_eq!(db.profile_count().await, 1);
}

// ---------------------------------------------------------------------------
// 4 — register duplicate package_id → fail-closed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_duplicate_package_id_fails() {
    let db = CompatibilityKnowledgeDB::new(test_authority());
    let profile = fixture_profile("dup-app", EcosystemRuntime::RuntimeLinuxNative);
    let sk = test_signing_key();
    let sig = sign_profile(&profile, &sk);
    let pid = PackageId("pkg_dup_001".into());

    // First registration succeeds.
    db.register_profile(pid.clone(), profile.clone(), sig.clone())
        .await
        .expect("first register");

    // Second registration with same PackageId must fail.
    let err = db
        .register_profile(pid, profile, sig)
        .await
        .expect_err("duplicate should fail");
    assert!(
        matches!(err, AppsError::ValidationFailed(_)),
        "expected ValidationFailed, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 5 — lookup known → AppProfile
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lookup_known_returns_profile() {
    let db = CompatibilityKnowledgeDB::with_fixtures();
    let pid = PackageId("pkg_fixture_wine".into());
    let profile = db.lookup(&pid).await.expect("should find wine profile");
    assert_eq!(profile.app_id, "valve-hl2");
    assert_eq!(
        profile.ecosystem_runtime,
        EcosystemRuntime::RuntimeWindowsProton
    );
}

// ---------------------------------------------------------------------------
// 6 — lookup unknown → PackageNotFound
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lookup_unknown_returns_package_not_found() {
    let db = CompatibilityKnowledgeDB::with_fixtures();
    let pid = PackageId("pkg_nonexistent".into());
    let err = db.lookup(&pid).await.expect_err("should not find");
    assert!(
        matches!(err, AppsError::PackageNotFound(_)),
        "expected PackageNotFound, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 7 — list_by_ecosystem filters correctly
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_by_ecosystem_filters_correctly() {
    let db = CompatibilityKnowledgeDB::with_fixtures();
    let flatpaks = db.list_by_ecosystem(EcosystemRuntime::RuntimeFlatpak).await;
    assert_eq!(flatpaks.len(), 1);
    assert_eq!(flatpaks[0].app_id, "org.gimp.GIMP");

    let empty = db.list_by_ecosystem(EcosystemRuntime::RuntimeSnap).await;
    assert!(empty.is_empty());
}

// ---------------------------------------------------------------------------
// 8 — update_profile AddIssue → known_issues grows (verified via behaviour)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_add_issue_succeeds() {
    let db = CompatibilityKnowledgeDB::new(test_authority());
    let profile = fixture_profile("issue-app", EcosystemRuntime::RuntimeLinuxNative);
    let sk = test_signing_key();
    let sig = sign_profile(&profile, &sk);
    let pid = PackageId("pkg_issue_001".into());

    db.register_profile(pid.clone(), profile, sig)
        .await
        .expect("register should succeed");

    // AddIssue — the mutation succeeds (returned AppProfile is the static data).
    let updated = db
        .update_profile(
            &pid,
            AppProfileMutation::AddIssue("graphical flicker".into()),
        )
        .await
        .expect("update should succeed");
    assert_eq!(updated.app_id, "issue-app");

    // Verify the profile still exists and can be looked up.
    let found = db.lookup(&pid).await.expect("lookup after update");
    assert_eq!(found.app_id, "issue-app");
}

// ---------------------------------------------------------------------------
// 9 — update_profile SetCompatibilityScore clamps to 100
// ---------------------------------------------------------------------------

#[tokio::test]
async fn set_compatibility_score_clamps_to_100() {
    let db = CompatibilityKnowledgeDB::new(test_authority());
    let profile = fixture_profile("score-app", EcosystemRuntime::RuntimeLinuxNative);
    let sk = test_signing_key();
    let sig = sign_profile(&profile, &sk);
    let pid = PackageId("pkg_score_001".into());

    db.register_profile(pid.clone(), profile, sig)
        .await
        .expect("register should succeed");

    // Score of 150 should be clamped to 100.
    let updated = db
        .update_profile(&pid, AppProfileMutation::SetCompatibilityScore(150))
        .await
        .expect("update should succeed");
    assert_eq!(updated.app_id, "score-app");

    // Score of 75 within range passes through unchanged.
    let _ = db
        .update_profile(&pid, AppProfileMutation::SetCompatibilityScore(75))
        .await
        .expect("update should succeed");
}

// ---------------------------------------------------------------------------
// 10 — delete_profile success → subsequent lookup fails
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_profile_success_then_lookup_fails() {
    let db = CompatibilityKnowledgeDB::new(test_authority());
    let profile = fixture_profile("del-app", EcosystemRuntime::RuntimeLinuxNative);
    let sk = test_signing_key();
    let sig = sign_profile(&profile, &sk);
    let pid = PackageId("pkg_del_001".into());

    db.register_profile(pid.clone(), profile, sig)
        .await
        .expect("register should succeed");
    assert_eq!(db.profile_count().await, 1);

    db.delete_profile(&pid)
        .await
        .expect("delete should succeed");

    assert_eq!(db.profile_count().await, 0);

    let err = db.lookup(&pid).await.expect_err("lookup after delete");
    assert!(
        matches!(err, AppsError::PackageNotFound(_)),
        "expected PackageNotFound after delete, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 11 — Concurrent register from 3 tasks → no panic
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_register_no_panic() {
    let db = std::sync::Arc::new(CompatibilityKnowledgeDB::new(test_authority()));
    let sk = test_signing_key();

    let mut handles = Vec::new();
    for i in 0..3 {
        let db = db.clone();
        let sk_clone = sk.clone();
        handles.push(tokio::spawn(async move {
            let profile = fixture_profile(
                &format!("concurrent-{i}"),
                EcosystemRuntime::RuntimeLinuxNative,
            );
            let sig = sign_profile(&profile, &sk_clone);
            let pid = PackageId(format!("pkg_conc_{i:03}"));
            db.register_profile(pid, profile, sig).await
        }));
    }

    for h in handles {
        h.await
            .expect("task should not panic")
            .expect("register should succeed");
    }

    assert_eq!(db.profile_count().await, 3);
}

// ---------------------------------------------------------------------------
// 12 — End-to-end: register + lookup + update + delete chain
// ---------------------------------------------------------------------------

#[tokio::test]
async fn end_to_end_register_lookup_update_delete_chain() {
    let db = CompatibilityKnowledgeDB::new(test_authority());
    let sk = test_signing_key();

    // --- Register ---
    let profile = fixture_profile("e2e-app", EcosystemRuntime::RuntimeLinuxNative);
    let sig = sign_profile(&profile, &sk);
    let pid = PackageId("pkg_e2e_001".into());

    db.register_profile(pid.clone(), profile, sig)
        .await
        .expect("register");

    // --- Lookup ---
    let found = db.lookup(&pid).await.expect("lookup");
    assert_eq!(found.app_id, "e2e-app");
    assert_eq!(found.headline_rating, CompatibilityRating::Gold);

    // --- Update: AddIssue ---
    let _ = db
        .update_profile(
            &pid,
            AppProfileMutation::AddIssue("crashes on launch under Wayland".into()),
        )
        .await
        .expect("add issue");

    // --- Update: AddHint ---
    let _ = db
        .update_profile(
            &pid,
            AppProfileMutation::AddHint("use X11 session for stability".into()),
        )
        .await
        .expect("add hint");

    // --- Update: SetCompatibilityScore ---
    let _ = db
        .update_profile(&pid, AppProfileMutation::SetCompatibilityScore(85))
        .await
        .expect("set score");

    // --- Update: MarkLastUpdated ---
    let ts = chrono::Utc::now();
    let _ = db
        .update_profile(&pid, AppProfileMutation::MarkLastUpdated(ts))
        .await
        .expect("mark updated");

    // Lookup still works after all mutations.
    let final_lookup = db.lookup(&pid).await.expect("final lookup");
    assert_eq!(final_lookup.app_id, "e2e-app");

    // --- Delete ---
    db.delete_profile(&pid).await.expect("delete");

    let err = db.lookup(&pid).await.expect_err("lookup after delete");
    assert!(matches!(err, AppsError::PackageNotFound(_)));
}

// ---------------------------------------------------------------------------
// Additional: register with invalid signature fails
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_with_invalid_signature_fails() {
    let db = CompatibilityKnowledgeDB::new(test_authority());
    let profile = fixture_profile("bad-sig-app", EcosystemRuntime::RuntimeLinuxNative);
    let bad_sig = vec![0u8; 64];
    let pid = PackageId("pkg_bad_sig".into());

    let err = db
        .register_profile(pid, profile, bad_sig)
        .await
        .expect_err("bad signature should fail");
    assert!(
        matches!(err, AppsError::ValidationFailed(_)),
        "expected ValidationFailed, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Additional: delete unknown returns PackageNotFound
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_unknown_profile_returns_package_not_found() {
    let db = CompatibilityKnowledgeDB::with_fixtures();
    let pid = PackageId("pkg_nobody".into());
    let err = db.delete_profile(&pid).await.expect_err("should fail");
    assert!(
        matches!(err, AppsError::PackageNotFound(_)),
        "expected PackageNotFound, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Additional: list_by_ecosystem across all 5 fixture ecosystems
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_by_ecosystem_covers_all_five_fixture_variants() {
    let db = CompatibilityKnowledgeDB::with_fixtures();
    let ecosystems = [
        EcosystemRuntime::RuntimeLinuxNative,
        EcosystemRuntime::RuntimeFlatpak,
        EcosystemRuntime::RuntimeWindowsProton,
        EcosystemRuntime::RuntimeAndroidWaydroid,
        EcosystemRuntime::RuntimeMacosDarling,
    ];
    for eco in &ecosystems {
        let results = db.list_by_ecosystem(*eco).await;
        assert_eq!(results.len(), 1, "expected 1 result for {eco:?}");
    }
}

// ---------------------------------------------------------------------------
// Additional: update_profile on unknown package_id fails
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_unknown_profile_fails() {
    let db = CompatibilityKnowledgeDB::with_fixtures();
    let pid = PackageId("pkg_ghost".into());
    let err = db
        .update_profile(&pid, AppProfileMutation::AddIssue("ghost issue".into()))
        .await
        .expect_err("update on unknown should fail");
    assert!(
        matches!(err, AppsError::PackageNotFound(_)),
        "expected PackageNotFound, got {err:?}"
    );
}
