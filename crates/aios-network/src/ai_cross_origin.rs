use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Closed AI cross-origin posture (S8.1 INV I4).
///
/// Controls whether and how AI subjects can make outbound calls to external
/// model providers. The default is `DenyAllExternal`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AICrossOriginPosture {
    /// AI cannot reach the internet at all.
    DenyAllExternal,
    /// AI calls external models only through the Vault Broker.
    VaultBrokeredOnly {
        /// The Vault Broker capability handle.
        broker_handle: String,
    },
    /// AI must request operator approval per call.
    OperatorMediated {
        /// The operator's canonical subject ID.
        operator_canonical_id: String,
    },
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
    fn ai_cross_origin_posture_has_3_variants() {
        assert_eq!(AICrossOriginPosture::COUNT, 3);
        assert_eq!(AICrossOriginPosture::iter().count(), 3);
    }

    #[test]
    fn ai_cross_origin_posture_deny_all_external_carries_no_handle() {
        let posture = AICrossOriginPosture::DenyAllExternal;
        let json = serde_json::to_string(&posture).unwrap();
        assert!(json.contains("DENY_ALL_EXTERNAL"));
        // DenyAllExternal is a unit variant — no payload data.
        assert!(!json.contains("broker_handle"));
        assert!(!json.contains("operator_canonical_id"));
    }

    #[test]
    fn ai_cross_origin_posture_serde_round_trip() {
        for variant in AICrossOriginPosture::iter() {
            let json = serde_json::to_string(&variant).unwrap();
            let back: AICrossOriginPosture = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back, "round-trip failed: {json}");
        }
    }
}
