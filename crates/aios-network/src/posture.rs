use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Closed network posture vocabulary (S8.1 §3.1).
///
/// The system-wide network posture determines what network access is permitted.
/// Posture transitions are governed by the policy kernel; AI cannot escalate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NetworkPosture {
    /// Pre-recovery boot default. No network at all.
    Airgap,
    /// Recovery shell startup. Loopback only.
    LoopbackOnly,
    /// First-boot default. LAN-local only.
    LanLocal,
    /// Operator-granted LAN listening.
    LanExposed,
    /// Public internet. Requires recovery + STRONG + co-signer.
    Public,
}

impl NetworkPosture {
    /// Human-readable label for each posture.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Airgap => "airgap",
            Self::LoopbackOnly => "loopback-only",
            Self::LanLocal => "lan-local",
            Self::LanExposed => "lan-exposed",
            Self::Public => "public",
        }
    }
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
    fn network_posture_has_5_variants() {
        assert_eq!(NetworkPosture::COUNT, 5);
        assert_eq!(NetworkPosture::iter().count(), 5);
    }

    #[test]
    fn network_posture_label_for_airgap() {
        assert_eq!(NetworkPosture::Airgap.label(), "airgap");
    }

    #[test]
    fn network_posture_serde_round_trip() {
        for variant in NetworkPosture::iter() {
            let json = serde_json::to_string(&variant).unwrap();
            let back: NetworkPosture = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back, "round-trip failed for {variant:?}");
        }
    }
}
