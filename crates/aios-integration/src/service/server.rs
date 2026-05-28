//! gRPC `IntegrationService` server adapter (T-183).
//!
//! [`IntegrationServer`] mounts the vendor registry, standards registry, CVE feed,
//! bridge registry, orchestrator, and control-map registry behind the
//! tonic-generated `IntegrationService` trait.
//!
//! Each RPC method:
//! 1. Converts the proto request into Rust domain types via [`super::conversions`].
//! 2. Calls the backing implementation.
//! 3. Converts the Rust response back into a proto message.
//! 4. Maps [`IntegrationError`] → [`tonic::Status`] via [`integration_error_to_status`].

#![allow(clippy::result_large_err)]

use std::sync::Arc;

use chrono::Utc;
use tonic::{Request, Response, Status};

use crate::bridges::ExternalBridgeRegistry;
use crate::control_map::ControlMapRegistry;
use crate::cve::CveId;
use crate::cve_feed::CveFeedShape;
use crate::ids::{StandardSubscriptionId, VendorContractId};
use crate::orchestrator::Orchestrator;
use crate::service::proto;
use crate::service::proto::integration_service_server::IntegrationService;
use crate::standard_registry::ExternalStandardRegistry;
use crate::vendor_registry::VendorIntegrationRegistry;

use super::conversions::{
    baseline_to_proto, binding_from_proto, binding_to_proto, bridge_contract_from_proto,
    bridge_contract_to_proto, composition_from_proto, control_mapping_from_proto,
    cve_record_from_proto, cve_record_to_proto, enforcement_level_to_proto,
    health_summary_to_proto, integration_error_to_status, lifecycle_state_from_proto,
    subscription_from_proto, subscription_to_proto, vendor_contract_from_proto,
    vendor_contract_to_proto,
};

use super::conversions::subscription_status_to_proto;

// ── IntegrationServer ────────────────────────────────────────────────────────

/// Mounts the vendor registry, standards registry, CVE feed, bridge registry,
/// orchestrator, and control-map registry behind the gRPC `IntegrationService` trait.
#[derive(Clone)]
pub struct IntegrationServer {
    vendor_registry: Arc<VendorIntegrationRegistry>,
    standard_registry: Arc<ExternalStandardRegistry>,
    cve_feed: Arc<CveFeedShape>,
    bridge_registry: Arc<ExternalBridgeRegistry>,
    orchestrator: Arc<Orchestrator>,
    control_map: Arc<ControlMapRegistry>,
}

impl std::fmt::Debug for IntegrationServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IntegrationServer").finish_non_exhaustive()
    }
}

impl IntegrationServer {
    /// Construct a server mounting all six backing components.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(
        vendor_registry: Arc<VendorIntegrationRegistry>,
        standard_registry: Arc<ExternalStandardRegistry>,
        cve_feed: Arc<CveFeedShape>,
        bridge_registry: Arc<ExternalBridgeRegistry>,
        orchestrator: Arc<Orchestrator>,
        control_map: Arc<ControlMapRegistry>,
    ) -> Self {
        Self {
            vendor_registry,
            standard_registry,
            cve_feed,
            bridge_registry,
            orchestrator,
            control_map,
        }
    }
}

// ── Helper ───────────────────────────────────────────────────────────────────

fn not_found(what: &str, id: &str) -> Status {
    Status::not_found(format!("{what} {id} not found"))
}

#[allow(dead_code)]
fn internal(msg: impl Into<String>) -> Status {
    Status::internal(msg.into())
}

// ── RPC implementations ──────────────────────────────────────────────────────

#[tonic::async_trait]
impl IntegrationService for IntegrationServer {
    // ── Vendor contract management (5 RPCs) ─────────────────────────────────

    async fn admit_contract(
        &self,
        request: Request<proto::AdmitContractRequest>,
    ) -> Result<Response<proto::AdmitContractResponse>, Status> {
        let r = request.into_inner();
        let contract = r
            .contract
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("contract required"))?;
        let domain =
            vendor_contract_from_proto(contract).map_err(|e| integration_error_to_status(&e))?;
        self.vendor_registry
            .admit_contract(domain)
            .await
            .map_err(|e| integration_error_to_status(&e))?;
        Ok(Response::new(proto::AdmitContractResponse {}))
    }

    async fn get_contract(
        &self,
        request: Request<proto::GetContractRequest>,
    ) -> Result<Response<proto::GetContractResponse>, Status> {
        let r = request.into_inner();
        let cid = VendorContractId(r.contract_id);
        match self.vendor_registry.get_contract(&cid).await {
            Some(c) => Ok(Response::new(proto::GetContractResponse {
                contract: Some(vendor_contract_to_proto(&c)),
            })),
            None => Err(not_found("vendor contract", &cid.0)),
        }
    }

    async fn list_contracts(
        &self,
        _request: Request<proto::ListContractsRequest>,
    ) -> Result<Response<proto::ListContractsResponse>, Status> {
        let contracts = self.vendor_registry.list_contracts().await;
        let protos: Vec<proto::VendorIntegrationContractProto> =
            contracts.iter().map(vendor_contract_to_proto).collect();
        Ok(Response::new(proto::ListContractsResponse {
            contracts: protos,
        }))
    }

    async fn transition_lifecycle(
        &self,
        request: Request<proto::TransitionLifecycleRequest>,
    ) -> Result<Response<proto::TransitionLifecycleResponse>, Status> {
        let r = request.into_inner();
        let cid = VendorContractId(r.contract_id);
        let new_state = r
            .new_state
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("new_state required"))?;
        let state =
            lifecycle_state_from_proto(new_state).map_err(|e| integration_error_to_status(&e))?;
        self.vendor_registry
            .transition_lifecycle(&cid, state)
            .await
            .map_err(|e| integration_error_to_status(&e))?;
        Ok(Response::new(proto::TransitionLifecycleResponse {}))
    }

    async fn revoke_contract(
        &self,
        request: Request<proto::RevokeContractRequest>,
    ) -> Result<Response<proto::RevokeContractResponse>, Status> {
        let r = request.into_inner();
        let cid = VendorContractId(r.contract_id);
        self.vendor_registry
            .revoke_contract(&cid, &r.reason)
            .await
            .map_err(|e| integration_error_to_status(&e))?;
        Ok(Response::new(proto::RevokeContractResponse {}))
    }

    // ── Standards subscription management (4 RPCs) ──────────────────────────

    async fn subscribe(
        &self,
        request: Request<proto::SubscribeRequest>,
    ) -> Result<Response<proto::SubscribeResponse>, Status> {
        let r = request.into_inner();
        let sub = r
            .subscription
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("subscription required"))?;
        let domain = subscription_from_proto(sub).map_err(|e| integration_error_to_status(&e))?;
        self.standard_registry
            .subscribe(domain)
            .await
            .map_err(|e| integration_error_to_status(&e))?;
        Ok(Response::new(proto::SubscribeResponse {}))
    }

    async fn get_subscription_status(
        &self,
        request: Request<proto::GetSubscriptionStatusRequest>,
    ) -> Result<Response<proto::GetSubscriptionStatusResponse>, Status> {
        let r = request.into_inner();
        let sid = StandardSubscriptionId(r.subscription_id);
        let status = self
            .standard_registry
            .status(&sid, Utc::now())
            .await
            .map_err(|e| integration_error_to_status(&e))?;
        Ok(Response::new(subscription_status_to_proto(&sid.0, &status)))
    }

    async fn list_subscriptions(
        &self,
        _request: Request<proto::ListSubscriptionsRequest>,
    ) -> Result<Response<proto::ListSubscriptionsResponse>, Status> {
        let subs = self.standard_registry.list_subscriptions().await;
        let protos: Vec<proto::StandardSubscriptionProto> =
            subs.iter().map(subscription_to_proto).collect();
        Ok(Response::new(proto::ListSubscriptionsResponse {
            subscriptions: protos,
        }))
    }

    async fn unsubscribe(
        &self,
        request: Request<proto::UnsubscribeRequest>,
    ) -> Result<Response<proto::UnsubscribeResponse>, Status> {
        let r = request.into_inner();
        let sid = StandardSubscriptionId(r.subscription_id);
        self.standard_registry
            .unsubscribe(&sid)
            .await
            .map_err(|e| integration_error_to_status(&e))?;
        Ok(Response::new(proto::UnsubscribeResponse {}))
    }

    // ── CVE feed (6 RPCs) ───────────────────────────────────────────────────

    async fn ingest_cve_record(
        &self,
        request: Request<proto::IngestCveRecordRequest>,
    ) -> Result<Response<proto::IngestCveRecordResponse>, Status> {
        let r = request.into_inner();
        let record = r
            .record
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("record required"))?;
        let domain = cve_record_from_proto(record).map_err(|e| integration_error_to_status(&e))?;
        self.cve_feed
            .ingest_record(domain)
            .await
            .map_err(|e| integration_error_to_status(&e))?;
        Ok(Response::new(proto::IngestCveRecordResponse {}))
    }

    async fn get_cve_record(
        &self,
        request: Request<proto::GetCveRecordRequest>,
    ) -> Result<Response<proto::GetCveRecordResponse>, Status> {
        let r = request.into_inner();
        let cid = CveId(r.cve_id);
        match self.cve_feed.get_record(&cid).await {
            Some(rec) => Ok(Response::new(proto::GetCveRecordResponse {
                record: Some(cve_record_to_proto(&rec)),
            })),
            None => Err(not_found("CVE record", &cid.0)),
        }
    }

    async fn list_cve_records(
        &self,
        _request: Request<proto::ListCveRecordsRequest>,
    ) -> Result<Response<proto::ListCveRecordsResponse>, Status> {
        let records = self.cve_feed.list_records().await;
        let protos: Vec<proto::CveRecordProto> = records.iter().map(cve_record_to_proto).collect();
        Ok(Response::new(proto::ListCveRecordsResponse {
            records: protos,
        }))
    }

    async fn bind_cve_to_package(
        &self,
        request: Request<proto::BindCveToPackageRequest>,
    ) -> Result<Response<proto::BindCveToPackageResponse>, Status> {
        let r = request.into_inner();
        let binding = r
            .binding
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("binding required"))?;
        let domain = binding_from_proto(binding).map_err(|e| integration_error_to_status(&e))?;
        self.cve_feed
            .bind_to_package(domain)
            .await
            .map_err(|e| integration_error_to_status(&e))?;
        Ok(Response::new(proto::BindCveToPackageResponse {}))
    }

    async fn list_cve_bindings(
        &self,
        _request: Request<proto::ListCveBindingsRequest>,
    ) -> Result<Response<proto::ListCveBindingsResponse>, Status> {
        let bindings = self.cve_feed.list_bindings().await;
        let protos: Vec<proto::PackageCveBindingProto> =
            bindings.iter().map(binding_to_proto).collect();
        Ok(Response::new(proto::ListCveBindingsResponse {
            bindings: protos,
        }))
    }

    async fn get_enforcement_level(
        &self,
        request: Request<proto::GetEnforcementLevelRequest>,
    ) -> Result<Response<proto::GetEnforcementLevelResponse>, Status> {
        let r = request.into_inner();
        let cid = CveId(r.cve_id);
        match self.cve_feed.enforcement_level_for(&cid).await {
            Some(level) => Ok(Response::new(proto::GetEnforcementLevelResponse {
                level: enforcement_level_to_proto(level),
            })),
            None => Err(not_found("CVE record", &cid.0)),
        }
    }

    // ── External bridge management (4 RPCs) ──────────────────────────────────

    async fn admit_bridge(
        &self,
        request: Request<proto::AdmitBridgeRequest>,
    ) -> Result<Response<proto::AdmitBridgeResponse>, Status> {
        let r = request.into_inner();
        let bridge = r
            .bridge
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("bridge required"))?;
        let domain =
            bridge_contract_from_proto(bridge).map_err(|e| integration_error_to_status(&e))?;
        self.bridge_registry
            .admit_bridge(domain)
            .await
            .map_err(|e| integration_error_to_status(&e))?;
        Ok(Response::new(proto::AdmitBridgeResponse {}))
    }

    async fn get_bridge(
        &self,
        request: Request<proto::GetBridgeRequest>,
    ) -> Result<Response<proto::GetBridgeResponse>, Status> {
        let r = request.into_inner();
        match self.bridge_registry.get_bridge(&r.bridge_id).await {
            Some(b) => Ok(Response::new(proto::GetBridgeResponse {
                bridge: Some(bridge_contract_to_proto(&b)),
            })),
            None => Err(not_found("bridge", &r.bridge_id)),
        }
    }

    async fn list_bridges(
        &self,
        _request: Request<proto::ListBridgesRequest>,
    ) -> Result<Response<proto::ListBridgesResponse>, Status> {
        let bridges = self.bridge_registry.list_bridges().await;
        let protos: Vec<proto::BridgeContractProto> =
            bridges.iter().map(bridge_contract_to_proto).collect();
        Ok(Response::new(proto::ListBridgesResponse {
            bridges: protos,
        }))
    }

    async fn revoke_bridge(
        &self,
        request: Request<proto::RevokeBridgeRequest>,
    ) -> Result<Response<proto::RevokeBridgeResponse>, Status> {
        let r = request.into_inner();
        self.bridge_registry
            .revoke_bridge(&r.bridge_id, &r.reason)
            .await
            .map_err(|e| integration_error_to_status(&e))?;
        Ok(Response::new(proto::RevokeBridgeResponse {}))
    }

    // ── Service composition (3 RPCs) ─────────────────────────────────────────

    async fn validate_composition(
        &self,
        request: Request<proto::ValidateCompositionRequest>,
    ) -> Result<Response<proto::ValidateCompositionResponse>, Status> {
        let r = request.into_inner();
        let composition = r
            .composition
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("composition required"))?;
        let domain = composition_from_proto(composition);
        let boot_order = self
            .orchestrator
            .validate_external_composition(&domain)
            .await
            .map_err(|e| integration_error_to_status(&e))?;
        Ok(Response::new(proto::ValidateCompositionResponse {
            boot_order,
        }))
    }

    async fn get_boot_order(
        &self,
        _request: Request<proto::GetBootOrderRequest>,
    ) -> Result<Response<proto::GetBootOrderResponse>, Status> {
        let boot_order = self.orchestrator.boot_order().await;
        Ok(Response::new(proto::GetBootOrderResponse { boot_order }))
    }

    async fn health_summary(
        &self,
        _request: Request<proto::HealthSummaryRequest>,
    ) -> Result<Response<proto::HealthSummaryResponse>, Status> {
        let summaries = self.orchestrator.health_summary().await;
        let protos: Vec<proto::ServiceHealthSummaryProto> =
            summaries.iter().map(health_summary_to_proto).collect();
        Ok(Response::new(proto::HealthSummaryResponse {
            summaries: protos,
        }))
    }

    // ── Control map (3 RPCs) ─────────────────────────────────────────────────

    async fn add_control_mapping(
        &self,
        request: Request<proto::AddControlMappingRequest>,
    ) -> Result<Response<proto::AddControlMappingResponse>, Status> {
        let r = request.into_inner();
        let mapping = r
            .mapping
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("mapping required"))?;
        let domain =
            control_mapping_from_proto(mapping).map_err(|e| integration_error_to_status(&e))?;
        self.control_map
            .add_mapping(domain)
            .await
            .map_err(|e| integration_error_to_status(&e))?;
        Ok(Response::new(proto::AddControlMappingResponse {}))
    }

    async fn snapshot_baseline(
        &self,
        request: Request<proto::SnapshotBaselineRequest>,
    ) -> Result<Response<proto::SnapshotBaselineResponse>, Status> {
        let r = request.into_inner();
        let baseline = self
            .control_map
            .snapshot_baseline(r.baseline_id, r.aios_version, r.validator_canonical_id)
            .await
            .map_err(|e| integration_error_to_status(&e))?;
        Ok(Response::new(proto::SnapshotBaselineResponse {
            baseline: Some(baseline_to_proto(&baseline)),
        }))
    }

    async fn get_baseline(
        &self,
        request: Request<proto::GetBaselineRequest>,
    ) -> Result<Response<proto::GetBaselineResponse>, Status> {
        let r = request.into_inner();
        match self.control_map.get_baseline(&r.baseline_id).await {
            Some(b) => Ok(Response::new(proto::GetBaselineResponse {
                baseline: Some(baseline_to_proto(&b)),
            })),
            None => Err(not_found("baseline", &r.baseline_id)),
        }
    }

    // ── Info (1 RPC) ─────────────────────────────────────────────────────────

    async fn get_integration_info(
        &self,
        _request: Request<proto::GetIntegrationInfoRequest>,
    ) -> Result<Response<proto::GetIntegrationInfoResponse>, Status> {
        let vendor_count =
            u32::try_from(self.vendor_registry.list_contracts().await.len()).unwrap_or(u32::MAX);
        let standard_count = u32::try_from(self.standard_registry.list_subscriptions().await.len())
            .unwrap_or(u32::MAX);
        let cve_count = u32::try_from(self.cve_feed.list_records().await.len()).unwrap_or(u32::MAX);
        let bridge_count =
            u32::try_from(self.bridge_registry.list_bridges().await.len()).unwrap_or(u32::MAX);
        let composition_services =
            u32::try_from(self.orchestrator.boot_order().await.len()).unwrap_or(u32::MAX);
        // We don't have a direct list_mappings_count; use list_mappings_for_invariant
        // scanning would be expensive. Instead we enumerate all standard frameworks
        // and sum mappings. For now, approximate by iterating known invariants.
        // The most reliable count is: none of the registry APIs expose a direct count,
        // so we approximate by what we can query cheaply.
        let control_mapping_count = 0_u32; // No direct count API; placeholder.

        Ok(Response::new(proto::GetIntegrationInfoResponse {
            code_version: crate::DEFAULT_CODE_VERSION.to_string(),
            schema_version: super::SCHEMA_VERSION.to_string(),
            vendor_contract_count: vendor_count,
            standard_subscription_count: standard_count,
            cve_record_count: cve_count,
            bridge_count,
            composition_service_count: composition_services,
            control_mapping_count,
        }))
    }
}

// ── Bootstrap helper ─────────────────────────────────────────────────────────

/// Builds a [`tonic::transport::server::Router`] from an [`IntegrationServer`].
///
/// The router can be mounted into a shared gRPC server via
/// `.add_service(router)` alongside other services (e.g., aios-network,
/// aios-hardware).
#[must_use]
pub fn build_router(svc: IntegrationServer) -> tonic::transport::server::Router {
    tonic::transport::Server::builder()
        .add_service(super::proto::integration_service_server::IntegrationServiceServer::new(svc))
}
