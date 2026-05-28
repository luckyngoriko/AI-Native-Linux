use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

use crate::inbound::PortPolicy;
use crate::protocol::ProtocolFamily;

/// Closed allowlist entry kind vocabulary (S8.1 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AllowlistEntryKind {
    /// A host FQDN.
    HostFqdn,
    /// An IPv4 address.
    IpV4Address,
    /// An IPv6 address.
    IpV6Address,
    /// An IPv4 CIDR block.
    IpV4Cidr,
    /// An IPv6 CIDR block.
    IpV6Cidr,
    /// A DNS-over-TLS resolver.
    DnsOverTlsResolver,
    /// A VPN peer endpoint.
    VpnPeerEndpoint,
}

/// A single allowlist entry pairing a kind with its canonical text form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowlistEntry {
    /// The kind of this entry.
    pub kind: AllowlistEntryKind,
    /// Canonical text form (e.g., `"192.168.1.1"`, `"example.com"`).
    pub value: String,
    /// Port policy for this entry.
    pub port_policy: PortPolicy,
    /// Protocol family for this entry.
    pub protocol: ProtocolFamily,
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
    fn allowlist_entry_kind_has_7_variants() {
        assert_eq!(AllowlistEntryKind::COUNT, 7);
        assert_eq!(AllowlistEntryKind::iter().count(), 7);
    }

    #[test]
    fn allowlist_entry_serde_round_trip() {
        let entry = AllowlistEntry {
            kind: AllowlistEntryKind::HostFqdn,
            value: "example.com".into(),
            port_policy: PortPolicy::OperatorAssigned { port: 443 },
            protocol: ProtocolFamily::Tcp,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: AllowlistEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, back);
    }
}
