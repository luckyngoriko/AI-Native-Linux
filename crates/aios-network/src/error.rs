use thiserror::Error;

use crate::ids::{GroupId, SubjectId};

/// Closed error code enum for pattern-matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetworkPolicyErrorCode {
    /// Default deny — no matching grant for this subject.
    DefaultDeny,
    /// Cross-group access forbidden (INV I1).
    CrossGroupAccessForbidden,
    /// AI direct internet denied (INV I4).
    AiDirectInternetDenied,
    /// Allowlist FQDN fan-out exceeded (INV I7).
    AllowlistFqdnFanoutExceeded,
    /// Exposure escalation denied (INV I3).
    ExposureEscalationDenied,
    /// Grant signature invalid (INV I9).
    GrantSignatureInvalid,
    /// Raw socket bypass attempted (INV I10).
    RawSocketBypassAttempted,
    /// Manifest mutation forbidden (INV I8).
    ManifestMutationForbidden,
    /// Resolver signature invalid (S8.4).
    ResolverSignatureInvalid,
    /// VPN peer key signature invalid (S8.4).
    VpnPeerKeySignatureInvalid,
    /// Plain DNS forbidden (INV I11).
    PlainDnsForbidden,
    /// mDNS advertisement denied (INV I12).
    MdnsAdvertisementDenied,
    /// Internal error.
    Internal,
}

/// Closed error taxonomy for L8 Network Policy (S8.1).
#[derive(Debug, Error)]
pub enum NetworkPolicyError {
    /// Default deny — no matching grant for this subject.
    #[error("default deny: {0}")]
    DefaultDeny(String),

    /// Cross-group access forbidden. Source group cannot reach destination group.
    #[error("cross-group access forbidden: {source_group} -> {dest_group}")]
    CrossGroupAccessForbidden {
        /// The source group.
        source_group: GroupId,
        /// The destination group.
        dest_group: GroupId,
    },

    /// AI direct internet denied (INV I4).
    #[error("AI direct internet denied: {subject} attempted {attempted_endpoint}")]
    AiDirectInternetDenied {
        /// The AI subject.
        subject: SubjectId,
        /// The endpoint the subject attempted to reach.
        attempted_endpoint: String,
    },

    /// Allowlist FQDN fan-out exceeded (INV I7).
    #[error("allowlist FQDN fan-out exceeded: {fqdn} resolved to {resolved_count} addresses")]
    AllowlistFqdnFanoutExceeded {
        /// The FQDN that was resolved.
        fqdn: String,
        /// How many addresses were resolved.
        resolved_count: usize,
    },

    /// Exposure escalation denied (INV I3).
    #[error("exposure escalation denied: from {from} to {to}: {reason}")]
    ExposureEscalationDenied {
        /// The source exposure class.
        from: String,
        /// The target exposure class.
        to: String,
        /// Why the escalation was denied.
        reason: String,
    },

    /// Grant signature invalid (INV I9).
    #[error("grant signature invalid: {grant_id}: {reason}")]
    GrantSignatureInvalid {
        /// The grant ID whose signature failed.
        grant_id: String,
        /// Why the signature was invalid.
        reason: String,
    },

    /// Raw socket bypass attempted (INV I10).
    #[error("raw socket bypass attempted by {0}")]
    RawSocketBypassAttempted(SubjectId),

    /// Manifest mutation forbidden (INV I8).
    #[error("manifest mutation forbidden: {0}")]
    ManifestMutationForbidden(String),

    /// Resolver signature invalid (S8.4).
    #[error("resolver signature invalid: {0}")]
    ResolverSignatureInvalid(String),

    /// VPN peer key signature invalid (S8.4).
    #[error("VPN peer key signature invalid: {0}")]
    VpnPeerKeySignatureInvalid(String),

    /// Plain DNS forbidden (INV I11).
    #[error("plain DNS forbidden: {0}")]
    PlainDnsForbidden(String),

    /// mDNS advertisement denied (INV I12).
    #[error("mDNS advertisement denied: {0}")]
    MdnsAdvertisementDenied(String),

    /// Internal error.
    #[error("internal network policy error: {0}")]
    Internal(String),
}

impl NetworkPolicyError {
    /// Returns the closed error code for pattern-matching.
    #[must_use]
    pub const fn code(&self) -> NetworkPolicyErrorCode {
        match self {
            Self::DefaultDeny(_) => NetworkPolicyErrorCode::DefaultDeny,
            Self::CrossGroupAccessForbidden { .. } => {
                NetworkPolicyErrorCode::CrossGroupAccessForbidden
            }
            Self::AiDirectInternetDenied { .. } => NetworkPolicyErrorCode::AiDirectInternetDenied,
            Self::AllowlistFqdnFanoutExceeded { .. } => {
                NetworkPolicyErrorCode::AllowlistFqdnFanoutExceeded
            }
            Self::ExposureEscalationDenied { .. } => {
                NetworkPolicyErrorCode::ExposureEscalationDenied
            }
            Self::GrantSignatureInvalid { .. } => NetworkPolicyErrorCode::GrantSignatureInvalid,
            Self::RawSocketBypassAttempted(_) => NetworkPolicyErrorCode::RawSocketBypassAttempted,
            Self::ManifestMutationForbidden(_) => NetworkPolicyErrorCode::ManifestMutationForbidden,
            Self::ResolverSignatureInvalid(_) => NetworkPolicyErrorCode::ResolverSignatureInvalid,
            Self::VpnPeerKeySignatureInvalid(_) => {
                NetworkPolicyErrorCode::VpnPeerKeySignatureInvalid
            }
            Self::PlainDnsForbidden(_) => NetworkPolicyErrorCode::PlainDnsForbidden,
            Self::MdnsAdvertisementDenied(_) => NetworkPolicyErrorCode::MdnsAdvertisementDenied,
            Self::Internal(_) => NetworkPolicyErrorCode::Internal,
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

    #[test]
    fn error_code_variant_count() {
        // 13 closed error codes covering INVs I1, I3, I4, I7, I8, I9, I10, I11, I12
        // plus S8.4 resolver/VPN/DNS/mDNS error codes.
        let codes: &[NetworkPolicyErrorCode] = &[
            NetworkPolicyErrorCode::DefaultDeny,
            NetworkPolicyErrorCode::CrossGroupAccessForbidden,
            NetworkPolicyErrorCode::AiDirectInternetDenied,
            NetworkPolicyErrorCode::AllowlistFqdnFanoutExceeded,
            NetworkPolicyErrorCode::ExposureEscalationDenied,
            NetworkPolicyErrorCode::GrantSignatureInvalid,
            NetworkPolicyErrorCode::RawSocketBypassAttempted,
            NetworkPolicyErrorCode::ManifestMutationForbidden,
            NetworkPolicyErrorCode::ResolverSignatureInvalid,
            NetworkPolicyErrorCode::VpnPeerKeySignatureInvalid,
            NetworkPolicyErrorCode::PlainDnsForbidden,
            NetworkPolicyErrorCode::MdnsAdvertisementDenied,
            NetworkPolicyErrorCode::Internal,
        ];
        assert_eq!(codes.len(), 13);
    }

    #[test]
    fn error_default_deny_code_matches() {
        let err = NetworkPolicyError::DefaultDeny("test".into());
        assert_eq!(err.code(), NetworkPolicyErrorCode::DefaultDeny);
    }

    #[test]
    fn error_cross_group_access_forbidden_code_matches() {
        let err = NetworkPolicyError::CrossGroupAccessForbidden {
            source_group: GroupId("src".into()),
            dest_group: GroupId("dst".into()),
        };
        assert_eq!(
            err.code(),
            NetworkPolicyErrorCode::CrossGroupAccessForbidden
        );
    }

    #[test]
    fn error_ai_direct_internet_denied_code_matches() {
        let err = NetworkPolicyError::AiDirectInternetDenied {
            subject: SubjectId("agent:test".into()),
            attempted_endpoint: "https://example.com".into(),
        };
        assert_eq!(err.code(), NetworkPolicyErrorCode::AiDirectInternetDenied);
    }

    #[test]
    fn error_display_non_empty_all_variants() {
        let subject = SubjectId("subj".into());
        let group_a = GroupId("ga".into());
        let group_b = GroupId("gb".into());

        let variants: &[NetworkPolicyError] = &[
            NetworkPolicyError::DefaultDeny("test".into()),
            NetworkPolicyError::CrossGroupAccessForbidden {
                source_group: group_a,
                dest_group: group_b,
            },
            NetworkPolicyError::AiDirectInternetDenied {
                subject,
                attempted_endpoint: "https://evil.com".into(),
            },
            NetworkPolicyError::AllowlistFqdnFanoutExceeded {
                fqdn: "example.com".into(),
                resolved_count: 42,
            },
            NetworkPolicyError::ExposureEscalationDenied {
                from: "Loopback".into(),
                to: "Public".into(),
                reason: "policy".into(),
            },
            NetworkPolicyError::GrantSignatureInvalid {
                grant_id: "g1".into(),
                reason: "bad sig".into(),
            },
            NetworkPolicyError::RawSocketBypassAttempted(SubjectId("agent:x".into())),
            NetworkPolicyError::ManifestMutationForbidden("mutated".into()),
            NetworkPolicyError::ResolverSignatureInvalid("bad-resolver".into()),
            NetworkPolicyError::VpnPeerKeySignatureInvalid("bad-peer".into()),
            NetworkPolicyError::PlainDnsForbidden("dns".into()),
            NetworkPolicyError::MdnsAdvertisementDenied("mdns".into()),
            NetworkPolicyError::Internal("boom".into()),
        ];

        for err in variants {
            let msg = format!("{err}");
            assert!(!msg.is_empty(), "empty Display for {err:?}");
        }
    }
}
