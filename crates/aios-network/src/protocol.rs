use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Closed protocol family vocabulary (S8.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProtocolFamily {
    /// TCP.
    Tcp,
    /// UDP.
    Udp,
    /// ICMP.
    Icmp,
    /// QUIC.
    Quic,
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
    fn protocol_family_has_tcp_udp_icmp_quic() {
        assert_eq!(ProtocolFamily::COUNT, 4);
        let variants: Vec<_> = ProtocolFamily::iter().collect();
        assert!(variants.contains(&ProtocolFamily::Tcp));
        assert!(variants.contains(&ProtocolFamily::Udp));
        assert!(variants.contains(&ProtocolFamily::Icmp));
        assert!(variants.contains(&ProtocolFamily::Quic));
    }

    #[test]
    fn protocol_family_serde_round_trip() {
        for variant in ProtocolFamily::iter() {
            let json = serde_json::to_string(&variant).unwrap();
            let back: ProtocolFamily = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back);
        }
    }
}
