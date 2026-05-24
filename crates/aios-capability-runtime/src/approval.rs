//! Approval orchestration types — `ApprovalRequest`, `ApprovalBinding`,
//! `ApprovalBindingState` (S10.1 §6 ↔ S5.3 §3.1 / §5 / §6 / §13).
//!
//! T-034 implements the L3 runtime side of the S5.3 Approval Mechanics
//! contract. The runtime is the **consumer** of the approval state machine:
//! when [`aios_policy::Decision::RequireApproval`] short-circuits pipeline
//! step 2, the runtime emits an [`ApprovalRequest`], parks the action at
//! [`crate::ActionLifecycleState::ApprovalPending`], and resumes only on
//! `ExecuteAction` with a [`ApprovalBinding`] that has reached
//! [`ApprovalBindingState::Granted`].
//!
//! ## What lives here vs in S5.3 Approval Mechanics service
//!
//! S5.3 owns the eight-state `ApprovalRequestState` FSM (`DRAFT`,
//! `AWAITING_OPERATOR`, `GRANTED`, `DENIED`, `EXPIRED`, `REVOKED`,
//! `CONSUMED`, `FAILED_DELIVERY`). The Approval Mechanics service —
//! independently implemented in a future task — drives that FSM end-to-end
//! including channel selection (§7), TTL discipline (§8), dual control
//! (§12), revocation (§11), and the binding signature ceremony (§5
//! `signer_signature`).
//!
//! The L3 runtime only needs a **projection** of that FSM: it submits a
//! request, awaits a Granted binding, consumes the binding atomically.
//! The runtime never sees `AWAITING_OPERATOR` / `FAILED_DELIVERY` internal
//! states; those are surfaced through the binding's terminal state at
//! consume time.
//!
//! [`ApprovalBindingState`] is therefore a closed 5-variant enum that
//! collapses the FSM onto the surface the runtime cares about:
//! `Pending` (request emitted, not yet decided), `Granted` (binding active,
//! consumable), `Consumed` (binding spent — terminal anti-replay), `Denied`
//! (binding rejected — terminal), `Expired` (TTL elapsed — terminal).
//!
//! ## Anti-replay invariant
//!
//! Per S5.3 §13.1, [`ApprovalBindingState::Granted`] → `Consumed` is the
//! only success transition the runtime drives. The consume operation is
//! atomic on the binding id (the [`crate::approval_sink::ApprovalBindingSink`]
//! trait guarantees this); a second consume attempt observes `Consumed`
//! and is rejected with [`crate::RuntimeError::ApprovalBindingConsumed`].
//!
//! ## Action-revision invariant
//!
//! Per S5.3 §13.2, the binding carries `bound_action_canonical_hash` (the
//! BLAKE3 of the JCS-canonical action envelope at grant time). The runtime
//! recomputes the hash at `ExecuteAction` and the consume call compares;
//! divergence voids the binding with `ACTION_REVISED` and fails closed.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use aios_action::ActionId;
use aios_policy::ApprovalRequirement;

// ---------------------------------------------------------------------------
// ApprovalBindingState — closed 5-variant projection of S5.3 §3.1.
// ---------------------------------------------------------------------------

/// Closed state machine projection the Capability Runtime drives.
///
/// Maps onto S5.3 §3.1's eight-state `ApprovalRequestState` as follows:
///
/// | S5.3 `ApprovalRequestState`            | `ApprovalBindingState` here |
/// | -------------------------------------- | --------------------------- |
/// | `DRAFT` / `AWAITING_OPERATOR`          | [`Self::Pending`]           |
/// | `GRANTED`                              | [`Self::Granted`]           |
/// | `CONSUMED`                             | [`Self::Consumed`]          |
/// | `DENIED` / `FAILED_DELIVERY` / `REVOKED` (pre-consume) | [`Self::Denied`] |
/// | `EXPIRED`                              | [`Self::Expired`]           |
///
/// Only [`Self::Granted`] is consumable; every other state is fail-closed
/// at the consume gate. Terminal states ([`Self::Consumed`], [`Self::Denied`],
/// [`Self::Expired`]) never transition out.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    strum_macros::EnumIter,
    strum_macros::EnumCount,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalBindingState {
    /// Request emitted; no binding has been minted yet (the Approval
    /// Mechanics service has not transitioned to `GRANTED`).
    Pending,
    /// Binding minted and signed; the runtime may consume it once.
    Granted,
    /// Binding spent (terminal). Set atomically by the consume gate.
    /// A second consume against this state fails closed with
    /// [`crate::RuntimeError::ApprovalBindingConsumed`].
    Consumed,
    /// Binding rejected by the operator or voided by S5.3
    /// `ACTION_REVISED` / `SCOPE_DRIFT` / `REVOKED_BY_OPERATOR` /
    /// `FAILED_DELIVERY` (terminal).
    Denied,
    /// TTL elapsed in `Pending` / `Granted` before consume (terminal).
    Expired,
}

impl ApprovalBindingState {
    /// `true` iff this is a terminal state. Terminal states never transition.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Consumed | Self::Denied | Self::Expired)
    }
}

// ---------------------------------------------------------------------------
// ApprovalRequest — the runtime's outbound message to Approval Mechanics.
// ---------------------------------------------------------------------------

/// What the Capability Runtime sends to the Approval Mechanics service when
/// the policy decision is [`aios_policy::Decision::RequireApproval`].
///
/// Mirrors S5.3 §4 `ApprovalRequest` projected onto the runtime's slice:
/// only the fields the runtime knows at emission time are carried; the
/// fields owned by Approval Mechanics (channel selection, delivery
/// timestamps, surface node id) are populated by the service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRequest {
    /// `actrq_<ULID>` runtime request handle (S0.1 §3.2.1). The Approval
    /// Mechanics service mints its own `apprq_<ULID>` id internally; the
    /// runtime threads `request_id` so the eventual binding can be
    /// correlated back to the L3 action.
    pub request_id: String,
    /// The L3 action this approval is bound to.
    pub action_id: ActionId,
    /// The S2.3 §11.2 [`ApprovalRequirement`] projected from the policy
    /// decision. Carries TTL, approver classes, scope, dual-control flag.
    pub requirement: ApprovalRequirement,
    /// Canonical subject id of the action's proposing subject (S5.1).
    pub proposing_subject_id: String,
    /// `true` iff the proposing subject is an AI agent (S5.1 / S2.3 §17 —
    /// the Approval Mechanics service rejects an AI subject as `granting_subject_id`
    /// at signature time, but the request also carries the flag for
    /// channel-selection forensics).
    pub proposing_subject_is_ai: bool,
    /// Frozen request hash (BLAKE3 of JCS-canonical action) at request
    /// emission time. Used by S5.3 §13.2 (`ACTION_REVISED` invariant) at
    /// consume time.
    pub bound_action_canonical_hash: String,
    /// Wall-clock the request was emitted.
    pub requested_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// ApprovalBinding — the durable receipt of operator consent.
// ---------------------------------------------------------------------------

/// Operator consent receipt — the runtime's inbound from Approval Mechanics.
///
/// Mirrors S5.3 §5 `ApprovalBinding`. The runtime treats the binding as a
/// frozen capability: once `state == Granted`, the runtime may consume it
/// once via [`crate::ApprovalBindingSink::consume_binding`]. The consume
/// gate enforces the §13.1 single-use semantics and the §13.2
/// action-revision invariant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalBinding {
    /// `appb_<ULID>` — the binding's S0.1 §3.2.1 id.
    pub binding_id: String,
    /// Backlink to the originating [`ApprovalRequest::request_id`].
    pub request_id: String,
    /// L3 action the binding is consumable for.
    pub action_id: ActionId,
    /// Canonical subject id of the operator who granted (S5.3 §5
    /// `granting_subject_id`). Must be a `SubjectKind = HUMAN_USER`
    /// (S5.3 §14.3 AI self-approval prevention).
    pub granted_by: String,
    /// Subject's [`aios_policy::ApproverClass`] at grant time. Compared
    /// against the requirement's `approver_classes` filter at consume.
    pub granted_by_class: aios_policy::ApproverClass,
    /// Wall-clock at which the binding was minted.
    pub granted_at: DateTime<Utc>,
    /// Wall-clock at which the binding expires; consume after this time
    /// transitions the binding to [`ApprovalBindingState::Expired`].
    pub expires_at: DateTime<Utc>,
    /// Frozen action canonical hash (S5.3 §5 `bound_action_canonical_hash`).
    /// The runtime recomputes at `ExecuteAction` and compares; divergence
    /// triggers `ACTION_REVISED` per §13.2.
    pub bound_action_canonical_hash: String,
    /// Ed25519 signature over the canonical binding fields (S5.3 §5
    /// `signer_signature`). The L3 runtime does not verify the signature
    /// itself — verification is the Approval Mechanics service's job
    /// before transitioning to `GRANTED`. The runtime carries the bytes so
    /// evidence emissions and audit replays can reproduce the trust
    /// surface.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signature_ed25519: Vec<u8>,
    /// Current state.
    pub state: ApprovalBindingState,
}

impl ApprovalBinding {
    /// `true` iff this binding can be consumed right now.
    ///
    /// Returns `true` only when `state == Granted`. Every other state is
    /// fail-closed by construction (Pending = not yet decided; Consumed /
    /// Denied / Expired = terminal).
    #[must_use]
    pub const fn is_consumable(&self) -> bool {
        matches!(self.state, ApprovalBindingState::Granted)
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;

    fn fixture_binding(state: ApprovalBindingState) -> ApprovalBinding {
        ApprovalBinding {
            binding_id: "appb_0123456789abcdef0123456789".to_string(),
            request_id: "actrq_0123456789abcdef0123456789".to_string(),
            action_id: ActionId::new(),
            granted_by: "human:lucky:01HX0000000000000000000000".to_string(),
            granted_by_class: aios_policy::ApproverClass::Human,
            granted_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(300),
            bound_action_canonical_hash: "a".repeat(32),
            signature_ed25519: vec![0xAB, 0xCD, 0xEF],
            state,
        }
    }

    #[test]
    fn is_consumable_iff_state_is_granted() {
        assert!(fixture_binding(ApprovalBindingState::Granted).is_consumable());
        assert!(!fixture_binding(ApprovalBindingState::Pending).is_consumable());
        assert!(!fixture_binding(ApprovalBindingState::Consumed).is_consumable());
        assert!(!fixture_binding(ApprovalBindingState::Denied).is_consumable());
        assert!(!fixture_binding(ApprovalBindingState::Expired).is_consumable());
    }

    #[test]
    fn terminal_states_are_terminal() {
        assert!(!ApprovalBindingState::Pending.is_terminal());
        assert!(!ApprovalBindingState::Granted.is_terminal());
        assert!(ApprovalBindingState::Consumed.is_terminal());
        assert!(ApprovalBindingState::Denied.is_terminal());
        assert!(ApprovalBindingState::Expired.is_terminal());
    }

    #[test]
    fn binding_roundtrips_through_serde_json() {
        let original = fixture_binding(ApprovalBindingState::Granted);
        let s = serde_json::to_string(&original).expect("serialize");
        let restored: ApprovalBinding = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(restored, original);
        assert_eq!(restored.signature_ed25519, vec![0xAB, 0xCD, 0xEF]);
    }
}
