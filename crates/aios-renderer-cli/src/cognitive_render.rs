//! Cross-crate renderers for L5 Cognitive Core types.

use serde::Serialize;

use aios_cognitive::core::IntentCapability;
use aios_cognitive::{
    CircuitState, CognitiveIntent, CognitiveModel, LatencyTier, ModelBackendKind, RoutingDecision,
    TranslationProvenance, TranslationResult,
};

use crate::{
    OutputFormat, RenderContext, RenderError, Renderable, TableAlign, TableRenderer, TableSpec,
    TextRenderer, TreeNode, TreeRenderer,
};

// ---------------------------------------------------------------------------
// ANSI colour tokens
// ---------------------------------------------------------------------------

const GREEN: &str = "\x1b[32m";
const CYAN: &str = "\x1b[36m";
const BLUE: &str = "\x1b[34m";
const YELLOW: &str = "\x1b[33m";
const MAGENTA: &str = "\x1b[35m";
const RED: &str = "\x1b[31m";
const WHITE: &str = "\x1b[37m";
const RESET: &str = "\x1b[0m";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn colour(code: &str, text: &str, ctx: &RenderContext) -> String {
    if ctx.color {
        format!("{code}{text}{RESET}")
    } else {
        text.to_owned()
    }
}

const fn latency_colour(tier: LatencyTier) -> &'static str {
    match tier {
        LatencyTier::T0CachedUiState => GREEN,
        LatencyTier::T1Deterministic => CYAN,
        LatencyTier::T2CatalogRetrieval => BLUE,
        LatencyTier::T3LocalCognitive => YELLOW,
        LatencyTier::T4PowerfulReasoning => MAGENTA,
    }
}

fn latency_display(tier: LatencyTier, ctx: &RenderContext) -> String {
    let name = format!("{tier:?}");
    colour(latency_colour(tier), &name, ctx)
}

const fn backend_colour(kind: ModelBackendKind) -> &'static str {
    match kind {
        ModelBackendKind::LocalCpu => GREEN,
        ModelBackendKind::LocalGpu => CYAN,
        ModelBackendKind::LocalDistributed => BLUE,
        ModelBackendKind::ExternalVaultBrokered => YELLOW,
        ModelBackendKind::FallbackRuleBased => MAGENTA,
        ModelBackendKind::Cached => WHITE,
        ModelBackendKind::DegradedNull | ModelBackendKind::Forbidden => RED,
    }
}

fn backend_display(kind: ModelBackendKind, ctx: &RenderContext) -> String {
    let name = format!("{kind:?}");
    colour(backend_colour(kind), &name, ctx)
}

const fn circuit_colour(state: CircuitState) -> &'static str {
    match state {
        CircuitState::Closed => GREEN,
        CircuitState::HalfOpen => YELLOW,
        CircuitState::Open => RED,
    }
}

fn circuit_display(state: CircuitState, ctx: &RenderContext) -> String {
    let name = format!("{state:?}");
    colour(circuit_colour(state), &name, ctx)
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_owned()
    } else {
        let truncated: String = s.chars().take(max_len - 3).collect();
        format!("{truncated}...")
    }
}

fn format_cost(micro_units: u64) -> String {
    if micro_units == 0 {
        "free".to_owned()
    } else {
        let whole = micro_units / 1_000_000;
        let frac = (micro_units % 1_000_000) / 10_000;
        format!("${whole}.{frac:02}/1k")
    }
}

fn action_summary(envelope: &aios_action::ActionEnvelope) -> String {
    let action_kind = &envelope.request.action;
    let target = envelope
        .request
        .target
        .get("action_target")
        .and_then(|v| v.as_str())
        .or_else(|| {
            envelope
                .request
                .target
                .get("intent_id")
                .and_then(|v| v.as_str())
        })
        .unwrap_or("(json payload)");
    format!("{action_kind} → {target}")
}

// ---------------------------------------------------------------------------
// CognitiveIntent
// ---------------------------------------------------------------------------

impl Renderable for CognitiveIntent {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let r = TextRenderer::new(ctx.clone());
                let lines = vec![
                    r.render_kv("intent_id", self.intent_id.0.as_str()),
                    r.render_kv("subject", &self.subject.0),
                    r.render_kv(
                        "natural_language",
                        &truncate_str(&self.natural_language, 80),
                    ),
                    r.render_kv("context_hash", &self.context_hash),
                    r.render_kv("created_at", &self.created_at.to_rfc3339()),
                    r.render_kv("latency_class", &latency_display(self.latency_class, ctx)),
                    r.render_kv("privacy_class", &format!("{:?}", self.privacy_class)),
                ];
                Ok(r.render_section("CognitiveIntent", &lines))
            }
            OutputFormat::Json => serde_json::to_string(self)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: format!("intent {}", self.intent_id.0),
                    children: vec![
                        TreeNode {
                            label: format!("subject: {}", self.subject.0),
                            children: vec![],
                        },
                        TreeNode {
                            label: format!("query: {}", truncate_str(&self.natural_language, 60)),
                            children: vec![],
                        },
                        TreeNode {
                            label: format!("latency: {}", latency_display(self.latency_class, ctx)),
                            children: vec![],
                        },
                        TreeNode {
                            label: format!("privacy: {:?}", self.privacy_class),
                            children: vec![],
                        },
                        TreeNode {
                            label: format!("created: {}", self.created_at.to_rfc3339()),
                            children: vec![],
                        },
                    ],
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec![
                        "intent_id".into(),
                        "subject".into(),
                        "latency".into(),
                        "privacy".into(),
                    ],
                    rows: vec![vec![
                        self.intent_id.0.clone(),
                        self.subject.0.clone(),
                        latency_display(self.latency_class, ctx),
                        format!("{:?}", self.privacy_class),
                    ]],
                    align: vec![TableAlign::Left; 4],
                };
                TableRenderer::new(ctx.clone()).render(&spec)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TranslationResult
// ---------------------------------------------------------------------------

impl Renderable for TranslationResult {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let r = TextRenderer::new(ctx.clone());
                let lines = vec![
                    r.render_kv("intent_id", self.intent_id.0.as_str()),
                    r.render_kv("produced_action", &action_summary(&self.produced_action)),
                    r.render_kv(
                        "routing_decision_id",
                        self.routing_decision_id.as_deref().unwrap_or("(none)"),
                    ),
                    r.render_kv(
                        "verification_intent",
                        self.verification_intent.as_deref().unwrap_or("(none)"),
                    ),
                    r.render_kv("translated_at", &self.translated_at.to_rfc3339()),
                ];
                Ok(r.render_section("TranslationResult", &lines))
            }
            OutputFormat::Json => serde_json::to_string(self)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: format!("translation {}", self.intent_id.0),
                    children: vec![
                        TreeNode {
                            label: format!("action: {}", action_summary(&self.produced_action)),
                            children: vec![],
                        },
                        TreeNode {
                            label: "provenance".into(),
                            children: vec![
                                TreeNode {
                                    label: format!(
                                        "translator: {}",
                                        self.translation_provenance.translator_version
                                    ),
                                    children: vec![],
                                },
                                TreeNode {
                                    label: format!(
                                        "model: {}",
                                        self.translation_provenance.model_used
                                    ),
                                    children: vec![],
                                },
                                TreeNode {
                                    label: format!(
                                        "tokens: {}→{}",
                                        self.translation_provenance.tokens_in,
                                        self.translation_provenance.tokens_out
                                    ),
                                    children: vec![],
                                },
                            ],
                        },
                        TreeNode {
                            label: format!("translated_at: {}", self.translated_at.to_rfc3339()),
                            children: vec![],
                        },
                    ],
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec![
                        "intent_id".into(),
                        "action".into(),
                        "model".into(),
                        "tokens".into(),
                    ],
                    rows: vec![vec![
                        self.intent_id.0.clone(),
                        action_summary(&self.produced_action),
                        self.translation_provenance.model_used.clone(),
                        format!(
                            "{}→{}",
                            self.translation_provenance.tokens_in,
                            self.translation_provenance.tokens_out
                        ),
                    ]],
                    align: vec![TableAlign::Left; 4],
                };
                TableRenderer::new(ctx.clone()).render(&spec)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TranslationProvenance
// ---------------------------------------------------------------------------

impl Renderable for TranslationProvenance {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let r = TextRenderer::new(ctx.clone());
                let lines = vec![
                    r.render_kv("translator_version", &self.translator_version),
                    r.render_kv("model_used", &self.model_used),
                    r.render_kv("tokens_in", &self.tokens_in.to_string()),
                    r.render_kv("tokens_out", &self.tokens_out.to_string()),
                    r.render_kv(
                        "model_signed_response",
                        self.model_signed_response.as_deref().unwrap_or("(none)"),
                    ),
                ];
                Ok(r.render_section("TranslationProvenance", &lines))
            }
            OutputFormat::Json => serde_json::to_string(self)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: "provenance".into(),
                    children: vec![
                        TreeNode {
                            label: format!("version: {}", self.translator_version),
                            children: vec![],
                        },
                        TreeNode {
                            label: format!("model: {}", self.model_used),
                            children: vec![],
                        },
                        TreeNode {
                            label: format!(
                                "tokens: {} in, {} out",
                                self.tokens_in, self.tokens_out
                            ),
                            children: vec![],
                        },
                    ],
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec![
                        "translator".into(),
                        "model".into(),
                        "tokens_in".into(),
                        "tokens_out".into(),
                    ],
                    rows: vec![vec![
                        self.translator_version.clone(),
                        self.model_used.clone(),
                        self.tokens_in.to_string(),
                        self.tokens_out.to_string(),
                    ]],
                    align: vec![TableAlign::Left; 4],
                };
                TableRenderer::new(ctx.clone()).render(&spec)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// LatencyTier
// ---------------------------------------------------------------------------

impl Renderable for LatencyTier {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(latency_display(*self, ctx)),
            OutputFormat::Json => serde_json::to_string(self)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: latency_display(*self, ctx),
                    children: vec![],
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec!["tier".into(), "display".into()],
                    rows: vec![vec![format!("{:?}", self), latency_display(*self, ctx)]],
                    align: vec![TableAlign::Left, TableAlign::Left],
                };
                TableRenderer::new(ctx.clone()).render(&spec)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ModelBackendKind
// ---------------------------------------------------------------------------

impl Renderable for ModelBackendKind {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(backend_display(*self, ctx)),
            OutputFormat::Json => serde_json::to_string(self)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: backend_display(*self, ctx),
                    children: vec![],
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec!["backend".into(), "display".into()],
                    rows: vec![vec![format!("{:?}", self), backend_display(*self, ctx)]],
                    align: vec![TableAlign::Left, TableAlign::Left],
                };
                TableRenderer::new(ctx.clone()).render(&spec)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CircuitState
// ---------------------------------------------------------------------------

impl Renderable for CircuitState {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(circuit_display(*self, ctx)),
            OutputFormat::Json => serde_json::to_string(self)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: circuit_display(*self, ctx),
                    children: vec![],
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec!["state".into(), "display".into()],
                    rows: vec![vec![format!("{:?}", self), circuit_display(*self, ctx)]],
                    align: vec![TableAlign::Left, TableAlign::Left],
                };
                TableRenderer::new(ctx.clone()).render(&spec)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CognitiveModel
// ---------------------------------------------------------------------------

impl Renderable for CognitiveModel {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let r = TextRenderer::new(ctx.clone());
                let lines = vec![
                    r.render_kv("model_id", self.model_id.0.as_str()),
                    r.render_kv("provider", &format!("{:?}", self.provider)),
                    r.render_kv("capabilities", &self.capabilities.join(", ")),
                    r.render_kv("max_tokens", &self.max_tokens.to_string()),
                    r.render_kv("input_cost_per_1k", &format_cost(self.input_cost_per_1k)),
                    r.render_kv("output_cost_per_1k", &format_cost(self.output_cost_per_1k)),
                    r.render_kv(
                        "vault_capability_id",
                        self.vault_capability_id.as_deref().unwrap_or("(none)"),
                    ),
                    r.render_kv("created_at", &self.created_at.to_rfc3339()),
                ];
                Ok(r.render_section("CognitiveModel", &lines))
            }
            OutputFormat::Json => serde_json::to_string(self)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: format!("model {}", self.model_id.0),
                    children: vec![
                        TreeNode {
                            label: format!("provider: {:?}", self.provider),
                            children: vec![],
                        },
                        TreeNode {
                            label: format!("capabilities: {}", self.capabilities.join(", ")),
                            children: vec![],
                        },
                        TreeNode {
                            label: format!("max_tokens: {}", self.max_tokens),
                            children: vec![],
                        },
                        TreeNode {
                            label: format!(
                                "cost: {}/{}",
                                format_cost(self.input_cost_per_1k),
                                format_cost(self.output_cost_per_1k)
                            ),
                            children: vec![],
                        },
                    ],
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec![
                        "model_id".into(),
                        "provider".into(),
                        "max_tokens".into(),
                        "input_cost".into(),
                    ],
                    rows: vec![vec![
                        self.model_id.0.clone(),
                        format!("{:?}", self.provider),
                        self.max_tokens.to_string(),
                        format_cost(self.input_cost_per_1k),
                    ]],
                    align: vec![TableAlign::Left; 4],
                };
                TableRenderer::new(ctx.clone()).render(&spec)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// RoutingDecision
// ---------------------------------------------------------------------------

impl Renderable for RoutingDecision {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let r = TextRenderer::new(ctx.clone());
                let lines = vec![
                    r.render_kv("routing_id", &self.routing_id),
                    r.render_kv("chosen_backend", &backend_display(self.chosen_backend, ctx)),
                    r.render_kv("provider_class", &format!("{:?}", self.provider_class)),
                    r.render_kv("backend_id", &self.backend_id),
                    r.render_kv("matched_rule", &self.matched_rule.to_string()),
                    r.render_kv("degraded", &self.degraded.to_string()),
                    r.render_kv("reason", self.reason.as_deref().unwrap_or("(none)")),
                    r.render_kv("decided_at", &self.decided_at.to_rfc3339()),
                ];
                Ok(r.render_section("RoutingDecision", &lines))
            }
            OutputFormat::Json => serde_json::to_string(self)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: format!("route {}", self.routing_id),
                    children: vec![
                        TreeNode {
                            label: format!(
                                "backend: {}",
                                backend_display(self.chosen_backend, ctx)
                            ),
                            children: vec![],
                        },
                        TreeNode {
                            label: format!("provider: {:?}", self.provider_class),
                            children: vec![],
                        },
                        TreeNode {
                            label: format!("rule: #{}", self.matched_rule),
                            children: vec![],
                        },
                        TreeNode {
                            label: format!("degraded: {}", self.degraded),
                            children: vec![],
                        },
                    ],
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec![
                        "routing_id".into(),
                        "backend".into(),
                        "rule".into(),
                        "degraded".into(),
                    ],
                    rows: vec![vec![
                        self.routing_id.clone(),
                        format!("{:?}", self.chosen_backend),
                        format!("#{}", self.matched_rule),
                        self.degraded.to_string(),
                    ]],
                    align: vec![TableAlign::Left; 4],
                };
                TableRenderer::new(ctx.clone()).render(&spec)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CognitiveIntentCapabilityList — wrapper for CLI list output
// ---------------------------------------------------------------------------

/// Wrapper for rendering a list of cognitive intent capabilities.
#[derive(Debug, Clone, Serialize)]
pub struct CognitiveIntentCapabilityList {
    capabilities: Vec<IntentCapability>,
}

impl CognitiveIntentCapabilityList {
    /// New wrapper over a list of intent capabilities.
    #[must_use]
    pub const fn new(capabilities: Vec<IntentCapability>) -> Self {
        Self { capabilities }
    }

    /// Access the inner capability slice.
    #[must_use]
    pub fn capabilities(&self) -> &[IntentCapability] {
        &self.capabilities
    }
}

impl Renderable for CognitiveIntentCapabilityList {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let r = TextRenderer::new(ctx.clone());
                let mut lines = vec![r.render_kv("intents", &self.capabilities.len().to_string())];
                for cap in &self.capabilities {
                    lines.push(format!(
                        "{}  {:?}  {}  {} tokens",
                        cap.intent_kind,
                        cap.requires_latency_tier,
                        cap.produces_action_type,
                        cap.max_tokens_estimate,
                    ));
                }
                Ok(r.render_section("IntentCapabilities", &lines))
            }
            OutputFormat::Json => serde_json::to_string(self)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: format!("intent_capabilities count={}", self.capabilities.len()),
                    children: self
                        .capabilities
                        .iter()
                        .map(|cap| TreeNode {
                            label: cap.intent_kind.clone(),
                            children: vec![
                                TreeNode {
                                    label: format!("desc: {}", cap.description),
                                    children: vec![],
                                },
                                TreeNode {
                                    label: format!("latency: {:?}", cap.requires_latency_tier),
                                    children: vec![],
                                },
                                TreeNode {
                                    label: format!("produces: {}", cap.produces_action_type),
                                    children: vec![],
                                },
                                TreeNode {
                                    label: format!("max_tokens: {}", cap.max_tokens_estimate),
                                    children: vec![],
                                },
                            ],
                        })
                        .collect(),
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec![
                        "intent_kind".into(),
                        "latency".into(),
                        "action_type".into(),
                        "tokens".into(),
                    ],
                    rows: self
                        .capabilities
                        .iter()
                        .map(|cap| {
                            vec![
                                cap.intent_kind.clone(),
                                format!("{:?}", cap.requires_latency_tier),
                                cap.produces_action_type.clone(),
                                cap.max_tokens_estimate.to_string(),
                            ]
                        })
                        .collect(),
                    align: vec![TableAlign::Left; 4],
                };
                TableRenderer::new(ctx.clone()).render(&spec)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CognitiveModelList — wrapper for CLI list output
// ---------------------------------------------------------------------------

/// Wrapper for rendering a list of registered cognitive models.
#[derive(Debug, Clone, Serialize)]
pub struct CognitiveModelList {
    models: Vec<CognitiveModel>,
}

impl CognitiveModelList {
    /// New wrapper over a list of cognitive models.
    #[must_use]
    pub const fn new(models: Vec<CognitiveModel>) -> Self {
        Self { models }
    }
}

impl Renderable for CognitiveModelList {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let r = TextRenderer::new(ctx.clone());
                let mut lines = vec![r.render_kv("models", &self.models.len().to_string())];
                for model in &self.models {
                    lines.push(format!(
                        "{}  {:?}  {} tokens  {}/{}",
                        model.model_id.0,
                        model.provider,
                        model.max_tokens,
                        format_cost(model.input_cost_per_1k),
                        format_cost(model.output_cost_per_1k),
                    ));
                }
                Ok(r.render_section("CognitiveModels", &lines))
            }
            OutputFormat::Json => serde_json::to_string(self)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: format!("cognitive_models count={}", self.models.len()),
                    children: self
                        .models
                        .iter()
                        .map(|model| TreeNode {
                            label: model.model_id.0.clone(),
                            children: vec![
                                TreeNode {
                                    label: format!("provider: {:?}", model.provider),
                                    children: vec![],
                                },
                                TreeNode {
                                    label: format!("max_tokens: {}", model.max_tokens),
                                    children: vec![],
                                },
                            ],
                        })
                        .collect(),
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec![
                        "model_id".into(),
                        "provider".into(),
                        "max_tokens".into(),
                        "cost_in".into(),
                    ],
                    rows: self
                        .models
                        .iter()
                        .map(|model| {
                            vec![
                                model.model_id.0.clone(),
                                format!("{:?}", model.provider),
                                model.max_tokens.to_string(),
                                format_cost(model.input_cost_per_1k),
                            ]
                        })
                        .collect(),
                    align: vec![TableAlign::Left; 4],
                };
                TableRenderer::new(ctx.clone()).render(&spec)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CircuitStateList — wrapper for CLI list output (all backends)
// ---------------------------------------------------------------------------

/// Wrapper for rendering circuit breaker states across all backends.
#[derive(Debug, Clone, Serialize)]
pub struct CircuitStateList {
    entries: Vec<(ModelBackendKind, CircuitState)>,
}

impl CircuitStateList {
    /// New wrapper over a list of (backend kind, circuit state) pairs.
    #[must_use]
    pub const fn new(entries: Vec<(ModelBackendKind, CircuitState)>) -> Self {
        Self { entries }
    }
}

impl Renderable for CircuitStateList {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let r = TextRenderer::new(ctx.clone());
                let mut lines = Vec::new();
                for (kind, state) in &self.entries {
                    lines.push(format!(
                        "{}  {}",
                        backend_display(*kind, ctx),
                        circuit_display(*state, ctx),
                    ));
                }
                Ok(r.render_section("CircuitStates", &lines))
            }
            OutputFormat::Json => serde_json::to_string(self)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: "circuit_states".into(),
                    children: self
                        .entries
                        .iter()
                        .map(|(kind, state)| TreeNode {
                            label: format!("{kind:?}"),
                            children: vec![TreeNode {
                                label: format!("state: {}", circuit_display(*state, ctx)),
                                children: vec![],
                            }],
                        })
                        .collect(),
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec!["backend".into(), "state".into()],
                    rows: self
                        .entries
                        .iter()
                        .map(|(kind, state)| vec![format!("{:?}", kind), format!("{:?}", state)])
                        .collect(),
                    align: vec![TableAlign::Left; 2],
                };
                TableRenderer::new(ctx.clone()).render(&spec)
            }
        }
    }
}
