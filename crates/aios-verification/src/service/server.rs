//! gRPC `VerificationEngine` server adapter + bootstrap helpers (T-069).

#![allow(clippy::result_large_err)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use strum::IntoEnumIterator;
use tokio::sync::RwLock;
use tonic::transport::server::{Router, Server};
use tonic::{Request, Response, Status};

use crate::service::conversions::{
    action_id_from_proto, datetime_to_proto, verification_error_to_status,
    verification_intent_from_proto, verification_result_to_proto,
};
use crate::service::proto;
use crate::service::SCHEMA_VERSION;
use crate::{
    InMemoryVerificationEngine, IntentId, VerificationContext, VerificationEngine,
    VerificationError, VerificationPrimitive, VerificationStatus, DEFAULT_CODE_VERSION,
};

/// Default `VerificationEngine` service id reported by `GetEngineInfo`.
pub const DEFAULT_ENGINE_ID: &str = "aios-verification-inproc";

/// gRPC adapter mounting the in-memory verification engine behind tonic.
#[derive(Clone, Debug)]
pub struct VerificationEngineService {
    engine: Arc<InMemoryVerificationEngine>,
    engine_id: String,
    code_version: String,
    started_at: DateTime<Utc>,
    result_index: Arc<RwLock<HashMap<String, IntentId>>>,
}

impl VerificationEngineService {
    /// Construct an adapter over the in-memory verification engine.
    #[must_use]
    pub fn new(engine: Arc<InMemoryVerificationEngine>) -> Self {
        Self {
            engine,
            engine_id: DEFAULT_ENGINE_ID.to_owned(),
            code_version: DEFAULT_CODE_VERSION.to_owned(),
            started_at: Utc::now(),
            result_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Override the engine id reported by `GetEngineInfo`.
    #[must_use]
    pub fn with_engine_id(mut self, id: impl Into<String>) -> Self {
        self.engine_id = id.into();
        self
    }

    /// Override the code version reported by `GetEngineInfo`.
    #[must_use]
    pub fn with_code_version(mut self, version: impl Into<String>) -> Self {
        self.code_version = version.into();
        self
    }

    /// Return the wrapped in-memory engine.
    #[must_use]
    pub fn engine(&self) -> Arc<InMemoryVerificationEngine> {
        Arc::clone(&self.engine)
    }

    async fn intent_id_for_verification_id(&self, verification_id: &str) -> Option<IntentId> {
        if verification_id.starts_with(IntentId::PREFIX) {
            return Some(IntentId(verification_id.to_owned()));
        }
        self.result_index.read().await.get(verification_id).cloned()
    }
}

#[async_trait]
impl proto::verification_engine_server::VerificationEngine for VerificationEngineService {
    async fn run_verification(
        &self,
        request: Request<proto::RunVerificationRequest>,
    ) -> Result<Response<proto::VerificationResultProto>, Status> {
        let request = request.into_inner();
        if !request.schema_version.is_empty() && request.schema_version != SCHEMA_VERSION {
            return Err(Status::invalid_argument(format!(
                "unsupported schema_version `{}`",
                request.schema_version
            )));
        }

        let outer_action_id = if request.action_id_proto.is_empty() {
            None
        } else {
            Some(action_id_from_proto(&request.action_id_proto)?)
        };
        let mut intent_proto = request
            .intent
            .ok_or_else(|| Status::invalid_argument("intent is required"))?;
        if intent_proto.action_id_proto.is_empty() {
            intent_proto
                .action_id_proto
                .clone_from(&request.action_id_proto);
        }
        let intent = verification_intent_from_proto(intent_proto)?;
        if let Some(action_id) = outer_action_id {
            if action_id != intent.action_id {
                return Err(Status::invalid_argument(
                    "request action_id_proto does not match intent action_id_proto",
                ));
            }
        }

        let context = VerificationContext {
            subject: request.subject,
            action_id: intent.action_id.clone(),
            started_at: Utc::now(),
            timeout_seconds: intent.timeout_seconds,
            dry_run: request.simulate,
        };
        let result = self
            .engine
            .run_verification(&intent, &context)
            .await
            .map_err(|err| verification_error_to_status(&err))?;

        self.result_index
            .write()
            .await
            .insert(result.result_id.clone(), result.intent_id.clone());

        if result.status == VerificationStatus::Timeout {
            return Err(verification_error_to_status(
                &VerificationError::TimeoutExceeded {
                    intent_id: result.intent_id,
                    after_ms: result.duration_ms,
                },
            ));
        }

        Ok(Response::new(verification_result_to_proto(&result)))
    }

    async fn explain_result(
        &self,
        request: Request<proto::ExplainResultRequest>,
    ) -> Result<Response<proto::ExplainResultResponse>, Status> {
        let request = request.into_inner();
        if request.verification_id.is_empty() {
            return Err(Status::invalid_argument("verification_id is required"));
        }
        let intent_id = self
            .intent_id_for_verification_id(&request.verification_id)
            .await
            .ok_or_else(|| {
                Status::not_found(format!(
                    "verification result `{}` was not found",
                    request.verification_id
                ))
            })?;
        let result = self.engine.get_result(&intent_id).await.ok_or_else(|| {
            Status::not_found(format!("verification result `{intent_id}` was not found"))
        })?;
        let narrative = format!(
            "verification {} completed with {}",
            result.result_id, result.status
        );

        Ok(Response::new(proto::ExplainResultResponse {
            result: Some(verification_result_to_proto(&result)),
            narrative,
            snapshot_ids: Vec::new(),
        }))
    }

    async fn get_engine_info(
        &self,
        _request: Request<()>,
    ) -> Result<Response<proto::VerificationEngineInfo>, Status> {
        let supported_primitives = VerificationPrimitive::iter()
            .map(|primitive| primitive.as_wire_str().to_owned())
            .collect();

        Ok(Response::new(proto::VerificationEngineInfo {
            engine_id: self.engine_id.clone(),
            supported_schema_versions: vec![SCHEMA_VERSION.to_owned()],
            default_schema_version: SCHEMA_VERSION.to_owned(),
            supported_primitives,
            supported_property_types: Vec::new(),
            started_at: Some(datetime_to_proto(self.started_at)),
            code_version: self.code_version.clone(),
        }))
    }
}

/// Build a `tonic::transport::server::Router` with the `VerificationEngine`
/// service mounted.
#[must_use]
pub fn build_router(svc: VerificationEngineService) -> Router {
    Server::builder()
        .add_service(proto::verification_engine_server::VerificationEngineServer::new(svc))
}

/// Convenience helper: bind to `addr` and serve until the future is dropped.
///
/// # Errors
///
/// Returns the underlying [`tonic::transport::Error`] when the bind / listen
/// loop fails.
pub async fn serve(
    svc: VerificationEngineService,
    addr: SocketAddr,
) -> Result<(), tonic::transport::Error> {
    build_router(svc).serve(addr).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_router_compiles_and_accepts_service() {
        let svc = VerificationEngineService::new(Arc::new(InMemoryVerificationEngine::new()));
        let _router = build_router(svc);
    }

    #[test]
    fn default_labels_are_populated() {
        let svc = VerificationEngineService::new(Arc::new(InMemoryVerificationEngine::new()));

        assert_eq!(svc.engine_id, DEFAULT_ENGINE_ID);
        assert_eq!(svc.code_version, DEFAULT_CODE_VERSION);
    }
}
