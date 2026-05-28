//! L8 Network Evidence Emitter (S8.1 + S8.4 ↔ S3.1) — typed lifecycle event
//! emission into the append-only Evidence Log.
//!
//! Every posture change, exposure FSM transition, outbound grant issue/revoke,
//! connection decision, cross-group/cross-origin denial, DNS query audit, VPN
//! tunnel lifecycle, peer key rotation, and mDNS posture/rejection produces a
//! chained evidence receipt. INV-015: NO raw signatures, NO private key
//! material. DNS audit records the question, not the answer (S8.4 §3).

use std::fmt::Write;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use aios_evidence::{ReceiptBuilder, ReceiptChain, RecordType, RetentionClass};

use crate::connection_evaluator::{ConnectionDecisionV2, EvaluateConnectionRequestV2};
use crate::error::NetworkPolicyError;
use crate::exposure_fsm::{ExposureApprovalLabel, ExposureTransition};
use crate::firewall::FirewallBackend;
use crate::ids::{GroupId, SubjectId};
use crate::mdns::MdnsAvahiPosture;
use crate::outbound_grant::GrantTombstone;
use crate::vpn::{PeerKeyRotation, TunnelLifecycleLabel};

// ---------------------------------------------------------------------------
// NetworkRecordType — 30 lifecycle event discriminators (18 S8.1 + 12 S8.4)
// ---------------------------------------------------------------------------

/// Closed set of L8 network policy lifecycle event types.
///
/// These map to the nearest `aios_evidence::RecordType` variant at emission
/// time. The mapping is stable and one-directional.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NetworkRecordType {
    // ── S8.1 Network Policy (18) ──
    /// System-wide network posture changed.
    NetworkPostureChanged,
    /// Exposure level was requested.
    ExposureRequested,
    /// Exposure level was granted (FOREVER retention).
    ExposureGranted,
    /// Exposure was activated (entered active state).
    ExposureActivated,
    /// Exposure heartbeat recorded.
    ExposureHeartbeatRecorded,
    /// Exposure was revoked.
    ExposureRevoked,
    /// PUBLIC exposure explicitly granted (FOREVER, INV I10).
    PublicExposureGranted,
    /// PUBLIC exposure TTL expired (FOREVER).
    PublicExposureTtlExpired,
    /// Outbound grant was issued.
    OutboundGrantIssued,
    /// Outbound grant was revoked (tombstoned).
    OutboundGrantRevoked,
    /// Connection was allowed by policy.
    ConnectionAllowed,
    /// Connection was denied by policy.
    ConnectionDenied,
    /// Cross-group access forbidden (INV I3).
    CrossGroupAccessForbidden,
    /// Allowlist FQDN fan-out exceeded (INV I9; `EXTENDED_60M` retention).
    AllowlistFqdnFanoutExceeded,
    /// AI direct internet access denied (FOREVER; INV I4).
    AiDirectInternetDenied,
    /// AI external model call brokered through Vault.
    AiExternalCallBrokered,
    /// Raw socket bypass attempted (FOREVER; INV I12).
    RawSocketBypassAttempted,
    /// Firewall degraded to iptables fallback (FOREVER marker).
    FirewallFallbackActivated,

    // ── S8.4 DNS / VPN / mDNS (12) ──
    /// DNS query audit record (`STANDARD_24M`; question-only per S8.4 §3).
    DnsQueryAudit,
    /// Resolver list admitted (new allowlist loaded).
    ResolverListAdmitted,
    /// Resolver list rotated (old allowlist replaced).
    ResolverListRotated,
    /// Plain DNS blocked (FOREVER; INV I9 negative).
    PlainDnsBlocked,
    /// VPN tunnel proposed.
    VpnTunnelProposed,
    /// VPN tunnel approved.
    VpnTunnelApproved,
    /// VPN tunnel activated.
    VpnTunnelActivated,
    /// VPN tunnel handshake recorded.
    VpnTunnelHandshakeRecorded,
    /// VPN tunnel revoked.
    VpnTunnelRevoked,
    /// VPN peer key rotated (FOREVER).
    VpnPeerKeyRotated,
    /// mDNS posture changed.
    MdnsPostureChanged,
    /// mDNS advertisement rejected.
    MdnsAdvertisementRejected,
}

impl NetworkRecordType {
    /// Map to the evidence crate's closed `RecordType`.
    const fn to_evidence_record_type(self) -> RecordType {
        match self {
            // ── S8.1 ──
            Self::NetworkPostureChanged => RecordType::NetworkPostureChanged,
            Self::ExposureRequested => RecordType::ExposureRequested,
            Self::ExposureGranted | Self::ExposureActivated => RecordType::ExposureGranted,
            Self::ExposureHeartbeatRecorded | Self::PublicExposureGranted => {
                RecordType::PublicExposureHeartbeat
            }
            Self::ExposureRevoked => RecordType::ExposureRevoked,
            Self::PublicExposureTtlExpired => RecordType::ExposureTerminatedTtlExpired,
            Self::OutboundGrantIssued => RecordType::OutboundGrantIssued,
            Self::OutboundGrantRevoked => RecordType::OutboundGrantRevoked,
            Self::ConnectionAllowed => RecordType::ActionPolicyDecision,
            Self::ConnectionDenied => RecordType::PolicyDecision,
            Self::CrossGroupAccessForbidden => RecordType::CrossGroupAccessDenied,
            Self::AllowlistFqdnFanoutExceeded => RecordType::AllowlistFqdnFanoutExceeded,
            Self::AiDirectInternetDenied => RecordType::AiDirectInternetDenied,
            Self::AiExternalCallBrokered => RecordType::ExternalModelCallBrokered,
            Self::RawSocketBypassAttempted => RecordType::RawSocketBypassAttempted,
            Self::FirewallFallbackActivated => RecordType::BackendDegradedNftablesToIptables,
            // ── S8.4 ──
            Self::DnsQueryAudit => RecordType::DnsQueryPerformed,
            Self::ResolverListAdmitted | Self::ResolverListRotated => RecordType::PolicyBundleLoad,
            Self::PlainDnsBlocked => RecordType::DnsPlainBlocked,
            Self::VpnTunnelProposed
            | Self::VpnTunnelApproved
            | Self::VpnTunnelActivated
            | Self::VpnTunnelHandshakeRecorded => RecordType::VpnTunnelEstablished,
            Self::VpnTunnelRevoked => RecordType::VpnTunnelFailed,
            Self::VpnPeerKeyRotated => RecordType::VpnProviderKeyRotated,
            Self::MdnsPostureChanged => RecordType::MdnsRequestReceived,
            Self::MdnsAdvertisementRejected => RecordType::MdnsBroadcastDenied,
        }
    }

    /// Retention class for this event type.
    const fn retention_class(self) -> RetentionClass {
        match self {
            // FOREVER: denials, tamper, constitutional barriers
            Self::ExposureGranted
            | Self::PublicExposureGranted
            | Self::PublicExposureTtlExpired
            | Self::AiDirectInternetDenied
            | Self::RawSocketBypassAttempted
            | Self::FirewallFallbackActivated
            | Self::PlainDnsBlocked
            | Self::VpnPeerKeyRotated => RetentionClass::Forever,
            // EXTENDED_60M: high-value forensic events
            Self::AllowlistFqdnFanoutExceeded => RetentionClass::Extended60M,
            // STANDARD_24M: everything else including DNS query audit
            _ => RetentionClass::Standard24M,
        }
    }

    /// Wire-name string for this discriminator.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            // ── S8.1 ──
            Self::NetworkPostureChanged => "NETWORK_POSTURE_CHANGED",
            Self::ExposureRequested => "EXPOSURE_REQUESTED",
            Self::ExposureGranted => "EXPOSURE_GRANTED",
            Self::ExposureActivated => "EXPOSURE_ACTIVATED",
            Self::ExposureHeartbeatRecorded => "EXPOSURE_HEARTBEAT_RECORDED",
            Self::ExposureRevoked => "EXPOSURE_REVOKED",
            Self::PublicExposureGranted => "PUBLIC_EXPOSURE_GRANTED",
            Self::PublicExposureTtlExpired => "PUBLIC_EXPOSURE_TTL_EXPIRED",
            Self::OutboundGrantIssued => "OUTBOUND_GRANT_ISSUED",
            Self::OutboundGrantRevoked => "OUTBOUND_GRANT_REVOKED",
            Self::ConnectionAllowed => "CONNECTION_ALLOWED",
            Self::ConnectionDenied => "CONNECTION_DENIED",
            Self::CrossGroupAccessForbidden => "CROSS_GROUP_ACCESS_FORBIDDEN",
            Self::AllowlistFqdnFanoutExceeded => "ALLOWLIST_FQDN_FANOUT_EXCEEDED",
            Self::AiDirectInternetDenied => "AI_DIRECT_INTERNET_DENIED",
            Self::AiExternalCallBrokered => "AI_EXTERNAL_CALL_BROKERED",
            Self::RawSocketBypassAttempted => "RAW_SOCKET_BYPASS_ATTEMPTED",
            Self::FirewallFallbackActivated => "FIREWALL_FALLBACK_ACTIVATED",
            // ── S8.4 ──
            Self::DnsQueryAudit => "DNS_QUERY_AUDIT",
            Self::ResolverListAdmitted => "RESOLVER_LIST_ADMITTED",
            Self::ResolverListRotated => "RESOLVER_LIST_ROTATED",
            Self::PlainDnsBlocked => "PLAIN_DNS_BLOCKED",
            Self::VpnTunnelProposed => "VPN_TUNNEL_PROPOSED",
            Self::VpnTunnelApproved => "VPN_TUNNEL_APPROVED",
            Self::VpnTunnelActivated => "VPN_TUNNEL_ACTIVATED",
            Self::VpnTunnelHandshakeRecorded => "VPN_TUNNEL_HANDSHAKE_RECORDED",
            Self::VpnTunnelRevoked => "VPN_TUNNEL_REVOKED",
            Self::VpnPeerKeyRotated => "VPN_PEER_KEY_ROTATED",
            Self::MdnsPostureChanged => "MDNS_POSTURE_CHANGED",
            Self::MdnsAdvertisementRejected => "MDNS_ADVERTISEMENT_REJECTED",
        }
    }
}

// ---------------------------------------------------------------------------
// EvidenceReceipt — network-side receipt view
// ---------------------------------------------------------------------------

/// Evidence receipt returned to L8 callers.
///
/// Carries the record identity, content hash, and chain sequence number.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceReceipt {
    /// Evidence receipt id (`evr_<ULID>`).
    pub record_id: String,
    /// BLAKE3-256 content hash (64 lowercase hex chars).
    pub hash: String,
    /// 0-based sequence position in the emitter's chain.
    pub sequence: u64,
}

impl EvidenceReceipt {
    fn from_evidence_receipt(r: &aios_evidence::EvidenceReceipt) -> Self {
        Self {
            record_id: r.receipt_id().as_str().to_owned(),
            hash: r.content_hash().to_owned(),
            sequence: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// PostureChangeReceipt re-export (mirrors controller.rs shape)
// ---------------------------------------------------------------------------

use crate::controller::PostureChangeReceipt;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Hex-encode a `[u8; 32]` key fingerprint.
fn hex_encode_32(bytes: &[u8; 32]) -> String {
    bytes.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

// ---------------------------------------------------------------------------
// NetworkEvidenceEmitter trait
// ---------------------------------------------------------------------------

/// S8.1 + S8.4 ↔ S3.1 — async contract for emitting network policy lifecycle
/// events into the Evidence Log.
///
/// Nineteen methods cover the full L8 network event surface. Implementations are
/// optional (`Option<Arc<dyn NetworkEvidenceEmitter>>`): when `None`, no
/// emission occurs and no error is raised.
#[async_trait]
pub trait NetworkEvidenceEmitter: Send + Sync {
    /// Emit a `NETWORK_POSTURE_CHANGED` evidence record.
    async fn emit_posture_changed(
        &self,
        receipt: &PostureChangeReceipt,
    ) -> Result<EvidenceReceipt, NetworkPolicyError>;

    /// Emit an exposure transition evidence record.
    async fn emit_exposure_transition(
        &self,
        transition: &ExposureTransition,
        actor: &str,
    ) -> Result<EvidenceReceipt, NetworkPolicyError>;

    /// Emit an `OUTBOUND_GRANT_ISSUED` evidence record.
    async fn emit_outbound_grant_issued(
        &self,
        grant_id: &str,
        subject: &SubjectId,
    ) -> Result<EvidenceReceipt, NetworkPolicyError>;

    /// Emit an `OUTBOUND_GRANT_REVOKED` evidence record.
    async fn emit_outbound_grant_revoked(
        &self,
        tombstone: &GrantTombstone,
    ) -> Result<EvidenceReceipt, NetworkPolicyError>;

    /// Emit a connection decision evidence record.
    async fn emit_connection_decision(
        &self,
        req: &EvaluateConnectionRequestV2,
        decision: &ConnectionDecisionV2,
    ) -> Result<EvidenceReceipt, NetworkPolicyError>;

    /// Emit a `CROSS_GROUP_ACCESS_FORBIDDEN` evidence record (INV I3).
    async fn emit_cross_group_forbidden(
        &self,
        source: &GroupId,
        dest: &GroupId,
    ) -> Result<EvidenceReceipt, NetworkPolicyError>;

    /// Emit an `ALLOWLIST_FQDN_FANOUT_EXCEEDED` evidence record (INV I9).
    async fn emit_fqdn_fanout_exceeded(
        &self,
        fqdn: &str,
        resolved_count: usize,
    ) -> Result<EvidenceReceipt, NetworkPolicyError>;

    /// Emit an `AI_DIRECT_INTERNET_DENIED` evidence record (INV I4).
    async fn emit_ai_direct_internet_denied(
        &self,
        subject: &SubjectId,
        endpoint: &str,
    ) -> Result<EvidenceReceipt, NetworkPolicyError>;

    /// Emit an `AI_EXTERNAL_CALL_BROKERED` evidence record.
    async fn emit_ai_external_call_brokered(
        &self,
        subject: &SubjectId,
        broker_handle: &str,
    ) -> Result<EvidenceReceipt, NetworkPolicyError>;

    /// Emit a `RAW_SOCKET_BYPASS_ATTEMPTED` evidence record (INV I12).
    async fn emit_raw_socket_bypass_attempted(
        &self,
        subject: &SubjectId,
    ) -> Result<EvidenceReceipt, NetworkPolicyError>;

    /// Emit a `FIREWALL_FALLBACK_ACTIVATED` evidence record.
    async fn emit_firewall_fallback_activated(
        &self,
        backend: FirewallBackend,
    ) -> Result<EvidenceReceipt, NetworkPolicyError>;

    /// Emit a `DNS_QUERY_AUDIT` evidence record (question-only per S8.4 §3).
    async fn emit_dns_query_audit(
        &self,
        fqdn: &str,
        resolver: crate::dns::ResolverBackend,
        outcome: &str,
    ) -> Result<EvidenceReceipt, NetworkPolicyError>;

    /// Emit a `RESOLVER_LIST_ADMITTED` evidence record.
    async fn emit_resolver_list_admitted(
        &self,
        list_id: &str,
        signer_fingerprint: &str,
    ) -> Result<EvidenceReceipt, NetworkPolicyError>;

    /// Emit a `RESOLVER_LIST_ROTATED` evidence record.
    async fn emit_resolver_list_rotated(
        &self,
        from_list_id: &str,
        to_list_id: &str,
    ) -> Result<EvidenceReceipt, NetworkPolicyError>;

    /// Emit a `PLAIN_DNS_BLOCKED` evidence record (INV I9 negative).
    async fn emit_plain_dns_blocked(
        &self,
        attempt_context: &str,
    ) -> Result<EvidenceReceipt, NetworkPolicyError>;

    /// Emit a VPN tunnel lifecycle event evidence record.
    async fn emit_vpn_tunnel_event(
        &self,
        tunnel_id: &str,
        label: TunnelLifecycleLabel,
    ) -> Result<EvidenceReceipt, NetworkPolicyError>;

    /// Emit a `VPN_PEER_KEY_ROTATED` evidence record (FOREVER).
    async fn emit_vpn_peer_key_rotated(
        &self,
        rotation: &PeerKeyRotation,
    ) -> Result<EvidenceReceipt, NetworkPolicyError>;

    /// Emit an `MDNS_POSTURE_CHANGED` evidence record.
    async fn emit_mdns_posture_changed(
        &self,
        new_posture: &MdnsAvahiPosture,
    ) -> Result<EvidenceReceipt, NetworkPolicyError>;

    /// Emit an `MDNS_ADVERTISEMENT_REJECTED` evidence record.
    async fn emit_mdns_advertisement_rejected(
        &self,
        service_type: &str,
        instance_name: &str,
        port: u16,
        reason: &str,
    ) -> Result<EvidenceReceipt, NetworkPolicyError>;
}

// ---------------------------------------------------------------------------
// InMemoryNetworkEvidenceEmitter
// ---------------------------------------------------------------------------

/// In-memory `NetworkEvidenceEmitter` backed by a `ReceiptChain`.
///
/// Every emission seals a new receipt, appends it to the chain, and returns
/// a `EvidenceReceipt` with the chain sequence number.
pub struct InMemoryNetworkEvidenceEmitter {
    chain: Arc<RwLock<ReceiptChain>>,
    subject: String,
}

impl InMemoryNetworkEvidenceEmitter {
    /// Create an emitter that stamps receipts with the given subject
    /// canonical id (e.g. `"service:aios-network"`).
    #[must_use]
    pub fn new(subject: impl Into<String>) -> Self {
        Self {
            chain: Arc::new(RwLock::new(ReceiptChain::new())),
            subject: subject.into(),
        }
    }

    /// Return the number of receipts currently in the chain (test seam).
    pub async fn receipt_count(&self) -> usize {
        self.chain.read().await.len()
    }

    /// Return the payload at the given 0-based index (test seam).
    #[must_use]
    pub async fn get_payload(&self, index: usize) -> Option<serde_json::Value> {
        let chain = self.chain.read().await;
        chain.receipts().get(index).map(|r| r.payload().clone())
    }

    /// Verify the full hash-chain integrity (test seam).
    ///
    /// # Errors
    ///
    /// Returns `Internal` wrapping the underlying chain error.
    pub async fn verify_chain(&self) -> Result<(), NetworkPolicyError> {
        self.chain
            .read()
            .await
            .verify_integrity()
            .map_err(|e| NetworkPolicyError::Internal(format!("chain integrity: {e}")))
    }

    /// Shared seal-and-append helper.
    async fn seal_and_append(
        &self,
        network_record_type: NetworkRecordType,
        payload: serde_json::Value,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        let mut chain = self.chain.write().await;
        let prev = chain.receipts().last();
        let seq = chain.len() as u64;

        let builder = ReceiptBuilder::new(
            network_record_type.to_evidence_record_type(),
            network_record_type.retention_class(),
            &self.subject,
        )
        .with_payload(payload);

        let receipt = builder
            .seal(prev)
            .map_err(|e| NetworkPolicyError::Internal(format!("seal: {e}")))?;

        let mut net_receipt = EvidenceReceipt::from_evidence_receipt(&receipt);
        net_receipt.sequence = seq;

        chain
            .append(receipt)
            .map_err(|e| NetworkPolicyError::Internal(format!("append: {e}")))?;
        drop(chain);

        Ok(net_receipt)
    }
}

#[async_trait]
impl NetworkEvidenceEmitter for InMemoryNetworkEvidenceEmitter {
    async fn emit_posture_changed(
        &self,
        receipt: &PostureChangeReceipt,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        let payload = serde_json::json!({
            "from": receipt.from.label(),
            "to": receipt.to.label(),
            "actor": receipt.actor.0,
            "at": receipt.at.to_rfc3339(),
        });
        self.seal_and_append(NetworkRecordType::NetworkPostureChanged, payload)
            .await
    }

    async fn emit_exposure_transition(
        &self,
        transition: &ExposureTransition,
        actor: &str,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        let payload = serde_json::json!({
            "from": format!("{}", transition.from),
            "to": format!("{}", transition.to),
            "reason": format!("{:?}", transition.reason),
            "actor": actor,
            "transitioned_at": transition.transitioned_at.to_rfc3339(),
        });
        let record_type = match transition.to {
            ExposureApprovalLabel::LanActive | ExposureApprovalLabel::PublicActive => {
                NetworkRecordType::ExposureActivated
            }
            ExposureApprovalLabel::LanApproved | ExposureApprovalLabel::PublicApproved => {
                NetworkRecordType::ExposureGranted
            }
            ExposureApprovalLabel::Revoked => NetworkRecordType::ExposureRevoked,
            _ => NetworkRecordType::ExposureRequested,
        };
        self.seal_and_append(record_type, payload).await
    }

    async fn emit_outbound_grant_issued(
        &self,
        grant_id: &str,
        subject: &SubjectId,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        let payload = serde_json::json!({
            "grant_id": grant_id,
            "subject": subject.0,
        });
        self.seal_and_append(NetworkRecordType::OutboundGrantIssued, payload)
            .await
    }

    async fn emit_outbound_grant_revoked(
        &self,
        tombstone: &GrantTombstone,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        let payload = serde_json::json!({
            "revoked_grant_id": tombstone.revoked_grant_id,
            "revoked_at": tombstone.revoked_at.to_rfc3339(),
            "revoker": tombstone.revoker.0,
            "reason": tombstone.reason,
        });
        self.seal_and_append(NetworkRecordType::OutboundGrantRevoked, payload)
            .await
    }

    async fn emit_connection_decision(
        &self,
        req: &EvaluateConnectionRequestV2,
        decision: &ConnectionDecisionV2,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        let (record_type, allowed, details) = match decision {
            ConnectionDecisionV2::Allowed {
                ref matched_rule_id,
                ..
            } => (
                NetworkRecordType::ConnectionAllowed,
                true,
                format!("matched: {matched_rule_id}"),
            ),
            ConnectionDecisionV2::Denied {
                ref code,
                ref reason,
                ..
            } => (
                NetworkRecordType::ConnectionDenied,
                false,
                format!("denied [{code:?}]: {reason}"),
            ),
        };
        let payload = serde_json::json!({
            "subject": req.subject.0,
            "destination_host": req.destination_host,
            "destination_port": req.destination_port,
            "protocol": req.protocol,
            "allowed": allowed,
            "details": details,
        });
        self.seal_and_append(record_type, payload).await
    }

    async fn emit_cross_group_forbidden(
        &self,
        source: &GroupId,
        dest: &GroupId,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        let payload = serde_json::json!({
            "source_group": source.0,
            "dest_group": dest.0,
        });
        self.seal_and_append(NetworkRecordType::CrossGroupAccessForbidden, payload)
            .await
    }

    async fn emit_fqdn_fanout_exceeded(
        &self,
        fqdn: &str,
        resolved_count: usize,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        let payload = serde_json::json!({
            "fqdn": fqdn,
            "resolved_count": resolved_count,
        });
        self.seal_and_append(NetworkRecordType::AllowlistFqdnFanoutExceeded, payload)
            .await
    }

    async fn emit_ai_direct_internet_denied(
        &self,
        subject: &SubjectId,
        endpoint: &str,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        let payload = serde_json::json!({
            "subject": subject.0,
            "attempted_endpoint": endpoint,
        });
        self.seal_and_append(NetworkRecordType::AiDirectInternetDenied, payload)
            .await
    }

    async fn emit_ai_external_call_brokered(
        &self,
        subject: &SubjectId,
        broker_handle: &str,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        let payload = serde_json::json!({
            "subject": subject.0,
            "broker_handle": broker_handle,
        });
        self.seal_and_append(NetworkRecordType::AiExternalCallBrokered, payload)
            .await
    }

    async fn emit_raw_socket_bypass_attempted(
        &self,
        subject: &SubjectId,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        let payload = serde_json::json!({
            "subject": subject.0,
        });
        self.seal_and_append(NetworkRecordType::RawSocketBypassAttempted, payload)
            .await
    }

    async fn emit_firewall_fallback_activated(
        &self,
        backend: FirewallBackend,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        let backend_label = serde_json::to_value(backend)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "unknown".to_string());
        let payload = serde_json::json!({
            "backend": backend_label,
        });
        self.seal_and_append(NetworkRecordType::FirewallFallbackActivated, payload)
            .await
    }

    async fn emit_dns_query_audit(
        &self,
        fqdn: &str,
        resolver: crate::dns::ResolverBackend,
        outcome: &str,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        // S8.4 §3: audit records the question (FQDN + resolver + outcome class),
        // NOT the answer set. INV-015: no raw signatures.
        let payload = serde_json::json!({
            "fqdn": fqdn,
            "resolver": format!("{resolver:?}"),
            "outcome": outcome,
        });
        self.seal_and_append(NetworkRecordType::DnsQueryAudit, payload)
            .await
    }

    async fn emit_resolver_list_admitted(
        &self,
        list_id: &str,
        signer_fingerprint: &str,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        let payload = serde_json::json!({
            "list_id": list_id,
            "signer_fingerprint": signer_fingerprint,
        });
        self.seal_and_append(NetworkRecordType::ResolverListAdmitted, payload)
            .await
    }

    async fn emit_resolver_list_rotated(
        &self,
        from_list_id: &str,
        to_list_id: &str,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        let payload = serde_json::json!({
            "from_list_id": from_list_id,
            "to_list_id": to_list_id,
        });
        self.seal_and_append(NetworkRecordType::ResolverListRotated, payload)
            .await
    }

    async fn emit_plain_dns_blocked(
        &self,
        attempt_context: &str,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        let payload = serde_json::json!({
            "attempt_context": attempt_context,
        });
        self.seal_and_append(NetworkRecordType::PlainDnsBlocked, payload)
            .await
    }

    async fn emit_vpn_tunnel_event(
        &self,
        tunnel_id: &str,
        label: TunnelLifecycleLabel,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        let label_str = format!("{label:?}");
        let record_type = match label {
            TunnelLifecycleLabel::Proposed => NetworkRecordType::VpnTunnelProposed,
            TunnelLifecycleLabel::Approved => NetworkRecordType::VpnTunnelApproved,
            TunnelLifecycleLabel::Active => NetworkRecordType::VpnTunnelActivated,
            TunnelLifecycleLabel::Failed | TunnelLifecycleLabel::Revoked => {
                NetworkRecordType::VpnTunnelRevoked
            }
        };
        let payload = serde_json::json!({
            "tunnel_id": tunnel_id,
            "label": label_str,
        });
        self.seal_and_append(record_type, payload).await
    }

    async fn emit_vpn_peer_key_rotated(
        &self,
        rotation: &PeerKeyRotation,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        // INV-015: NO raw signature bytes in the payload.
        let payload = serde_json::json!({
            "tunnel_id": rotation.tunnel_id,
            "old_pubkey_fingerprint": hex_encode_32(&rotation.old_pubkey),
            "new_pubkey_fingerprint": hex_encode_32(&rotation.new_pubkey),
            "rotated_at": rotation.rotated_at.to_rfc3339(),
            "authority_fingerprint": rotation.authority_fingerprint,
        });
        self.seal_and_append(NetworkRecordType::VpnPeerKeyRotated, payload)
            .await
    }

    async fn emit_mdns_posture_changed(
        &self,
        new_posture: &MdnsAvahiPosture,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        let posture_label = match new_posture {
            MdnsAvahiPosture::DenyDefault => "deny_default",
            MdnsAvahiPosture::RecoveryDenied => "recovery_denied",
            MdnsAvahiPosture::OperatorAuthorised { .. } => "operator_authorised",
            MdnsAvahiPosture::AirgapDenied => "airgap_denied",
        };
        let payload = serde_json::json!({
            "posture": posture_label,
        });
        self.seal_and_append(NetworkRecordType::MdnsPostureChanged, payload)
            .await
    }

    async fn emit_mdns_advertisement_rejected(
        &self,
        service_type: &str,
        instance_name: &str,
        port: u16,
        reason: &str,
    ) -> Result<EvidenceReceipt, NetworkPolicyError> {
        let payload = serde_json::json!({
            "service_type": service_type,
            "instance_name": instance_name,
            "port": port,
            "reason": reason,
        });
        self.seal_and_append(NetworkRecordType::MdnsAdvertisementRejected, payload)
            .await
    }
}

// ---------------------------------------------------------------------------
// Optional-emitter wiring helpers
// ---------------------------------------------------------------------------

/// Trait for types that accept an optional emitter.
///
/// Implemented on the 9 network policy subsystems so that the evidence half
/// can be wired independently of the policy half at construction time.
pub trait WithEmitter {
    /// Attach an optional `NetworkEvidenceEmitter`.
    ///
    /// When `None` is passed, the subsystem operates without evidence
    /// emission — existing callers and tests are unaffected.
    #[must_use]
    fn with_emitter(self, emitter: Option<Arc<dyn NetworkEvidenceEmitter>>) -> Self;
}
