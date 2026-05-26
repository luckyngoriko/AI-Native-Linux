//! gRPC `CognitiveCore` server adapter + bootstrap helpers (T-101).

#![allow(clippy::result_large_err)]

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tonic::transport::server::{Router, Server};
use tonic::{Request, Response, Status};

use crate::core::{CognitiveCore, TranslationContext};
use crate::in_memory_core::InMemoryCognitiveCore;
use crate::intent::{CognitiveIntent, IntentId, SubjectRef};
use crate::latency::{LatencyTier, PrivacyClass};
use crate::model_catalog::CognitiveModelCatalog;
use crate::routing::AICrossOriginPosture;
use crate::service::conversions::{
    cognitive_error_to_proto_code, datetime_to_proto, validate_schema_version,
};
use crate::service::proto;
use crate::service::SCHEMA_VERSION;

/// gRPC adapter mounting the in-memory cognitive core behind tonic.
#[derive(Clone)]
pub struct CognitiveCoreServiceImpl {
    core: Arc<InMemoryCognitiveCore>,
    catalog: Option<Arc<CognitiveModelCatalog>>,
    started_at: chrono::DateTime<chrono::Utc>,
}

impl CognitiveCoreServiceImpl {
    /// Construct an adapter over the in-memory cognitive core.
    #[must_use]
    pub fn new(core: Arc<InMemoryCognitiveCore>) -> Self {
        Self {
            core,
            catalog: None,
            started_at: Utc::now(),
        }
    }

    /// Attach a model catalog for model listing/introspection RPCs.
    #[must_use]
    pub fn with_catalog(mut self, catalog: Arc<CognitiveModelCatalog>) -> Self {
        self.catalog = Some(catalog);
        self
    }

    /// Return the wrapped cognitive core.
    #[must_use]
    pub fn core(&self) -> Arc<InMemoryCognitiveCore> {
        Arc::clone(&self.core)
    }
}

#[async_trait]
impl proto::cognitive_core_server::CognitiveCore for CognitiveCoreServiceImpl {
    // ── Agent lifecycle (Unimplemented — no agent store in T-094..T-100) ──

    /// Not implemented — no agent store exists in T-094..T-100.
    async fn register_agent(
        &self,
        _request: Request<proto::RegisterAgentRequest>,
    ) -> Result<Response<proto::RegisterAgentResponse>, Status> {
        Err(Status::unimplemented(
            "RegisterAgent: agent store not yet implemented (deferred to post-T-101 agent lifecycle)",
        ))
    }

    /// Not implemented — no agent store exists in T-094..T-100.
    async fn get_agent(
        &self,
        _request: Request<proto::GetAgentRequest>,
    ) -> Result<Response<proto::Agent>, Status> {
        Err(Status::unimplemented(
            "GetAgent: agent store not yet implemented (deferred to post-T-101 agent lifecycle)",
        ))
    }

    /// Not implemented — no agent store exists in T-094..T-100.
    async fn list_agents(
        &self,
        _request: Request<proto::ListAgentsRequest>,
    ) -> Result<Response<proto::ListAgentsResponse>, Status> {
        Err(Status::unimplemented(
            "ListAgents: agent store not yet implemented (deferred to post-T-101 agent lifecycle)",
        ))
    }

    /// Not implemented — no agent store exists in T-094..T-100.
    async fn retire_agent(
        &self,
        _request: Request<proto::RetireAgentRequest>,
    ) -> Result<Response<proto::RetireAgentResponse>, Status> {
        Err(Status::unimplemented(
            "RetireAgent: agent store not yet implemented (deferred to post-T-101 agent lifecycle)",
        ))
    }

    // ── Plans and memory (Unimplemented — no plan/memory store) ──

    /// Not implemented — no plan store exists in T-094..T-100.
    async fn get_plan(
        &self,
        _request: Request<proto::GetPlanRequest>,
    ) -> Result<Response<proto::Plan>, Status> {
        Err(Status::unimplemented(
            "GetPlan: plan store not yet implemented (deferred to post-T-101 planning subsystem)",
        ))
    }

    /// Not implemented — no plan store exists in T-094..T-100.
    async fn list_plans(
        &self,
        _request: Request<proto::ListPlansRequest>,
    ) -> Result<Response<proto::ListPlansResponse>, Status> {
        Err(Status::unimplemented(
            "ListPlans: plan store not yet implemented (deferred to post-T-101 planning subsystem)",
        ))
    }

    /// Not implemented — no memory entry store exists in T-094..T-100.
    async fn get_memory_entry(
        &self,
        _request: Request<proto::GetMemoryEntryRequest>,
    ) -> Result<Response<proto::MemoryEntry>, Status> {
        Err(Status::unimplemented(
            "GetMemoryEntry: memory entry store not yet implemented (deferred to post-T-101 memory subsystem)",
        ))
    }

    // ── Cognitive task surface ──

    async fn perceive_intent(
        &self,
        request: Request<proto::PerceiveIntentRequest>,
    ) -> Result<Response<proto::PerceiveIntentResponse>, Status> {
        let request = request.into_inner();
        validate_schema_version(&request.schema_version)?;

        let subject = SubjectRef(if request.agent_canonical_id.is_empty() {
            "agent:default:anonymous:00000000000000000000000000".into()
        } else {
            request.agent_canonical_id
        });

        let intent = CognitiveIntent {
            intent_id: IntentId::new(),
            subject,
            natural_language: request.utterance,
            context_hash: "00000000000000000000000000000000".into(),
            created_at: Utc::now(),
            latency_class: LatencyTier::T3LocalCognitive,
            privacy_class: PrivacyClass::Internal,
        };

        let context = TranslationContext {
            subject: SubjectRef("agent:default:anonymous:00000000000000000000000000".into()),
            available_models: Vec::new(),
            latency_class: LatencyTier::T3LocalCognitive,
            privacy_class: PrivacyClass::Internal,
            ai_cross_origin_posture: AICrossOriginPosture::AiVaultBrokeredOnly,
            recovery_mode: false,
            budget_ok: true,
        };

        match self.core.translate_intent(&intent, &context).await {
            Ok(result) => {
                let mut fields = std::collections::BTreeMap::new();
                fields.insert(
                    "intent_id".into(),
                    prost_types::Value {
                        kind: Some(prost_types::value::Kind::StringValue(
                            result.intent_id.0.clone(),
                        )),
                    },
                );
                fields.insert(
                    "action_type".into(),
                    prost_types::Value {
                        kind: Some(prost_types::value::Kind::StringValue(
                            result.produced_action.request.action.clone(),
                        )),
                    },
                );
                fields.insert(
                    "model_used".into(),
                    prost_types::Value {
                        kind: Some(prost_types::value::Kind::StringValue(
                            result.translation_provenance.model_used.clone(),
                        )),
                    },
                );
                let structured_intent = prost_types::Struct { fields };

                Ok(Response::new(proto::PerceiveIntentResponse {
                    intent_id: result.intent_id.0,
                    structured_intent: Some(structured_intent),
                    error_code: 0, // COGNITIVE_ERROR_CODE_UNSPECIFIED = success
                }))
            }
            Err(err) => {
                let error_code = cognitive_error_to_proto_code(&err);
                Ok(Response::new(proto::PerceiveIntentResponse {
                    intent_id: String::new(),
                    structured_intent: None,
                    error_code,
                }))
            }
        }
    }

    /// Not implemented — no plan store exists in T-094..T-100.
    async fn draft_plan(
        &self,
        _request: Request<proto::DraftPlanRequest>,
    ) -> Result<Response<proto::DraftPlanResponse>, Status> {
        Err(Status::unimplemented(
            "DraftPlan: plan store not yet implemented (deferred to post-T-101 planning subsystem)",
        ))
    }

    /// Not implemented — no plan/action proposal store exists in T-094..T-100.
    async fn draft_action_proposal(
        &self,
        _request: Request<proto::DraftActionProposalRequest>,
    ) -> Result<Response<proto::DraftActionProposalResponse>, Status> {
        Err(Status::unimplemented(
            "DraftActionProposal: action proposal store not yet implemented (deferred to post-T-101)",
        ))
    }

    /// Not implemented — no verification reasoning store exists in T-094..T-100.
    async fn reason_about_verification(
        &self,
        _request: Request<proto::ReasonAboutVerificationRequest>,
    ) -> Result<Response<proto::ReasonAboutVerificationResponse>, Status> {
        Err(Status::unimplemented(
            "ReasonAboutVerification: verification reasoning store not yet implemented (deferred to post-T-101)",
        ))
    }

    // ── Surface info ──

    async fn get_cognitive_core_info(
        &self,
        _request: Request<()>,
    ) -> Result<Response<proto::CognitiveCoreInfo>, Status> {
        let _active_models = self.catalog.as_ref().map_or(0, |_| 0u32);

        Ok(Response::new(proto::CognitiveCoreInfo {
            cognitive_core_id: "aios-cognitive-core-001".into(),
            supported_schema_versions: vec![SCHEMA_VERSION.into()],
            default_schema_version: SCHEMA_VERSION.into(),
            active_agents: 0,
            active_plans: 0,
            recovery_mode_active: false,
            started_at: Some(datetime_to_proto(self.started_at)),
        }))
    }
}

// ---------------------------------------------------------------------------
// Bootstrap helpers
// ---------------------------------------------------------------------------

/// Build a `tonic::transport::server::Router` with `CognitiveCore` mounted.
#[must_use]
pub fn build_router(svc: CognitiveCoreServiceImpl) -> Router {
    Server::builder().add_service(proto::cognitive_core_server::CognitiveCoreServer::new(svc))
}

/// Convenience helper: bind to `addr` and serve until the future is dropped.
///
/// # Errors
///
/// Returns the underlying [`tonic::transport::Error`] when the bind / listen
/// loop fails.
pub async fn serve(
    svc: CognitiveCoreServiceImpl,
    addr: SocketAddr,
) -> Result<(), tonic::transport::Error> {
    build_router(svc).serve(addr).await
}
