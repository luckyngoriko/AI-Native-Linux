//! Outbound grant data structures (S8.1 §4, INV I7+I8).
//!
//! `OutboundGrant` carries an Ed25519 signature from a registered trusted
//! authority over canonical bytes. `NetworkOutboundManifest` is append-only
//! at the subject level — grants cannot be removed, only tombstoned.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::allowlist::AllowlistEntry;
use crate::ids::SubjectId;

/// Simplified on-the-wire grant shape (S8.1 §4).
///
/// Distinct from the runtime [`crate::outbound::OutboundDirective`] —
/// this enum captures what appears in the grant payload, not the full
/// runtime decision vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OutboundDirectiveKind {
    /// Only endpoints on an explicit allowlist.
    AllowListOnly,
    /// Only a specific VPN tunnel.
    AllowVpnOnly {
        /// The VPN tunnel ID.
        tunnel_id: String,
    },
}

impl OutboundDirectiveKind {
    /// Stable label for canonical byte construction during signing.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::AllowListOnly => "ALLOW_LIST_ONLY",
            Self::AllowVpnOnly { .. } => "ALLOW_VPN_ONLY",
        }
    }
}

/// A signed outbound grant from a trusted authority (INV I7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboundGrant {
    /// Unique grant identifier.
    pub grant_id: String,
    /// The subject this grant governs.
    pub subject: SubjectId,
    /// Allowed endpoints under this grant.
    pub allowlist: Vec<AllowlistEntry>,
    /// The directive kind encoded in this grant.
    pub directive_kind: OutboundDirectiveKind,
    /// When this grant was issued.
    pub issued_at: DateTime<Utc>,
    /// Optional expiry. `None` means the grant never expires.
    pub expires_at: Option<DateTime<Utc>>,
    /// Hex-encoded Ed25519 public key fingerprint of the signing authority.
    pub signer_fingerprint: String,
    /// Ed25519 signature over the canonical byte sequence (64 bytes).
    pub signature: Vec<u8>,
}

impl OutboundGrant {
    /// Build the canonical byte sequence that must be signed (INV I7).
    ///
    /// Format: `grant_id || subject || allowlist_json || directive_kind_label || issued_at_rfc3339`
    #[must_use]
    pub fn canonical_signing_bytes(&self) -> Vec<u8> {
        let allowlist_json = serde_json::to_vec(&self.allowlist).unwrap_or_default();
        let mut out = Vec::new();
        out.extend_from_slice(self.grant_id.as_bytes());
        out.extend_from_slice(b"||");
        out.extend_from_slice(self.subject.0.as_bytes());
        out.extend_from_slice(b"||");
        out.extend_from_slice(&allowlist_json);
        out.extend_from_slice(b"||");
        out.extend_from_slice(self.directive_kind.label().as_bytes());
        out.extend_from_slice(b"||");
        out.extend_from_slice(self.issued_at.to_rfc3339().as_bytes());
        out
    }
}

/// Per-subject append-only manifest of outbound grants (INV I8).
///
/// Grants can only be **appended**, never removed or modified in-place.
/// To reduce permissions, issue a [`GrantTombstone`] via
/// `RevokeOutboundGrant` and then re-issue a fresh narrower grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkOutboundManifest {
    /// The subject this manifest governs.
    pub subject: SubjectId,
    /// All grants for this subject (monotonically growing).
    pub grants: Vec<OutboundGrant>,
    /// Unique manifest identifier.
    pub manifest_id: String,
    /// When this manifest was first created.
    pub created_at: DateTime<Utc>,
    /// Last time a grant was appended to this manifest.
    pub last_appended_at: DateTime<Utc>,
}

impl NetworkOutboundManifest {
    /// Append a grant to this manifest.
    ///
    /// # Panics
    ///
    /// Panics if the grant's subject does not match the manifest's subject
    /// (caller invariant — enforced by `OutboundGrantRegistry`).
    pub fn append_grant(&mut self, grant: OutboundGrant) {
        assert_eq!(
            grant.subject, self.subject,
            "grant subject must match manifest subject"
        );
        self.grants.push(grant);
        self.last_appended_at = Utc::now();
    }

    /// Count of grants currently in this manifest.
    #[must_use]
    pub const fn grant_count(&self) -> usize {
        self.grants.len()
    }
}

/// A tombstone record for a revoked grant (INV I8).
///
/// Tombstones are append-only as well — once a grant is revoked, the
/// tombstone cannot be removed. The effective allowlist for a subject
/// is the union of all non-tombstoned grants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantTombstone {
    /// The grant ID that was revoked.
    pub revoked_grant_id: String,
    /// When the revocation was recorded.
    pub revoked_at: DateTime<Utc>,
    /// Who requested the revocation.
    pub revoker: SubjectId,
    /// Human-readable reason for the revocation.
    pub reason: String,
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
    fn outbound_directive_kind_label_explicit() {
        assert_eq!(
            OutboundDirectiveKind::AllowListOnly.label(),
            "ALLOW_LIST_ONLY"
        );
        assert_eq!(
            OutboundDirectiveKind::AllowVpnOnly {
                tunnel_id: "tun0".into()
            }
            .label(),
            "ALLOW_VPN_ONLY"
        );
    }

    #[test]
    fn outbound_directive_kind_serde_round_trip() {
        let kinds = vec![
            OutboundDirectiveKind::AllowListOnly,
            OutboundDirectiveKind::AllowVpnOnly {
                tunnel_id: "wg-primary".into(),
            },
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let back: OutboundDirectiveKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, back);
        }
    }

    #[test]
    fn canonical_signing_bytes_deterministic_for_same_input() {
        let grant = OutboundGrant {
            grant_id: "g-1".into(),
            subject: SubjectId("human:test".into()),
            allowlist: vec![],
            directive_kind: OutboundDirectiveKind::AllowListOnly,
            issued_at: Utc::now(),
            expires_at: None,
            signer_fingerprint: "abc".into(),
            signature: vec![],
        };
        let a = grant.canonical_signing_bytes();
        let b = grant.canonical_signing_bytes();
        assert_eq!(a, b);
    }

    #[test]
    fn manifest_append_grant_updates_last_appended_at() {
        let mut manifest = NetworkOutboundManifest {
            subject: SubjectId("human:test".into()),
            grants: vec![],
            manifest_id: "m-1".into(),
            created_at: Utc::now(),
            last_appended_at: Utc::now(),
        };
        let before = manifest.last_appended_at;
        // tiny sleep to guarantee clock ticks forward
        std::thread::sleep(std::time::Duration::from_millis(1));
        manifest.append_grant(OutboundGrant {
            grant_id: "g-1".into(),
            subject: SubjectId("human:test".into()),
            allowlist: vec![],
            directive_kind: OutboundDirectiveKind::AllowListOnly,
            issued_at: Utc::now(),
            expires_at: None,
            signer_fingerprint: "abc".into(),
            signature: vec![],
        });
        assert!(manifest.last_appended_at > before);
    }

    #[test]
    fn manifest_grant_count_after_append_is_correct() {
        let mut manifest = NetworkOutboundManifest {
            subject: SubjectId("human:test".into()),
            grants: vec![],
            manifest_id: "m-1".into(),
            created_at: Utc::now(),
            last_appended_at: Utc::now(),
        };
        assert_eq!(manifest.grant_count(), 0);
        manifest.append_grant(OutboundGrant {
            grant_id: "g-1".into(),
            subject: SubjectId("human:test".into()),
            allowlist: vec![],
            directive_kind: OutboundDirectiveKind::AllowListOnly,
            issued_at: Utc::now(),
            expires_at: None,
            signer_fingerprint: "abc".into(),
            signature: vec![],
        });
        assert_eq!(manifest.grant_count(), 1);
    }
}
