//! Integration tests for the T-026 types-only skeleton.
//!
//! These tests pin:
//!
//! 1. The S10.1 §3.1 fourteen-state invariant.
//! 2. The strict-terminal classification per §4.2.
//! 3. JSON round-trip for every closed enum (`SCREAMING_SNAKE_CASE` wire form).
//! 4. JSON round-trip for [`ActionContext`] and [`AdapterManifest`].
//! 5. Canonical English [`RuntimeError`] `Display` strings.
//! 6. Cross-crate import: importing [`aios_action::ActionEnvelope`] +
//!    [`aios_action::ActionId`] compiles and round-trips alongside our types.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::no_effect_underscore_binding,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use chrono::{TimeZone, Utc};
use strum::{EnumCount, IntoEnumIterator};

use aios_action::{ActionEnvelope, ActionId};
use aios_capability_runtime::adapter_manifest::AdapterActionDeclaration;
use aios_capability_runtime::{
    ActionContext, ActionDispatchKind, ActionLifecycleState, AdapterIOMode, AdapterManifest,
    AdapterStability, ExecutionFailureReason, QueueClass, RollbackOutcome, RuntimeError,
    RuntimeErrorCode,
};

// ---------------------------------------------------------------------------
// §3.1 — 14-state FSM invariant
// ---------------------------------------------------------------------------

#[test]
fn action_lifecycle_state_has_exactly_fourteen_variants() {
    // S10.1 §3.1 explicitly: "Closed enum, fourteen states." Bump this assertion
    // if and only if the spec adds a row to the §3.1 table.
    assert_eq!(ActionLifecycleState::COUNT, 14);
    assert_eq!(ActionLifecycleState::iter().count(), 14);
}

#[test]
fn action_lifecycle_state_terminal_classification_matches_spec() {
    // §4.2 forbidden-transition table: strict terminals are SUCCEEDED,
    // ROLLED_BACK, ROLLBACK_FAILED, OVERRIDE_DENIED.
    //
    // POLICY_DENIED is *not* a strict terminal: T21 allows POLICY_DENIED ->
    // OVERRIDE_PENDING after an operator-authored override request.
    // FAILED is *not* a strict terminal: T19/T20 allow FAILED -> ROLLED_BACK
    // or FAILED -> ROLLBACK_FAILED.
    let strict_terminals = [
        ActionLifecycleState::Succeeded,
        ActionLifecycleState::RolledBack,
        ActionLifecycleState::RollbackFailed,
        ActionLifecycleState::OverrideDenied,
    ];
    for s in ActionLifecycleState::iter() {
        let expected = strict_terminals.contains(&s);
        assert_eq!(
            s.is_terminal(),
            expected,
            "{s:?}.is_terminal() should be {expected}",
        );
    }
    // Four strict terminals.
    assert_eq!(
        ActionLifecycleState::iter()
            .filter(ActionLifecycleState::is_terminal)
            .count(),
        4,
    );
}

#[test]
fn action_lifecycle_state_roundtrip_screaming_snake_case() {
    // Spot-check the wire form for two states then assert round-trip for all 14.
    let created = serde_json::to_string(&ActionLifecycleState::Created).expect("ser ok");
    assert_eq!(created, "\"CREATED\"");
    let rb_failed = serde_json::to_string(&ActionLifecycleState::RollbackFailed).expect("ser ok");
    assert_eq!(rb_failed, "\"ROLLBACK_FAILED\"");

    for s in ActionLifecycleState::iter() {
        let json = serde_json::to_string(&s).expect("ser ok");
        let back: ActionLifecycleState = serde_json::from_str(&json).expect("de ok");
        assert_eq!(back, s);
    }
}

// ---------------------------------------------------------------------------
// §3.2 / §3.3 / §3.4 / §3.5 — dispatch-shape enums round-trip
// ---------------------------------------------------------------------------

#[test]
fn action_dispatch_kind_roundtrip() {
    assert_eq!(ActionDispatchKind::COUNT, 4);
    for k in ActionDispatchKind::iter() {
        let json = serde_json::to_string(&k).expect("ser ok");
        let back: ActionDispatchKind = serde_json::from_str(&json).expect("de ok");
        assert_eq!(back, k);
    }
    // Pin the wire form explicitly so a future rename_all drift is caught.
    assert_eq!(
        serde_json::to_string(&ActionDispatchKind::IsolatedSandbox).expect("ser ok"),
        "\"ISOLATED_SANDBOX\"",
    );
    assert_eq!(
        serde_json::to_string(&ActionDispatchKind::DryRun).expect("ser ok"),
        "\"DRY_RUN\"",
    );
}

#[test]
fn adapter_io_mode_roundtrip() {
    assert_eq!(AdapterIOMode::COUNT, 2);
    for m in AdapterIOMode::iter() {
        let json = serde_json::to_string(&m).expect("ser ok");
        let back: AdapterIOMode = serde_json::from_str(&json).expect("de ok");
        assert_eq!(back, m);
    }
    assert_eq!(
        serde_json::to_string(&AdapterIOMode::TypedParametersOnly).expect("ser ok"),
        "\"TYPED_PARAMETERS_ONLY\"",
    );
}

#[test]
fn adapter_stability_roundtrip() {
    assert_eq!(AdapterStability::COUNT, 5);
    for s in AdapterStability::iter() {
        let json = serde_json::to_string(&s).expect("ser ok");
        let back: AdapterStability = serde_json::from_str(&json).expect("de ok");
        assert_eq!(back, s);
    }
    assert_eq!(
        serde_json::to_string(&AdapterStability::Experimental).expect("ser ok"),
        "\"EXPERIMENTAL\"",
    );
}

#[test]
fn queue_class_roundtrip() {
    assert_eq!(QueueClass::COUNT, 4);
    for q in QueueClass::iter() {
        let json = serde_json::to_string(&q).expect("ser ok");
        let back: QueueClass = serde_json::from_str(&json).expect("de ok");
        assert_eq!(back, q);
    }
    assert_eq!(
        serde_json::to_string(&QueueClass::RecoveryPriority).expect("ser ok"),
        "\"RECOVERY_PRIORITY\"",
    );
    assert_eq!(
        serde_json::to_string(&QueueClass::AgentProposal).expect("ser ok"),
        "\"AGENT_PROPOSAL\"",
    );
}

// ---------------------------------------------------------------------------
// §3.6 / §3.7 / §3.8 — failure-shape enums round-trip
// ---------------------------------------------------------------------------

#[test]
fn execution_failure_reason_roundtrip() {
    assert_eq!(ExecutionFailureReason::COUNT, 12);
    for r in ExecutionFailureReason::iter() {
        let json = serde_json::to_string(&r).expect("ser ok");
        let back: ExecutionFailureReason = serde_json::from_str(&json).expect("de ok");
        assert_eq!(back, r);
    }
    assert_eq!(
        serde_json::to_string(&ExecutionFailureReason::IdempotencyKeyReplayDetected)
            .expect("ser ok"),
        "\"IDEMPOTENCY_KEY_REPLAY_DETECTED\"",
    );
}

#[test]
fn rollback_outcome_roundtrip() {
    assert_eq!(RollbackOutcome::COUNT, 4);
    for o in RollbackOutcome::iter() {
        let json = serde_json::to_string(&o).expect("ser ok");
        let back: RollbackOutcome = serde_json::from_str(&json).expect("de ok");
        assert_eq!(back, o);
    }
    assert_eq!(
        serde_json::to_string(&RollbackOutcome::NotAttempted).expect("ser ok"),
        "\"NOT_ATTEMPTED\"",
    );
    assert_eq!(
        serde_json::to_string(&RollbackOutcome::NotApplicable).expect("ser ok"),
        "\"NOT_APPLICABLE\"",
    );
}

#[test]
fn runtime_error_code_roundtrip() {
    // §3.8 explicitly enumerates twenty values.
    assert_eq!(RuntimeErrorCode::COUNT, 20);
    for c in RuntimeErrorCode::iter() {
        let json = serde_json::to_string(&c).expect("ser ok");
        let back: RuntimeErrorCode = serde_json::from_str(&json).expect("de ok");
        assert_eq!(back, c);
    }
    assert_eq!(
        serde_json::to_string(&RuntimeErrorCode::ManifestSignatureInvalid).expect("ser ok"),
        "\"MANIFEST_SIGNATURE_INVALID\"",
    );
    assert_eq!(
        serde_json::to_string(&RuntimeErrorCode::RuntimeOk).expect("ser ok"),
        "\"RUNTIME_OK\"",
    );
}

// ---------------------------------------------------------------------------
// AdapterManifest + ActionContext round-trip
// ---------------------------------------------------------------------------

fn sample_manifest() -> AdapterManifest {
    AdapterManifest {
        adapter_id: "adapter:aios:pkg-dnf:1.2.0".to_string(),
        adapter_version: "1.2.0".to_string(),
        vendor: "aios".to_string(),
        name: "pkg-dnf".to_string(),
        declared_stability: AdapterStability::Stable,
        io_mode: AdapterIOMode::TemplateParameters,
        dispatch_kind: ActionDispatchKind::SubprocessFork,
        declared_actions: vec![AdapterActionDeclaration {
            action_kind: "pkg.install".to_string(),
            target_schema: serde_json::json!({"type": "object"}),
            response_schema: serde_json::json!({"type": "object"}),
            rollback_strategy: "INVERSE_ACTION".to_string(),
            timeout_seconds: 120,
            template_string: Some("dnf install -y ${pkg_name}".to_string()),
            template_substitution_variables: vec!["pkg_name".to_string()],
        }],
        declared_invariants_supported: vec!["INV-013".to_string(), "INV-021".to_string()],
        default_adapter_timeout_seconds: 60,
        default_sandbox_profile_id: "sandbox:pkg".to_string(),
        adapter_signature: "deadbeef".repeat(16),
        signing_key_id: "key:publisher:aios-root".to_string(),
        manifest_created_at: Utc.with_ymd_and_hms(2026, 5, 24, 12, 0, 0).unwrap(),
        manifest_expires_at: Utc.with_ymd_and_hms(2027, 5, 24, 12, 0, 0).unwrap(),
    }
}

#[test]
fn adapter_manifest_roundtrip_json() {
    let m = sample_manifest();
    let json = serde_json::to_string(&m).expect("ser ok");
    let back: AdapterManifest = serde_json::from_str(&json).expect("de ok");
    assert_eq!(back, m);
    // deny_unknown_fields contract: introducing a bogus field must reject.
    let mut parsed: serde_json::Value = serde_json::from_str(&json).expect("re-parse ok");
    parsed["wandering_field"] = serde_json::json!("noise");
    let re = serde_json::to_string(&parsed).expect("re-ser ok");
    let err = serde_json::from_str::<AdapterManifest>(&re);
    assert!(err.is_err(), "deny_unknown_fields must reject extra keys");
}

#[test]
fn action_context_roundtrip_json() {
    let id = ActionId::new();
    let now = Utc.with_ymd_and_hms(2026, 5, 24, 13, 0, 0).unwrap();
    let ctx = ActionContext::new(
        id.clone(),
        ActionDispatchKind::IsolatedSandbox,
        QueueClass::AgentProposal,
        now,
    );

    let json = serde_json::to_string(&ctx).expect("ser ok");
    let back: ActionContext = serde_json::from_str(&json).expect("de ok");
    assert_eq!(back, ctx);

    // Fresh context invariants per ActionContext::new contract.
    assert_eq!(back.status, ActionLifecycleState::Created);
    assert_eq!(back.action_id, id);
    assert_eq!(back.created_at, back.last_updated_at);
    assert!(back.error.is_none());
    assert!(back.rollback_outcome.is_none());
    assert!(back.evidence_chain.is_empty());
}

// ---------------------------------------------------------------------------
// RuntimeError canonical English display strings
// ---------------------------------------------------------------------------

#[test]
fn runtime_error_display_strings_are_canonical_english() {
    let id = ActionId::parse("act_01HN9MN8ZRZTYG9V2QXRG7M3VK").expect("known-good id");

    assert_eq!(
        RuntimeError::ActionNotFound(id.clone()).to_string(),
        format!("action not found: {id}"),
    );
    assert_eq!(
        RuntimeError::InvalidTransition {
            from: ActionLifecycleState::Created,
            to: ActionLifecycleState::Succeeded,
        }
        .to_string(),
        "illegal lifecycle transition: Created -> Succeeded",
    );
    assert_eq!(
        RuntimeError::AdapterUnknown("adapter:aios:pkg-dnf:1.2.0".to_string()).to_string(),
        "unknown adapter: adapter:aios:pkg-dnf:1.2.0",
    );
    assert_eq!(
        RuntimeError::AdapterSignatureInvalid.to_string(),
        "adapter manifest signature invalid",
    );
    assert_eq!(
        RuntimeError::ManifestInvalid("expired".to_string()).to_string(),
        "adapter manifest invalid: expired",
    );
    assert_eq!(
        RuntimeError::Internal("clock rewind detected".to_string()).to_string(),
        "runtime internal error: clock rewind detected",
    );
}

// ---------------------------------------------------------------------------
// Cross-crate import smoke test
// ---------------------------------------------------------------------------

#[test]
fn cross_crate_import_of_aios_action_compiles_and_roundtrips() {
    // ActionId from aios-action round-trips inside our ActionContext.
    let id = ActionId::new();
    let now = Utc.with_ymd_and_hms(2026, 5, 24, 14, 0, 0).unwrap();
    let ctx = ActionContext::new(
        id.clone(),
        ActionDispatchKind::SubprocessFork,
        QueueClass::Interactive,
        now,
    );
    let json = serde_json::to_string(&ctx).expect("ser ok");
    let back: ActionContext = serde_json::from_str(&json).expect("de ok");
    assert_eq!(back.action_id, id);

    // ActionEnvelope name resolves from aios-action; we don't construct one
    // here (the M1 builder API is separate from this skeleton), but we *do*
    // require the type path to be importable so T-027's pipeline can take
    // `ActionEnvelope` references without re-declaring the type.
    let _phantom: Option<ActionEnvelope> = None;
}
