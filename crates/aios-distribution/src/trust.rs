//! Publisher trust-level vocabulary per S11.1 §3.1.
//!
//! The five-tier `PublisherTrustLevel` governs which package kinds a publisher
//! may ship, what default capability budget it receives, and whether new
//! installs are admitted.  Trust transitions downward (`Verified → Community`,
//! `Community → Deprecated`, etc.) are recorded in the AIOS-root-signed
//! publisher catalog.

use serde::{Deserialize, Serialize};

/// Closed enum — 5 tiers per S11.1 §3.1.
///
/// | Variant        | S11.1 label  | Can publish? |
/// |----------------|--------------|--------------|
/// | `AiosRoot`     | `AIOS_ROOT`  | Yes          |
/// | `Verified`     | `VERIFIED`   | Yes          |
/// | `Community`    | `COMMUNITY`  | Yes          |
/// | `Deprecated`   | `DEPRECATED` | No           |
/// | `Deplatformed` | `DEPLATFORMED` | No         |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublisherTrustLevel {
    /// The AIOS organisation itself — full capability budget, all package kinds.
    AiosRoot,
    /// Publisher passed onboarding review — broad capability budget.
    Verified,
    /// Lightweight self-attestation — tight sandbox floor enforced.
    Community,
    /// Being phased out — no new packages admitted.
    Deprecated,
    /// Explicitly removed by AIOS root — auto-quarantine on next health check.
    Deplatformed,
}

impl PublisherTrustLevel {
    /// Returns the canonical S11.1 label for this trust level.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AiosRoot => "AIOS_ROOT",
            Self::Verified => "VERIFIED",
            Self::Community => "COMMUNITY",
            Self::Deprecated => "DEPRECATED",
            Self::Deplatformed => "DEPLATFORMED",
        }
    }

    /// Returns `true` if a publisher at this trust level may publish new packages.
    ///
    /// `Deprecated` and `Deplatformed` publishers cannot publish; their existing
    /// installs may continue (or auto-quarantine in the `Deplatformed` case) but
    /// no new installs from them are admitted.
    #[must_use]
    pub const fn can_publish(self) -> bool {
        matches!(self, Self::AiosRoot | Self::Verified | Self::Community)
    }
}
