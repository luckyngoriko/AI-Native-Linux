//! gRPC `AppsService` server adapter + bootstrap helpers (T-122).
//!
//! [`AppsServer`] mounts the five L6 backing drivers behind the tonic-generated
//! `apps_service_server::AppsService` transport trait. Each RPC method:
//!
//! 1. Converts the proto request into Rust domain types via [`super::conversions`].
//! 2. Calls the backing driver.
//! 3. Converts the Rust response back into a proto message.
//! 4. Maps [`AppsError`] → [`tonic::Status`] via [`apps_error_to_status`].
//!
//! ## Backing drivers
//!
//! | Driver                       | RPCs                                      |
//! | ---------------------------- | ----------------------------------------- |
//! | `PackageStore`               | RegisterPackage, GetPackage, ListPackages |
//! | `SessionDriver`              | OpenSession, CloseSession, ListSessions   |
//! | `UpdateRollbackDriver`       | PlanUpdate, ExecuteUpdate, VerifyUpdate, ActivateUpdate, RollbackUpdate |
//! | `CompatibilityKnowledgeDB`   | LookupCompatibilityProfile               |

#![allow(clippy::result_large_err)]

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use tonic::transport::server::{Router, Server};
use tonic::{Request, Response, Status};

use crate::compatibility_orchestrator::CompatibilityOrchestrator;
use crate::knowledge_db::CompatibilityKnowledgeDB;
use crate::package::PackageId;
use crate::package_store::PackageStore;
use crate::service::conversions::{
    app_package_from_proto, app_package_to_proto, app_profile_to_proto, apps_error_to_status,
    ecosystem_runtime_from_proto, rollback_receipt_to_proto, session_descriptor_to_proto,
    session_filter_from_proto, session_termination_receipt_to_proto, update_outcome_to_proto,
    update_plan_to_proto, update_verification_to_proto,
};
use crate::service::proto;
use crate::session::SessionId;
use crate::session_driver::{OpenSessionRequest, Principal, SessionDriver, SessionFilter};
use crate::update_driver::{RollbackReason, UpdatePlanRequest, UpdateRollbackDriver};

/// Mounts the five L6 backing drivers behind the gRPC `AppsService` trait.
#[derive(Clone)]
pub struct AppsServer {
    store: Arc<dyn PackageStore>,
    knowledge: Arc<CompatibilityKnowledgeDB>,
    sessions: Arc<dyn SessionDriver>,
    updates: Arc<dyn UpdateRollbackDriver>,
    #[allow(dead_code)]
    orchestrator: Arc<CompatibilityOrchestrator>,
}

impl std::fmt::Debug for AppsServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppsServer").finish_non_exhaustive()
    }
}

impl AppsServer {
    /// Construct a server mounting all five drivers.
    #[must_use]
    pub fn new(
        store: Arc<dyn PackageStore>,
        knowledge: Arc<CompatibilityKnowledgeDB>,
        sessions: Arc<dyn SessionDriver>,
        updates: Arc<dyn UpdateRollbackDriver>,
        orchestrator: Arc<CompatibilityOrchestrator>,
    ) -> Self {
        Self {
            store,
            knowledge,
            sessions,
            updates,
            orchestrator,
        }
    }
}

#[async_trait]
impl proto::apps_service_server::AppsService for AppsServer {
    // ------------------------------------------------------------------
    // Package Store RPCs
    // ------------------------------------------------------------------

    async fn register_package(
        &self,
        request: Request<proto::RegisterPackageRequest>,
    ) -> Result<Response<proto::RegisterPackageResponse>, Status> {
        let r = request.into_inner();
        let pkg_proto = r
            .package
            .ok_or_else(|| Status::invalid_argument("package field is required"))?;
        let package = app_package_from_proto(&pkg_proto);
        let package_id = self
            .store
            .register_package(package)
            .await
            .map_err(|e| apps_error_to_status(&e))?;
        Ok(Response::new(proto::RegisterPackageResponse {
            package_id: package_id.0,
        }))
    }

    async fn get_package(
        &self,
        request: Request<proto::GetPackageRequest>,
    ) -> Result<Response<proto::GetPackageResponse>, Status> {
        let r = request.into_inner();
        let id = PackageId(r.package_id);
        let package = self
            .store
            .lookup_package(&id)
            .await
            .map_err(|e| apps_error_to_status(&e))?;
        Ok(Response::new(proto::GetPackageResponse {
            package: Some(app_package_to_proto(&package)),
        }))
    }

    async fn list_packages(
        &self,
        _request: Request<()>,
    ) -> Result<Response<proto::ListPackagesResponse>, Status> {
        let packages = self
            .store
            .list_packages()
            .await
            .map_err(|e| apps_error_to_status(&e))?;
        let entries: Vec<proto::PackageEnvelopeProto> =
            packages.iter().map(app_package_to_proto).collect();
        Ok(Response::new(proto::ListPackagesResponse {
            packages: entries,
        }))
    }

    // ------------------------------------------------------------------
    // Session Driver RPCs
    // ------------------------------------------------------------------

    async fn open_session(
        &self,
        request: Request<proto::OpenSessionRequest>,
    ) -> Result<Response<proto::OpenSessionResponse>, Status> {
        let r = request.into_inner();
        let ecosystem = ecosystem_runtime_from_proto(
            proto::EcosystemRuntimeProto::try_from(r.ecosystem).map_err(|_| {
                Status::invalid_argument(format!("invalid ecosystem: {}", r.ecosystem))
            })?,
        )
        .ok_or_else(|| Status::invalid_argument("ecosystem is UNSPECIFIED"))?;

        let requester = match r.requester {
            Some(p) => Principal {
                canonical_id: p.canonical_id,
            },
            None => Principal {
                canonical_id: String::new(),
            },
        };

        let capability_grants = r
            .capability_grants
            .into_iter()
            .map(|c| crate::session_driver::CapabilityHandle {
                capability_id: c.capability_id,
            })
            .collect();

        let req = OpenSessionRequest {
            package_id: PackageId(r.package_id),
            ecosystem,
            requester,
            capability_grants,
            timeout: std::time::Duration::from_secs(r.timeout_seconds),
        };

        let session = self
            .sessions
            .open_session(req)
            .await
            .map_err(|e| apps_error_to_status(&e))?;

        Ok(Response::new(proto::OpenSessionResponse {
            session: Some(session_descriptor_to_proto(&session)),
        }))
    }

    async fn close_session(
        &self,
        request: Request<proto::CloseSessionRequest>,
    ) -> Result<Response<proto::CloseSessionResponse>, Status> {
        let r = request.into_inner();
        let id = SessionId(r.session_id);
        let receipt = self
            .sessions
            .close_session(id)
            .await
            .map_err(|e| apps_error_to_status(&e))?;
        Ok(Response::new(proto::CloseSessionResponse {
            receipt: Some(session_termination_receipt_to_proto(&receipt)),
        }))
    }

    async fn list_sessions(
        &self,
        request: Request<proto::ListSessionsRequest>,
    ) -> Result<Response<proto::ListSessionsResponse>, Status> {
        let r = request.into_inner();
        let filter = r
            .filter
            .as_ref()
            .map_or(SessionFilter::All, session_filter_from_proto);
        let sessions = self.sessions.list_sessions(filter).await;
        let entries: Vec<proto::SessionDescriptorProto> =
            sessions.iter().map(session_descriptor_to_proto).collect();
        Ok(Response::new(proto::ListSessionsResponse {
            sessions: entries,
        }))
    }

    // ------------------------------------------------------------------
    // Update Driver RPCs
    // ------------------------------------------------------------------

    async fn plan_update(
        &self,
        request: Request<proto::PlanUpdateRequest>,
    ) -> Result<Response<proto::PlanUpdateResponse>, Status> {
        let r = request.into_inner();
        let req = UpdatePlanRequest {
            package_id: PackageId(r.package_id),
            from_version: r.from_version,
            to_version: r.to_version,
            requester: Principal {
                canonical_id: r.requester,
            },
            dry_run: r.dry_run,
        };
        let plan = self
            .updates
            .plan_update(req)
            .await
            .map_err(|e| apps_error_to_status(&e))?;
        Ok(Response::new(proto::PlanUpdateResponse {
            plan: Some(update_plan_to_proto(&plan)),
        }))
    }

    async fn execute_update(
        &self,
        request: Request<proto::ExecuteUpdateRequest>,
    ) -> Result<Response<proto::ExecuteUpdateResponse>, Status> {
        let r = request.into_inner();
        let plan_id = crate::update_driver::UpdatePlanId(r.plan_id);
        let outcome = self
            .updates
            .execute_update(plan_id)
            .await
            .map_err(|e| apps_error_to_status(&e))?;
        Ok(Response::new(proto::ExecuteUpdateResponse {
            outcome: Some(update_outcome_to_proto(&outcome)),
        }))
    }

    async fn verify_update(
        &self,
        request: Request<proto::VerifyUpdateRequest>,
    ) -> Result<Response<proto::VerifyUpdateResponse>, Status> {
        let r = request.into_inner();
        let plan_id = crate::update_driver::UpdatePlanId(r.plan_id);
        let verification = self
            .updates
            .verify_update(plan_id)
            .await
            .map_err(|e| apps_error_to_status(&e))?;
        Ok(Response::new(proto::VerifyUpdateResponse {
            verification: Some(update_verification_to_proto(&verification)),
        }))
    }

    async fn activate_update(
        &self,
        request: Request<proto::ActivateUpdateRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let plan_id = crate::update_driver::UpdatePlanId(r.plan_id);
        self.updates
            .activate_update(plan_id)
            .await
            .map_err(|e| apps_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn rollback_update(
        &self,
        request: Request<proto::RollbackUpdateRequest>,
    ) -> Result<Response<proto::RollbackUpdateResponse>, Status> {
        let r = request.into_inner();
        let plan_id = crate::update_driver::UpdatePlanId(r.plan_id);
        let reason = match proto::RollbackReasonProto::try_from(r.reason) {
            Ok(proto::RollbackReasonProto::VerifyFailed) => RollbackReason::VerifyFailed,
            Ok(proto::RollbackReasonProto::PolicyRevoked) => RollbackReason::PolicyRevoked,
            Ok(proto::RollbackReasonProto::UserRequested) => RollbackReason::UserRequested,
            Ok(proto::RollbackReasonProto::RegressionDetected) => {
                RollbackReason::RegressionDetected
            }
            _ => {
                return Err(Status::invalid_argument(format!(
                    "invalid rollback reason: {}",
                    r.reason
                )));
            }
        };
        let receipt = self
            .updates
            .rollback_update(plan_id, reason)
            .await
            .map_err(|e| apps_error_to_status(&e))?;
        Ok(Response::new(proto::RollbackUpdateResponse {
            receipt: Some(rollback_receipt_to_proto(&receipt)),
        }))
    }

    // ------------------------------------------------------------------
    // Compatibility Knowledge DB RPC
    // ------------------------------------------------------------------

    async fn lookup_compatibility_profile(
        &self,
        request: Request<proto::LookupCompatibilityProfileRequest>,
    ) -> Result<Response<proto::LookupCompatibilityProfileResponse>, Status> {
        let r = request.into_inner();
        let package_id = PackageId(r.package_id);
        let profile = self.knowledge.lookup(&package_id).await.map_err(|_| {
            // Map PackageNotFound to a more specific ProfileNotFound status
            tonic::Status::not_found(format!(
                "compatibility profile not found for package_id: {}",
                package_id.0
            ))
        })?;
        Ok(Response::new(proto::LookupCompatibilityProfileResponse {
            profile: Some(app_profile_to_proto(&profile)),
        }))
    }
}

// ---------------------------------------------------------------------------
// Bootstrap helpers
// ---------------------------------------------------------------------------

/// Build a `tonic::transport::server::Router` with the `AppsService` mounted.
#[must_use]
pub fn build_router(svc: AppsServer) -> Router {
    Server::builder().add_service(proto::apps_service_server::AppsServiceServer::new(svc))
}

/// Convenience helper: bind to `addr` and serve until the future is dropped.
///
/// # Errors
///
/// Returns the underlying [`tonic::transport::Error`] when the bind / listen
/// loop fails.
pub async fn serve(svc: AppsServer, addr: SocketAddr) -> Result<(), tonic::transport::Error> {
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
    fn build_router_compiles() {
        let store = Arc::new(crate::package_store::InMemoryPackageStore::new(
            std::collections::HashMap::new(),
        ));
        let knowledge = Arc::new(CompatibilityKnowledgeDB::with_fixtures());
        let orchestrator = Arc::new(CompatibilityOrchestrator::new_with_defaults());
        let sessions = Arc::new(crate::session_driver::InMemorySessionDriver::new_with_defaults());
        let updates = Arc::new(crate::update_driver::InMemoryUpdateDriver::new());
        let svc = AppsServer::new(store, knowledge, sessions, updates, orchestrator);
        let _router = build_router(svc);
    }
}
