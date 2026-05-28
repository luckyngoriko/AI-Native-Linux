use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Closed inbound exposure class (S8.1 §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InboundExposureClass {
    /// Only loopback (`127.0.0.1`, `::1`).
    Loopback,
    /// LAN-local only.
    Lan,
    /// Public internet.
    Public,
}

/// Closed port policy vocabulary (S8.1 §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PortPolicy {
    /// Well-known ports (0–1023).
    WellKnown,
    /// Registered/ephemeral ports (1024–65535).
    RegisteredEphemeral,
    /// A specific operator-assigned port.
    OperatorAssigned {
        /// The assigned port number.
        port: u16,
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

    #[test]
    fn inbound_exposure_class_loopback_lan_public() {
        let json = serde_json::to_string(&InboundExposureClass::Loopback).unwrap();
        assert!(json.contains("LOOPBACK"));

        let json = serde_json::to_string(&InboundExposureClass::Lan).unwrap();
        assert!(json.contains("LAN"));

        let json = serde_json::to_string(&InboundExposureClass::Public).unwrap();
        assert!(json.contains("PUBLIC"));
    }

    #[test]
    fn port_policy_operator_assigned_round_trip() {
        let policy = PortPolicy::OperatorAssigned { port: 8443 };
        let json = serde_json::to_string(&policy).unwrap();
        let back: PortPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(policy, back);
    }

    #[test]
    fn port_policy_serde_round_trip() {
        for policy in &[
            PortPolicy::WellKnown,
            PortPolicy::RegisteredEphemeral,
            PortPolicy::OperatorAssigned { port: 22 },
        ] {
            let json = serde_json::to_string(policy).unwrap();
            let back: PortPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(*policy, back);
        }
    }
}
