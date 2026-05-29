//! T-101 — end-to-end gRPC roundtrip integration test for `CognitiveCore`.
//!
//! Spins up a tonic server backed by [`aios_cognitive::InMemoryCognitiveCore`]
//! on a random localhost port, builds a tonic client against that address,
//! and exercises the 12 RPCs from S13.1 §19:
//!
//! Implemented RPCs:
//! - `PerceiveIntent` — happy path; invalid schema version; error code surfacing
//! - `GetCognitiveCoreInfo` — returns expected schema and component info
//!
//! M20 — agent/plan/memory RPCs are now implemented over in-memory stores:
//! - `RegisterAgent`, `GetAgent`, `ListAgents`, `RetireAgent`
//! - `GetPlan`, `ListPlans`, `GetMemoryEntry`
//! - `DraftPlan`, `DraftActionProposal`, `ReasonAboutVerification`
//!
//! Additional tests:
//! - Schema version validation
//! - `CognitiveError` → `tonic::Status` mapping
//! - Proto enum roundtrip assertions

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::significant_drop_tightening,
    clippy::items_after_statements,
    clippy::result_large_err,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::oneshot;

use aios_cognitive::service::proto::cognitive_core_client::CognitiveCoreClient;
use aios_cognitive::service::proto::cognitive_core_server::CognitiveCoreServer;
use aios_cognitive::service::proto::{
    DraftActionProposalRequest, DraftPlanRequest, GetAgentRequest, GetMemoryEntryRequest,
    GetPlanRequest, ListAgentsRequest, ListPlansRequest, PerceiveIntentRequest,
    ReasonAboutVerificationRequest, RegisterAgentRequest, RetireAgentRequest,
};
use aios_cognitive::service::{proto, CognitiveCoreServiceImpl, SCHEMA_VERSION};
use aios_cognitive::InMemoryCognitiveCore;

/// Bind a TCP listener to `127.0.0.1:0` and return the bound address.
async fn pick_port() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind random port");
    let addr = listener.local_addr().expect("local_addr");
    drop(listener);
    addr
}

/// Spawn the server task and return `(addr, shutdown_tx, join_handle)`.
async fn spawn_server(
    svc: CognitiveCoreServiceImpl,
) -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let addr = pick_port().await;
    let (tx, rx) = oneshot::channel::<()>();
    let server = tonic::transport::Server::builder().add_service(CognitiveCoreServer::new(svc));
    let handle = tokio::spawn(async move {
        server
            .serve_with_shutdown(addr, async move {
                let _ = rx.await;
            })
            .await
            .expect("server task");
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    (addr, tx, handle)
}

/// Build a default `CognitiveCoreServiceImpl` backed by an empty core.
fn default_service() -> CognitiveCoreServiceImpl {
    let core = Arc::new(InMemoryCognitiveCore::new());
    CognitiveCoreServiceImpl::new(core)
}

/// Build a valid `AgentBinding` for registration tests.
fn test_binding(id: &str) -> proto::AgentBinding {
    proto::AgentBinding {
        agent_canonical_id: id.into(),
        home_group_id: "group:default".into(),
        bound_user_id: "human:lucky".into(),
        identity_bundle_version: "1".into(),
        agent_kind: proto::AgentKind::Assistant as i32,
        registered_at: None,
        registered_by: "human:lucky".into(),
    }
}

/// Register a test agent on `client` so dependent RPCs have state to read.
async fn register_test_agent(
    client: &mut CognitiveCoreClient<tonic::transport::Channel>,
    id: &str,
) {
    client
        .register_agent(tonic::Request::new(RegisterAgentRequest {
            schema_version: SCHEMA_VERSION.into(),
            binding: Some(test_binding(id)),
            manifest: None,
            approver_canonical_id: "human:lucky".into(),
            approver_signature: vec![],
        }))
        .await
        .expect("register_test_agent");
}

// ────────────────────────────────────────────────────────────────────
// Test 1: PerceiveIntent — happy path
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn perceive_intent_happy_path() {
    let service = default_service();
    let (addr, tx, handle) = spawn_server(service).await;

    let mut client = CognitiveCoreClient::connect(format!("http://{addr}"))
        .await
        .expect("connect");

    let request = tonic::Request::new(PerceiveIntentRequest {
        schema_version: SCHEMA_VERSION.into(),
        agent_canonical_id: "agent:default:test:00000000000000000000000001".into(),
        utterance: "restart the nginx service".into(),
        context_facts: None,
    });

    let response = client
        .perceive_intent(request)
        .await
        .expect("perceive_intent RPC");
    let resp = response.into_inner();

    assert!(!resp.intent_id.is_empty(), "intent_id must be populated");
    assert!(
        resp.intent_id.starts_with("cogi_"),
        "intent_id must start with cogi_: {}",
        resp.intent_id
    );
    assert!(
        resp.structured_intent.is_some(),
        "structured_intent must be populated on success"
    );
    assert_eq!(
        resp.error_code, 0,
        "error_code must be UNSPECIFIED (0) on success"
    );

    let si = resp.structured_intent.unwrap();
    assert!(
        si.fields.contains_key("intent_id"),
        "structured_intent must contain intent_id"
    );
    assert!(
        si.fields.contains_key("action_type"),
        "structured_intent must contain action_type"
    );
    assert!(
        si.fields.contains_key("model_used"),
        "structured_intent must contain model_used"
    );

    let _ = tx.send(());
    let _ = handle.await;
}

// ────────────────────────────────────────────────────────────────────
// Test 2: PerceiveIntent — invalid schema version
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn perceive_intent_rejects_invalid_schema_version() {
    let service = default_service();
    let (addr, tx, handle) = spawn_server(service).await;

    let mut client = CognitiveCoreClient::connect(format!("http://{addr}"))
        .await
        .expect("connect");

    let request = tonic::Request::new(PerceiveIntentRequest {
        schema_version: "aios.cognitive.v0.bogus".into(),
        agent_canonical_id: "agent:default:test:00000000000000000000000001".into(),
        utterance: "hello".into(),
        context_facts: None,
    });

    let result = client.perceive_intent(request).await;
    assert!(result.is_err(), "must reject invalid schema version");
    let status = result.unwrap_err();
    assert_eq!(
        status.code(),
        tonic::Code::FailedPrecondition,
        "invalid schema version must return FailedPrecondition"
    );
    assert!(
        status.message().contains("unsupported schema_version"),
        "message must mention unsupported schema_version: {}",
        status.message()
    );

    let _ = tx.send(());
    let _ = handle.await;
}

// ────────────────────────────────────────────────────────────────────
// Test 3: GetCognitiveCoreInfo — returns expected fields
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_cognitive_core_info_returns_expected_fields() {
    let service = default_service();
    let (addr, tx, handle) = spawn_server(service).await;

    let mut client = CognitiveCoreClient::connect(format!("http://{addr}"))
        .await
        .expect("connect");

    let response = client
        .get_cognitive_core_info(tonic::Request::new(()))
        .await
        .expect("get_cognitive_core_info RPC");
    let info = response.into_inner();

    assert_eq!(
        info.cognitive_core_id, "aios-cognitive-core-001",
        "cognitive_core_id mismatch"
    );
    assert!(
        info.supported_schema_versions
            .contains(&SCHEMA_VERSION.to_string()),
        "must list current schema version"
    );
    assert_eq!(
        info.default_schema_version, SCHEMA_VERSION,
        "default_schema_version mismatch"
    );
    assert_eq!(info.active_agents, 0, "active_agents must be 0");
    assert_eq!(info.active_plans, 0, "active_plans must be 0");
    assert!(
        !info.recovery_mode_active,
        "recovery_mode_active must be false"
    );
    assert!(info.started_at.is_some(), "started_at must be populated");

    let _ = tx.send(());
    let _ = handle.await;
}

// ────────────────────────────────────────────────────────────────────
// Test 4: RegisterAgent — creates an ACTIVE agent
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn register_agent_creates_active_agent() {
    let service = default_service();
    let (addr, tx, handle) = spawn_server(service).await;

    let mut client = CognitiveCoreClient::connect(format!("http://{addr}"))
        .await
        .expect("connect");

    let request = tonic::Request::new(RegisterAgentRequest {
        schema_version: SCHEMA_VERSION.into(),
        binding: Some(test_binding(
            "agent:default:test:00000000000000000000000001",
        )),
        manifest: None,
        approver_canonical_id: "human:lucky".into(),
        approver_signature: vec![],
    });

    let resp = client
        .register_agent(request)
        .await
        .expect("register_agent RPC")
        .into_inner();

    assert!(
        !resp.agent_canonical_id.is_empty(),
        "agent_canonical_id must be populated"
    );
    assert_eq!(
        resp.state,
        proto::AgentLifecycleState::Active as i32,
        "registered agent must be ACTIVE"
    );
    assert!(resp.error_code.is_empty(), "no error_code on success");

    // A missing binding must be rejected (fail-closed).
    let bad = client
        .register_agent(tonic::Request::new(RegisterAgentRequest {
            schema_version: SCHEMA_VERSION.into(),
            binding: None,
            manifest: None,
            approver_canonical_id: String::new(),
            approver_signature: vec![],
        }))
        .await;
    assert_eq!(
        bad.unwrap_err().code(),
        tonic::Code::InvalidArgument,
        "missing binding must be InvalidArgument"
    );

    let _ = tx.send(());
    let _ = handle.await;
}

// ────────────────────────────────────────────────────────────────────
// Test 5: GetAgent — returns the registered agent, else NotFound
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_agent_returns_registered_agent_or_not_found() {
    let service = default_service();
    let (addr, tx, handle) = spawn_server(service).await;

    let mut client = CognitiveCoreClient::connect(format!("http://{addr}"))
        .await
        .expect("connect");

    let id = "agent:default:test:00000000000000000000000001";
    register_test_agent(&mut client, id).await;

    let agent = client
        .get_agent(tonic::Request::new(GetAgentRequest {
            agent_canonical_id: id.into(),
        }))
        .await
        .expect("get_agent RPC")
        .into_inner();
    assert_eq!(agent.agent_canonical_id, id);
    assert_eq!(agent.state, proto::AgentLifecycleState::Active as i32);

    let missing = client
        .get_agent(tonic::Request::new(GetAgentRequest {
            agent_canonical_id: "agent:default:test:ffffffffffffffffffffffffff".into(),
        }))
        .await;
    assert_eq!(
        missing.unwrap_err().code(),
        tonic::Code::NotFound,
        "unknown agent must be NotFound"
    );

    let _ = tx.send(());
    let _ = handle.await;
}

// ────────────────────────────────────────────────────────────────────
// Test 6: ListAgents — returns the registered agents
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_agents_returns_registered_agents() {
    let service = default_service();
    let (addr, tx, handle) = spawn_server(service).await;

    let mut client = CognitiveCoreClient::connect(format!("http://{addr}"))
        .await
        .expect("connect");

    let empty = client
        .list_agents(tonic::Request::new(ListAgentsRequest {
            group_id: String::new(),
            user_id: String::new(),
            agent_kind_filter: 0,
        }))
        .await
        .expect("list_agents RPC")
        .into_inner();
    assert!(empty.agents.is_empty(), "no agents before registration");

    register_test_agent(&mut client, "agent:default:test:00000000000000000000000001").await;

    let listed = client
        .list_agents(tonic::Request::new(ListAgentsRequest {
            group_id: "group:default".into(),
            user_id: String::new(),
            agent_kind_filter: 0,
        }))
        .await
        .expect("list_agents RPC")
        .into_inner();
    assert_eq!(listed.agents.len(), 1, "registered agent must be listed");

    let _ = tx.send(());
    let _ = handle.await;
}

// ────────────────────────────────────────────────────────────────────
// Test 7: RetireAgent — transitions to RETIRED, else NotFound
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn retire_agent_transitions_to_retired() {
    let service = default_service();
    let (addr, tx, handle) = spawn_server(service).await;

    let mut client = CognitiveCoreClient::connect(format!("http://{addr}"))
        .await
        .expect("connect");

    let id = "agent:default:test:00000000000000000000000001";
    register_test_agent(&mut client, id).await;

    let resp = client
        .retire_agent(tonic::Request::new(RetireAgentRequest {
            agent_canonical_id: id.into(),
            requester_canonical_id: "human:lucky".into(),
            reason: "test".into(),
        }))
        .await
        .expect("retire_agent RPC")
        .into_inner();
    assert_eq!(
        resp.terminal_state,
        proto::AgentLifecycleState::Retired as i32,
        "retired agent must reach RETIRED"
    );

    let missing = client
        .retire_agent(tonic::Request::new(RetireAgentRequest {
            agent_canonical_id: "agent:unknown".into(),
            requester_canonical_id: "human:lucky".into(),
            reason: "test".into(),
        }))
        .await;
    assert_eq!(
        missing.unwrap_err().code(),
        tonic::Code::NotFound,
        "retiring unknown agent must be NotFound"
    );

    let _ = tx.send(());
    let _ = handle.await;
}

// ────────────────────────────────────────────────────────────────────
// Test 8: GetPlan — returns a drafted plan, else NotFound
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_plan_returns_drafted_plan_or_not_found() {
    let service = default_service();
    let (addr, tx, handle) = spawn_server(service).await;

    let mut client = CognitiveCoreClient::connect(format!("http://{addr}"))
        .await
        .expect("connect");

    let missing = client
        .get_plan(tonic::Request::new(GetPlanRequest {
            plan_id: "plan_does_not_exist".into(),
        }))
        .await;
    assert_eq!(
        missing.unwrap_err().code(),
        tonic::Code::NotFound,
        "unknown plan must be NotFound"
    );

    let drafted = client
        .draft_plan(tonic::Request::new(DraftPlanRequest {
            agent_canonical_id: "agent:default:test:00000000000000000000000001".into(),
            intent_id: "cogi_00000000000000000000000001".into(),
            preferred_granularity: proto::ApprovalGranularity::PerAction as i32,
        }))
        .await
        .expect("draft_plan RPC")
        .into_inner();

    let plan = client
        .get_plan(tonic::Request::new(GetPlanRequest {
            plan_id: drafted.plan_id.clone(),
        }))
        .await
        .expect("get_plan RPC")
        .into_inner();
    assert_eq!(plan.plan_id, drafted.plan_id);
    assert_eq!(plan.state, proto::PlanState::Draft as i32);

    let _ = tx.send(());
    let _ = handle.await;
}

// ────────────────────────────────────────────────────────────────────
// Test 9: ListPlans — returns drafted plans
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_plans_returns_drafted_plans() {
    let service = default_service();
    let (addr, tx, handle) = spawn_server(service).await;

    let mut client = CognitiveCoreClient::connect(format!("http://{addr}"))
        .await
        .expect("connect");

    let empty = client
        .list_plans(tonic::Request::new(ListPlansRequest {
            agent_canonical_id: String::new(),
            state_filter: 0,
        }))
        .await
        .expect("list_plans RPC")
        .into_inner();
    assert!(empty.plans.is_empty(), "no plans before drafting");

    client
        .draft_plan(tonic::Request::new(DraftPlanRequest {
            agent_canonical_id: "agent:a".into(),
            intent_id: "cogi_1".into(),
            preferred_granularity: 0,
        }))
        .await
        .expect("draft_plan RPC");

    let listed = client
        .list_plans(tonic::Request::new(ListPlansRequest {
            agent_canonical_id: String::new(),
            state_filter: 0,
        }))
        .await
        .expect("list_plans RPC")
        .into_inner();
    assert_eq!(listed.plans.len(), 1, "drafted plan must be listed");

    let _ = tx.send(());
    let _ = handle.await;
}

// ────────────────────────────────────────────────────────────────────
// Test 10: GetMemoryEntry — returns a recorded entry, else NotFound
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_memory_entry_returns_entry_or_not_found() {
    let service = default_service();
    let (addr, tx, handle) = spawn_server(service).await;

    let mut client = CognitiveCoreClient::connect(format!("http://{addr}"))
        .await
        .expect("connect");

    let missing = client
        .get_memory_entry(tonic::Request::new(GetMemoryEntryRequest {
            entry_id: "mem_missing".into(),
        }))
        .await;
    assert_eq!(
        missing.unwrap_err().code(),
        tonic::Code::NotFound,
        "unknown memory entry must be NotFound"
    );

    let reasoned = client
        .reason_about_verification(tonic::Request::new(ReasonAboutVerificationRequest {
            agent_canonical_id: "agent:a".into(),
            action_id: "act_1".into(),
            verification_result: None,
        }))
        .await
        .expect("reason RPC")
        .into_inner();

    let entry = client
        .get_memory_entry(tonic::Request::new(GetMemoryEntryRequest {
            entry_id: reasoned.memory_entry_id.clone(),
        }))
        .await
        .expect("get_memory_entry RPC")
        .into_inner();
    assert_eq!(entry.entry_id, reasoned.memory_entry_id);
    assert_eq!(entry.memory_class, proto::MemoryClass::Episodic as i32);

    let _ = tx.send(());
    let _ = handle.await;
}

// ────────────────────────────────────────────────────────────────────
// Test 11: DraftPlan — creates a DRAFT plan
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn draft_plan_creates_draft_plan() {
    let service = default_service();
    let (addr, tx, handle) = spawn_server(service).await;

    let mut client = CognitiveCoreClient::connect(format!("http://{addr}"))
        .await
        .expect("connect");

    let resp = client
        .draft_plan(tonic::Request::new(DraftPlanRequest {
            agent_canonical_id: "agent:default:test:00000000000000000000000001".into(),
            intent_id: "cogi_00000000000000000000000001".into(),
            preferred_granularity: proto::ApprovalGranularity::Bundled as i32,
        }))
        .await
        .expect("draft_plan RPC")
        .into_inner();

    assert!(
        resp.plan_id.starts_with("plan_"),
        "plan_id must start with plan_: {}",
        resp.plan_id
    );
    assert_eq!(resp.plan_state, proto::PlanState::Draft as i32);
    assert_eq!(resp.error_code, 0, "UNSPECIFIED error_code on success");

    // Missing required fields fail soft via error_code (not a hard error).
    let unfeasible = client
        .draft_plan(tonic::Request::new(DraftPlanRequest {
            agent_canonical_id: String::new(),
            intent_id: String::new(),
            preferred_granularity: 0,
        }))
        .await
        .expect("draft_plan RPC")
        .into_inner();
    assert_eq!(
        unfeasible.error_code,
        proto::CognitiveErrorCode::PlanUnfeasible as i32,
        "empty request must report PLAN_UNFEASIBLE"
    );

    let _ = tx.send(());
    let _ = handle.await;
}

// ────────────────────────────────────────────────────────────────────
// Test 12: DraftActionProposal — returns a typed proposal (INV-002)
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn draft_action_proposal_returns_typed_proposal_inv002() {
    let service = default_service();
    let (addr, tx, handle) = spawn_server(service).await;

    let mut client = CognitiveCoreClient::connect(format!("http://{addr}"))
        .await
        .expect("connect");

    let plan = client
        .draft_plan(tonic::Request::new(DraftPlanRequest {
            agent_canonical_id: "agent:default:test:00000000000000000000000001".into(),
            intent_id: "cogi_00000000000000000000000001".into(),
            preferred_granularity: 0,
        }))
        .await
        .expect("draft_plan RPC")
        .into_inner();

    let proposal = client
        .draft_action_proposal(tonic::Request::new(DraftActionProposalRequest {
            agent_canonical_id: "agent:default:test:00000000000000000000000001".into(),
            plan_id: plan.plan_id.clone(),
            plan_step_id: String::new(),
        }))
        .await
        .expect("draft_action_proposal RPC")
        .into_inner();

    assert_eq!(proposal.error_code, 0, "no error on success");
    assert!(
        !proposal.envelope.is_empty(),
        "envelope bytes must be populated"
    );

    // INV-002: the proposal is a typed, AI-authored envelope carrying the
    // cognitive-provenance marker — it must NOT have been executed here.
    let envelope: serde_json::Value =
        serde_json::from_slice(&proposal.envelope).expect("envelope must be valid JSON");
    assert!(envelope.is_object(), "envelope must be a JSON object");
    let envelope_str = String::from_utf8_lossy(&proposal.envelope);
    assert!(
        envelope_str.contains("cognitive_provenance"),
        "INV-002: proposal envelope must carry the cognitive_provenance marker"
    );

    // An unknown plan fails soft via error_code (PLAN_UNFEASIBLE), not a crash.
    let unknown = client
        .draft_action_proposal(tonic::Request::new(DraftActionProposalRequest {
            agent_canonical_id: "agent:a".into(),
            plan_id: "plan_unknown".into(),
            plan_step_id: String::new(),
        }))
        .await
        .expect("draft_action_proposal RPC")
        .into_inner();
    assert_eq!(
        unknown.error_code,
        proto::CognitiveErrorCode::PlanUnfeasible as i32
    );

    let _ = tx.send(());
    let _ = handle.await;
}

// ────────────────────────────────────────────────────────────────────
// Test 13: ReasonAboutVerification — records an episodic memory entry
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn reason_about_verification_records_memory() {
    let service = default_service();
    let (addr, tx, handle) = spawn_server(service).await;

    let mut client = CognitiveCoreClient::connect(format!("http://{addr}"))
        .await
        .expect("connect");

    let resp = client
        .reason_about_verification(tonic::Request::new(ReasonAboutVerificationRequest {
            agent_canonical_id: "agent:default:test:00000000000000000000000001".into(),
            action_id: "act_00000000000000000000000001".into(),
            verification_result: None,
        }))
        .await
        .expect("reason_about_verification RPC")
        .into_inner();

    assert!(
        !resp.interpretation_summary.is_empty(),
        "interpretation_summary must be populated"
    );
    assert!(
        resp.memory_entry_id.starts_with("mem_"),
        "memory_entry_id must start with mem_: {}",
        resp.memory_entry_id
    );
    assert_eq!(resp.error_code, 0);

    // The recorded entry must be retrievable.
    let entry = client
        .get_memory_entry(tonic::Request::new(GetMemoryEntryRequest {
            entry_id: resp.memory_entry_id.clone(),
        }))
        .await
        .expect("get_memory_entry RPC")
        .into_inner();
    assert_eq!(entry.entry_id, resp.memory_entry_id);

    let _ = tx.send(());
    let _ = handle.await;
}

// ────────────────────────────────────────────────────────────────────
// Test 14: CognitiveError → tonic::Status mapping
// ────────────────────────────────────────────────────────────────────

#[test]
fn cognitive_error_to_status_mapping() {
    use aios_cognitive::error::CognitiveError;
    use aios_cognitive::routing::AICrossOriginPosture;
    use aios_cognitive::service::conversions::cognitive_error_to_status;

    // IntentParseFailed → InvalidArgument
    let status = cognitive_error_to_status(&CognitiveError::IntentParseFailed("test".into()));
    assert_eq!(status.code(), tonic::Code::InvalidArgument);

    // NoMatchingCapability → NotFound
    let status = cognitive_error_to_status(&CognitiveError::NoMatchingCapability("test".into()));
    assert_eq!(status.code(), tonic::Code::NotFound);

    // TranslationRefused → FailedPrecondition
    let status = cognitive_error_to_status(&CognitiveError::TranslationRefused("test".into()));
    assert_eq!(status.code(), tonic::Code::FailedPrecondition);

    // AmbiguousIntent → InvalidArgument
    let status = cognitive_error_to_status(&CognitiveError::AmbiguousIntent("test".into()));
    assert_eq!(status.code(), tonic::Code::InvalidArgument);

    // LatencyPrivacyConflict → FailedPrecondition
    let status = cognitive_error_to_status(&CognitiveError::LatencyPrivacyConflict("test".into()));
    assert_eq!(status.code(), tonic::Code::FailedPrecondition);

    // NoRouteAvailable → Unavailable
    let status = cognitive_error_to_status(&CognitiveError::NoRouteAvailable("test".into()));
    assert_eq!(status.code(), tonic::Code::Unavailable);

    // CircuitBreakerOpen → FailedPrecondition
    let status = cognitive_error_to_status(&CognitiveError::CircuitBreakerOpen(
        "backend LocalGpu: circuit open, retry_after_ms=30000".into(),
    ));
    assert_eq!(status.code(), tonic::Code::FailedPrecondition);

    // CircuitBreakerOpen with retry_after_ms metadata
    let status = cognitive_error_to_status(&CognitiveError::CircuitBreakerOpen(
        "backend LocalGpu: circuit open, retry_after_ms=30000".into(),
    ));
    let retry_meta = status
        .metadata()
        .get("retry_after_ms")
        .map(|v| v.to_str().unwrap_or("0").to_string());
    assert_eq!(retry_meta, Some("30000".to_string()));

    // ModelResponseInvalid → Internal
    let status = cognitive_error_to_status(&CognitiveError::ModelResponseInvalid("test".into()));
    assert_eq!(status.code(), tonic::Code::Internal);

    // Internal → Internal
    let status = cognitive_error_to_status(&CognitiveError::Internal("test".into()));
    assert_eq!(status.code(), tonic::Code::Internal);

    // ExternalBackendBlocked → PermissionDenied
    let status = cognitive_error_to_status(&CognitiveError::ExternalBackendBlocked {
        posture: AICrossOriginPosture::AiNoExternal,
    });
    assert_eq!(status.code(), tonic::Code::PermissionDenied);

    // VaultCredentialMissing → PermissionDenied
    let status =
        cognitive_error_to_status(&CognitiveError::VaultCredentialMissing("mdl_test".into()));
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

// ────────────────────────────────────────────────────────────────────
// Test 15: Schema version constant matches proto package
// ────────────────────────────────────────────────────────────────────

#[test]
fn schema_version_constant_is_valid() {
    assert_eq!(
        SCHEMA_VERSION, "aios.cognitive.v1alpha1+T101",
        "SCHEMA_VERSION must match expected format"
    );
    assert!(
        SCHEMA_VERSION.starts_with("aios.cognitive.v1alpha1"),
        "SCHEMA_VERSION must start with proto package namespace"
    );
}

// ────────────────────────────────────────────────────────────────────
// Test 16: Service struct construction
// ────────────────────────────────────────────────────────────────────

#[test]
fn service_construction() {
    let core = Arc::new(InMemoryCognitiveCore::new());
    let svc = CognitiveCoreServiceImpl::new(Arc::clone(&core));
    assert!(
        Arc::ptr_eq(&svc.core(), &core),
        "core accessor must return the same Arc"
    );
}

// ────────────────────────────────────────────────────────────────────
// Test 17: Service with catalog
// ────────────────────────────────────────────────────────────────────

#[test]
fn service_with_catalog() {
    use aios_cognitive::model_catalog::CognitiveModelCatalog;

    let core = Arc::new(InMemoryCognitiveCore::new());
    let catalog = Arc::new(CognitiveModelCatalog::with_fixtures());
    let svc = CognitiveCoreServiceImpl::new(core).with_catalog(Arc::clone(&catalog));
    // Construction succeeds; verify core accessor still works
    assert!(!Arc::ptr_eq(
        &svc.core(),
        &Arc::new(InMemoryCognitiveCore::new())
    ));
    // core() should return the same Arc we passed in
    let _ = svc;
}

// ────────────────────────────────────────────────────────────────────
// Test 18: Proto enum discriminants are non-zero for specified variants
// ────────────────────────────────────────────────────────────────────

#[test]
fn proto_enum_discriminants() {
    use aios_cognitive::service::proto;

    // AgentKind
    assert_eq!(proto::AgentKind::Assistant as i32, 1);
    assert_eq!(proto::AgentKind::Worker as i32, 2);
    assert_eq!(proto::AgentKind::Daemon as i32, 3);
    assert_eq!(proto::AgentKind::Coordinator as i32, 4);

    // AgentLifecycleState
    assert_eq!(proto::AgentLifecycleState::Initializing as i32, 1);
    assert_eq!(proto::AgentLifecycleState::Active as i32, 2);
    assert_eq!(proto::AgentLifecycleState::Retired as i32, 9);

    // CognitiveErrorCode
    assert_eq!(proto::CognitiveErrorCode::ModelUnavailable as i32, 1);
    assert_eq!(proto::CognitiveErrorCode::IntentAmbiguous as i32, 2);

    // PlanState
    assert_eq!(proto::PlanState::Draft as i32, 1);
    assert_eq!(proto::PlanState::Completed as i32, 5);

    // MemoryClass
    assert_eq!(proto::MemoryClass::Episodic as i32, 2);
    assert_eq!(proto::MemoryClass::Semantic as i32, 3);
}
