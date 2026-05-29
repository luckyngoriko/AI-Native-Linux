//! Tier-3 cross-layer primitive coverage (M20 — primitives are now REAL,
//! resolved through an injected `StateProbe`). Without a configured probe they
//! fail closed with a `PROBE_ERROR`; with a `MockStateProbe` they produce real
//! `Passed` / `Failed` verdicts.

use std::error::Error;
use std::sync::Arc;

use aios_action::ActionId;
use aios_verification::primitives::tier3;
use aios_verification::{
    InMemoryVerificationEngine, MockStateProbe, VerificationContext, VerificationEngine,
    VerificationIntent, VerificationPrimitive, VerificationStatus,
};
use chrono::Utc;
use serde_json::{json, Map, Value};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

/// The 22 Tier-3 primitives (kept explicit so the fail-closed sweep does not
/// depend on a strum iterator import).
const TIER3_PRIMITIVES: &[VerificationPrimitive] = &[
    VerificationPrimitive::HttpOk,
    VerificationPrimitive::AiosfsPointer,
    VerificationPrimitive::PolicyDecision,
    VerificationPrimitive::EvidenceExists,
    VerificationPrimitive::NetworkSubjectOutboundClass,
    VerificationPrimitive::NetworkActiveExposureClass,
    VerificationPrimitive::NetworkExternalModelCallBrokeredOnly,
    VerificationPrimitive::DnsResolverBackend,
    VerificationPrimitive::VpnTunnelActive,
    VerificationPrimitive::MdnsPosture,
    VerificationPrimitive::AiosfsPathInNamespace,
    VerificationPrimitive::SurfaceInZone,
    VerificationPrimitive::TreeContainsKind,
    VerificationPrimitive::ThemeSatisfiesInvariants,
    VerificationPrimitive::ThemeConstitutionalIconsIntact,
    VerificationPrimitive::GpuBindingClass,
    VerificationPrimitive::AiosfsPathOwnerResolved,
    VerificationPrimitive::AiosfsPathRecoveryTreatmentSet,
    VerificationPrimitive::StatusIndicatorVisible,
    VerificationPrimitive::FilesystemRootIntact,
    VerificationPrimitive::SpecConsumesTable,
    VerificationPrimitive::ApprovalBindingState,
];

fn expression(primitive: VerificationPrimitive, payload: Value) -> TestResult<String> {
    let mut object = match payload {
        Value::Object(object) => object,
        _ => Map::new(),
    };
    object.insert(
        "primitive".to_owned(),
        Value::String(primitive.as_wire_str().to_owned()),
    );
    Ok(serde_json::to_string(&vec![Value::Object(object)])?)
}

fn intent_with(primitive: VerificationPrimitive, payload: Value) -> TestResult<VerificationIntent> {
    Ok(VerificationIntent::new(
        ActionId::new(),
        expression(primitive, payload)?,
        5,
    ))
}

fn context_for(action_id: ActionId) -> VerificationContext {
    VerificationContext {
        subject: "operator:goriko".to_owned(),
        action_id,
        started_at: Utc::now(),
        timeout_seconds: 5,
        dry_run: true,
    }
}

#[tokio::test]
async fn http_ok_without_state_source_probe_errors() -> TestResult {
    // Default engine uses StdStateProbe → no configured source → fail closed.
    let engine = InMemoryVerificationEngine::new();
    let intent = intent_with(
        VerificationPrimitive::HttpOk,
        json!({"url": "http://127.0.0.1/"}),
    )?;
    let context = context_for(intent.action_id.clone());

    let result = engine.run_verification(&intent, &context).await?;

    assert_eq!(result.status, VerificationStatus::ProbeError);
    assert!(!result.per_primitive[0].passed);
    assert!(result.per_primitive[0]
        .error
        .as_deref()
        .is_some_and(|err| err.contains("state source unavailable")));
    Ok(())
}

#[tokio::test]
async fn dns_resolver_backend_real_verdict_via_state_probe() -> TestResult {
    let probe = MockStateProbe::default().with_observed(
        VerificationPrimitive::DnsResolverBackend,
        "host_local",
        json!({"transport": "DnsOverTls"}),
    );
    let engine = InMemoryVerificationEngine::new().with_state_probe(Arc::new(probe));

    // Observed transport matches expected → PASSED (a real verdict, not deferred).
    let intent = intent_with(
        VerificationPrimitive::DnsResolverBackend,
        json!({"host_id": "host_local", "expected_transport": "DnsOverTls"}),
    )?;
    let context = context_for(intent.action_id.clone());
    let result = engine.run_verification(&intent, &context).await?;
    assert_eq!(result.status, VerificationStatus::Passed);
    assert!(result.per_primitive[0].passed);

    // Observed transport differs → FAILED (ran, mismatch — distinct from probe error).
    let intent = intent_with(
        VerificationPrimitive::DnsResolverBackend,
        json!({"host_id": "host_local", "expected_transport": "PlainDns"}),
    )?;
    let context = context_for(intent.action_id.clone());
    let result = engine.run_verification(&intent, &context).await?;
    assert_eq!(result.status, VerificationStatus::Failed);
    assert!(!result.per_primitive[0].passed);
    Ok(())
}

#[tokio::test]
async fn policy_decision_real_verdict_via_state_probe() -> TestResult {
    let probe = MockStateProbe::default().with_observed(
        VerificationPrimitive::PolicyDecision,
        "pd_001",
        json!({"observed_decision": "DENY"}),
    );
    let engine = InMemoryVerificationEngine::new().with_state_probe(Arc::new(probe));

    let intent = intent_with(
        VerificationPrimitive::PolicyDecision,
        json!({"policy_decision_id": "pd_001", "expected_decision": "DENY"}),
    )?;
    let context = context_for(intent.action_id.clone());
    let result = engine.run_verification(&intent, &context).await?;
    assert_eq!(result.status, VerificationStatus::Passed);
    assert!(result.per_primitive[0].passed);
    Ok(())
}

#[tokio::test]
async fn evidence_exists_real_verdict_via_state_probe() -> TestResult {
    let probe = MockStateProbe::default().with_observed(
        VerificationPrimitive::EvidenceExists,
        "rcpt_42",
        json!({"record_type": "POLICY_DECISION", "segment_id": "seg_1"}),
    );
    let engine = InMemoryVerificationEngine::new().with_state_probe(Arc::new(probe));

    let intent = intent_with(
        VerificationPrimitive::EvidenceExists,
        json!({"receipt_id": "rcpt_42"}),
    )?;
    let context = context_for(intent.action_id.clone());
    let result = engine.run_verification(&intent, &context).await?;
    assert_eq!(result.status, VerificationStatus::Passed);
    assert!(result.per_primitive[0].passed);

    // An unknown receipt id → PROBE_ERROR (source has no such entry), not Failed.
    let intent = intent_with(
        VerificationPrimitive::EvidenceExists,
        json!({"receipt_id": "rcpt_absent"}),
    )?;
    let context = context_for(intent.action_id.clone());
    let result = engine.run_verification(&intent, &context).await?;
    assert_eq!(result.status, VerificationStatus::ProbeError);
    Ok(())
}

#[tokio::test]
async fn all_tier3_primitives_are_handled_and_fail_closed_without_state_source() {
    // An empty probe returns None for every observation; with the lookup key
    // present, each primitive reaches the observe() step and must PROBE_ERROR
    // (fail closed) — never silently pass, never fall through as "not Tier-3".
    let probe = MockStateProbe::default();
    let payload = json!({
        "receipt_id": "x", "policy_decision_id": "x", "object_id": "x", "path": "x",
        "root": "x", "spec_id": "x", "surface_id": "x", "tree_id": "x", "theme_id": "x",
        "indicator_id": "x", "binding_id": "x", "approval_id": "x", "subject_id": "x",
        "host_id": "x", "tunnel_id": "x", "url": "x"
    });
    for primitive in TIER3_PRIMITIVES {
        let result = tier3::execute(*primitive, &payload, &probe).await;
        assert_eq!(result.primitive_kind, *primitive);
        assert!(
            !result.passed,
            "{primitive} must not pass without a configured state source"
        );
        assert!(
            result.error.as_deref().is_some_and(|err| {
                err.contains("PROBE_ERROR") && err.contains("state source unavailable")
            }),
            "{primitive} must PROBE_ERROR (source unavailable) when unconfigured; got {:?}",
            result.error
        );
    }
}
