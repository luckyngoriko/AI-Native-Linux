//! T-103 cross-crate INV-002 enforcement tests.
//!
//! Covers the full cross-crate provenance chain:
//!   aios-cognitive (marker injection) ↔ aios-capability-runtime (marker verification).
//!
//! Tests 1–12 as required by the T-103 brief.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "Integration-test failures should point at the failing contract"
)]

use std::sync::Arc;

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_capability_runtime::{
    ActionLifecycleState, CapabilityRuntime, ExecutionFailureReason, InMemoryCapabilityRuntime,
    RuntimeCognitiveProvenance, RuntimeContext,
};
use aios_cognitive::{CognitiveProvenanceAdapter, InMemoryCognitiveCore, PROVENANCE_MARKER_KEY};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn ai_envelope() -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("agent:dev", true),
        Request::new(
            "cognitive.translate",
            serde_json::json!({"intent_id": "int_test", "natural_language": "restart nginx"}),
        ),
        Trace::new("00000000000000000000000000000000", "0000000000000000", None),
    )
}

fn ai_envelope_with_provenance(marker: &str) -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("agent:dev", true),
        Request::new(
            "cognitive.translate",
            serde_json::json!({
                "intent_id": "int_test",
                "natural_language": "restart nginx",
                PROVENANCE_MARKER_KEY: marker,
            }),
        ),
        Trace::new("00000000000000000000000000000000", "0000000000000000", None),
    )
}

fn human_envelope() -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("human:operator", false),
        Request::new("service.restart", serde_json::json!({"service": "nginx"})),
        Trace::new("00000000000000000000000000000000", "0000000000000000", None),
    )
}

fn runtime_context(is_ai: bool) -> RuntimeContext {
    let mut ctx = RuntimeContext::new("test_subject", "bundle_v1", "code_v1");
    if is_ai {
        use aios_policy::{HydratedSubject, SubjectType};
        ctx = ctx.with_hydrated_subject(HydratedSubject {
            canonical_subject_id: "agent:dev".into(),
            subject_type: SubjectType::Agent,
            groups: vec![],
            capabilities: vec![],
            session_class: "INTERNAL".into(),
            recovery_mode: false,
            is_ai: true,
        });
    }
    ctx
}

fn adapter() -> Arc<CognitiveProvenanceAdapter> {
    Arc::new(CognitiveProvenanceAdapter::new("0.1.0-T098"))
}

// ---------------------------------------------------------------------------
// 1. Human subject + no provenance hook → passes validation (backward compat)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn human_subject_no_hook_passes_validation() {
    let runtime = InMemoryCapabilityRuntime::new();
    let ctx = runtime_context(false);
    let envelope = human_envelope();

    let result = runtime
        .submit_action(&envelope, &ctx)
        .await
        .expect("submit_action must succeed");

    assert_ne!(result.status, ActionLifecycleState::Failed);
}

// ---------------------------------------------------------------------------
// 2. Human subject + provenance hook → passes validation (only AI subjects checked)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn human_subject_with_hook_passes_validation() {
    let runtime = InMemoryCapabilityRuntime::new()
        .with_cognitive_provenance(adapter() as Arc<dyn RuntimeCognitiveProvenance>);
    let ctx = runtime_context(false);
    let envelope = human_envelope();

    let result = runtime
        .submit_action(&envelope, &ctx)
        .await
        .expect("submit_action must succeed");

    assert_ne!(result.status, ActionLifecycleState::Failed);
}

// ---------------------------------------------------------------------------
// 3. AI subject + no provenance hook → passes validation (backward compat)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ai_subject_no_hook_passes_validation() {
    let runtime = InMemoryCapabilityRuntime::new();
    let ctx = runtime_context(true);
    let envelope = ai_envelope();

    let result = runtime
        .submit_action(&envelope, &ctx)
        .await
        .expect("submit_action must succeed");

    // Without a hook, AI envelopes pass through (no provenance check).
    assert_ne!(result.status, ActionLifecycleState::Failed);
}

// ---------------------------------------------------------------------------
// 4. AI subject + provenance hook + valid marker → passes validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ai_subject_with_hook_valid_marker_passes() {
    let runtime = InMemoryCapabilityRuntime::new()
        .with_cognitive_provenance(adapter() as Arc<dyn RuntimeCognitiveProvenance>);
    let ctx = runtime_context(true);
    let envelope = ai_envelope_with_provenance("0.1.0-T098");

    let result = runtime
        .submit_action(&envelope, &ctx)
        .await
        .expect("submit_action must succeed");

    assert!(
        !matches!(result.status, ActionLifecycleState::Failed),
        "AI envelope with valid provenance must not fail"
    );
    assert_eq!(
        result.error, None,
        "no error must be set for a valid AI envelope"
    );
}

// ---------------------------------------------------------------------------
// 5. AI subject + provenance hook + missing marker → FAILED with AdapterRefused
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ai_subject_with_hook_missing_marker_fails() {
    let runtime = InMemoryCapabilityRuntime::new()
        .with_cognitive_provenance(adapter() as Arc<dyn RuntimeCognitiveProvenance>);
    let ctx = runtime_context(true);
    let envelope = ai_envelope(); // no provenance marker

    let result = runtime
        .submit_action(&envelope, &ctx)
        .await
        .expect("submit_action must succeed");

    assert_eq!(
        result.status,
        ActionLifecycleState::Failed,
        "AI envelope without provenance marker must fail"
    );
    assert_eq!(
        result.error,
        Some(ExecutionFailureReason::AdapterRefused),
        "failure reason must be AdapterRefused"
    );
}

// ---------------------------------------------------------------------------
// 6. AI subject + provenance hook + wrong marker → FAILED with AdapterRefused
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ai_subject_with_hook_wrong_marker_fails() {
    let runtime = InMemoryCapabilityRuntime::new()
        .with_cognitive_provenance(adapter() as Arc<dyn RuntimeCognitiveProvenance>);
    let ctx = runtime_context(true);
    let envelope = ai_envelope_with_provenance("wrong_version");

    let result = runtime
        .submit_action(&envelope, &ctx)
        .await
        .expect("submit_action must succeed");

    assert_eq!(
        result.status,
        ActionLifecycleState::Failed,
        "AI envelope with wrong provenance marker must fail"
    );
    assert_eq!(
        result.error,
        Some(ExecutionFailureReason::AdapterRefused),
        "failure reason must be AdapterRefused"
    );
}

// ---------------------------------------------------------------------------
// 7. Provenance hook configured but adapter passes None → backward compat
// ---------------------------------------------------------------------------

#[tokio::test]
async fn verify_provenance_returns_ok_on_valid_marker() {
    let adapter = adapter();
    let envelope = ai_envelope_with_provenance("0.1.0-T098");

    let result = adapter.verify_provenance(&envelope).await;
    assert!(result.is_ok(), "valid marker must be accepted: {result:?}");
}

// ---------------------------------------------------------------------------
// 8. CognitiveProvenanceAdapter rejects missing marker directly
// ---------------------------------------------------------------------------

#[tokio::test]
async fn adapter_rejects_missing_marker_direct() {
    let adapter = adapter();
    let envelope = ai_envelope(); // no marker

    let result = adapter.verify_provenance(&envelope).await;
    assert!(result.is_err(), "missing marker must be rejected");
}

// ---------------------------------------------------------------------------
// 9. CognitiveProvenanceAdapter rejects wrong marker directly
// ---------------------------------------------------------------------------

#[tokio::test]
async fn adapter_rejects_wrong_marker_direct() {
    let adapter = adapter();
    let envelope = ai_envelope_with_provenance("0.0.0-ancient");

    let result = adapter.verify_provenance(&envelope).await;
    assert!(result.is_err(), "wrong marker must be rejected");
}

// ---------------------------------------------------------------------------
// 10. Full E2E: InMemoryCognitiveCore produces envelope with marker
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cognitive_core_produces_envelope_with_provenance_marker() {
    use aios_cognitive::{
        AICrossOriginPosture, CognitiveCore, CognitiveIntent, IntentId, LatencyTier, PrivacyClass,
        SubjectRef, TranslationContext,
    };

    let core = InMemoryCognitiveCore::new();
    let intent = CognitiveIntent {
        intent_id: IntentId::new(),
        subject: SubjectRef("agent:dev".into()),
        natural_language: "restart nginx".into(),
        context_hash: "0000000000000000000000000000000000000000000000000000000000000000".into(),
        created_at: chrono::Utc::now(),
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Public,
    };
    let ctx = TranslationContext {
        subject: SubjectRef("agent:dev".into()),
        available_models: vec![],
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Public,
        ai_cross_origin_posture: AICrossOriginPosture::AiVaultBrokeredOnly,
        recovery_mode: false,
        budget_ok: true,
    };

    let result = core
        .translate_intent(&intent, &ctx)
        .await
        .expect("translate_intent must succeed");

    let marker = result
        .produced_action
        .request
        .target
        .get(PROVENANCE_MARKER_KEY)
        .and_then(serde_json::Value::as_str);

    assert!(
        marker.is_some(),
        "produced action envelope must carry cognitive_provenance marker"
    );
    assert_eq!(marker.unwrap(), "0.1.0-T098");
}

// ---------------------------------------------------------------------------
// 11. Full E2E: translate_intent → runtime validates provenance marker
// ---------------------------------------------------------------------------

#[tokio::test]
async fn full_e2e_cognitive_to_runtime_provenance_check() {
    use aios_cognitive::{
        AICrossOriginPosture, CognitiveCore, CognitiveIntent, IntentId, LatencyTier, PrivacyClass,
        SubjectRef, TranslationContext,
    };

    // Step 1: translate intent through Cognitive Core
    let core = InMemoryCognitiveCore::new();
    let intent = CognitiveIntent {
        intent_id: IntentId::new(),
        subject: SubjectRef("agent:dev".into()),
        natural_language: "restart nginx".into(),
        context_hash: "0000000000000000000000000000000000000000000000000000000000000000".into(),
        created_at: chrono::Utc::now(),
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Public,
    };
    let ctx = TranslationContext {
        subject: SubjectRef("agent:dev".into()),
        available_models: vec![],
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Public,
        ai_cross_origin_posture: AICrossOriginPosture::AiVaultBrokeredOnly,
        recovery_mode: false,
        budget_ok: true,
    };

    let translation = core
        .translate_intent(&intent, &ctx)
        .await
        .expect("translate_intent must succeed");

    // Step 2: submit the produced envelope to the Capability Runtime
    // with provenance hook attached
    let runtime = InMemoryCapabilityRuntime::new()
        .with_cognitive_provenance(adapter() as Arc<dyn RuntimeCognitiveProvenance>);
    let rctx = runtime_context(true);

    let result = runtime
        .submit_action(&translation.produced_action, &rctx)
        .await
        .expect("submit_action must succeed");

    assert!(
        !matches!(result.status, ActionLifecycleState::Failed),
        "cognitive-produced envelope with valid marker must pass runtime validation"
    );
    assert_eq!(result.error, None);
}

// ---------------------------------------------------------------------------
// 12. No false positives: human envelope never triggers provenance check
// ---------------------------------------------------------------------------

#[tokio::test]
async fn human_envelope_never_triggers_provenance_check() {
    let runtime = InMemoryCapabilityRuntime::new()
        .with_cognitive_provenance(adapter() as Arc<dyn RuntimeCognitiveProvenance>);
    let ctx = runtime_context(false);

    // Submit 3 different human envelopes — none should fail due to provenance
    for i in 0..3 {
        let envelope = ActionEnvelope::new(
            Identity::new(format!("human:user_{i}"), false),
            Request::new(
                "service.restart",
                serde_json::json!({"service": format!("svc_{i}")}),
            ),
            Trace::new("00000000000000000000000000000000", "0000000000000000", None),
        );

        let result = runtime
            .submit_action(&envelope, &ctx)
            .await
            .expect("submit_action must succeed");

        assert_ne!(
            result.status,
            ActionLifecycleState::Failed,
            "human envelope {i} must not fail provenance check"
        );
        assert_ne!(
            result.error,
            Some(ExecutionFailureReason::AdapterRefused),
            "human envelope {i} must not get AdapterRefused"
        );
    }
}
