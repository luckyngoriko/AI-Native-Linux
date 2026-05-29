//! gRPC `CognitiveCore` server adapter + bootstrap helpers (T-101).

#![allow(clippy::result_large_err)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use aios_action::{ActionEnvelope, Identity, Request as ActionRequest, Trace};
use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;
use tonic::transport::server::{Router, Server};
use tonic::{Request, Response, Status};

use crate::core::{CognitiveCore, TranslationContext};
use crate::in_memory_core::InMemoryCognitiveCore;
use crate::intent::{CognitiveIntent, IntentId, SubjectRef};
use crate::latency::{LatencyTier, PrivacyClass};
use crate::model_catalog::CognitiveModelCatalog;
use crate::routing::AICrossOriginPosture;
use crate::service::conversions::{
    cognitive_error_to_proto_code, datetime_to_proto, prost_struct_to_json, validate_schema_version,
};
use crate::service::proto;
use crate::service::SCHEMA_VERSION;

/// gRPC adapter mounting the in-memory cognitive core behind tonic.
#[derive(Clone)]
pub struct CognitiveCoreServiceImpl {
    core: Arc<InMemoryCognitiveCore>,
    catalog: Option<Arc<CognitiveModelCatalog>>,
    started_at: chrono::DateTime<chrono::Utc>,
    /// In-memory agent registry (M20: L5 agent lifecycle, keyed by canonical id).
    agents: Arc<RwLock<HashMap<String, proto::Agent>>>,
    /// In-memory plan store (M20: L5 planning surface, keyed by plan id).
    plans: Arc<RwLock<HashMap<String, proto::Plan>>>,
    /// In-memory cognitive memory store (M20: L5 memory surface, keyed by entry id).
    memory: Arc<RwLock<HashMap<String, proto::MemoryEntry>>>,
}

impl CognitiveCoreServiceImpl {
    /// Construct an adapter over the in-memory cognitive core.
    #[must_use]
    pub fn new(core: Arc<InMemoryCognitiveCore>) -> Self {
        Self {
            core,
            catalog: None,
            started_at: Utc::now(),
            agents: Arc::new(RwLock::new(HashMap::new())),
            plans: Arc::new(RwLock::new(HashMap::new())),
            memory: Arc::new(RwLock::new(HashMap::new())),
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
#[allow(
    clippy::significant_drop_tightening,
    reason = "gRPC handlers intentionally hold the in-memory store lock for the handler body; further tightening yields no real benefit"
)]
impl proto::cognitive_core_server::CognitiveCore for CognitiveCoreServiceImpl {
    // ── Agent lifecycle (M20: in-memory agent store) ──

    /// Register a new agent from its binding. Fails closed on a missing binding
    /// or duplicate canonical id. INV-002 preserved: registration records an
    /// agent; it never executes anything.
    async fn register_agent(
        &self,
        request: Request<proto::RegisterAgentRequest>,
    ) -> Result<Response<proto::RegisterAgentResponse>, Status> {
        let req = request.into_inner();
        validate_schema_version(&req.schema_version)?;

        let Some(binding) = req.binding else {
            return Err(Status::invalid_argument(
                "RegisterAgent: binding is required",
            ));
        };
        let agent_id = binding.agent_canonical_id.clone();
        if agent_id.is_empty() {
            return Err(Status::invalid_argument(
                "RegisterAgent: binding.agent_canonical_id is required",
            ));
        }

        let now = datetime_to_proto(Utc::now());
        let agent = proto::Agent {
            agent_canonical_id: agent_id.clone(),
            binding: Some(binding),
            state: proto::AgentLifecycleState::Active as i32,
            registered_at: Some(now),
            last_active_at: Some(now),
        };

        {
            let mut guard = self.agents.write().await;
            if guard.contains_key(&agent_id) {
                return Err(Status::already_exists(format!(
                    "RegisterAgent: agent already registered: {agent_id}"
                )));
            }
            guard.insert(agent_id.clone(), agent);
        }

        Ok(Response::new(proto::RegisterAgentResponse {
            agent_canonical_id: agent_id,
            state: proto::AgentLifecycleState::Active as i32,
            error_code: String::new(),
        }))
    }

    /// Look up a registered agent by canonical id.
    async fn get_agent(
        &self,
        request: Request<proto::GetAgentRequest>,
    ) -> Result<Response<proto::Agent>, Status> {
        let req = request.into_inner();
        let found = self
            .agents
            .read()
            .await
            .get(&req.agent_canonical_id)
            .cloned();
        found.map_or_else(
            || {
                Err(Status::not_found(format!(
                    "GetAgent: agent not found: {}",
                    req.agent_canonical_id
                )))
            },
            |agent| Ok(Response::new(agent)),
        )
    }

    /// List agents, optionally filtered by home group, bound user, and kind.
    async fn list_agents(
        &self,
        request: Request<proto::ListAgentsRequest>,
    ) -> Result<Response<proto::ListAgentsResponse>, Status> {
        let req = request.into_inner();
        let agents: Vec<proto::Agent> = {
            let guard = self.agents.read().await;
            guard
                .values()
                .filter(|agent| {
                    let binding = agent.binding.as_ref();
                    let group_ok = req.group_id.is_empty()
                        || binding.is_some_and(|b| b.home_group_id == req.group_id);
                    let user_ok = req.user_id.is_empty()
                        || binding.is_some_and(|b| b.bound_user_id == req.user_id);
                    let kind_ok = req.agent_kind_filter == 0
                        || binding.is_some_and(|b| b.agent_kind == req.agent_kind_filter);
                    group_ok && user_ok && kind_ok
                })
                .cloned()
                .collect()
        };
        Ok(Response::new(proto::ListAgentsResponse { agents }))
    }

    /// Retire an agent, transitioning it to the terminal `RETIRED` state.
    async fn retire_agent(
        &self,
        request: Request<proto::RetireAgentRequest>,
    ) -> Result<Response<proto::RetireAgentResponse>, Status> {
        let req = request.into_inner();
        {
            let mut guard = self.agents.write().await;
            let Some(agent) = guard.get_mut(&req.agent_canonical_id) else {
                return Err(Status::not_found(format!(
                    "RetireAgent: agent not found: {}",
                    req.agent_canonical_id
                )));
            };
            agent.state = proto::AgentLifecycleState::Retired as i32;
            agent.last_active_at = Some(datetime_to_proto(Utc::now()));
        }
        Ok(Response::new(proto::RetireAgentResponse {
            terminal_state: proto::AgentLifecycleState::Retired as i32,
        }))
    }

    // ── Plans and memory (M20: in-memory plan + memory stores) ──

    /// Look up a plan by id.
    async fn get_plan(
        &self,
        request: Request<proto::GetPlanRequest>,
    ) -> Result<Response<proto::Plan>, Status> {
        let req = request.into_inner();
        let found = self.plans.read().await.get(&req.plan_id).cloned();
        found.map_or_else(
            || {
                Err(Status::not_found(format!(
                    "GetPlan: plan not found: {}",
                    req.plan_id
                )))
            },
            |plan| Ok(Response::new(plan)),
        )
    }

    /// List plans, optionally filtered by author agent and plan state.
    async fn list_plans(
        &self,
        request: Request<proto::ListPlansRequest>,
    ) -> Result<Response<proto::ListPlansResponse>, Status> {
        let req = request.into_inner();
        let plans: Vec<proto::Plan> = {
            let guard = self.plans.read().await;
            guard
                .values()
                .filter(|plan| {
                    let agent_ok = req.agent_canonical_id.is_empty()
                        || plan.author_agent_canonical_id == req.agent_canonical_id;
                    let state_ok = req.state_filter == 0 || plan.state == req.state_filter;
                    agent_ok && state_ok
                })
                .cloned()
                .collect()
        };
        Ok(Response::new(proto::ListPlansResponse { plans }))
    }

    /// Look up a memory entry by id.
    async fn get_memory_entry(
        &self,
        request: Request<proto::GetMemoryEntryRequest>,
    ) -> Result<Response<proto::MemoryEntry>, Status> {
        let req = request.into_inner();
        let found = self.memory.read().await.get(&req.entry_id).cloned();
        found.map_or_else(
            || {
                Err(Status::not_found(format!(
                    "GetMemoryEntry: entry not found: {}",
                    req.entry_id
                )))
            },
            |entry| Ok(Response::new(entry)),
        )
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

    /// Draft a plan for an intent. Deterministic and honest: creates a `DRAFT`
    /// plan with a single step referencing the intent's action draft — it does
    /// NOT fabricate multi-step AI plans. INV-002 preserved (a draft, never an
    /// execution).
    async fn draft_plan(
        &self,
        request: Request<proto::DraftPlanRequest>,
    ) -> Result<Response<proto::DraftPlanResponse>, Status> {
        let req = request.into_inner();
        if req.agent_canonical_id.is_empty() || req.intent_id.is_empty() {
            return Ok(Response::new(proto::DraftPlanResponse {
                plan_id: String::new(),
                plan_state: proto::PlanState::Unspecified as i32,
                error_code: proto::CognitiveErrorCode::PlanUnfeasible as i32,
            }));
        }

        let plan_id = format!("plan_{}", ulid::Ulid::new());
        let step = proto::PlanStep {
            step_id: format!("step_{}", ulid::Ulid::new()),
            action_draft_ref: req.intent_id.clone(),
            parent_step_id: String::new(),
        };
        let bundled = req.preferred_granularity == proto::ApprovalGranularity::Bundled as i32;
        let approval_bundle_hash = if bundled {
            blake3::hash(
                format!(
                    "{}|{}|{}",
                    req.agent_canonical_id, req.intent_id, step.step_id
                )
                .as_bytes(),
            )
            .to_hex()
            .to_string()
        } else {
            String::new()
        };

        let plan = proto::Plan {
            plan_id: plan_id.clone(),
            author_agent_canonical_id: req.agent_canonical_id,
            intent_id: req.intent_id,
            state: proto::PlanState::Draft as i32,
            steps: vec![step],
            approval_granularity: req.preferred_granularity,
            approval_bundle_hash,
            drafted_at: Some(datetime_to_proto(Utc::now())),
            submitted_at: None,
            finalized_at: None,
        };

        self.plans.write().await.insert(plan_id.clone(), plan);

        Ok(Response::new(proto::DraftPlanResponse {
            plan_id,
            plan_state: proto::PlanState::Draft as i32,
            error_code: 0,
        }))
    }

    /// Draft a typed action proposal for a plan step. INV-002: produces a typed,
    /// AI-authored `ActionEnvelope` PROPOSAL stamped with a cognitive-provenance
    /// marker — it is never executed here (the Capability Runtime executes only
    /// after a Policy Kernel decision).
    async fn draft_action_proposal(
        &self,
        request: Request<proto::DraftActionProposalRequest>,
    ) -> Result<Response<proto::DraftActionProposalResponse>, Status> {
        let req = request.into_inner();

        // Resolve the referenced plan + step (fail soft via error_code).
        let action_draft_ref = {
            let guard = self.plans.read().await;
            let Some(plan) = guard.get(&req.plan_id) else {
                return Ok(Response::new(proto::DraftActionProposalResponse {
                    envelope: Vec::new(),
                    error_code: proto::CognitiveErrorCode::PlanUnfeasible as i32,
                }));
            };
            plan.steps
                .iter()
                .find(|s| s.step_id == req.plan_step_id)
                .map_or_else(|| plan.intent_id.clone(), |s| s.action_draft_ref.clone())
        };

        // INV-002: build a typed, AI-authored, never-executed proposal envelope.
        let mut envelope = ActionEnvelope::new(
            Identity::new(req.agent_canonical_id.clone(), true),
            ActionRequest::new(
                "cognitive.propose",
                serde_json::json!({
                    "plan_id": req.plan_id,
                    "plan_step_id": req.plan_step_id,
                    "action_draft_ref": action_draft_ref,
                }),
            ),
            Trace::new("00000000000000000000000000000000", "0000000000000000", None),
        );
        if let Some(target) = envelope.request.target.as_object_mut() {
            target.insert(
                "cognitive_provenance".to_string(),
                serde_json::Value::String("aios-cognitive/0.1.0-M20".to_string()),
            );
        }

        let bytes = serde_json::to_vec(&envelope).map_err(|e| {
            Status::internal(format!(
                "DraftActionProposal: envelope serialization failed: {e}"
            ))
        })?;

        Ok(Response::new(proto::DraftActionProposalResponse {
            envelope: bytes,
            error_code: 0,
        }))
    }

    /// Interpret a verification result deterministically and record an episodic
    /// memory entry. Honest: the summary reflects only the facts in the
    /// verification result — it does not fabricate reasoning, and makes no model
    /// call.
    async fn reason_about_verification(
        &self,
        request: Request<proto::ReasonAboutVerificationRequest>,
    ) -> Result<Response<proto::ReasonAboutVerificationResponse>, Status> {
        let req = request.into_inner();

        let verification = req
            .verification_result
            .as_ref()
            .map_or(serde_json::Value::Null, prost_struct_to_json);
        let summary = interpret_verification(&req.action_id, &verification);
        let canonical = serde_json::to_string(&verification).unwrap_or_default();
        let payload_digest = blake3::hash(canonical.as_bytes()).to_hex().to_string();

        let entry_id = format!("mem_{}", ulid::Ulid::new());
        let entry = proto::MemoryEntry {
            entry_id: entry_id.clone(),
            agent_canonical_id: req.agent_canonical_id,
            memory_class: proto::MemoryClass::Episodic as i32,
            privacy: proto::MemoryPrivacyClass::SystemInternal as i32,
            payload_digest,
            written_at: Some(datetime_to_proto(Utc::now())),
        };
        self.memory.write().await.insert(entry_id.clone(), entry);

        Ok(Response::new(proto::ReasonAboutVerificationResponse {
            interpretation_summary: summary,
            memory_entry_id: entry_id,
            error_code: 0,
        }))
    }

    // ── Surface info ──

    async fn get_cognitive_core_info(
        &self,
        _request: Request<()>,
    ) -> Result<Response<proto::CognitiveCoreInfo>, Status> {
        let active_agents = {
            let guard = self.agents.read().await;
            let count = guard
                .values()
                .filter(|a| a.state == proto::AgentLifecycleState::Active as i32)
                .count();
            u32::try_from(count).unwrap_or(u32::MAX)
        };
        let active_plans = {
            let guard = self.plans.read().await;
            let count = guard
                .values()
                .filter(|p| {
                    !matches!(
                        proto::PlanState::try_from(p.state),
                        Ok(proto::PlanState::Completed
                            | proto::PlanState::Abandoned
                            | proto::PlanState::Failed)
                    )
                })
                .count();
            u32::try_from(count).unwrap_or(u32::MAX)
        };

        Ok(Response::new(proto::CognitiveCoreInfo {
            cognitive_core_id: "aios-cognitive-core-001".into(),
            supported_schema_versions: vec![SCHEMA_VERSION.into()],
            default_schema_version: SCHEMA_VERSION.into(),
            active_agents,
            active_plans,
            recovery_mode_active: false,
            started_at: Some(datetime_to_proto(self.started_at)),
        }))
    }
}

/// Deterministically summarize a verification result. Reports only the facts
/// present in the result — it never fabricates reasoning (CLAUDE.md honesty
/// rule). Recognizes a boolean `passed` field or a string `status` field.
fn interpret_verification(action_id: &str, verification: &serde_json::Value) -> String {
    let verdict = verification
        .get("passed")
        .and_then(serde_json::Value::as_bool)
        .map_or_else(
            || {
                verification
                    .get("status")
                    .and_then(serde_json::Value::as_str)
                    .map_or_else(
                        || "no recognized verdict field".to_string(),
                        |s| format!("status={s}"),
                    )
            },
            |passed| {
                if passed {
                    "verification PASSED".to_string()
                } else {
                    "verification FAILED".to_string()
                }
            },
        );
    if action_id.is_empty() {
        format!("verification interpreted: {verdict}")
    } else {
        format!("action {action_id}: {verdict}")
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
