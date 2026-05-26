//! T-112 — `SandboxCognitiveHint` for cognitive↔sandbox profile suggestions.
//!
//! The cognitive layer can emit a `SandboxCognitiveHint` alongside a
//! [`CognitiveIntent`] to express profile preferences (e.g. "this model run
//! needs a GPU compute profile"). The sandbox composer reads these hints
//! during the merge step when the `user_request` source is populated from the
//! cognitive provenance adapter.

use serde::{Deserialize, Serialize};

/// Optional hint from the cognitive layer to the sandbox composer.
///
/// These hints are advisory — the composer uses them to seed the
/// `user_request` source but may tighten (never loosen) the resulting
/// profile through the most-restrictive-wins merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxCognitiveHint {
    /// Suggested isolation level.
    pub suggested_isolation: Option<String>,
    /// Suggested network posture.
    pub suggested_network: Option<String>,
    /// Suggested GPU capability class.
    pub suggested_gpu_class: Option<String>,
    /// Whether the intent requires network access.
    pub requires_network: bool,
    /// Whether the intent requires GPU compute.
    pub requires_gpu: bool,
    /// Whether the intent requires filesystem access.
    pub requires_filesystem: bool,
    /// Free-form rationale from the cognitive layer.
    pub rationale: Option<String>,
}

impl SandboxCognitiveHint {
    /// Build a hint from a cognitive intent's natural-language content and
    /// translated action target.
    ///
    /// This is a heuristic builder — the hint is advisory and does not
    /// replace the policy kernel or the runtime safety floor.
    #[must_use]
    pub const fn build_hint_from_intent(
        natural_language: &str,
        action_target: &str,
        requires_network: bool,
        requires_gpu: bool,
        requires_filesystem: bool,
    ) -> Self {
        let _ = natural_language;
        let _ = action_target;
        Self {
            suggested_isolation: None,
            suggested_network: None,
            suggested_gpu_class: None,
            requires_network,
            requires_gpu,
            requires_filesystem,
            rationale: None,
        }
    }

    /// Set a human-readable rationale for this hint.
    #[must_use]
    pub fn with_rationale(mut self, rationale: impl Into<String>) -> Self {
        self.rationale = Some(rationale.into());
        self
    }

    /// Set the suggested isolation kind.
    #[must_use]
    pub fn with_isolation(mut self, isolation: impl Into<String>) -> Self {
        self.suggested_isolation = Some(isolation.into());
        self
    }

    /// Set the suggested network posture.
    #[must_use]
    pub fn with_network(mut self, network: impl Into<String>) -> Self {
        self.suggested_network = Some(network.into());
        self
    }

    /// Set the suggested GPU capability class.
    #[must_use]
    pub fn with_gpu_class(mut self, gpu_class: impl Into<String>) -> Self {
        self.suggested_gpu_class = Some(gpu_class.into());
        self
    }
}
