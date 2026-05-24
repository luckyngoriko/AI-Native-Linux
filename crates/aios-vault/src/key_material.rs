//! Broker-internal key material records and redaction guards.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::ser::Error as SerdeError;
use serde::{Deserialize, Serialize, Serializer};
use strum_macros::{EnumCount, EnumIter};

use crate::error::VaultError;

/// Internal key algorithm marker for T-046 broker-held material.
///
/// S5.2 closes `VaultMaterialKind`; concrete algorithms remain deployment
/// configuration. This small enum is the task-local internal marker and is not
/// a wire contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KeyAlgorithm {
    /// AES-256-GCM symmetric key material.
    Aes256Gcm,
    /// HMAC-SHA256 key material.
    HmacSha256,
    /// HKDF-SHA256 derivation input material.
    HkdfSha256,
    /// Ed25519 signing or verification key material.
    Ed25519,
    /// X25519 key agreement material.
    X25519,
}

/// Broker-internal key material.
///
/// This type is intentionally not a public wire payload. Its [`Debug`] and
/// [`Serialize`] implementations are leak guards: formatting redacts, and any
/// serialization attempt fails.
#[derive(Clone, PartialEq, Eq)]
pub struct KeyMaterial {
    /// Algorithm marker for the internal key bytes.
    pub algorithm: KeyAlgorithm,
    /// Material creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Raw key bytes held inside the broker only.
    pub bytes: Vec<u8>,
}

impl fmt::Debug for KeyMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<key-material-redacted>")
    }
}

impl Serialize for KeyMaterial {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Err(S::Error::custom(VaultError::KeyMaterialLeak.to_string()))
    }
}
