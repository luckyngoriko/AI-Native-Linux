//! gRPC `PolicyKernel` server adapter + bootstrap helpers (T-023).
//!
//! [`PolicyKernelService`] is the adapter that mounts an in-tree
//! [`crate::PolicyKernel`] impl behind the tonic-generated
//! `policy_kernel_server::PolicyKernel` transport trait. The split between
//! the typed Rust trait and the wire-typed server trait keeps the kernel
//! itself unaware of tonic — the kernel speaks `(ActionEnvelope,
//! PolicyContext) -> Result<PolicyDecision, PolicyError>`, the adapter
//! handles all the proto3 envelope coercion and the §18.2 status mapping.
//!
//! ## Discipline
//!
//! - The adapter owns an `Arc<dyn PolicyKernel>` — production wiring will
//!   construct an [`crate::InMemoryPolicyKernel`] (or a future caching
//!   wrapper) and hand it to the adapter via [`PolicyKernelService::new`].
//! - Each RPC reconstructs the [`crate::PolicyContext`] from
//!   request-supplied bytes (envelope) + a fixed subject (T-023 baseline:
//!   the envelope's own `subject_canonical_id`; T-024 will lift this onto
//!   per-RPC mTLS-derived identity). The bundle version + code version
//!   come from the service's own state (`active_bundle_version` / static
//!   string built into the binary).
//! - `SimulatePolicy` is the same path as `EvaluatePolicy` but with
//!   `simulated = true` on the decision before it goes out. Per §14 the
//!   simulation share the same pipeline; the only difference is the audit
//!   marker and the "never grants durable approval" rule (enforced
//!   in-band by setting `simulated` — durable approval is granted by L4
//!   when the approval pipeline lands in T-024+, not here).
//! - `LoadBundle`, `RollbackBundle`, `ExplainDecision` are deliberately
//!   stubbed (T-024 / T-025) and return `Unimplemented` so callers cannot
//!   silently rely on a half-built path.
//!
//! ## Schema-version check
//!
//! Every `EvaluatePolicy` / `SimulatePolicy` request carries a
//! `schema_version` string. The adapter rejects mismatches at the wire
//! boundary with `Code::FailedPrecondition` per the §18.2 failure-mode
//! discipline (the engine is structurally incompatible with the caller's
//! schema; not a payload defect, not an auth problem). Empty strings are
//! accepted as a forward-compat affordance — older clients that don't yet
//! emit the field still work.

// `tonic::Status` carries headers + metadata + source error and is wide by
// design. The `result_large_err` lint flags every `Result<_, Status>`; in
// the gRPC server adapter that lint would fire on every tonic trait method.
// Boxing would fight the tonic API ergonomics for no measurable gain.
#![allow(clippy::result_large_err)]

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use tonic::transport::server::{Router, Server};
use tonic::{Request, Response, Status};

use crate::kernel::{EnrichmentSnapshot, PolicyContext, PolicyKernel};
use crate::service::conversions::{
    envelope_from_bytes, policy_decision_to_proto, policy_error_to_status,
};
use crate::service::proto;
use crate::service::SCHEMA_VERSION;
use crate::subject::{HydratedSubject, SubjectType};

/// Default engine id reported by `GetPolicyEngineInfo`. Production binaries
/// override this via [`PolicyKernelService::with_engine_id`] so multiple
/// kernels in the same cluster can be distinguished.
pub const DEFAULT_ENGINE_ID: &str = "aios-policy-inproc";

/// Default code version reported on every decision. Production wiring should
/// pass the build-time `git describe` string here.
pub const DEFAULT_CODE_VERSION: &str = "aios-policy/0.0.1-T023";

/// Default bundle version reported by `GetPolicyEngineInfo` when no bundle
/// is loaded. The engine treats an empty `active_bundle_version` as
/// "default-deny floor only" per S2.3 §11.
pub const DEFAULT_BUNDLE_VERSION: &str = "polb_default";

/// gRPC adapter for an in-tree [`crate::PolicyKernel`] impl.
///
/// Mounted under tonic via
/// `Server::builder().add_service(PolicyKernelGrpcServer::new(svc))` (see
/// [`build_router`] for the canonical bootstrap).
#[derive(Clone)]
pub struct PolicyKernelService {
    inner: Arc<dyn PolicyKernel>,
    engine_id: String,
    code_version: String,
    active_bundle_version: String,
    /// The placeholder subject used to populate the `PolicyContext` until
    /// per-RPC mTLS-derived identity lands in T-024. The default value
    /// matches the canonical `agent:dev` test fixture (§22). Callers can
    /// override it via [`PolicyKernelService::with_default_subject`].
    default_subject: HydratedSubject,
    started_at: chrono::DateTime<chrono::Utc>,
}

impl std::fmt::Debug for PolicyKernelService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PolicyKernelService")
            .field("engine_id", &self.engine_id)
            .field("code_version", &self.code_version)
            .field("active_bundle_version", &self.active_bundle_version)
            .field("default_subject", &self.default_subject)
            .field("started_at", &self.started_at)
            .finish_non_exhaustive()
    }
}

impl PolicyKernelService {
    /// Construct an adapter wrapping the given kernel.
    ///
    /// Defaults: `engine_id` = [`DEFAULT_ENGINE_ID`], `code_version` =
    /// [`DEFAULT_CODE_VERSION`], `bundle_version` = [`DEFAULT_BUNDLE_VERSION`],
    /// default subject = placeholder `agent:dev` AI subject per the §22
    /// golden fixtures (T-024 swaps this for per-RPC mTLS identity).
    #[must_use]
    pub fn new(inner: Arc<dyn PolicyKernel>) -> Self {
        Self {
            inner,
            engine_id: DEFAULT_ENGINE_ID.to_owned(),
            code_version: DEFAULT_CODE_VERSION.to_owned(),
            active_bundle_version: DEFAULT_BUNDLE_VERSION.to_owned(),
            default_subject: default_placeholder_subject(),
            started_at: chrono::Utc::now(),
        }
    }

    /// Override the engine id reported by `GetPolicyEngineInfo`.
    #[must_use]
    pub fn with_engine_id(mut self, id: impl Into<String>) -> Self {
        self.engine_id = id.into();
        self
    }

    /// Override the active bundle version reported by decisions and
    /// `GetPolicyEngineInfo`.
    #[must_use]
    pub fn with_bundle_version(mut self, v: impl Into<String>) -> Self {
        self.active_bundle_version = v.into();
        self
    }

    /// Override the default subject used to populate `PolicyContext` when
    /// the request has no mTLS-derived identity (T-024).
    #[must_use]
    pub fn with_default_subject(mut self, s: HydratedSubject) -> Self {
        self.default_subject = s;
        self
    }

    /// Validate the schema version on an incoming request.
    fn check_schema_version(supplied: &str) -> Result<(), Status> {
        if supplied.is_empty() || supplied == SCHEMA_VERSION {
            Ok(())
        } else {
            Err(Status::failed_precondition(format!(
                "schema_version mismatch: server speaks `{SCHEMA_VERSION}`, request sent `{supplied}`"
            )))
        }
    }

    /// Build a fresh [`PolicyContext`] from request-supplied + service-owned
    /// state. Per §13 the resulting `(request_hash, bundle_version,
    /// enrichment_snapshot_id)` triple must produce a deterministic decision —
    /// the service therefore mints a stable `snapshot_id` per request from
    /// the envelope's identity (T-024 swaps this for a real enrichment-snapshot
    /// id from L2 AIOS-FS once the read-path lands).
    fn build_context(&self, env: &aios_action::ActionEnvelope) -> PolicyContext {
        let mut subject = self.default_subject.clone();
        subject
            .canonical_subject_id
            .clone_from(&env.identity.subject_canonical_id);
        // Anchor the snapshot id on the envelope's canonical request hash —
        // the envelope itself does not yet carry an `action_id` field
        // (`aios_action::ActionEnvelope` only carries `identity` + `request` +
        // `execution` + `trace`; an envelope-scoped action_id lands when the
        // Capability Runtime ships). The request hash is the §13 determinism
        // anchor's first component anyway, so using it as the snapshot id
        // collapses two of the three triple components into the same content
        // address until T-024 attaches the real AIOS-FS enrichment snapshot.
        let snapshot_id = env
            .request
            .request_hash()
            .unwrap_or_else(|_| "snap_unknown".to_owned());
        PolicyContext::new(
            subject,
            EnrichmentSnapshot { snapshot_id },
            self.active_bundle_version.clone(),
            self.code_version.clone(),
        )
    }

    /// Internal: drive the kernel for one envelope and return the proto
    /// decision, optionally marking it as simulated (§14).
    async fn evaluate_internal(
        &self,
        bytes: &[u8],
        schema_version: &str,
        simulated: bool,
    ) -> Result<proto::PolicyDecision, Status> {
        Self::check_schema_version(schema_version)?;
        let envelope = envelope_from_bytes(bytes)?;
        let context = self.build_context(&envelope);
        let mut decision = self
            .inner
            .evaluate_policy(&envelope, &context)
            .await
            .map_err(|e| policy_error_to_status(&e))?;
        if simulated {
            decision.simulated = true;
        }
        Ok(policy_decision_to_proto(&decision))
    }
}

/// Build the default placeholder [`HydratedSubject`] used until T-024 lifts
/// per-RPC identity onto mTLS. Mirrors the `agent:dev` §22 fixture.
fn default_placeholder_subject() -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: "agent:dev".to_owned(),
        subject_type: SubjectType::Agent,
        groups: Vec::new(),
        capabilities: Vec::new(),
        session_class: "INTERNAL".to_owned(),
        recovery_mode: false,
        is_ai: true,
    }
}

#[async_trait]
impl proto::policy_kernel_server::PolicyKernel for PolicyKernelService {
    async fn evaluate_policy(
        &self,
        request: Request<proto::EvaluatePolicyRequest>,
    ) -> Result<Response<proto::PolicyDecision>, Status> {
        let r = request.into_inner();
        let decision = self
            .evaluate_internal(&r.envelope_proto, &r.schema_version, false)
            .await?;
        Ok(Response::new(decision))
    }

    async fn simulate_policy(
        &self,
        request: Request<proto::EvaluatePolicyRequest>,
    ) -> Result<Response<proto::PolicyDecision>, Status> {
        let r = request.into_inner();
        let decision = self
            .evaluate_internal(&r.envelope_proto, &r.schema_version, true)
            .await?;
        Ok(Response::new(decision))
    }

    async fn load_bundle(
        &self,
        _request: Request<proto::LoadBundleRequest>,
    ) -> Result<Response<proto::LoadBundleResponse>, Status> {
        // T-024 lands the bundle activation path. T-023 declines to fake it
        // — a stub that accepted-but-ignored the bundle would silently
        // diverge the wire-level "active bundle" from the in-memory state
        // and break the §13 determinism contract.
        Err(Status::unimplemented(
            "LoadBundle is not yet wired (queued for T-024 / T-025 — bundle activation + override boundary)",
        ))
    }

    async fn rollback_bundle(
        &self,
        _request: Request<proto::RollbackBundleRequest>,
    ) -> Result<Response<proto::RollbackBundleResponse>, Status> {
        Err(Status::unimplemented(
            "RollbackBundle is not yet wired (queued for T-025 — M3 closer)",
        ))
    }

    async fn explain_decision(
        &self,
        _request: Request<proto::ExplainDecisionRequest>,
    ) -> Result<Response<proto::ExplainDecisionResponse>, Status> {
        // ExplainDecision requires a decision cache keyed by
        // policy_decision_id. T-024 lands the cache + this RPC together.
        Err(Status::unimplemented(
            "ExplainDecision requires the decision cache (queued for T-024)",
        ))
    }

    async fn get_policy_engine_info(
        &self,
        _request: Request<()>,
    ) -> Result<Response<proto::PolicyEngineInfo>, Status> {
        Ok(Response::new(proto::PolicyEngineInfo {
            engine_id: self.engine_id.clone(),
            supported_schema_versions: vec![SCHEMA_VERSION.to_owned()],
            default_schema_version: SCHEMA_VERSION.to_owned(),
            active_bundle_version: self.active_bundle_version.clone(),
            // T-024 lifts this onto real bundle-state inspection. T-023
            // returns `degraded = false` because no bundle activation
            // path exists yet — the engine is on the default-deny floor
            // by construction, which §11 defines as the non-degraded
            // baseline (degraded mode is §12.3, post-LoadBundle failure).
            degraded: false,
            rules_in_active_bundle: 0,
            started_at: Some(crate::service::conversions::datetime_to_proto(
                self.started_at,
            )),
        }))
    }
}

// ---------------------------------------------------------------------------
// Bootstrap helpers
// ---------------------------------------------------------------------------

/// Build a `tonic::transport::server::Router` with the `PolicyKernel` service
/// mounted.
///
/// The returned router is fluent — callers may chain `.add_service(...)` for
/// additional services (e.g. health, reflection) before calling `.serve(addr)`
/// or `.serve_with_incoming(...)`.
#[must_use]
pub fn build_router(svc: PolicyKernelService) -> Router {
    Server::builder().add_service(proto::policy_kernel_server::PolicyKernelServer::new(svc))
}

/// Convenience helper: bind to `addr` and serve until the future is dropped.
///
/// This blocks the calling task indefinitely (until the server is shut down
/// by some other signal). For graceful shutdown, prefer
/// `Server::builder().add_service(...).serve_with_shutdown(addr, signal)`
/// directly — the integration test uses an `oneshot` cancellation channel
/// for that purpose.
///
/// # Errors
///
/// Returns the underlying [`tonic::transport::Error`] when the bind / listen
/// loop fails (port in use, permission denied, etc.).
pub async fn serve(
    svc: PolicyKernelService,
    addr: SocketAddr,
) -> Result<(), tonic::transport::Error> {
    build_router(svc).serve(addr).await
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use crate::kernel::InMemoryPolicyKernel;

    #[test]
    fn schema_version_check_accepts_empty_and_canonical() {
        assert!(PolicyKernelService::check_schema_version("").is_ok());
        assert!(PolicyKernelService::check_schema_version(SCHEMA_VERSION).is_ok());
    }

    #[test]
    fn schema_version_check_rejects_mismatch_with_failed_precondition() {
        let err = PolicyKernelService::check_schema_version("aios.policy.v0").unwrap_err();
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
    }

    #[test]
    fn default_subject_is_agent_dev_ai_subject() {
        let s = default_placeholder_subject();
        assert_eq!(s.canonical_subject_id, "agent:dev");
        assert_eq!(s.subject_type, SubjectType::Agent);
        assert!(s.is_ai);
        assert!(!s.recovery_mode);
    }

    #[test]
    fn build_router_compiles_and_accepts_policy_kernel_service() {
        // Smoke test: the type plumbing wires through. There is no behaviour
        // to assert on a `Router` value beyond constructing it; the
        // compile-time check that `PolicyKernelServer<PolicyKernelService>`
        // is a valid `tonic::server::NamedService` is the real assertion.
        let kernel: Arc<dyn PolicyKernel> = Arc::new(InMemoryPolicyKernel::new());
        let svc = PolicyKernelService::new(kernel)
            .with_engine_id("test-engine")
            .with_bundle_version("polb_test");
        let _router = build_router(svc);
    }
}
