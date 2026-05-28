//! Capability bridge: `OutboundGrant` → typed `ActionEnvelope`.
//!
//! Converts an outbound grant into a `network.outbound.grant_issue` action envelope
//! so the Capability Runtime / Policy Kernel can audit it through the normal pipeline.

use aios_action::envelope::ActionEnvelope;
use aios_action::identity::Identity;
use aios_action::request::{DryRunMode, Request};
use aios_action::trace::Trace;

use crate::outbound_grant::OutboundGrant;

/// Turn an `OutboundGrant` into a typed `network.outbound.grant_issue` action envelope.
///
/// The envelope wraps the grant as a JSON payload under `action = "network.outbound.grant_issue"`.
/// The caller identity is set to the grant subject with `is_ai: false` — the cognitive core
/// may override this when an AI agent requests the grant.
#[must_use]
pub fn outbound_grant_to_capability_proposal(grant: &OutboundGrant) -> ActionEnvelope {
    let identity = Identity {
        subject_canonical_id: grant.subject.0.clone(),
        is_ai: false,
        session_id: None,
    };

    let payload = serde_json::to_value(grant).unwrap_or_default();

    let request = Request {
        action: "network.outbound.grant_issue".to_owned(),
        target: payload,
        idempotency_key: Some(format!("grant_issue_{}", grant.grant_id)),
        parent_action_id: None,
        dry_run: DryRunMode::Live,
    };

    let trace = Trace {
        trace_id: String::new(),
        span_id: String::new(),
        parent_span_id: None,
    };

    ActionEnvelope::new(identity, request, trace)
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
    use crate::ids::SubjectId;

    #[test]
    fn grant_produces_action_envelope() {
        let grant = OutboundGrant {
            grant_id: "grt_test01".into(),
            subject: SubjectId("subj_test01".into()),
            allowlist: vec![],
            directive_kind: crate::outbound_grant::OutboundDirectiveKind::AllowListOnly,
            issued_at: chrono::Utc::now(),
            expires_at: None,
            signer_fingerprint: String::new(),
            signature: vec![],
        };

        let env = outbound_grant_to_capability_proposal(&grant);
        assert_eq!(
            env.request.action, "network.outbound.grant_issue",
            "action name must match"
        );
        assert_eq!(env.identity.subject_canonical_id, "subj_test01");
    }
}
