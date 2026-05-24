//! T-030 integration tests — Policy Kernel ⇄ Capability Runtime.
//!
//! Anchors:
//! - [`aios_capability_runtime::InMemoryCapabilityRuntime::with_policy_kernel`]
//!   wires an `aios_policy::PolicyKernel` into pipeline step 2 and drives
//!   the S10.1 §4.2 transition table verbatim:
//!     - `Decision::Allow` → T4 (`POLICY_PENDING → APPROVED`).
//!     - `Decision::RequireApproval` → T5
//!       (`POLICY_PENDING → APPROVAL_PENDING`).
//!     - `Decision::Deny` → T6 (`POLICY_PENDING → POLICY_DENIED`).
//!     - `PolicyError::SubjectUnauthenticated` → T6 with
//!       `error = EnvelopeValidationFailed`.
//!     - `PolicyError::BundleVersionMismatch` → `RuntimeError::PolicyEvalFailed`.
//! - The `policy_constraints` slot on `RuntimeContext` is populated after an
//!   Allow / `RequireApproval` result so downstream steps (T-029 dispatcher,
//!   T-035 verify) can read the bound §10 constraints.
//! - The §17 defense-in-depth tripwire counter increments for an AI subject
//!   + Allow without a human approver class.
//! - A runtime without a kernel attached preserves the T-027 stub baseline
//!   (unconditional T4).
//! - A full happy-path submission with kernel attached walks
//!   Validate → PolicyEvaluate(Allow) → Queue → Execute → Verify → Succeeded.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_capability_runtime::{
    ActionLifecycleState, CapabilityRuntime, ExecutionFailureReason, InMemoryCapabilityRuntime,
    RuntimeContext, RuntimeError,
};
use aios_policy::{
    ApprovalRequirement, ApprovalScope, ApproverClass, Constraints, Decision, HydratedSubject,
    PolicyContext, PolicyDecision, PolicyError, PolicyKernel, SandboxProfileId, SubjectType,
};

// ---------------------------------------------------------------------------
// Scripted mock kernel — returns whatever decision/error the test asks for.
// ---------------------------------------------------------------------------

/// Test-only [`PolicyKernel`] that returns a pre-canned `(Decision, …)` or
/// a typed [`PolicyError`]. Keeps the integration tests independent of the
/// `InMemoryPolicyKernel` bundle-loading machinery (the goal is to prove
/// the L3 ⇄ L4 wiring, not re-test L4 itself).
#[derive(Debug)]
struct ScriptedKernel {
    /// `Ok(...)` → return this decision. `Err(...)` → return this error.
    result: std::sync::Mutex<Result<PolicyDecision, PolicyError>>,
}

impl ScriptedKernel {
    fn allow(constraints: Constraints, approval: ApprovalRequirement) -> Arc<Self> {
        Arc::new(Self {
            result: std::sync::Mutex::new(Ok(make_decision(
                Decision::Allow,
                "ScopedAllow",
                constraints,
                approval,
            ))),
        })
    }

    fn require_approval(approver: Vec<ApproverClass>) -> Arc<Self> {
        let approval = ApprovalRequirement {
            required: true,
            approval_scope: ApprovalScope::ExactRequestHash,
            ttl_seconds: 300,
            approver_classes: approver,
            require_human_co_signer: false,
        };
        Arc::new(Self {
            result: std::sync::Mutex::new(Ok(make_decision(
                Decision::RequireApproval,
                "RequireApproval",
                Constraints::default(),
                approval,
            ))),
        })
    }

    fn deny() -> Arc<Self> {
        Arc::new(Self {
            result: std::sync::Mutex::new(Ok(make_decision(
                Decision::Deny,
                "DefaultDeny",
                Constraints::default(),
                ApprovalRequirement::default(),
            ))),
        })
    }

    fn error(err: PolicyError) -> Arc<Self> {
        Arc::new(Self {
            result: std::sync::Mutex::new(Err(err)),
        })
    }
}

#[async_trait]
impl PolicyKernel for ScriptedKernel {
    async fn evaluate_policy(
        &self,
        _envelope: &ActionEnvelope,
        _context: &PolicyContext,
    ) -> Result<PolicyDecision, PolicyError> {
        let guard = self.result.lock().expect("scripted result mutex");
        guard.clone()
    }
}

fn make_decision(
    decision: Decision,
    reason: &str,
    constraints: Constraints,
    approval: ApprovalRequirement,
) -> PolicyDecision {
    use aios_action::ActionId;
    PolicyDecision {
        policy_decision_id: "poldec_test_0000000000000000000000".to_string(),
        action_id: ActionId::new(),
        request_hash: "0".repeat(32),
        bundle_version: "polb_test_v1".to_string(),
        enrichment_snapshot_id: "polb_snap_test".to_string(),
        decision,
        reason_code: reason.to_string(),
        reason_message: format!("test {reason}"),
        constraints,
        approval,
        evidence_receipt_id: "evr_test_0".to_string(),
        evaluated_at: Utc::now(),
        rules_consulted: 0,
        simulated: false,
    }
}

// ---------------------------------------------------------------------------
// Fixtures.
// ---------------------------------------------------------------------------

fn happy_envelope(subject: &str, is_ai: bool) -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new(subject, is_ai),
        Request::new("service.restart", serde_json::json!({"service": "nginx"})),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn human_subject(canonical: &str) -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: canonical.to_string(),
        subject_type: SubjectType::Human,
        groups: vec!["operators".to_string()],
        capabilities: vec!["cap.service.manage".to_string()],
        session_class: "INTERNAL".to_string(),
        recovery_mode: false,
        is_ai: false,
    }
}

fn ai_subject(canonical: &str) -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: canonical.to_string(),
        subject_type: SubjectType::Agent,
        groups: Vec::new(),
        capabilities: Vec::new(),
        session_class: "INTERNAL".to_string(),
        recovery_mode: false,
        is_ai: true,
    }
}

fn constraints_with_sandbox(profile: &str) -> Constraints {
    Constraints {
        sandbox_profile_id: Some(SandboxProfileId(profile.to_string())),
        ..Constraints::default()
    }
}

// ---------------------------------------------------------------------------
// 1. ALLOW → T4 PolicyPending → Approved → … → Succeeded.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn allow_decision_drives_t4_and_reaches_succeeded() {
    let kernel = ScriptedKernel::allow(
        constraints_with_sandbox("host-service-control"),
        ApprovalRequirement::default(),
    );
    let runtime = InMemoryCapabilityRuntime::new().with_policy_kernel(kernel);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000000"),
        "polb_test_v1",
        "code_t030",
    );
    let env = happy_envelope("human:lucky", false);
    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");

    // T-027 stub steps drive the post-approval path to SUCCEEDED.
    assert_eq!(result.status, ActionLifecycleState::Succeeded);
    assert_eq!(result.error, None);
    // Constraints projection: the Allow constraints reached the
    // RuntimeContext.
    let cons = rctx
        .policy_constraints_snapshot()
        .expect("constraints projected after Allow");
    assert_eq!(
        cons.sandbox_profile_id,
        Some(SandboxProfileId("host-service-control".to_string()))
    );
}

// ---------------------------------------------------------------------------
// 2. REQUIRE_APPROVAL → T5 PolicyPending → ApprovalPending (short-circuit).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn require_approval_decision_drives_t5_and_short_circuits() {
    let kernel = ScriptedKernel::require_approval(vec![ApproverClass::Human]);
    let runtime = InMemoryCapabilityRuntime::new().with_policy_kernel(kernel);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000000"),
        "polb_test_v1",
        "code_t030",
    );
    let env = happy_envelope("human:lucky", false);
    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");

    assert_eq!(result.status, ActionLifecycleState::ApprovalPending);
    assert_eq!(result.error, None);
    // Constraints projection still happens for RequireApproval (T-034 will
    // honour them once it lands the binding check).
    assert!(rctx.policy_constraints_snapshot().is_some());
}

// ---------------------------------------------------------------------------
// 3. DENY → T6 PolicyPending → PolicyDenied (terminal).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn deny_decision_drives_t6_to_policy_denied() {
    let kernel = ScriptedKernel::deny();
    let runtime = InMemoryCapabilityRuntime::new().with_policy_kernel(kernel);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000000"),
        "polb_test_v1",
        "code_t030",
    );
    let env = happy_envelope("human:lucky", false);
    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");

    assert_eq!(result.status, ActionLifecycleState::PolicyDenied);
    // Constraints slot cleared on Deny.
    assert!(rctx.policy_constraints_snapshot().is_none());
}

// ---------------------------------------------------------------------------
// 4. Backward compat: no kernel → T-027 stub (unconditional Approved → … → Succeeded).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn no_policy_kernel_preserves_t027_baseline() {
    let runtime = InMemoryCapabilityRuntime::new();
    let rctx = RuntimeContext::new("human:lucky", "polb_v1", "code_v1");
    let env = happy_envelope("human:lucky", false);
    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("baseline submit_action");
    assert_eq!(result.status, ActionLifecycleState::Succeeded);
    assert!(rctx.policy_constraints_snapshot().is_none());
}

// ---------------------------------------------------------------------------
// 5. Hydration: HydratedSubject carries through to the kernel's PolicyContext.
// ---------------------------------------------------------------------------

/// Asserts that the kernel observes the exact `HydratedSubject` the caller
/// installed on the `RuntimeContext` (not the envelope's plain string id).
#[derive(Debug)]
struct EchoSubjectKernel {
    seen: tokio::sync::Mutex<Option<HydratedSubject>>,
}

#[async_trait]
impl PolicyKernel for EchoSubjectKernel {
    async fn evaluate_policy(
        &self,
        _envelope: &ActionEnvelope,
        context: &PolicyContext,
    ) -> Result<PolicyDecision, PolicyError> {
        *self.seen.lock().await = Some(context.subject.clone());
        Ok(make_decision(
            Decision::Allow,
            "ScopedAllow",
            Constraints::default(),
            ApprovalRequirement::default(),
        ))
    }
}

#[tokio::test]
async fn hydrated_subject_reaches_policy_kernel_unchanged() {
    let kernel: Arc<EchoSubjectKernel> = Arc::new(EchoSubjectKernel {
        seen: tokio::sync::Mutex::new(None),
    });
    let runtime = InMemoryCapabilityRuntime::new().with_policy_kernel(kernel.clone());

    let subject = HydratedSubject {
        canonical_subject_id: "human:lucky:01HX0000000000000000000000".to_string(),
        subject_type: SubjectType::Human,
        groups: vec!["operators".to_string(), "admins".to_string()],
        capabilities: vec!["cap.service.manage".to_string()],
        session_class: "CONFIDENTIAL".to_string(),
        recovery_mode: false,
        is_ai: false,
    };
    let rctx = RuntimeContext::from_subject(subject.clone(), "polb_test_v1", "code_t030");
    let env = happy_envelope("human:lucky", false);
    let _ = runtime.submit_action(&env, &rctx).await.expect("submit");

    let seen = kernel.seen.lock().await.clone().expect("seen subject");
    assert_eq!(seen, subject);
}

// ---------------------------------------------------------------------------
// 6. Constraints projection: dispatcher can read `sandbox_profile_id`.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn allow_constraints_visible_via_policy_constraints_snapshot() {
    let kernel = ScriptedKernel::allow(
        constraints_with_sandbox("net-restricted"),
        ApprovalRequirement::default(),
    );
    let runtime = InMemoryCapabilityRuntime::new().with_policy_kernel(kernel);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000000"),
        "polb_test_v1",
        "code_t030",
    );
    let env = happy_envelope("human:lucky", false);
    let _ = runtime.submit_action(&env, &rctx).await.expect("submit");

    let cons = rctx.policy_constraints_snapshot().expect("constraints");
    assert_eq!(
        cons.sandbox_profile_id,
        Some(SandboxProfileId("net-restricted".to_string())),
        "dispatcher must see the policy-bound sandbox profile id"
    );
}

// ---------------------------------------------------------------------------
// 7. AI tripwire: AI subject + Allow with no human approver → counter +1.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ai_subject_with_allow_increments_tripwire() {
    let kernel = ScriptedKernel::allow(Constraints::default(), ApprovalRequirement::default());
    let runtime = InMemoryCapabilityRuntime::new().with_policy_kernel(kernel);
    assert_eq!(runtime.policy_double_check_warnings(), 0);

    let rctx = RuntimeContext::from_subject(
        ai_subject("agent:llm:01HX0000000000000000000000"),
        "polb_test_v1",
        "code_t030",
    );
    let env = happy_envelope("agent:llm", true);
    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");
    assert_eq!(result.status, ActionLifecycleState::Succeeded);
    assert_eq!(
        runtime.policy_double_check_warnings(),
        1,
        "AI + Allow without human approver must trip the §17 tripwire"
    );
}

// ---------------------------------------------------------------------------
// 8. Human subject + Allow → tripwire does NOT increment.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn human_subject_with_allow_does_not_increment_tripwire() {
    let kernel = ScriptedKernel::allow(Constraints::default(), ApprovalRequirement::default());
    let runtime = InMemoryCapabilityRuntime::new().with_policy_kernel(kernel);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000000"),
        "polb_test_v1",
        "code_t030",
    );
    let env = happy_envelope("human:lucky", false);
    let _ = runtime.submit_action(&env, &rctx).await.expect("submit");

    assert_eq!(
        runtime.policy_double_check_warnings(),
        0,
        "human subject must NOT trip the §17 AI tripwire"
    );
}

// ---------------------------------------------------------------------------
// 9. AI subject + Allow WITH human approver → tripwire does NOT increment.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ai_subject_with_human_approver_does_not_increment_tripwire() {
    let approval = ApprovalRequirement {
        required: true,
        approval_scope: ApprovalScope::ExactRequestHash,
        ttl_seconds: 300,
        approver_classes: vec![ApproverClass::Human],
        require_human_co_signer: false,
    };
    let kernel = ScriptedKernel::allow(Constraints::default(), approval);
    let runtime = InMemoryCapabilityRuntime::new().with_policy_kernel(kernel);

    let rctx = RuntimeContext::from_subject(
        ai_subject("agent:llm:01HX0000000000000000000000"),
        "polb_test_v1",
        "code_t030",
    );
    let env = happy_envelope("agent:llm", true);
    let _ = runtime.submit_action(&env, &rctx).await.expect("submit");

    assert_eq!(
        runtime.policy_double_check_warnings(),
        0,
        "AI + Allow WITH human approver must NOT trip the §17 tripwire"
    );
}

// ---------------------------------------------------------------------------
// 10. SubjectUnauthenticated → T6 POLICY_DENIED + EnvelopeValidationFailed.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn subject_unauthenticated_short_circuits_to_policy_denied() {
    let kernel = ScriptedKernel::error(PolicyError::SubjectUnauthenticated);
    let runtime = InMemoryCapabilityRuntime::new().with_policy_kernel(kernel);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:unknown:01HX0000000000000000000000"),
        "polb_test_v1",
        "code_t030",
    );
    let env = happy_envelope("human:unknown", false);
    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action returns context even on unauthenticated");

    assert_eq!(result.status, ActionLifecycleState::PolicyDenied);
    assert_eq!(
        result.error,
        Some(ExecutionFailureReason::EnvelopeValidationFailed),
        "SubjectUnauthenticated must map to EnvelopeValidationFailed (S2.3 §7 / S10.1 §6.1 step 0)"
    );
    assert!(rctx.policy_constraints_snapshot().is_none());
}

// ---------------------------------------------------------------------------
// 11. BundleVersionMismatch → RuntimeError::PolicyEvalFailed.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn bundle_version_mismatch_propagates_as_policy_eval_failed() {
    let kernel = ScriptedKernel::error(PolicyError::BundleVersionMismatch {
        expected: "polb_v1".to_string(),
        found: "polb_v2".to_string(),
    });
    let runtime = InMemoryCapabilityRuntime::new().with_policy_kernel(kernel);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000000"),
        "polb_v1",
        "code_t030",
    );
    let env = happy_envelope("human:lucky", false);
    let err = runtime
        .submit_action(&env, &rctx)
        .await
        .expect_err("BundleVersionMismatch must surface as PolicyEvalFailed");
    assert!(
        matches!(err, RuntimeError::PolicyEvalFailed(_)),
        "got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 12. End-to-end happy path: full pipeline walk on Allow.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn end_to_end_allow_walks_full_pipeline_to_succeeded() {
    let kernel = ScriptedKernel::allow(
        constraints_with_sandbox("host-service-control"),
        ApprovalRequirement::default(),
    );
    let runtime = InMemoryCapabilityRuntime::new().with_policy_kernel(kernel);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000000"),
        "polb_test_v1",
        "code_t030",
    );
    let env = happy_envelope("human:lucky", false);
    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");

    // Validate (T2) → PolicyEvaluate Allow (T4) → Queue (T12 stub) →
    // Execute (T13 stub) → Verify (T15+T17) ⇒ Succeeded.
    assert_eq!(result.status, ActionLifecycleState::Succeeded);
    assert_eq!(result.error, None);
    // last_updated_at is monotonic >= created_at (sanity that transitions
    // actually moved the clock).
    assert!(result.last_updated_at >= result.created_at);
    // Constraints projection survived to the end of the run.
    assert!(rctx.policy_constraints_snapshot().is_some());
}
