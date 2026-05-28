//! Policy bridge: `NetworkPolicyError` → `aios_policy::PolicyDecision` denial pathway.
//!
//! Translates L8 network errors into structured policy denial records consumable by
//! the Capability Runtime when a typed action is denied at the network layer.

use aios_action::ActionId;
use aios_policy::{ApprovalRequirement, Constraints, Decision, PolicyDecision};

use crate::error::{NetworkPolicyError, NetworkPolicyErrorCode};

/// Synthesise a structured `PolicyDecision` denial from a `NetworkPolicyError`.
///
/// The returned decision always has `decision: Deny` and carries a stable `reason_code`
/// matching `NetworkPolicyErrorCode` discriminators so the pipeline can route to
/// hard-deny classification, explain-logs, and evidence linkage.
#[must_use]
pub fn network_error_to_policy_denial(
    err: &NetworkPolicyError,
    decision_id: &str,
) -> PolicyDecision {
    let (reason_code, reason_message) = match err {
        NetworkPolicyError::DefaultDeny(msg) => ("DefaultDeny", msg.clone()),
        NetworkPolicyError::CrossGroupAccessForbidden {
            source_group,
            dest_group,
        } => (
            "CrossGroupAccessForbidden",
            format!("cross-group: {source_group} -> {dest_group}"),
        ),
        NetworkPolicyError::AiDirectInternetDenied {
            subject,
            attempted_endpoint,
        } => (
            "AiDirectInternetDenied",
            format!("AI subject {subject} attempted {attempted_endpoint}"),
        ),
        NetworkPolicyError::AllowlistFqdnFanoutExceeded {
            fqdn,
            resolved_count,
        } => (
            "AllowlistFqdnFanoutExceeded",
            format!("{fqdn} fan-out={resolved_count}"),
        ),
        NetworkPolicyError::ExposureEscalationDenied { from, to, reason } => (
            "ExposureEscalationDenied",
            format!("{from} -> {to}: {reason}"),
        ),
        NetworkPolicyError::GrantSignatureInvalid { grant_id, reason } => {
            ("GrantSignatureInvalid", format!("{grant_id}: {reason}"))
        }
        NetworkPolicyError::RawSocketBypassAttempted(subject) => {
            ("RawSocketBypassAttempted", format!("{subject}"))
        }
        NetworkPolicyError::ManifestMutationForbidden(detail) => {
            ("ManifestMutationForbidden", detail.clone())
        }
        NetworkPolicyError::ResolverSignatureInvalid(detail) => {
            ("ResolverSignatureInvalid", detail.clone())
        }
        NetworkPolicyError::VpnPeerKeySignatureInvalid(detail) => {
            ("VpnPeerKeySignatureInvalid", detail.clone())
        }
        NetworkPolicyError::PlainDnsForbidden(detail) => ("PlainDnsForbidden", detail.clone()),
        NetworkPolicyError::MdnsAdvertisementDenied(detail) => {
            ("MdnsAdvertisementDenied", detail.clone())
        }
        NetworkPolicyError::Internal(detail) => ("Internal", detail.clone()),
    };

    PolicyDecision {
        policy_decision_id: decision_id.to_owned(),
        action_id: ActionId::default(),
        request_hash: String::new(),
        bundle_version: String::new(),
        enrichment_snapshot_id: String::new(),
        decision: Decision::Deny,
        reason_code: reason_code.to_owned(),
        reason_message,
        constraints: Constraints::default(),
        approval: ApprovalRequirement::default(),
        evidence_receipt_id: String::new(),
        evaluated_at: chrono::Utc::now(),
        rules_consulted: 1,
        simulated: false,
    }
}

impl From<&NetworkPolicyErrorCode> for &'static str {
    fn from(code: &NetworkPolicyErrorCode) -> Self {
        match code {
            NetworkPolicyErrorCode::DefaultDeny => "DefaultDeny",
            NetworkPolicyErrorCode::CrossGroupAccessForbidden => "CrossGroupAccessForbidden",
            NetworkPolicyErrorCode::AiDirectInternetDenied => "AiDirectInternetDenied",
            NetworkPolicyErrorCode::AllowlistFqdnFanoutExceeded => "AllowlistFqdnFanoutExceeded",
            NetworkPolicyErrorCode::ExposureEscalationDenied => "ExposureEscalationDenied",
            NetworkPolicyErrorCode::GrantSignatureInvalid => "GrantSignatureInvalid",
            NetworkPolicyErrorCode::RawSocketBypassAttempted => "RawSocketBypassAttempted",
            NetworkPolicyErrorCode::ManifestMutationForbidden => "ManifestMutationForbidden",
            NetworkPolicyErrorCode::ResolverSignatureInvalid => "ResolverSignatureInvalid",
            NetworkPolicyErrorCode::VpnPeerKeySignatureInvalid => "VpnPeerKeySignatureInvalid",
            NetworkPolicyErrorCode::PlainDnsForbidden => "PlainDnsForbidden",
            NetworkPolicyErrorCode::MdnsAdvertisementDenied => "MdnsAdvertisementDenied",
            NetworkPolicyErrorCode::Internal => "Internal",
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test"
)]
mod tests {
    use super::*;

    #[test]
    fn default_deny_produces_policy_denial() {
        let err = NetworkPolicyError::DefaultDeny("no grant".into());
        let decision = network_error_to_policy_denial(&err, "poldec_test");
        assert_eq!(decision.decision, Decision::Deny);
        assert_eq!(decision.reason_code, "DefaultDeny");
    }

    #[test]
    fn ai_deny_produces_policy_denial() {
        let err = NetworkPolicyError::AiDirectInternetDenied {
            subject: crate::ids::SubjectId("subj_01".into()),
            attempted_endpoint: "api.openai.com:443".into(),
        };
        let decision = network_error_to_policy_denial(&err, "poldec_02");
        assert_eq!(decision.decision, Decision::Deny);
        assert_eq!(decision.reason_code, "AiDirectInternetDenied");
    }

    #[test]
    fn all_error_codes_map_to_static_str() {
        // Prove every code has a string representation.
        let codes = [
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
        assert_eq!(
            codes.len(),
            13,
            "NetworkPolicyErrorCode should have 13 variants"
        );
        for code in &codes {
            let s: &str = code.into();
            assert!(
                !s.is_empty(),
                "code {code:?} should map to non-empty string"
            );
        }
    }
}
