//! gRPC `SgrService` server adapter + bootstrap helpers (T-089).

#![allow(clippy::result_large_err)]

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use tonic::transport::server::{Router, Server};
use tonic::{Request, Response, Status};

use crate::service::conversions::{
    adapter_capability_from_proto, adapter_declaration_from_json, dependency_edge_to_proto,
    dependency_kind_from_proto, graph_state_to_proto, registered_adapter_to_proto,
    service_unit_to_proto, sgr_error_to_status, unit_id_from_string, unit_manifest_from_proto,
};
use crate::service::proto;
use crate::service::SCHEMA_VERSION;
use crate::{
    GraphEvaluator, InMemoryServiceGraph, ServiceGraph, SgrAdapterRegistry, UnitFsmDriver,
};

/// gRPC adapter mounting the in-memory S15 service graph behind tonic.
#[derive(Clone)]
pub struct SgrServiceImpl {
    graph: Arc<InMemoryServiceGraph>,
    fsm: Arc<UnitFsmDriver>,
    evaluator: Arc<GraphEvaluator>,
    registry: Arc<SgrAdapterRegistry>,
}

impl SgrServiceImpl {
    /// Construct an adapter over the in-memory SGR drivers.
    #[must_use]
    pub const fn new(
        graph: Arc<InMemoryServiceGraph>,
        fsm: Arc<UnitFsmDriver>,
        evaluator: Arc<GraphEvaluator>,
        registry: Arc<SgrAdapterRegistry>,
    ) -> Self {
        Self {
            graph,
            fsm,
            evaluator,
            registry,
        }
    }

    /// Return the wrapped service graph.
    #[must_use]
    pub fn graph(&self) -> Arc<InMemoryServiceGraph> {
        Arc::clone(&self.graph)
    }

    /// Return the wrapped unit FSM driver.
    #[must_use]
    pub fn fsm(&self) -> Arc<UnitFsmDriver> {
        Arc::clone(&self.fsm)
    }

    /// Return the wrapped graph evaluator.
    #[must_use]
    pub fn evaluator(&self) -> Arc<GraphEvaluator> {
        Arc::clone(&self.evaluator)
    }

    /// Return the wrapped adapter registry.
    #[must_use]
    pub fn registry(&self) -> Arc<SgrAdapterRegistry> {
        Arc::clone(&self.registry)
    }
}

#[async_trait]
impl proto::sgr_service_server::SgrService for SgrServiceImpl {
    async fn register_unit(
        &self,
        request: Request<proto::RegisterUnitRequest>,
    ) -> Result<Response<proto::ServiceUnitProto>, Status> {
        let request = request.into_inner();
        validate_schema_version(&request.schema_version)?;
        let manifest = unit_manifest_from_proto(
            request
                .manifest
                .ok_or_else(|| Status::invalid_argument("manifest is required"))?,
        )?;
        let unit = self
            .graph
            .register_unit(manifest)
            .await
            .map_err(|err| sgr_error_to_status(&err))?;
        Ok(Response::new(service_unit_to_proto(&unit)))
    }

    async fn get_unit(
        &self,
        request: Request<proto::GetUnitRequest>,
    ) -> Result<Response<proto::ServiceUnitProto>, Status> {
        let request = request.into_inner();
        validate_schema_version(&request.schema_version)?;
        let unit_id = unit_id_from_string(&request.unit_id)?;
        let unit = self
            .graph
            .get_unit(&unit_id)
            .await
            .map_err(|err| sgr_error_to_status(&err))?;
        Ok(Response::new(service_unit_to_proto(&unit)))
    }

    async fn list_units(
        &self,
        request: Request<proto::ListUnitsRequest>,
    ) -> Result<Response<proto::ListUnitsResponse>, Status> {
        validate_schema_version(&request.into_inner().schema_version)?;
        let units = self
            .graph
            .list_units()
            .await
            .map_err(|err| sgr_error_to_status(&err))?
            .iter()
            .map(service_unit_to_proto)
            .collect();
        Ok(Response::new(proto::ListUnitsResponse { units }))
    }

    async fn declare_dependency(
        &self,
        request: Request<proto::DeclareDependencyRequest>,
    ) -> Result<Response<proto::DependencyEdgeProto>, Status> {
        let request = request.into_inner();
        validate_schema_version(&request.schema_version)?;
        let kind = dependency_kind_from_proto(request.kind())?;
        let from = unit_id_from_string(&request.from_unit_id)?;
        let to = unit_id_from_string(&request.to_unit_id)?;
        let edge = self
            .graph
            .declare_dependency(&from, &to, kind)
            .await
            .map_err(|err| sgr_error_to_status(&err))?;
        Ok(Response::new(dependency_edge_to_proto(&edge)))
    }

    async fn list_dependencies(
        &self,
        request: Request<proto::ListDependenciesRequest>,
    ) -> Result<Response<proto::ListDependenciesResponse>, Status> {
        let request = request.into_inner();
        validate_schema_version(&request.schema_version)?;
        let unit_id = unit_id_from_string(&request.unit_id)?;
        let edges = self
            .graph
            .list_dependencies(&unit_id)
            .await
            .map_err(|err| sgr_error_to_status(&err))?
            .iter()
            .map(dependency_edge_to_proto)
            .collect();
        Ok(Response::new(proto::ListDependenciesResponse { edges }))
    }

    async fn traverse_graph(
        &self,
        request: Request<proto::TraverseGraphRequest>,
    ) -> Result<Response<proto::TraverseGraphResponse>, Status> {
        validate_schema_version(&request.into_inner().schema_version)?;
        let ordered_unit_ids = self
            .evaluator
            .topological_sort()
            .await
            .map_err(|err| sgr_error_to_status(&err))?
            .iter()
            .map(ToString::to_string)
            .collect();
        Ok(Response::new(proto::TraverseGraphResponse {
            ordered_unit_ids,
        }))
    }

    async fn get_graph_state(
        &self,
        request: Request<proto::GetGraphStateRequest>,
    ) -> Result<Response<proto::GetGraphStateResponse>, Status> {
        validate_schema_version(&request.into_inner().schema_version)?;
        let state = self
            .graph
            .graph_state()
            .await
            .map_err(|err| sgr_error_to_status(&err))?;
        Ok(Response::new(proto::GetGraphStateResponse {
            state: i32::from(graph_state_to_proto(state)),
        }))
    }

    async fn evaluate_graph(
        &self,
        request: Request<proto::EvaluateGraphRequest>,
    ) -> Result<Response<proto::EvaluateGraphResponse>, Status> {
        validate_schema_version(&request.into_inner().schema_version)?;
        let convergence_state = self
            .evaluator
            .convergence_state()
            .await
            .map_err(|err| sgr_error_to_status(&err))?;
        let converged = self
            .evaluator
            .is_converged()
            .await
            .map_err(|err| sgr_error_to_status(&err))?;
        Ok(Response::new(proto::EvaluateGraphResponse {
            convergence_state: i32::from(graph_state_to_proto(convergence_state)),
            converged,
        }))
    }

    async fn start_unit(
        &self,
        request: Request<proto::StartUnitRequest>,
    ) -> Result<Response<proto::ServiceUnitProto>, Status> {
        let request = request.into_inner();
        validate_schema_version(&request.schema_version)?;
        let unit_id = unit_id_from_string(&request.unit_id)?;
        let unit = self
            .fsm
            .start(&unit_id)
            .await
            .map_err(|err| sgr_error_to_status(&err))?;
        Ok(Response::new(service_unit_to_proto(&unit)))
    }

    async fn stop_unit(
        &self,
        request: Request<proto::StopUnitRequest>,
    ) -> Result<Response<proto::ServiceUnitProto>, Status> {
        let request = request.into_inner();
        validate_schema_version(&request.schema_version)?;
        let unit_id = unit_id_from_string(&request.unit_id)?;
        let unit = self
            .fsm
            .stop(&unit_id)
            .await
            .map_err(|err| sgr_error_to_status(&err))?;
        Ok(Response::new(service_unit_to_proto(&unit)))
    }

    async fn restart_unit(
        &self,
        request: Request<proto::RestartUnitRequest>,
    ) -> Result<Response<proto::ServiceUnitProto>, Status> {
        let request = request.into_inner();
        validate_schema_version(&request.schema_version)?;
        let unit_id = unit_id_from_string(&request.unit_id)?;
        let unit = self
            .fsm
            .restart(&unit_id)
            .await
            .map_err(|err| sgr_error_to_status(&err))?;
        Ok(Response::new(service_unit_to_proto(&unit)))
    }

    async fn mark_unit_failed(
        &self,
        request: Request<proto::MarkUnitFailedRequest>,
    ) -> Result<Response<proto::ServiceUnitProto>, Status> {
        let request = request.into_inner();
        validate_schema_version(&request.schema_version)?;
        let unit_id = unit_id_from_string(&request.unit_id)?;
        let unit = self
            .fsm
            .mark_failed(&unit_id, request.reason)
            .await
            .map_err(|err| sgr_error_to_status(&err))?;
        Ok(Response::new(service_unit_to_proto(&unit)))
    }

    async fn register_adapter(
        &self,
        request: Request<proto::RegisterAdapterRequest>,
    ) -> Result<Response<proto::RegisteredAdapterProto>, Status> {
        let request = request.into_inner();
        validate_schema_version(&request.schema_version)?;
        let capability = adapter_capability_from_proto(
            request
                .capability
                .ok_or_else(|| Status::invalid_argument("capability is required"))?,
        )?;
        let declaration = adapter_declaration_from_json(&request.declaration_json)?;
        let adapter = self
            .registry
            .register_adapter(capability, declaration)
            .await
            .map_err(|err| sgr_error_to_status(&err))?;
        Ok(Response::new(registered_adapter_to_proto(&adapter)?))
    }

    async fn lookup_adapter(
        &self,
        request: Request<proto::LookupAdapterRequest>,
    ) -> Result<Response<proto::RegisteredAdapterProto>, Status> {
        let request = request.into_inner();
        validate_schema_version(&request.schema_version)?;
        if request.capability_id.trim().is_empty() {
            return Err(Status::invalid_argument("capability_id is required"));
        }
        let adapter = self
            .registry
            .lookup_adapter(&request.capability_id)
            .await
            .map_err(|err| sgr_error_to_status(&err))?;
        Ok(Response::new(registered_adapter_to_proto(&adapter)?))
    }

    async fn list_adapters(
        &self,
        request: Request<proto::ListAdaptersRequest>,
    ) -> Result<Response<proto::ListAdaptersResponse>, Status> {
        validate_schema_version(&request.into_inner().schema_version)?;
        let adapters = self
            .registry
            .list_adapters()
            .await
            .iter()
            .map(registered_adapter_to_proto)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Response::new(proto::ListAdaptersResponse { adapters }))
    }

    async fn find_adapter_for_unit(
        &self,
        request: Request<proto::FindAdapterForUnitRequest>,
    ) -> Result<Response<proto::FindAdapterForUnitResponse>, Status> {
        let request = request.into_inner();
        validate_schema_version(&request.schema_version)?;
        let manifest = unit_manifest_from_proto(
            request
                .manifest
                .ok_or_else(|| Status::invalid_argument("manifest is required"))?,
        )?;
        let adapter = self
            .registry
            .find_adapter_for_unit(&manifest)
            .await
            .map_err(|err| sgr_error_to_status(&err))?
            .as_ref()
            .map(registered_adapter_to_proto)
            .transpose()?;
        Ok(Response::new(proto::FindAdapterForUnitResponse { adapter }))
    }
}

fn validate_schema_version(schema_version: &str) -> Result<(), Status> {
    if schema_version.is_empty() || schema_version == SCHEMA_VERSION {
        return Ok(());
    }
    Err(Status::failed_precondition(format!(
        "unsupported schema_version `{schema_version}`"
    )))
}

/// Build a `tonic::transport::server::Router` with `SgrService` mounted.
#[must_use]
pub fn build_router(svc: SgrServiceImpl) -> Router {
    Server::builder().add_service(proto::sgr_service_server::SgrServiceServer::new(svc))
}

/// Convenience helper: bind to `addr` and serve until the future is dropped.
///
/// # Errors
///
/// Returns the underlying [`tonic::transport::Error`] when the bind / listen
/// loop fails.
pub async fn serve(svc: SgrServiceImpl, addr: SocketAddr) -> Result<(), tonic::transport::Error> {
    build_router(svc).serve(addr).await
}
