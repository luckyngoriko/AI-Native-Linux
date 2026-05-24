//! gRPC `AiosFs` server adapter + bootstrap helpers (T-043).

#![allow(clippy::result_large_err)]

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use tonic::transport::server::{Router, Server};
use tonic::{Request, Response, Status};

use crate::gc::GcPassDriver;
use crate::in_memory::InMemoryAiosFs;
use crate::quarantine::QuarantineDriver;
use crate::query_eval::materialize_view;
use crate::service::conversions::{
    fs_error_to_status, gc_pass_report_to_proto, object_read_result_to_proto,
    object_write_request_from_proto, object_write_result_to_proto, parse_object_id,
    parse_pointer_id, parse_version_id, pointer_to_proto, quarantine_receipt_to_proto,
    quarantine_trigger_from_proto, query_from_proto, snapshot_from_string,
    snapshot_summary_to_proto, version_to_proto, view_to_proto,
};
use crate::service::proto;
use crate::AiosFs;

/// Default AIOS-FS service id reported by production wiring once an info RPC
/// lands. Kept public to mirror the policy/runtime service adapters.
pub const DEFAULT_FS_ID: &str = "aios-fs-inproc";

/// Default Rust crate code version for T-043 service wiring.
pub const DEFAULT_CODE_VERSION: &str = "aios-fs/0.0.1-T043";

/// gRPC adapter mounting an [`InMemoryAiosFs`] behind tonic.
#[derive(Clone, Debug)]
pub struct AiosFsService {
    inner: Arc<InMemoryAiosFs>,
    fs_id: String,
    code_version: String,
}

impl AiosFsService {
    /// Construct an adapter wrapping the given in-memory AIOS-FS handle.
    #[must_use]
    pub fn new(inner: Arc<InMemoryAiosFs>) -> Self {
        Self {
            inner,
            fs_id: DEFAULT_FS_ID.to_owned(),
            code_version: DEFAULT_CODE_VERSION.to_owned(),
        }
    }

    /// Override the filesystem id kept for production bootstrap parity.
    #[must_use]
    pub fn with_fs_id(mut self, id: impl Into<String>) -> Self {
        self.fs_id = id.into();
        self
    }

    /// Override the code version kept for production bootstrap parity.
    #[must_use]
    pub fn with_code_version(mut self, version: impl Into<String>) -> Self {
        self.code_version = version.into();
        self
    }

    fn gc_driver_from_request(request: &proto::RunGcPassRequestProto) -> GcPassDriver {
        if request.max_chunks_per_pass == 0 && request.max_versions_per_pass == 0 {
            return GcPassDriver::new_with_defaults();
        }

        GcPassDriver::new(
            usize::try_from(request.max_chunks_per_pass).unwrap_or(usize::MAX),
            usize::try_from(request.max_versions_per_pass).unwrap_or(usize::MAX),
        )
    }
}

#[async_trait]
impl proto::aios_fs_server::AiosFs for AiosFsService {
    async fn read_object(
        &self,
        request: Request<proto::ReadObjectRequestProto>,
    ) -> Result<Response<proto::ObjectReadResultProto>, Status> {
        let request = request.into_inner();
        let object_id = parse_object_id(&request.object_id)?;
        let snapshot_id = snapshot_from_string(&request.snapshot_id);
        let read = self
            .inner
            .read_object(&object_id, snapshot_id.as_ref())
            .await
            .map_err(|err| fs_error_to_status(&err))?;
        Ok(Response::new(object_read_result_to_proto(&read)))
    }

    async fn write_object(
        &self,
        request: Request<proto::ObjectWriteRequestProto>,
    ) -> Result<Response<proto::ObjectWriteResultProto>, Status> {
        let request = request.into_inner();
        let (write, context) = object_write_request_from_proto(&request)?;
        let written = self
            .inner
            .write_object(write, &context)
            .await
            .map_err(|err| fs_error_to_status(&err))?;
        Ok(Response::new(object_write_result_to_proto(&written)))
    }

    async fn list_versions(
        &self,
        request: Request<proto::ListVersionsRequestProto>,
    ) -> Result<Response<proto::ListVersionsResponseProto>, Status> {
        let request = request.into_inner();
        let object_id = parse_object_id(&request.object_id)?;
        let versions = self
            .inner
            .list_versions(&object_id)
            .await
            .map_err(|err| fs_error_to_status(&err))?;
        Ok(Response::new(proto::ListVersionsResponseProto {
            versions: versions.iter().map(version_to_proto).collect(),
        }))
    }

    async fn resolve_pointer(
        &self,
        request: Request<proto::ResolvePointerRequestProto>,
    ) -> Result<Response<proto::PointerProto>, Status> {
        let request = request.into_inner();
        let pointer_id = parse_pointer_id(&request.pointer_id)?;
        let pointer = self
            .inner
            .resolve_pointer(&pointer_id)
            .await
            .map_err(|err| fs_error_to_status(&err))?;
        Ok(Response::new(pointer_to_proto(&pointer)))
    }

    async fn get_snapshot(
        &self,
        request: Request<proto::GetSnapshotRequestProto>,
    ) -> Result<Response<proto::SnapshotSummaryProto>, Status> {
        let request = request.into_inner();
        let snapshot_id = snapshot_from_string(&request.snapshot_id)
            .ok_or_else(|| Status::invalid_argument("snapshot_id is required"))?;
        let summary = self
            .inner
            .get_snapshot(&snapshot_id)
            .await
            .map_err(|err| fs_error_to_status(&err))?;
        Ok(Response::new(snapshot_summary_to_proto(&summary)))
    }

    async fn quarantine_object(
        &self,
        request: Request<proto::QuarantineObjectRequestProto>,
    ) -> Result<Response<proto::QuarantineReceiptProto>, Status> {
        let request = request.into_inner();
        let version_id = parse_version_id(&request.version_id)?;
        let trigger = quarantine_trigger_from_proto(
            proto::QuarantineTriggerProto::try_from(request.trigger).map_err(|_| {
                Status::invalid_argument(format!(
                    "unknown quarantine trigger value {}",
                    request.trigger
                ))
            })?,
        )?;
        let driver = QuarantineDriver::new((*self.inner).clone());
        let receipt = driver
            .enter(&version_id, trigger, &request.reason, self.inner.as_ref())
            .await
            .map_err(|err| fs_error_to_status(&err))?;
        Ok(Response::new(quarantine_receipt_to_proto(&receipt)))
    }

    async fn run_gc_pass(
        &self,
        request: Request<proto::RunGcPassRequestProto>,
    ) -> Result<Response<proto::GcPassReportProto>, Status> {
        let request = request.into_inner();
        let report = Self::gc_driver_from_request(&request)
            .run_pass(self.inner.as_ref())
            .await
            .map_err(|err| fs_error_to_status(&err))?;
        Ok(Response::new(gc_pass_report_to_proto(&report)))
    }

    async fn materialize_view(
        &self,
        request: Request<proto::MaterializeViewRequestProto>,
    ) -> Result<Response<proto::ViewProto>, Status> {
        let request = request.into_inner();
        let query = request.query.as_ref().map_or_else(
            || Err(Status::invalid_argument("query is required")),
            query_from_proto,
        )?;
        let snapshot_id = snapshot_from_string(&request.snapshot_id);
        let view = materialize_view(&query, self.inner.as_ref(), snapshot_id.as_ref())
            .await
            .map_err(|err| fs_error_to_status(&err))?;
        Ok(Response::new(view_to_proto(&view)))
    }
}

/// Build a `tonic::transport::server::Router` with the `AiosFs` service
/// mounted.
#[must_use]
pub fn build_router(svc: AiosFsService) -> Router {
    Server::builder().add_service(proto::aios_fs_server::AiosFsServer::new(svc))
}

/// Convenience helper: bind to `addr` and serve until the future is dropped.
///
/// # Errors
///
/// Returns the underlying [`tonic::transport::Error`] when the bind / listen
/// loop fails.
pub async fn serve(svc: AiosFsService, addr: SocketAddr) -> Result<(), tonic::transport::Error> {
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
        let svc = AiosFsService::new(Arc::new(InMemoryAiosFs::new()));
        let _router = build_router(svc);
    }

    #[test]
    fn default_labels_are_populated() {
        let svc = AiosFsService::new(Arc::new(InMemoryAiosFs::new()));
        assert_eq!(svc.fs_id, DEFAULT_FS_ID);
        assert_eq!(svc.code_version, DEFAULT_CODE_VERSION);
    }
}
