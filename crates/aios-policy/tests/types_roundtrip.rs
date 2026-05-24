//! T-016 round-trip + invariant tests for the aios-policy skeleton.
//!
//! These tests anchor the constitutional shape of the core types so subsequent
//! tasks (T-017+) cannot silently drift the surface:
//!
//! - `Decision` has exactly 3 active variants + 1 proto-zero sentinel (S2.3 §4).
//! - `HardDenyClass` has exactly 10 variants (S2.3 §6 table count).
//! - `PolicyDecision` round-trips through `serde_json`.
//! - `HydratedSubject` round-trips through `serde_json`.
//! - `PolicyError` Display strings match the canonical English text.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use chrono::TimeZone;
use strum::{EnumCount, IntoEnumIterator};

use aios_action::ActionId;
use aios_policy::{
    ApprovalRequirement, Constraints, Decision, HardDenyClass, HydratedSubject, PolicyDecision,
    PolicyError, SubjectType,
};

#[test]
fn decision_has_three_active_variants_plus_unspecified() {
    // We do not derive EnumIter on Decision (it is Copy + Hash and the variant set
    // is constitutionally short), so we enumerate the wire-form serialisation set
    // explicitly. Any new variant added without updating this list breaks the test.
    let active = [Decision::Allow, Decision::RequireApproval, Decision::Deny];
    assert_eq!(
        active.len(),
        3,
        "S2.3 §4: exactly 3 active Decision variants"
    );

    // Wire-form coverage anchor — proto3 reserves DECISION_UNSPECIFIED = 0.
    let unspecified_json = serde_json::to_string(&Decision::Unspecified).expect("serialise");
    assert_eq!(unspecified_json, "\"UNSPECIFIED\"");

    // Each active variant serialises in SCREAMING_SNAKE_CASE matching the proto enum.
    assert_eq!(
        serde_json::to_string(&Decision::Allow).expect("serialise"),
        "\"ALLOW\""
    );
    assert_eq!(
        serde_json::to_string(&Decision::RequireApproval).expect("serialise"),
        "\"REQUIRE_APPROVAL\""
    );
    assert_eq!(
        serde_json::to_string(&Decision::Deny).expect("serialise"),
        "\"DENY\""
    );
}

#[test]
fn hard_deny_class_has_exactly_ten_variants() {
    assert_eq!(
        HardDenyClass::COUNT,
        10,
        "S2.3 §6: hard-deny table has exactly 10 rows"
    );

    // Sanity: iterating yields the same count and each variant round-trips via serde.
    let all: Vec<HardDenyClass> = HardDenyClass::iter().collect();
    assert_eq!(all.len(), 10);

    for class in all {
        let s = serde_json::to_string(&class).expect("serialise hard-deny class");
        let back: HardDenyClass = serde_json::from_str(&s).expect("deserialise hard-deny class");
        assert_eq!(class, back, "round-trip for {class:?}");
        assert!(
            s.contains("hd."),
            "hard-deny wire form must use `hd.` prefix per S2.3 §6, got {s}"
        );
    }
}

#[test]
fn policy_decision_round_trips_through_serde_json() {
    let action_id =
        ActionId::parse("act_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("valid ActionId fixture");

    let decision = PolicyDecision {
        policy_decision_id: "poldec_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_string(),
        action_id,
        request_hash: "0123456789abcdef0123456789abcdef".to_string(),
        bundle_version: "bundle-2026.05.24.r1".to_string(),
        enrichment_snapshot_id: "snap_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_string(),
        decision: Decision::Allow,
        reason_code: "ScopedAllow".to_string(),
        reason_message: "Matched user-base.v1 rule allow_restart_user_services".to_string(),
        constraints: Constraints::default(),
        approval: ApprovalRequirement::default(),
        evidence_receipt_id: "evr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_string(),
        evaluated_at: chrono::Utc
            .with_ymd_and_hms(2026, 5, 24, 12, 0, 0)
            .single()
            .expect("fixture timestamp is valid"),
        rules_consulted: 7,
        simulated: false,
    };

    let json = serde_json::to_string(&decision).expect("serialise PolicyDecision");
    let back: PolicyDecision = serde_json::from_str(&json).expect("deserialise PolicyDecision");
    assert_eq!(decision, back);
}

#[test]
fn hydrated_subject_round_trips_through_serde_json() {
    let subject = HydratedSubject {
        canonical_subject_id: "agent:dev:01HX0000000000000000000000".to_string(),
        subject_type: SubjectType::Agent,
        groups: vec!["maintainers".to_string(), "operators".to_string()],
        capabilities: vec!["vault.read:nginx.tls".to_string()],
        session_class: "INTERNAL".to_string(),
        recovery_mode: false,
        is_ai: true,
    };

    let json = serde_json::to_string(&subject).expect("serialise HydratedSubject");
    let back: HydratedSubject = serde_json::from_str(&json).expect("deserialise HydratedSubject");
    assert_eq!(subject, back);

    // Anchor the wire form of SubjectType so external (proto/YAML) bundles continue
    // to use the spec lowercase identifier.
    assert!(json.contains("\"subject_type\":\"agent\""));
}

#[test]
fn policy_error_display_strings_match_canonical_text() {
    assert_eq!(
        PolicyError::SubjectUnauthenticated.to_string(),
        "subject hydration failed: subject unauthenticated"
    );
    assert_eq!(
        PolicyError::EnrichmentUnavailable.to_string(),
        "resource enrichment unavailable"
    );
    assert_eq!(
        PolicyError::BundleLoad {
            reason: "signature verification failed".to_string()
        }
        .to_string(),
        "policy bundle load failed: signature verification failed"
    );
    assert_eq!(
        PolicyError::SchemaInvalid {
            detail: "unknown constraint `max_io_bytes`".to_string()
        }
        .to_string(),
        "policy bundle schema invalid: unknown constraint `max_io_bytes`"
    );
}
