//! Caller's request payload — "what does the caller want done" (S0.1 §4).
//!
//! Immutable after `SubmitAction` returns: approval is bound to `hash(request)` and any
//! mutation invalidates the binding (S0.1 §2.2 invariant 1). The full S0.1 §4 message
//! (environment / risk / verification / sandbox profile id) lives behind richer types in
//! later tasks; T-001 ships the lifecycle-critical subset.

use serde::{Deserialize, Serialize};

use crate::canonical::{blake3_hash, jcs_canonicalize, CanonicalError};
use crate::id::ActionId;

/// Dry-run mode — closed enum, `Live` is the default per S0.1 §6 / §4.10.
///
/// Serialised in `SCREAMING_SNAKE_CASE` to match the proto enum names in S0.1 §4.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DryRunMode {
    /// Full lifecycle — real policy, real execution, real side effects. **Default.**
    Live,
    /// Schema + idempotency check only; no policy, no execution, no evidence by default.
    Validate,
    /// Full path with sandboxed simulated execution; no committed production side effects.
    Simulate,
}

impl Default for DryRunMode {
    /// `Live` per S0.1 §6 (`LIVE if unset`).
    fn default() -> Self {
        Self::Live
    }
}

/// "What the caller wants done." Immutable after submission.
///
/// `target` is intentionally a free-form `serde_json::Value` at this layer: type safety is
/// restored at the adapter level (each adapter manifest declares a JSON Schema, and the
/// Capability Runtime validates against it before execution — S0.1 §4.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// Dotted action name `<domain>.<verb>` per S0.1 §4.2 (e.g. `service.restart`).
    pub action: String,

    /// Typed payload; schema is owned by the adapter manifest (L3 `04_adapter_model.md`).
    pub target: serde_json::Value,

    /// Caller-supplied deduplication key per S0.1 §3.3; `None` opts out of dedup.
    pub idempotency_key: Option<String>,

    /// Single-parent causal link (S0.1 §3.4); saga / multi-parent is deferred.
    pub parent_action_id: Option<ActionId>,

    /// Dry-run mode; defaults to [`DryRunMode::Live`].
    #[serde(default)]
    pub dry_run: DryRunMode,
}

impl Request {
    /// Convenience constructor: build a `Live` request with no parent and no idempotency key.
    #[must_use]
    pub fn new(action: impl Into<String>, target: serde_json::Value) -> Self {
        Self {
            action: action.into(),
            target,
            idempotency_key: None,
            parent_action_id: None,
            dry_run: DryRunMode::Live,
        }
    }

    /// Compute `hash(request)` per S0.1 §3.3 / §8.5 — `BLAKE3(JCS(self))`.
    ///
    /// Returns a 64-character lowercase hex BLAKE3-256 digest. The Capability Runtime
    /// uses this value to detect [`Request`] drift across retries that reuse the same
    /// `idempotency_key` (S0.1 §3.3 rule 2: same key + different `hash(request)` →
    /// `IdempotencyConflict`).
    ///
    /// # Errors
    ///
    /// Propagates [`CanonicalError`] from the underlying JCS canonicalizer. In practice
    /// this only fails if a custom `Serialize` impl errors; the fields on this type
    /// never trigger it.
    pub fn request_hash(&self) -> Result<String, CanonicalError> {
        let canonical = jcs_canonicalize(self)?;
        Ok(blake3_hash(canonical.as_bytes()))
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::{DryRunMode, Request};

    #[test]
    fn dry_run_default_is_live_per_s01_section_6() {
        // S0.1 §6 / §4.10: "LIVE if unset". This is a constitutional default.
        assert_eq!(DryRunMode::default(), DryRunMode::Live);
    }

    #[test]
    fn request_hash_is_deterministic_across_logically_equal_construction() {
        // Build two `Request` values with logically identical content. They must hash
        // to the same value regardless of how the target Value was constructed (literal
        // vs explicit insertion order vs map type).
        let a = Request::new(
            "service.restart",
            serde_json::json!({
                "service": "nginx",
                "force":   true,
                "timeout": 30,
            }),
        );

        // Construct the same target with a different literal key order.
        let b = Request::new(
            "service.restart",
            serde_json::json!({
                "timeout": 30,
                "force":   true,
                "service": "nginx",
            }),
        );

        let ha = a.request_hash().expect("hash a must succeed");
        let hb = b.request_hash().expect("hash b must succeed");

        assert_eq!(
            ha, hb,
            "request_hash must be independent of target key ordering"
        );
        assert_eq!(ha.len(), 64, "request_hash must be 64 hex chars");
    }

    #[test]
    fn request_hash_changes_when_payload_changes() {
        // Same action, slightly different target → different hash. This is what makes
        // the IdempotencyConflict rule (S0.1 §3.3) detectable.
        let a = Request::new("service.restart", serde_json::json!({"service": "nginx"}));
        let b = Request::new("service.restart", serde_json::json!({"service": "apache"}));

        let ha = a.request_hash().expect("hash a must succeed");
        let hb = b.request_hash().expect("hash b must succeed");

        assert_ne!(
            ha, hb,
            "request_hash must change when any payload field changes"
        );
    }
}
