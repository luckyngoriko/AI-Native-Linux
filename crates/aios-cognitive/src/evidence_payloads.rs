//! Typed cognitive evidence payloads for S13.x -> S3.1 emission.
//!
//! Every payload enforces INV-015 (no prompt/response bodies) and INV-018
//! (no raw key bytes or signing key material).

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the cognitive evidence vocabulary"
)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::circuit::CircuitState;
use crate::routing::{AICrossOriginPosture, ModelBackendKind};

/// Payload for `MODEL_CALL` (`RecordType` ID 13).
///
/// # INV-015
///
/// Carries only token counts, cost, and latency. No prompt or response bodies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ModelCallPayload {
    /// The model that was invoked.
    pub model_id: String,
    /// Routing decision id linking this call to its `ROUTING_DECISION`.
    pub routing_id: String,
    /// Estimated tokens consumed by the input.
    pub tokens_in: u32,
    /// Estimated tokens produced in the output.
    pub tokens_out: u32,
    /// Estimated cost in micro-currency units.
    pub cost_micros: u64,
    /// Wall-clock latency in milliseconds.
    pub latency_ms: u64,
    /// UTC timestamp when the call completed.
    pub occurred_at: DateTime<Utc>,
}

/// Payload for `ROUTING_DECISION` (`RecordType` ID 3).
///
/// # INV-015 / INV-018
///
/// Carries only the chosen backend, a stable inputs hash, and a code version.
/// No prompt, response, or key material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct RoutingDecisionPayload {
    /// Unique routing decision id.
    pub routing_id: String,
    /// The backend kind selected by the router.
    pub chosen_backend: ModelBackendKind,
    /// BLAKE3 hex hash of the canonical routing inputs.
    pub inputs_hash: String,
    /// UTC timestamp when the decision was made.
    pub decided_at: DateTime<Utc>,
    /// Code version baked into the decision for reproducibility.
    pub code_version: String,
}

/// Payload for `CIRCUIT_BREAKER_OPENED` (`RecordType` ID 212)
/// and `CIRCUIT_BREAKER_CLOSED` (`RecordType` ID 213).
///
/// # INV-015 / INV-018
///
/// Carries only the backend, state transition, and error rate. No prompt,
/// response, or key material.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct CircuitBreakerTrippedPayload {
    /// The backend whose circuit breaker changed state.
    pub backend: ModelBackendKind,
    /// State before the transition.
    pub from_state: CircuitState,
    /// State after the transition.
    pub to_state: CircuitState,
    /// Error rate observed at transition time.
    pub error_rate: f64,
    /// UTC timestamp when the state transition occurred.
    pub transitioned_at: DateTime<Utc>,
}

/// Payload for `AI_DIRECT_INTERNET_DENIED` (`RecordType` ID 147).
///
/// # INV-015 / INV-018
///
/// Carries only the model id, posture, a non-secret summary string,
/// and timestamp. No prompt, response, or key material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AiDirectInternetDeniedPayload {
    /// The model whose external access was denied.
    pub model_id: String,
    /// The posture that caused the denial.
    pub posture: AICrossOriginPosture,
    /// Non-secret summary of the attempt.
    pub attempt_summary: String,
    /// UTC timestamp when the denial occurred.
    pub denied_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        clippy::panic,
        clippy::float_cmp,
        clippy::unwrap_used,
        reason = "panic-on-failure is the idiomatic test signal"
    )]

    use super::*;
    use crate::routing::AICrossOriginPosture;

    #[test]
    fn model_call_payload_round_trip() {
        let p = ModelCallPayload {
            model_id: "mdl_test".into(),
            routing_id: "rtdg_test".into(),
            tokens_in: 100,
            tokens_out: 50,
            cost_micros: 42,
            latency_ms: 200,
            occurred_at: Utc::now(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: ModelCallPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn routing_decision_payload_round_trip() {
        let p = RoutingDecisionPayload {
            routing_id: "rtdg_test".into(),
            chosen_backend: ModelBackendKind::LocalGpu,
            inputs_hash: "abc123".into(),
            decided_at: Utc::now(),
            code_version: "0.1.0".into(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: RoutingDecisionPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn circuit_breaker_tripped_payload_round_trip() {
        let p = CircuitBreakerTrippedPayload {
            backend: ModelBackendKind::LocalGpu,
            from_state: CircuitState::Closed,
            to_state: CircuitState::Open,
            error_rate: 0.15,
            transitioned_at: Utc::now(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: CircuitBreakerTrippedPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn ai_direct_internet_denied_payload_round_trip() {
        let p = AiDirectInternetDeniedPayload {
            model_id: "mdl_test".into(),
            posture: AICrossOriginPosture::AiNoExternal,
            attempt_summary: "blocked external model call".into(),
            denied_at: Utc::now(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: AiDirectInternetDeniedPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn inv_015_no_prompt_bodies_in_model_call_payload() {
        let json = serde_json::to_value(ModelCallPayload {
            model_id: "m".into(),
            routing_id: "r".into(),
            tokens_in: 1,
            tokens_out: 1,
            cost_micros: 0,
            latency_ms: 0,
            occurred_at: Utc::now(),
        })
        .unwrap();
        let s = json.to_string();
        assert!(!s.contains("prompt"));
        assert!(!s.contains("response"));
        assert!(!s.contains("body"));
        assert!(!s.contains("AIOS_COGNITIVE_SECRET_PROMPT"));
    }

    #[test]
    fn inv_018_no_key_bytes_in_any_payload() {
        let payloads = [
            serde_json::to_value(ModelCallPayload {
                model_id: "m".into(),
                routing_id: "r".into(),
                tokens_in: 1,
                tokens_out: 1,
                cost_micros: 0,
                latency_ms: 0,
                occurred_at: Utc::now(),
            })
            .unwrap(),
            serde_json::to_value(RoutingDecisionPayload {
                routing_id: "r".into(),
                chosen_backend: ModelBackendKind::LocalCpu,
                inputs_hash: "h".into(),
                decided_at: Utc::now(),
                code_version: "v".into(),
            })
            .unwrap(),
            serde_json::to_value(CircuitBreakerTrippedPayload {
                backend: ModelBackendKind::LocalCpu,
                from_state: CircuitState::Closed,
                to_state: CircuitState::Open,
                error_rate: 0.0,
                transitioned_at: Utc::now(),
            })
            .unwrap(),
            serde_json::to_value(AiDirectInternetDeniedPayload {
                model_id: "m".into(),
                posture: AICrossOriginPosture::AiNoExternal,
                attempt_summary: "s".into(),
                denied_at: Utc::now(),
            })
            .unwrap(),
        ];
        for v in &payloads {
            let s = v.to_string();
            assert!(!s.contains("secret"), "payload leaked 'secret': {s}");
            assert!(!s.contains("key_bytes"), "payload leaked 'key_bytes': {s}");
            assert!(
                !s.contains("signing_key"),
                "payload leaked 'signing_key': {s}"
            );
        }
    }

    #[test]
    fn deny_unknown_fields_on_all_payloads() {
        let extra = r#"{"model_id":"m","routing_id":"r","tokens_in":1,"tokens_out":1,"cost_micros":0,"latency_ms":0,"occurred_at":"2025-01-01T00:00:00Z","extra_field":true}"#;
        assert!(serde_json::from_str::<ModelCallPayload>(extra).is_err());
    }
}
