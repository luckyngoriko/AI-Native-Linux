//! Per-caller privacy ceiling on the Query / Subscribe paths (T-014, S3.1 §10 + §23.2).
//!
//! ## Spec source of truth
//!
//! - **§10 Query API.** "Privacy ceiling applies (S2.1 §5 pattern): receipts
//!   whose payload references objects above the caller's ceiling are silently
//!   filtered with a count returned in the stream trailer."
//! - **§23.2 Namespace extension.** "A subject with `primary_group_id = A`
//!   cannot retrieve records with `namespace_scope.group_id = B` unless the
//!   subject is in the `_system` scope under recovery mode with
//!   `system_audit_read` capability and a human approver. Excluded records
//!   are silently filtered with a `suppressed_count` field."
//! - **§11.4 Recovery mode** broadens read access to `FOREVER` retention
//!   forensic records for the operator subject.
//!
//! ## Implemented model (T-014 — minimum viable)
//!
//! The full S5.1 `NormalizedSubject` + S4.1 `NamespaceScope` wiring is deferred to
//! a future task (T-016 / M3). Until then, the privacy ceiling is computed
//! from a **caller projection**:
//!
//! ```text
//! PrivacyCeiling {
//!     subject_canonical_id,   // who is reading
//!     primary_group,          // their S5.1 group (None = no group)
//!     is_ai,                  // AI agents get a stricter ceiling
//!     is_recovery_mode,       // broadens to FOREVER receipts
//! }
//! ```
//!
//! and a **receipt projection** restricted to what the T-007..T-013 receipt
//! envelope actually carries: `subject_canonical_id` + `record_type` + the
//! retention class implied by the record type.
//!
//! Until the receipt envelope grows a typed `namespace_scope` field (Wave
//! 14+), we approximate "is this a system / constitutionally public record"
//! via a closed predicate over `RecordType` — see
//! [`is_constitutionally_public_record_type`]. This is the same set the spec
//! describes in §13 (FOREVER retention bucket for constitutional events).
//!
//! ## Rules implemented
//!
//! The [`PrivacyCeiling::admits`] predicate is a closed `match` over four
//! cases, in priority order:
//!
//! 1. **System caller / empty subject = admit all.** The empty caller subject
//!    is the **test-only shortcut** documented at the gRPC layer (it preserves
//!    the T-007..T-013 wire-baseline tests). Production callers MUST set a
//!    non-empty `subject_canonical_id` — a future task will harden this into
//!    a hard reject.
//! 2. **Self-receipt always admitted.** A caller can always read receipts they
//!    are the subject of. (`receipt.subject_canonical_id == caller.subject_canonical_id`).
//! 3. **Constitutionally-public record types always admitted.** Policy
//!    decisions, recovery events, tamper events, segment seals, chain
//!    checkpoints, capability runtime audit records, first-boot, override
//!    grants, bundle loads — these are operator-visible by spec §13 FOREVER
//!    retention and §11.4 recovery semantics.
//! 4. **Group-scoped admission.** If the caller declares `primary_group =
//!    Some(g)` and the receipt's `subject_canonical_id` belongs to the same
//!    group (per the same grammar — see [`subject_primary_group`]), admit.
//!    Cross-group denial is silent (§23.2).
//! 5. **AI strictness.** AI callers (`is_ai = true`) cannot see *other* AI
//!    subjects' receipts even if they share a group, unless the record is
//!    constitutionally public. AI callers can ONLY see (a) their own receipts
//!    and (b) constitutionally-public records about themselves or the system.
//!    This implements the §11.4 / S5.1 AI-isolation rule.
//! 6. **Recovery-mode broadening.** When `is_recovery_mode = true`, admission
//!    additionally accepts every receipt whose record type maps to
//!    `RetentionClass::Forever` — this is the §11.4 forensic-access path.
//!    Recovery mode does NOT override AI strictness: an AI agent in recovery
//!    mode still cannot read another AI's non-public records.
//!
//! ## Carry-forward
//!
//! - **§23.2 `group_id` check on receipts:** the receipt envelope does not yet
//!   carry an explicit `namespace_scope`. T-014's group check is computed
//!   from the subject canonical id grammar (see [`subject_primary_group`]).
//!   When the `namespace_scope` field is added (Wave 14+), the predicate will
//!   prefer it.
//! - **`system_audit_read` capability:** the §23.2 explicit capability check
//!   is deferred until S5.2 Vault Broker integration lands. Until then,
//!   `is_recovery_mode = true` is the gating flag for cross-group forensic
//!   reads of FOREVER records.

use crate::receipt::EvidenceReceipt;
use crate::record::{RecordType, RetentionClass};

/// Per-caller privacy ceiling derived from S3.1 §10 + §23.2 + §11.4.
///
/// Construct via [`PrivacyCeiling::from_caller`] at the start of a `Query` /
/// `Subscribe` RPC. The struct is `Send + Sync` and cheap to clone, so it can
/// be moved into a stream's `filter_map` closure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacyCeiling {
    subject_canonical_id: String,
    primary_group: Option<String>,
    is_ai: bool,
    is_recovery_mode: bool,
}

impl PrivacyCeiling {
    /// Build a ceiling from a caller projection.
    ///
    /// `subject_canonical_id` is the S5.1 `NormalizedSubject` canonical id of
    /// the caller (e.g. `human:operator-1`, `service:capability-runtime`,
    /// `ai:agent-7`, `_system:service:audit`). An **empty** value selects the
    /// test-only "system caller" path that admits every receipt.
    ///
    /// `primary_group` is the caller's S5.1 primary group id (`None` = the
    /// caller declares no group; treated as cross-group-blind).
    ///
    /// `is_ai` enables the §11.4 / S5.1 AI-isolation rule.
    ///
    /// `is_recovery_mode` enables the §11.4 broadened forensic ceiling on
    /// FOREVER-retention records.
    #[must_use]
    pub fn from_caller(
        subject_canonical_id: impl Into<String>,
        primary_group: Option<String>,
        is_ai: bool,
        is_recovery_mode: bool,
    ) -> Self {
        Self {
            subject_canonical_id: subject_canonical_id.into(),
            primary_group,
            is_ai,
            is_recovery_mode,
        }
    }

    /// Construct the "system caller" ceiling that admits everything.
    ///
    /// Internal helper for backward compatibility with the T-007..T-013 test
    /// suite. Equivalent to `from_caller("", None, false, false)`.
    #[must_use]
    pub const fn system_caller() -> Self {
        Self {
            subject_canonical_id: String::new(),
            primary_group: None,
            is_ai: false,
            is_recovery_mode: false,
        }
    }

    /// True when this ceiling is the (test-only) system-caller bypass.
    #[must_use]
    pub const fn is_system_caller(&self) -> bool {
        self.subject_canonical_id.is_empty()
    }

    /// The caller's canonical subject id (empty for system caller).
    #[must_use]
    pub fn subject_canonical_id(&self) -> &str {
        &self.subject_canonical_id
    }

    /// The caller's declared primary group, if any.
    #[must_use]
    pub fn primary_group(&self) -> Option<&str> {
        self.primary_group.as_deref()
    }

    /// True when this caller is an AI agent (stricter ceiling per §11.4).
    #[must_use]
    pub const fn is_ai(&self) -> bool {
        self.is_ai
    }

    /// True when this caller is reading under recovery mode (broadened ceiling
    /// for FOREVER-retention forensic records per §11.4).
    #[must_use]
    pub const fn is_recovery_mode(&self) -> bool {
        self.is_recovery_mode
    }

    /// Wire-form variant of [`PrivacyCeiling::admits`] usable from a live
    /// broadcast stream where the sealed [`EvidenceReceipt`] is not at hand.
    ///
    /// Takes the (`subject_canonical_id`, `record_type`) projection that
    /// `admits` would have computed from a sealed receipt. The semantics
    /// are identical — used by the `Subscribe` path because broadcast
    /// payloads carry only the wire-form proto receipt.
    #[must_use]
    pub fn admits_wire(&self, receipt_subject: &str, record_type: RecordType) -> bool {
        // Rule 1 — system caller / empty subject = admit all.
        if self.is_system_caller() {
            return true;
        }
        // Rule 2 — self-receipt.
        if receipt_subject == self.subject_canonical_id {
            return true;
        }
        let is_public_record = is_constitutionally_public_record_type(record_type);
        // Rule 3 — public records to non-AI callers.
        if is_public_record && !self.is_ai {
            return true;
        }
        // Rule 5 — AI strictness.
        if self.is_ai {
            if is_public_record && receipt_subject_is_system(receipt_subject) {
                return true;
            }
            return false;
        }
        // Rule 4 — group-scoped admission.
        if let Some(caller_group) = self.primary_group.as_deref() {
            if let Some(receipt_group) = subject_primary_group(receipt_subject) {
                if receipt_group == caller_group {
                    return true;
                }
            }
        }
        // Rule 6 — recovery-mode broadening on FOREVER records.
        if self.is_recovery_mode && record_type.retention_class() == RetentionClass::Forever {
            return true;
        }
        false
    }

    /// Closed predicate: may this caller read this receipt?
    ///
    /// Implements the §10 + §23.2 + §11.4 admission rules described in the
    /// module doc. Returns `true` for "admit" / `false` for "silently filter
    /// and increment the `suppressed_count` trailer counter".
    #[must_use]
    pub fn admits(&self, receipt: &EvidenceReceipt) -> bool {
        self.admits_wire(receipt.subject_canonical_id(), receipt.record_type())
    }
}

// ─────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────

/// Closed predicate over [`RecordType`]: is this record type
/// **constitutionally public** (operator-visible by spec without group scope)?
///
/// The set mirrors S3.1 §13 FOREVER-retention constitutional events plus the
/// system-audit families that have no per-subject privacy at all:
///
/// - Policy decisions, approvals, override grants, bundle loads.
/// - Recovery events, recovery boot / operator-authenticated, recovery exits.
/// - Tamper / chain inconsistency / segment seal / chain checkpoint.
/// - First-boot, factory-reset.
/// - Identity / invariant / capability / sandbox bundle loads/rejections.
/// - Cross-group access denial and `system_admin_operation` (S4.1 §23.2).
///
/// These records are emitted by the system itself and are part of the
/// constitutional audit chain — every operator can see them, but AI agents
/// remain isolated unless the record's subject is the system or themselves
/// (see [`PrivacyCeiling::admits`] Rule 5).
#[must_use]
pub const fn is_constitutionally_public_record_type(rt: RecordType) -> bool {
    matches!(
        rt,
        // Original §4 constitutional events
        RecordType::PolicyDecision
            | RecordType::ApprovalRequested
            | RecordType::ApprovalGranted
            | RecordType::ApprovalDenied
            | RecordType::RecoveryEvent
            | RecordType::ChainCheckpoint
            | RecordType::SegmentSealed
            | RecordType::ChainInconsistencyDetected
            | RecordType::TamperDetected
            | RecordType::EmergencyOverrideGrant
            | RecordType::PolicyBundleLoad
            // §23 namespace integration
            | RecordType::SystemAdminOperation
            | RecordType::CrossGroupAccessDenied
            // Recovery boundary (Wave 6 §25.5)
            | RecordType::RecoveryBootEntered
            | RecordType::RecoveryOperatorAuthenticated
            | RecordType::RecoveryOperationPerformed
            | RecordType::RecoveryTtlExpiredAutoReboot
            | RecordType::RecoveryBootExited
            | RecordType::RecoveryL5StartBlocked
            | RecordType::RecoveryForensicAttachPerformed
            | RecordType::BootFailureAutoRecoveryTriggered
            // First-boot (Wave 8 §27.2 + Wave 9 W9-B)
            | RecordType::FirstBootStarted
            | RecordType::FirstBootStageCompleted
            | RecordType::FirstBootFailed
            | RecordType::FirstBootComplete
            | RecordType::ResetToFactoryInitiated
            | RecordType::FirstBootOperation
            // Bundle loads / rejections (Wave 10 §28)
            | RecordType::IdentityBundleLoaded
            | RecordType::InvariantBundleLoaded
            | RecordType::InvariantBundleRejected
            | RecordType::PolicyBundleRejected
            | RecordType::IdentityBundleRejected
            | RecordType::CapabilityBundleRejected
            | RecordType::SandboxBundleRejected
            // Override mechanics (Wave 6 §25.4)
            | RecordType::OverrideRequested
            | RecordType::OverrideQuorumReceived
            | RecordType::OverrideGranted
            | RecordType::OverrideConsumed
            | RecordType::OverrideDenied
            | RecordType::OverrideExpired
            | RecordType::OverrideRevoked
            | RecordType::OverrideReview
    )
}

/// Extract the **primary group token** from a subject canonical id, per the
/// rev.2 provisional grammar.
///
/// The full S5.1 grammar (`<type>:<name>[/<sub_id>]`) does not yet carry an
/// explicit group token, so T-014 uses a convention: if the canonical id has
/// the form `<type>:<group>/<name>` (e.g. `human:ops/alice`,
/// `ai:research-team/agent-7`), the segment between the first `:` and the
/// first `/` is treated as the group. Forms without a `/` (e.g.
/// `human:alice`) are considered to declare **no group** (returns `None`).
///
/// When S5.1 lands a typed `primary_group_id` on the canonical-subject
/// envelope, this helper will switch to reading it directly.
#[must_use]
pub fn subject_primary_group(subject_canonical_id: &str) -> Option<&str> {
    let after_kind = subject_canonical_id.split_once(':')?.1;
    let (group, _rest) = after_kind.split_once('/')?;
    if group.is_empty() {
        None
    } else {
        Some(group)
    }
}

/// True when the given subject canonical id designates a `_system:*` actor.
///
/// System actors are services, daemons, and audit subjects. Per S5.1, the
/// leading underscore is reserved for system subjects whose receipts are
/// public.
#[must_use]
pub fn receipt_subject_is_system(subject_canonical_id: &str) -> bool {
    subject_canonical_id.starts_with("_system")
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use crate::receipt::ReceiptBuilder;
    use crate::record::RetentionClass;

    fn mint_receipt(subject: &str, rt: RecordType) -> EvidenceReceipt {
        ReceiptBuilder::new(rt, RetentionClass::Standard24M, subject)
            .seal(None)
            .expect("seal")
    }

    // ─── system caller (test-only bypass) ──────────────────────────────

    #[test]
    fn system_caller_admits_everything() {
        let ceiling = PrivacyCeiling::system_caller();
        assert!(ceiling.is_system_caller());
        for (subj, rt) in [
            ("human:operator-1", RecordType::ActionReceived),
            ("ai:agent-x", RecordType::ModelCall),
            ("_system:service:audit", RecordType::PolicyDecision),
            ("human:ops/alice", RecordType::ExecutionCompleted),
        ] {
            let r = mint_receipt(subj, rt);
            assert!(
                ceiling.admits(&r),
                "system caller must admit {subj} / {rt:?}"
            );
        }
    }

    // ─── self-receipt admission ────────────────────────────────────────

    #[test]
    fn self_receipt_always_admitted() {
        let ceiling =
            PrivacyCeiling::from_caller("human:operator-1", None, /*is_ai=*/ false, false);
        let r = mint_receipt("human:operator-1", RecordType::ActionReceived);
        assert!(ceiling.admits(&r));
    }

    #[test]
    fn self_receipt_admitted_for_ai_caller() {
        let ceiling = PrivacyCeiling::from_caller("ai:agent-7", None, /*is_ai=*/ true, false);
        let r = mint_receipt("ai:agent-7", RecordType::ModelCall);
        assert!(ceiling.admits(&r));
    }

    // ─── constitutionally-public record types ──────────────────────────

    #[test]
    fn public_records_admitted_to_non_ai_callers() {
        let ceiling =
            PrivacyCeiling::from_caller("human:operator-1", None, /*is_ai=*/ false, false);
        let r = mint_receipt("service:other", RecordType::PolicyDecision);
        assert!(ceiling.admits(&r));
        let r = mint_receipt("service:other", RecordType::RecoveryEvent);
        assert!(ceiling.admits(&r));
        let r = mint_receipt("service:other", RecordType::TamperDetected);
        assert!(ceiling.admits(&r));
    }

    #[test]
    fn public_records_about_system_admitted_to_ai_callers() {
        let ceiling = PrivacyCeiling::from_caller("ai:agent-7", None, /*is_ai=*/ true, false);
        let r = mint_receipt("_system:service:policy-kernel", RecordType::PolicyDecision);
        assert!(ceiling.admits(&r));
    }

    #[test]
    fn public_records_about_non_system_denied_to_ai_callers() {
        // AI cannot see another agent's public records, nor a human's,
        // even if the record type is constitutionally public.
        let ceiling = PrivacyCeiling::from_caller("ai:agent-7", None, /*is_ai=*/ true, false);
        let r = mint_receipt("ai:agent-other", RecordType::PolicyDecision);
        assert!(!ceiling.admits(&r));
        let r = mint_receipt("human:operator-1", RecordType::PolicyDecision);
        assert!(!ceiling.admits(&r));
    }

    // ─── group-scoped admission ────────────────────────────────────────

    #[test]
    fn same_group_admitted() {
        let ceiling =
            PrivacyCeiling::from_caller("human:ops/alice", Some("ops".to_owned()), false, false);
        let r = mint_receipt("human:ops/bob", RecordType::ActionReceived);
        assert!(ceiling.admits(&r));
    }

    #[test]
    fn cross_group_denied() {
        let ceiling =
            PrivacyCeiling::from_caller("human:ops/alice", Some("ops".to_owned()), false, false);
        let r = mint_receipt("human:finance/dave", RecordType::ActionReceived);
        assert!(!ceiling.admits(&r));
    }

    #[test]
    fn caller_without_group_cannot_use_group_path() {
        let ceiling = PrivacyCeiling::from_caller("human:alice", None, false, false);
        // Even if receipt has a group, the caller without a group cannot
        // hit the group rule.
        let r = mint_receipt("human:ops/bob", RecordType::ActionReceived);
        assert!(!ceiling.admits(&r));
    }

    // ─── AI isolation in group context ─────────────────────────────────

    #[test]
    fn ai_caller_cannot_see_other_ai_in_same_group() {
        let ceiling = PrivacyCeiling::from_caller(
            "ai:research/agent-7",
            Some("research".to_owned()),
            /*is_ai=*/ true,
            false,
        );
        let r = mint_receipt("ai:research/agent-9", RecordType::ActionReceived);
        assert!(!ceiling.admits(&r));
    }

    #[test]
    fn ai_caller_cannot_see_human_in_same_group_even_if_public() {
        let ceiling = PrivacyCeiling::from_caller(
            "ai:research/agent-7",
            Some("research".to_owned()),
            /*is_ai=*/ true,
            false,
        );
        let r = mint_receipt("human:research/alice", RecordType::PolicyDecision);
        assert!(!ceiling.admits(&r));
    }

    // ─── recovery mode broadening ──────────────────────────────────────

    #[test]
    fn recovery_mode_broadens_forever_records_to_operator() {
        let ceiling = PrivacyCeiling::from_caller(
            "human:operator-1",
            None,
            /*is_ai=*/ false,
            /*recovery=*/ true,
        );
        // EXECUTION_COMPLETED has FOREVER retention; cross-subject + cross-group
        // but operator in recovery mode sees it.
        let r = mint_receipt("service:capability", RecordType::ExecutionCompleted);
        assert!(ceiling.admits(&r));
    }

    #[test]
    fn recovery_mode_does_not_broaden_standard_records() {
        let ceiling = PrivacyCeiling::from_caller(
            "human:operator-1",
            None,
            /*is_ai=*/ false,
            /*recovery=*/ true,
        );
        // ACTION_RECEIVED has STANDARD_24M retention — not broadened.
        let r = mint_receipt("service:other", RecordType::ActionReceived);
        assert!(!ceiling.admits(&r));
    }

    #[test]
    fn recovery_mode_does_not_broaden_for_ai_caller() {
        let ceiling = PrivacyCeiling::from_caller(
            "ai:agent-7",
            None,
            /*is_ai=*/ true,
            /*recovery=*/ true,
        );
        let r = mint_receipt("ai:agent-other", RecordType::ExecutionCompleted);
        assert!(!ceiling.admits(&r));
    }

    // ─── helpers ──────────────────────────────────────────────────────

    #[test]
    fn subject_primary_group_extracts_token_or_none() {
        assert_eq!(subject_primary_group("human:ops/alice"), Some("ops"));
        assert_eq!(
            subject_primary_group("ai:research/agent-7"),
            Some("research")
        );
        assert_eq!(subject_primary_group("human:alice"), None);
        assert_eq!(subject_primary_group("malformed"), None);
        assert_eq!(subject_primary_group("human:/alice"), None);
    }

    #[test]
    fn receipt_subject_is_system_detects_underscore_prefix() {
        assert!(receipt_subject_is_system("_system:service:audit"));
        assert!(receipt_subject_is_system("_system"));
        assert!(!receipt_subject_is_system("human:operator-1"));
        assert!(!receipt_subject_is_system("ai:agent"));
    }

    #[test]
    fn is_constitutionally_public_record_type_covers_spec_set() {
        // Sanity-check membership for the headline cases.
        assert!(is_constitutionally_public_record_type(
            RecordType::PolicyDecision
        ));
        assert!(is_constitutionally_public_record_type(
            RecordType::TamperDetected
        ));
        assert!(is_constitutionally_public_record_type(
            RecordType::SegmentSealed
        ));
        assert!(is_constitutionally_public_record_type(
            RecordType::FirstBootStarted
        ));
        assert!(is_constitutionally_public_record_type(
            RecordType::EmergencyOverrideGrant
        ));
        // Not public:
        assert!(!is_constitutionally_public_record_type(
            RecordType::ActionReceived
        ));
        assert!(!is_constitutionally_public_record_type(
            RecordType::ModelCall
        ));
    }
}
