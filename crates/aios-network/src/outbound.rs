use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Closed outbound directive vocabulary (S8.1 §5).
///
/// Governs what a subject is allowed to connect to. The default for any subject
/// without a grant is `DenyAll`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OutboundDirective {
    /// Default for any subject without a grant.
    DenyAll,
    /// Loopback-only (`127.0.0.0/8`, `::1`).
    AllowLoopbackOnly,
    /// Only endpoints on an explicit allowlist.
    AllowListOnly {
        /// The allowlist set to reference.
        allowlist_id: String,
    },
    /// Only a specific VPN tunnel.
    AllowVpnOnly {
        /// The VPN tunnel ID.
        tunnel_id: String,
    },
    /// Full internet access. Humans only, not AI.
    AllowInternet,
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use strum::{EnumCount, IntoEnumIterator};

    #[test]
    fn outbound_directive_has_5_variants() {
        assert_eq!(OutboundDirective::COUNT, 5);
        assert_eq!(OutboundDirective::iter().count(), 5);
    }

    #[test]
    fn outbound_directive_deny_all_default() {
        let directive = OutboundDirective::DenyAll;
        let json = serde_json::to_string(&directive).unwrap();
        assert!(json.contains("DENY_ALL"));
    }

    #[test]
    fn outbound_directive_serde_round_trip() {
        for variant in OutboundDirective::iter() {
            let json = serde_json::to_string(&variant).unwrap();
            let back: OutboundDirective = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back, "round-trip failed: {json}");
        }
    }
}
