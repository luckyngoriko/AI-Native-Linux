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

use crate::bundle_loader::BundleLoader;
use crate::cache::SharedDecisionCache;
use crate::explain::SharedDecisionLog;
use crate::kernel::{InMemoryPolicyKernel, PolicyContext, PolicyKernel};
use crate::service::conversions::{
    envelope_from_bytes, policy_decision_to_proto, policy_error_to_status,
};
use crate::service::proto;
use crate::service::SCHEMA_VERSION;
use crate::snapshot::EnrichmentSnapshot;
use crate::subject::{HydratedSubject, SubjectType};

/// Default engine id reported by `GetPolicyEngineInfo`. Production binaries
/// override this via [`PolicyKernelService::with_engine_id`] so multiple
/// kernels in the same cluster can be distinguished.
pub const DEFAULT_ENGINE_ID: &str = "aios-policy-inproc";

/// Default code version reported on every decision. Production wiring should
/// pass the build-time `git describe` string here.
pub const DEFAULT_CODE_VERSION: &str = "aios-policy/0.1.0-T025";

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
    /// Sibling handle on the same kernel when it's an
    /// [`InMemoryPolicyKernel`]; used by `LoadBundle` to call
    /// `set_active_bundle`. The trait object `Arc<dyn PolicyKernel>` has
    /// no `Any` supertrait, so we keep a typed shadow pointer here when
    /// constructed via [`Self::new_in_memory`]. Production wiring uses
    /// `new_in_memory` exclusively; `new(dyn PolicyKernel)` is for tests
    /// that exercise mock kernels.
    in_memory_kernel: Option<Arc<InMemoryPolicyKernel>>,
    engine_id: String,
    code_version: String,
    /// Currently active bundle version, held behind `Arc<RwLock<>>` so the
    /// `LoadBundle` RPC can swap it atomically with the kernel's own
    /// active-bundle pointer (§12.4 hot reload).
    active_bundle_version: Arc<std::sync::RwLock<String>>,
    /// The placeholder subject used to populate the `PolicyContext` until
    /// per-RPC mTLS-derived identity lands in T-024. The default value
    /// matches the canonical `agent:dev` test fixture (§22). Callers can
    /// override it via [`PolicyKernelService::with_default_subject`].
    default_subject: HydratedSubject,
    started_at: chrono::DateTime<chrono::Utc>,
    /// Optional bundle loader (T-024). When `Some`, the `LoadBundle` RPC
    /// uses it to verify incoming bundles; when `None`, `LoadBundle`
    /// returns `Unimplemented` (operators must opt-in to the load path by
    /// constructing the service with [`PolicyKernelService::with_bundle_loader`]).
    bundle_loader: Option<Arc<BundleLoader>>,
    /// Optional decision-cache handle (T-024). Used to invalidate the
    /// old bundle's cached decisions on `LoadBundle`. Independent from
    /// the kernel's cache attachment so the service can wire one cache
    /// across a multi-kernel deployment if needed.
    cache: Option<SharedDecisionCache>,
    /// Optional decision log for the `ExplainDecision` RPC (T-024). When
    /// `None`, `ExplainDecision` returns `NotFound`. The pipeline writes
    /// every successful decision into this log.
    decision_log: Option<SharedDecisionLog>,
}

impl std::fmt::Debug for PolicyKernelService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PolicyKernelService")
            .field("engine_id", &self.engine_id)
            .field("code_version", &self.code_version)
            .field("active_bundle_version", &self.read_active_bundle_version())
            .field("default_subject", &self.default_subject)
            .field("started_at", &self.started_at)
            .field("bundle_loader", &self.bundle_loader.is_some())
            .field("cache", &self.cache.is_some())
            .field("decision_log", &self.decision_log.is_some())
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
            in_memory_kernel: None,
            engine_id: DEFAULT_ENGINE_ID.to_owned(),
            code_version: DEFAULT_CODE_VERSION.to_owned(),
            active_bundle_version: Arc::new(std::sync::RwLock::new(
                DEFAULT_BUNDLE_VERSION.to_owned(),
            )),
            default_subject: default_placeholder_subject(),
            started_at: chrono::Utc::now(),
            bundle_loader: None,
            cache: None,
            decision_log: None,
        }
    }

    /// Construct an adapter around an [`InMemoryPolicyKernel`] (T-024 —
    /// the canonical production ctor; preserves the typed pointer so
    /// `LoadBundle` can swap the kernel's active bundle in place).
    #[must_use]
    pub fn new_in_memory(kernel: Arc<InMemoryPolicyKernel>) -> Self {
        let dyn_handle: Arc<dyn PolicyKernel> = kernel.clone();
        Self {
            inner: dyn_handle,
            in_memory_kernel: Some(kernel),
            engine_id: DEFAULT_ENGINE_ID.to_owned(),
            code_version: DEFAULT_CODE_VERSION.to_owned(),
            active_bundle_version: Arc::new(std::sync::RwLock::new(
                DEFAULT_BUNDLE_VERSION.to_owned(),
            )),
            default_subject: default_placeholder_subject(),
            started_at: chrono::Utc::now(),
            bundle_loader: None,
            cache: None,
            decision_log: None,
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
    pub fn with_bundle_version(self, v: impl Into<String>) -> Self {
        if let Ok(mut guard) = self.active_bundle_version.write() {
            *guard = v.into();
        }
        self
    }

    /// Attach a [`BundleLoader`] (T-024 — enables the `LoadBundle` RPC).
    #[must_use]
    pub fn with_bundle_loader(mut self, loader: BundleLoader) -> Self {
        self.bundle_loader = Some(Arc::new(loader));
        self
    }

    /// Attach a [`SharedDecisionCache`] handle (T-024).
    ///
    /// The service uses this only to invalidate cached decisions on
    /// `LoadBundle`; the kernel attaches its own cache via
    /// [`InMemoryPolicyKernel::new_with_cache`] for the read-side hits.
    /// Production wiring passes the SAME handle to both so the
    /// invalidate-on-reload path lines up with the read-side hits.
    #[must_use]
    pub fn with_cache(mut self, cache: SharedDecisionCache) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Attach a [`SharedDecisionLog`] handle (T-024 — enables
    /// `ExplainDecision`). The pipeline records every successful decision
    /// here; the RPC reads by `policy_decision_id`.
    #[must_use]
    pub fn with_decision_log(mut self, log: SharedDecisionLog) -> Self {
        self.decision_log = Some(log);
        self
    }

    /// Read the currently active bundle version. Poisoned-lock tolerant.
    fn read_active_bundle_version(&self) -> String {
        match self.active_bundle_version.read() {
            Ok(g) => g.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Swap the active bundle version (T-024 — `LoadBundle` integration).
    fn swap_active_bundle_version(&self, new_version: String) -> String {
        match self.active_bundle_version.write() {
            Ok(mut g) => std::mem::replace(&mut *g, new_version),
            Err(poisoned) => std::mem::replace(&mut *poisoned.into_inner(), new_version),
        }
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
        // T-024: the snapshot is now real (content-addressed); the gRPC
        // adapter still mints an empty snapshot until the M4 AIOS-FS read-
        // path lands. `compute_id` produces a deterministic anchor over
        // the empty `(object, adapter)` tuple — the same id for every
        // request until M4 wires per-object enrichment.
        let snapshot = EnrichmentSnapshot::with_fields(
            crate::snapshot::ObjectEnrichment::default(),
            crate::snapshot::AdapterEnrichment::default(),
        )
        .unwrap_or_default();
        PolicyContext::new(
            subject,
            snapshot,
            self.read_active_bundle_version(),
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
        // T-024 — record the decision into the explain log when one is
        // attached. The log is a bounded ring-buffer; `record` evicts the
        // oldest entry on overflow per §20 / Appendix A.
        if let Some(log) = self.decision_log.as_ref() {
            log.record(crate::explain::DecisionPath::from_decision(
                decision.clone(),
            ));
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
        request: Request<proto::LoadBundleRequest>,
    ) -> Result<Response<proto::LoadBundleResponse>, Status> {
        // T-024 — full bundle activation path (S2.3 §12.4 hot reload).
        // Flow:
        //   1. Extract the proto bundle from the request.
        //   2. Verify the loader is configured; reject `Unimplemented`
        //      otherwise (the service is opted-out of LoadBundle by
        //      default, per the operator-explicit-opt-in discipline).
        //   3. Bridge proto::PolicyBundle to the Rust shape via
        //      `proto_bundle_to_rust`.
        //   4. Verify via `BundleLoader::accept_bundle` — version pin,
        //      authority lookup, Ed25519 signature, per-rule condition
        //      parse. Failure ⇒ `FailedPrecondition` per §18.2.
        //   5. On `stage_only = true`, return without activating.
        //   6. Otherwise: swap kernel's active bundle pointer + service's
        //      active_bundle_version atomically; then invalidate the
        //      decision cache for the OLD version (§13.2). Return
        //      `LoadBundleResponse { active = true }`.
        let r = request.into_inner();
        let proto_bundle = r
            .bundle
            .ok_or_else(|| Status::invalid_argument("LoadBundleRequest.bundle is required"))?;
        let loader = self.bundle_loader.as_ref().ok_or_else(|| {
            Status::unimplemented(
                "LoadBundle disabled — service constructed without a BundleLoader (T-024 opt-in)",
            )
        })?;
        let rust_bundle = crate::service::conversions::proto_bundle_to_rust(&proto_bundle)
            .map_err(|e| policy_error_to_status(&e))?;
        let verified = loader
            .accept_bundle(rust_bundle)
            .map_err(|e| policy_error_to_status(&e))?;
        let new_version = verified.bundle_version.clone();
        let activated_at = chrono::Utc::now();

        if r.stage_only {
            return Ok(Response::new(proto::LoadBundleResponse {
                bundle_version: new_version,
                active: false,
                status_message: "bundle verified; not activated (stage_only=true)".into(),
                activated_at: Some(crate::service::conversions::datetime_to_proto(activated_at)),
            }));
        }

        // T-024 — bundle activation is best-effort coordinated across the
        // two pointers (service-level version label + kernel-level active
        // bundle). Both swaps use lock-and-replace primitives so the
        // window between them is bounded by lock acquisition only.
        let previous_version = self.swap_active_bundle_version(new_version.clone());
        // Try to swap the kernel's active bundle when the kernel is in
        // fact an `InMemoryPolicyKernel`. The trait object route via
        // `Any` would force a downcast; instead we route via the kernel's
        // own `set_active_bundle` method when accessible. Production
        // wiring always uses an `InMemoryPolicyKernel`, so the downcast
        // succeeds; otherwise the swap is silently best-effort (the
        // service-level pointer is the authoritative one for decisions).
        if let Some(in_mem) = self.in_memory_kernel.as_ref() {
            in_mem.set_active_bundle(verified);
        }
        // §13.2: bundle flip ⇒ invalidate every cached decision for the
        // old version. Counts go onto the status_message for operator
        // visibility.
        let invalidated = self
            .cache
            .as_ref()
            .map_or(0, |c| c.invalidate_for_bundle(&previous_version));
        Ok(Response::new(proto::LoadBundleResponse {
            bundle_version: new_version,
            active: true,
            status_message: format!(
                "bundle activated; previous={previous_version}; cache entries invalidated={invalidated}"
            ),
            activated_at: Some(crate::service::conversions::datetime_to_proto(activated_at)),
        }))
    }

    async fn rollback_bundle(
        &self,
        request: Request<proto::RollbackBundleRequest>,
    ) -> Result<Response<proto::RollbackBundleResponse>, Status> {
        // T-025 — operator-only rollback per S2.3 §12.5. Flow:
        //   1. Require the kernel to be an `InMemoryPolicyKernel` (only impl
        //      that carries the rollback stack). Other impls ⇒ `Unimplemented`.
        //   2. Pop the most-recent displaced bundle off the kernel's
        //      rollback stack and install it. Empty stack ⇒
        //      `FailedPrecondition` per §18.2 (no prior bundle to restore).
        //   3. Invalidate every cached decision for the bundle being
        //      displaced (§13.2 — bundle flip invalidates the cache). The
        //      override boundary is cleared by `rollback_active_bundle`
        //      itself per §16.3.
        //   4. Mint an evidence receipt id `evr_rb_<ULID>` for the rollback
        //      action; full S3.1 evidence record emission lands at M5+.
        let req = request.into_inner();
        let kernel = self.in_memory_kernel.as_ref().ok_or_else(|| {
            Status::unimplemented(
                "RollbackBundle requires an InMemoryPolicyKernel (constructed via PolicyKernelService::new_in_memory)",
            )
        })?;
        let (restored, displaced_version) = kernel.rollback_active_bundle().ok_or_else(|| {
            Status::failed_precondition(
                "RollbackBundle: rollback stack is empty (no previous bundle to restore — S2.3 §12.5)",
            )
        })?;
        // Optional: a target_bundle_version was requested. The rev.2 §12.5
        // contract is "operator-only rollback to the immediately previous
        // bundle"; if the caller specifies a target_bundle_version that
        // does not match the one we just popped, return
        // `FailedPrecondition` so a mismatched expectation is not silently
        // swallowed.
        if !req.target_bundle_version.is_empty()
            && req.target_bundle_version != restored.bundle_version
        {
            // Best-effort restore the displacement so the kernel isn't
            // left in an unexpected state; ignore the result because the
            // operator is going to retry / inspect anyway.
            let _ = kernel.set_active_bundle(restored.clone());
            return Err(Status::failed_precondition(format!(
                "RollbackBundle: target_bundle_version={} but the rollback stack head is {} (S2.3 §12.5)",
                req.target_bundle_version, restored.bundle_version
            )));
        }
        let restored_version = restored.bundle_version;
        // Swap the service-level version pointer.
        let previously_active = self.swap_active_bundle_version(restored_version.clone());
        // §13.2 — invalidate cached decisions for the displaced bundle
        // version. `displaced_version` (from kernel) and
        // `previously_active` (service-level) should agree when the
        // service was constructed via `new_in_memory`; we use the
        // service-level value as the cache key.
        let invalidated = self
            .cache
            .as_ref()
            .map_or(0, |c| c.invalidate_for_bundle(&previously_active));
        // Mint an evidence-receipt id (placeholder until M5 evidence-log
        // integration); the id is content-addressed enough to be unique
        // per rollback for audit purposes.
        let evidence_receipt_id = format!("evr_rb_{}", ulid::Ulid::new());
        let _ = displaced_version; // already captured via swap_active_bundle_version
        let _ = invalidated; // surfaced via tracing in M5+; not part of proto response
        Ok(Response::new(proto::RollbackBundleResponse {
            previous_bundle_version: previously_active,
            current_bundle_version: restored_version,
            evidence_receipt_id,
        }))
    }

    async fn explain_decision(
        &self,
        request: Request<proto::ExplainDecisionRequest>,
    ) -> Result<Response<proto::ExplainDecisionResponse>, Status> {
        // T-024 — explain trail lookup. The pipeline writes each
        // successful decision to the [`SharedDecisionLog`]; this RPC
        // looks up by `policy_decision_id`. Missing log handle ⇒
        // `Unimplemented` (operator opt-in). Unknown id ⇒ `NotFound`.
        let req = request.into_inner();
        let log = self.decision_log.as_ref().ok_or_else(|| {
            Status::unimplemented(
                "ExplainDecision disabled — service constructed without a SharedDecisionLog (T-024 opt-in)",
            )
        })?;
        let path = log.get(&req.policy_decision_id).ok_or_else(|| {
            Status::not_found(format!(
                "no decision found for policy_decision_id={}",
                req.policy_decision_id
            ))
        })?;
        Ok(Response::new(proto::ExplainDecisionResponse {
            decision: Some(crate::service::conversions::policy_decision_to_proto(
                &path.decision,
            )),
            rule_chain: path.rule_chain,
            narrative: path.narrative,
            enrichment_snapshot: None,
        }))
    }

    async fn get_policy_engine_info(
        &self,
        _request: Request<()>,
    ) -> Result<Response<proto::PolicyEngineInfo>, Status> {
        Ok(Response::new(proto::PolicyEngineInfo {
            engine_id: self.engine_id.clone(),
            supported_schema_versions: vec![SCHEMA_VERSION.to_owned()],
            default_schema_version: SCHEMA_VERSION.to_owned(),
            active_bundle_version: self.read_active_bundle_version(),
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
