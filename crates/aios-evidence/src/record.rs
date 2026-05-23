//! Closed taxonomies for the Evidence Log: [`RetentionClass`] and [`RecordType`].
//!
//! ## Scope of T-007
//!
//! - [`RetentionClass`] — complete 3-value enum from S3.1 §4 / §13 (the closed
//!   retention vocabulary). No extension.
//! - [`RecordType`] — **initial 30-variant subset** of the 427-entry vocabulary
//!   reconciled into S3.1 Appendix A by Wave 13. The full enum is queued for T-008
//!   (see the `TODO(T-008)` comment immediately above the type).
//!
//! ## Why a hand-rolled `match` for serde renames
//!
//! Each variant carries an exact `SCREAMING_SNAKE_CASE` wire name from S3.1
//! Appendix A. Using `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]` would correctly
//! handle most names, but two of the spec's variants — for example
//! `EXECUTION_COMPLETED` vs the Wave-6 family — require exact pinning. We therefore
//! attach `#[serde(rename = "...")]` to every variant, which makes drift between
//! Rust identifier and spec wire-name impossible without a compile-time touch to
//! this file.

use serde::{Deserialize, Serialize};

/// Closed retention vocabulary per S3.1 §4 (implicit) and §13 ("Forever" /
/// time-bounded retention horizons).
///
/// The Wave 13 reconciliation kept §6.4's retention enum implicit in the IDL but
/// every Wave-introduced `RecordType` is tagged with one of these three classes in
/// the Wave tables (e.g. §24 surface composition uses `STANDARD_24M` for
/// observability and `FOREVER` for constitutional barriers).
///
/// Three values exhaust the spec:
///
/// - `STANDARD_24M` — 24-month default retention for routine observability.
/// - `EXTENDED_60M` — 60-month retention for budget breaches, render failures,
///   and other moderate-priority forensic events.
/// - `FOREVER` — denials, tamper events, recovery transitions, constitutional
///   barriers. Never GC'd.
///
/// Serde-renamed to the exact spec strings.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RetentionClass {
    /// 24-month default retention.
    #[serde(rename = "STANDARD_24M")]
    Standard24M,

    /// 60-month extended retention for forensic / budget events.
    #[serde(rename = "EXTENDED_60M")]
    Extended60M,

    /// Permanent retention. Denials, tamper, recovery, constitutional barriers.
    #[serde(rename = "FOREVER")]
    Forever,
}

impl RetentionClass {
    /// Return the canonical `SCREAMING_SNAKE_CASE` wire name (mirrors serde rename).
    #[must_use]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::Standard24M => "STANDARD_24M",
            Self::Extended60M => "EXTENDED_60M",
            Self::Forever => "FOREVER",
        }
    }
}

// =====================================================================
// RecordType — closed enum, T-007 initial subset of S3.1 Appendix A
// =====================================================================

// TODO(T-008): extend to full 428-entry RecordType vocabulary per S3.1
// Appendix A Wave 13 IDL. The 30+ variants below cover the action-lifecycle
// emissions the M1 `aios-action` crate would produce, plus the highest-priority
// constitutional and recovery records (tamper detection, recovery boundary,
// first-boot operations, vault operations, approval lifecycle, build/test
// evidence). The remaining ~397 variants are queued for T-008.

/// Closed taxonomy of S3.1 `RecordType` values.
///
/// **This is an explicit, hand-curated subset.** The full Wave 13 vocabulary has
/// 427 `RecordType` IDs plus `RECORD_TYPE_UNSPECIFIED = 0` (see S3.1 Appendix A).
/// T-007 lands the highest-priority 30+ variants the M1 `aios-action` lifecycle
/// would emit; T-008 will reconcile the remainder. The variants here use the
/// exact `PascalCase` Rust identifier convention and serde-rename to the exact
/// `SCREAMING_SNAKE_CASE` spec strings.
///
/// ## Discipline
///
/// - No `Other`, no string fallback. Drift between code and spec is a compile
///   error in this file.
/// - `#[non_exhaustive]` is deliberately NOT applied: S3.1's enum is closed by
///   constitutional design (see §4 "closed `RecordType` vocabulary"). Adding
///   `#[non_exhaustive]` would silently allow downstream extension paths the
///   spec forbids.
/// - When T-008 extends the enum, downstream `match` statements that exhausted
///   the T-007 subset will gain `match arm missing` compile errors — that is
///   the intended forcing function to find every emission site.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecordType {
    // ----- Original §4 / Appendix A IDs 1..22 — action lifecycle and chain -----
    /// `ACTION_RECEIVED` (Appendix A ID 1) — an action envelope entered the
    /// Capability Runtime. Carries `action_id`, `envelope_hash`, `subject`, `adapter_id`.
    #[serde(rename = "ACTION_RECEIVED")]
    ActionReceived,

    /// `POLICY_DECISION` (Appendix A ID 4) — Policy Kernel returned `ALLOW` /
    /// `REQUIRE_APPROVAL` / `DENY` for an action.
    #[serde(rename = "POLICY_DECISION")]
    PolicyDecision,

    /// `APPROVAL_REQUESTED` (Appendix A ID 5) — an approval workflow was
    /// initiated because policy returned `REQUIRE_APPROVAL`.
    #[serde(rename = "APPROVAL_REQUESTED")]
    ApprovalRequested,

    /// `APPROVAL_GRANTED` (Appendix A ID 6) — operator approval recorded.
    #[serde(rename = "APPROVAL_GRANTED")]
    ApprovalGranted,

    /// `APPROVAL_DENIED` (Appendix A ID 7) — operator denial recorded.
    #[serde(rename = "APPROVAL_DENIED")]
    ApprovalDenied,

    /// `EXECUTION_STARTED` (Appendix A ID 8) — adapter began executing.
    #[serde(rename = "EXECUTION_STARTED")]
    ExecutionStarted,

    /// `EXECUTION_COMPLETED` (Appendix A ID 9) — adapter finished (success or
    /// failure surface is in the payload).
    #[serde(rename = "EXECUTION_COMPLETED")]
    ExecutionCompleted,

    /// `VERIFICATION_RESULT` (Appendix A ID 10) — verification harness reported
    /// `PASSED` / `FAILED` / `TIMEOUT` / `PROBE_ERROR` / `SKIPPED`.
    #[serde(rename = "VERIFICATION_RESULT")]
    VerificationResult,

    /// `ROLLBACK_COMPLETED` (Appendix A ID 11) — rollback path completed.
    #[serde(rename = "ROLLBACK_COMPLETED")]
    RollbackCompleted,

    /// `RECOVERY_EVENT` (Appendix A ID 12) — recovery mode entry / exit.
    /// Permanent retention.
    #[serde(rename = "RECOVERY_EVENT")]
    RecoveryEvent,

    /// `MODEL_CALL` (Appendix A ID 13) — cognitive model invocation evidence.
    #[serde(rename = "MODEL_CALL")]
    ModelCall,

    /// `CHAIN_CHECKPOINT` (Appendix A ID 14) — periodic rolling-hash checkpoint.
    #[serde(rename = "CHAIN_CHECKPOINT")]
    ChainCheckpoint,

    /// `EMERGENCY_OVERRIDE_GRANT` (Appendix A ID 18) — S5.4 override binding
    /// issued. Permanent retention.
    #[serde(rename = "EMERGENCY_OVERRIDE_GRANT")]
    EmergencyOverrideGrant,

    /// `POLICY_BUNDLE_LOAD` (Appendix A ID 19) — a policy bundle version was
    /// loaded. Permanent retention.
    #[serde(rename = "POLICY_BUNDLE_LOAD")]
    PolicyBundleLoad,

    /// `SEGMENT_SEALED` (Appendix A ID 20) — segment finalisation + signature.
    #[serde(rename = "SEGMENT_SEALED")]
    SegmentSealed,

    /// `CHAIN_INCONSISTENCY_DETECTED` (Appendix A ID 21) — `VerifyChain` or audit
    /// found a chain anomaly. Permanent retention; degraded mode trigger.
    #[serde(rename = "CHAIN_INCONSISTENCY_DETECTED")]
    ChainInconsistencyDetected,

    /// `TAMPER_DETECTED` (Appendix A ID 22) — segment signature or content hash
    /// mismatch. Permanent retention; degraded mode trigger.
    #[serde(rename = "TAMPER_DETECTED")]
    TamperDetected,

    // ----- §23 namespace integration — IDs 23..24 -----
    /// `SYSTEM_ADMIN_OPERATION` (Appendix A ID 23) — mutation of
    /// `/aios/system/apps/` or `/aios/system/agents/` by a `system_admin` holder.
    #[serde(rename = "SYSTEM_ADMIN_OPERATION")]
    SystemAdminOperation,

    /// `CROSS_GROUP_ACCESS_DENIED` (Appendix A ID 24) — S2.3
    /// `CrossGroupAccessForbidden` hard-deny fired.
    #[serde(rename = "CROSS_GROUP_ACCESS_DENIED")]
    CrossGroupAccessDenied,

    // ----- Constitutional / vault / recovery — Wave-6 and later -----
    /// `FIRST_BOOT_OPERATION` — per-mutation first-boot record (S9.1 / S9.2).
    /// Permanent retention.
    #[serde(rename = "FIRST_BOOT_OPERATION")]
    FirstBootOperation,

    /// `VAULT_OPERATION` — Vault Broker operation evidence (S5.2). Secrets
    /// never appear in the payload — only operation handles.
    #[serde(rename = "VAULT_OPERATION")]
    VaultOperation,

    /// `VAULT_BOOTSTRAP_KEY_USED` — bootstrap key consumption (S5.2 §10).
    /// Permanent retention; single-use enforcement.
    #[serde(rename = "VAULT_BOOTSTRAP_KEY_USED")]
    VaultBootstrapKeyUsed,

    /// `BOOTSTRAP_KEY_USE_AFTER_EXHAUST_BLOCKED` — attempt to reuse an exhausted
    /// bootstrap key was blocked (S5.2 §10).
    #[serde(rename = "BOOTSTRAP_KEY_USE_AFTER_EXHAUST_BLOCKED")]
    BootstrapKeyUseAfterExhaustBlocked,

    /// `EVIDENCE_LOG_TAMPER_DETECTED` — tamper detected on the evidence log
    /// itself. Permanent retention.
    #[serde(rename = "EVIDENCE_LOG_TAMPER_DETECTED")]
    EvidenceLogTamperDetected,

    /// `KERNEL_IMAGE_TAMPER_DETECTED` — kernel image hash mismatch on boot.
    #[serde(rename = "KERNEL_IMAGE_TAMPER_DETECTED")]
    KernelImageTamperDetected,

    /// `FIRMWARE_TAMPER_DETECTED` — firmware hash mismatch (S8.5).
    #[serde(rename = "FIRMWARE_TAMPER_DETECTED")]
    FirmwareTamperDetected,

    // ----- AI subject barriers — constitutional refusals -----
    /// `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED` — an AI subject attempted to
    /// install an app without operator approval. Constitutional barrier.
    #[serde(rename = "APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED")]
    AppAiDirectInstallAttemptedBlocked,

    /// `AGENT_SELF_GRADING_BLOCKED` — an agent attempted to mark its own work
    /// `REAL` without independent verification. Constitutional barrier.
    #[serde(rename = "AGENT_SELF_GRADING_BLOCKED")]
    AgentSelfGradingBlocked,

    /// `AGENT_DIRECT_FS_WRITE_BLOCKED` — an agent attempted a raw filesystem
    /// write outside the Capability Runtime. Constitutional barrier.
    #[serde(rename = "AGENT_DIRECT_FS_WRITE_BLOCKED")]
    AgentDirectFsWriteBlocked,

    /// `AI_DIRECT_INTERNET_DENIED` — an AI subject attempted unbroker'd network
    /// egress. Constitutional barrier.
    #[serde(rename = "AI_DIRECT_INTERNET_DENIED")]
    AiDirectInternetDenied,

    // ----- Build / test / release evidence -----
    /// `BUILD_PASSED` — workspace / crate build succeeded. Evidence grade E2.
    #[serde(rename = "BUILD_PASSED")]
    BuildPassed,

    /// `TEST_PASSED` — test suite passed (E3 evidence).
    #[serde(rename = "TEST_PASSED")]
    TestPassed,

    /// `E2E_PASSED` — end-to-end / release gate passed (E4 evidence).
    #[serde(rename = "E2E_PASSED")]
    E2ePassed,

    /// `ARTIFACT_RECORDED` — a build artifact's identity + hash was recorded.
    #[serde(rename = "ARTIFACT_RECORDED")]
    ArtifactRecorded,

    /// `OPERATIONAL_HEALTHY` — live operational evidence (E5).
    #[serde(rename = "OPERATIONAL_HEALTHY")]
    OperationalHealthy,

    /// `RECEIPT_SEALED` — a receipt has been sealed and committed to the chain.
    #[serde(rename = "RECEIPT_SEALED")]
    ReceiptSealed,

    /// `STATUS_TRANSITION` — a system or service status transition was recorded
    /// (e.g. SHELL → CONTRACT → REAL).
    #[serde(rename = "STATUS_TRANSITION")]
    StatusTransition,

    // ----- Failure surface for the action FSM (T-007 explicit additions
    //         backing the M1 aios-action lifecycle) -----
    /// `ACTION_FAILED` — companion to `EXECUTION_COMPLETED` when the action
    /// terminated in `failed` phase per S0.1 §6.
    #[serde(rename = "ACTION_FAILED")]
    ActionFailed,

    /// `ACTION_ROLLED_BACK` — companion to `ROLLBACK_COMPLETED` recording the
    /// envelope transition into `rolled_back`.
    #[serde(rename = "ACTION_ROLLED_BACK")]
    ActionRolledBack,
}

impl RecordType {
    /// The exact `SCREAMING_SNAKE_CASE` wire string for this `RecordType`.
    ///
    /// Identical to what `serde_json::to_string(&record_type)` emits, minus the
    /// surrounding quotation marks. Provided so callers do not need a JSON
    /// round-trip to log the canonical name.
    #[must_use]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::ActionReceived => "ACTION_RECEIVED",
            Self::PolicyDecision => "POLICY_DECISION",
            Self::ApprovalRequested => "APPROVAL_REQUESTED",
            Self::ApprovalGranted => "APPROVAL_GRANTED",
            Self::ApprovalDenied => "APPROVAL_DENIED",
            Self::ExecutionStarted => "EXECUTION_STARTED",
            Self::ExecutionCompleted => "EXECUTION_COMPLETED",
            Self::VerificationResult => "VERIFICATION_RESULT",
            Self::RollbackCompleted => "ROLLBACK_COMPLETED",
            Self::RecoveryEvent => "RECOVERY_EVENT",
            Self::ModelCall => "MODEL_CALL",
            Self::ChainCheckpoint => "CHAIN_CHECKPOINT",
            Self::EmergencyOverrideGrant => "EMERGENCY_OVERRIDE_GRANT",
            Self::PolicyBundleLoad => "POLICY_BUNDLE_LOAD",
            Self::SegmentSealed => "SEGMENT_SEALED",
            Self::ChainInconsistencyDetected => "CHAIN_INCONSISTENCY_DETECTED",
            Self::TamperDetected => "TAMPER_DETECTED",
            Self::SystemAdminOperation => "SYSTEM_ADMIN_OPERATION",
            Self::CrossGroupAccessDenied => "CROSS_GROUP_ACCESS_DENIED",
            Self::FirstBootOperation => "FIRST_BOOT_OPERATION",
            Self::VaultOperation => "VAULT_OPERATION",
            Self::VaultBootstrapKeyUsed => "VAULT_BOOTSTRAP_KEY_USED",
            Self::BootstrapKeyUseAfterExhaustBlocked => "BOOTSTRAP_KEY_USE_AFTER_EXHAUST_BLOCKED",
            Self::EvidenceLogTamperDetected => "EVIDENCE_LOG_TAMPER_DETECTED",
            Self::KernelImageTamperDetected => "KERNEL_IMAGE_TAMPER_DETECTED",
            Self::FirmwareTamperDetected => "FIRMWARE_TAMPER_DETECTED",
            Self::AppAiDirectInstallAttemptedBlocked => "APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED",
            Self::AgentSelfGradingBlocked => "AGENT_SELF_GRADING_BLOCKED",
            Self::AgentDirectFsWriteBlocked => "AGENT_DIRECT_FS_WRITE_BLOCKED",
            Self::AiDirectInternetDenied => "AI_DIRECT_INTERNET_DENIED",
            Self::BuildPassed => "BUILD_PASSED",
            Self::TestPassed => "TEST_PASSED",
            Self::E2ePassed => "E2E_PASSED",
            Self::ArtifactRecorded => "ARTIFACT_RECORDED",
            Self::OperationalHealthy => "OPERATIONAL_HEALTHY",
            Self::ReceiptSealed => "RECEIPT_SEALED",
            Self::StatusTransition => "STATUS_TRANSITION",
            Self::ActionFailed => "ACTION_FAILED",
            Self::ActionRolledBack => "ACTION_ROLLED_BACK",
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;

    // ---- RetentionClass --------------------------------------------------

    #[test]
    fn retention_class_serializes_to_spec_strings() {
        let s_standard = serde_json::to_string(&RetentionClass::Standard24M).expect("ser");
        assert_eq!(s_standard, "\"STANDARD_24M\"");

        let s_extended = serde_json::to_string(&RetentionClass::Extended60M).expect("ser");
        assert_eq!(s_extended, "\"EXTENDED_60M\"");

        let s_forever = serde_json::to_string(&RetentionClass::Forever).expect("ser");
        assert_eq!(s_forever, "\"FOREVER\"");
    }

    #[test]
    fn retention_class_round_trips() {
        for v in [
            RetentionClass::Standard24M,
            RetentionClass::Extended60M,
            RetentionClass::Forever,
        ] {
            let s = serde_json::to_string(&v).expect("ser");
            let back: RetentionClass = serde_json::from_str(&s).expect("de");
            assert_eq!(back, v);
        }
    }

    #[test]
    fn retention_class_as_wire_str_matches_serde() {
        for v in [
            RetentionClass::Standard24M,
            RetentionClass::Extended60M,
            RetentionClass::Forever,
        ] {
            let s = serde_json::to_string(&v).expect("ser");
            assert_eq!(s, format!("\"{}\"", v.as_wire_str()));
        }
    }

    // ---- RecordType ------------------------------------------------------

    /// The complete list of `RecordType` variants landed in T-007 with their exact
    /// wire-name strings. Kept here so any drift between Rust identifier and
    /// spec wire-name fails this single test.
    const T007_VARIANTS: &[(RecordType, &str)] = &[
        (RecordType::ActionReceived, "ACTION_RECEIVED"),
        (RecordType::PolicyDecision, "POLICY_DECISION"),
        (RecordType::ApprovalRequested, "APPROVAL_REQUESTED"),
        (RecordType::ApprovalGranted, "APPROVAL_GRANTED"),
        (RecordType::ApprovalDenied, "APPROVAL_DENIED"),
        (RecordType::ExecutionStarted, "EXECUTION_STARTED"),
        (RecordType::ExecutionCompleted, "EXECUTION_COMPLETED"),
        (RecordType::VerificationResult, "VERIFICATION_RESULT"),
        (RecordType::RollbackCompleted, "ROLLBACK_COMPLETED"),
        (RecordType::RecoveryEvent, "RECOVERY_EVENT"),
        (RecordType::ModelCall, "MODEL_CALL"),
        (RecordType::ChainCheckpoint, "CHAIN_CHECKPOINT"),
        (
            RecordType::EmergencyOverrideGrant,
            "EMERGENCY_OVERRIDE_GRANT",
        ),
        (RecordType::PolicyBundleLoad, "POLICY_BUNDLE_LOAD"),
        (RecordType::SegmentSealed, "SEGMENT_SEALED"),
        (
            RecordType::ChainInconsistencyDetected,
            "CHAIN_INCONSISTENCY_DETECTED",
        ),
        (RecordType::TamperDetected, "TAMPER_DETECTED"),
        (RecordType::SystemAdminOperation, "SYSTEM_ADMIN_OPERATION"),
        (
            RecordType::CrossGroupAccessDenied,
            "CROSS_GROUP_ACCESS_DENIED",
        ),
        (RecordType::FirstBootOperation, "FIRST_BOOT_OPERATION"),
        (RecordType::VaultOperation, "VAULT_OPERATION"),
        (
            RecordType::VaultBootstrapKeyUsed,
            "VAULT_BOOTSTRAP_KEY_USED",
        ),
        (
            RecordType::BootstrapKeyUseAfterExhaustBlocked,
            "BOOTSTRAP_KEY_USE_AFTER_EXHAUST_BLOCKED",
        ),
        (
            RecordType::EvidenceLogTamperDetected,
            "EVIDENCE_LOG_TAMPER_DETECTED",
        ),
        (
            RecordType::KernelImageTamperDetected,
            "KERNEL_IMAGE_TAMPER_DETECTED",
        ),
        (
            RecordType::FirmwareTamperDetected,
            "FIRMWARE_TAMPER_DETECTED",
        ),
        (
            RecordType::AppAiDirectInstallAttemptedBlocked,
            "APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED",
        ),
        (
            RecordType::AgentSelfGradingBlocked,
            "AGENT_SELF_GRADING_BLOCKED",
        ),
        (
            RecordType::AgentDirectFsWriteBlocked,
            "AGENT_DIRECT_FS_WRITE_BLOCKED",
        ),
        (
            RecordType::AiDirectInternetDenied,
            "AI_DIRECT_INTERNET_DENIED",
        ),
        (RecordType::BuildPassed, "BUILD_PASSED"),
        (RecordType::TestPassed, "TEST_PASSED"),
        (RecordType::E2ePassed, "E2E_PASSED"),
        (RecordType::ArtifactRecorded, "ARTIFACT_RECORDED"),
        (RecordType::OperationalHealthy, "OPERATIONAL_HEALTHY"),
        (RecordType::ReceiptSealed, "RECEIPT_SEALED"),
        (RecordType::StatusTransition, "STATUS_TRANSITION"),
        (RecordType::ActionFailed, "ACTION_FAILED"),
        (RecordType::ActionRolledBack, "ACTION_ROLLED_BACK"),
    ];

    #[test]
    fn t007_subset_has_at_least_thirty_variants() {
        // Acceptance criterion: at least 30 variants land in T-007.
        assert!(
            T007_VARIANTS.len() >= 30,
            "T-007 must land at least 30 RecordType variants, got {}",
            T007_VARIANTS.len()
        );
    }

    #[test]
    fn every_record_type_serializes_to_its_spec_string() {
        for (rt, expected) in T007_VARIANTS {
            let s = serde_json::to_string(rt).expect("serialize");
            assert_eq!(
                s,
                format!("\"{expected}\""),
                "RecordType {rt:?} did not serialize to {expected}"
            );
        }
    }

    #[test]
    fn every_record_type_round_trips_through_json() {
        for (rt, _) in T007_VARIANTS {
            let s = serde_json::to_string(rt).expect("ser");
            let back: RecordType = serde_json::from_str(&s).expect("de");
            assert_eq!(back, *rt);
        }
    }

    #[test]
    fn as_wire_str_matches_serde_for_every_record_type() {
        for (rt, expected) in T007_VARIANTS {
            assert_eq!(rt.as_wire_str(), *expected);
            let s = serde_json::to_string(rt).expect("ser");
            assert_eq!(s, format!("\"{}\"", rt.as_wire_str()));
        }
    }
}
