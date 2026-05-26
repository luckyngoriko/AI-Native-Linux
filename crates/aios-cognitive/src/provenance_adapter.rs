//! INV-002 cross-crate provenance adapter.
//!
//! `CognitiveProvenanceAdapter` implements [`aios_capability_runtime::RuntimeCognitiveProvenance`]
//! so the Capability Runtime can verify that AI envelopes were produced by the
//! Cognitive Core without depending on `aios-cognitive`.

use async_trait::async_trait;

use aios_action::ActionEnvelope;
use aios_capability_runtime::RuntimeCognitiveProvenance;

/// Expected provenance marker key in `request.target`.
pub const PROVENANCE_MARKER_KEY: &str = "cognitive_provenance";

/// Adapter that bridges `aios-cognitive` ↔ `aios-capability-runtime` for INV-002.
///
/// Holds the expected provenance marker value (the translator version string).
/// During `verify_provenance`, it checks that `request.target` carries a
/// `cognitive_provenance` key whose value matches the expected marker.
#[derive(Debug, Clone)]
pub struct CognitiveProvenanceAdapter {
    expected_marker: String,
}

impl CognitiveProvenanceAdapter {
    /// Create an adapter that expects the given provenance marker value.
    #[must_use]
    pub fn new(expected_marker: impl Into<String>) -> Self {
        Self {
            expected_marker: expected_marker.into(),
        }
    }

    /// The marker value this adapter expects.
    #[must_use]
    pub fn expected_marker(&self) -> &str {
        &self.expected_marker
    }
}

#[async_trait]
impl RuntimeCognitiveProvenance for CognitiveProvenanceAdapter {
    async fn verify_provenance(&self, envelope: &ActionEnvelope) -> Result<(), String> {
        let marker = envelope
            .request
            .target
            .get(PROVENANCE_MARKER_KEY)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "cognitive_provenance marker missing from request.target".to_string())?;

        if marker != self.expected_marker {
            return Err(format!(
                "provenance marker mismatch: expected '{}', got '{}'",
                self.expected_marker, marker
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use aios_action::{Identity, Request, Trace};

    fn make_envelope(is_ai: bool, target: serde_json::Value) -> ActionEnvelope {
        ActionEnvelope::new(
            Identity::new("test_subject", is_ai),
            Request::new("cognitive.translate", target),
            Trace::new("00000000000000000000000000000000", "0000000000000000", None),
        )
    }

    #[tokio::test]
    async fn provenance_marker_present_and_matches() {
        let adapter = CognitiveProvenanceAdapter::new("0.1.0-T098");
        let envelope = make_envelope(
            true,
            serde_json::json!({"cognitive_provenance": "0.1.0-T098"}),
        );
        assert!(adapter.verify_provenance(&envelope).await.is_ok());
    }

    #[tokio::test]
    async fn provenance_marker_missing() {
        let adapter = CognitiveProvenanceAdapter::new("0.1.0-T098");
        let envelope = make_envelope(true, serde_json::json!({}));
        let err = adapter.verify_provenance(&envelope).await.unwrap_err();
        assert!(err.contains("missing"));
    }

    #[tokio::test]
    async fn provenance_marker_mismatch() {
        let adapter = CognitiveProvenanceAdapter::new("0.1.0-T098");
        let envelope = make_envelope(
            true,
            serde_json::json!({"cognitive_provenance": "wrong_version"}),
        );
        let err = adapter.verify_provenance(&envelope).await.unwrap_err();
        assert!(err.contains("mismatch"));
    }

    #[tokio::test]
    async fn provenance_marker_empty_string() {
        let adapter = CognitiveProvenanceAdapter::new("0.1.0-T098");
        let envelope = make_envelope(true, serde_json::json!({"cognitive_provenance": ""}));
        // as_str() returns Some("") for empty strings, which won't match
        let err = adapter.verify_provenance(&envelope).await.unwrap_err();
        assert!(err.contains("mismatch"));
    }
}
