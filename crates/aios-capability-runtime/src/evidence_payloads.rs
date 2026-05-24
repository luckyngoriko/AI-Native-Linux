//! Typed payload structs for the L3 evidence emissions (S10.1 ↔ S3.1).
//!
//! Per S3.1 §29.6 the canonical per-`RecordType` payload schemas are queued
//! for Wave 14+ — the closed `RecordPayload` `oneof` in the spec proto still
//! carries 22 variants while the `RecordType` enum lists 427 entries. The
//! capability runtime emits a typed, JSON-round-trippable struct for each
//! pipeline transition so the receipt's opaque payload is structurally
//! recognisable even before the proto `RecordPayload` catches up.
//!
//! ## Discipline
//!
//! - Every struct implements [`serde::Serialize`] + [`serde::Deserialize`]
//!   and is exhaustively round-tripped by the integration tests
//!   (`tests/evidence_emission.rs`).
//! - Every field is documented inline with the S10.1 / S3.1 anchor the
//!   field projects from. New fields are a versioned spec change.
//! - Secret-shaped fields are forbidden by construction (INV-015 — evidence
//!   never contains secrets). The structs only carry typed identifiers,
//!   closed-enum tokens, and booleans / timestamps — never raw secret
//!   material. The S3.1 §14 redaction profile is applied at seal time
//!   regardless.
//! - **No defaults.** Every payload field is filled at emission time. If a
//!   value is genuinely unknown (e.g. `adapter_id` before lookup),
//!   the field is typed `Option<...>` and the caller passes `None`
//!   explicitly — silent defaulting is an evidence-falsification risk.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::dispatch::{ActionDispatchKind, QueueClass};
use crate::failure::{ExecutionFailureReason, RollbackOutcome};
use crate::status::ActionLifecycleState;

// ---------------------------------------------------------------------------
// ACTION_RECEIVED (S3.1 §4 ID 1).
// ---------------------------------------------------------------------------

/// Payload for `RecordType::ActionReceived` — S3.1 §4 / S10.1 §13.
///
/// Emitted on the §6.1 step 0 `ValidateAction` success transition
/// `CREATED → POLICY_PENDING` (T2). Per the spec golden path (§22 lines
/// 1059..1061) the runtime emits this immediately after envelope schema
/// validation succeeds.
///
/// The payload carries the typed shape of the validated envelope at receipt
/// time so a downstream auditor can reconstruct the runtime's view without
/// re-loading the action store.
///
/// `queue_class_initial` is the *seeded* class (from [`crate::fresh_context`])
/// — it is **not** the final enrolled class. The §11.4 AI-interactive
/// downgrade fires later in the pipeline and emits its own marker record.
/// Folding the queue selection into this payload would conflate two
/// transitions and prevent T-029's downgrade marker from being independently
/// auditable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionReceivedPayload {
    /// Envelope-declared `action_kind` (e.g. `service.restart`).
    pub action_kind: String,
    /// Caller's canonical subject id (S5.1 grammar).
    pub subject_canonical_id: String,
    /// `true` iff the envelope's `identity.is_ai` flag is set.
    pub is_ai: bool,
    /// Wall-clock at which the runtime received the envelope.
    pub received_at: DateTime<Utc>,
    /// Lifecycle state the runtime moved into after validation (`POLICY_PENDING`
    /// in the happy path; the `FAILED` branch is covered by a separate
    /// `EXECUTION_FAILED` / `LIFECYCLE` record under T-032's forensic surface).
    pub lifecycle_state_after: ActionLifecycleState,
}

// ---------------------------------------------------------------------------
// POLICY_DECISION (S3.1 §4 ID 4).
// ---------------------------------------------------------------------------

/// Payload for `RecordType::PolicyDecision` — S3.1 §4 / S10.1 §13.
///
/// Emitted after pipeline step 2 (`EvaluatePolicyForAction`) completes. The
/// `policy_decision_id` is the `poldec_<ULID>` minted by the Policy Kernel
/// (S2.3 §4 field 1) and links this evidence record to the kernel's
/// decision row.
///
/// The S3.1 §4 record name is the same in the S10.1 §22 narrative
/// (`ACTION_POLICY_DECISION` is the §13 sub-spec alias; the canonical name
/// is `POLICY_DECISION` at ID 4 — DEC-051 collapses synonyms onto the
/// earlier ID).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyDecisionPayload {
    /// `poldec_<ULID>` (S2.3 §4 field 1).
    pub policy_decision_id: String,
    /// Decision verdict as wire token (`ALLOW` / `REQUIRE_APPROVAL` /
    /// `DENY` — `UNSPECIFIED` is reserved and never emitted here).
    pub decision: String,
    /// Reason code (`ScopedAllow`, `HardDeny`, …).
    pub reason_code: String,
    /// Policy bundle version (S2.3 §4 field 4).
    pub bundle_version: String,
    /// Lifecycle state after the transition (`APPROVED`, `APPROVAL_PENDING`,
    /// `POLICY_DENIED`, or `OVERRIDE_PENDING`).
    pub lifecycle_state_after: ActionLifecycleState,
}

// ---------------------------------------------------------------------------
// ROUTING_DECISION (S3.1 §4 ID 3).
// ---------------------------------------------------------------------------

/// Payload for `RecordType::RoutingDecision` — S3.1 §4 / S10.1 §13.
///
/// Emitted after step 5 (`ExecuteAction`) resolves the adapter and the
/// dispatcher picks the [`ActionDispatchKind`] per the §3.2 closed table.
/// The Wave-13 emitter attribution sheet credits this `RecordType` to
/// S10.1 (capability runtime), not to S13.2 — the model-routing flavour
/// is a separate concern that ships its own `MODEL_INVOCATION_*` records
/// (IDs 248..=259).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutingDecisionPayload {
    /// The chosen adapter's `adapter:<ULID>` id (§10.1 grammar).
    pub adapter_id: String,
    /// Adapter-declared `action_kind` the envelope was routed to.
    pub action_kind: String,
    /// The decided dispatch kind (§3.2 closed table).
    pub dispatch_kind: ActionDispatchKind,
}

// ---------------------------------------------------------------------------
// ACTION_QUEUED — folded into ACTION_RECEIVED extended payload per the
// brief's STOP-condition. S3.1 §4 does NOT enumerate ACTION_QUEUED (no
// such RecordType exists at any ID in the 1..=427 vocabulary). The
// queue-class selection is instead surfaced via the dedicated
// AI_INTERACTIVE_QUEUE_DOWNGRADE marker (ID 129) when applicable, plus
// the queue_class field on the routing payload below.
// ---------------------------------------------------------------------------

/// Payload for the "action queued" transition (T12 — APPROVED → QUEUED).
///
/// **Spec deviation note.** The §13 sub-spec lists `ACTION_QUEUED` among
/// the 20 `RecordType`s queued for S3.1 addition, but the actual S3.1
/// vocabulary (Wave 13, DEC-051) does not include it. Per the T-031 brief
/// STOP-condition, the runtime emits this payload under
/// `RecordType::ActionDispatched` (S3.1 §4 ID 116, the closest
/// spec-pinned dispatch-side record) with `dispatched=false` marking the
/// queue-enrolment phase as distinct from the actual dispatch in
/// `EXECUTION_STARTED` below. The two transitions are independent: queue
/// enrolment can succeed while dispatch later fails closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionQueuedPayload {
    /// The selected queue class (§3.5).
    pub queue_class: QueueClass,
    /// `true` once the action actually leaves the queue for an adapter
    /// (the `ACTION_DISPATCHED` canonical semantic). When emitted by
    /// `emit_action_queued`, this is always `false` — the dispatch event
    /// is recorded separately via `EXECUTION_STARTED`.
    pub dispatched: bool,
    /// Subject id for forensic correlation against the queue depth gauge.
    pub subject_canonical_id: String,
}

// ---------------------------------------------------------------------------
// EXECUTION_STARTED (S3.1 §4 ID 8).
// ---------------------------------------------------------------------------

/// Payload for `RecordType::ExecutionStarted` — S3.1 §4 / S10.1 §13.
///
/// Emitted at the §4.2 T13 transition `QUEUED → EXECUTING`, i.e. the moment
/// the dispatcher hands the envelope to the adapter handle. The adapter
/// has not yet returned; that's `EXECUTION_COMPLETED`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionStartedPayload {
    /// The decided dispatch kind (§3.2).
    pub dispatch_kind: ActionDispatchKind,
    /// The queue class the action was enrolled under (§3.5).
    pub queue_class: QueueClass,
}

// ---------------------------------------------------------------------------
// EXECUTION_COMPLETED (S3.1 §4 ID 9).
// ---------------------------------------------------------------------------

/// Payload for `RecordType::ExecutionCompleted` — S3.1 §4 / S10.1 §13.
///
/// Emitted after the adapter handle returns (success or failure). The
/// `outcome` token is the closed `AdapterResult` shape from §6.3
/// (`ADAPTER_OK` / `ADAPTER_FAILED` / `ADAPTER_PANICKED` / `ADAPTER_TIMED_OUT`).
/// The payload itself does not carry the adapter's free-form result body
/// — INV-015 (evidence never contains secrets) forbids it, and the
/// verification record below carries the typed outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionCompletedPayload {
    /// Closed token from the §6.3 `AdapterResult` shape.
    pub outcome: String,
    /// Lifecycle state after the transition (`VERIFYING` on success;
    /// `FAILED` on adapter failure).
    pub lifecycle_state_after: ActionLifecycleState,
}

// ---------------------------------------------------------------------------
// VERIFICATION_RESULT (S3.1 §4 ID 10).
// ---------------------------------------------------------------------------

/// Payload for `RecordType::VerificationResult` — S3.1 §4 / S10.1 §13.
///
/// Emitted after the verification engine runs against the envelope's
/// `verification_intent` (S2.4 / S10.1 §7.1). The `passed` flag drives
/// the §4.2 T17 (`SUCCEEDED`) / T18 (`FAILED`) branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationResultPayload {
    /// `true` iff every declared verification intent passed.
    pub passed: bool,
    /// Lifecycle state after the transition (`SUCCEEDED` or `FAILED`).
    pub lifecycle_state_after: ActionLifecycleState,
}

// ---------------------------------------------------------------------------
// ROLLBACK_COMPLETED (S3.1 §4 ID 11).
// ---------------------------------------------------------------------------

/// Payload for `RecordType::RollbackCompleted` — S3.1 §4 / S10.1 §13.
///
/// Emitted after the rollback FSM (T-032) reaches a terminal state. The
/// `outcome` is the closed §3.7 enum; the `ROLLBACK_FAILED` outcome
/// additionally triggers the operator alert per §7.4 (out of scope for
/// T-031).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RollbackCompletedPayload {
    /// The closed §3.7 rollback outcome.
    pub outcome: RollbackOutcome,
    /// The §3.6 reason that caused the rollback to be attempted.
    /// `None` only if rollback is being driven for a non-execution
    /// failure (verification timeout, etc.) — kept `Option` so the
    /// payload shape doesn't fork between paths.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triggering_reason: Option<ExecutionFailureReason>,
    /// Lifecycle state after the transition (`ROLLED_BACK` or
    /// `ROLLBACK_FAILED`).
    pub lifecycle_state_after: ActionLifecycleState,
}

// ---------------------------------------------------------------------------
// AI_INTERACTIVE_QUEUE_DOWNGRADE (S3.1 §25.2 ID 129).
// ---------------------------------------------------------------------------

/// Payload for `RecordType::AiInteractiveQueueDowngrade` — S3.1 §25.2 /
/// S10.1 §11.4.
///
/// Emitted when an AI subject's submission is silently downgraded from
/// `INTERACTIVE` to `AGENT_PROPOSAL`. The downgrade is silent at the
/// action level (no failure) but loud at the audit level — every
/// downgrade produces one of these records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiInteractiveQueueDowngradePayload {
    /// AI subject id that triggered the downgrade.
    pub subject_canonical_id: String,
    /// The queue class the subject originally requested (`INTERACTIVE`).
    pub original_queue_class: QueueClass,
    /// The queue class the runtime enrolled the action under after the
    /// §11.4 downgrade (`AGENT_PROPOSAL`).
    pub effective_queue_class: QueueClass,
}

// ---------------------------------------------------------------------------
// APPROVAL_REQUESTED (S3.1 §4 ID 5).
// ---------------------------------------------------------------------------

/// Payload for `RecordType::ApprovalRequested` — S10.1 §6 ↔ S5.3 §10.1.
///
/// Emitted by pipeline step 3 (`step_request_approval`) when the policy
/// decision was `REQUIRE_APPROVAL` and an [`crate::ApprovalBindingSink`]
/// is wired. Carries the runtime's frozen view of the request handed to
/// the Approval Mechanics service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRequestedPayload {
    /// Approval request handle (`actrq_<ULID>`).
    pub approval_request_id: String,
    /// Proposing subject canonical id (S5.1).
    pub proposing_subject_id: String,
    /// `true` iff the proposing subject is AI.
    pub proposing_subject_is_ai: bool,
    /// Approval TTL in seconds (from `ApprovalRequirement.ttl_seconds`).
    pub ttl_seconds: u32,
    /// `true` iff the policy demands a second human co-signer (S5.3 §12).
    pub require_human_co_signer: bool,
    /// Lifecycle state the runtime moved into (always `APPROVAL_PENDING`).
    pub lifecycle_state_after: ActionLifecycleState,
}

// ---------------------------------------------------------------------------
// APPROVAL_GRANTED (S3.1 §4 ID 6).
// ---------------------------------------------------------------------------

/// Payload for `RecordType::ApprovalGranted` — S10.1 §6 ↔ S5.3 §10.1.
///
/// Emitted when `ExecuteAction` successfully consumes a `Granted` binding
/// and the action proceeds past `APPROVAL_PENDING`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalGrantedPayload {
    /// Approval request handle.
    pub approval_request_id: String,
    /// Binding handle (`appb_<ULID>`).
    pub binding_id: String,
    /// Canonical subject id of the granting operator.
    pub granting_subject_id: String,
    /// Frozen action canonical hash (S5.3 §5).
    pub bound_action_canonical_hash: String,
    /// Lifecycle state the runtime moved into (always `APPROVED`).
    pub lifecycle_state_after: ActionLifecycleState,
}

// ---------------------------------------------------------------------------
// APPROVAL_DENIED (S3.1 §4 ID 7).
// ---------------------------------------------------------------------------

/// Payload for `RecordType::ApprovalDenied` — S10.1 §6 ↔ S5.3 §10.1.
///
/// Emitted when an `ExecuteAction` fails the consume gate (invalid /
/// consumed / expired binding, approver-class mismatch, AI self-approval
/// defense-in-depth). The runtime fails the action closed and emits this
/// receipt; the binding's authoritative FSM record lives in the Approval
/// Mechanics service evidence stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalDeniedPayload {
    /// Approval request handle, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_request_id: Option<String>,
    /// Binding handle that was attempted, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_id: Option<String>,
    /// Closed denial reason code — one of `BINDING_INVALID`,
    /// `BINDING_CONSUMED`, `BINDING_EXPIRED`, `APPROVER_CLASS_MISMATCH`,
    /// `AI_SELF_APPROVAL_BLOCKED`.
    pub reason_code: String,
    /// English plain-text detail; never contains secrets (INV-015).
    pub reason_message: String,
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 24, 12, 0, 0)
            .single()
            .expect("fixed wall-clock")
    }

    #[test]
    fn action_received_round_trips_through_json() {
        let p = ActionReceivedPayload {
            action_kind: "service.restart".into(),
            subject_canonical_id: "human:lucky".into(),
            is_ai: false,
            received_at: fixed_now(),
            lifecycle_state_after: ActionLifecycleState::PolicyPending,
        };
        let s = serde_json::to_string(&p).expect("ser");
        let back: ActionReceivedPayload = serde_json::from_str(&s).expect("de");
        assert_eq!(back, p);
    }

    #[test]
    fn policy_decision_round_trips_through_json() {
        let p = PolicyDecisionPayload {
            policy_decision_id: "poldec_01HX0000000000000000000000".into(),
            decision: "ALLOW".into(),
            reason_code: "ScopedAllow".into(),
            bundle_version: "polb_v1".into(),
            lifecycle_state_after: ActionLifecycleState::Approved,
        };
        let s = serde_json::to_string(&p).expect("ser");
        let back: PolicyDecisionPayload = serde_json::from_str(&s).expect("de");
        assert_eq!(back, p);
    }

    #[test]
    fn routing_decision_round_trips_through_json() {
        let p = RoutingDecisionPayload {
            adapter_id: "adapter:01HX0000000000000000000000".into(),
            action_kind: "service.restart".into(),
            dispatch_kind: ActionDispatchKind::IsolatedSandbox,
        };
        let s = serde_json::to_string(&p).expect("ser");
        let back: RoutingDecisionPayload = serde_json::from_str(&s).expect("de");
        assert_eq!(back, p);
    }

    #[test]
    fn action_queued_round_trips_through_json() {
        let p = ActionQueuedPayload {
            queue_class: QueueClass::AgentProposal,
            dispatched: false,
            subject_canonical_id: "ai:agent-1".into(),
        };
        let s = serde_json::to_string(&p).expect("ser");
        let back: ActionQueuedPayload = serde_json::from_str(&s).expect("de");
        assert_eq!(back, p);
    }

    #[test]
    fn execution_started_round_trips_through_json() {
        let p = ExecutionStartedPayload {
            dispatch_kind: ActionDispatchKind::SubprocessFork,
            queue_class: QueueClass::Interactive,
        };
        let s = serde_json::to_string(&p).expect("ser");
        let back: ExecutionStartedPayload = serde_json::from_str(&s).expect("de");
        assert_eq!(back, p);
    }

    #[test]
    fn execution_completed_round_trips_through_json() {
        let p = ExecutionCompletedPayload {
            outcome: "ADAPTER_OK".into(),
            lifecycle_state_after: ActionLifecycleState::Verifying,
        };
        let s = serde_json::to_string(&p).expect("ser");
        let back: ExecutionCompletedPayload = serde_json::from_str(&s).expect("de");
        assert_eq!(back, p);
    }

    #[test]
    fn verification_result_round_trips_through_json() {
        let p = VerificationResultPayload {
            passed: true,
            lifecycle_state_after: ActionLifecycleState::Succeeded,
        };
        let s = serde_json::to_string(&p).expect("ser");
        let back: VerificationResultPayload = serde_json::from_str(&s).expect("de");
        assert_eq!(back, p);
    }

    #[test]
    fn rollback_completed_round_trips_through_json() {
        let p = RollbackCompletedPayload {
            outcome: RollbackOutcome::Succeeded,
            triggering_reason: Some(ExecutionFailureReason::AdapterRefused),
            lifecycle_state_after: ActionLifecycleState::RolledBack,
        };
        let s = serde_json::to_string(&p).expect("ser");
        let back: RollbackCompletedPayload = serde_json::from_str(&s).expect("de");
        assert_eq!(back, p);
    }

    #[test]
    fn ai_interactive_queue_downgrade_round_trips_through_json() {
        let p = AiInteractiveQueueDowngradePayload {
            subject_canonical_id: "ai:agent-1".into(),
            original_queue_class: QueueClass::Interactive,
            effective_queue_class: QueueClass::AgentProposal,
        };
        let s = serde_json::to_string(&p).expect("ser");
        let back: AiInteractiveQueueDowngradePayload = serde_json::from_str(&s).expect("de");
        assert_eq!(back, p);
    }

    #[test]
    fn approval_requested_round_trips_through_json() {
        let p = ApprovalRequestedPayload {
            approval_request_id: "actrq_01HX0000000000000000000000".into(),
            proposing_subject_id: "ai:agent-1".into(),
            proposing_subject_is_ai: true,
            ttl_seconds: 300,
            require_human_co_signer: false,
            lifecycle_state_after: ActionLifecycleState::ApprovalPending,
        };
        let s = serde_json::to_string(&p).expect("ser");
        let back: ApprovalRequestedPayload = serde_json::from_str(&s).expect("de");
        assert_eq!(back, p);
    }

    #[test]
    fn approval_granted_round_trips_through_json() {
        let p = ApprovalGrantedPayload {
            approval_request_id: "actrq_01HX0000000000000000000000".into(),
            binding_id: "appb_01HX0000000000000000000000".into(),
            granting_subject_id: "human:lucky".into(),
            bound_action_canonical_hash: "a".repeat(32),
            lifecycle_state_after: ActionLifecycleState::Approved,
        };
        let s = serde_json::to_string(&p).expect("ser");
        let back: ApprovalGrantedPayload = serde_json::from_str(&s).expect("de");
        assert_eq!(back, p);
    }

    #[test]
    fn approval_denied_round_trips_through_json() {
        let p = ApprovalDeniedPayload {
            approval_request_id: Some("actrq_01HX0000000000000000000000".into()),
            binding_id: Some("appb_01HX0000000000000000000000".into()),
            reason_code: "BINDING_CONSUMED".into(),
            reason_message: "binding already consumed".into(),
        };
        let s = serde_json::to_string(&p).expect("ser");
        let back: ApprovalDeniedPayload = serde_json::from_str(&s).expect("de");
        assert_eq!(back, p);
    }
}
