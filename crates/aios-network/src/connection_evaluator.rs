//! Connection evaluator with cross-group access check (INV I3) and FQDN
//! fan-out bound (INV I9).
//!
//! Implements the S8.1 §6 evaluation pipeline — look up the subject's
//! effective allowlist from `OutboundGrantRegistry`, enforce cross-group
//! access policy, match destinations against allowlist entries
//! (FQDN / IPv4 / IPv6 / CIDR) with port policy + protocol checks, and
//! enforce the FQDN fan-out cardinality bound. DNS resolution and VPN
//! peer endpoint matching are deferred to T-157/T-158.

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::allowlist::AllowlistEntryKind;
use crate::error::{NetworkPolicyError, NetworkPolicyErrorCode};
use crate::grant_registry::OutboundGrantRegistry;
use crate::ids::{GroupId, SubjectId};
use crate::inbound::PortPolicy;
use crate::protocol::ProtocolFamily;

/// Evaluates connection requests against the subject's effective allowlist,
/// cross-group access policy (INV I3), and FQDN fan-out bound (INV I9).
pub struct ConnectionEvaluator {
    registry: Arc<OutboundGrantRegistry>,
    group_membership: RwLock<HashMap<SubjectId, GroupId>>,
    fqdn_fanout_limit: usize,
}

/// Request to evaluate a connection through the full evaluator pipeline.
#[derive(Debug, Clone)]
pub struct EvaluateConnectionRequestV2 {
    /// The subject requesting the connection.
    pub subject: SubjectId,
    /// Destination FQDN or IP address (no port).
    pub destination_host: String,
    /// Destination port number.
    pub destination_port: u16,
    /// Protocol family for this connection.
    pub protocol: ProtocolFamily,
    /// Optional destination group hint for cross-group checks (INV I3).
    /// `None` defaults to same-group — no cross-group check.
    pub destination_group_hint: Option<GroupId>,
}

/// Outcome of the full connection evaluation pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionDecisionV2 {
    /// Connection is allowed, with the matched rule and entry kind.
    Allowed {
        /// Identifier of the matching allowlist entry.
        matched_rule_id: String,
        /// Kind of the matching allowlist entry.
        allowlist_entry_kind: AllowlistEntryKind,
    },
    /// Connection is denied with a closed error code and human-readable reason.
    Denied {
        /// The denial reason code.
        code: NetworkPolicyErrorCode,
        /// Human-readable explanation.
        reason: String,
    },
}

/// Resolved FQDN with cardinality bound check (INV I9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFqdn {
    /// The FQDN that was resolved.
    pub fqdn: String,
    /// Resolved IP addresses (bounded by `fqdn_fanout_limit`).
    pub addresses: Vec<std::net::IpAddr>,
    /// Timestamp of resolution (wall clock).
    pub resolved_at: DateTime<Utc>,
}

impl ConnectionEvaluator {
    /// Create a new evaluator backed by the given grant registry.
    #[must_use]
    pub fn new(registry: Arc<OutboundGrantRegistry>) -> Self {
        Self {
            registry,
            group_membership: RwLock::new(HashMap::new()),
            fqdn_fanout_limit: 16,
        }
    }

    /// Override the default FQDN fan-out limit (default 16 per INV I9).
    #[must_use]
    pub const fn with_fanout_limit(mut self, limit: usize) -> Self {
        self.fqdn_fanout_limit = limit;
        self
    }

    /// Register a subject as belonging to a group for cross-group checks (INV I3).
    ///
    /// # Errors
    ///
    /// Returns `Internal` on lock poisoning.
    pub async fn register_subject_group(
        &self,
        subject: SubjectId,
        group: GroupId,
    ) -> Result<(), NetworkPolicyError> {
        self.group_membership.write().await.insert(subject, group);
        Ok(())
    }

    /// Evaluate a connection request through the full pipeline.
    ///
    /// # Pipeline order
    ///
    /// 1. **INV I3** — cross-group access check (if `destination_group_hint` is set).
    /// 2. Look up the subject's effective allowlist from the registry.
    /// 3. Match each allowlist entry against the request (kind, port policy, protocol).
    /// 4. First match wins → `Allowed`; no match → `Denied { DefaultDeny }`.
    ///
    /// # Errors
    ///
    /// Returns `Internal` on lock poisoning.
    pub async fn evaluate(
        &self,
        req: EvaluateConnectionRequestV2,
    ) -> Result<ConnectionDecisionV2, NetworkPolicyError> {
        // --- INV I3: cross-group access check ---
        if let Some(ref dest_group) = req.destination_group_hint {
            let memberships = self.group_membership.read().await;
            if let Some(src_group) = memberships.get(&req.subject) {
                if src_group != dest_group {
                    return Ok(ConnectionDecisionV2::Denied {
                        code: NetworkPolicyErrorCode::CrossGroupAccessForbidden,
                        reason: format!(
                            "source group {} cannot reach destination group {}",
                            src_group.0, dest_group.0
                        ),
                    });
                }
            }
        }

        // --- Look up effective allowlist ---
        let entries = self.registry.effective_allowlist(&req.subject).await;

        // --- Match against allowlist entries ---
        for entry in &entries {
            // Protocol must match.
            if entry.protocol != req.protocol {
                continue;
            }

            // Port policy must match.
            if !port_policy_matches(entry.port_policy, req.destination_port) {
                continue;
            }

            // Match by entry kind.
            let matched = match entry.kind {
                AllowlistEntryKind::HostFqdn => {
                    req.destination_host.eq_ignore_ascii_case(&entry.value)
                }
                AllowlistEntryKind::IpV4Address => {
                    match_ipv4_exact(&req.destination_host, &entry.value)
                }
                AllowlistEntryKind::IpV6Address => {
                    match_ipv6_exact(&req.destination_host, &entry.value)
                }
                AllowlistEntryKind::IpV4Cidr => ipv4_in_cidr(&req.destination_host, &entry.value),
                AllowlistEntryKind::IpV6Cidr => ipv6_in_cidr(&req.destination_host, &entry.value),
                // Deferred to T-157/T-158.
                AllowlistEntryKind::DnsOverTlsResolver | AllowlistEntryKind::VpnPeerEndpoint => {
                    continue;
                }
            };

            if matched {
                let kind_label = match entry.kind {
                    AllowlistEntryKind::HostFqdn => "fqdn",
                    AllowlistEntryKind::IpV4Address => "ipv4",
                    AllowlistEntryKind::IpV6Address => "ipv6",
                    AllowlistEntryKind::IpV4Cidr => "cidr4",
                    AllowlistEntryKind::IpV6Cidr => "cidr6",
                    _ => "other",
                };
                return Ok(ConnectionDecisionV2::Allowed {
                    matched_rule_id: format!("{kind_label}:{}", entry.value),
                    allowlist_entry_kind: entry.kind,
                });
            }
        }

        // No matching entry — default deny.
        Ok(ConnectionDecisionV2::Denied {
            code: NetworkPolicyErrorCode::DefaultDeny,
            reason: "no allowlist entry matches".into(),
        })
    }

    /// Check FQDN fan-out cardinality bound (INV I9).
    ///
    /// No real DNS resolution — the caller supplies pre-resolved addresses.
    /// Returns `Err(AllowlistFqdnFanoutExceeded)` when the address count
    /// exceeds `self.fqdn_fanout_limit`.
    ///
    /// # Errors
    ///
    /// Returns `AllowlistFqdnFanoutExceeded` when `simulated_addresses.len()`
    /// exceeds the fan-out limit.
    pub fn resolve_fqdn_bounded(
        &self,
        fqdn: &str,
        simulated_addresses: Vec<std::net::IpAddr>,
    ) -> Result<ResolvedFqdn, NetworkPolicyError> {
        if simulated_addresses.len() > self.fqdn_fanout_limit {
            return Err(NetworkPolicyError::AllowlistFqdnFanoutExceeded {
                fqdn: fqdn.to_string(),
                resolved_count: simulated_addresses.len(),
            });
        }
        Ok(ResolvedFqdn {
            fqdn: fqdn.to_string(),
            addresses: simulated_addresses,
            resolved_at: Utc::now(),
        })
    }
}

// ── port policy matching ──────────────────────────────────────────────────────

const fn port_policy_matches(policy: PortPolicy, port: u16) -> bool {
    match policy {
        PortPolicy::WellKnown => port <= 1023,
        PortPolicy::RegisteredEphemeral => port >= 1024,
        PortPolicy::OperatorAssigned { port: assigned } => port == assigned,
    }
}

// ── IP matching helpers ───────────────────────────────────────────────────────

fn match_ipv4_exact(host: &str, expected: &str) -> bool {
    let Ok(ip) = host.parse::<Ipv4Addr>() else {
        return false;
    };
    let Ok(expected_ip) = expected.parse::<Ipv4Addr>() else {
        return false;
    };
    ip == expected_ip
}

fn match_ipv6_exact(host: &str, expected: &str) -> bool {
    let Ok(ip) = host.parse::<Ipv6Addr>() else {
        return false;
    };
    let Ok(expected_ip) = expected.parse::<Ipv6Addr>() else {
        return false;
    };
    ip == expected_ip
}

fn ipv4_in_cidr(host: &str, cidr: &str) -> bool {
    let Ok(ip) = host.parse::<Ipv4Addr>() else {
        return false;
    };
    let Some((net, prefix)) = parse_ipv4_cidr(cidr) else {
        return false;
    };
    let ip_bits = u32::from_be_bytes(ip.octets());
    let net_bits = u32::from_be_bytes(net.octets());
    let mask = if prefix == 0 {
        0u32
    } else {
        u32::MAX.checked_shl(32 - u32::from(prefix)).unwrap_or(0)
    };
    (ip_bits & mask) == (net_bits & mask)
}

fn ipv6_in_cidr(host: &str, cidr: &str) -> bool {
    let Ok(ip) = host.parse::<Ipv6Addr>() else {
        return false;
    };
    let Some((net, prefix)) = parse_ipv6_cidr(cidr) else {
        return false;
    };
    let ip_bits = u128::from_be_bytes(ip.octets());
    let net_bits = u128::from_be_bytes(net.octets());
    let mask = if prefix == 0 {
        0u128
    } else {
        u128::MAX.checked_shl(128 - u32::from(prefix)).unwrap_or(0)
    };
    (ip_bits & mask) == (net_bits & mask)
}

fn parse_ipv4_cidr(cidr: &str) -> Option<(Ipv4Addr, u8)> {
    let (net_str, prefix_str) = cidr.split_once('/')?;
    let net: Ipv4Addr = net_str.parse().ok()?;
    let prefix: u8 = prefix_str.parse().ok()?;
    if prefix > 32 {
        return None;
    }
    Some((net, prefix))
}

fn parse_ipv6_cidr(cidr: &str) -> Option<(Ipv6Addr, u8)> {
    let (net_str, prefix_str) = cidr.split_once('/')?;
    let net: Ipv6Addr = net_str.parse().ok()?;
    let prefix: u8 = prefix_str.parse().ok()?;
    if prefix > 128 {
        return None;
    }
    Some((net, prefix))
}
