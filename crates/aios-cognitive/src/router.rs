//! Deterministic precedence-table model router per S13.2 §7.
//!
//! # Determinism contract (S13.2 §C4)
//!
//! Given identical `(LatencyClass, PrivacyClass, AICrossOriginPosture, BackendHealthState,
//! recovery_mode, budget_state, code_version)` the router returns the same `ModelBackendKind`.
//! No randomness in selection. Per-call retry choices are deterministic too — next backend
//! in the precedence list, never a random pick.

use std::sync::Arc;

use chrono::Utc;

use crate::error::CognitiveError;
use crate::evidence_emit::CognitiveEvidenceEmitter;
use crate::latency::{LatencyTier, PrivacyClass};
use crate::routing::{
    AICrossOriginPosture, BackendHealthState, ModelBackendKind, ProviderClass, RoutingDecision,
    RoutingInputs,
};

/// A single row in the deterministic routing precedence table (S13.2 §7).
///
/// Fields use `Option<T>` for wildcard matching — `None` matches any value.
#[derive(Debug, Clone)]
pub struct RoutingRule {
    /// Precedence priority (lower = higher priority). Rules evaluated top-to-bottom;
    /// first match wins.
    pub priority: u32,
    /// The backend kind this rule produces when matched.
    pub output_backend: ModelBackendKind,
    /// Matched rule id per S13.2 §7.1 (1–13).
    pub rule_id: u32,
    /// Whether this rule produces a degraded result.
    pub degraded: bool,
    /// Reason code when the rule matches (carried in `RoutingDecision.reason`).
    pub reason: Option<&'static str>,

    /// `None` = wildcard (matches any `LatencyTier`).
    pub latency_class: Option<LatencyTier>,
    /// `None` = wildcard (matches any `PrivacyClass`).
    pub privacy_class: Option<PrivacyClass>,
    /// `None` = wildcard (matches any `AICrossOriginPosture`).
    pub ai_cross_origin_posture: Option<AICrossOriginPosture>,
    /// `None` = wildcard (matches any recovery state).
    pub recovery_mode: Option<bool>,
    /// `None` = wildcard (matches any budget state).
    pub budget_ok: Option<bool>,
    /// If set, this backend kind must be healthy for the rule to match.
    pub require_backend_healthy: Option<ModelBackendKind>,
}

/// Deterministic precedence-table model router per S13.2 §7.
///
/// # Determinism contract (S13.2 §C4)
///
/// Given identical inputs and `code_version`, the router returns the same
/// `ModelBackendKind` decision. No randomness in selection — the precedence
/// table is evaluated top-to-bottom and the first matching rule wins.
pub struct ModelRouter {
    /// Canonical precedence table ordered by priority (rule 1 first).
    precedence_table: Arc<Vec<RoutingRule>>,
    /// Code version baked into every routing decision for reproducibility.
    code_version: String,
    /// Optional evidence emitter for `ROUTING_DECISION` receipts (T-102).
    evidence_emitter: Option<Arc<CognitiveEvidenceEmitter>>,
}

impl ModelRouter {
    /// Create a router pre-loaded with the canonical S13.2 §7 precedence table.
    #[must_use]
    pub fn new_with_defaults() -> Self {
        use AICrossOriginPosture::AiVaultBrokeredOnly;
        use LatencyTier::{T0CachedUiState, T4PowerfulReasoning};
        use ModelBackendKind::{Cached, DegradedNull, ExternalVaultBrokered, LocalGpu};
        use PrivacyClass::SecretBearing;

        let table = vec![
            // Rule 1 — Recovery forbids T3/T4 (C8)
            RoutingRule {
                priority: 1,
                output_backend: DegradedNull, // FORBIDDEN stand-in — see S13.2 §4.7
                rule_id: 1,
                degraded: false,
                reason: Some("recovery_mode"),
                latency_class: None,
                privacy_class: None,
                ai_cross_origin_posture: None,
                recovery_mode: Some(true),
                budget_ok: None,
                require_backend_healthy: None,
            },
            // Rule 2 — T0 cache hit (bookkeeping only; S1.2 cache-hit path)
            RoutingRule {
                priority: 2,
                output_backend: Cached,
                rule_id: 2,
                degraded: false,
                reason: None,
                latency_class: Some(T0CachedUiState),
                privacy_class: None,
                ai_cross_origin_posture: None,
                recovery_mode: None,
                budget_ok: None,
                require_backend_healthy: None,
            },
            // Rule 5 — SECRET_BEARING forbids external (precedes rule 8/10)
            RoutingRule {
                priority: 5,
                output_backend: LocalGpu,
                rule_id: 5,
                degraded: false,
                reason: Some("secret_bearing_local_only"),
                latency_class: None,
                privacy_class: Some(SecretBearing),
                ai_cross_origin_posture: None,
                recovery_mode: None,
                budget_ok: None,
                require_backend_healthy: None,
            },
            // Rule 10 — T4 + vault-brokered posture + budget ok → external
            RoutingRule {
                priority: 10,
                output_backend: ExternalVaultBrokered,
                rule_id: 10,
                degraded: false,
                reason: None,
                latency_class: Some(T4PowerfulReasoning),
                privacy_class: None,
                ai_cross_origin_posture: Some(AiVaultBrokeredOnly),
                recovery_mode: None,
                budget_ok: Some(true),
                require_backend_healthy: None,
            },
        ];

        Self {
            precedence_table: Arc::new(table),
            code_version: crate::DEFAULT_CODE_VERSION.to_string(),
            evidence_emitter: None,
        }
    }

    /// Create a router with a custom precedence table (for testing).
    #[must_use]
    pub fn with_table(table: Vec<RoutingRule>) -> Self {
        Self {
            precedence_table: Arc::new(table),
            code_version: crate::DEFAULT_CODE_VERSION.to_string(),
            evidence_emitter: None,
        }
    }

    /// Attach an evidence emitter for `ROUTING_DECISION` receipts (T-102).
    #[must_use]
    pub fn with_evidence_emitter(mut self, emitter: Arc<CognitiveEvidenceEmitter>) -> Self {
        self.evidence_emitter = Some(emitter);
        self
    }

    /// Return a shared reference to the precedence table.
    #[must_use]
    pub const fn precedence_table(&self) -> &Arc<Vec<RoutingRule>> {
        &self.precedence_table
    }

    /// Return the router's code version string.
    #[must_use]
    pub fn code_version(&self) -> &str {
        &self.code_version
    }

    /// Route a request to a backend per the deterministic precedence table (S13.2 §7).
    ///
    /// Rules are evaluated top-to-bottom; the first matching rule wins (S13.2 §C4).
    /// When a rule's `require_backend_healthy` field is set, the named backend kind
    /// must be present and healthy in `inputs.backend_health_snapshot`.
    ///
    /// # Errors
    ///
    /// Returns `CognitiveError::NoRouteAvailable` only when the routing table itself
    /// is empty (programmer error). Under normal operation rule 13 (the catch-all)
    /// produces a `DegradedNull` decision.
    #[allow(clippy::too_many_lines)]
    pub fn route(&self, inputs: &RoutingInputs) -> Result<RoutingDecision, CognitiveError> {
        use AICrossOriginPosture::{AiLoopbackOnly, AiNoExternal, AiVaultBrokeredOnly};
        use BackendHealthState::{Healthy, Suspended, Unhealthy};
        use LatencyTier::{
            T0CachedUiState, T1Deterministic, T2CatalogRetrieval, T3LocalCognitive,
            T4PowerfulReasoning,
        };
        use ModelBackendKind::{
            Cached, DegradedNull, ExternalVaultBrokered, FallbackRuleBased, LocalCpu,
            LocalDistributed, LocalGpu,
        };

        let in_t3_t4 = matches!(inputs.latency_class, T3LocalCognitive | T4PowerfulReasoning);

        // Deterministic inputs hash for ROUTING_DECISION evidence (S13.2 §C4)
        let inputs_hash = {
            let json = serde_json::to_string(inputs).unwrap_or_default();
            let hash = blake3::hash(json.as_bytes());
            hash.to_hex().to_string()
        };

        // Helper: is a given backend kind healthy in the snapshot?
        let is_healthy = |kind: ModelBackendKind| -> bool {
            inputs
                .backend_health_snapshot
                .iter()
                .any(|e| e.backend_kind == kind && e.state == Healthy)
        };

        // Helper: select the best available local backend.
        // Precedence: LocalGpu → LocalCpu → LocalDistributed (per spec tie-break: GPU first).
        let best_local = || -> Option<ModelBackendKind> {
            if is_healthy(LocalGpu) {
                Some(LocalGpu)
            } else if is_healthy(LocalCpu) {
                Some(LocalCpu)
            } else if is_healthy(LocalDistributed) {
                Some(LocalDistributed)
            } else {
                None
            }
        };

        // Helper: any preferred backend unhealthy or suspended?
        let all_preferred_unhealthy = || -> bool {
            [LocalGpu, LocalCpu, LocalDistributed, ExternalVaultBrokered]
                .iter()
                .all(|k| {
                    inputs
                        .backend_health_snapshot
                        .iter()
                        .any(|e| e.backend_kind == *k && matches!(e.state, Unhealthy | Suspended))
                })
        };

        // ── Rule 1: recovery_mode forbids T3/T4 (C8) ──
        if inputs.recovery_mode && in_t3_t4 {
            return self.build_decision(
                &inputs_hash,
                DegradedNull, // FORBIDDEN stand-in
                ProviderClass::Anthropic,
                "forbidden-backend",
                1,
                false,
                Some("recovery_mode"),
            );
        }

        // ── Rule 2: T0 cache hit ──
        if inputs.latency_class == T0CachedUiState {
            return self.build_decision(
                &inputs_hash,
                Cached,
                ProviderClass::Anthropic,
                "cached-backend",
                2,
                false,
                None,
            );
        }

        // ── Rule 3: T1 — router not invoked (return null for completeness) ──
        if inputs.latency_class == T1Deterministic {
            return self.build_decision(
                &inputs_hash,
                DegradedNull,
                ProviderClass::Anthropic,
                "t1-null",
                3,
                false,
                Some("t1_not_routed"),
            );
        }

        // ── Rule 4: T2 → fallback rule-based ──
        if inputs.latency_class == T2CatalogRetrieval {
            return self.build_decision(
                &inputs_hash,
                FallbackRuleBased,
                ProviderClass::Ollama,
                "rule-based-backend",
                4,
                false,
                None,
            );
        }

        // ── Rule 5: SECRET_BEARING + T3/T4 → local only ──
        if inputs.privacy_class == PrivacyClass::SecretBearing && in_t3_t4 {
            if let Some(backend) = best_local() {
                return self.build_decision(
                    &inputs_hash,
                    backend,
                    ProviderClass::Ollama,
                    "local-backend",
                    5,
                    false,
                    Some("secret_bearing_local_only"),
                );
            }
            // No local backend healthy → fallthrough to degraded
            return self.build_decision(
                &inputs_hash,
                DegradedNull,
                ProviderClass::Ollama,
                "no-local-backend",
                5,
                true,
                Some("secret_bearing_no_local"),
            );
        }

        // ── Rule 6: AI_NO_EXTERNAL + T3/T4 → local only ──
        if inputs.ai_cross_origin_posture == AiNoExternal && in_t3_t4 {
            if let Some(backend) = best_local() {
                return self.build_decision(
                    &inputs_hash,
                    backend,
                    ProviderClass::Ollama,
                    "local-backend",
                    6,
                    false,
                    Some("ai_no_external_local_only"),
                );
            }
            return self.build_decision(
                &inputs_hash,
                DegradedNull,
                ProviderClass::Ollama,
                "no-local-backend",
                6,
                true,
                Some("ai_no_external_no_local"),
            );
        }

        // ── Rule 7: AI_LOOPBACK_ONLY + T3/T4 → LOCAL_CPU or LOCAL_GPU only (no LAN) ──
        if inputs.ai_cross_origin_posture == AiLoopbackOnly && in_t3_t4 {
            if is_healthy(LocalGpu) {
                return self.build_decision(
                    &inputs_hash,
                    LocalGpu,
                    ProviderClass::Ollama,
                    "local-gpu-backend",
                    7,
                    false,
                    Some("ai_loopback_only"),
                );
            }
            if is_healthy(LocalCpu) {
                return self.build_decision(
                    &inputs_hash,
                    LocalCpu,
                    ProviderClass::Ollama,
                    "local-cpu-backend",
                    7,
                    false,
                    Some("ai_loopback_only"),
                );
            }
            // No loopback backends healthy
            return self.build_decision(
                &inputs_hash,
                DegradedNull,
                ProviderClass::Ollama,
                "no-loopback-backend",
                7,
                true,
                Some("ai_loopback_only_no_lan"),
            );
        }

        // ── Rule 8: T3 + LOCAL_GPU healthy → LOCAL_GPU ──
        if inputs.latency_class == T3LocalCognitive && is_healthy(LocalGpu) {
            return self.build_decision(
                &inputs_hash,
                LocalGpu,
                ProviderClass::Ollama,
                "local-gpu-backend",
                8,
                false,
                None,
            );
        }

        // ── Rule 9: T3 + LOCAL_GPU not healthy + LOCAL_CPU healthy → LOCAL_CPU ──
        if inputs.latency_class == T3LocalCognitive && !is_healthy(LocalGpu) && is_healthy(LocalCpu)
        {
            return self.build_decision(
                &inputs_hash,
                LocalCpu,
                ProviderClass::Ollama,
                "local-cpu-backend",
                9,
                false,
                None,
            );
        }

        // ── Rule 10: T4 + vault-brokered + budget OK → EXTERNAL_VAULT_BROKERED ──
        if inputs.latency_class == T4PowerfulReasoning
            && inputs.ai_cross_origin_posture == AiVaultBrokeredOnly
            && inputs.budget_ok
        {
            return self.build_decision(
                &inputs_hash,
                ExternalVaultBrokered,
                ProviderClass::Anthropic,
                "external-vault-backend",
                10,
                false,
                None,
            );
        }

        // ── Rule 11: T4 + vault-brokered + budget exhausted + LOCAL_GPU healthy → LOCAL_GPU (degraded) ──
        if inputs.latency_class == T4PowerfulReasoning
            && inputs.ai_cross_origin_posture == AiVaultBrokeredOnly
            && !inputs.budget_ok
            && is_healthy(LocalGpu)
        {
            return self.build_decision(
                &inputs_hash,
                LocalGpu,
                ProviderClass::Ollama,
                "local-gpu-backend",
                11,
                true,
                Some("budget_exhausted_local_fallback"),
            );
        }

        // ── Rule 12: T3/T4 all preferred unhealthy → FALLBACK_RULE_BASED ──
        if in_t3_t4 && all_preferred_unhealthy() {
            return self.build_decision(
                &inputs_hash,
                FallbackRuleBased,
                ProviderClass::Ollama,
                "rule-based-backend",
                12,
                true,
                Some("all_backends_unhealthy"),
            );
        }

        // ── Rule 13: catch-all → DEGRADED_NULL ──
        self.build_decision(
            &inputs_hash,
            DegradedNull,
            ProviderClass::Anthropic,
            "null-backend",
            13,
            true,
            Some("degraded_null_fallback"),
        )
    }

    /// Build a `RoutingDecision` with the current timestamp and context.
    #[allow(
        clippy::too_many_arguments,
        clippy::unused_self,
        clippy::unnecessary_wraps
    )]
    fn build_decision(
        &self,
        inputs_hash: &str,
        chosen_backend: ModelBackendKind,
        provider_class: ProviderClass,
        backend_id: &str,
        matched_rule: u32,
        degraded: bool,
        reason: Option<&str>,
    ) -> Result<RoutingDecision, CognitiveError> {
        let decision = RoutingDecision {
            routing_id: {
                let id = ulid::Ulid::new();
                format!("rtdg_{id}")
            },
            chosen_backend,
            provider_class,
            backend_id: backend_id.to_string(),
            matched_rule,
            degraded,
            reason: reason.map(str::to_string),
            decided_at: Utc::now(),
        };

        // Best-effort ROUTING_DECISION evidence emission (fire-and-forget)
        if let Some(ref emitter) = self.evidence_emitter {
            let emitter = Arc::clone(emitter);
            let routing_id = decision.routing_id.clone();
            let chosen_backend = decision.chosen_backend;
            let inputs_hash = inputs_hash.to_string();
            let code_version = self.code_version.clone();
            tokio::spawn(async move {
                let _ = emitter
                    .emit_routing_decision(&routing_id, chosen_backend, &inputs_hash, &code_version)
                    .await;
            });
        }

        Ok(decision)
    }
}
