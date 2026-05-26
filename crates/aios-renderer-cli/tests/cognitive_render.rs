//! T-104 test coverage for cognitive rendering and `aios cognitive` CLI.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::too_many_lines,
    reason = "tests use panic-on-failure as the idiomatic signal"
)]

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_cognitive::core::IntentCapability;
use aios_cognitive::{
    CircuitState, CognitiveIntent, CognitiveModel, IntentId, LatencyTier, ModelBackendKind,
    PrivacyClass, ProviderClass, RoutingDecision, SubjectRef, TranslationProvenance,
    TranslationResult,
};
use aios_renderer_cli::{
    AiosCli, AiosCommand, CircuitStateList, CognitiveIntentCapabilityList, CognitiveModelList,
    CognitiveSubcommand, InProcessBackend, OutputFormat, RenderContext, Renderable,
};
use chrono::Utc;
use clap::Parser;

fn ctx(color: bool) -> RenderContext {
    RenderContext {
        color,
        width: Some(200),
        redact_secrets: true,
        verbose: false,
        locale: "en_US.UTF-8".to_owned(),
    }
}

fn fixed_time() -> chrono::DateTime<Utc> {
    Utc::now()
}

fn make_intent() -> CognitiveIntent {
    CognitiveIntent {
        intent_id: IntentId("cogi_7ABCDEFGHIJKLMNOPQRSTUVWXYZ".into()),
        subject: SubjectRef("human:lucky".into()),
        natural_language: "restart the nginx service".into(),
        context_hash: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6".into(),
        created_at: fixed_time(),
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Internal,
    }
}

fn make_translation_result() -> TranslationResult {
    TranslationResult {
        intent_id: IntentId("cogi_7ABCDEFGHIJKLMNOPQRSTUVWXYZ".into()),
        produced_action: ActionEnvelope::new(
            Identity::new("human:lucky", false),
            Request::new("service.restart", serde_json::json!({"service": "nginx"})),
            Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
        ),
        routing_decision_id: Some("rtdg_01ABCDEFGHIJKLMNOPQRSTUVWXYZ".into()),
        verification_intent: Some("vrfi_01ABCDEFGHIJKLMNOPQRSTUVWXYZ".into()),
        translation_provenance: TranslationProvenance {
            translator_version: "0.1.0-T098".into(),
            model_used: "claude-sonnet-4-6".into(),
            tokens_in: 120,
            tokens_out: 45,
            model_signed_response: Some("sig_abc123".into()),
        },
        translated_at: fixed_time(),
    }
}

fn make_model() -> CognitiveModel {
    CognitiveModel {
        model_id: aios_cognitive::ModelId("mdl_01ABCDEFGHIJKLMNOPQRSTUVWXYZ".into()),
        provider: ProviderClass::Anthropic,
        capabilities: vec!["text-generation".into(), "code-completion".into()],
        max_tokens: 200_000,
        input_cost_per_1k: 15_000,
        output_cost_per_1k: 75_000,
        vault_capability_id: Some("vcap_01ABCDEFGHIJKLMNOPQRSTUVWXYZ".into()),
        created_at: fixed_time(),
    }
}

fn make_routing_decision() -> RoutingDecision {
    RoutingDecision {
        routing_id: "rtdg_01ABCDEFGHIJKLMNOPQRSTUVWXYZ".into(),
        chosen_backend: ModelBackendKind::LocalGpu,
        provider_class: ProviderClass::Vllm,
        backend_id: "aios-vllm-adapter".into(),
        matched_rule: 5,
        degraded: false,
        reason: None,
        decided_at: fixed_time(),
    }
}

fn make_intent_capabilities() -> Vec<IntentCapability> {
    vec![
        IntentCapability {
            intent_kind: "service.restart".into(),
            description: "Restart a systemd service unit".into(),
            requires_latency_tier: LatencyTier::T3LocalCognitive,
            produces_action_type: "service.restart".into(),
            max_tokens_estimate: 80,
        },
        IntentCapability {
            intent_kind: "service.status".into(),
            description: "Query service status".into(),
            requires_latency_tier: LatencyTier::T1Deterministic,
            produces_action_type: "service.status".into(),
            max_tokens_estimate: 40,
        },
    ]
}

// ---------------------------------------------------------------------------
// CognitiveIntent rendering
// ---------------------------------------------------------------------------

#[test]
fn cognitive_intent_text_includes_intent_id_and_subject() {
    let intent = make_intent();
    let output = intent.render(OutputFormat::Text, &ctx(false)).unwrap();
    assert!(output.contains("cogi_"));
    assert!(output.contains("human:lucky"));
    assert!(output.contains("restart the nginx service"));
}

#[test]
fn cognitive_intent_json_round_trips_through_serde() {
    let intent = make_intent();
    let output = intent.render(OutputFormat::Json, &ctx(false)).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["intent_id"], "cogi_7ABCDEFGHIJKLMNOPQRSTUVWXYZ");
    assert_eq!(parsed["subject"], "human:lucky");
}

#[test]
fn cognitive_intent_tree_includes_subject_and_utterance() {
    let intent = make_intent();
    let output = intent.render(OutputFormat::Tree, &ctx(false)).unwrap();
    assert!(output.contains("human:lucky"));
    assert!(output.contains("restart the nginx service"));
}

#[test]
fn cognitive_intent_table_has_expected_columns() {
    let intent = make_intent();
    let output = intent.render(OutputFormat::Table, &ctx(false)).unwrap();
    assert!(output.contains("intent_id"));
    assert!(output.contains("subject"));
    assert!(output.contains("latency"));
}

// ---------------------------------------------------------------------------
// TranslationResult rendering
// ---------------------------------------------------------------------------

#[test]
fn translation_result_text_shows_provenance_tokens() {
    let result = make_translation_result();
    let output = result.render(OutputFormat::Text, &ctx(false)).unwrap();
    assert!(output.contains("cogi_"));
    assert!(output.contains("service.restart"));
}

#[test]
fn translation_result_json_includes_routing_decision_id() {
    let result = make_translation_result();
    let output = result.render(OutputFormat::Json, &ctx(false)).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(
        parsed["routing_decision_id"],
        "rtdg_01ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    );
}

#[test]
fn translation_result_tree_shows_provenance() {
    let result = make_translation_result();
    let output = result.render(OutputFormat::Tree, &ctx(false)).unwrap();
    assert!(output.contains("claude-sonnet-4-6"));
    assert!(output.contains("120"));
    assert!(output.contains("45"));
}

// ---------------------------------------------------------------------------
// LatencyTier colour coding
// ---------------------------------------------------------------------------

#[test]
fn latency_tier_colour_in_text_mode_when_color_enabled() {
    let intent = make_intent();
    let colored = intent.render(OutputFormat::Text, &ctx(true)).unwrap();
    let plain = intent.render(OutputFormat::Text, &ctx(false)).unwrap();
    assert!(colored.contains("\x1b["));
    assert!(!plain.contains("\x1b["));
}

#[test]
fn latency_tier_t0_uses_green_ansi_code() {
    let intent = CognitiveIntent {
        latency_class: LatencyTier::T0CachedUiState,
        ..make_intent()
    };
    let output = intent.render(OutputFormat::Text, &ctx(true)).unwrap();
    assert!(output.contains("\x1b[32m"));
}

#[test]
fn latency_tier_t4_uses_magenta_ansi_code() {
    let intent = CognitiveIntent {
        latency_class: LatencyTier::T4PowerfulReasoning,
        ..make_intent()
    };
    let output = intent.render(OutputFormat::Text, &ctx(true)).unwrap();
    assert!(output.contains("\x1b[35m"));
}

// ---------------------------------------------------------------------------
// ModelBackendKind rendering (8 variants)
// ---------------------------------------------------------------------------

#[test]
fn model_backend_kind_text_renders_all_eight_variants() {
    let kinds = [
        ModelBackendKind::LocalCpu,
        ModelBackendKind::LocalGpu,
        ModelBackendKind::LocalDistributed,
        ModelBackendKind::ExternalVaultBrokered,
        ModelBackendKind::FallbackRuleBased,
        ModelBackendKind::Cached,
        ModelBackendKind::DegradedNull,
        ModelBackendKind::Forbidden,
    ];
    for kind in &kinds {
        let output = kind.render(OutputFormat::Text, &ctx(false)).unwrap();
        assert!(!output.is_empty(), "empty output for {kind:?}");
    }
}

#[test]
fn model_backend_kind_json_serializes_as_screaming_snake_case() {
    let kind = ModelBackendKind::ExternalVaultBrokered;
    let output = kind.render(OutputFormat::Json, &ctx(false)).unwrap();
    assert!(output.contains("EXTERNAL_VAULT_BROKERED"));
}

// ---------------------------------------------------------------------------
// CircuitState rendering
// ---------------------------------------------------------------------------

#[test]
fn circuit_state_closed_renders_green_in_color_mode() {
    let state = CircuitState::Closed;
    let output = state.render(OutputFormat::Text, &ctx(true)).unwrap();
    assert!(output.contains("\x1b[32m"));
}

#[test]
fn circuit_state_open_renders_red_in_color_mode() {
    let state = CircuitState::Open;
    let output = state.render(OutputFormat::Text, &ctx(true)).unwrap();
    assert!(output.contains("\x1b[31m"));
}

#[test]
fn circuit_state_half_open_renders_yellow_in_color_mode() {
    let state = CircuitState::HalfOpen;
    let output = state.render(OutputFormat::Text, &ctx(true)).unwrap();
    assert!(output.contains("\x1b[33m"));
}

// ---------------------------------------------------------------------------
// CognitiveModel rendering
// ---------------------------------------------------------------------------

#[test]
fn cognitive_model_text_shows_provider_and_capabilities() {
    let model = make_model();
    let output = model.render(OutputFormat::Text, &ctx(false)).unwrap();
    assert!(output.contains("mdl_"));
    assert!(output.contains("Anthropic"));
    assert!(output.contains("text-generation"));
}

#[test]
fn cognitive_model_json_includes_token_and_cost_fields() {
    let model = make_model();
    let output = model.render(OutputFormat::Json, &ctx(false)).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["max_tokens"], 200_000);
    assert_eq!(parsed["input_cost_per_1k"], 15_000);
}

#[test]
fn cognitive_model_table_has_provider_and_cost_columns() {
    let model = make_model();
    let output = model.render(OutputFormat::Table, &ctx(false)).unwrap();
    assert!(output.contains("model_id"));
    assert!(output.contains("provider"));
    assert!(output.contains("max_tokens"));
}

// ---------------------------------------------------------------------------
// RoutingDecision rendering
// ---------------------------------------------------------------------------

#[test]
fn routing_decision_text_shows_backend_and_rule() {
    let decision = make_routing_decision();
    let output = decision.render(OutputFormat::Text, &ctx(false)).unwrap();
    assert!(output.contains("rtdg_"));
    assert!(output.contains("LocalGpu"));
    assert!(output.contains('5'));
}

#[test]
fn routing_decision_degraded_shows_flag_and_reason() {
    let decision = RoutingDecision {
        degraded: true,
        reason: Some("all primary backends unhealthy".into()),
        ..make_routing_decision()
    };
    let output = decision.render(OutputFormat::Text, &ctx(false)).unwrap();
    assert!(output.contains("degraded: true"));
    assert!(output.contains("all primary backends unhealthy"));
}

#[test]
fn routing_decision_tree_shows_degraded_status() {
    let decision = RoutingDecision {
        degraded: true,
        ..make_routing_decision()
    };
    let output = decision.render(OutputFormat::Tree, &ctx(false)).unwrap();
    assert!(output.contains("degraded: true"));
}

// ---------------------------------------------------------------------------
// TranslationProvenance rendering
// ---------------------------------------------------------------------------

#[test]
fn translation_provenance_text_shows_model_and_version() {
    let provenance = TranslationProvenance {
        translator_version: "0.1.0-T098".into(),
        model_used: "claude-sonnet-4-6".into(),
        tokens_in: 256,
        tokens_out: 128,
        model_signed_response: None,
    };
    let output = provenance.render(OutputFormat::Text, &ctx(false)).unwrap();
    assert!(output.contains("claude-sonnet-4-6"));
    assert!(output.contains("0.1.0-T098"));
}

#[test]
fn translation_provenance_json_omits_null_signed_response() {
    let provenance = TranslationProvenance {
        translator_version: "0.1.0".into(),
        model_used: "test-model".into(),
        tokens_in: 0,
        tokens_out: 0,
        model_signed_response: None,
    };
    let output = provenance.render(OutputFormat::Json, &ctx(false)).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(parsed["model_signed_response"].is_null());
}

// ---------------------------------------------------------------------------
// CognitiveIntentCapabilityList wrapper
// ---------------------------------------------------------------------------

#[test]
fn cognitive_intent_capability_list_text_shows_count_and_capabilities() {
    let caps = make_intent_capabilities();
    let list = CognitiveIntentCapabilityList::new(caps);
    assert_eq!(list.capabilities().len(), 2);
    let output = list.render(OutputFormat::Text, &ctx(false)).unwrap();
    assert!(output.contains("service.restart"));
    assert!(output.contains("service.status"));
}

#[test]
fn cognitive_intent_capability_list_json_is_array_wrapped() {
    let caps = make_intent_capabilities();
    let list = CognitiveIntentCapabilityList::new(caps);
    let output = list.render(OutputFormat::Json, &ctx(false)).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(parsed.is_object());
}

#[test]
fn cognitive_intent_capability_list_tree_shows_each_capability() {
    let caps = make_intent_capabilities();
    let list = CognitiveIntentCapabilityList::new(caps);
    let output = list.render(OutputFormat::Tree, &ctx(false)).unwrap();
    assert!(output.contains("service.restart"));
    assert!(output.contains("Restart a systemd service unit"));
}

// ---------------------------------------------------------------------------
// CognitiveModelList wrapper
// ---------------------------------------------------------------------------

#[test]
fn cognitive_model_list_text_shows_models() {
    let models = vec![make_model()];
    let list = CognitiveModelList::new(models);
    let output = list.render(OutputFormat::Text, &ctx(false)).unwrap();
    assert!(output.contains("mdl_"));
    assert!(output.contains("Anthropic"));
}

#[test]
fn cognitive_model_list_json_contains_model_fields() {
    let models = vec![make_model()];
    let list = CognitiveModelList::new(models);
    let output = list.render(OutputFormat::Json, &ctx(false)).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(parsed.is_object());
}

#[test]
fn cognitive_model_list_empty_renders_zero_count() {
    let list = CognitiveModelList::new(vec![]);
    let output = list.render(OutputFormat::Text, &ctx(false)).unwrap();
    assert!(output.contains("models"));
    assert!(output.contains('0'));
}

// ---------------------------------------------------------------------------
// CircuitStateList wrapper
// ---------------------------------------------------------------------------

#[test]
fn circuit_state_list_text_shows_backend_and_state() {
    let entries = vec![
        (ModelBackendKind::LocalCpu, CircuitState::Closed),
        (ModelBackendKind::ExternalVaultBrokered, CircuitState::Open),
    ];
    let list = CircuitStateList::new(entries);
    let output = list.render(OutputFormat::Text, &ctx(false)).unwrap();
    assert!(output.contains("LocalCpu"));
    assert!(output.contains("Closed"));
    assert!(output.contains("ExternalVaultBrokered"));
    assert!(output.contains("Open"));
}

#[test]
fn circuit_state_list_json_is_an_object() {
    let entries = vec![(ModelBackendKind::LocalGpu, CircuitState::HalfOpen)];
    let list = CircuitStateList::new(entries);
    let output = list.render(OutputFormat::Json, &ctx(false)).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(parsed.is_object());
}

// ---------------------------------------------------------------------------
// ANSI colour presence / absence
// ---------------------------------------------------------------------------

#[test]
fn no_ansi_codes_in_plain_text_mode() {
    let intent = make_intent();
    let output = intent.render(OutputFormat::Text, &ctx(false)).unwrap();
    assert!(!output.contains('\x1b'));
}

#[test]
fn no_ansi_codes_in_json_mode_even_with_color() {
    let intent = make_intent();
    let output = intent.render(OutputFormat::Json, &ctx(true)).unwrap();
    assert!(!output.contains('\x1b'));
}

// ---------------------------------------------------------------------------
// CLI integration — `aios cognitive` subcommand dispatch
// ---------------------------------------------------------------------------

#[test]
fn cli_parse_cognitive_intents_subcommand() {
    let cli = AiosCli::try_parse_from(["aios", "cognitive", "intents"]).expect("parse");
    assert!(matches!(
        cli.command,
        AiosCommand::Cognitive {
            subcommand: CognitiveSubcommand::Intents
        }
    ));
}

#[test]
fn cli_parse_cognitive_models_subcommand() {
    let cli = AiosCli::try_parse_from(["aios", "cognitive", "models"]).expect("parse");
    assert!(matches!(
        cli.command,
        AiosCommand::Cognitive {
            subcommand: CognitiveSubcommand::Models
        }
    ));
}

#[test]
fn cli_parse_cognitive_circuits_subcommand() {
    let cli = AiosCli::try_parse_from(["aios", "cognitive", "circuits"]).expect("parse");
    assert!(matches!(
        cli.command,
        AiosCommand::Cognitive {
            subcommand: CognitiveSubcommand::Circuits
        }
    ));
}

#[test]
fn cli_parse_cognitive_translate_subcommand() {
    let cli = AiosCli::try_parse_from([
        "aios",
        "cognitive",
        "translate",
        "--utterance",
        "restart nginx",
    ])
    .expect("parse");
    assert!(matches!(
        cli.command,
        AiosCommand::Cognitive {
            subcommand: CognitiveSubcommand::Translate { .. }
        }
    ));
}

// ---------------------------------------------------------------------------
// In-process backend cognitive integration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn in_process_backend_cognitive_intents_returns_capabilities() {
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    let intents = client
        .list_supported_intents()
        .await
        .expect("list supported intents");

    assert!(!intents.is_empty());
    assert!(intents.iter().any(|c| c.intent_kind == "service.restart"));

    shutdown.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn in_process_backend_cognitive_translate_intent_returns_id() {
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    let intent_id = client
        .translate_intent("restart the nginx service", "agent:cli:default")
        .await
        .expect("translate intent");

    assert!(!intent_id.is_empty());
    assert!(intent_id.starts_with("cogi_"));

    shutdown.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn in_process_backend_cognitive_circuits_returns_closed() {
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    let state = client
        .get_circuit_state(ModelBackendKind::LocalCpu)
        .await
        .expect("get circuit state");

    assert_eq!(state, CircuitState::Closed);

    shutdown.shutdown().await.expect("shutdown");
}
