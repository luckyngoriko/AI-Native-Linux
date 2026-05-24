//! T-025 integration tests for [`aios_policy::OverrideBoundary`] — S2.3 §16.
//!
//! The boundary surface defined by §16 is closed: grant + lookup + revoke +
//! invalidate-on-bundle-flip. Each integration test pins one row of the §16.2 /
//! §16.3 contract so a future refactor that relaxes the human-only,
//! hard-deny, or 24h-cap guard gets caught here.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use chrono::{Duration, Utc};

use aios_policy::subject::SubjectType;
use aios_policy::{
    EmergencyOverride, HardDenyClass, HydratedSubject, OverrideBoundary, OverrideError,
    OverrideRequest, OverrideScope, MAX_OVERRIDE_TTL_SECONDS,
};

fn human(id: &str) -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: id.to_owned(),
        subject_type: SubjectType::Human,
        groups: vec!["operators".to_owned()],
        capabilities: vec!["emergency_override.grant".to_owned()],
        session_class: "INTERNAL".to_owned(),
        recovery_mode: false,
        is_ai: false,
    }
}

fn agent(id: &str) -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: id.to_owned(),
        subject_type: SubjectType::Agent,
        groups: vec!["agents".to_owned()],
        capabilities: vec![],
        session_class: "INTERNAL".to_owned(),
        recovery_mode: false,
        is_ai: true,
    }
}

fn make_request(action: &str) -> OverrideRequest {
    OverrideRequest {
        granted_by_subject: human("human:lucky"),
        scope: OverrideScope {
            rule_id: "deny_lan_exposure".to_owned(),
            action: action.to_owned(),
            subjects: vec![],
        },
        reason: "lan_exposure_incident_response".to_owned(),
        ttl_seconds: 3600,
        attempted_hard_deny: None,
    }
}

// ---------------------------------------------------------------------------
// 1. Grant path — human operator, well-formed scope, mints `ovr_<ULID>`.
// ---------------------------------------------------------------------------

#[test]
fn request_override_mints_receipt_with_expected_shape() {
    let b = OverrideBoundary::new();
    let g: EmergencyOverride = b.request_override(make_request("net.expose_lan")).unwrap();
    assert!(g.override_id.starts_with("ovr_"));
    assert_eq!(g.override_id.len(), 4 + 26); // "ovr_" + 26-char ULID
    assert_eq!(g.granted_by_subject_id, "human:lucky");
    assert!(!g.revoked);
    assert!(g.expires_at > g.granted_at);
    assert_eq!(g.scope.rule_id, "deny_lan_exposure");
    assert_eq!(g.scope.action, "net.expose_lan");
    assert_eq!(b.len(), 1);
}

// ---------------------------------------------------------------------------
// 2. §16.3 authority check — AI subjects cannot grant overrides.
// ---------------------------------------------------------------------------

#[test]
fn request_override_authority_check_rejects_ai_subject() {
    let b = OverrideBoundary::new();
    let mut r = make_request("net.expose_lan");
    r.granted_by_subject = agent("agent:dev");
    let err = b.request_override(r).unwrap_err();
    match err {
        OverrideError::HumanOnly { granted_by } => {
            assert_eq!(granted_by, "agent:dev");
        }
        other => panic!("expected HumanOnly, got {other:?}"),
    }
    assert!(b.is_empty(), "rejected grant must not be recorded");
}

// ---------------------------------------------------------------------------
// 3. §16.2 — overriding a hard-deny class is rejected at grant time.
//    The two §6 rows with recovery-mode override paths use a separate flow.
// ---------------------------------------------------------------------------

#[test]
fn hard_denies_marked_override_path_none_cannot_be_overridden() {
    let b = OverrideBoundary::new();
    // §6 rows with "Override path: None":
    for class in [
        HardDenyClass::SecretRawReadByAi,
        HardDenyClass::RecursiveDeleteRoot,
        HardDenyClass::PolicyLogMutation,
        HardDenyClass::EvidenceLogMutation,
        HardDenyClass::DisablePolicyKernel,
        HardDenyClass::DisableRecoveryPath,
        HardDenyClass::UntypedShellPrivileged,
        HardDenyClass::PrivacyClassDowngrade,
    ] {
        let mut r = make_request("anything");
        r.attempted_hard_deny = Some(class);
        let err = b.request_override(r).unwrap_err();
        match err {
            OverrideError::HardDenyCannotBeOverridden(c) => assert_eq!(c, class),
            other => panic!("expected HardDenyCannotBeOverridden({class:?}), got {other:?}"),
        }
    }
    // The two recovery-mode-overridable §6 rows are also rejected by THIS
    // boundary — their override flow lives in `05_emergency_override.md`.
    for class in [
        HardDenyClass::ModifyBootChain,
        HardDenyClass::AiosFsPointerRollbackOnRecovery,
    ] {
        let mut r = make_request("anything");
        r.attempted_hard_deny = Some(class);
        assert!(b.request_override(r).is_err());
    }
    assert!(b.is_empty());
}

// ---------------------------------------------------------------------------
// 4. Override receipt is recorded + retrievable via is_overridden.
// ---------------------------------------------------------------------------

#[test]
fn override_receipt_is_emitted_and_retrievable_via_is_overridden() {
    let b = OverrideBoundary::new();
    let g = b.request_override(make_request("net.expose_lan")).unwrap();
    let hit = b
        .is_overridden("net.expose_lan", "human:lucky")
        .expect("active grant matching scope must be returned");
    assert_eq!(hit.override_id, g.override_id);
    assert_eq!(hit.scope.rule_id, "deny_lan_exposure");
}

// ---------------------------------------------------------------------------
// 5. Multiple grants for the same action: both are stored; lookup returns one.
//    (Spec does not mandate uniqueness; production policy is one-per-incident.)
// ---------------------------------------------------------------------------

#[test]
fn double_override_on_same_action_stores_both_grants() {
    let b = OverrideBoundary::new();
    let g1 = b.request_override(make_request("net.expose_lan")).unwrap();
    let g2 = b.request_override(make_request("net.expose_lan")).unwrap();
    assert_ne!(g1.override_id, g2.override_id);
    assert_eq!(b.len(), 2);
    // is_overridden returns *some* active grant; both ids are valid.
    let hit = b.is_overridden("net.expose_lan", "human:lucky").unwrap();
    assert!(hit.override_id == g1.override_id || hit.override_id == g2.override_id);
}

// ---------------------------------------------------------------------------
// 6. Expired grants are ignored.
// ---------------------------------------------------------------------------

#[test]
fn expired_override_is_ignored_by_is_overridden() {
    let b = OverrideBoundary::new();
    let now = Utc::now();
    let mut r = make_request("net.expose_lan");
    r.ttl_seconds = 1;
    let g = b.request_override_at(r, now).unwrap();
    // Within TTL: visible.
    assert!(b
        .is_overridden_at("net.expose_lan", "human:lucky", now)
        .is_some());
    // After expiry: invisible.
    let later = now + Duration::seconds(2);
    assert!(b
        .is_overridden_at("net.expose_lan", "human:lucky", later)
        .is_none());
    // The grant is still in the boundary (not yet pruned), just expired.
    assert_eq!(b.len(), 1);
    let pruned = b.prune_expired(later);
    assert_eq!(pruned, 1);
    assert!(b.is_empty());
    let _ = g;
}

// ---------------------------------------------------------------------------
// 7. Revoked grants are ignored.
// ---------------------------------------------------------------------------

#[test]
fn revoked_override_is_ignored_by_is_overridden() {
    let b = OverrideBoundary::new();
    let g = b.request_override(make_request("net.expose_lan")).unwrap();
    assert!(b.revoke(&g.override_id));
    assert!(b.is_overridden("net.expose_lan", "human:lucky").is_none());
    // Double-revoke is a no-op.
    assert!(!b.revoke(&g.override_id));
}

// ---------------------------------------------------------------------------
// 8. TTL = 24h is accepted; 24h + 1s is rejected.
// ---------------------------------------------------------------------------

#[test]
fn ttl_boundary_accepts_24h_rejects_24h_plus_one() {
    let b = OverrideBoundary::new();
    // Exactly at the cap.
    let mut r = make_request("net.expose_lan");
    r.ttl_seconds = MAX_OVERRIDE_TTL_SECONDS;
    assert!(b.request_override(r).is_ok());
    // One second past the cap.
    let mut r = make_request("net.expose_lan");
    r.ttl_seconds = MAX_OVERRIDE_TTL_SECONDS + 1;
    matches!(
        b.request_override(r).unwrap_err(),
        OverrideError::TtlExceeded { .. }
    )
    .then_some(())
    .expect("expected TtlExceeded");
}

// ---------------------------------------------------------------------------
// 9. Bundle flip invalidates active grants (§16.3 non-persistence rule).
// ---------------------------------------------------------------------------

#[test]
fn invalidate_for_bundle_flip_drops_every_active_grant() {
    let b = OverrideBoundary::new();
    let _ = b.request_override(make_request("net.expose_lan")).unwrap();
    let _ = b.request_override(make_request("svc.restart")).unwrap();
    assert_eq!(b.len(), 2);
    let n = b.invalidate_for_bundle_flip();
    assert_eq!(n, 2);
    assert!(b.is_empty());
}

// ---------------------------------------------------------------------------
// 10. Scope with empty action is rejected (§16.3 scoping invariant).
// ---------------------------------------------------------------------------

#[test]
fn empty_action_scope_is_rejected_as_invalid() {
    let b = OverrideBoundary::new();
    let r = make_request("");
    assert_eq!(
        b.request_override(r).unwrap_err(),
        OverrideError::ScopeInvalid
    );
}

// ---------------------------------------------------------------------------
// 11. Zero TTL is rejected (would mint an already-expired grant).
// ---------------------------------------------------------------------------

#[test]
fn zero_ttl_is_rejected() {
    let b = OverrideBoundary::new();
    let mut r = make_request("net.expose_lan");
    r.ttl_seconds = 0;
    assert_eq!(b.request_override(r).unwrap_err(), OverrideError::TtlZero);
}
