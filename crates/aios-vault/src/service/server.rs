//! gRPC `VaultBroker` server adapter + bootstrap helpers (T-052).

#![allow(clippy::result_large_err)]

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use tonic::transport::server::{Router, Server};
use tonic::{Request, Response, Status};

use crate::broker::{
    UseCapabilityRequest as RustUseCapabilityRequest, VaultBroker as RustVaultBroker,
};
use crate::identity::SubjectRef;
use crate::override_broker::{GrantOverrideRequest as RustGrantOverrideRequest, OverrideBroker};
use crate::service::conversions::{
    audit_entry_to_proto, datetime_to_proto, expiration_report_to_proto,
    issue_capability_request_from_proto, override_binding_to_proto, override_class_from_proto,
    parse_capability_id, required_datetime_from_proto, session_to_proto, subject_from_proto,
    subject_to_proto, target_action_id_from_proto, use_capability_result_to_proto,
    vault_capability_to_proto, vault_error_to_status, vault_operation_from_proto,
};
use crate::service::proto;
use crate::service::SCHEMA_VERSION;
use crate::{
    CapabilityAuditLog, CapabilityLifecycleDriver, IdentityCatalog, InMemoryOverrideBroker,
    InMemoryVaultBroker,
};

/// Default `VaultBroker` service id reported by the info RPC.
pub const DEFAULT_VAULT_ID: &str = "aios-vault-inproc";

/// Default Rust crate code version for T-052 service wiring.
pub const DEFAULT_CODE_VERSION: &str = "aios-vault/0.0.1-T052";

/// gRPC adapter mounting the in-memory vault stack behind tonic.
#[derive(Clone, Debug)]
pub struct VaultBrokerService {
    vault: Arc<InMemoryVaultBroker>,
    overrides: Arc<InMemoryOverrideBroker>,
    identity: Arc<IdentityCatalog>,
    audit: Arc<CapabilityAuditLog>,
    lifecycle: Arc<CapabilityLifecycleDriver>,
    vault_id: String,
    code_version: String,
    started_at: chrono::DateTime<chrono::Utc>,
}

impl VaultBrokerService {
    /// Construct an adapter over the in-memory vault components.
    #[must_use]
    pub fn new(
        vault: Arc<InMemoryVaultBroker>,
        overrides: Arc<InMemoryOverrideBroker>,
        identity: Arc<IdentityCatalog>,
        audit: Arc<CapabilityAuditLog>,
        lifecycle: Arc<CapabilityLifecycleDriver>,
    ) -> Self {
        Self {
            vault,
            overrides,
            identity,
            audit,
            lifecycle,
            vault_id: DEFAULT_VAULT_ID.to_owned(),
            code_version: DEFAULT_CODE_VERSION.to_owned(),
            started_at: chrono::Utc::now(),
        }
    }

    /// Override the vault id reported by `GetVaultInfo`.
    #[must_use]
    pub fn with_vault_id(mut self, id: impl Into<String>) -> Self {
        self.vault_id = id.into();
        self
    }

    /// Override the code version reported by `GetVaultInfo`.
    #[must_use]
    pub fn with_code_version(mut self, version: impl Into<String>) -> Self {
        self.code_version = version.into();
        self
    }

    async fn active_capability_count(&self) -> u64 {
        let store = self.vault.capabilities.read().await;
        u64::try_from(
            store
                .values()
                .filter(|(capability, _key_material)| {
                    capability.state == crate::CapabilityState::Active
                })
                .count(),
        )
        .unwrap_or(u64::MAX)
    }
}

#[async_trait]
impl proto::vault_broker_server::VaultBroker for VaultBrokerService {
    async fn issue_capability(
        &self,
        request: Request<proto::IssueCapabilityRequest>,
    ) -> Result<Response<proto::IssueCapabilityResponse>, Status> {
        let request = issue_capability_request_from_proto(request.into_inner())?;
        let capability = self
            .vault
            .issue_capability(request)
            .await
            .map_err(|err| vault_error_to_status(&err))?;
        Ok(Response::new(proto::IssueCapabilityResponse {
            capability: Some(vault_capability_to_proto(&capability)),
        }))
    }

    async fn use_capability(
        &self,
        request: Request<proto::UseCapabilityRequest>,
    ) -> Result<Response<proto::UseCapabilityResponse>, Status> {
        let request = request.into_inner();
        let capability_id = parse_capability_id(&request.capability_id)?;
        let operation = vault_operation_from_proto(request.operation)?;
        let result = self
            .vault
            .use_capability(RustUseCapabilityRequest {
                capability_id,
                operation,
            })
            .await
            .map_err(|err| vault_error_to_status(&err))?;
        Ok(Response::new(proto::UseCapabilityResponse {
            result: Some(use_capability_result_to_proto(&result)),
        }))
    }

    async fn list_capabilities(
        &self,
        request: Request<proto::ListCapabilitiesRequest>,
    ) -> Result<Response<proto::ListCapabilitiesResponse>, Status> {
        let request = request.into_inner();
        let subject = SubjectRef(request.subject);
        let capabilities = self
            .vault
            .list_capabilities(&subject)
            .await
            .map_err(|err| vault_error_to_status(&err))?;
        Ok(Response::new(proto::ListCapabilitiesResponse {
            capabilities: capabilities.iter().map(vault_capability_to_proto).collect(),
        }))
    }

    async fn revoke_capability(
        &self,
        request: Request<proto::RevokeCapabilityRequest>,
    ) -> Result<Response<proto::RevokeCapabilityResponse>, Status> {
        let request = request.into_inner();
        let capability_id = parse_capability_id(&request.capability_id)?;
        let revoked_by = SubjectRef(request.revoked_by);
        self.vault
            .revoke_capability(&capability_id, &revoked_by)
            .await
            .map_err(|err| vault_error_to_status(&err))?;
        Ok(Response::new(proto::RevokeCapabilityResponse {
            capability_id: capability_id.to_string(),
            state: i32::from(crate::service::conversions::capability_state_to_proto(
                crate::CapabilityState::Revoked,
            )),
        }))
    }

    async fn grant_override(
        &self,
        request: Request<proto::GrantOverrideRequest>,
    ) -> Result<Response<proto::GrantOverrideResponse>, Status> {
        let request = request.into_inner();
        let class = proto::OverrideClass::try_from(request.class)
            .map_err(|_| {
                Status::invalid_argument(format!("unknown override class {}", request.class))
            })
            .and_then(override_class_from_proto)?;
        let expires_at = required_datetime_from_proto(request.expires_at, "expires_at")?;
        let target_action_id = target_action_id_from_proto(request.target_action_id_proto)?;
        let binding = self
            .overrides
            .grant_override(RustGrantOverrideRequest {
                class,
                granted_by: request.granted_by.into_iter().map(SubjectRef).collect(),
                target_action_id,
                expires_at,
                reason: request.reason,
            })
            .await
            .map_err(|err| vault_error_to_status(&err))?;
        Ok(Response::new(proto::GrantOverrideResponse {
            binding: Some(override_binding_to_proto(&binding)),
        }))
    }

    async fn consume_override(
        &self,
        request: Request<proto::ConsumeOverrideRequest>,
    ) -> Result<Response<proto::ConsumeOverrideResponse>, Status> {
        let request = request.into_inner();
        let binding = self
            .overrides
            .consume_override(&request.binding_id, &SubjectRef(request.consumer))
            .await
            .map_err(|err| vault_error_to_status(&err))?;
        Ok(Response::new(proto::ConsumeOverrideResponse {
            binding: Some(override_binding_to_proto(&binding)),
        }))
    }

    async fn revoke_override(
        &self,
        request: Request<proto::RevokeOverrideRequest>,
    ) -> Result<Response<proto::RevokeOverrideResponse>, Status> {
        let request = request.into_inner();
        self.overrides
            .revoke_override(&request.binding_id, &SubjectRef(request.revoker))
            .await
            .map_err(|err| vault_error_to_status(&err))?;
        Ok(Response::new(proto::RevokeOverrideResponse {
            binding_id: request.binding_id,
            state: i32::from(crate::service::conversions::override_state_to_proto(
                crate::OverrideBindingState::Revoked,
            )),
        }))
    }

    async fn lookup_override(
        &self,
        request: Request<proto::LookupOverrideRequest>,
    ) -> Result<Response<proto::LookupOverrideResponse>, Status> {
        let request = request.into_inner();
        let binding = self
            .overrides
            .lookup_override(&request.binding_id)
            .await
            .map_err(|err| vault_error_to_status(&err))?;
        Ok(Response::new(proto::LookupOverrideResponse {
            binding: Some(override_binding_to_proto(&binding)),
        }))
    }

    async fn list_overrides_for_subject(
        &self,
        request: Request<proto::ListOverridesForSubjectRequest>,
    ) -> Result<Response<proto::ListOverridesForSubjectResponse>, Status> {
        let request = request.into_inner();
        let bindings = self
            .overrides
            .list_overrides_for_subject(&SubjectRef(request.subject))
            .await
            .map_err(|err| vault_error_to_status(&err))?;
        Ok(Response::new(proto::ListOverridesForSubjectResponse {
            bindings: bindings.iter().map(override_binding_to_proto).collect(),
        }))
    }

    async fn register_subject(
        &self,
        request: Request<proto::RegisterSubjectRequest>,
    ) -> Result<Response<proto::RegisterSubjectResponse>, Status> {
        let request = request.into_inner();
        let subject = request.subject.map_or_else(
            || Err(Status::invalid_argument("subject is required")),
            subject_from_proto,
        )?;
        self.identity
            .register_subject(subject.clone())
            .await
            .map_err(|err| vault_error_to_status(&err))?;
        Ok(Response::new(proto::RegisterSubjectResponse {
            subject: Some(subject_to_proto(&subject)),
        }))
    }

    async fn lookup_subject(
        &self,
        request: Request<proto::LookupSubjectRequest>,
    ) -> Result<Response<proto::LookupSubjectResponse>, Status> {
        let request = request.into_inner();
        let subject = self
            .identity
            .lookup_subject(&request.canonical_subject_id)
            .await
            .map_err(|err| vault_error_to_status(&err))?;
        Ok(Response::new(proto::LookupSubjectResponse {
            subject: Some(subject_to_proto(&subject)),
        }))
    }

    async fn start_session(
        &self,
        request: Request<proto::StartSessionRequest>,
    ) -> Result<Response<proto::StartSessionResponse>, Status> {
        let request = request.into_inner();
        let expires_at = required_datetime_from_proto(request.expires_at, "expires_at")?;
        let session = self
            .identity
            .start_session(&request.subject_id, expires_at)
            .await
            .map_err(|err| vault_error_to_status(&err))?;
        Ok(Response::new(proto::StartSessionResponse {
            session: Some(session_to_proto(&session)),
        }))
    }

    async fn lookup_session(
        &self,
        request: Request<proto::LookupSessionRequest>,
    ) -> Result<Response<proto::LookupSessionResponse>, Status> {
        let request = request.into_inner();
        let session = self
            .identity
            .lookup_session(&request.session_id)
            .await
            .map_err(|err| vault_error_to_status(&err))?;
        Ok(Response::new(proto::LookupSessionResponse {
            session: Some(session_to_proto(&session)),
        }))
    }

    async fn suspend_session(
        &self,
        request: Request<proto::SuspendSessionRequest>,
    ) -> Result<Response<proto::SuspendSessionResponse>, Status> {
        let request = request.into_inner();
        self.identity
            .suspend_session(&request.session_id)
            .await
            .map_err(|err| vault_error_to_status(&err))?;
        let session = self
            .identity
            .lookup_session(&request.session_id)
            .await
            .map_err(|err| vault_error_to_status(&err))?;
        Ok(Response::new(proto::SuspendSessionResponse {
            session: Some(session_to_proto(&session)),
        }))
    }

    async fn revoke_session(
        &self,
        request: Request<proto::RevokeSessionRequest>,
    ) -> Result<Response<proto::RevokeSessionResponse>, Status> {
        let request = request.into_inner();
        self.identity
            .revoke_session(&request.session_id)
            .await
            .map_err(|err| vault_error_to_status(&err))?;
        let session = self
            .identity
            .lookup_session(&request.session_id)
            .await
            .map_err(|err| vault_error_to_status(&err))?;
        Ok(Response::new(proto::RevokeSessionResponse {
            session: Some(session_to_proto(&session)),
        }))
    }

    async fn get_audit_entry(
        &self,
        request: Request<proto::GetAuditEntryRequest>,
    ) -> Result<Response<proto::GetAuditEntryResponse>, Status> {
        let request = request.into_inner();
        let capability_id = parse_capability_id(&request.capability_id)?;
        let entry = self
            .audit
            .lookup(&capability_id)
            .ok_or_else(|| Status::not_found(format!("audit entry not found: {capability_id}")))?;
        Ok(Response::new(proto::GetAuditEntryResponse {
            entry: Some(audit_entry_to_proto(&entry)),
        }))
    }

    async fn list_audit_entries(
        &self,
        _request: Request<proto::ListAuditEntriesRequest>,
    ) -> Result<Response<proto::ListAuditEntriesResponse>, Status> {
        let entries = self.audit.list_all();
        Ok(Response::new(proto::ListAuditEntriesResponse {
            entries: entries.iter().map(audit_entry_to_proto).collect(),
        }))
    }

    async fn run_expiration_pass(
        &self,
        request: Request<proto::RunExpirationPassRequest>,
    ) -> Result<Response<proto::RunExpirationPassResponse>, Status> {
        let request = request.into_inner();
        let now = required_datetime_from_proto(request.now, "now")?;
        let report = self
            .lifecycle
            .run_expiration_pass(now)
            .await
            .map_err(|err| vault_error_to_status(&err))?;
        Ok(Response::new(proto::RunExpirationPassResponse {
            report: Some(expiration_report_to_proto(&report)),
        }))
    }

    async fn get_vault_info(
        &self,
        _request: Request<proto::GetVaultInfoRequest>,
    ) -> Result<Response<proto::GetVaultInfoResponse>, Status> {
        Ok(Response::new(proto::GetVaultInfoResponse {
            schema_version: SCHEMA_VERSION.to_owned(),
            code_version: self.code_version.clone(),
            vault_id: self.vault_id.clone(),
            audit_entry_count: u64::try_from(self.audit.list_all().len()).unwrap_or(u64::MAX),
            active_capability_count: self.active_capability_count().await,
            started_at: Some(datetime_to_proto(self.started_at)),
        }))
    }
}

/// Build a `tonic::transport::server::Router` with the `VaultBroker` service
/// mounted.
#[must_use]
pub fn build_router(svc: VaultBrokerService) -> Router {
    Server::builder().add_service(proto::vault_broker_server::VaultBrokerServer::new(svc))
}

/// Convenience helper: bind to `addr` and serve until the future is dropped.
///
/// # Errors
///
/// Returns the underlying [`tonic::transport::Error`] when the bind / listen
/// loop fails.
pub async fn serve(
    svc: VaultBrokerService,
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
    fn build_router_compiles_and_accepts_service() {
        let audit = Arc::new(CapabilityAuditLog::new());
        let vault = Arc::new(InMemoryVaultBroker::new().with_audit_log(Arc::clone(&audit)));
        let identity = Arc::new(IdentityCatalog::with_fixtures());
        let overrides = Arc::new(InMemoryOverrideBroker::new(Arc::clone(&identity)));
        let lifecycle = Arc::new(CapabilityLifecycleDriver::new(
            Arc::clone(&vault),
            Arc::clone(&audit),
        ));
        let svc = VaultBrokerService::new(vault, overrides, identity, audit, lifecycle);
        let _router = build_router(svc);
    }

    #[test]
    fn default_labels_are_populated() {
        let audit = Arc::new(CapabilityAuditLog::new());
        let vault = Arc::new(InMemoryVaultBroker::new().with_audit_log(Arc::clone(&audit)));
        let identity = Arc::new(IdentityCatalog::with_fixtures());
        let overrides = Arc::new(InMemoryOverrideBroker::new(Arc::clone(&identity)));
        let lifecycle = Arc::new(CapabilityLifecycleDriver::new(
            Arc::clone(&vault),
            Arc::clone(&audit),
        ));
        let svc = VaultBrokerService::new(vault, overrides, identity, audit, lifecycle);

        assert_eq!(svc.vault_id, DEFAULT_VAULT_ID);
        assert_eq!(svc.code_version, DEFAULT_CODE_VERSION);
    }

    #[tokio::test]
    async fn active_capability_count_starts_empty() {
        let audit = Arc::new(CapabilityAuditLog::new());
        let vault = Arc::new(InMemoryVaultBroker::new().with_audit_log(Arc::clone(&audit)));
        let identity = Arc::new(IdentityCatalog::with_fixtures());
        let overrides = Arc::new(InMemoryOverrideBroker::new(Arc::clone(&identity)));
        let lifecycle = Arc::new(CapabilityLifecycleDriver::new(
            Arc::clone(&vault),
            Arc::clone(&audit),
        ));
        let svc = VaultBrokerService::new(vault, overrides, identity, audit, lifecycle);

        assert_eq!(svc.active_capability_count().await, 0);
    }
}
