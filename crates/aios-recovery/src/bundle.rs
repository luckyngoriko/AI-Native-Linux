//! S9.1 degraded-subset recovery bundle types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Static signed material pre-loaded at recovery entry.
///
/// This bundle anchors the degraded policy subset needed to evaluate S2.3 hard
/// denies and S5.4 `STRONG_SOLO` recovery-only overrides while the full system
/// is unavailable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryBundle {
    /// Recovery bundle identifier.
    pub bundle_id: String,
    /// UTC timestamp when the bundle was loaded into the recovery context.
    pub loaded_at: DateTime<Utc>,
    /// Signed hard-deny material available to the degraded policy subset.
    pub hard_deny_signatures: Vec<String>,
    /// Signed override material available to the degraded override subset.
    pub override_signatures: Vec<String>,
    /// Signing authority for the recovery bundle.
    pub signing_authority: String,
}
