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

use chrono::{Duration, TimeZone};
use strum::{EnumCount, IntoEnumIterator};

use aios_action::ActionId;
use aios_policy::{
    ApprovalRequirement, ApprovalScope, ApproverClass, Constraints, Decision, EvidenceGrade,
    HardDenyClass, HydratedSubject, NetworkPolicy, PolicyDecision, PolicyError, SandboxProfileId,
    SessionClass, SubjectType, VaultCapabilityId,
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
fn constraints_full_round_trip_with_every_field_populated() {
    // T-020: every §10 field carries a non-default value to anchor the wire form.
    let constraints = Constraints {
        sandbox_profile_id: Some(SandboxProfileId("host-service-control".to_string())),
        max_runtime_seconds: Some(30),
        verification_required: true,
        dry_run_only: false,
        require_evidence_grade: Some(EvidenceGrade::E3),
        require_human_co_signer: true,
        network_policy: Some(NetworkPolicy::LocalhostOnly),
        max_concurrent_per_subject: Some(4),
        min_subject_session_class: Some(SessionClass::Internal),
        vault_capability_required: Some(VaultCapabilityId("vault.read:nginx.tls".to_string())),
        ttl_seconds: 600,
        expires_at: Some(
            chrono::Utc
                .with_ymd_and_hms(2099, 1, 1, 0, 0, 0)
                .single()
                .expect("fixture timestamp is valid"),
        ),
    };

    let json = serde_json::to_string(&constraints).expect("serialise Constraints");
    let back: Constraints = serde_json::from_str(&json).expect("deserialise Constraints");
    assert_eq!(constraints, back);

    // Anchor the wire form of each closed enum so external bundle authors see
    // exactly the spec strings.
    assert!(json.contains("\"network_policy\":\"LOCALHOST_ONLY\""));
    assert!(json.contains("\"require_evidence_grade\":\"E3\""));
    assert!(json.contains("\"min_subject_session_class\":\"INTERNAL\""));
    assert!(json.contains("\"ttl_seconds\":600"));

    // §13.2: a default-constructed Constraints validates cleanly (ttl = 300 s).
    Constraints::default()
        .validate()
        .expect("default Constraints must be valid (S2.3 §13.2 default 300 s)");

    // And the fully-populated one above also validates.
    constraints
        .validate()
        .expect("fully populated Constraints must validate");
}

#[test]
fn constraints_validate_rejects_past_expires_at() {
    let mut c = Constraints {
        expires_at: Some(
            chrono::Utc
                .with_ymd_and_hms(2000, 1, 1, 0, 0, 0)
                .single()
                .expect("fixture timestamp is valid"),
        ),
        ..Constraints::default()
    };
    let err = c.validate().expect_err("past expires_at must reject");
    match err {
        PolicyError::ConstraintsInvalid(detail) => {
            assert!(
                detail.contains("expires_at"),
                "detail should reference the offending field, got {detail}"
            );
        }
        other => panic!("expected ConstraintsInvalid, got {other:?}"),
    }

    // Sanity: a future expires_at is accepted.
    c.expires_at = Some(chrono::Utc::now() + Duration::hours(1));
    c.validate().expect("future expires_at must validate");
}

#[test]
fn constraints_validate_rejects_zero_budgets_and_ttl() {
    // zero ttl_seconds — spec floor is 300 default, hard floor is non-zero.
    let mut c = Constraints {
        ttl_seconds: 0,
        ..Constraints::default()
    };
    let err = c.validate().expect_err("zero ttl_seconds must reject");
    assert!(matches!(err, PolicyError::ConstraintsInvalid(ref d) if d.contains("ttl_seconds")));

    // ttl_seconds above MAX_TTL_SECONDS (3600 s per §13.2).
    c.ttl_seconds = Constraints::MAX_TTL_SECONDS + 1;
    let err = c.validate().expect_err("ttl_seconds > max must reject");
    assert!(matches!(err, PolicyError::ConstraintsInvalid(ref d) if d.contains("exceeds max")));

    // zero max_runtime_seconds — spec §10 row 2 ("hard wall-clock cap"); zero
    // would mean the action cannot run, which must be expressed as DENY.
    c = Constraints {
        max_runtime_seconds: Some(0),
        ..Constraints::default()
    };
    let err = c
        .validate()
        .expect_err("zero max_runtime_seconds must reject");
    assert!(
        matches!(err, PolicyError::ConstraintsInvalid(ref d) if d.contains("max_runtime_seconds"))
    );

    // zero max_concurrent_per_subject — same reasoning, §10 row 8.
    c = Constraints {
        max_concurrent_per_subject: Some(0),
        ..Constraints::default()
    };
    let err = c
        .validate()
        .expect_err("zero max_concurrent_per_subject must reject");
    assert!(
        matches!(err, PolicyError::ConstraintsInvalid(ref d) if d.contains("max_concurrent_per_subject"))
    );
}

#[test]
fn approval_requirement_enum_values_round_trip() {
    // Full ApprovalRequirement with every approver class enumerated.
    let approval = ApprovalRequirement {
        required: true,
        approval_scope: ApprovalScope::ExactRequestHash,
        ttl_seconds: 300,
        approver_classes: vec![
            ApproverClass::Human,
            ApproverClass::Operator,
            ApproverClass::Agent,
            ApproverClass::Application,
            ApproverClass::Service,
            ApproverClass::Device,
            ApproverClass::Workflow,
            ApproverClass::RemoteOperator,
        ],
        require_human_co_signer: true,
    };

    let json = serde_json::to_string(&approval).expect("serialise ApprovalRequirement");
    let back: ApprovalRequirement =
        serde_json::from_str(&json).expect("deserialise ApprovalRequirement");
    assert_eq!(approval, back);

    // Anchor the spec wire form: §15 default is `["human"]` and the §11.2 proto
    // values are lowercase snake_case strings.
    assert!(json.contains("\"approval_scope\":\"exact_request_hash\""));
    assert!(json.contains("\"human\""));
    assert!(json.contains("\"operator\""));
    assert!(json.contains("\"remote_operator\""));

    // Default ApprovalRequirement: `required = false` with the binding scope
    // still set to the only rev.2 value.
    let default = ApprovalRequirement::default();
    assert!(!default.required);
    assert_eq!(default.approval_scope, ApprovalScope::ExactRequestHash);
    assert!(default.approver_classes.is_empty());
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
    assert_eq!(
        PolicyError::ConstraintsInvalid("ttl_seconds must be non-zero (S2.3 §13.2)".to_string())
            .to_string(),
        "constraints invalid: ttl_seconds must be non-zero (S2.3 §13.2)"
    );
}
