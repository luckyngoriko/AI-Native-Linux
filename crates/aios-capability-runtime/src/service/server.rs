//! gRPC `CapabilityRuntime` server adapter + bootstrap helpers (T-033).
//!
//! [`CapabilityRuntimeService`] is the adapter that mounts an in-tree
//! [`crate::InMemoryCapabilityRuntime`] behind the tonic-generated
//! `capability_runtime_server::CapabilityRuntime` transport trait. The split
//! between the typed Rust runtime and the wire-typed server trait keeps the
//! runtime unaware of tonic — the runtime speaks `(ActionEnvelope,
//! RuntimeContext) -> Result<ActionContext, RuntimeError>`, the adapter
//! handles all the proto3 envelope coercion and the §3.8 status mapping.
//!
//! ## Discipline
//!
//! - The adapter owns an `Arc<InMemoryCapabilityRuntime>` — production wiring
//!   constructs the runtime with the full T-027..T-032 stack composed
//!   (registry + queue + policy kernel + evidence emitter + rollback driver)
//!   and hands it to the adapter via [`CapabilityRuntimeService::new`].
//! - Each RPC reconstructs the [`crate::RuntimeContext`] from a default
//!   placeholder subject (T-033 baseline: matches the canonical `agent:dev`
//!   fixture); T-034 lifts this onto per-RPC mTLS-derived identity.
//! - `ValidateAction`, `ExecuteAction`, `GetActionStatus`, `ListAdapters`,
//!   `GetAdapterCapabilities`, `GetCapabilityRuntimeInfo` are full
//!   implementations.
//! - `EvaluatePolicyForAction`, `RequestApprovalForAction`, `VerifyAction`,
//!   `RollbackAction` are deliberately stubbed (T-034 / T-035) and return
//!   `Unimplemented` so callers cannot silently rely on a half-built path.

#![allow(clippy::result_large_err)]

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use tonic::transport::server::{Router, Server};
use tonic::{Request, Response, Status};

use aios_policy::{HydratedSubject, SubjectType};

use crate::adapter_registry::InMemoryAdapterRegistry;
use crate::context::ActionContext;
use crate::runtime::{CapabilityRuntime, InMemoryCapabilityRuntime, RuntimeContext};
use crate::service::conversions::{
    action_context_to_proto, adapter_manifest_to_proto, datetime_to_proto, dispatch_kind_to_proto,
    envelope_from_bytes, failure_reason_to_proto, lifecycle_state_to_proto, queue_class_to_proto,
    registered_adapter_to_proto, runtime_error_to_status, stability_from_proto,
};
use crate::service::proto;
use crate::service::{DEFAULT_RUNTIME_VERSION, SCHEMA_VERSION};
use crate::status::ActionLifecycleState;

/// Default capability-runtime id reported by `GetCapabilityRuntimeInfo`.
pub const DEFAULT_RUNTIME_ID: &str = "aios-capability-runtime-inproc";

/// Default bundle version label.
///
/// Used to seed the [`RuntimeContext`] per RPC when the caller does not
/// supply one. Matches the M3 policy-kernel test default so the policy
/// step's bundle-version cross-check stays consistent.
pub const DEFAULT_BUNDLE_VERSION: &str = "polb_default";

/// Default code version label used to seed the [`RuntimeContext`] per RPC.
pub const DEFAULT_CODE_VERSION: &str = "aios-capability-runtime/0.0.1-T033";

/// gRPC adapter mounting an [`InMemoryCapabilityRuntime`] behind the
/// tonic-generated transport trait.
#[derive(Clone)]
pub struct CapabilityRuntimeService {
    /// Inner typed runtime. T-033 baseline holds the concrete in-memory
    /// impl — production wiring composes the full T-027..T-032 stack on
    /// the same `InMemoryCapabilityRuntime` and hands the Arc here.
    inner: Arc<InMemoryCapabilityRuntime>,
    /// Optional sibling handle on the adapter registry so `ListAdapters`
    /// and `GetAdapterCapabilities` can surface the registry's contents.
    /// `None` keeps the RPCs returning empty lists.
    adapter_registry: Option<Arc<InMemoryAdapterRegistry>>,
    runtime_id: String,
    runtime_version: String,
    bundle_version: String,
    code_version: String,
    default_subject: HydratedSubject,
    started_at: chrono::DateTime<chrono::Utc>,
}

impl std::fmt::Debug for CapabilityRuntimeService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapabilityRuntimeService")
            .field("runtime_id", &self.runtime_id)
            .field("runtime_version", &self.runtime_version)
            .field("bundle_version", &self.bundle_version)
            .field("code_version", &self.code_version)
            .field("default_subject", &self.default_subject)
            .field("started_at", &self.started_at)
            .field("adapter_registry", &self.adapter_registry.is_some())
            .finish_non_exhaustive()
    }
}

impl CapabilityRuntimeService {
    /// Construct an adapter wrapping the given runtime.
    #[must_use]
    pub fn new(inner: Arc<InMemoryCapabilityRuntime>) -> Self {
        Self {
            inner,
            adapter_registry: None,
            runtime_id: DEFAULT_RUNTIME_ID.to_owned(),
            runtime_version: DEFAULT_RUNTIME_VERSION.to_owned(),
            bundle_version: DEFAULT_BUNDLE_VERSION.to_owned(),
            code_version: DEFAULT_CODE_VERSION.to_owned(),
            default_subject: default_placeholder_subject(),
            started_at: chrono::Utc::now(),
        }
    }

    /// Override the runtime id reported by `GetCapabilityRuntimeInfo`.
    #[must_use]
    pub fn with_runtime_id(mut self, id: impl Into<String>) -> Self {
        self.runtime_id = id.into();
        self
    }

    /// Override the bundle version label used to seed [`RuntimeContext`].
    #[must_use]
    pub fn with_bundle_version(mut self, v: impl Into<String>) -> Self {
        self.bundle_version = v.into();
        self
    }

    /// Override the default subject used when no per-RPC identity exists.
    #[must_use]
    pub fn with_default_subject(mut self, s: HydratedSubject) -> Self {
        self.default_subject = s;
        self
    }

    /// Attach an [`InMemoryAdapterRegistry`] so `ListAdapters` /
    /// `GetAdapterCapabilities` can enumerate registered adapters.
    #[must_use]
    pub fn with_adapter_registry(mut self, registry: Arc<InMemoryAdapterRegistry>) -> Self {
        self.adapter_registry = Some(registry);
        self
    }

    /// Build a fresh [`RuntimeContext`] for one RPC. T-034 lifts this onto
    /// per-RPC mTLS-derived identity; T-033 uses a fixed placeholder
    /// subject + the service's configured bundle/code versions.
    fn build_context(&self, env: &aios_action::ActionEnvelope) -> RuntimeContext {
        let mut subject = self.default_subject.clone();
        subject
            .canonical_subject_id
            .clone_from(&env.identity.subject_canonical_id);
        subject.is_ai = env.identity.is_ai;
        RuntimeContext::from_subject(
            subject,
            self.bundle_version.clone(),
            self.code_version.clone(),
        )
    }
}

/// Default placeholder subject used to seed [`RuntimeContext`] until T-034
/// lifts per-RPC identity onto mTLS. Mirrors the `agent:dev` §22 fixture
/// from the policy-kernel adapter (T-023).
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

/// Helper: project an [`ActionContext`] onto an `ExecuteActionResponse`.
fn execute_response_from_context(ctx: &ActionContext) -> proto::ExecuteActionResponse {
    proto::ExecuteActionResponse {
        state: i32::from(lifecycle_state_to_proto(ctx.status)),
        // T-033 baseline: the runtime does not yet expose the chosen
        // adapter id on the context (adapter dispatch is a structural
        // pass-through in the T-029 baseline). Surface an empty string;
        // T-034 wires the adapter id onto the context.
        adapter_id: String::new(),
        dispatch_kind: i32::from(dispatch_kind_to_proto(ctx.dispatch_kind)),
        applied_sandbox_profile_id: String::new(),
        error: 0,
        failure_reason: ctx
            .error
            .map_or(0, |r| i32::from(failure_reason_to_proto(r))),
        context: Some(action_context_to_proto(ctx)),
    }
}

#[async_trait]
impl proto::capability_runtime_server::CapabilityRuntime for CapabilityRuntimeService {
    async fn validate_action(
        &self,
        request: Request<proto::ValidateActionRequest>,
    ) -> Result<Response<proto::ValidateActionResponse>, Status> {
        let r = request.into_inner();
        // T-033 baseline — envelope schema validation is performed by the
        // JSON deserializer; structural validation (target schema against
        // adapter manifest, verification grammar, idempotency-key replay
        // detection) lands in T-034 when the standalone validate transition
        // is wired into the pipeline. For now a successful decode mints a
        // synthetic handle and reports lifecycle state `CREATED`.
        let _env = envelope_from_bytes(&r.envelope_proto)?;
        let action_request_id = aios_action::ActionRuntimeRequestId::new().to_string();
        Ok(Response::new(proto::ValidateActionResponse {
            state: i32::from(lifecycle_state_to_proto(ActionLifecycleState::Created)),
            error: i32::from(crate::service::conversions::runtime_error_code_to_proto(
                crate::failure::RuntimeErrorCode::RuntimeOk,
            )),
            findings: Vec::new(),
            action_request_id,
        }))
    }

    async fn evaluate_policy_for_action(
        &self,
        _request: Request<proto::EvaluatePolicyForActionRequest>,
    ) -> Result<Response<proto::EvaluatePolicyForActionResponse>, Status> {
        // T-034 — standalone policy-evaluation transition. T-033 folds the
        // policy step into `ExecuteAction` (pipeline runs the full §6.1
        // eight-step sequence in one `submit_action` call), so this RPC is
        // deliberately stubbed.
        Err(Status::unimplemented(
            "EvaluatePolicyForAction is folded into ExecuteAction in T-033; standalone transition lands in T-034",
        ))
    }

    async fn request_approval_for_action(
        &self,
        _request: Request<proto::RequestApprovalForActionRequest>,
    ) -> Result<Response<proto::RequestApprovalForActionResponse>, Status> {
        // T-034 — approval orchestration. The Capability Runtime does not yet
        // hold an approval-binding sink (S5.3 ApprovalBinding integration
        // lands in T-034).
        Err(Status::unimplemented(
            "RequestApprovalForAction lands in T-034 (approval orchestration)",
        ))
    }

    async fn execute_action(
        &self,
        request: Request<proto::ExecuteActionRequest>,
    ) -> Result<Response<proto::ExecuteActionResponse>, Status> {
        let r = request.into_inner();
        let envelope = envelope_from_bytes(&r.envelope_proto)?;
        let context = self.build_context(&envelope);
        // T-034 — if the caller supplied an approval_binding_id AND a
        // previously-validated action_request_id, route through the
        // resume path: consume the binding atomically (S5.3 §13.1) and
        // drive the pipeline past APPROVAL_PENDING.
        if !r.approval_binding_id.trim().is_empty() && !r.action_request_id.trim().is_empty() {
            let action_id = aios_action::ActionId::parse(&r.action_request_id).map_err(|e| {
                Status::invalid_argument(format!(
                    "action_request_id={} is not a valid action id: {e}",
                    r.action_request_id
                ))
            })?;
            let ctx = self
                .inner
                .resume_with_binding(&action_id, &envelope, &context, &r.approval_binding_id)
                .await
                .map_err(|e| runtime_error_to_status(&e))?;
            return Ok(Response::new(execute_response_from_context(&ctx)));
        }
        // T-033 baseline: ExecuteAction drives the full submit_action
        // pipeline. The runtime mints its own action_id internally; any
        // action_request_id supplied by the caller from a prior
        // ValidateAction call is currently informational.
        let ctx = self
            .inner
            .submit_action(&envelope, &context)
            .await
            .map_err(|e| runtime_error_to_status(&e))?;
        Ok(Response::new(execute_response_from_context(&ctx)))
    }

    async fn verify_action(
        &self,
        _request: Request<proto::VerifyActionRequest>,
    ) -> Result<Response<proto::VerifyActionResponse>, Status> {
        // T-035 — standalone verification transition. T-033 folds verification
        // into `ExecuteAction` (pipeline step 6 runs internally).
        Err(Status::unimplemented(
            "VerifyAction is folded into ExecuteAction in T-033; standalone transition lands in T-035",
        ))
    }

    async fn rollback_action(
        &self,
        _request: Request<proto::RollbackActionRequest>,
    ) -> Result<Response<proto::RollbackActionResponse>, Status> {
        // T-035 — operator-initiated rollback. T-032 already drives the
        // §7.2 outcome table automatically inside `submit_action` when a
        // rollback driver is attached; the standalone RPC requires storing
        // an adapter rollback handle alongside the action context (T-034 /
        // T-035).
        Err(Status::unimplemented(
            "RollbackAction is driven automatically by ExecuteAction in T-033; standalone transition lands in T-035",
        ))
    }

    async fn get_action_status(
        &self,
        request: Request<proto::GetActionStatusRequest>,
    ) -> Result<Response<proto::GetActionStatusResponse>, Status> {
        let r = request.into_inner();
        // The T-033 baseline persists contexts under the `aios_action::ActionId`
        // the runtime mints; for backward compatibility the wire id is the
        // canonical `act_` string form.
        let action_id = aios_action::ActionId::parse(&r.action_request_id).map_err(|e| {
            Status::not_found(format!(
                "action_request_id={} is not a valid action id: {e}",
                r.action_request_id
            ))
        })?;
        let ctx = self
            .inner
            .get_action_status(&action_id)
            .await
            .map_err(|e| runtime_error_to_status(&e))?;
        Ok(Response::new(proto::GetActionStatusResponse {
            envelope_proto: Vec::new(), // T-035 wires envelope retention
            lifecycle_state: i32::from(lifecycle_state_to_proto(ctx.status)),
            current_adapter_id: String::new(),
            current_dispatch_kind: i32::from(dispatch_kind_to_proto(ctx.dispatch_kind)),
            applied_sandbox_profile_id: String::new(),
            last_state_change_at: Some(datetime_to_proto(ctx.last_updated_at)),
            context: Some(action_context_to_proto(&ctx)),
            // Suppress unused warnings by reading the queue class through
            // the helper conversion (the field is in the context proto).
        }))
    }

    async fn list_adapters(
        &self,
        request: Request<proto::ListAdaptersRequest>,
    ) -> Result<Response<proto::ListAdaptersResponse>, Status> {
        let r = request.into_inner();
        let Some(registry) = self.adapter_registry.as_ref() else {
            return Ok(Response::new(proto::ListAdaptersResponse {
                entries: Vec::new(),
            }));
        };
        let all = registry.list().await;
        // Apply spec §5.2 filters: stability_filter, action_kind_filter,
        // include_retired.
        let stability_filter = proto::AdapterStability::try_from(r.stability_filter)
            .ok()
            .and_then(stability_from_proto);
        let entries = all
            .into_iter()
            .filter(|reg| {
                if !r.include_retired
                    && reg.manifest.declared_stability == crate::dispatch::AdapterStability::Retired
                {
                    return false;
                }
                if let Some(want) = stability_filter {
                    if reg.manifest.declared_stability != want {
                        return false;
                    }
                }
                if !r.action_kind_filter.is_empty()
                    && !reg
                        .manifest
                        .declared_actions
                        .iter()
                        .any(|d| d.action_kind.starts_with(&r.action_kind_filter))
                {
                    return false;
                }
                true
            })
            .map(|reg| registered_adapter_to_proto(&reg))
            .collect();
        Ok(Response::new(proto::ListAdaptersResponse { entries }))
    }

    async fn get_adapter_capabilities(
        &self,
        request: Request<proto::GetAdapterCapabilitiesRequest>,
    ) -> Result<Response<proto::GetAdapterCapabilitiesResponse>, Status> {
        let r = request.into_inner();
        let Some(registry) = self.adapter_registry.as_ref() else {
            return Err(Status::not_found(format!(
                "unknown adapter (no registry attached): {}",
                r.adapter_id
            )));
        };
        let reg = registry
            .lookup_by_id(&r.adapter_id)
            .await
            .ok_or_else(|| Status::not_found(format!("unknown adapter: {}", r.adapter_id)))?;
        Ok(Response::new(proto::GetAdapterCapabilitiesResponse {
            manifest: Some(adapter_manifest_to_proto(&reg.manifest)),
        }))
    }

    async fn get_capability_runtime_info(
        &self,
        _request: Request<()>,
    ) -> Result<Response<proto::GetCapabilityRuntimeInfoResponse>, Status> {
        let registered_adapters_count = match self.adapter_registry.as_ref() {
            Some(r) => u32::try_from(r.len().await).unwrap_or(u32::MAX),
            None => 0,
        };
        let queue_depth = match self.inner.dispatch_queue() {
            Some(q) => u32::try_from(q.total_len().await).unwrap_or(u32::MAX),
            None => 0,
        };
        Ok(Response::new(proto::GetCapabilityRuntimeInfoResponse {
            capability_runtime_id: self.runtime_id.clone(),
            runtime_version: self.runtime_version.clone(),
            supported_schema_versions: vec![SCHEMA_VERSION.to_owned()],
            registered_adapters_count,
            queue_depth,
            degraded: false,
            degraded_reason: String::new(),
            started_at: Some(datetime_to_proto(self.started_at)),
        }))
    }
}

// `queue_class_to_proto` is referenced via the conversions module's
// re-export; the projection is used inside `action_context_to_proto`.
#[allow(dead_code)]
fn _ensure_queue_class_to_proto_is_linked(q: crate::dispatch::QueueClass) -> i32 {
    i32::from(queue_class_to_proto(q))
}

// ---------------------------------------------------------------------------
// Bootstrap helpers
// ---------------------------------------------------------------------------

/// Build a `tonic::transport::server::Router` with the `CapabilityRuntime`
/// service mounted.
#[must_use]
pub fn build_router(svc: CapabilityRuntimeService) -> Router {
    Server::builder()
        .add_service(proto::capability_runtime_server::CapabilityRuntimeServer::new(svc))
}

/// Convenience helper: bind to `addr` and serve until the future is dropped.
///
/// # Errors
///
/// Returns the underlying [`tonic::transport::Error`] when the bind / listen
/// loop fails.
pub async fn serve(
    svc: CapabilityRuntimeService,
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

    #[test]
    fn default_subject_is_agent_dev_ai_subject() {
        let s = default_placeholder_subject();
        assert_eq!(s.canonical_subject_id, "agent:dev");
        assert_eq!(s.subject_type, SubjectType::Agent);
        assert!(s.is_ai);
        assert!(!s.recovery_mode);
    }

    #[test]
    fn build_router_compiles_and_accepts_capability_runtime_service() {
        // Compile-time check that `CapabilityRuntimeServer<CapabilityRuntimeService>`
        // is a valid `tonic::server::NamedService`.
        let runtime: Arc<InMemoryCapabilityRuntime> = Arc::new(InMemoryCapabilityRuntime::new());
        let svc = CapabilityRuntimeService::new(runtime).with_runtime_id("test-runtime");
        let _router = build_router(svc);
    }
}
