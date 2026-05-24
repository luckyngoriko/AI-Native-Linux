//! T-021 integration tests — subject hydration (S2.3 §7).
//!
//! Pins:
//! - [`InMemoryHydrator::with_fixtures`] canonicalises the four standard
//!   provisional subject ids (human / agent / application / service).
//! - Unknown / revoked / expired provisional ids surface as
//!   [`PolicyError::SubjectUnauthenticated`] without discriminating between
//!   the three failure modes (§7).
//! - The [`SubjectHydrator`] trait is dyn-compatible through
//!   `Arc<dyn SubjectHydrator + Send + Sync>` so the kernel can hold the
//!   hydrator without monomorphising on the concrete impl.
//! - End-to-end via [`InMemoryPolicyKernel`]: an envelope referencing an
//!   unknown provisional id short-circuits to `DENY` with
//!   `reason_code = "SubjectUnauthenticated"` at pipeline step 2.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::{Duration, Utc};

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_policy::{
    Decision, EnrichmentSnapshot, HydratedRecord, HydratedSubject, InMemoryHydrator,
    InMemoryPolicyKernel, PolicyContext, PolicyError, PolicyKernel, SubjectHydrator, SubjectType,
};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn make_context(subject: HydratedSubject) -> PolicyContext {
    PolicyContext::new(
        subject,
        EnrichmentSnapshot {
            snapshot_id: "snap_t021".to_owned(),
        },
        "polb_t021_v1",
        "code_t021",
    )
}

fn placeholder_subject() -> HydratedSubject {
    // Pre-hydration placeholder; the kernel replaces it after a successful
    // `SubjectHydrator::hydrate` call, so the contents are irrelevant.
    HydratedSubject {
        canonical_subject_id: "placeholder".to_owned(),
        subject_type: SubjectType::Human,
        groups: vec![],
        capabilities: vec![],
        session_class: "INTERNAL".to_owned(),
        recovery_mode: false,
        is_ai: false,
    }
}

fn envelope_for(subject_canonical_id: &str) -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new(subject_canonical_id, false),
        Request::new("service.restart", serde_json::json!({"service": "nginx"})),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

// ---------------------------------------------------------------------------
// 1. Fixtures load the canonical four subjects
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fixtures_provide_the_four_canonical_subject_classes() {
    let h = InMemoryHydrator::with_fixtures();
    assert_eq!(h.len(), 4, "with_fixtures must load exactly 4 records");

    let human = h.hydrate("human:lucky").await.expect("human:lucky present");
    assert_eq!(human.subject_type, SubjectType::Human);
    assert!(!human.is_ai, "human subject is_ai must be false");

    let agent = h.hydrate("agent:dev").await.expect("agent:dev present");
    assert_eq!(agent.subject_type, SubjectType::Agent);
    assert!(agent.is_ai, "agent subject is_ai must be true");

    let app = h
        .hydrate("application:planner")
        .await
        .expect("application:planner present");
    assert_eq!(app.subject_type, SubjectType::Application);
    assert!(app.is_ai, "application subject is_ai must be true");

    let svc = h
        .hydrate("service:systemd")
        .await
        .expect("service:systemd present");
    assert_eq!(svc.subject_type, SubjectType::Service);
    assert!(!svc.is_ai, "service subject is_ai must be false");
}

// ---------------------------------------------------------------------------
// 2. Round-trip — hydrating the same id twice returns the same subject
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hydration_is_deterministic_for_the_same_provisional_id() {
    let h = InMemoryHydrator::with_fixtures();
    let a = h.hydrate("agent:dev").await.expect("agent:dev present");
    let b = h.hydrate("agent:dev").await.expect("agent:dev present");
    assert_eq!(
        a, b,
        "hydration must be deterministic across repeated calls"
    );
}

// ---------------------------------------------------------------------------
// 3. Unknown id → SubjectUnauthenticated
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unknown_provisional_id_rejects_with_subject_unauthenticated() {
    let h = InMemoryHydrator::with_fixtures();
    let err = h
        .hydrate("agent:nonexistent")
        .await
        .expect_err("unknown id must error");
    assert_eq!(err, PolicyError::SubjectUnauthenticated);
}

// ---------------------------------------------------------------------------
// 4. Expired record → SubjectUnauthenticated
// ---------------------------------------------------------------------------

#[tokio::test]
async fn expired_record_rejects_with_subject_unauthenticated() {
    let mut h = InMemoryHydrator::new();
    h.insert(
        "agent:legacy",
        HydratedRecord::new(HydratedSubject {
            canonical_subject_id: "agent:legacy:01HXLEGACY00000000000000".to_owned(),
            subject_type: SubjectType::Agent,
            groups: vec![],
            capabilities: vec![],
            session_class: "INTERNAL".to_owned(),
            recovery_mode: false,
            is_ai: true,
        })
        .with_expiry(Utc::now() - Duration::seconds(1)),
    );
    let err = h
        .hydrate("agent:legacy")
        .await
        .expect_err("expired record must error");
    assert_eq!(err, PolicyError::SubjectUnauthenticated);
}

// ---------------------------------------------------------------------------
// 5. dyn trait usage — Arc<dyn SubjectHydrator> works
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dyn_subject_hydrator_through_arc_is_usable() {
    let h: Arc<dyn SubjectHydrator + Send + Sync> = Arc::new(InMemoryHydrator::with_fixtures());
    let s = h
        .hydrate("agent:dev")
        .await
        .expect("agent:dev present via dyn");
    assert_eq!(s.subject_type, SubjectType::Agent);
}

// ---------------------------------------------------------------------------
// 6. End-to-end via the kernel — unknown id short-circuits at step 2
// ---------------------------------------------------------------------------

#[tokio::test]
async fn kernel_with_hydrator_short_circuits_unknown_subject_to_subject_unauthenticated() {
    let hydrator: Arc<dyn SubjectHydrator + Send + Sync> =
        Arc::new(InMemoryHydrator::with_fixtures());
    let kernel = InMemoryPolicyKernel::new_with_subject_hydrator(hydrator);

    let env = envelope_for("agent:ghost"); // not in fixtures
    let ctx = make_context(placeholder_subject());

    let decision = kernel
        .evaluate_policy(&env, &ctx)
        .await
        .expect("evaluate_policy must not raise; §7 collapses to a DENY decision");

    assert_eq!(decision.decision, Decision::Deny);
    assert_eq!(decision.reason_code, "SubjectUnauthenticated");
    assert_eq!(
        decision.bundle_version, "polb_t021_v1",
        "DENY decision must still carry the original bundle_version"
    );
}

// ---------------------------------------------------------------------------
// 7. End-to-end via the kernel — known subject replaces the placeholder
// ---------------------------------------------------------------------------

#[tokio::test]
async fn kernel_with_hydrator_replaces_placeholder_with_hydrated_subject() {
    let hydrator: Arc<dyn SubjectHydrator + Send + Sync> =
        Arc::new(InMemoryHydrator::with_fixtures());
    let kernel = InMemoryPolicyKernel::new_with_subject_hydrator(hydrator);

    let env = envelope_for("agent:dev");
    let ctx = make_context(placeholder_subject());

    let decision = kernel
        .evaluate_policy(&env, &ctx)
        .await
        .expect("must succeed");

    // With a hydrator attached and no scoped rules, the pipeline still
    // lands at the default-deny floor (S2.3 §11); the hydration succeeded
    // (no SubjectUnauthenticated), so we see DefaultDeny.
    assert_eq!(decision.decision, Decision::Deny);
    assert_eq!(
        decision.reason_code, "DefaultDeny",
        "successful hydration must not short-circuit; default-deny floor must fire"
    );
}

// ---------------------------------------------------------------------------
// 8. Backward compatibility — kernel without hydrator uses passed subject
// ---------------------------------------------------------------------------

#[tokio::test]
async fn kernel_without_hydrator_uses_passed_subject_unchanged() {
    let kernel = InMemoryPolicyKernel::new();
    assert!(!kernel.has_subject_hydrator(), "no hydrator attached");

    let env = envelope_for("human:lucky");
    let ctx = make_context(placeholder_subject());

    let decision = kernel
        .evaluate_policy(&env, &ctx)
        .await
        .expect("must succeed");
    assert_eq!(decision.decision, Decision::Deny);
    assert_eq!(
        decision.reason_code, "DefaultDeny",
        "no hydrator → no SubjectUnauthenticated; default-deny floor must fire"
    );
}
