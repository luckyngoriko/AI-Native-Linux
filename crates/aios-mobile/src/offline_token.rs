//! Offline approval token — a time-bounded, single-use bearer token emitted
//! by the AIOS host for approval when no live connection is available.

use crate::enums::ApprovalRiskBand;
use chrono::{DateTime, Duration, Utc};

/// A single-use offline approval token that allows a mobile surface to
/// approve a bound action without a live connection. Maximum risk band
/// is `Medium` — high and critical risk approvals require live transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfflineApprovalToken {
    /// Unique token identifier (format `oatk_<ULID>`).
    pub token_id: String,
    /// The surface this token is bound to.
    pub surface_id: String,
    /// Canonical hash of the action request this token approves.
    pub bound_action_canonical_hash: String,
    /// Maximum risk band this token may approve (capped at `Medium`).
    pub max_risk_band: ApprovalRiskBand,
    /// Always `true` — tokens are single-use by construction.
    pub single_use: bool,
    /// Earliest time the token becomes valid.
    pub not_before: DateTime<Utc>,
    /// Time after which the token is no longer valid.
    pub expires_at: DateTime<Utc>,
}

impl OfflineApprovalToken {
    /// Creates a new offline approval token. Returns `None` if
    /// `max_risk_band` exceeds `Medium`.
    #[must_use]
    pub fn new(
        surface_id: String,
        bound_action_canonical_hash: String,
        max_risk_band: ApprovalRiskBand,
        validity_seconds: i64,
    ) -> Option<Self> {
        if max_risk_band > ApprovalRiskBand::Medium {
            return None;
        }
        let token_id = format!("oatk_{}", ulid::Ulid::new());
        let now = Utc::now();
        let expires_at = now + Duration::seconds(validity_seconds);
        Some(Self {
            token_id,
            surface_id,
            bound_action_canonical_hash,
            max_risk_band,
            single_use: true,
            not_before: now,
            expires_at,
        })
    }

    /// Returns `true` if the token is valid at the given time (within the
    /// validity window and not expired).
    #[must_use]
    pub fn is_valid(&self, at: DateTime<Utc>) -> bool {
        at >= self.not_before && at <= self.expires_at
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn token_creation_low_risk_succeeds() {
        let token = OfflineApprovalToken::new(
            "msrf_01TEST".to_string(),
            "abcdef1234567890".to_string(),
            ApprovalRiskBand::Low,
            3600,
        );
        assert!(token.is_some());
        let t = token.unwrap();
        assert!(t.single_use);
        assert!(t.token_id.starts_with("oatk_"));
        assert_eq!(t.max_risk_band, ApprovalRiskBand::Low);
    }

    #[test]
    fn token_creation_medium_risk_succeeds() {
        let token = OfflineApprovalToken::new(
            "msrf_01TEST".to_string(),
            "abcdef1234567890".to_string(),
            ApprovalRiskBand::Medium,
            3600,
        );
        assert!(token.is_some());
    }

    #[test]
    fn high_risk_rejected() {
        let token = OfflineApprovalToken::new(
            "msrf_01TEST".to_string(),
            "abcdef1234567890".to_string(),
            ApprovalRiskBand::High,
            3600,
        );
        assert!(token.is_none());
    }

    #[test]
    fn critical_risk_rejected() {
        let token = OfflineApprovalToken::new(
            "msrf_01TEST".to_string(),
            "abcdef1234567890".to_string(),
            ApprovalRiskBand::Critical,
            3600,
        );
        assert!(token.is_none());
    }

    #[test]
    fn token_is_valid_within_window() {
        let token = OfflineApprovalToken::new(
            "msrf_01TEST".to_string(),
            "abcdef1234567890".to_string(),
            ApprovalRiskBand::Low,
            3600,
        )
        .unwrap();
        let now = Utc::now();
        assert!(token.is_valid(now));
        let in_window = now + chrono::Duration::seconds(1800);
        assert!(token.is_valid(in_window));
    }

    #[test]
    fn token_is_invalid_before_not_before() {
        let token = OfflineApprovalToken::new(
            "msrf_01TEST".to_string(),
            "abcdef1234567890".to_string(),
            ApprovalRiskBand::Low,
            3600,
        )
        .unwrap();
        let before = token.not_before - chrono::Duration::seconds(1);
        assert!(!token.is_valid(before));
    }

    #[test]
    fn token_is_invalid_after_expiry() {
        let token = OfflineApprovalToken::new(
            "msrf_01TEST".to_string(),
            "abcdef1234567890".to_string(),
            ApprovalRiskBand::Low,
            3600,
        )
        .unwrap();
        let after = token.expires_at + chrono::Duration::seconds(1);
        assert!(!token.is_valid(after));
    }
}
