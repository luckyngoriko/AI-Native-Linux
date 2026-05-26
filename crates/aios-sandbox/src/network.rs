use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Closed network posture vocabulary (S3.2).
///
/// Maps to the spec `NetworkMode` enum. Variant names follow the spec
/// vocabulary rather than the brief's suggested names per TRUST SPEC
/// directive.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    EnumIter,
    EnumCount,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NetworkPosture {
    /// All network access denied. Default for omitted network blocks.
    DenyAll,
    /// Loopback-only (`127.0.0.0/8`, `::1`). No external connectivity.
    LoopbackOnly,
    /// Only explicitly listed host endpoints permitted.
    HostLimited,
    /// Only endpoints on an explicit allow-list.
    ExplicitAllowlist,
    /// Full network access. Requires policy override.
    Full,
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use strum::{EnumCount, IntoEnumIterator};

    #[test]
    fn network_posture_has_five_variants() {
        assert_eq!(NetworkPosture::COUNT, 5);
        assert_eq!(NetworkPosture::iter().count(), 5);
    }

    #[test]
    fn network_posture_serde_round_trip() {
        for variant in NetworkPosture::iter() {
            let json = serde_json::to_string(&variant).unwrap();
            let back: NetworkPosture = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back, "round-trip failed for {variant:?}");
        }
    }

    #[test]
    fn deny_all_is_most_restrictive() {
        // Per spec strictness rank: DENY_ALL > LOOPBACK_ONLY > HOST_LIMITED >
        // EXPLICIT_ALLOWLIST > FULL. Our `PartialOrd` derive follows variant
        // declaration order (top-first = smallest = most restrictive).
        assert!(NetworkPosture::DenyAll < NetworkPosture::Full);
        assert!(NetworkPosture::LoopbackOnly < NetworkPosture::ExplicitAllowlist);
    }
}
