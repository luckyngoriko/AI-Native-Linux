//! Verification intent identifiers and the T-064 intent carrier.

use std::fmt;

use aios_action::ActionId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// Verification intent id assigned by the S2.4 engine.
///
/// S2.4 §3 names the engine-assigned prefix as `vrfi_`. The T-064 brief used
/// `vri_`; this crate follows the spec-owned prefix.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IntentId(
    /// Canonical `vrfi_<ULID>` string.
    pub String,
);

impl IntentId {
    /// Canonical verification-intent prefix from S2.4 §3.
    pub const PREFIX: &'static str = "vrfi_";

    /// Mint a fresh verification intent id.
    #[must_use]
    pub fn new() -> Self {
        Self(format!("{}{}", Self::PREFIX, Ulid::new()))
    }

    /// Borrow the canonical id string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for IntentId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for IntentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for IntentId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// T-064 verification intent carrier.
///
/// The future parser task owns expression validation. This typed core stores
/// the caller-supplied expression and its BLAKE3 hex digest so later engine
/// work has a stable identity surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationIntent {
    /// Engine-assigned verification intent id.
    pub intent_id: IntentId,
    /// Action whose postcondition will be verified.
    pub action_id: ActionId,
    /// Caller-supplied verification expression.
    pub expression: String,
    /// BLAKE3 hex digest of [`Self::expression`].
    pub expression_hash: String,
    /// UTC creation time.
    pub created_at: DateTime<Utc>,
    /// Caller-requested timeout budget in whole seconds.
    pub timeout_seconds: u32,
}

impl VerificationIntent {
    /// Create a verification intent with a fresh id and current timestamp.
    #[must_use]
    pub fn new(action_id: ActionId, expression: impl Into<String>, timeout_seconds: u32) -> Self {
        let expression = expression.into();
        let expression_hash = blake3::hash(expression.as_bytes()).to_hex().to_string();

        Self {
            intent_id: IntentId::new(),
            action_id,
            expression,
            expression_hash,
            created_at: Utc::now(),
            timeout_seconds,
        }
    }
}
