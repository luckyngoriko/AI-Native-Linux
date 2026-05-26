//! Tests for provider dispatch — S13.2 §5 `ProviderClass` routing + vault-brokered
//! external calls (T-100).
//!
//! Covers: `ProviderDispatcher` construction, local/external dispatch semantics,
//! vault enforcement, `AI_NO_EXTERNAL` guard, INV-015 / INV-018 invariants, and
//! `InMemoryCognitiveCore` integration.

#![allow(clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::Mutex;

use aios_cognitive::{
    AICrossOriginPosture, CognitiveCore, CognitiveError, CognitiveIntent, CognitiveModel,
    DispatchOutcome, InMemoryCognitiveCore, IntentId, LatencyTier, ModelBackendKind, PrivacyClass,
    ProviderClass, ProviderDispatcher, SubjectRef, TranslationContext, VaultClientAdapter,
    VaultRequest, VaultResponse,
};

// ---------------------------------------------------------------------------
// Mock VaultClientAdapter for tests
// ---------------------------------------------------------------------------

/// Mock vault client that records calls and returns configurable responses.
struct MockVaultClient {
    calls: Mutex<Vec<(String, VaultRequest)>>,
    handle: String,
    output: String,
    latency_ms: u64,
}

impl MockVaultClient {
    fn new_success() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            handle: "vault_handle_stub_001".into(),
            output: "stub model output".into(),
            latency_ms: 55,
        }
    }
}

#[async_trait]
impl VaultClientAdapter for MockVaultClient {
    async fn use_capability(
        &self,
        capability_id: &str,
        request: VaultRequest,
    ) -> Result<VaultResponse, CognitiveError> {
        self.calls
            .lock()
            .await
            .push((capability_id.to_string(), request));
        Ok(VaultResponse {
            handle: self.handle.clone(),
            output: self.output.clone(),
            latency_ms: self.latency_ms,
        })
    }
}

// ---------------------------------------------------------------------------
// Helper factories
// ---------------------------------------------------------------------------

fn stub_intent() -> CognitiveIntent {
    CognitiveIntent {
        intent_id: IntentId::new(),
        subject: SubjectRef("agent:test".into()),
        natural_language: "test intent".into(),
        context_hash: "00".repeat(32),
        created_at: Utc::now(),
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Internal,
    }
}

fn stub_context() -> TranslationContext {
    TranslationContext {
        subject: SubjectRef("agent:test".into()),
        available_models: vec![],
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Internal,
        ai_cross_origin_posture: AICrossOriginPosture::AiVaultBrokeredOnly,
        recovery_mode: false,
        budget_ok: true,
    }
}

fn ollama_model() -> CognitiveModel {
    CognitiveModel {
        model_id: aios_cognitive::ModelId::new(),
        provider: ProviderClass::Ollama,
        capabilities: vec!["text-generation".into()],
        max_tokens: 8192,
        input_cost_per_1k: 0,
        output_cost_per_1k: 0,
        vault_capability_id: None,
        created_at: Utc::now(),
    }
}

fn vllm_model() -> CognitiveModel {
    CognitiveModel {
        model_id: aios_cognitive::ModelId::new(),
        provider: ProviderClass::Vllm,
        capabilities: vec!["text-generation".into()],
        max_tokens: 8192,
        input_cost_per_1k: 0,
        output_cost_per_1k: 0,
        vault_capability_id: None,
        created_at: Utc::now(),
    }
}

fn anthropic_model(vault_cap: Option<&str>) -> CognitiveModel {
    CognitiveModel {
        model_id: aios_cognitive::ModelId::new(),
        provider: ProviderClass::Anthropic,
        capabilities: vec!["text-generation".into()],
        max_tokens: 200_000,
        input_cost_per_1k: 15_000,
        output_cost_per_1k: 75_000,
        vault_capability_id: vault_cap.map(ToString::to_string),
        created_at: Utc::now(),
    }
}

fn other_vault_model() -> CognitiveModel {
    CognitiveModel {
        model_id: aios_cognitive::ModelId::new(),
        provider: ProviderClass::OtherVaultBrokered,
        capabilities: vec!["text-generation".into()],
        max_tokens: 131_072,
        input_cost_per_1k: 5_000,
        output_cost_per_1k: 15_000,
        vault_capability_id: Some("vcap_other_test".into()),
        created_at: Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// 1. ProviderDispatcher::new() succeeds
// ---------------------------------------------------------------------------

#[tokio::test]
async fn provider_dispatcher_new_succeeds() {
    let d = ProviderDispatcher::new();
    assert!(d
        .dispatch_to_provider(
            &ollama_model(),
            &stub_intent(),
            AICrossOriginPosture::AiVaultBrokeredOnly,
        )
        .await
        .is_ok());
}

// ---------------------------------------------------------------------------
// 2. LOCAL_OLLAMA model + any posture → LocalInvocation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn local_ollama_returns_local_invocation() {
    let d = ProviderDispatcher::new();
    let outcome = d
        .dispatch_to_provider(
            &ollama_model(),
            &stub_intent(),
            AICrossOriginPosture::AiVaultBrokeredOnly,
        )
        .await
        .expect("ollama dispatch should succeed");
    match outcome {
        DispatchOutcome::LocalInvocation { backend, .. } => {
            assert_eq!(backend, ModelBackendKind::LocalCpu);
        }
        _ => panic!("expected LocalInvocation, got {outcome:?}"),
    }
}

// ---------------------------------------------------------------------------
// 3. EXTERNAL_VAULT_BROKERED + no vault_client → Internal error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn external_no_vault_client_returns_internal_error() {
    let d = ProviderDispatcher::new();
    let err = d
        .dispatch_to_provider(
            &anthropic_model(Some("vcap_test")),
            &stub_intent(),
            AICrossOriginPosture::AiVaultBrokeredOnly,
        )
        .await
        .expect_err("should fail without vault client");
    match err {
        CognitiveError::Internal(msg) => {
            assert!(msg.contains("vault client not configured"));
        }
        other => panic!("expected Internal, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 4. EXTERNAL_VAULT_BROKERED + vault_client + NO vault_capability_id → VaultCredentialMissing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn external_no_capability_id_returns_vault_credential_missing() {
    let vault: Arc<dyn VaultClientAdapter> = Arc::new(MockVaultClient::new_success());
    let d = ProviderDispatcher::new().with_vault_client(vault);
    let model = anthropic_model(None); // no vault_capability_id
    let err = d
        .dispatch_to_provider(
            &model,
            &stub_intent(),
            AICrossOriginPosture::AiVaultBrokeredOnly,
        )
        .await
        .expect_err("should fail without capability id");
    assert!(matches!(err, CognitiveError::VaultCredentialMissing(_)));
    assert!(format!("{err}").contains("vault credential missing"));
}

// ---------------------------------------------------------------------------
// 5. EXTERNAL_VAULT_BROKERED + posture=AI_NO_EXTERNAL → ExternalBackendBlocked
// ---------------------------------------------------------------------------

#[tokio::test]
async fn external_ai_no_external_posture_returns_blocked() {
    let vault: Arc<dyn VaultClientAdapter> = Arc::new(MockVaultClient::new_success());
    let d = ProviderDispatcher::new().with_vault_client(vault);
    let err = d
        .dispatch_to_provider(
            &anthropic_model(Some("vcap_test")),
            &stub_intent(),
            AICrossOriginPosture::AiNoExternal,
        )
        .await
        .expect_err("should be blocked");
    assert!(matches!(err, CognitiveError::ExternalBackendBlocked { .. }));
}

// ---------------------------------------------------------------------------
// 6. EXTERNAL_VAULT_BROKERED + vault present + capability → VaultBrokeredInvocation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn external_with_vault_returns_vault_brokered_invocation() {
    let vault: Arc<dyn VaultClientAdapter> = Arc::new(MockVaultClient::new_success());
    let d = ProviderDispatcher::new().with_vault_client(vault);
    let outcome = d
        .dispatch_to_provider(
            &anthropic_model(Some("vcap_anthropic_test")),
            &stub_intent(),
            AICrossOriginPosture::AiVaultBrokeredOnly,
        )
        .await
        .expect("external dispatch should succeed");
    match outcome {
        DispatchOutcome::VaultBrokeredInvocation {
            ref vault_response_handle,
            tokens_in,
            tokens_out,
            latency_ms,
        } => {
            assert_eq!(vault_response_handle, "vault_handle_stub_001");
            assert!(tokens_in > 0);
            assert!(tokens_out > 0);
            assert!(latency_ms > 0);
        }
        _ => panic!("expected VaultBrokeredInvocation, got {outcome:?}"),
    }
}

// ---------------------------------------------------------------------------
// 7. OTHER_VAULT_BROKERED + vault present → VaultBrokeredInvocation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn other_vault_brokered_returns_vault_brokered_invocation() {
    let vault: Arc<dyn VaultClientAdapter> = Arc::new(MockVaultClient::new_success());
    let d = ProviderDispatcher::new().with_vault_client(vault);
    let outcome = d
        .dispatch_to_provider(
            &other_vault_model(),
            &stub_intent(),
            AICrossOriginPosture::AiVaultBrokeredOnly,
        )
        .await
        .expect("other vault dispatch should succeed");
    assert!(matches!(
        outcome,
        DispatchOutcome::VaultBrokeredInvocation { .. }
    ));
}

// ---------------------------------------------------------------------------
// 8. LOCAL_VLLM + posture=AI_NO_EXTERNAL → LocalInvocation (local allowed)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn local_vllm_with_no_external_still_returns_local_invocation() {
    let d = ProviderDispatcher::new();
    let outcome = d
        .dispatch_to_provider(
            &vllm_model(),
            &stub_intent(),
            AICrossOriginPosture::AiNoExternal,
        )
        .await
        .expect("local vLLM should succeed even with AI_NO_EXTERNAL");
    assert!(matches!(
        outcome,
        DispatchOutcome::LocalInvocation {
            backend: ModelBackendKind::LocalGpu,
            ..
        }
    ));
}

// ---------------------------------------------------------------------------
// 9. VaultClientAdapter as Arc<dyn ...> usage compiles (this test IS the check)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vault_client_adapter_dyn_trait_compiles_and_works() {
    let vault: Arc<dyn VaultClientAdapter> = Arc::new(MockVaultClient::new_success());
    let req = VaultRequest {
        operation: "test".into(),
        opaque_payload: "test_payload".into(),
    };
    let resp = vault
        .use_capability("vcap_test", req)
        .await
        .expect("mock vault should succeed");
    assert_eq!(resp.handle, "vault_handle_stub_001");
}

// ---------------------------------------------------------------------------
// 10. Mock VaultClientAdapter implementation for tests (verified by tests 6/7/9)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mock_vault_client_tracks_calls() {
    let vault = Arc::new(MockVaultClient::new_success());
    let d = ProviderDispatcher::new().with_vault_client(vault.clone());
    d.dispatch_to_provider(
        &anthropic_model(Some("vcap_test")),
        &stub_intent(),
        AICrossOriginPosture::AiVaultBrokeredOnly,
    )
    .await
    .expect("dispatch should succeed");
    let calls = vault.calls.lock().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "vcap_test");
    assert_eq!(calls[0].1.operation, "cognitive.invoke");
    drop(calls);
}

// ---------------------------------------------------------------------------
// 11. INV-015: DispatchOutcome serialisation contains no prompt/response bodies
// ---------------------------------------------------------------------------

#[test]
fn inv_015_dispatch_outcome_serialization_no_prompt_or_response_bodies() {
    let outcome = DispatchOutcome::LocalInvocation {
        backend: ModelBackendKind::LocalCpu,
        tokens_in: 10,
        tokens_out: 50,
        latency_ms: 42,
    };
    let json = serde_json::to_string(&outcome).expect("serialisation should succeed");
    // No prompt, response, or large body fields (quote-delimited JSON keys)
    assert!(!json.contains("\"prompt\""));
    assert!(!json.contains("\"response\""));
    assert!(!json.contains("\"body\""));
    // Token/latency fields ARE present
    assert!(json.contains("tokens_in"));
    assert!(json.contains("tokens_out"));

    let vault_outcome = DispatchOutcome::VaultBrokeredInvocation {
        vault_response_handle: "h".into(),
        tokens_in: 3,
        tokens_out: 12,
        latency_ms: 100,
    };
    let vjson = serde_json::to_string(&vault_outcome).expect("serialisation should succeed");
    // No field named "prompt" or "response" (quote-delimited JSON keys)
    assert!(!vjson.contains("\"prompt\""));
    assert!(!vjson.contains("\"response\""));
    // Handle is an opaque string, not a body
    assert!(vjson.contains("vault_response_handle"));
}

#[test]
fn inv_015_denied_variant_contains_no_bodies() {
    let denied = DispatchOutcome::Denied {
        reason: "budget exceeded".into(),
        posture: AICrossOriginPosture::AiNoExternal,
    };
    let json = serde_json::to_string(&denied).expect("serialisation should succeed");
    assert!(!json.contains("\"prompt\""));
    assert!(!json.contains("\"response\""));
    assert!(!json.contains("\"body\""));
    assert!(json.contains("reason"));
    assert!(json.contains("AI_NO_EXTERNAL"));
}

// ---------------------------------------------------------------------------
// 12. INV-018: VaultRequest / VaultResponse contain no raw key fields
// ---------------------------------------------------------------------------

#[test]
fn inv_018_vault_request_response_no_raw_key_fields() {
    let req = VaultRequest {
        operation: "cognitive.invoke".into(),
        opaque_payload: "payload".into(),
    };
    let debug_str = format!("{req:?}");
    // No byte arrays or hex key patterns in Debug output
    assert!(!debug_str.contains("[0"));
    assert!(!debug_str.contains("key"));
    assert!(!debug_str.contains("secret"));
    // Fields are all Strings
    assert!(debug_str.contains("cognitive.invoke"));

    let resp = VaultResponse {
        handle: "opaque_handle".into(),
        output: "model output".into(),
        latency_ms: 42,
    };
    let resp_debug = format!("{resp:?}");
    assert!(!resp_debug.contains("[0"));
    assert!(!resp_debug.contains("key"));
    // Handle is an opaque string, not raw bytes
    assert!(resp_debug.contains("opaque_handle"));
}

// ---------------------------------------------------------------------------
// 13. InMemoryCognitiveCore.with_provider_dispatcher integration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn in_memory_core_with_dispatcher_populates_tokens() {
    let vault: Arc<dyn VaultClientAdapter> = Arc::new(MockVaultClient::new_success());
    let dispatcher = Arc::new(ProviderDispatcher::new().with_vault_client(vault));
    let model = anthropic_model(Some("vcap_test"));
    let core = InMemoryCognitiveCore::with_models(vec![model.clone()])
        .with_provider_dispatcher(dispatcher);

    let intent = stub_intent();
    let mut ctx = stub_context();
    ctx.available_models = vec![model];
    ctx.ai_cross_origin_posture = AICrossOriginPosture::AiVaultBrokeredOnly;

    let result = core
        .translate_intent(&intent, &ctx)
        .await
        .expect("translate should succeed");
    // Tokens should be non-zero because dispatcher is configured
    assert!(
        result.translation_provenance.tokens_in > 0,
        "tokens_in should be >0 with dispatcher, got {}",
        result.translation_provenance.tokens_in
    );
    assert!(
        result.translation_provenance.tokens_out > 0,
        "tokens_out should be >0 with dispatcher, got {}",
        result.translation_provenance.tokens_out
    );
}

// ---------------------------------------------------------------------------
// 14. Backward compat: InMemoryCognitiveCore without dispatcher → tokens stay 0
// ---------------------------------------------------------------------------

#[tokio::test]
async fn in_memory_core_without_dispatcher_keeps_tokens_zero() {
    let core = InMemoryCognitiveCore::new();
    let intent = stub_intent();
    let ctx = stub_context();
    let result = core
        .translate_intent(&intent, &ctx)
        .await
        .expect("translate should succeed");
    assert_eq!(result.translation_provenance.tokens_in, 0);
    assert_eq!(result.translation_provenance.tokens_out, 0);
}

// ---------------------------------------------------------------------------
// 15. Concurrent dispatch_to_provider from 3 tokio tasks
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_dispatch_three_tasks_all_complete() {
    let vault: Arc<dyn VaultClientAdapter> = Arc::new(MockVaultClient::new_success());
    let dispatcher = Arc::new(ProviderDispatcher::new().with_vault_client(vault));

    let model_a = anthropic_model(Some("vcap_a"));
    let model_b = anthropic_model(Some("vcap_b"));
    let model_c = ollama_model();
    let intent_a = stub_intent();
    let intent_b = stub_intent();
    let intent_c = stub_intent();
    let dispatcher_a = Arc::clone(&dispatcher);
    let dispatcher_b = Arc::clone(&dispatcher);
    let dispatcher_c = Arc::clone(&dispatcher);

    let (r1, r2, r3) = tokio::join!(
        tokio::spawn(async move {
            dispatcher_a
                .dispatch_to_provider(
                    &model_a,
                    &intent_a,
                    AICrossOriginPosture::AiVaultBrokeredOnly,
                )
                .await
        }),
        tokio::spawn(async move {
            dispatcher_b
                .dispatch_to_provider(
                    &model_b,
                    &intent_b,
                    AICrossOriginPosture::AiVaultBrokeredOnly,
                )
                .await
        }),
        tokio::spawn(async move {
            dispatcher_c
                .dispatch_to_provider(
                    &model_c,
                    &intent_c,
                    AICrossOriginPosture::AiVaultBrokeredOnly,
                )
                .await
        }),
    );

    let o1 = r1.expect("task 1 panicked").expect("task 1 failed");
    let o2 = r2.expect("task 2 panicked").expect("task 2 failed");
    let o3 = r3.expect("task 3 panicked").expect("task 3 failed");

    assert!(matches!(
        o1,
        DispatchOutcome::VaultBrokeredInvocation { .. }
    ));
    assert!(matches!(
        o2,
        DispatchOutcome::VaultBrokeredInvocation { .. }
    ));
    assert!(matches!(o3, DispatchOutcome::LocalInvocation { .. }));
}
