//! Vault capability records and capability-state vocabulary (S5.2).

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};
use ulid::Ulid;

use crate::identity::SubjectRef;

/// Validate `<prefix><ULID>` and return the owned canonical string.
fn validate_prefixed_ulid(input: &str, expected_prefix: &'static str) -> Result<String, String> {
    if input.is_empty() {
        return Err("identifier is empty".to_owned());
    }

    let Some(body) = input.strip_prefix(expected_prefix) else {
        return Err(format!("expected prefix {expected_prefix}, got {input}"));
    };

    Ulid::from_string(body).map_err(|err| format!("invalid ULID body for {input}: {err}"))?;

    Ok(input.to_owned())
}

/// Mint a fresh `<prefix><ULID>` string.
fn fresh_prefixed_ulid(prefix: &'static str) -> String {
    format!("{prefix}{}", Ulid::new())
}

/// Stable vault capability identifier: `"cap_<ULID>"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilityId(String);

impl CapabilityId {
    /// Canonical capability identifier prefix.
    pub const PREFIX: &'static str = "cap_";

    /// Mint a fresh capability id.
    #[must_use]
    pub fn new() -> Self {
        Self(fresh_prefixed_ulid(Self::PREFIX))
    }

    /// Validate and adopt an externally supplied capability id.
    ///
    /// # Errors
    ///
    /// Returns a string error when the prefix is not `cap_` or the body is not
    /// a valid ULID.
    pub fn parse(input: &str) -> Result<Self, String> {
        validate_prefixed_ulid(input, Self::PREFIX).map(Self)
    }

    /// Borrow the canonical string form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for CapabilityId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for CapabilityId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// S5.2 `VaultCapabilityClass` closed enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapabilityClass {
    /// `KEY_SIGN` — sign a blob with private key material.
    KeySign,
    /// `KEY_VERIFY` — verify a signature with public key material.
    KeyVerify,
    /// `KEY_ENCRYPT` — encrypt or wrap data using vault-held material.
    KeyEncrypt,
    /// `KEY_DECRYPT` — decrypt or unwrap data using vault-held material.
    KeyDecrypt,
    /// `MAC_GENERATE` — produce a MAC.
    MacGenerate,
    /// `MAC_VERIFY` — verify a MAC.
    MacVerify,
    /// `RANDOM_GENERATE` — return broker-generated random bytes.
    RandomGenerate,
    /// `SECRET_GET` — restricted raw-secret reveal path.
    SecretGet,
    /// `BOOTSTRAP_KEY_SIGN` — Wave 9 first-boot one-shot signing exception.
    BootstrapKeySign,
}

/// S5.2 capability lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapabilityState {
    /// `DRAFT` — issuance has started but is not active.
    Draft,
    /// `ACTIVE` — capability may be exercised.
    Active,
    /// `EXPIRED` — time or usage budget exhausted.
    Expired,
    /// `REVOKED` — revoked explicitly or by bundle rollover.
    Revoked,
    /// `ROTATED` — material rotated while retaining capability identity.
    Rotated,
    /// `DISCARDED` — issuance rejected before activation.
    Discarded,
}

/// Opaque pointer to broker-internal key storage.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KeyMaterialHandle(
    /// Internal storage pointer; never display directly in operator-facing logs.
    pub String,
);

impl fmt::Display for KeyMaterialHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<vault-handle>")
    }
}

/// S5.2 public vault capability record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct VaultCapability {
    /// Stable `cap_<ULID>` capability id.
    pub capability_id: CapabilityId,
    /// Closed class of operation this capability authorizes.
    pub class: CapabilityClass,
    /// Subject to which this capability was issued.
    pub issued_to: SubjectRef,
    /// Issuance timestamp.
    pub issued_at: DateTime<Utc>,
    /// Optional hard expiry timestamp.
    pub expires_at: Option<DateTime<Utc>>,
    /// Current lifecycle state.
    pub state: CapabilityState,
    /// Opaque handle to broker-internal storage; never contains key bytes.
    pub key_material_handle: KeyMaterialHandle,
}
