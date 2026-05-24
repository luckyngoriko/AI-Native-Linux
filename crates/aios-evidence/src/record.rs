//! Closed taxonomies for the Evidence Log: [`RetentionClass`] and [`RecordType`].
//!
//! Per S3.1 Appendix A (Wave 13 IDL reconciliation, DEC-051); 427 closed
//! `RecordType` variants spanning wire IDs 1..=427 (plus `RECORD_TYPE_UNSPECIFIED = 0`
//! which the Rust enum models as the absence of a `RecordType` value rather than as
//! a variant — the Rust enum is exhaustive over the 427 named records).
//!
//! ## Wave / ID-range breakdown
//!
//! | Source                           | Wire ID range | Count | Reference         |
//! |----------------------------------|---------------|-------|-------------------|
//! | Original Appendix A              | 1..=22        | 22    | S3.1 §4           |
//! | §23 Namespace integration (S4.1) | 23..=24       | 2     | S3.1 §23          |
//! | Wave 5 (renderers + GPU)         | 25..=82       | 58    | S3.1 §24          |
//! | Wave 6 (vault/approval/network)  | 83..=150      | 68    | S3.1 §25          |
//! | Wave 7 (repo/kernel/apps)        | 151..=196     | 46    | S3.1 §26          |
//! | Wave 8 (Tier 1+2 consolidation)  | 197..=387     | 191   | S3.1 §27          |
//! | Wave 10 (orphans + W9 + C13)     | 388..=427     | 40    | S3.1 §28 + §29.5  |
//! | **Total**                        | **1..=427**   | **427** | **S3.1 §29.2** |
//!
//! ## Closed-vocabulary discipline
//!
//! - No `Other`, no string fallback, no `#[non_exhaustive]`. S3.1 §29 freezes the
//!   IDs at 1..=427; Wave 14+ extensions land in the reserved `1000..=9999` range,
//!   not by re-ordering existing variants.
//! - Eight Wave 8 narrative entries reuse Wave 6 IDs (`ADAPTER_REGISTERED`,
//!   `ADAPTER_REGISTRATION_REJECTED`, `ADAPTER_DEGRADED`, `ADAPTER_DEREGISTERED`)
//!   and three §25.2 narrative entries reuse original IDs (`APPROVAL_REQUESTED`,
//!   `APPROVAL_GRANTED`, `APPROVAL_DENIED`) plus `ACTION_RECEIVED` at ID 1.
//!   Per §29.3 the canonical (earlier) ID is authoritative, and this enum lists
//!   each name exactly once.
//! - The synonym `APPROVAL_BINDING_VOIDED` was collapsed into
//!   `BINDING_VOIDED_ACTION_REVISED` (ID 128) per §29.3 and is intentionally
//!   absent from this enum.
//! - The four `MODEL_CALL`-family synonym retentions described in §29.4 ARE all
//!   present as distinct variants (legacy `MODEL_CALL` at ID 13 plus the 12-name
//!   S13.2 family `MODEL_INVOCATION_*`/`MODEL_BACKEND_*`/... at IDs 248..=259).
//!
//! ## Retention-class mapping
//!
//! [`retention_class_for`] maps every variant to its declared
//! [`RetentionClass`] per S3.1's per-record tables. Conflicts between the Wave 6
//! S10.1 declaration of `ADAPTER_DEGRADED` (`STANDARD_24M`) and the Wave 8 S15.3
//! re-declaration (`EXTENDED_60M`) are resolved by the canonical-site rule from
//! §29.3: the earlier ID's retention class wins (here `STANDARD_24M`).

use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Closed retention vocabulary per S3.1 §4 / §13 / §6.4.
///
/// Three classes exhaust the Wave 13 taxonomy:
///
/// - `STANDARD_24M` — 24-month default retention for routine observability.
/// - `EXTENDED_60M` — 60-month retention for budget breaches, render failures,
///   and other moderate-priority forensic events.
/// - `FOREVER` — denials, tamper, recovery transitions, constitutional barriers.
///   Never garbage-collected.
///
/// Wire format is the exact `SCREAMING_SNAKE_CASE` token from the spec.
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
// RecordType — closed enum, 427 variants per S3.1 Appendix A Wave 13.
// =====================================================================

/// Closed taxonomy of S3.1 `RecordType` values (Wave 13 / DEC-051).
///
/// Every variant carries an explicit `#[serde(rename = "EXACT_WIRE_NAME")]` so
/// drift between Rust identifier and spec wire name is a compile-time touch to
/// this file. Variant declaration order matches the spec's wire-ID order
/// (1..=427) — this is what an auditor expects when diffing against
/// `Appendix A`.
///
/// ## Discipline
///
/// - No `Other`, no string fallback. Drift between code and spec is a compile
///   error in this file.
/// - `#[non_exhaustive]` is deliberately NOT applied: S3.1 §24 / §29 freeze
///   the vocabulary at 427 entries; Wave 14+ additions land in the reserved
///   `1000..=9999` range and add new variants without rewriting existing ones.
/// - Downstream `match RecordType { ... }` callers must be exhaustive; the
///   compiler will enforce this.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
pub enum RecordType {
    // ─── Original Appendix A (§4) (IDs 1..=22) ───
    /// `ACTION_RECEIVED` — Wire ID 1.
    #[serde(rename = "ACTION_RECEIVED")]
    ActionReceived,
    /// `TRANSLATION_CREATED` — Wire ID 2.
    #[serde(rename = "TRANSLATION_CREATED")]
    TranslationCreated,
    /// `ROUTING_DECISION` — Wire ID 3.
    #[serde(rename = "ROUTING_DECISION")]
    RoutingDecision,
    /// `POLICY_DECISION` — Wire ID 4.
    #[serde(rename = "POLICY_DECISION")]
    PolicyDecision,
    /// `APPROVAL_REQUESTED` — Wire ID 5.
    #[serde(rename = "APPROVAL_REQUESTED")]
    ApprovalRequested,
    /// `APPROVAL_GRANTED` — Wire ID 6.
    #[serde(rename = "APPROVAL_GRANTED")]
    ApprovalGranted,
    /// `APPROVAL_DENIED` — Wire ID 7.
    #[serde(rename = "APPROVAL_DENIED")]
    ApprovalDenied,
    /// `EXECUTION_STARTED` — Wire ID 8.
    #[serde(rename = "EXECUTION_STARTED")]
    ExecutionStarted,
    /// `EXECUTION_COMPLETED` — Wire ID 9.
    #[serde(rename = "EXECUTION_COMPLETED")]
    ExecutionCompleted,
    /// `VERIFICATION_RESULT` — Wire ID 10.
    #[serde(rename = "VERIFICATION_RESULT")]
    VerificationResult,
    /// `ROLLBACK_COMPLETED` — Wire ID 11.
    #[serde(rename = "ROLLBACK_COMPLETED")]
    RollbackCompleted,
    /// `RECOVERY_EVENT` — Wire ID 12.
    #[serde(rename = "RECOVERY_EVENT")]
    RecoveryEvent,
    /// `MODEL_CALL` — Wire ID 13.
    #[serde(rename = "MODEL_CALL")]
    ModelCall,
    /// `CHAIN_CHECKPOINT` — Wire ID 14.
    #[serde(rename = "CHAIN_CHECKPOINT")]
    ChainCheckpoint,
    /// `GC_PASS` — Wire ID 15.
    #[serde(rename = "GC_PASS")]
    GcPass,
    /// `QUARANTINE_EVENT` — Wire ID 16.
    #[serde(rename = "QUARANTINE_EVENT")]
    QuarantineEvent,
    /// `CONFLICT_EVENT` — Wire ID 17.
    #[serde(rename = "CONFLICT_EVENT")]
    ConflictEvent,
    /// `EMERGENCY_OVERRIDE_GRANT` — Wire ID 18.
    #[serde(rename = "EMERGENCY_OVERRIDE_GRANT")]
    EmergencyOverrideGrant,
    /// `POLICY_BUNDLE_LOAD` — Wire ID 19.
    #[serde(rename = "POLICY_BUNDLE_LOAD")]
    PolicyBundleLoad,
    /// `SEGMENT_SEALED` — Wire ID 20.
    #[serde(rename = "SEGMENT_SEALED")]
    SegmentSealed,
    /// `CHAIN_INCONSISTENCY_DETECTED` — Wire ID 21.
    #[serde(rename = "CHAIN_INCONSISTENCY_DETECTED")]
    ChainInconsistencyDetected,
    /// `TAMPER_DETECTED` — Wire ID 22.
    #[serde(rename = "TAMPER_DETECTED")]
    TamperDetected,

    // ─── §23 Namespace integration (S4.1) (IDs 23..=24) ───
    /// `SYSTEM_ADMIN_OPERATION` — Wire ID 23.
    #[serde(rename = "SYSTEM_ADMIN_OPERATION")]
    SystemAdminOperation,
    /// `CROSS_GROUP_ACCESS_DENIED` — Wire ID 24.
    #[serde(rename = "CROSS_GROUP_ACCESS_DENIED")]
    CrossGroupAccessDenied,

    // ─── Wave 5 (§24): renderers + GPU (IDs 25..=82) ───
    // ── S7.1 Surface Composition (7) — §24.1.1 ──
    /// `SURFACE_CREATED` — Wire ID 25.
    #[serde(rename = "SURFACE_CREATED")]
    SurfaceCreated,
    /// `SURFACE_DESTROYED` — Wire ID 26.
    #[serde(rename = "SURFACE_DESTROYED")]
    SurfaceDestroyed,
    /// `SURFACE_GPU_BUDGET_EXCEEDED` — Wire ID 27.
    #[serde(rename = "SURFACE_GPU_BUDGET_EXCEEDED")]
    SurfaceGpuBudgetExceeded,
    /// `CROSS_SURFACE_READ_DENIED` — Wire ID 28.
    #[serde(rename = "CROSS_SURFACE_READ_DENIED")]
    CrossSurfaceReadDenied,
    /// `CROSS_ZONE_VIOLATION_ATTEMPTED` — Wire ID 29.
    #[serde(rename = "CROSS_ZONE_VIOLATION_ATTEMPTED")]
    CrossZoneViolationAttempted,
    /// `RECOVERY_KIND_REJECTED` — Wire ID 30.
    #[serde(rename = "RECOVERY_KIND_REJECTED")]
    RecoveryKindRejected,
    /// `SURFACE_NEVER_RENDERED` — Wire ID 31.
    #[serde(rename = "SURFACE_NEVER_RENDERED")]
    SurfaceNeverRendered,
    // ── S7.2 Shared UI Schema (3) — §24.1.2 ──
    /// `UI_TREE_VALIDATION_REJECTED` — Wire ID 32.
    #[serde(rename = "UI_TREE_VALIDATION_REJECTED")]
    UiTreeValidationRejected,
    /// `UI_TRUST_BEARING_AUTHORSHIP_REFUSED` — Wire ID 33.
    #[serde(rename = "UI_TRUST_BEARING_AUTHORSHIP_REFUSED")]
    UiTrustBearingAuthorshipRefused,
    /// `UI_RECOVERY_NODE_DROPPED` — Wire ID 34.
    #[serde(rename = "UI_RECOVERY_NODE_DROPPED")]
    UiRecoveryNodeDropped,
    // ── S7.3 Visual Language (4) — §24.1.3 ──
    /// `THEME_LOADED` — Wire ID 35.
    #[serde(rename = "THEME_LOADED")]
    ThemeLoaded,
    /// `THEME_REJECTED` — Wire ID 36.
    #[serde(rename = "THEME_REJECTED")]
    ThemeRejected,
    /// `THEME_SWITCHED` — Wire ID 37.
    #[serde(rename = "THEME_SWITCHED")]
    ThemeSwitched,
    /// `THEME_INVARIANT_VIOLATED` — Wire ID 38.
    #[serde(rename = "THEME_INVARIANT_VIOLATED")]
    ThemeInvariantViolated,
    // ── S7.4 KDE Renderer (11) — §24.1.4 ──
    /// `KDE_RENDERER_STARTED` — Wire ID 39.
    #[serde(rename = "KDE_RENDERER_STARTED")]
    KdeRendererStarted,
    /// `KDE_RENDERER_DEGRADED` — Wire ID 40.
    #[serde(rename = "KDE_RENDERER_DEGRADED")]
    KdeRendererDegraded,
    /// `KDE_FRAME_DROPPED` — Wire ID 41.
    #[serde(rename = "KDE_FRAME_DROPPED")]
    KdeFrameDropped,
    /// `KDE_LAYER_SHELL_REJECTED` — Wire ID 42.
    #[serde(rename = "KDE_LAYER_SHELL_REJECTED")]
    KdeLayerShellRejected,
    /// `KDE_KWIN_SCRIPT_LOADED` — Wire ID 43.
    #[serde(rename = "KDE_KWIN_SCRIPT_LOADED")]
    KdeKwinScriptLoaded,
    /// `KDE_KWIN_SCRIPT_REJECTED` — Wire ID 44.
    #[serde(rename = "KDE_KWIN_SCRIPT_REJECTED")]
    KdeKwinScriptRejected,
    /// `KDE_RECOVERY_SHELL_STARTED` — Wire ID 45.
    #[serde(rename = "KDE_RECOVERY_SHELL_STARTED")]
    KdeRecoveryShellStarted,
    /// `KDE_RECOVERY_KIND_REJECTED_AT_RENDERER` — Wire ID 46.
    #[serde(rename = "KDE_RECOVERY_KIND_REJECTED_AT_RENDERER")]
    KdeRecoveryKindRejectedAtRenderer,
    /// `KDE_PLASMA_THEME_OVERRIDDEN` — Wire ID 47.
    #[serde(rename = "KDE_PLASMA_THEME_OVERRIDDEN")]
    KdePlasmaThemeOverridden,
    /// `KDE_RENDER_FAILED` — Wire ID 48.
    #[serde(rename = "KDE_RENDER_FAILED")]
    KdeRenderFailed,
    /// `KDE_TOKEN_FALLBACK_USED` — Wire ID 49.
    #[serde(rename = "KDE_TOKEN_FALLBACK_USED")]
    KdeTokenFallbackUsed,
    // ── S7.5 Web Renderer (17) — §24.1.5 ──
    /// `WEB_LAN_EXPOSURE_GRANTED` — Wire ID 50.
    #[serde(rename = "WEB_LAN_EXPOSURE_GRANTED")]
    WebLanExposureGranted,
    /// `WEB_PUBLIC_EXPOSURE_GRANTED` — Wire ID 51.
    #[serde(rename = "WEB_PUBLIC_EXPOSURE_GRANTED")]
    WebPublicExposureGranted,
    /// `WEB_RECOVERY_KIND_REJECTED` — Wire ID 52.
    #[serde(rename = "WEB_RECOVERY_KIND_REJECTED")]
    WebRecoveryKindRejected,
    /// `WEB_PUBLIC_EXPOSURE_FIREWALL_RECORDED` — Wire ID 53.
    #[serde(rename = "WEB_PUBLIC_EXPOSURE_FIREWALL_RECORDED")]
    WebPublicExposureFirewallRecorded,
    /// `WEB_RECOVERY_PAGE_LOADED` — Wire ID 54.
    #[serde(rename = "WEB_RECOVERY_PAGE_LOADED")]
    WebRecoveryPageLoaded,
    /// `WEB_RECOVERY_PAGE_EXITED` — Wire ID 55.
    #[serde(rename = "WEB_RECOVERY_PAGE_EXITED")]
    WebRecoveryPageExited,
    /// `WEB_RENDERER_STARTED` — Wire ID 56.
    #[serde(rename = "WEB_RENDERER_STARTED")]
    WebRendererStarted,
    /// `WEB_RENDERER_DEGRADED` — Wire ID 57.
    #[serde(rename = "WEB_RENDERER_DEGRADED")]
    WebRendererDegraded,
    /// `WEB_LAN_EXPOSURE_ACTIVE` — Wire ID 58.
    #[serde(rename = "WEB_LAN_EXPOSURE_ACTIVE")]
    WebLanExposureActive,
    /// `WEB_EXPOSURE_REVOKED` — Wire ID 59.
    #[serde(rename = "WEB_EXPOSURE_REVOKED")]
    WebExposureRevoked,
    /// `WEB_EXTENSION_INTERFERENCE` — Wire ID 60.
    #[serde(rename = "WEB_EXTENSION_INTERFERENCE")]
    WebExtensionInterference,
    /// `WEB_FULLSCREEN_REQUESTED` — Wire ID 61.
    #[serde(rename = "WEB_FULLSCREEN_REQUESTED")]
    WebFullscreenRequested,
    /// `WEB_THEME_INJECTION_BLOCKED` — Wire ID 62.
    #[serde(rename = "WEB_THEME_INJECTION_BLOCKED")]
    WebThemeInjectionBlocked,
    /// `WEB_THEME_FALLBACK_USED` — Wire ID 63.
    #[serde(rename = "WEB_THEME_FALLBACK_USED")]
    WebThemeFallbackUsed,
    /// `WEB_CLIENT_STORAGE_QUOTA_BREACH` — Wire ID 64.
    #[serde(rename = "WEB_CLIENT_STORAGE_QUOTA_BREACH")]
    WebClientStorageQuotaBreach,
    /// `WEB_RENDERER_CLS_BREACH` — Wire ID 65.
    #[serde(rename = "WEB_RENDERER_CLS_BREACH")]
    WebRendererClsBreach,
    /// `WEB_CONSTITUTIONAL_ELEMENT_REREGISTER_BLOCKED` — Wire ID 66.
    #[serde(rename = "WEB_CONSTITUTIONAL_ELEMENT_REREGISTER_BLOCKED")]
    WebConstitutionalElementReregisterBlocked,
    // ── S8.2 GPU Resource Model (16) — §24.1.6 ──
    /// `GPU_DEVICE_ENUMERATED` — Wire ID 67.
    #[serde(rename = "GPU_DEVICE_ENUMERATED")]
    GpuDeviceEnumerated,
    /// `GPU_DEVICE_DISCONNECTED` — Wire ID 68.
    #[serde(rename = "GPU_DEVICE_DISCONNECTED")]
    GpuDeviceDisconnected,
    /// `GPU_VK_DEVICE_CREATED` — Wire ID 69.
    #[serde(rename = "GPU_VK_DEVICE_CREATED")]
    GpuVkDeviceCreated,
    /// `GPU_VK_DEVICE_DESTROYED` — Wire ID 70.
    #[serde(rename = "GPU_VK_DEVICE_DESTROYED")]
    GpuVkDeviceDestroyed,
    /// `GPU_DMABUF_GRANTED` — Wire ID 71.
    #[serde(rename = "GPU_DMABUF_GRANTED")]
    GpuDmabufGranted,
    /// `GPU_DMABUF_DENIED` — Wire ID 72.
    #[serde(rename = "GPU_DMABUF_DENIED")]
    GpuDmabufDenied,
    /// `GPU_CAPABILITY_DENIED` — Wire ID 73.
    #[serde(rename = "GPU_CAPABILITY_DENIED")]
    GpuCapabilityDenied,
    /// `GPU_VALIDATION_DISABLED_RECOVERY` — Wire ID 74.
    #[serde(rename = "GPU_VALIDATION_DISABLED_RECOVERY")]
    GpuValidationDisabledRecovery,
    /// `GPU_VALIDATION_ENABLED_NORMAL` — Wire ID 75.
    #[serde(rename = "GPU_VALIDATION_ENABLED_NORMAL")]
    GpuValidationEnabledNormal,
    /// `DRIVER_UNAVAILABLE` — Wire ID 76.
    #[serde(rename = "DRIVER_UNAVAILABLE")]
    DriverUnavailable,
    /// `GPU_BUDGET_EXCEEDED` — Wire ID 77.
    #[serde(rename = "GPU_BUDGET_EXCEEDED")]
    GpuBudgetExceeded,
    /// `GPU_BUDGET_DOWNGRADED` — Wire ID 78.
    #[serde(rename = "GPU_BUDGET_DOWNGRADED")]
    GpuBudgetDowngraded,
    /// `IOMMU_UNAVAILABLE_DEGRADED` — Wire ID 79.
    #[serde(rename = "IOMMU_UNAVAILABLE_DEGRADED")]
    IommuUnavailableDegraded,
    /// `HOST_CAPABILITY_LIE` — Wire ID 80.
    #[serde(rename = "HOST_CAPABILITY_LIE")]
    HostCapabilityLie,
    /// `GPU_BINDING_FORGERY` — Wire ID 81.
    #[serde(rename = "GPU_BINDING_FORGERY")]
    GpuBindingForgery,
    /// `GPU_DEVICE_FORCE_RECLAIMED` — Wire ID 82.
    #[serde(rename = "GPU_DEVICE_FORCE_RECLAIMED")]
    GpuDeviceForceReclaimed,

    // ─── Wave 6 (§25): vault / approval / override / recovery / capability runtime / network (IDs 83..=150) ───
    // ── S5.2 Vault Broker (8) — §25.1 ──
    /// `VAULT_CAPABILITY_ISSUED` — Wire ID 83.
    #[serde(rename = "VAULT_CAPABILITY_ISSUED")]
    VaultCapabilityIssued,
    /// `VAULT_CAPABILITY_ROTATED` — Wire ID 84.
    #[serde(rename = "VAULT_CAPABILITY_ROTATED")]
    VaultCapabilityRotated,
    /// `VAULT_CAPABILITY_REVOKED` — Wire ID 85.
    #[serde(rename = "VAULT_CAPABILITY_REVOKED")]
    VaultCapabilityRevoked,
    /// `VAULT_OPERATION` — Wire ID 86.
    #[serde(rename = "VAULT_OPERATION")]
    VaultOperation,
    /// `VAULT_RAW_REVEAL` — Wire ID 87.
    #[serde(rename = "VAULT_RAW_REVEAL")]
    VaultRawReveal,
    /// `VAULT_CAPABILITY_FORGERY` — Wire ID 88.
    #[serde(rename = "VAULT_CAPABILITY_FORGERY")]
    VaultCapabilityForgery,
    /// `SUBJECT_KIND_REJECTED_FOR_VAULT` — Wire ID 89.
    #[serde(rename = "SUBJECT_KIND_REJECTED_FOR_VAULT")]
    SubjectKindRejectedForVault,
    /// `VAULT_RECOVERY_SNAPSHOT_LOADED` — Wire ID 90.
    #[serde(rename = "VAULT_RECOVERY_SNAPSHOT_LOADED")]
    VaultRecoverySnapshotLoaded,
    // ── S5.3 Approval Mechanics (5 new; 3 reused at IDs 5/6/7) — §25.2 ──
    /// `APPROVAL_DELIVERED` — Wire ID 91.
    #[serde(rename = "APPROVAL_DELIVERED")]
    ApprovalDelivered,
    /// `APPROVAL_EXPIRED` — Wire ID 92.
    #[serde(rename = "APPROVAL_EXPIRED")]
    ApprovalExpired,
    /// `APPROVAL_CONSUMED` — Wire ID 93.
    #[serde(rename = "APPROVAL_CONSUMED")]
    ApprovalConsumed,
    /// `APPROVAL_REVOKED` — Wire ID 94.
    #[serde(rename = "APPROVAL_REVOKED")]
    ApprovalRevoked,
    /// `APPROVAL_DELIVERY_FAILED` — Wire ID 95.
    #[serde(rename = "APPROVAL_DELIVERY_FAILED")]
    ApprovalDeliveryFailed,
    // ── S5.4 Emergency Override (8) — §25.3 ──
    /// `OVERRIDE_REQUESTED` — Wire ID 96.
    #[serde(rename = "OVERRIDE_REQUESTED")]
    OverrideRequested,
    /// `OVERRIDE_QUORUM_RECEIVED` — Wire ID 97.
    #[serde(rename = "OVERRIDE_QUORUM_RECEIVED")]
    OverrideQuorumReceived,
    /// `OVERRIDE_GRANTED` — Wire ID 98.
    #[serde(rename = "OVERRIDE_GRANTED")]
    OverrideGranted,
    /// `OVERRIDE_CONSUMED` — Wire ID 99.
    #[serde(rename = "OVERRIDE_CONSUMED")]
    OverrideConsumed,
    /// `OVERRIDE_DENIED` — Wire ID 100.
    #[serde(rename = "OVERRIDE_DENIED")]
    OverrideDenied,
    /// `OVERRIDE_EXPIRED` — Wire ID 101.
    #[serde(rename = "OVERRIDE_EXPIRED")]
    OverrideExpired,
    /// `OVERRIDE_REVOKED` — Wire ID 102.
    #[serde(rename = "OVERRIDE_REVOKED")]
    OverrideRevoked,
    /// `OVERRIDE_REVIEW` — Wire ID 103.
    #[serde(rename = "OVERRIDE_REVIEW")]
    OverrideReview,
    // ── S9.1 Recovery Boundary (10) — §25.4 ──
    /// `RECOVERY_BOOT_ENTERED` — Wire ID 104.
    #[serde(rename = "RECOVERY_BOOT_ENTERED")]
    RecoveryBootEntered,
    /// `RECOVERY_OPERATOR_AUTHENTICATED` — Wire ID 105.
    #[serde(rename = "RECOVERY_OPERATOR_AUTHENTICATED")]
    RecoveryOperatorAuthenticated,
    /// `RECOVERY_OPERATION_PERFORMED` — Wire ID 106.
    #[serde(rename = "RECOVERY_OPERATION_PERFORMED")]
    RecoveryOperationPerformed,
    /// `RECOVERY_TTL_EXPIRED_AUTO_REBOOT` — Wire ID 107.
    #[serde(rename = "RECOVERY_TTL_EXPIRED_AUTO_REBOOT")]
    RecoveryTtlExpiredAutoReboot,
    /// `RECOVERY_BOOT_EXITED` — Wire ID 108.
    #[serde(rename = "RECOVERY_BOOT_EXITED")]
    RecoveryBootExited,
    /// `RECOVERY_L5_START_BLOCKED` — Wire ID 109.
    #[serde(rename = "RECOVERY_L5_START_BLOCKED")]
    RecoveryL5StartBlocked,
    /// `RECOVERY_NETWORK_LAN_ENABLED` — Wire ID 110.
    #[serde(rename = "RECOVERY_NETWORK_LAN_ENABLED")]
    RecoveryNetworkLanEnabled,
    /// `RECOVERY_NETWORK_LAN_DISABLED` — Wire ID 111.
    #[serde(rename = "RECOVERY_NETWORK_LAN_DISABLED")]
    RecoveryNetworkLanDisabled,
    /// `RECOVERY_FORENSIC_ATTACH_PERFORMED` — Wire ID 112.
    #[serde(rename = "RECOVERY_FORENSIC_ATTACH_PERFORMED")]
    RecoveryForensicAttachPerformed,
    /// `BOOT_FAILURE_AUTO_RECOVERY_TRIGGERED` — Wire ID 113.
    #[serde(rename = "BOOT_FAILURE_AUTO_RECOVERY_TRIGGERED")]
    BootFailureAutoRecoveryTriggered,
    // ── S10.1 Capability Runtime gRPC (19 new; ACTION_RECEIVED reused at ID 1) — §25.5 ──
    /// `ACTION_VALIDATED` — Wire ID 114.
    #[serde(rename = "ACTION_VALIDATED")]
    ActionValidated,
    /// `ACTION_POLICY_DECISION` — Wire ID 115.
    #[serde(rename = "ACTION_POLICY_DECISION")]
    ActionPolicyDecision,
    /// `ACTION_DISPATCHED` — Wire ID 116.
    #[serde(rename = "ACTION_DISPATCHED")]
    ActionDispatched,
    /// `EXECUTION_SUCCEEDED` — Wire ID 117.
    #[serde(rename = "EXECUTION_SUCCEEDED")]
    ExecutionSucceeded,
    /// `EXECUTION_FAILED` — Wire ID 118.
    #[serde(rename = "EXECUTION_FAILED")]
    ExecutionFailed,
    /// `EXECUTION_VERIFICATION_FAILED` — Wire ID 119.
    #[serde(rename = "EXECUTION_VERIFICATION_FAILED")]
    ExecutionVerificationFailed,
    /// `ROLLBACK_ATTEMPTED` — Wire ID 120.
    #[serde(rename = "ROLLBACK_ATTEMPTED")]
    RollbackAttempted,
    /// `ROLLBACK_SUCCEEDED` — Wire ID 121.
    #[serde(rename = "ROLLBACK_SUCCEEDED")]
    RollbackSucceeded,
    /// `ROLLBACK_FAILED_REQUIRES_OPERATOR` — Wire ID 122.
    #[serde(rename = "ROLLBACK_FAILED_REQUIRES_OPERATOR")]
    RollbackFailedRequiresOperator,
    /// `ADAPTER_REGISTERED` — Wire ID 123.
    #[serde(rename = "ADAPTER_REGISTERED")]
    AdapterRegistered,
    /// `ADAPTER_REGISTRATION_REJECTED` — Wire ID 124.
    #[serde(rename = "ADAPTER_REGISTRATION_REJECTED")]
    AdapterRegistrationRejected,
    /// `ADAPTER_DEGRADED` — Wire ID 125.
    #[serde(rename = "ADAPTER_DEGRADED")]
    AdapterDegraded,
    /// `ADAPTER_DEREGISTERED` — Wire ID 126.
    #[serde(rename = "ADAPTER_DEREGISTERED")]
    AdapterDeregistered,
    /// `IDEMPOTENCY_KEY_REPLAY_DETECTED` — Wire ID 127.
    #[serde(rename = "IDEMPOTENCY_KEY_REPLAY_DETECTED")]
    IdempotencyKeyReplayDetected,
    /// `BINDING_VOIDED_ACTION_REVISED` — Wire ID 128.
    #[serde(rename = "BINDING_VOIDED_ACTION_REVISED")]
    BindingVoidedActionRevised,
    /// `AI_INTERACTIVE_QUEUE_DOWNGRADE` — Wire ID 129.
    #[serde(rename = "AI_INTERACTIVE_QUEUE_DOWNGRADE")]
    AiInteractiveQueueDowngrade,
    /// `DRY_RUN_SIMULATION_RECORDED` — Wire ID 130.
    #[serde(rename = "DRY_RUN_SIMULATION_RECORDED")]
    DryRunSimulationRecorded,
    /// `EXPERIMENTAL_ADAPTER_LIVE_DISPATCH` — Wire ID 131.
    #[serde(rename = "EXPERIMENTAL_ADAPTER_LIVE_DISPATCH")]
    ExperimentalAdapterLiveDispatch,
    /// `ADAPTER_DEPRECATED_DISPATCH` — Wire ID 132.
    #[serde(rename = "ADAPTER_DEPRECATED_DISPATCH")]
    AdapterDeprecatedDispatch,
    // ── S8.1 Network Policy (18) — §25.6 ──
    /// `NETWORK_POSTURE_CHANGED` — Wire ID 133.
    #[serde(rename = "NETWORK_POSTURE_CHANGED")]
    NetworkPostureChanged,
    /// `EXPOSURE_REQUESTED` — Wire ID 134.
    #[serde(rename = "EXPOSURE_REQUESTED")]
    ExposureRequested,
    /// `EXPOSURE_GRANTED` — Wire ID 135.
    #[serde(rename = "EXPOSURE_GRANTED")]
    ExposureGranted,
    /// `EXPOSURE_DENIED` — Wire ID 136.
    #[serde(rename = "EXPOSURE_DENIED")]
    ExposureDenied,
    /// `EXPOSURE_REVOKED` — Wire ID 137.
    #[serde(rename = "EXPOSURE_REVOKED")]
    ExposureRevoked,
    /// `EXPOSURE_TERMINATED_TTL_EXPIRED` — Wire ID 138.
    #[serde(rename = "EXPOSURE_TERMINATED_TTL_EXPIRED")]
    ExposureTerminatedTtlExpired,
    /// `PUBLIC_EXPOSURE_HEARTBEAT` — Wire ID 139.
    #[serde(rename = "PUBLIC_EXPOSURE_HEARTBEAT")]
    PublicExposureHeartbeat,
    /// `OUTBOUND_GRANT_ISSUED` — Wire ID 140.
    #[serde(rename = "OUTBOUND_GRANT_ISSUED")]
    OutboundGrantIssued,
    /// `OUTBOUND_GRANT_REVOKED` — Wire ID 141.
    #[serde(rename = "OUTBOUND_GRANT_REVOKED")]
    OutboundGrantRevoked,
    /// `OUTBOUND_OUTSIDE_MANIFEST` — Wire ID 142.
    #[serde(rename = "OUTBOUND_OUTSIDE_MANIFEST")]
    OutboundOutsideManifest,
    /// `OUTBOUND_DEGRADED_TO_LOOPBACK_AUTO` — Wire ID 143.
    #[serde(rename = "OUTBOUND_DEGRADED_TO_LOOPBACK_AUTO")]
    OutboundDegradedToLoopbackAuto,
    /// `ALLOWLIST_FQDN_FANOUT_EXCEEDED` — Wire ID 144.
    #[serde(rename = "ALLOWLIST_FQDN_FANOUT_EXCEEDED")]
    AllowlistFqdnFanoutExceeded,
    /// `LAN_SUBNET_DRIFT_DETECTED` — Wire ID 145.
    #[serde(rename = "LAN_SUBNET_DRIFT_DETECTED")]
    LanSubnetDriftDetected,
    /// `LAN_PEER_DRIFT_DETECTED` — Wire ID 146.
    #[serde(rename = "LAN_PEER_DRIFT_DETECTED")]
    LanPeerDriftDetected,
    /// `AI_DIRECT_INTERNET_DENIED` — Wire ID 147.
    #[serde(rename = "AI_DIRECT_INTERNET_DENIED")]
    AiDirectInternetDenied,
    /// `EXTERNAL_MODEL_CALL_BROKERED` — Wire ID 148.
    #[serde(rename = "EXTERNAL_MODEL_CALL_BROKERED")]
    ExternalModelCallBrokered,
    /// `BACKEND_DEGRADED_NFTABLES_TO_IPTABLES` — Wire ID 149.
    #[serde(rename = "BACKEND_DEGRADED_NFTABLES_TO_IPTABLES")]
    BackendDegradedNftablesToIptables,
    /// `RAW_SOCKET_BYPASS_ATTEMPTED` — Wire ID 150.
    #[serde(rename = "RAW_SOCKET_BYPASS_ATTEMPTED")]
    RawSocketBypassAttempted,

    // ─── Wave 7 (§26): repo / kernel pipeline / app runtime (IDs 151..=196) ───
    // ── S11.1 Repository Model (19) — §26.1 ──
    /// `PACKAGE_FETCH_STARTED` — Wire ID 151.
    #[serde(rename = "PACKAGE_FETCH_STARTED")]
    PackageFetchStarted,
    /// `PACKAGE_VERIFIED` — Wire ID 152.
    #[serde(rename = "PACKAGE_VERIFIED")]
    PackageVerified,
    /// `PACKAGE_VERIFICATION_FAILED` — Wire ID 153.
    #[serde(rename = "PACKAGE_VERIFICATION_FAILED")]
    PackageVerificationFailed,
    /// `PACKAGE_APPROVAL_REQUESTED` — Wire ID 154.
    #[serde(rename = "PACKAGE_APPROVAL_REQUESTED")]
    PackageApprovalRequested,
    /// `PACKAGE_INSTALLED` — Wire ID 155.
    #[serde(rename = "PACKAGE_INSTALLED")]
    PackageInstalled,
    /// `PACKAGE_INSTALL_FAILED` — Wire ID 156.
    #[serde(rename = "PACKAGE_INSTALL_FAILED")]
    PackageInstallFailed,
    /// `PACKAGE_QUARANTINED` — Wire ID 157.
    #[serde(rename = "PACKAGE_QUARANTINED")]
    PackageQuarantined,
    /// `PACKAGE_UNINSTALLED` — Wire ID 158.
    #[serde(rename = "PACKAGE_UNINSTALLED")]
    PackageUninstalled,
    /// `PACKAGE_DOWNGRADE_BLOCKED` — Wire ID 159.
    #[serde(rename = "PACKAGE_DOWNGRADE_BLOCKED")]
    PackageDowngradeBlocked,
    /// `CAPABILITY_LIE_DETECTED` — Wire ID 160.
    #[serde(rename = "CAPABILITY_LIE_DETECTED")]
    CapabilityLieDetected,
    /// `TRUST_CHAIN_BROKEN` — Wire ID 161.
    #[serde(rename = "TRUST_CHAIN_BROKEN")]
    TrustChainBroken,
    /// `TRUST_CHAIN_TOO_DEEP` — Wire ID 162.
    #[serde(rename = "TRUST_CHAIN_TOO_DEEP")]
    TrustChainTooDeep,
    /// `MANIFEST_FORGED` — Wire ID 163.
    #[serde(rename = "MANIFEST_FORGED")]
    ManifestForged,
    /// `MIRROR_HASH_MISMATCH_BLACKLISTED` — Wire ID 164.
    #[serde(rename = "MIRROR_HASH_MISMATCH_BLACKLISTED")]
    MirrorHashMismatchBlacklisted,
    /// `PUBLISHER_KEY_ROTATED` — Wire ID 165.
    #[serde(rename = "PUBLISHER_KEY_ROTATED")]
    PublisherKeyRotated,
    /// `PUBLISHER_DEPLATFORMED` — Wire ID 166.
    #[serde(rename = "PUBLISHER_DEPLATFORMED")]
    PublisherDeplatformed,
    /// `EXTERNAL_BRIDGE_PACKAGE_ADMITTED` — Wire ID 167.
    #[serde(rename = "EXTERNAL_BRIDGE_PACKAGE_ADMITTED")]
    ExternalBridgePackageAdmitted,
    /// `EXTERNAL_BRIDGE_UPSTREAM_SIGNATURE_FAILED` — Wire ID 168.
    #[serde(rename = "EXTERNAL_BRIDGE_UPSTREAM_SIGNATURE_FAILED")]
    ExternalBridgeUpstreamSignatureFailed,
    /// `AIOS_ROOT_KEY_ROTATED` — Wire ID 169.
    #[serde(rename = "AIOS_ROOT_KEY_ROTATED")]
    AiosRootKeyRotated,
    // ── S9.3 Dedicated Kernel Pipeline (13) — §26.2 ──
    /// `KERNEL_PIPELINE_STARTED` — Wire ID 170.
    #[serde(rename = "KERNEL_PIPELINE_STARTED")]
    KernelPipelineStarted,
    /// `KERNEL_BUILD_COMPLETED` — Wire ID 171.
    #[serde(rename = "KERNEL_BUILD_COMPLETED")]
    KernelBuildCompleted,
    /// `KERNEL_GATE_RESULT` — Wire ID 172.
    #[serde(rename = "KERNEL_GATE_RESULT")]
    KernelGateResult,
    /// `KERNEL_CONVERGED` — Wire ID 173.
    #[serde(rename = "KERNEL_CONVERGED")]
    KernelConverged,
    /// `KERNEL_DIVERGED_REGRESSION` — Wire ID 174.
    #[serde(rename = "KERNEL_DIVERGED_REGRESSION")]
    KernelDivergedRegression,
    /// `KERNEL_PROMOTED_TO_A` — Wire ID 175.
    #[serde(rename = "KERNEL_PROMOTED_TO_A")]
    KernelPromotedToA,
    /// `KERNEL_PROMOTED_TO_B` — Wire ID 176.
    #[serde(rename = "KERNEL_PROMOTED_TO_B")]
    KernelPromotedToB,
    /// `KERNEL_ROLLBACK_PERFORMED` — Wire ID 177.
    #[serde(rename = "KERNEL_ROLLBACK_PERFORMED")]
    KernelRollbackPerformed,
    /// `KERNEL_IMAGE_OBSERVED` — Wire ID 178.
    #[serde(rename = "KERNEL_IMAGE_OBSERVED")]
    KernelImageObserved,
    /// `KERNEL_IMAGE_DRIFT_DETECTED` — Wire ID 179.
    #[serde(rename = "KERNEL_IMAGE_DRIFT_DETECTED")]
    KernelImageDriftDetected,
    /// `KERNEL_REFRESH_SCHEDULED` — Wire ID 180.
    #[serde(rename = "KERNEL_REFRESH_SCHEDULED")]
    KernelRefreshScheduled,
    /// `KERNEL_REFRESH_PIPELINE_FAILED` — Wire ID 181.
    #[serde(rename = "KERNEL_REFRESH_PIPELINE_FAILED")]
    KernelRefreshPipelineFailed,
    /// `PIPELINE_DEFINITION_REPLACED` — Wire ID 182.
    #[serde(rename = "PIPELINE_DEFINITION_REPLACED")]
    PipelineDefinitionReplaced,
    // ── S12.1 App Runtime Model (14) — §26.3 ──
    /// `APP_OBSERVE_STARTED` — Wire ID 183.
    #[serde(rename = "APP_OBSERVE_STARTED")]
    AppObserveStarted,
    /// `APP_OBSERVE_COMPLETED` — Wire ID 184.
    #[serde(rename = "APP_OBSERVE_COMPLETED")]
    AppObserveCompleted,
    /// `APP_OBSERVE_TIMEOUT` — Wire ID 185.
    #[serde(rename = "APP_OBSERVE_TIMEOUT")]
    AppObserveTimeout,
    /// `APP_TRANSLATE_MANIFEST_PROPOSED` — Wire ID 186.
    #[serde(rename = "APP_TRANSLATE_MANIFEST_PROPOSED")]
    AppTranslateManifestProposed,
    /// `APP_TRANSLATE_MANIFEST_APPROVED` — Wire ID 187.
    #[serde(rename = "APP_TRANSLATE_MANIFEST_APPROVED")]
    AppTranslateManifestApproved,
    /// `APP_TRANSLATE_MANIFEST_REJECTED` — Wire ID 188.
    #[serde(rename = "APP_TRANSLATE_MANIFEST_REJECTED")]
    AppTranslateManifestRejected,
    /// `APP_RECIPE_CONTRIBUTED` — Wire ID 189.
    #[serde(rename = "APP_RECIPE_CONTRIBUTED")]
    AppRecipeContributed,
    /// `APP_RECIPE_IMPORTED` — Wire ID 190.
    #[serde(rename = "APP_RECIPE_IMPORTED")]
    AppRecipeImported,
    /// `APP_MANIFEST_DELTA_PROPOSED` — Wire ID 191.
    #[serde(rename = "APP_MANIFEST_DELTA_PROPOSED")]
    AppManifestDeltaProposed,
    /// `APP_MANIFEST_DELTA_APPROVED` — Wire ID 192.
    #[serde(rename = "APP_MANIFEST_DELTA_APPROVED")]
    AppManifestDeltaApproved,
    /// `APP_HONESTY_CLASS_VIOLATION` — Wire ID 193.
    #[serde(rename = "APP_HONESTY_CLASS_VIOLATION")]
    AppHonestyClassViolation,
    /// `APP_ECOSYSTEM_RUNTIME_BREAKOUT_ATTEMPTED` — Wire ID 194.
    #[serde(rename = "APP_ECOSYSTEM_RUNTIME_BREAKOUT_ATTEMPTED")]
    AppEcosystemRuntimeBreakoutAttempted,
    /// `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED` — Wire ID 195.
    #[serde(rename = "APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED")]
    AppAiDirectInstallAttemptedBlocked,
    /// `APP_RECIPE_DECEPTIVE_REJECTED_AT_INGEST` — Wire ID 196.
    #[serde(rename = "APP_RECIPE_DECEPTIVE_REJECTED_AT_INGEST")]
    AppRecipeDeceptiveRejectedAtIngest,

    // ─── Wave 8 (§27): Tier 1+2 cross-spec consolidation (IDs 197..=387) ───
    // ── S9.2 First-Boot Flow (11) — §27.1 ──
    /// `FIRST_BOOT_STARTED` — Wire ID 197.
    #[serde(rename = "FIRST_BOOT_STARTED")]
    FirstBootStarted,
    /// `FIRST_BOOT_STAGE_COMPLETED` — Wire ID 198.
    #[serde(rename = "FIRST_BOOT_STAGE_COMPLETED")]
    FirstBootStageCompleted,
    /// `FIRST_BOOT_FAILED` — Wire ID 199.
    #[serde(rename = "FIRST_BOOT_FAILED")]
    FirstBootFailed,
    /// `VAULT_ROOT_KEY_GENERATED` — Wire ID 200.
    #[serde(rename = "VAULT_ROOT_KEY_GENERATED")]
    VaultRootKeyGenerated,
    /// `AI_PROVIDER_MODE_SET` — Wire ID 201.
    #[serde(rename = "AI_PROVIDER_MODE_SET")]
    AiProviderModeSet,
    /// `INITIAL_FIREWALL_POSTURE_SET` — Wire ID 202.
    #[serde(rename = "INITIAL_FIREWALL_POSTURE_SET")]
    InitialFirewallPostureSet,
    /// `FIRST_GROUP_REGISTERED` — Wire ID 203.
    #[serde(rename = "FIRST_GROUP_REGISTERED")]
    FirstGroupRegistered,
    /// `FIRST_USER_REGISTERED` — Wire ID 204.
    #[serde(rename = "FIRST_USER_REGISTERED")]
    FirstUserRegistered,
    /// `RECOVERY_OPERATOR_REGISTERED` — Wire ID 205.
    #[serde(rename = "RECOVERY_OPERATOR_REGISTERED")]
    RecoveryOperatorRegistered,
    /// `FIRST_BOOT_COMPLETE` — Wire ID 206.
    #[serde(rename = "FIRST_BOOT_COMPLETE")]
    FirstBootComplete,
    /// `RESET_TO_FACTORY_INITIATED` — Wire ID 207.
    #[serde(rename = "RESET_TO_FACTORY_INITIATED")]
    ResetToFactoryInitiated,
    // ── S14.1 Failure Handling (10) — §27.2 ──
    /// `FAILURE_OBSERVED` — Wire ID 208.
    #[serde(rename = "FAILURE_OBSERVED")]
    FailureObserved,
    /// `DEGRADATION_LEVEL_TRANSITIONED` — Wire ID 209.
    #[serde(rename = "DEGRADATION_LEVEL_TRANSITIONED")]
    DegradationLevelTransitioned,
    /// `COMPONENT_RESTARTED` — Wire ID 210.
    #[serde(rename = "COMPONENT_RESTARTED")]
    ComponentRestarted,
    /// `COMPONENT_RESTART_BUDGET_EXHAUSTED` — Wire ID 211.
    #[serde(rename = "COMPONENT_RESTART_BUDGET_EXHAUSTED")]
    ComponentRestartBudgetExhausted,
    /// `CIRCUIT_BREAKER_OPENED` — Wire ID 212.
    #[serde(rename = "CIRCUIT_BREAKER_OPENED")]
    CircuitBreakerOpened,
    /// `CIRCUIT_BREAKER_CLOSED` — Wire ID 213.
    #[serde(rename = "CIRCUIT_BREAKER_CLOSED")]
    CircuitBreakerClosed,
    /// `HALTED_PENDING_OPERATOR` — Wire ID 214.
    #[serde(rename = "HALTED_PENDING_OPERATOR")]
    HaltedPendingOperator,
    /// `TIME_DRIFT_DETECTED` — Wire ID 215.
    #[serde(rename = "TIME_DRIFT_DETECTED")]
    TimeDriftDetected,
    /// `BACKEND_VERSION_MISMATCH` — Wire ID 216.
    #[serde(rename = "BACKEND_VERSION_MISMATCH")]
    BackendVersionMismatch,
    /// `RECOVERY_LOOP_DETECTED` — Wire ID 217.
    #[serde(rename = "RECOVERY_LOOP_DETECTED")]
    RecoveryLoopDetected,
    // ── S6.3 Evidence Receipt Schema (4) — §27.3 ──
    /// `RECEIPT_REDACTION_FAILED` — Wire ID 218.
    #[serde(rename = "RECEIPT_REDACTION_FAILED")]
    ReceiptRedactionFailed,
    /// `RECEIPT_INTEGRITY_QUARANTINED` — Wire ID 219.
    #[serde(rename = "RECEIPT_INTEGRITY_QUARANTINED")]
    ReceiptIntegrityQuarantined,
    /// `RECEIPT_LINEAGE_CYCLE_DETECTED` — Wire ID 220.
    #[serde(rename = "RECEIPT_LINEAGE_CYCLE_DETECTED")]
    ReceiptLineageCycleDetected,
    /// `RECEIPT_SEQUENCE_OUT_OF_ORDER` — Wire ID 221.
    #[serde(rename = "RECEIPT_SEQUENCE_OUT_OF_ORDER")]
    ReceiptSequenceOutOfOrder,
    // ── S15.1 Unit Manifest (8) — §27.4 ──
    /// `UNIT_REGISTERED` — Wire ID 222.
    #[serde(rename = "UNIT_REGISTERED")]
    UnitRegistered,
    /// `UNIT_STARTED` — Wire ID 223.
    #[serde(rename = "UNIT_STARTED")]
    UnitStarted,
    /// `UNIT_HEALTHY` — Wire ID 224.
    #[serde(rename = "UNIT_HEALTHY")]
    UnitHealthy,
    /// `UNIT_DEGRADED` — Wire ID 225.
    #[serde(rename = "UNIT_DEGRADED")]
    UnitDegraded,
    /// `UNIT_FAILED` — Wire ID 226.
    #[serde(rename = "UNIT_FAILED")]
    UnitFailed,
    /// `UNIT_STOPPED` — Wire ID 227.
    #[serde(rename = "UNIT_STOPPED")]
    UnitStopped,
    /// `UNIT_ROLLBACK_TRIGGERED` — Wire ID 228.
    #[serde(rename = "UNIT_ROLLBACK_TRIGGERED")]
    UnitRollbackTriggered,
    /// `UNIT_DEPENDENCY_CYCLE_DETECTED` — Wire ID 229.
    #[serde(rename = "UNIT_DEPENDENCY_CYCLE_DETECTED")]
    UnitDependencyCycleDetected,
    // ── S15.2 SGR State Transitions (12) — §27.5 ──
    /// `GRAPH_EVALUATED` — Wire ID 230.
    #[serde(rename = "GRAPH_EVALUATED")]
    GraphEvaluated,
    /// `TRANSITION_QUEUED` — Wire ID 231.
    #[serde(rename = "TRANSITION_QUEUED")]
    TransitionQueued,
    /// `TRANSITION_STARTED` — Wire ID 232.
    #[serde(rename = "TRANSITION_STARTED")]
    TransitionStarted,
    /// `TRANSITION_SUCCEEDED` — Wire ID 233.
    #[serde(rename = "TRANSITION_SUCCEEDED")]
    TransitionSucceeded,
    /// `TRANSITION_FAILED` — Wire ID 234.
    #[serde(rename = "TRANSITION_FAILED")]
    TransitionFailed,
    /// `AB_CANARY_PROMOTED` — Wire ID 235.
    #[serde(rename = "AB_CANARY_PROMOTED")]
    AbCanaryPromoted,
    /// `AB_ROLLBACK_PERFORMED` — Wire ID 236.
    #[serde(rename = "AB_ROLLBACK_PERFORMED")]
    AbRollbackPerformed,
    /// `DEPENDENCY_CYCLE_DETECTED` — Wire ID 237.
    #[serde(rename = "DEPENDENCY_CYCLE_DETECTED")]
    DependencyCycleDetected,
    /// `TRANSITION_CONFLICT` — Wire ID 238.
    #[serde(rename = "TRANSITION_CONFLICT")]
    TransitionConflict,
    /// `RESOURCE_BUDGET_DENIED` — Wire ID 239.
    #[serde(rename = "RESOURCE_BUDGET_DENIED")]
    ResourceBudgetDenied,
    /// `GRAPH_BLOCKED_RESOURCE` — Wire ID 240.
    #[serde(rename = "GRAPH_BLOCKED_RESOURCE")]
    GraphBlockedResource,
    /// `GRAPH_CONVERGED` — Wire ID 241.
    #[serde(rename = "GRAPH_CONVERGED")]
    GraphConverged,
    // ── S15.3 SGR Adapter Model (6 new; 4 reused at IDs 123/124/125/126) — §27.6 ──
    /// `ADAPTER_REGISTRATION_REQUESTED` — Wire ID 242.
    #[serde(rename = "ADAPTER_REGISTRATION_REQUESTED")]
    AdapterRegistrationRequested,
    /// `ADAPTER_HEALTHY` — Wire ID 243.
    #[serde(rename = "ADAPTER_HEALTHY")]
    AdapterHealthy,
    /// `ADAPTER_ACTION_KIND_VIOLATION` — Wire ID 244.
    #[serde(rename = "ADAPTER_ACTION_KIND_VIOLATION")]
    AdapterActionKindViolation,
    /// `ADAPTER_CAPABILITY_VIOLATION` — Wire ID 245.
    #[serde(rename = "ADAPTER_CAPABILITY_VIOLATION")]
    AdapterCapabilityViolation,
    /// `ADAPTER_HOT_RELOADED` — Wire ID 246.
    #[serde(rename = "ADAPTER_HOT_RELOADED")]
    AdapterHotReloaded,
    /// `ADAPTER_DOWNGRADE_REJECTED` — Wire ID 247.
    #[serde(rename = "ADAPTER_DOWNGRADE_REJECTED")]
    AdapterDowngradeRejected,
    // ── S13.2 Cognitive Model Router (12) — §27.7 ──
    /// `MODEL_INVOCATION_STARTED` — Wire ID 248.
    #[serde(rename = "MODEL_INVOCATION_STARTED")]
    ModelInvocationStarted,
    /// `MODEL_INVOCATION_SUCCEEDED` — Wire ID 249.
    #[serde(rename = "MODEL_INVOCATION_SUCCEEDED")]
    ModelInvocationSucceeded,
    /// `MODEL_INVOCATION_FAILED` — Wire ID 250.
    #[serde(rename = "MODEL_INVOCATION_FAILED")]
    ModelInvocationFailed,
    /// `MODEL_BACKEND_DEGRADED` — Wire ID 251.
    #[serde(rename = "MODEL_BACKEND_DEGRADED")]
    ModelBackendDegraded,
    /// `MODEL_CIRCUIT_OPENED` — Wire ID 252.
    #[serde(rename = "MODEL_CIRCUIT_OPENED")]
    ModelCircuitOpened,
    /// `MODEL_PROMPT_INJECTION_DETECTED` — Wire ID 253.
    #[serde(rename = "MODEL_PROMPT_INJECTION_DETECTED")]
    ModelPromptInjectionDetected,
    /// `MODEL_RESPONSE_SIGNATURE_FAILED` — Wire ID 254.
    #[serde(rename = "MODEL_RESPONSE_SIGNATURE_FAILED")]
    ModelResponseSignatureFailed,
    /// `MODEL_VAULT_DENY` — Wire ID 255.
    #[serde(rename = "MODEL_VAULT_DENY")]
    ModelVaultDeny,
    /// `MODEL_NETWORK_DENY` — Wire ID 256.
    #[serde(rename = "MODEL_NETWORK_DENY")]
    ModelNetworkDeny,
    /// `MODEL_RATE_LIMITED` — Wire ID 257.
    #[serde(rename = "MODEL_RATE_LIMITED")]
    ModelRateLimited,
    /// `MODEL_BACKEND_REGISTERED` — Wire ID 258.
    #[serde(rename = "MODEL_BACKEND_REGISTERED")]
    ModelBackendRegistered,
    /// `MODEL_BACKEND_RETIRED` — Wire ID 259.
    #[serde(rename = "MODEL_BACKEND_RETIRED")]
    ModelBackendRetired,
    // ── S13.1 Cognitive Core Model (18) — §27.8 ──
    /// `AGENT_REGISTERED` — Wire ID 260.
    #[serde(rename = "AGENT_REGISTERED")]
    AgentRegistered,
    /// `AGENT_RETIRED` — Wire ID 261.
    #[serde(rename = "AGENT_RETIRED")]
    AgentRetired,
    /// `AGENT_INTERRUPTED_BY_RECOVERY` — Wire ID 262.
    #[serde(rename = "AGENT_INTERRUPTED_BY_RECOVERY")]
    AgentInterruptedByRecovery,
    /// `AGENT_PROPOSAL_EMITTED` — Wire ID 263.
    #[serde(rename = "AGENT_PROPOSAL_EMITTED")]
    AgentProposalEmitted,
    /// `AGENT_PROPOSAL_APPROVED` — Wire ID 264.
    #[serde(rename = "AGENT_PROPOSAL_APPROVED")]
    AgentProposalApproved,
    /// `AGENT_PROPOSAL_DENIED` — Wire ID 265.
    #[serde(rename = "AGENT_PROPOSAL_DENIED")]
    AgentProposalDenied,
    /// `AGENT_PLAN_BUNDLED_APPROVED` — Wire ID 266.
    #[serde(rename = "AGENT_PLAN_BUNDLED_APPROVED")]
    AgentPlanBundledApproved,
    /// `AGENT_PLAN_ABANDONED` — Wire ID 267.
    #[serde(rename = "AGENT_PLAN_ABANDONED")]
    AgentPlanAbandoned,
    /// `AGENT_MEMORY_WRITE` — Wire ID 268.
    #[serde(rename = "AGENT_MEMORY_WRITE")]
    AgentMemoryWrite,
    /// `AGENT_MEMORY_READ` — Wire ID 269.
    #[serde(rename = "AGENT_MEMORY_READ")]
    AgentMemoryRead,
    /// `AGENT_MEMORY_CROSS_USER_DENIED` — Wire ID 270.
    #[serde(rename = "AGENT_MEMORY_CROSS_USER_DENIED")]
    AgentMemoryCrossUserDenied,
    /// `AGENT_INTER_MESSAGE_SENT` — Wire ID 271.
    #[serde(rename = "AGENT_INTER_MESSAGE_SENT")]
    AgentInterMessageSent,
    /// `AGENT_INTER_MESSAGE_REJECTED` — Wire ID 272.
    #[serde(rename = "AGENT_INTER_MESSAGE_REJECTED")]
    AgentInterMessageRejected,
    /// `AGENT_SELF_GRADING_BLOCKED` — Wire ID 273.
    #[serde(rename = "AGENT_SELF_GRADING_BLOCKED")]
    AgentSelfGradingBlocked,
    /// `AGENT_DIRECT_FS_WRITE_BLOCKED` — Wire ID 274.
    #[serde(rename = "AGENT_DIRECT_FS_WRITE_BLOCKED")]
    AgentDirectFsWriteBlocked,
    /// `AGENT_CROSS_GROUP_COORDINATION_BLOCKED` — Wire ID 275.
    #[serde(rename = "AGENT_CROSS_GROUP_COORDINATION_BLOCKED")]
    AgentCrossGroupCoordinationBlocked,
    /// `AGENT_BACKEND_DEGRADED` — Wire ID 276.
    #[serde(rename = "AGENT_BACKEND_DEGRADED")]
    AgentBackendDegraded,
    /// `AGENT_PROMPT_INJECTION_DETECTED` — Wire ID 277.
    #[serde(rename = "AGENT_PROMPT_INJECTION_DETECTED")]
    AgentPromptInjectionDetected,
    // ── S12.2 Package Object Model (10) — §27.9 ──
    /// `PACKAGE_OBJECT_CREATED` — Wire ID 278.
    #[serde(rename = "PACKAGE_OBJECT_CREATED")]
    PackageObjectCreated,
    /// `PACKAGE_OBJECT_UPDATED` — Wire ID 279.
    #[serde(rename = "PACKAGE_OBJECT_UPDATED")]
    PackageObjectUpdated,
    /// `PACKAGE_OBJECT_ROLLED_BACK` — Wire ID 280.
    #[serde(rename = "PACKAGE_OBJECT_ROLLED_BACK")]
    PackageObjectRolledBack,
    /// `PACKAGE_OBJECT_QUARANTINED` — Wire ID 281.
    #[serde(rename = "PACKAGE_OBJECT_QUARANTINED")]
    PackageObjectQuarantined,
    /// `PACKAGE_PRIVATE_STATE_INITIALIZED` — Wire ID 282.
    #[serde(rename = "PACKAGE_PRIVATE_STATE_INITIALIZED")]
    PackagePrivateStateInitialized,
    /// `PACKAGE_PRIVATE_STATE_CORRUPT_DETECTED` — Wire ID 283.
    #[serde(rename = "PACKAGE_PRIVATE_STATE_CORRUPT_DETECTED")]
    PackagePrivateStateCorruptDetected,
    /// `PACKAGE_VERSION_DOWNGRADE_BLOCKED` — Wire ID 284.
    #[serde(rename = "PACKAGE_VERSION_DOWNGRADE_BLOCKED")]
    PackageVersionDowngradeBlocked,
    /// `PACKAGE_OBJECT_RETIRED` — Wire ID 285.
    #[serde(rename = "PACKAGE_OBJECT_RETIRED")]
    PackageObjectRetired,
    /// `PACKAGE_OBJECT_VERIFICATION_FAILED` — Wire ID 286.
    #[serde(rename = "PACKAGE_OBJECT_VERIFICATION_FAILED")]
    PackageObjectVerificationFailed,
    /// `PACKAGE_RECOVERY_RESTORE_PERFORMED` — Wire ID 287.
    #[serde(rename = "PACKAGE_RECOVERY_RESTORE_PERFORMED")]
    PackageRecoveryRestorePerformed,
    // ── S12.3 Compatibility Runtime (10) — §27.10 ──
    /// `APP_LAUNCH_STARTED` — Wire ID 288.
    #[serde(rename = "APP_LAUNCH_STARTED")]
    AppLaunchStarted,
    /// `APP_LAUNCH_SUCCEEDED` — Wire ID 289.
    #[serde(rename = "APP_LAUNCH_SUCCEEDED")]
    AppLaunchSucceeded,
    /// `APP_LAUNCH_FAILED` — Wire ID 290.
    #[serde(rename = "APP_LAUNCH_FAILED")]
    AppLaunchFailed,
    /// `WINE_PREFIX_CREATED` — Wire ID 291.
    #[serde(rename = "WINE_PREFIX_CREATED")]
    WinePrefixCreated,
    /// `WINE_PREFIX_BREAKOUT_ATTEMPTED` — Wire ID 292.
    #[serde(rename = "WINE_PREFIX_BREAKOUT_ATTEMPTED")]
    WinePrefixBreakoutAttempted,
    /// `WAYDROID_CONTAINER_STARTED` — Wire ID 293.
    #[serde(rename = "WAYDROID_CONTAINER_STARTED")]
    WaydroidContainerStarted,
    /// `WAYDROID_ESCAPE_ATTEMPTED` — Wire ID 294.
    #[serde(rename = "WAYDROID_ESCAPE_ATTEMPTED")]
    WaydroidEscapeAttempted,
    /// `KVM_VM_BOOTED` — Wire ID 295.
    #[serde(rename = "KVM_VM_BOOTED")]
    KvmVmBooted,
    /// `KVM_VM_TERMINATED` — Wire ID 296.
    #[serde(rename = "KVM_VM_TERMINATED")]
    KvmVmTerminated,
    /// `ORCHESTRATION_KIND_MISMATCH_REJECTED` — Wire ID 297.
    #[serde(rename = "ORCHESTRATION_KIND_MISMATCH_REJECTED")]
    OrchestrationKindMismatchRejected,
    // ── S12.4 Compatibility Knowledge (8) — §27.11 ──
    /// `PROFILE_CONTRIBUTED` — Wire ID 298.
    #[serde(rename = "PROFILE_CONTRIBUTED")]
    ProfileContributed,
    /// `PROFILE_RATING_AGGREGATED` — Wire ID 299.
    #[serde(rename = "PROFILE_RATING_AGGREGATED")]
    ProfileRatingAggregated,
    /// `PROFILE_OUTLIER_DETECTED` — Wire ID 300.
    #[serde(rename = "PROFILE_OUTLIER_DETECTED")]
    ProfileOutlierDetected,
    /// `PROFILE_RECOMMENDATION_SHOWN` — Wire ID 301.
    #[serde(rename = "PROFILE_RECOMMENDATION_SHOWN")]
    ProfileRecommendationShown,
    /// `PROFILE_IMPORTED_FROM_UPSTREAM` — Wire ID 302.
    #[serde(rename = "PROFILE_IMPORTED_FROM_UPSTREAM")]
    ProfileImportedFromUpstream,
    /// `PROFILE_REPUTATION_FARM_SUSPECTED` — Wire ID 303.
    #[serde(rename = "PROFILE_REPUTATION_FARM_SUSPECTED")]
    ProfileReputationFarmSuspected,
    /// `PROFILE_VISIBILITY_DOWNGRADED` — Wire ID 304.
    #[serde(rename = "PROFILE_VISIBILITY_DOWNGRADED")]
    ProfileVisibilityDowngraded,
    /// `PROFILE_RETIRED` — Wire ID 305.
    #[serde(rename = "PROFILE_RETIRED")]
    ProfileRetired,
    // ── S7.6 CLI Renderer (10) — §27.12 ──
    /// `CLI_RENDER_STARTED` — Wire ID 306.
    #[serde(rename = "CLI_RENDER_STARTED")]
    CliRenderStarted,
    /// `CLI_RENDER_FAILED` — Wire ID 307.
    #[serde(rename = "CLI_RENDER_FAILED")]
    CliRenderFailed,
    /// `CLI_NODE_KIND_UNSUPPORTED` — Wire ID 308.
    #[serde(rename = "CLI_NODE_KIND_UNSUPPORTED")]
    CliNodeKindUnsupported,
    /// `CLI_RECOVERY_KIND_REJECTED` — Wire ID 309.
    #[serde(rename = "CLI_RECOVERY_KIND_REJECTED")]
    CliRecoveryKindRejected,
    /// `CLI_AUTO_CONFIRM_REJECTED` — Wire ID 310.
    #[serde(rename = "CLI_AUTO_CONFIRM_REJECTED")]
    CliAutoConfirmRejected,
    /// `CLI_ANSI_INJECTION_BLOCKED` — Wire ID 311.
    #[serde(rename = "CLI_ANSI_INJECTION_BLOCKED")]
    CliAnsiInjectionBlocked,
    /// `CLI_DEGRADED_NO_TTY` — Wire ID 312.
    #[serde(rename = "CLI_DEGRADED_NO_TTY")]
    CliDegradedNoTty,
    /// `CLI_SCRIPTING_MODE_INVOKED` — Wire ID 313.
    #[serde(rename = "CLI_SCRIPTING_MODE_INVOKED")]
    CliScriptingModeInvoked,
    /// `CLI_OPERATOR_AUTHENTICATED` — Wire ID 314.
    #[serde(rename = "CLI_OPERATOR_AUTHENTICATED")]
    CliOperatorAuthenticated,
    /// `CLI_TRUST_INDICATOR_REORDERED` — Wire ID 315.
    #[serde(rename = "CLI_TRUST_INDICATOR_REORDERED")]
    CliTrustIndicatorReordered,
    // ── S8.3 Hardware Graph (14) — §27.13 ──
    /// `HARDWARE_GRAPH_REBUILT` — Wire ID 316.
    #[serde(rename = "HARDWARE_GRAPH_REBUILT")]
    HardwareGraphRebuilt,
    /// `DEVICE_DETECTED` — Wire ID 317.
    #[serde(rename = "DEVICE_DETECTED")]
    DeviceDetected,
    /// `DEVICE_DRIVER_BOUND` — Wire ID 318.
    #[serde(rename = "DEVICE_DRIVER_BOUND")]
    DeviceDriverBound,
    /// `DEVICE_DRIVER_REJECTED` — Wire ID 319.
    #[serde(rename = "DEVICE_DRIVER_REJECTED")]
    DeviceDriverRejected,
    /// `DEVICE_QUARANTINED` — Wire ID 320.
    #[serde(rename = "DEVICE_QUARANTINED")]
    DeviceQuarantined,
    /// `DEVICE_DISCONNECTED` — Wire ID 321.
    #[serde(rename = "DEVICE_DISCONNECTED")]
    DeviceDisconnected,
    /// `REMOVABLE_DEVICE_REQUEST` — Wire ID 322.
    #[serde(rename = "REMOVABLE_DEVICE_REQUEST")]
    RemovableDeviceRequest,
    /// `REMOVABLE_DEVICE_APPROVED` — Wire ID 323.
    #[serde(rename = "REMOVABLE_DEVICE_APPROVED")]
    RemovableDeviceApproved,
    /// `REMOVABLE_DEVICE_DENIED` — Wire ID 324.
    #[serde(rename = "REMOVABLE_DEVICE_DENIED")]
    RemovableDeviceDenied,
    /// `AI_REMOVABLE_DEVICE_BLOCKED` — Wire ID 325.
    #[serde(rename = "AI_REMOVABLE_DEVICE_BLOCKED")]
    AiRemovableDeviceBlocked,
    /// `HARDWARE_GRAPH_DRIFT_DETECTED` — Wire ID 326.
    #[serde(rename = "HARDWARE_GRAPH_DRIFT_DETECTED")]
    HardwareGraphDriftDetected,
    /// `FIRMWARE_VERSION_DOWNGRADE_BLOCKED` — Wire ID 327.
    #[serde(rename = "FIRMWARE_VERSION_DOWNGRADE_BLOCKED")]
    FirmwareVersionDowngradeBlocked,
    /// `IOMMU_DMA_PROTECTION_DEGRADED` — Wire ID 328.
    #[serde(rename = "IOMMU_DMA_PROTECTION_DEGRADED")]
    IommuDmaProtectionDegraded,
    /// `OUT_OF_TREE_DRIVER_BLOCKED` — Wire ID 329.
    #[serde(rename = "OUT_OF_TREE_DRIVER_BLOCKED")]
    OutOfTreeDriverBlocked,
    // ── S8.4 DNS / VPN Management (12) — §27.14 ──
    /// `DNS_QUERY_PERFORMED` — Wire ID 330.
    #[serde(rename = "DNS_QUERY_PERFORMED")]
    DnsQueryPerformed,
    /// `DNS_RESOLVER_REBINDING_DETECTED` — Wire ID 331.
    #[serde(rename = "DNS_RESOLVER_REBINDING_DETECTED")]
    DnsResolverRebindingDetected,
    /// `DNS_PLAIN_BLOCKED` — Wire ID 332.
    #[serde(rename = "DNS_PLAIN_BLOCKED")]
    DnsPlainBlocked,
    /// `DNS_RESOLVER_SUBSTITUTION_REJECTED` — Wire ID 333.
    #[serde(rename = "DNS_RESOLVER_SUBSTITUTION_REJECTED")]
    DnsResolverSubstitutionRejected,
    /// `VPN_TUNNEL_ESTABLISHED` — Wire ID 334.
    #[serde(rename = "VPN_TUNNEL_ESTABLISHED")]
    VpnTunnelEstablished,
    /// `VPN_TUNNEL_FAILED` — Wire ID 335.
    #[serde(rename = "VPN_TUNNEL_FAILED")]
    VpnTunnelFailed,
    /// `VPN_PROVIDER_KEY_ROTATED` — Wire ID 336.
    #[serde(rename = "VPN_PROVIDER_KEY_ROTATED")]
    VpnProviderKeyRotated,
    /// `VPN_PROVIDER_KEY_FORGERY_REJECTED` — Wire ID 337.
    #[serde(rename = "VPN_PROVIDER_KEY_FORGERY_REJECTED")]
    VpnProviderKeyForgeryRejected,
    /// `MDNS_REQUEST_RECEIVED` — Wire ID 338.
    #[serde(rename = "MDNS_REQUEST_RECEIVED")]
    MdnsRequestReceived,
    /// `MDNS_BROADCAST_DENIED` — Wire ID 339.
    #[serde(rename = "MDNS_BROADCAST_DENIED")]
    MdnsBroadcastDenied,
    /// `MDNS_POISONING_DETECTED` — Wire ID 340.
    #[serde(rename = "MDNS_POISONING_DETECTED")]
    MdnsPoisoningDetected,
    /// `RESOLVER_BACKEND_DEGRADED` — Wire ID 341.
    #[serde(rename = "RESOLVER_BACKEND_DEGRADED")]
    ResolverBackendDegraded,
    // ── S8.5 Firmware Trust (12) — §27.15 ──
    /// `FIRMWARE_UPDATE_REQUESTED` — Wire ID 342.
    #[serde(rename = "FIRMWARE_UPDATE_REQUESTED")]
    FirmwareUpdateRequested,
    /// `FIRMWARE_VERIFICATION_PASSED` — Wire ID 343.
    #[serde(rename = "FIRMWARE_VERIFICATION_PASSED")]
    FirmwareVerificationPassed,
    /// `FIRMWARE_VERIFICATION_FAILED` — Wire ID 344.
    #[serde(rename = "FIRMWARE_VERIFICATION_FAILED")]
    FirmwareVerificationFailed,
    /// `FIRMWARE_DOWNGRADE_BLOCKED` — Wire ID 345.
    #[serde(rename = "FIRMWARE_DOWNGRADE_BLOCKED")]
    FirmwareDowngradeBlocked,
    /// `FIRMWARE_UNSIGNED_REJECTED` — Wire ID 346.
    #[serde(rename = "FIRMWARE_UNSIGNED_REJECTED")]
    FirmwareUnsignedRejected,
    /// `FIRMWARE_VENDOR_DEPLATFORMED` — Wire ID 347.
    #[serde(rename = "FIRMWARE_VENDOR_DEPLATFORMED")]
    FirmwareVendorDeplatformed,
    /// `FIRMWARE_APPLIED` — Wire ID 348.
    #[serde(rename = "FIRMWARE_APPLIED")]
    FirmwareApplied,
    /// `FIRMWARE_APPLY_FAILED` — Wire ID 349.
    #[serde(rename = "FIRMWARE_APPLY_FAILED")]
    FirmwareApplyFailed,
    /// `FIRMWARE_ROLLBACK_PERFORMED` — Wire ID 350.
    #[serde(rename = "FIRMWARE_ROLLBACK_PERFORMED")]
    FirmwareRollbackPerformed,
    /// `BIOS_UEFI_UPDATE_DEFERRED` — Wire ID 351.
    #[serde(rename = "BIOS_UEFI_UPDATE_DEFERRED")]
    BiosUefiUpdateDeferred,
    /// `FIRMWARE_TAMPER_DETECTED` — Wire ID 352.
    #[serde(rename = "FIRMWARE_TAMPER_DETECTED")]
    FirmwareTamperDetected,
    /// `OPERATOR_LOCAL_FIRMWARE_INSTALLED` — Wire ID 353.
    #[serde(rename = "OPERATOR_LOCAL_FIRMWARE_INSTALLED")]
    OperatorLocalFirmwareInstalled,
    // ── S14.2 Telemetry Pipeline (10) — §27.16 ──
    /// `TELEMETRY_PIPELINE_STARTED` — Wire ID 354.
    #[serde(rename = "TELEMETRY_PIPELINE_STARTED")]
    TelemetryPipelineStarted,
    /// `TELEMETRY_CARDINALITY_BREACH` — Wire ID 355.
    #[serde(rename = "TELEMETRY_CARDINALITY_BREACH")]
    TelemetryCardinalityBreach,
    /// `TELEMETRY_REDACTION_FAILED` — Wire ID 356.
    #[serde(rename = "TELEMETRY_REDACTION_FAILED")]
    TelemetryRedactionFailed,
    /// `TELEMETRY_BACKEND_UNAVAILABLE` — Wire ID 357.
    #[serde(rename = "TELEMETRY_BACKEND_UNAVAILABLE")]
    TelemetryBackendUnavailable,
    /// `TELEMETRY_BACKEND_DEGRADED` — Wire ID 358.
    #[serde(rename = "TELEMETRY_BACKEND_DEGRADED")]
    TelemetryBackendDegraded,
    /// `TELEMETRY_LOG_INJECTION_DETECTED` — Wire ID 359.
    #[serde(rename = "TELEMETRY_LOG_INJECTION_DETECTED")]
    TelemetryLogInjectionDetected,
    /// `TELEMETRY_RETENTION_TIER_PROMOTED` — Wire ID 360.
    #[serde(rename = "TELEMETRY_RETENTION_TIER_PROMOTED")]
    TelemetryRetentionTierPromoted,
    /// `TELEMETRY_SAMPLING_RATE_ADJUSTED` — Wire ID 361.
    #[serde(rename = "TELEMETRY_SAMPLING_RATE_ADJUSTED")]
    TelemetrySamplingRateAdjusted,
    /// `TELEMETRY_EBPF_PROBE_LOADED` — Wire ID 362.
    #[serde(rename = "TELEMETRY_EBPF_PROBE_LOADED")]
    TelemetryEbpfProbeLoaded,
    /// `TELEMETRY_EBPF_PROBE_REJECTED` — Wire ID 363.
    #[serde(rename = "TELEMETRY_EBPF_PROBE_REJECTED")]
    TelemetryEbpfProbeRejected,
    // ── S11.2 Marketplace (12) — §27.17 ──
    /// `PUBLISHER_ONBOARDING_APPLICATION_SUBMITTED` — Wire ID 364.
    #[serde(rename = "PUBLISHER_ONBOARDING_APPLICATION_SUBMITTED")]
    PublisherOnboardingApplicationSubmitted,
    /// `PUBLISHER_ONBOARDING_IDENTITY_VERIFIED` — Wire ID 365.
    #[serde(rename = "PUBLISHER_ONBOARDING_IDENTITY_VERIFIED")]
    PublisherOnboardingIdentityVerified,
    /// `PUBLISHER_ONBOARDING_APPROVED` — Wire ID 366.
    #[serde(rename = "PUBLISHER_ONBOARDING_APPROVED")]
    PublisherOnboardingApproved,
    /// `PUBLISHER_ONBOARDING_REJECTED` — Wire ID 367.
    #[serde(rename = "PUBLISHER_ONBOARDING_REJECTED")]
    PublisherOnboardingRejected,
    /// `PUBLISHER_ONBOARDING_DEPLATFORMED` — Wire ID 368.
    #[serde(rename = "PUBLISHER_ONBOARDING_DEPLATFORMED")]
    PublisherOnboardingDeplatformed,
    /// `CAPABILITY_REVIEW_REQUESTED` — Wire ID 369.
    #[serde(rename = "CAPABILITY_REVIEW_REQUESTED")]
    CapabilityReviewRequested,
    /// `CAPABILITY_REVIEW_APPROVED` — Wire ID 370.
    #[serde(rename = "CAPABILITY_REVIEW_APPROVED")]
    CapabilityReviewApproved,
    /// `CAPABILITY_REVIEW_DECEPTIVE_REJECTED` — Wire ID 371.
    #[serde(rename = "CAPABILITY_REVIEW_DECEPTIVE_REJECTED")]
    CapabilityReviewDeceptiveRejected,
    /// `LISTING_PUBLISHED` — Wire ID 372.
    #[serde(rename = "LISTING_PUBLISHED")]
    ListingPublished,
    /// `LISTING_VISIBILITY_DOWNGRADED` — Wire ID 373.
    #[serde(rename = "LISTING_VISIBILITY_DOWNGRADED")]
    ListingVisibilityDowngraded,
    /// `LISTING_VS_MANIFEST_MISMATCH` — Wire ID 374.
    #[serde(rename = "LISTING_VS_MANIFEST_MISMATCH")]
    ListingVsManifestMismatch,
    /// `MARKETPLACE_REVIEW_BYPASS_ATTEMPTED` — Wire ID 375.
    #[serde(rename = "MARKETPLACE_REVIEW_BYPASS_ATTEMPTED")]
    MarketplaceReviewBypassAttempted,
    // ── S11.3 External Integrations (12) — §27.18 ──
    /// `BRIDGE_FETCH_STARTED` — Wire ID 376.
    #[serde(rename = "BRIDGE_FETCH_STARTED")]
    BridgeFetchStarted,
    /// `BRIDGE_FETCH_COMPLETED` — Wire ID 377.
    #[serde(rename = "BRIDGE_FETCH_COMPLETED")]
    BridgeFetchCompleted,
    /// `BRIDGE_UPSTREAM_SIGNATURE_VERIFIED` — Wire ID 378.
    #[serde(rename = "BRIDGE_UPSTREAM_SIGNATURE_VERIFIED")]
    BridgeUpstreamSignatureVerified,
    /// `BRIDGE_UPSTREAM_SIGNATURE_FAILED` — Wire ID 379.
    #[serde(rename = "BRIDGE_UPSTREAM_SIGNATURE_FAILED")]
    BridgeUpstreamSignatureFailed,
    /// `BRIDGE_REPACKAGED_WITH_AIOS_KEY` — Wire ID 380.
    #[serde(rename = "BRIDGE_REPACKAGED_WITH_AIOS_KEY")]
    BridgeRepackagedWithAiosKey,
    /// `BRIDGE_DECEPTIVE_REJECTED` — Wire ID 381.
    #[serde(rename = "BRIDGE_DECEPTIVE_REJECTED")]
    BridgeDeceptiveRejected,
    /// `BRIDGE_RATE_LIMIT_EXCEEDED` — Wire ID 382.
    #[serde(rename = "BRIDGE_RATE_LIMIT_EXCEEDED")]
    BridgeRateLimitExceeded,
    /// `BRIDGE_METADATA_IMPORTED` — Wire ID 383.
    #[serde(rename = "BRIDGE_METADATA_IMPORTED")]
    BridgeMetadataImported,
    /// `BRIDGE_RECIPE_IMPORTED` — Wire ID 384.
    #[serde(rename = "BRIDGE_RECIPE_IMPORTED")]
    BridgeRecipeImported,
    /// `BRIDGE_BLACKLISTED` — Wire ID 385.
    #[serde(rename = "BRIDGE_BLACKLISTED")]
    BridgeBlacklisted,
    /// `BRIDGE_DEGRADED_UPSTREAM_UNAVAILABLE` — Wire ID 386.
    #[serde(rename = "BRIDGE_DEGRADED_UPSTREAM_UNAVAILABLE")]
    BridgeDegradedUpstreamUnavailable,
    /// `BRIDGE_TRUST_CLASS_DECEPTION_DETECTED` — Wire ID 387.
    #[serde(rename = "BRIDGE_TRUST_CLASS_DECEPTION_DETECTED")]
    BridgeTrustClassDeceptionDetected,

    // ─── Wave 10 (§28): orphans + Wave 9 + Cluster 13 (IDs 388..=427) ───
    // ── S6.1 Status Taxonomy (1) — §28.1 ──
    /// `STATUS_TRANSITION` — Wire ID 388.
    #[serde(rename = "STATUS_TRANSITION")]
    StatusTransition,
    // ── S6.2 Evidence Grades (7) — §28.2 ──
    /// `ARTIFACT_RECORDED` — Wire ID 389.
    #[serde(rename = "ARTIFACT_RECORDED")]
    ArtifactRecorded,
    /// `BUILD_PASSED` — Wire ID 390.
    #[serde(rename = "BUILD_PASSED")]
    BuildPassed,
    /// `TEST_PASSED` — Wire ID 391.
    #[serde(rename = "TEST_PASSED")]
    TestPassed,
    /// `E2E_PASSED` — Wire ID 392.
    #[serde(rename = "E2E_PASSED")]
    E2ePassed,
    /// `RECOVERY_REHEARSAL_PASSED` — Wire ID 393.
    #[serde(rename = "RECOVERY_REHEARSAL_PASSED")]
    RecoveryRehearsalPassed,
    /// `RELEASE_GATE_PASSED` — Wire ID 394.
    #[serde(rename = "RELEASE_GATE_PASSED")]
    ReleaseGatePassed,
    /// `OPERATIONAL_HEALTHY` — Wire ID 395.
    #[serde(rename = "OPERATIONAL_HEALTHY")]
    OperationalHealthy,
    // ── S6.4 Invariants (2) — §28.3 ──
    /// `INVARIANT_BUNDLE_LOADED` — Wire ID 396.
    #[serde(rename = "INVARIANT_BUNDLE_LOADED")]
    InvariantBundleLoaded,
    /// `WEB_EXPOSURE_GRANTED` — Wire ID 397.
    #[serde(rename = "WEB_EXPOSURE_GRANTED")]
    WebExposureGranted,
    // ── S5.1 Identity Model (2) — §28.4 ──
    /// `IDENTITY_BUNDLE_LOADED` — Wire ID 398.
    #[serde(rename = "IDENTITY_BUNDLE_LOADED")]
    IdentityBundleLoaded,
    /// `GROUP_REGISTERED` — Wire ID 399.
    #[serde(rename = "GROUP_REGISTERED")]
    GroupRegistered,
    // ── S6.3 Evidence Receipt Schema orphans (4) — §28.5 ──
    /// `RECEIPT_FORGERY_DETECTED` — Wire ID 400.
    #[serde(rename = "RECEIPT_FORGERY_DETECTED")]
    ReceiptForgeryDetected,
    /// `RECEIPT_PAYLOAD_DUPLICATE_OBSERVED` — Wire ID 401.
    #[serde(rename = "RECEIPT_PAYLOAD_DUPLICATE_OBSERVED")]
    ReceiptPayloadDuplicateObserved,
    /// `RECEIPT_LINEAGE_DEPTH_EXCEEDED` — Wire ID 402.
    #[serde(rename = "RECEIPT_LINEAGE_DEPTH_EXCEEDED")]
    ReceiptLineageDepthExceeded,
    /// `RECEIPT_ORPHAN_ACTION_REF_DETECTED` — Wire ID 403.
    #[serde(rename = "RECEIPT_ORPHAN_ACTION_REF_DETECTED")]
    ReceiptOrphanActionRefDetected,
    // ── S14.1 Failure Handling (6) — §28.6 ──
    /// `INVARIANT_BUNDLE_REJECTED` — Wire ID 404.
    #[serde(rename = "INVARIANT_BUNDLE_REJECTED")]
    InvariantBundleRejected,
    /// `POLICY_BUNDLE_REJECTED` — Wire ID 405.
    #[serde(rename = "POLICY_BUNDLE_REJECTED")]
    PolicyBundleRejected,
    /// `IDENTITY_BUNDLE_REJECTED` — Wire ID 406.
    #[serde(rename = "IDENTITY_BUNDLE_REJECTED")]
    IdentityBundleRejected,
    /// `CAPABILITY_BUNDLE_REJECTED` — Wire ID 407.
    #[serde(rename = "CAPABILITY_BUNDLE_REJECTED")]
    CapabilityBundleRejected,
    /// `SANDBOX_BUNDLE_REJECTED` — Wire ID 408.
    #[serde(rename = "SANDBOX_BUNDLE_REJECTED")]
    SandboxBundleRejected,
    /// `FAILURE_OBSERVED_RATE_LIMITED` — Wire ID 409.
    #[serde(rename = "FAILURE_OBSERVED_RATE_LIMITED")]
    FailureObservedRateLimited,
    // ── S15.2 SGR State Transitions orphans (2) — §28.7 ──
    /// `GRAPH_EVALUATION_BUDGET_EXCEEDED` — Wire ID 410.
    #[serde(rename = "GRAPH_EVALUATION_BUDGET_EXCEEDED")]
    GraphEvaluationBudgetExceeded,
    /// `TRANSITION_BUDGET_EXCEEDED` — Wire ID 411.
    #[serde(rename = "TRANSITION_BUDGET_EXCEEDED")]
    TransitionBudgetExceeded,
    // ── S15.3 SGR Adapter Model orphan (1) — §28.8 ──
    /// `ADAPTER_LIFECYCLE_ILLEGAL_TRANSITION` — Wire ID 412.
    #[serde(rename = "ADAPTER_LIFECYCLE_ILLEGAL_TRANSITION")]
    AdapterLifecycleIllegalTransition,
    // ── S11.1 + S11.2 Repository Model + Marketplace (3) — §28.9 ──
    /// `PUBLISHER_TRUST_LEVEL_OBSERVED` — Wire ID 413.
    #[serde(rename = "PUBLISHER_TRUST_LEVEL_OBSERVED")]
    PublisherTrustLevelObserved,
    /// `PUBLISHER_KEY_COLLISION` — Wire ID 414.
    #[serde(rename = "PUBLISHER_KEY_COLLISION")]
    PublisherKeyCollision,
    /// `MARKETPLACE_REVIEW_BUDGET_EXCEEDED` — Wire ID 415.
    #[serde(rename = "MARKETPLACE_REVIEW_BUDGET_EXCEEDED")]
    MarketplaceReviewBudgetExceeded,
    // ── S11.3 External Integrations (4) — §28.10 ──
    /// `BRIDGE_OPERATOR_CONSENT_GRANTED` — Wire ID 416.
    #[serde(rename = "BRIDGE_OPERATOR_CONSENT_GRANTED")]
    BridgeOperatorConsentGranted,
    /// `BRIDGE_DEFERRED_NEEDS_REVIEW` — Wire ID 417.
    #[serde(rename = "BRIDGE_DEFERRED_NEEDS_REVIEW")]
    BridgeDeferredNeedsReview,
    /// `BRIDGE_METADATA_DRIFT_DETECTED` — Wire ID 418.
    #[serde(rename = "BRIDGE_METADATA_DRIFT_DETECTED")]
    BridgeMetadataDriftDetected,
    /// `BRIDGE_BLACKLIST_LIFTED` — Wire ID 419.
    #[serde(rename = "BRIDGE_BLACKLIST_LIFTED")]
    BridgeBlacklistLifted,
    // ── Wave 9 W9-A — S2.3 Policy Kernel hard-deny (1) — §28.11 ──
    /// `HARDWARE_SUBSTRATE_ACCEPT_OUTSIDE_RECOVERY_BLOCKED` — Wire ID 420.
    #[serde(rename = "HARDWARE_SUBSTRATE_ACCEPT_OUTSIDE_RECOVERY_BLOCKED")]
    HardwareSubstrateAcceptOutsideRecoveryBlocked,
    // ── Wave 9 W9-B — S9.1 + S9.2 first-boot mode (1) — §28.12 ──
    /// `FIRST_BOOT_OPERATION` — Wire ID 421.
    #[serde(rename = "FIRST_BOOT_OPERATION")]
    FirstBootOperation,
    // ── Wave 9 W9-C — S5.2 Vault Broker bootstrap key + rekey (3) — §28.13 ──
    /// `VAULT_BOOTSTRAP_KEY_USED` — Wire ID 422.
    #[serde(rename = "VAULT_BOOTSTRAP_KEY_USED")]
    VaultBootstrapKeyUsed,
    /// `BOOTSTRAP_KEY_USE_AFTER_EXHAUST_BLOCKED` — Wire ID 423.
    #[serde(rename = "BOOTSTRAP_KEY_USE_AFTER_EXHAUST_BLOCKED")]
    BootstrapKeyUseAfterExhaustBlocked,
    /// `VAULT_REKEYED` — Wire ID 424.
    #[serde(rename = "VAULT_REKEYED")]
    VaultRekeyed,
    // ── Wave 9 W9-D — S8.3 + S8.5 hardware drift + TPM reseal (2) — §28.14 ──
    /// `HARDWARE_GRAPH_DRIFT_ACCEPTED` — Wire ID 425.
    #[serde(rename = "HARDWARE_GRAPH_DRIFT_ACCEPTED")]
    HardwareGraphDriftAccepted,
    /// `VAULT_TPM_RESEAL_REQUIRED` — Wire ID 426.
    #[serde(rename = "VAULT_TPM_RESEAL_REQUIRED")]
    VaultTpmResealRequired,
    // ── Cluster 13 — S13.1 Cognitive Lifecycle Positive Emission (1) — §28.15 ──
    /// `AGENT_LIFECYCLE_TRANSITIONED` — Wire ID 427.
    #[serde(rename = "AGENT_LIFECYCLE_TRANSITIONED")]
    AgentLifecycleTransitioned,

    // ─── Wave 14+ constitutional additions (reserved range 1000..=9999) ───
    //
    // T-015 / S3.1 §11.5: the compaction worker emits this record when an
    // eligible sealed segment requires explicit operator approval before its
    // receipts may be removed (operator-approval policy mode). Allocated at
    // the start of the reserved 1000..=9999 range per the §29 forward-growth
    // contract; the dense 1..=427 Wave 13 block is preserved unchanged.
    /// `COMPACTION_APPROVAL_REQUIRED` — Wire ID 1000 (T-015 / S3.1 §11.5).
    #[serde(rename = "COMPACTION_APPROVAL_REQUIRED")]
    CompactionApprovalRequired,
}

impl RecordType {
    /// Return the canonical `SCREAMING_SNAKE_CASE` wire string for this `RecordType`.
    ///
    /// Identical to what `serde_json::to_string(&record_type)` emits, minus the
    /// surrounding quotation marks.
    #[must_use]
    #[allow(
        clippy::too_many_lines,
        reason = "exhaustive 427-variant wire-name table; intentional 1:1 with S3.1 Appendix A"
    )]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            // ─── Original Appendix A (§4) ───
            Self::ActionReceived => "ACTION_RECEIVED",
            Self::TranslationCreated => "TRANSLATION_CREATED",
            Self::RoutingDecision => "ROUTING_DECISION",
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
            Self::GcPass => "GC_PASS",
            Self::QuarantineEvent => "QUARANTINE_EVENT",
            Self::ConflictEvent => "CONFLICT_EVENT",
            Self::EmergencyOverrideGrant => "EMERGENCY_OVERRIDE_GRANT",
            Self::PolicyBundleLoad => "POLICY_BUNDLE_LOAD",
            Self::SegmentSealed => "SEGMENT_SEALED",
            Self::ChainInconsistencyDetected => "CHAIN_INCONSISTENCY_DETECTED",
            Self::TamperDetected => "TAMPER_DETECTED",
            // ─── §23 Namespace integration (S4.1) ───
            Self::SystemAdminOperation => "SYSTEM_ADMIN_OPERATION",
            Self::CrossGroupAccessDenied => "CROSS_GROUP_ACCESS_DENIED",
            // ─── Wave 5 (§24): renderers + GPU ───
            Self::SurfaceCreated => "SURFACE_CREATED",
            Self::SurfaceDestroyed => "SURFACE_DESTROYED",
            Self::SurfaceGpuBudgetExceeded => "SURFACE_GPU_BUDGET_EXCEEDED",
            Self::CrossSurfaceReadDenied => "CROSS_SURFACE_READ_DENIED",
            Self::CrossZoneViolationAttempted => "CROSS_ZONE_VIOLATION_ATTEMPTED",
            Self::RecoveryKindRejected => "RECOVERY_KIND_REJECTED",
            Self::SurfaceNeverRendered => "SURFACE_NEVER_RENDERED",
            Self::UiTreeValidationRejected => "UI_TREE_VALIDATION_REJECTED",
            Self::UiTrustBearingAuthorshipRefused => "UI_TRUST_BEARING_AUTHORSHIP_REFUSED",
            Self::UiRecoveryNodeDropped => "UI_RECOVERY_NODE_DROPPED",
            Self::ThemeLoaded => "THEME_LOADED",
            Self::ThemeRejected => "THEME_REJECTED",
            Self::ThemeSwitched => "THEME_SWITCHED",
            Self::ThemeInvariantViolated => "THEME_INVARIANT_VIOLATED",
            Self::KdeRendererStarted => "KDE_RENDERER_STARTED",
            Self::KdeRendererDegraded => "KDE_RENDERER_DEGRADED",
            Self::KdeFrameDropped => "KDE_FRAME_DROPPED",
            Self::KdeLayerShellRejected => "KDE_LAYER_SHELL_REJECTED",
            Self::KdeKwinScriptLoaded => "KDE_KWIN_SCRIPT_LOADED",
            Self::KdeKwinScriptRejected => "KDE_KWIN_SCRIPT_REJECTED",
            Self::KdeRecoveryShellStarted => "KDE_RECOVERY_SHELL_STARTED",
            Self::KdeRecoveryKindRejectedAtRenderer => "KDE_RECOVERY_KIND_REJECTED_AT_RENDERER",
            Self::KdePlasmaThemeOverridden => "KDE_PLASMA_THEME_OVERRIDDEN",
            Self::KdeRenderFailed => "KDE_RENDER_FAILED",
            Self::KdeTokenFallbackUsed => "KDE_TOKEN_FALLBACK_USED",
            Self::WebLanExposureGranted => "WEB_LAN_EXPOSURE_GRANTED",
            Self::WebPublicExposureGranted => "WEB_PUBLIC_EXPOSURE_GRANTED",
            Self::WebRecoveryKindRejected => "WEB_RECOVERY_KIND_REJECTED",
            Self::WebPublicExposureFirewallRecorded => "WEB_PUBLIC_EXPOSURE_FIREWALL_RECORDED",
            Self::WebRecoveryPageLoaded => "WEB_RECOVERY_PAGE_LOADED",
            Self::WebRecoveryPageExited => "WEB_RECOVERY_PAGE_EXITED",
            Self::WebRendererStarted => "WEB_RENDERER_STARTED",
            Self::WebRendererDegraded => "WEB_RENDERER_DEGRADED",
            Self::WebLanExposureActive => "WEB_LAN_EXPOSURE_ACTIVE",
            Self::WebExposureRevoked => "WEB_EXPOSURE_REVOKED",
            Self::WebExtensionInterference => "WEB_EXTENSION_INTERFERENCE",
            Self::WebFullscreenRequested => "WEB_FULLSCREEN_REQUESTED",
            Self::WebThemeInjectionBlocked => "WEB_THEME_INJECTION_BLOCKED",
            Self::WebThemeFallbackUsed => "WEB_THEME_FALLBACK_USED",
            Self::WebClientStorageQuotaBreach => "WEB_CLIENT_STORAGE_QUOTA_BREACH",
            Self::WebRendererClsBreach => "WEB_RENDERER_CLS_BREACH",
            Self::WebConstitutionalElementReregisterBlocked => {
                "WEB_CONSTITUTIONAL_ELEMENT_REREGISTER_BLOCKED"
            }
            Self::GpuDeviceEnumerated => "GPU_DEVICE_ENUMERATED",
            Self::GpuDeviceDisconnected => "GPU_DEVICE_DISCONNECTED",
            Self::GpuVkDeviceCreated => "GPU_VK_DEVICE_CREATED",
            Self::GpuVkDeviceDestroyed => "GPU_VK_DEVICE_DESTROYED",
            Self::GpuDmabufGranted => "GPU_DMABUF_GRANTED",
            Self::GpuDmabufDenied => "GPU_DMABUF_DENIED",
            Self::GpuCapabilityDenied => "GPU_CAPABILITY_DENIED",
            Self::GpuValidationDisabledRecovery => "GPU_VALIDATION_DISABLED_RECOVERY",
            Self::GpuValidationEnabledNormal => "GPU_VALIDATION_ENABLED_NORMAL",
            Self::DriverUnavailable => "DRIVER_UNAVAILABLE",
            Self::GpuBudgetExceeded => "GPU_BUDGET_EXCEEDED",
            Self::GpuBudgetDowngraded => "GPU_BUDGET_DOWNGRADED",
            Self::IommuUnavailableDegraded => "IOMMU_UNAVAILABLE_DEGRADED",
            Self::HostCapabilityLie => "HOST_CAPABILITY_LIE",
            Self::GpuBindingForgery => "GPU_BINDING_FORGERY",
            Self::GpuDeviceForceReclaimed => "GPU_DEVICE_FORCE_RECLAIMED",
            // ─── Wave 6 (§25): vault / approval / override / recovery / capability runtime / network ───
            Self::VaultCapabilityIssued => "VAULT_CAPABILITY_ISSUED",
            Self::VaultCapabilityRotated => "VAULT_CAPABILITY_ROTATED",
            Self::VaultCapabilityRevoked => "VAULT_CAPABILITY_REVOKED",
            Self::VaultOperation => "VAULT_OPERATION",
            Self::VaultRawReveal => "VAULT_RAW_REVEAL",
            Self::VaultCapabilityForgery => "VAULT_CAPABILITY_FORGERY",
            Self::SubjectKindRejectedForVault => "SUBJECT_KIND_REJECTED_FOR_VAULT",
            Self::VaultRecoverySnapshotLoaded => "VAULT_RECOVERY_SNAPSHOT_LOADED",
            Self::ApprovalDelivered => "APPROVAL_DELIVERED",
            Self::ApprovalExpired => "APPROVAL_EXPIRED",
            Self::ApprovalConsumed => "APPROVAL_CONSUMED",
            Self::ApprovalRevoked => "APPROVAL_REVOKED",
            Self::ApprovalDeliveryFailed => "APPROVAL_DELIVERY_FAILED",
            Self::OverrideRequested => "OVERRIDE_REQUESTED",
            Self::OverrideQuorumReceived => "OVERRIDE_QUORUM_RECEIVED",
            Self::OverrideGranted => "OVERRIDE_GRANTED",
            Self::OverrideConsumed => "OVERRIDE_CONSUMED",
            Self::OverrideDenied => "OVERRIDE_DENIED",
            Self::OverrideExpired => "OVERRIDE_EXPIRED",
            Self::OverrideRevoked => "OVERRIDE_REVOKED",
            Self::OverrideReview => "OVERRIDE_REVIEW",
            Self::RecoveryBootEntered => "RECOVERY_BOOT_ENTERED",
            Self::RecoveryOperatorAuthenticated => "RECOVERY_OPERATOR_AUTHENTICATED",
            Self::RecoveryOperationPerformed => "RECOVERY_OPERATION_PERFORMED",
            Self::RecoveryTtlExpiredAutoReboot => "RECOVERY_TTL_EXPIRED_AUTO_REBOOT",
            Self::RecoveryBootExited => "RECOVERY_BOOT_EXITED",
            Self::RecoveryL5StartBlocked => "RECOVERY_L5_START_BLOCKED",
            Self::RecoveryNetworkLanEnabled => "RECOVERY_NETWORK_LAN_ENABLED",
            Self::RecoveryNetworkLanDisabled => "RECOVERY_NETWORK_LAN_DISABLED",
            Self::RecoveryForensicAttachPerformed => "RECOVERY_FORENSIC_ATTACH_PERFORMED",
            Self::BootFailureAutoRecoveryTriggered => "BOOT_FAILURE_AUTO_RECOVERY_TRIGGERED",
            Self::ActionValidated => "ACTION_VALIDATED",
            Self::ActionPolicyDecision => "ACTION_POLICY_DECISION",
            Self::ActionDispatched => "ACTION_DISPATCHED",
            Self::ExecutionSucceeded => "EXECUTION_SUCCEEDED",
            Self::ExecutionFailed => "EXECUTION_FAILED",
            Self::ExecutionVerificationFailed => "EXECUTION_VERIFICATION_FAILED",
            Self::RollbackAttempted => "ROLLBACK_ATTEMPTED",
            Self::RollbackSucceeded => "ROLLBACK_SUCCEEDED",
            Self::RollbackFailedRequiresOperator => "ROLLBACK_FAILED_REQUIRES_OPERATOR",
            Self::AdapterRegistered => "ADAPTER_REGISTERED",
            Self::AdapterRegistrationRejected => "ADAPTER_REGISTRATION_REJECTED",
            Self::AdapterDegraded => "ADAPTER_DEGRADED",
            Self::AdapterDeregistered => "ADAPTER_DEREGISTERED",
            Self::IdempotencyKeyReplayDetected => "IDEMPOTENCY_KEY_REPLAY_DETECTED",
            Self::BindingVoidedActionRevised => "BINDING_VOIDED_ACTION_REVISED",
            Self::AiInteractiveQueueDowngrade => "AI_INTERACTIVE_QUEUE_DOWNGRADE",
            Self::DryRunSimulationRecorded => "DRY_RUN_SIMULATION_RECORDED",
            Self::ExperimentalAdapterLiveDispatch => "EXPERIMENTAL_ADAPTER_LIVE_DISPATCH",
            Self::AdapterDeprecatedDispatch => "ADAPTER_DEPRECATED_DISPATCH",
            Self::NetworkPostureChanged => "NETWORK_POSTURE_CHANGED",
            Self::ExposureRequested => "EXPOSURE_REQUESTED",
            Self::ExposureGranted => "EXPOSURE_GRANTED",
            Self::ExposureDenied => "EXPOSURE_DENIED",
            Self::ExposureRevoked => "EXPOSURE_REVOKED",
            Self::ExposureTerminatedTtlExpired => "EXPOSURE_TERMINATED_TTL_EXPIRED",
            Self::PublicExposureHeartbeat => "PUBLIC_EXPOSURE_HEARTBEAT",
            Self::OutboundGrantIssued => "OUTBOUND_GRANT_ISSUED",
            Self::OutboundGrantRevoked => "OUTBOUND_GRANT_REVOKED",
            Self::OutboundOutsideManifest => "OUTBOUND_OUTSIDE_MANIFEST",
            Self::OutboundDegradedToLoopbackAuto => "OUTBOUND_DEGRADED_TO_LOOPBACK_AUTO",
            Self::AllowlistFqdnFanoutExceeded => "ALLOWLIST_FQDN_FANOUT_EXCEEDED",
            Self::LanSubnetDriftDetected => "LAN_SUBNET_DRIFT_DETECTED",
            Self::LanPeerDriftDetected => "LAN_PEER_DRIFT_DETECTED",
            Self::AiDirectInternetDenied => "AI_DIRECT_INTERNET_DENIED",
            Self::ExternalModelCallBrokered => "EXTERNAL_MODEL_CALL_BROKERED",
            Self::BackendDegradedNftablesToIptables => "BACKEND_DEGRADED_NFTABLES_TO_IPTABLES",
            Self::RawSocketBypassAttempted => "RAW_SOCKET_BYPASS_ATTEMPTED",
            // ─── Wave 7 (§26): repo / kernel pipeline / app runtime ───
            Self::PackageFetchStarted => "PACKAGE_FETCH_STARTED",
            Self::PackageVerified => "PACKAGE_VERIFIED",
            Self::PackageVerificationFailed => "PACKAGE_VERIFICATION_FAILED",
            Self::PackageApprovalRequested => "PACKAGE_APPROVAL_REQUESTED",
            Self::PackageInstalled => "PACKAGE_INSTALLED",
            Self::PackageInstallFailed => "PACKAGE_INSTALL_FAILED",
            Self::PackageQuarantined => "PACKAGE_QUARANTINED",
            Self::PackageUninstalled => "PACKAGE_UNINSTALLED",
            Self::PackageDowngradeBlocked => "PACKAGE_DOWNGRADE_BLOCKED",
            Self::CapabilityLieDetected => "CAPABILITY_LIE_DETECTED",
            Self::TrustChainBroken => "TRUST_CHAIN_BROKEN",
            Self::TrustChainTooDeep => "TRUST_CHAIN_TOO_DEEP",
            Self::ManifestForged => "MANIFEST_FORGED",
            Self::MirrorHashMismatchBlacklisted => "MIRROR_HASH_MISMATCH_BLACKLISTED",
            Self::PublisherKeyRotated => "PUBLISHER_KEY_ROTATED",
            Self::PublisherDeplatformed => "PUBLISHER_DEPLATFORMED",
            Self::ExternalBridgePackageAdmitted => "EXTERNAL_BRIDGE_PACKAGE_ADMITTED",
            Self::ExternalBridgeUpstreamSignatureFailed => {
                "EXTERNAL_BRIDGE_UPSTREAM_SIGNATURE_FAILED"
            }
            Self::AiosRootKeyRotated => "AIOS_ROOT_KEY_ROTATED",
            Self::KernelPipelineStarted => "KERNEL_PIPELINE_STARTED",
            Self::KernelBuildCompleted => "KERNEL_BUILD_COMPLETED",
            Self::KernelGateResult => "KERNEL_GATE_RESULT",
            Self::KernelConverged => "KERNEL_CONVERGED",
            Self::KernelDivergedRegression => "KERNEL_DIVERGED_REGRESSION",
            Self::KernelPromotedToA => "KERNEL_PROMOTED_TO_A",
            Self::KernelPromotedToB => "KERNEL_PROMOTED_TO_B",
            Self::KernelRollbackPerformed => "KERNEL_ROLLBACK_PERFORMED",
            Self::KernelImageObserved => "KERNEL_IMAGE_OBSERVED",
            Self::KernelImageDriftDetected => "KERNEL_IMAGE_DRIFT_DETECTED",
            Self::KernelRefreshScheduled => "KERNEL_REFRESH_SCHEDULED",
            Self::KernelRefreshPipelineFailed => "KERNEL_REFRESH_PIPELINE_FAILED",
            Self::PipelineDefinitionReplaced => "PIPELINE_DEFINITION_REPLACED",
            Self::AppObserveStarted => "APP_OBSERVE_STARTED",
            Self::AppObserveCompleted => "APP_OBSERVE_COMPLETED",
            Self::AppObserveTimeout => "APP_OBSERVE_TIMEOUT",
            Self::AppTranslateManifestProposed => "APP_TRANSLATE_MANIFEST_PROPOSED",
            Self::AppTranslateManifestApproved => "APP_TRANSLATE_MANIFEST_APPROVED",
            Self::AppTranslateManifestRejected => "APP_TRANSLATE_MANIFEST_REJECTED",
            Self::AppRecipeContributed => "APP_RECIPE_CONTRIBUTED",
            Self::AppRecipeImported => "APP_RECIPE_IMPORTED",
            Self::AppManifestDeltaProposed => "APP_MANIFEST_DELTA_PROPOSED",
            Self::AppManifestDeltaApproved => "APP_MANIFEST_DELTA_APPROVED",
            Self::AppHonestyClassViolation => "APP_HONESTY_CLASS_VIOLATION",
            Self::AppEcosystemRuntimeBreakoutAttempted => {
                "APP_ECOSYSTEM_RUNTIME_BREAKOUT_ATTEMPTED"
            }
            Self::AppAiDirectInstallAttemptedBlocked => "APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED",
            Self::AppRecipeDeceptiveRejectedAtIngest => "APP_RECIPE_DECEPTIVE_REJECTED_AT_INGEST",
            // ─── Wave 8 (§27): Tier 1+2 cross-spec consolidation ───
            Self::FirstBootStarted => "FIRST_BOOT_STARTED",
            Self::FirstBootStageCompleted => "FIRST_BOOT_STAGE_COMPLETED",
            Self::FirstBootFailed => "FIRST_BOOT_FAILED",
            Self::VaultRootKeyGenerated => "VAULT_ROOT_KEY_GENERATED",
            Self::AiProviderModeSet => "AI_PROVIDER_MODE_SET",
            Self::InitialFirewallPostureSet => "INITIAL_FIREWALL_POSTURE_SET",
            Self::FirstGroupRegistered => "FIRST_GROUP_REGISTERED",
            Self::FirstUserRegistered => "FIRST_USER_REGISTERED",
            Self::RecoveryOperatorRegistered => "RECOVERY_OPERATOR_REGISTERED",
            Self::FirstBootComplete => "FIRST_BOOT_COMPLETE",
            Self::ResetToFactoryInitiated => "RESET_TO_FACTORY_INITIATED",
            Self::FailureObserved => "FAILURE_OBSERVED",
            Self::DegradationLevelTransitioned => "DEGRADATION_LEVEL_TRANSITIONED",
            Self::ComponentRestarted => "COMPONENT_RESTARTED",
            Self::ComponentRestartBudgetExhausted => "COMPONENT_RESTART_BUDGET_EXHAUSTED",
            Self::CircuitBreakerOpened => "CIRCUIT_BREAKER_OPENED",
            Self::CircuitBreakerClosed => "CIRCUIT_BREAKER_CLOSED",
            Self::HaltedPendingOperator => "HALTED_PENDING_OPERATOR",
            Self::TimeDriftDetected => "TIME_DRIFT_DETECTED",
            Self::BackendVersionMismatch => "BACKEND_VERSION_MISMATCH",
            Self::RecoveryLoopDetected => "RECOVERY_LOOP_DETECTED",
            Self::ReceiptRedactionFailed => "RECEIPT_REDACTION_FAILED",
            Self::ReceiptIntegrityQuarantined => "RECEIPT_INTEGRITY_QUARANTINED",
            Self::ReceiptLineageCycleDetected => "RECEIPT_LINEAGE_CYCLE_DETECTED",
            Self::ReceiptSequenceOutOfOrder => "RECEIPT_SEQUENCE_OUT_OF_ORDER",
            Self::UnitRegistered => "UNIT_REGISTERED",
            Self::UnitStarted => "UNIT_STARTED",
            Self::UnitHealthy => "UNIT_HEALTHY",
            Self::UnitDegraded => "UNIT_DEGRADED",
            Self::UnitFailed => "UNIT_FAILED",
            Self::UnitStopped => "UNIT_STOPPED",
            Self::UnitRollbackTriggered => "UNIT_ROLLBACK_TRIGGERED",
            Self::UnitDependencyCycleDetected => "UNIT_DEPENDENCY_CYCLE_DETECTED",
            Self::GraphEvaluated => "GRAPH_EVALUATED",
            Self::TransitionQueued => "TRANSITION_QUEUED",
            Self::TransitionStarted => "TRANSITION_STARTED",
            Self::TransitionSucceeded => "TRANSITION_SUCCEEDED",
            Self::TransitionFailed => "TRANSITION_FAILED",
            Self::AbCanaryPromoted => "AB_CANARY_PROMOTED",
            Self::AbRollbackPerformed => "AB_ROLLBACK_PERFORMED",
            Self::DependencyCycleDetected => "DEPENDENCY_CYCLE_DETECTED",
            Self::TransitionConflict => "TRANSITION_CONFLICT",
            Self::ResourceBudgetDenied => "RESOURCE_BUDGET_DENIED",
            Self::GraphBlockedResource => "GRAPH_BLOCKED_RESOURCE",
            Self::GraphConverged => "GRAPH_CONVERGED",
            Self::AdapterRegistrationRequested => "ADAPTER_REGISTRATION_REQUESTED",
            Self::AdapterHealthy => "ADAPTER_HEALTHY",
            Self::AdapterActionKindViolation => "ADAPTER_ACTION_KIND_VIOLATION",
            Self::AdapterCapabilityViolation => "ADAPTER_CAPABILITY_VIOLATION",
            Self::AdapterHotReloaded => "ADAPTER_HOT_RELOADED",
            Self::AdapterDowngradeRejected => "ADAPTER_DOWNGRADE_REJECTED",
            Self::ModelInvocationStarted => "MODEL_INVOCATION_STARTED",
            Self::ModelInvocationSucceeded => "MODEL_INVOCATION_SUCCEEDED",
            Self::ModelInvocationFailed => "MODEL_INVOCATION_FAILED",
            Self::ModelBackendDegraded => "MODEL_BACKEND_DEGRADED",
            Self::ModelCircuitOpened => "MODEL_CIRCUIT_OPENED",
            Self::ModelPromptInjectionDetected => "MODEL_PROMPT_INJECTION_DETECTED",
            Self::ModelResponseSignatureFailed => "MODEL_RESPONSE_SIGNATURE_FAILED",
            Self::ModelVaultDeny => "MODEL_VAULT_DENY",
            Self::ModelNetworkDeny => "MODEL_NETWORK_DENY",
            Self::ModelRateLimited => "MODEL_RATE_LIMITED",
            Self::ModelBackendRegistered => "MODEL_BACKEND_REGISTERED",
            Self::ModelBackendRetired => "MODEL_BACKEND_RETIRED",
            Self::AgentRegistered => "AGENT_REGISTERED",
            Self::AgentRetired => "AGENT_RETIRED",
            Self::AgentInterruptedByRecovery => "AGENT_INTERRUPTED_BY_RECOVERY",
            Self::AgentProposalEmitted => "AGENT_PROPOSAL_EMITTED",
            Self::AgentProposalApproved => "AGENT_PROPOSAL_APPROVED",
            Self::AgentProposalDenied => "AGENT_PROPOSAL_DENIED",
            Self::AgentPlanBundledApproved => "AGENT_PLAN_BUNDLED_APPROVED",
            Self::AgentPlanAbandoned => "AGENT_PLAN_ABANDONED",
            Self::AgentMemoryWrite => "AGENT_MEMORY_WRITE",
            Self::AgentMemoryRead => "AGENT_MEMORY_READ",
            Self::AgentMemoryCrossUserDenied => "AGENT_MEMORY_CROSS_USER_DENIED",
            Self::AgentInterMessageSent => "AGENT_INTER_MESSAGE_SENT",
            Self::AgentInterMessageRejected => "AGENT_INTER_MESSAGE_REJECTED",
            Self::AgentSelfGradingBlocked => "AGENT_SELF_GRADING_BLOCKED",
            Self::AgentDirectFsWriteBlocked => "AGENT_DIRECT_FS_WRITE_BLOCKED",
            Self::AgentCrossGroupCoordinationBlocked => "AGENT_CROSS_GROUP_COORDINATION_BLOCKED",
            Self::AgentBackendDegraded => "AGENT_BACKEND_DEGRADED",
            Self::AgentPromptInjectionDetected => "AGENT_PROMPT_INJECTION_DETECTED",
            Self::PackageObjectCreated => "PACKAGE_OBJECT_CREATED",
            Self::PackageObjectUpdated => "PACKAGE_OBJECT_UPDATED",
            Self::PackageObjectRolledBack => "PACKAGE_OBJECT_ROLLED_BACK",
            Self::PackageObjectQuarantined => "PACKAGE_OBJECT_QUARANTINED",
            Self::PackagePrivateStateInitialized => "PACKAGE_PRIVATE_STATE_INITIALIZED",
            Self::PackagePrivateStateCorruptDetected => "PACKAGE_PRIVATE_STATE_CORRUPT_DETECTED",
            Self::PackageVersionDowngradeBlocked => "PACKAGE_VERSION_DOWNGRADE_BLOCKED",
            Self::PackageObjectRetired => "PACKAGE_OBJECT_RETIRED",
            Self::PackageObjectVerificationFailed => "PACKAGE_OBJECT_VERIFICATION_FAILED",
            Self::PackageRecoveryRestorePerformed => "PACKAGE_RECOVERY_RESTORE_PERFORMED",
            Self::AppLaunchStarted => "APP_LAUNCH_STARTED",
            Self::AppLaunchSucceeded => "APP_LAUNCH_SUCCEEDED",
            Self::AppLaunchFailed => "APP_LAUNCH_FAILED",
            Self::WinePrefixCreated => "WINE_PREFIX_CREATED",
            Self::WinePrefixBreakoutAttempted => "WINE_PREFIX_BREAKOUT_ATTEMPTED",
            Self::WaydroidContainerStarted => "WAYDROID_CONTAINER_STARTED",
            Self::WaydroidEscapeAttempted => "WAYDROID_ESCAPE_ATTEMPTED",
            Self::KvmVmBooted => "KVM_VM_BOOTED",
            Self::KvmVmTerminated => "KVM_VM_TERMINATED",
            Self::OrchestrationKindMismatchRejected => "ORCHESTRATION_KIND_MISMATCH_REJECTED",
            Self::ProfileContributed => "PROFILE_CONTRIBUTED",
            Self::ProfileRatingAggregated => "PROFILE_RATING_AGGREGATED",
            Self::ProfileOutlierDetected => "PROFILE_OUTLIER_DETECTED",
            Self::ProfileRecommendationShown => "PROFILE_RECOMMENDATION_SHOWN",
            Self::ProfileImportedFromUpstream => "PROFILE_IMPORTED_FROM_UPSTREAM",
            Self::ProfileReputationFarmSuspected => "PROFILE_REPUTATION_FARM_SUSPECTED",
            Self::ProfileVisibilityDowngraded => "PROFILE_VISIBILITY_DOWNGRADED",
            Self::ProfileRetired => "PROFILE_RETIRED",
            Self::CliRenderStarted => "CLI_RENDER_STARTED",
            Self::CliRenderFailed => "CLI_RENDER_FAILED",
            Self::CliNodeKindUnsupported => "CLI_NODE_KIND_UNSUPPORTED",
            Self::CliRecoveryKindRejected => "CLI_RECOVERY_KIND_REJECTED",
            Self::CliAutoConfirmRejected => "CLI_AUTO_CONFIRM_REJECTED",
            Self::CliAnsiInjectionBlocked => "CLI_ANSI_INJECTION_BLOCKED",
            Self::CliDegradedNoTty => "CLI_DEGRADED_NO_TTY",
            Self::CliScriptingModeInvoked => "CLI_SCRIPTING_MODE_INVOKED",
            Self::CliOperatorAuthenticated => "CLI_OPERATOR_AUTHENTICATED",
            Self::CliTrustIndicatorReordered => "CLI_TRUST_INDICATOR_REORDERED",
            Self::HardwareGraphRebuilt => "HARDWARE_GRAPH_REBUILT",
            Self::DeviceDetected => "DEVICE_DETECTED",
            Self::DeviceDriverBound => "DEVICE_DRIVER_BOUND",
            Self::DeviceDriverRejected => "DEVICE_DRIVER_REJECTED",
            Self::DeviceQuarantined => "DEVICE_QUARANTINED",
            Self::DeviceDisconnected => "DEVICE_DISCONNECTED",
            Self::RemovableDeviceRequest => "REMOVABLE_DEVICE_REQUEST",
            Self::RemovableDeviceApproved => "REMOVABLE_DEVICE_APPROVED",
            Self::RemovableDeviceDenied => "REMOVABLE_DEVICE_DENIED",
            Self::AiRemovableDeviceBlocked => "AI_REMOVABLE_DEVICE_BLOCKED",
            Self::HardwareGraphDriftDetected => "HARDWARE_GRAPH_DRIFT_DETECTED",
            Self::FirmwareVersionDowngradeBlocked => "FIRMWARE_VERSION_DOWNGRADE_BLOCKED",
            Self::IommuDmaProtectionDegraded => "IOMMU_DMA_PROTECTION_DEGRADED",
            Self::OutOfTreeDriverBlocked => "OUT_OF_TREE_DRIVER_BLOCKED",
            Self::DnsQueryPerformed => "DNS_QUERY_PERFORMED",
            Self::DnsResolverRebindingDetected => "DNS_RESOLVER_REBINDING_DETECTED",
            Self::DnsPlainBlocked => "DNS_PLAIN_BLOCKED",
            Self::DnsResolverSubstitutionRejected => "DNS_RESOLVER_SUBSTITUTION_REJECTED",
            Self::VpnTunnelEstablished => "VPN_TUNNEL_ESTABLISHED",
            Self::VpnTunnelFailed => "VPN_TUNNEL_FAILED",
            Self::VpnProviderKeyRotated => "VPN_PROVIDER_KEY_ROTATED",
            Self::VpnProviderKeyForgeryRejected => "VPN_PROVIDER_KEY_FORGERY_REJECTED",
            Self::MdnsRequestReceived => "MDNS_REQUEST_RECEIVED",
            Self::MdnsBroadcastDenied => "MDNS_BROADCAST_DENIED",
            Self::MdnsPoisoningDetected => "MDNS_POISONING_DETECTED",
            Self::ResolverBackendDegraded => "RESOLVER_BACKEND_DEGRADED",
            Self::FirmwareUpdateRequested => "FIRMWARE_UPDATE_REQUESTED",
            Self::FirmwareVerificationPassed => "FIRMWARE_VERIFICATION_PASSED",
            Self::FirmwareVerificationFailed => "FIRMWARE_VERIFICATION_FAILED",
            Self::FirmwareDowngradeBlocked => "FIRMWARE_DOWNGRADE_BLOCKED",
            Self::FirmwareUnsignedRejected => "FIRMWARE_UNSIGNED_REJECTED",
            Self::FirmwareVendorDeplatformed => "FIRMWARE_VENDOR_DEPLATFORMED",
            Self::FirmwareApplied => "FIRMWARE_APPLIED",
            Self::FirmwareApplyFailed => "FIRMWARE_APPLY_FAILED",
            Self::FirmwareRollbackPerformed => "FIRMWARE_ROLLBACK_PERFORMED",
            Self::BiosUefiUpdateDeferred => "BIOS_UEFI_UPDATE_DEFERRED",
            Self::FirmwareTamperDetected => "FIRMWARE_TAMPER_DETECTED",
            Self::OperatorLocalFirmwareInstalled => "OPERATOR_LOCAL_FIRMWARE_INSTALLED",
            Self::TelemetryPipelineStarted => "TELEMETRY_PIPELINE_STARTED",
            Self::TelemetryCardinalityBreach => "TELEMETRY_CARDINALITY_BREACH",
            Self::TelemetryRedactionFailed => "TELEMETRY_REDACTION_FAILED",
            Self::TelemetryBackendUnavailable => "TELEMETRY_BACKEND_UNAVAILABLE",
            Self::TelemetryBackendDegraded => "TELEMETRY_BACKEND_DEGRADED",
            Self::TelemetryLogInjectionDetected => "TELEMETRY_LOG_INJECTION_DETECTED",
            Self::TelemetryRetentionTierPromoted => "TELEMETRY_RETENTION_TIER_PROMOTED",
            Self::TelemetrySamplingRateAdjusted => "TELEMETRY_SAMPLING_RATE_ADJUSTED",
            Self::TelemetryEbpfProbeLoaded => "TELEMETRY_EBPF_PROBE_LOADED",
            Self::TelemetryEbpfProbeRejected => "TELEMETRY_EBPF_PROBE_REJECTED",
            Self::PublisherOnboardingApplicationSubmitted => {
                "PUBLISHER_ONBOARDING_APPLICATION_SUBMITTED"
            }
            Self::PublisherOnboardingIdentityVerified => "PUBLISHER_ONBOARDING_IDENTITY_VERIFIED",
            Self::PublisherOnboardingApproved => "PUBLISHER_ONBOARDING_APPROVED",
            Self::PublisherOnboardingRejected => "PUBLISHER_ONBOARDING_REJECTED",
            Self::PublisherOnboardingDeplatformed => "PUBLISHER_ONBOARDING_DEPLATFORMED",
            Self::CapabilityReviewRequested => "CAPABILITY_REVIEW_REQUESTED",
            Self::CapabilityReviewApproved => "CAPABILITY_REVIEW_APPROVED",
            Self::CapabilityReviewDeceptiveRejected => "CAPABILITY_REVIEW_DECEPTIVE_REJECTED",
            Self::ListingPublished => "LISTING_PUBLISHED",
            Self::ListingVisibilityDowngraded => "LISTING_VISIBILITY_DOWNGRADED",
            Self::ListingVsManifestMismatch => "LISTING_VS_MANIFEST_MISMATCH",
            Self::MarketplaceReviewBypassAttempted => "MARKETPLACE_REVIEW_BYPASS_ATTEMPTED",
            Self::BridgeFetchStarted => "BRIDGE_FETCH_STARTED",
            Self::BridgeFetchCompleted => "BRIDGE_FETCH_COMPLETED",
            Self::BridgeUpstreamSignatureVerified => "BRIDGE_UPSTREAM_SIGNATURE_VERIFIED",
            Self::BridgeUpstreamSignatureFailed => "BRIDGE_UPSTREAM_SIGNATURE_FAILED",
            Self::BridgeRepackagedWithAiosKey => "BRIDGE_REPACKAGED_WITH_AIOS_KEY",
            Self::BridgeDeceptiveRejected => "BRIDGE_DECEPTIVE_REJECTED",
            Self::BridgeRateLimitExceeded => "BRIDGE_RATE_LIMIT_EXCEEDED",
            Self::BridgeMetadataImported => "BRIDGE_METADATA_IMPORTED",
            Self::BridgeRecipeImported => "BRIDGE_RECIPE_IMPORTED",
            Self::BridgeBlacklisted => "BRIDGE_BLACKLISTED",
            Self::BridgeDegradedUpstreamUnavailable => "BRIDGE_DEGRADED_UPSTREAM_UNAVAILABLE",
            Self::BridgeTrustClassDeceptionDetected => "BRIDGE_TRUST_CLASS_DECEPTION_DETECTED",
            // ─── Wave 10 (§28): orphans + Wave 9 + Cluster 13 ───
            Self::StatusTransition => "STATUS_TRANSITION",
            Self::ArtifactRecorded => "ARTIFACT_RECORDED",
            Self::BuildPassed => "BUILD_PASSED",
            Self::TestPassed => "TEST_PASSED",
            Self::E2ePassed => "E2E_PASSED",
            Self::RecoveryRehearsalPassed => "RECOVERY_REHEARSAL_PASSED",
            Self::ReleaseGatePassed => "RELEASE_GATE_PASSED",
            Self::OperationalHealthy => "OPERATIONAL_HEALTHY",
            Self::InvariantBundleLoaded => "INVARIANT_BUNDLE_LOADED",
            Self::WebExposureGranted => "WEB_EXPOSURE_GRANTED",
            Self::IdentityBundleLoaded => "IDENTITY_BUNDLE_LOADED",
            Self::GroupRegistered => "GROUP_REGISTERED",
            Self::ReceiptForgeryDetected => "RECEIPT_FORGERY_DETECTED",
            Self::ReceiptPayloadDuplicateObserved => "RECEIPT_PAYLOAD_DUPLICATE_OBSERVED",
            Self::ReceiptLineageDepthExceeded => "RECEIPT_LINEAGE_DEPTH_EXCEEDED",
            Self::ReceiptOrphanActionRefDetected => "RECEIPT_ORPHAN_ACTION_REF_DETECTED",
            Self::InvariantBundleRejected => "INVARIANT_BUNDLE_REJECTED",
            Self::PolicyBundleRejected => "POLICY_BUNDLE_REJECTED",
            Self::IdentityBundleRejected => "IDENTITY_BUNDLE_REJECTED",
            Self::CapabilityBundleRejected => "CAPABILITY_BUNDLE_REJECTED",
            Self::SandboxBundleRejected => "SANDBOX_BUNDLE_REJECTED",
            Self::FailureObservedRateLimited => "FAILURE_OBSERVED_RATE_LIMITED",
            Self::GraphEvaluationBudgetExceeded => "GRAPH_EVALUATION_BUDGET_EXCEEDED",
            Self::TransitionBudgetExceeded => "TRANSITION_BUDGET_EXCEEDED",
            Self::AdapterLifecycleIllegalTransition => "ADAPTER_LIFECYCLE_ILLEGAL_TRANSITION",
            Self::PublisherTrustLevelObserved => "PUBLISHER_TRUST_LEVEL_OBSERVED",
            Self::PublisherKeyCollision => "PUBLISHER_KEY_COLLISION",
            Self::MarketplaceReviewBudgetExceeded => "MARKETPLACE_REVIEW_BUDGET_EXCEEDED",
            Self::BridgeOperatorConsentGranted => "BRIDGE_OPERATOR_CONSENT_GRANTED",
            Self::BridgeDeferredNeedsReview => "BRIDGE_DEFERRED_NEEDS_REVIEW",
            Self::BridgeMetadataDriftDetected => "BRIDGE_METADATA_DRIFT_DETECTED",
            Self::BridgeBlacklistLifted => "BRIDGE_BLACKLIST_LIFTED",
            Self::HardwareSubstrateAcceptOutsideRecoveryBlocked => {
                "HARDWARE_SUBSTRATE_ACCEPT_OUTSIDE_RECOVERY_BLOCKED"
            }
            Self::FirstBootOperation => "FIRST_BOOT_OPERATION",
            Self::VaultBootstrapKeyUsed => "VAULT_BOOTSTRAP_KEY_USED",
            Self::BootstrapKeyUseAfterExhaustBlocked => "BOOTSTRAP_KEY_USE_AFTER_EXHAUST_BLOCKED",
            Self::VaultRekeyed => "VAULT_REKEYED",
            Self::HardwareGraphDriftAccepted => "HARDWARE_GRAPH_DRIFT_ACCEPTED",
            Self::VaultTpmResealRequired => "VAULT_TPM_RESEAL_REQUIRED",
            Self::AgentLifecycleTransitioned => "AGENT_LIFECYCLE_TRANSITIONED",
            // ─── Wave 14+ constitutional additions (reserved 1000..=9999) ───
            Self::CompactionApprovalRequired => "COMPACTION_APPROVAL_REQUIRED",
        }
    }

    /// Return the canonical [`RetentionClass`] for this `RecordType` per S3.1.
    ///
    /// Mapping sources:
    ///
    /// - IDs 1..=22 (original §4): per §13 default-retention table (`FOREVER` for
    ///   constitutional / tamper / recovery / override / segment seal /
    ///   chain-checkpoint / policy-bundle-load; `STANDARD_24M` for the time-bounded
    ///   `90`/`180`/`365`-day rows since the closed enum has no sub-`24M` class).
    /// - IDs 23..=427: explicit per-record `STANDARD_24M` / `EXTENDED_60M` /
    ///   `FOREVER` declarations in §23.3 / §24.1.* / §25.* / §26.* / §27.* / §28.*.
    ///
    /// Conflict resolution (Wave 8 §27.6 re-declares Wave 6 §25.5 records):
    /// per §29.3 the canonical (earlier) ID's retention is authoritative; e.g.
    /// `ADAPTER_DEGRADED` is `STANDARD_24M` per S10.1 §25.5, not `EXTENDED_60M`
    /// from S15.3 §27.6.
    #[must_use]
    #[allow(
        clippy::too_many_lines,
        reason = "exhaustive RecordType -> RetentionClass mapping per S3.1 closed contract; intentional"
    )]
    #[allow(
        clippy::match_same_arms,
        reason = "427 spec-citation arms intentionally kept per-record for audit traceability; collapsing same-retention arms via `|` would lose per-record S3.1 §X.Y citation discipline"
    )]
    pub const fn retention_class(self) -> RetentionClass {
        match self {
            // ─── Original Appendix A (§4) ───
            Self::ActionReceived => RetentionClass::Standard24M,
            Self::TranslationCreated => RetentionClass::Standard24M,
            Self::RoutingDecision => RetentionClass::Standard24M,
            Self::PolicyDecision => RetentionClass::Forever,
            Self::ApprovalRequested => RetentionClass::Standard24M,
            Self::ApprovalGranted => RetentionClass::Standard24M,
            Self::ApprovalDenied => RetentionClass::Extended60M,
            Self::ExecutionStarted => RetentionClass::Standard24M,
            Self::ExecutionCompleted => RetentionClass::Forever,
            Self::VerificationResult => RetentionClass::Standard24M,
            Self::RollbackCompleted => RetentionClass::Standard24M,
            Self::RecoveryEvent => RetentionClass::Forever,
            Self::ModelCall => RetentionClass::Standard24M,
            Self::ChainCheckpoint => RetentionClass::Forever,
            Self::GcPass => RetentionClass::Standard24M,
            Self::QuarantineEvent => RetentionClass::Standard24M,
            Self::ConflictEvent => RetentionClass::Standard24M,
            Self::EmergencyOverrideGrant => RetentionClass::Forever,
            Self::PolicyBundleLoad => RetentionClass::Forever,
            Self::SegmentSealed => RetentionClass::Forever,
            Self::ChainInconsistencyDetected => RetentionClass::Forever,
            Self::TamperDetected => RetentionClass::Forever,
            // ─── §23 Namespace integration (S4.1) ───
            Self::SystemAdminOperation => RetentionClass::Standard24M,
            Self::CrossGroupAccessDenied => RetentionClass::Standard24M,
            // ─── Wave 5 (§24): renderers + GPU ───
            Self::SurfaceCreated => RetentionClass::Standard24M,
            Self::SurfaceDestroyed => RetentionClass::Standard24M,
            Self::SurfaceGpuBudgetExceeded => RetentionClass::Extended60M,
            Self::CrossSurfaceReadDenied => RetentionClass::Forever,
            Self::CrossZoneViolationAttempted => RetentionClass::Extended60M,
            Self::RecoveryKindRejected => RetentionClass::Forever,
            Self::SurfaceNeverRendered => RetentionClass::Standard24M,
            Self::UiTreeValidationRejected => RetentionClass::Standard24M,
            Self::UiTrustBearingAuthorshipRefused => RetentionClass::Forever,
            Self::UiRecoveryNodeDropped => RetentionClass::Standard24M,
            Self::ThemeLoaded => RetentionClass::Standard24M,
            Self::ThemeRejected => RetentionClass::Extended60M,
            Self::ThemeSwitched => RetentionClass::Standard24M,
            Self::ThemeInvariantViolated => RetentionClass::Forever,
            Self::KdeRendererStarted => RetentionClass::Standard24M,
            Self::KdeRendererDegraded => RetentionClass::Forever,
            Self::KdeFrameDropped => RetentionClass::Standard24M,
            Self::KdeLayerShellRejected => RetentionClass::Forever,
            Self::KdeKwinScriptLoaded => RetentionClass::Standard24M,
            Self::KdeKwinScriptRejected => RetentionClass::Forever,
            Self::KdeRecoveryShellStarted => RetentionClass::Forever,
            Self::KdeRecoveryKindRejectedAtRenderer => RetentionClass::Forever,
            Self::KdePlasmaThemeOverridden => RetentionClass::Standard24M,
            Self::KdeRenderFailed => RetentionClass::Extended60M,
            Self::KdeTokenFallbackUsed => RetentionClass::Standard24M,
            Self::WebLanExposureGranted => RetentionClass::Forever,
            Self::WebPublicExposureGranted => RetentionClass::Forever,
            Self::WebRecoveryKindRejected => RetentionClass::Forever,
            Self::WebPublicExposureFirewallRecorded => RetentionClass::Forever,
            Self::WebRecoveryPageLoaded => RetentionClass::Extended60M,
            Self::WebRecoveryPageExited => RetentionClass::Extended60M,
            Self::WebRendererStarted => RetentionClass::Standard24M,
            Self::WebRendererDegraded => RetentionClass::Standard24M,
            Self::WebLanExposureActive => RetentionClass::Standard24M,
            Self::WebExposureRevoked => RetentionClass::Standard24M,
            Self::WebExtensionInterference => RetentionClass::Standard24M,
            Self::WebFullscreenRequested => RetentionClass::Standard24M,
            Self::WebThemeInjectionBlocked => RetentionClass::Standard24M,
            Self::WebThemeFallbackUsed => RetentionClass::Standard24M,
            Self::WebClientStorageQuotaBreach => RetentionClass::Standard24M,
            Self::WebRendererClsBreach => RetentionClass::Standard24M,
            Self::WebConstitutionalElementReregisterBlocked => RetentionClass::Standard24M,
            Self::GpuDeviceEnumerated => RetentionClass::Standard24M,
            Self::GpuDeviceDisconnected => RetentionClass::Standard24M,
            Self::GpuVkDeviceCreated => RetentionClass::Standard24M,
            Self::GpuVkDeviceDestroyed => RetentionClass::Standard24M,
            Self::GpuDmabufGranted => RetentionClass::Standard24M,
            Self::GpuDmabufDenied => RetentionClass::Standard24M,
            Self::GpuCapabilityDenied => RetentionClass::Standard24M,
            Self::GpuValidationDisabledRecovery => RetentionClass::Standard24M,
            Self::GpuValidationEnabledNormal => RetentionClass::Standard24M,
            Self::DriverUnavailable => RetentionClass::Standard24M,
            Self::GpuBudgetExceeded => RetentionClass::Extended60M,
            Self::GpuBudgetDowngraded => RetentionClass::Extended60M,
            Self::IommuUnavailableDegraded => RetentionClass::Extended60M,
            Self::HostCapabilityLie => RetentionClass::Forever,
            Self::GpuBindingForgery => RetentionClass::Forever,
            Self::GpuDeviceForceReclaimed => RetentionClass::Forever,
            // ─── Wave 6 (§25): vault / approval / override / recovery / capability runtime / network ───
            Self::VaultCapabilityIssued => RetentionClass::Standard24M,
            Self::VaultCapabilityRotated => RetentionClass::Standard24M,
            Self::VaultCapabilityRevoked => RetentionClass::Extended60M,
            Self::VaultOperation => RetentionClass::Standard24M,
            Self::VaultRawReveal => RetentionClass::Forever,
            Self::VaultCapabilityForgery => RetentionClass::Forever,
            Self::SubjectKindRejectedForVault => RetentionClass::Forever,
            Self::VaultRecoverySnapshotLoaded => RetentionClass::Forever,
            Self::ApprovalDelivered => RetentionClass::Standard24M,
            Self::ApprovalExpired => RetentionClass::Standard24M,
            Self::ApprovalConsumed => RetentionClass::Standard24M,
            Self::ApprovalRevoked => RetentionClass::Extended60M,
            Self::ApprovalDeliveryFailed => RetentionClass::Extended60M,
            Self::OverrideRequested => RetentionClass::Forever,
            Self::OverrideQuorumReceived => RetentionClass::Forever,
            Self::OverrideGranted => RetentionClass::Forever,
            Self::OverrideConsumed => RetentionClass::Forever,
            Self::OverrideDenied => RetentionClass::Forever,
            Self::OverrideExpired => RetentionClass::Forever,
            Self::OverrideRevoked => RetentionClass::Forever,
            Self::OverrideReview => RetentionClass::Forever,
            Self::RecoveryBootEntered => RetentionClass::Forever,
            Self::RecoveryOperatorAuthenticated => RetentionClass::Forever,
            Self::RecoveryOperationPerformed => RetentionClass::Forever,
            Self::RecoveryTtlExpiredAutoReboot => RetentionClass::Forever,
            Self::RecoveryBootExited => RetentionClass::Forever,
            Self::RecoveryL5StartBlocked => RetentionClass::Forever,
            Self::RecoveryNetworkLanEnabled => RetentionClass::Forever,
            Self::RecoveryNetworkLanDisabled => RetentionClass::Forever,
            Self::RecoveryForensicAttachPerformed => RetentionClass::Forever,
            Self::BootFailureAutoRecoveryTriggered => RetentionClass::Forever,
            Self::ActionValidated => RetentionClass::Standard24M,
            Self::ActionPolicyDecision => RetentionClass::Standard24M,
            Self::ActionDispatched => RetentionClass::Standard24M,
            Self::ExecutionSucceeded => RetentionClass::Standard24M,
            Self::ExecutionFailed => RetentionClass::Extended60M,
            Self::ExecutionVerificationFailed => RetentionClass::Extended60M,
            Self::RollbackAttempted => RetentionClass::Standard24M,
            Self::RollbackSucceeded => RetentionClass::Standard24M,
            Self::RollbackFailedRequiresOperator => RetentionClass::Forever,
            Self::AdapterRegistered => RetentionClass::Standard24M,
            Self::AdapterRegistrationRejected => RetentionClass::Forever,
            Self::AdapterDegraded => RetentionClass::Standard24M,
            Self::AdapterDeregistered => RetentionClass::Extended60M,
            Self::IdempotencyKeyReplayDetected => RetentionClass::Extended60M,
            Self::BindingVoidedActionRevised => RetentionClass::Forever,
            Self::AiInteractiveQueueDowngrade => RetentionClass::Standard24M,
            Self::DryRunSimulationRecorded => RetentionClass::Standard24M,
            Self::ExperimentalAdapterLiveDispatch => RetentionClass::Extended60M,
            Self::AdapterDeprecatedDispatch => RetentionClass::Standard24M,
            Self::NetworkPostureChanged => RetentionClass::Forever,
            Self::ExposureRequested => RetentionClass::Standard24M,
            Self::ExposureGranted => RetentionClass::Forever,
            Self::ExposureDenied => RetentionClass::Extended60M,
            Self::ExposureRevoked => RetentionClass::Extended60M,
            Self::ExposureTerminatedTtlExpired => RetentionClass::Extended60M,
            Self::PublicExposureHeartbeat => RetentionClass::Standard24M,
            Self::OutboundGrantIssued => RetentionClass::Standard24M,
            Self::OutboundGrantRevoked => RetentionClass::Extended60M,
            Self::OutboundOutsideManifest => RetentionClass::Forever,
            Self::OutboundDegradedToLoopbackAuto => RetentionClass::Forever,
            Self::AllowlistFqdnFanoutExceeded => RetentionClass::Extended60M,
            Self::LanSubnetDriftDetected => RetentionClass::Standard24M,
            Self::LanPeerDriftDetected => RetentionClass::Extended60M,
            Self::AiDirectInternetDenied => RetentionClass::Forever,
            Self::ExternalModelCallBrokered => RetentionClass::Standard24M,
            Self::BackendDegradedNftablesToIptables => RetentionClass::Forever,
            Self::RawSocketBypassAttempted => RetentionClass::Forever,
            // ─── Wave 7 (§26): repo / kernel pipeline / app runtime ───
            Self::PackageFetchStarted => RetentionClass::Standard24M,
            Self::PackageVerified => RetentionClass::Standard24M,
            Self::PackageVerificationFailed => RetentionClass::Extended60M,
            Self::PackageApprovalRequested => RetentionClass::Standard24M,
            Self::PackageInstalled => RetentionClass::Standard24M,
            Self::PackageInstallFailed => RetentionClass::Extended60M,
            Self::PackageQuarantined => RetentionClass::Forever,
            Self::PackageUninstalled => RetentionClass::Standard24M,
            Self::PackageDowngradeBlocked => RetentionClass::Extended60M,
            Self::CapabilityLieDetected => RetentionClass::Forever,
            Self::TrustChainBroken => RetentionClass::Forever,
            Self::TrustChainTooDeep => RetentionClass::Forever,
            Self::ManifestForged => RetentionClass::Forever,
            Self::MirrorHashMismatchBlacklisted => RetentionClass::Forever,
            Self::PublisherKeyRotated => RetentionClass::Forever,
            Self::PublisherDeplatformed => RetentionClass::Forever,
            Self::ExternalBridgePackageAdmitted => RetentionClass::Standard24M,
            Self::ExternalBridgeUpstreamSignatureFailed => RetentionClass::Extended60M,
            Self::AiosRootKeyRotated => RetentionClass::Forever,
            Self::KernelPipelineStarted => RetentionClass::Standard24M,
            Self::KernelBuildCompleted => RetentionClass::Standard24M,
            Self::KernelGateResult => RetentionClass::Standard24M,
            Self::KernelConverged => RetentionClass::Standard24M,
            Self::KernelDivergedRegression => RetentionClass::Extended60M,
            Self::KernelPromotedToA => RetentionClass::Forever,
            Self::KernelPromotedToB => RetentionClass::Forever,
            Self::KernelRollbackPerformed => RetentionClass::Forever,
            Self::KernelImageObserved => RetentionClass::Standard24M,
            Self::KernelImageDriftDetected => RetentionClass::Forever,
            Self::KernelRefreshScheduled => RetentionClass::Standard24M,
            Self::KernelRefreshPipelineFailed => RetentionClass::Extended60M,
            Self::PipelineDefinitionReplaced => RetentionClass::Forever,
            Self::AppObserveStarted => RetentionClass::Standard24M,
            Self::AppObserveCompleted => RetentionClass::Standard24M,
            Self::AppObserveTimeout => RetentionClass::Extended60M,
            Self::AppTranslateManifestProposed => RetentionClass::Standard24M,
            Self::AppTranslateManifestApproved => RetentionClass::Standard24M,
            Self::AppTranslateManifestRejected => RetentionClass::Extended60M,
            Self::AppRecipeContributed => RetentionClass::Standard24M,
            Self::AppRecipeImported => RetentionClass::Standard24M,
            Self::AppManifestDeltaProposed => RetentionClass::Standard24M,
            Self::AppManifestDeltaApproved => RetentionClass::Standard24M,
            Self::AppHonestyClassViolation => RetentionClass::Forever,
            Self::AppEcosystemRuntimeBreakoutAttempted => RetentionClass::Forever,
            Self::AppAiDirectInstallAttemptedBlocked => RetentionClass::Forever,
            Self::AppRecipeDeceptiveRejectedAtIngest => RetentionClass::Forever,
            // ─── Wave 8 (§27): Tier 1+2 cross-spec consolidation ───
            Self::FirstBootStarted => RetentionClass::Forever,
            Self::FirstBootStageCompleted => RetentionClass::Standard24M,
            Self::FirstBootFailed => RetentionClass::Forever,
            Self::VaultRootKeyGenerated => RetentionClass::Forever,
            Self::AiProviderModeSet => RetentionClass::Forever,
            Self::InitialFirewallPostureSet => RetentionClass::Forever,
            Self::FirstGroupRegistered => RetentionClass::Forever,
            Self::FirstUserRegistered => RetentionClass::Forever,
            Self::RecoveryOperatorRegistered => RetentionClass::Forever,
            Self::FirstBootComplete => RetentionClass::Forever,
            Self::ResetToFactoryInitiated => RetentionClass::Forever,
            Self::FailureObserved => RetentionClass::Standard24M,
            Self::DegradationLevelTransitioned => RetentionClass::Standard24M,
            Self::ComponentRestarted => RetentionClass::Standard24M,
            Self::ComponentRestartBudgetExhausted => RetentionClass::Forever,
            Self::CircuitBreakerOpened => RetentionClass::Extended60M,
            Self::CircuitBreakerClosed => RetentionClass::Standard24M,
            Self::HaltedPendingOperator => RetentionClass::Forever,
            Self::TimeDriftDetected => RetentionClass::Extended60M,
            Self::BackendVersionMismatch => RetentionClass::Forever,
            Self::RecoveryLoopDetected => RetentionClass::Forever,
            Self::ReceiptRedactionFailed => RetentionClass::Forever,
            Self::ReceiptIntegrityQuarantined => RetentionClass::Forever,
            Self::ReceiptLineageCycleDetected => RetentionClass::Forever,
            Self::ReceiptSequenceOutOfOrder => RetentionClass::Forever,
            Self::UnitRegistered => RetentionClass::Standard24M,
            Self::UnitStarted => RetentionClass::Standard24M,
            Self::UnitHealthy => RetentionClass::Standard24M,
            Self::UnitDegraded => RetentionClass::Extended60M,
            Self::UnitFailed => RetentionClass::Extended60M,
            Self::UnitStopped => RetentionClass::Standard24M,
            Self::UnitRollbackTriggered => RetentionClass::Extended60M,
            Self::UnitDependencyCycleDetected => RetentionClass::Forever,
            Self::GraphEvaluated => RetentionClass::Standard24M,
            Self::TransitionQueued => RetentionClass::Standard24M,
            Self::TransitionStarted => RetentionClass::Standard24M,
            Self::TransitionSucceeded => RetentionClass::Standard24M,
            Self::TransitionFailed => RetentionClass::Extended60M,
            Self::AbCanaryPromoted => RetentionClass::Standard24M,
            Self::AbRollbackPerformed => RetentionClass::Forever,
            Self::DependencyCycleDetected => RetentionClass::Forever,
            Self::TransitionConflict => RetentionClass::Forever,
            Self::ResourceBudgetDenied => RetentionClass::Extended60M,
            Self::GraphBlockedResource => RetentionClass::Standard24M,
            Self::GraphConverged => RetentionClass::Standard24M,
            Self::AdapterRegistrationRequested => RetentionClass::Standard24M,
            Self::AdapterHealthy => RetentionClass::Standard24M,
            Self::AdapterActionKindViolation => RetentionClass::Forever,
            Self::AdapterCapabilityViolation => RetentionClass::Forever,
            Self::AdapterHotReloaded => RetentionClass::Standard24M,
            Self::AdapterDowngradeRejected => RetentionClass::Forever,
            Self::ModelInvocationStarted => RetentionClass::Standard24M,
            Self::ModelInvocationSucceeded => RetentionClass::Standard24M,
            Self::ModelInvocationFailed => RetentionClass::Extended60M,
            Self::ModelBackendDegraded => RetentionClass::Extended60M,
            Self::ModelCircuitOpened => RetentionClass::Extended60M,
            Self::ModelPromptInjectionDetected => RetentionClass::Forever,
            Self::ModelResponseSignatureFailed => RetentionClass::Forever,
            Self::ModelVaultDeny => RetentionClass::Extended60M,
            Self::ModelNetworkDeny => RetentionClass::Extended60M,
            Self::ModelRateLimited => RetentionClass::Standard24M,
            Self::ModelBackendRegistered => RetentionClass::Standard24M,
            Self::ModelBackendRetired => RetentionClass::Extended60M,
            Self::AgentRegistered => RetentionClass::Standard24M,
            Self::AgentRetired => RetentionClass::Extended60M,
            Self::AgentInterruptedByRecovery => RetentionClass::Forever,
            Self::AgentProposalEmitted => RetentionClass::Standard24M,
            Self::AgentProposalApproved => RetentionClass::Standard24M,
            Self::AgentProposalDenied => RetentionClass::Extended60M,
            Self::AgentPlanBundledApproved => RetentionClass::Standard24M,
            Self::AgentPlanAbandoned => RetentionClass::Extended60M,
            Self::AgentMemoryWrite => RetentionClass::Standard24M,
            Self::AgentMemoryRead => RetentionClass::Standard24M,
            Self::AgentMemoryCrossUserDenied => RetentionClass::Forever,
            Self::AgentInterMessageSent => RetentionClass::Standard24M,
            Self::AgentInterMessageRejected => RetentionClass::Extended60M,
            Self::AgentSelfGradingBlocked => RetentionClass::Forever,
            Self::AgentDirectFsWriteBlocked => RetentionClass::Forever,
            Self::AgentCrossGroupCoordinationBlocked => RetentionClass::Forever,
            Self::AgentBackendDegraded => RetentionClass::Extended60M,
            Self::AgentPromptInjectionDetected => RetentionClass::Forever,
            Self::PackageObjectCreated => RetentionClass::Standard24M,
            Self::PackageObjectUpdated => RetentionClass::Standard24M,
            Self::PackageObjectRolledBack => RetentionClass::Forever,
            Self::PackageObjectQuarantined => RetentionClass::Forever,
            Self::PackagePrivateStateInitialized => RetentionClass::Standard24M,
            Self::PackagePrivateStateCorruptDetected => RetentionClass::Forever,
            Self::PackageVersionDowngradeBlocked => RetentionClass::Forever,
            Self::PackageObjectRetired => RetentionClass::Extended60M,
            Self::PackageObjectVerificationFailed => RetentionClass::Extended60M,
            Self::PackageRecoveryRestorePerformed => RetentionClass::Forever,
            Self::AppLaunchStarted => RetentionClass::Standard24M,
            Self::AppLaunchSucceeded => RetentionClass::Standard24M,
            Self::AppLaunchFailed => RetentionClass::Extended60M,
            Self::WinePrefixCreated => RetentionClass::Standard24M,
            Self::WinePrefixBreakoutAttempted => RetentionClass::Forever,
            Self::WaydroidContainerStarted => RetentionClass::Standard24M,
            Self::WaydroidEscapeAttempted => RetentionClass::Forever,
            Self::KvmVmBooted => RetentionClass::Standard24M,
            Self::KvmVmTerminated => RetentionClass::Standard24M,
            Self::OrchestrationKindMismatchRejected => RetentionClass::Forever,
            Self::ProfileContributed => RetentionClass::Standard24M,
            Self::ProfileRatingAggregated => RetentionClass::Standard24M,
            Self::ProfileOutlierDetected => RetentionClass::Extended60M,
            Self::ProfileRecommendationShown => RetentionClass::Standard24M,
            Self::ProfileImportedFromUpstream => RetentionClass::Standard24M,
            Self::ProfileReputationFarmSuspected => RetentionClass::Forever,
            Self::ProfileVisibilityDowngraded => RetentionClass::Extended60M,
            Self::ProfileRetired => RetentionClass::Extended60M,
            Self::CliRenderStarted => RetentionClass::Standard24M,
            Self::CliRenderFailed => RetentionClass::Extended60M,
            Self::CliNodeKindUnsupported => RetentionClass::Standard24M,
            Self::CliRecoveryKindRejected => RetentionClass::Forever,
            Self::CliAutoConfirmRejected => RetentionClass::Forever,
            Self::CliAnsiInjectionBlocked => RetentionClass::Forever,
            Self::CliDegradedNoTty => RetentionClass::Standard24M,
            Self::CliScriptingModeInvoked => RetentionClass::Standard24M,
            Self::CliOperatorAuthenticated => RetentionClass::Standard24M,
            Self::CliTrustIndicatorReordered => RetentionClass::Forever,
            Self::HardwareGraphRebuilt => RetentionClass::Standard24M,
            Self::DeviceDetected => RetentionClass::Standard24M,
            Self::DeviceDriverBound => RetentionClass::Standard24M,
            Self::DeviceDriverRejected => RetentionClass::Extended60M,
            Self::DeviceQuarantined => RetentionClass::Forever,
            Self::DeviceDisconnected => RetentionClass::Standard24M,
            Self::RemovableDeviceRequest => RetentionClass::Standard24M,
            Self::RemovableDeviceApproved => RetentionClass::Standard24M,
            Self::RemovableDeviceDenied => RetentionClass::Extended60M,
            Self::AiRemovableDeviceBlocked => RetentionClass::Forever,
            Self::HardwareGraphDriftDetected => RetentionClass::Forever,
            Self::FirmwareVersionDowngradeBlocked => RetentionClass::Forever,
            Self::IommuDmaProtectionDegraded => RetentionClass::Extended60M,
            Self::OutOfTreeDriverBlocked => RetentionClass::Forever,
            Self::DnsQueryPerformed => RetentionClass::Standard24M,
            Self::DnsResolverRebindingDetected => RetentionClass::Forever,
            Self::DnsPlainBlocked => RetentionClass::Forever,
            Self::DnsResolverSubstitutionRejected => RetentionClass::Forever,
            Self::VpnTunnelEstablished => RetentionClass::Standard24M,
            Self::VpnTunnelFailed => RetentionClass::Extended60M,
            Self::VpnProviderKeyRotated => RetentionClass::Forever,
            Self::VpnProviderKeyForgeryRejected => RetentionClass::Forever,
            Self::MdnsRequestReceived => RetentionClass::Standard24M,
            Self::MdnsBroadcastDenied => RetentionClass::Extended60M,
            Self::MdnsPoisoningDetected => RetentionClass::Forever,
            Self::ResolverBackendDegraded => RetentionClass::Extended60M,
            Self::FirmwareUpdateRequested => RetentionClass::Standard24M,
            Self::FirmwareVerificationPassed => RetentionClass::Standard24M,
            Self::FirmwareVerificationFailed => RetentionClass::Extended60M,
            Self::FirmwareDowngradeBlocked => RetentionClass::Forever,
            Self::FirmwareUnsignedRejected => RetentionClass::Forever,
            Self::FirmwareVendorDeplatformed => RetentionClass::Forever,
            Self::FirmwareApplied => RetentionClass::Standard24M,
            Self::FirmwareApplyFailed => RetentionClass::Extended60M,
            Self::FirmwareRollbackPerformed => RetentionClass::Forever,
            Self::BiosUefiUpdateDeferred => RetentionClass::Extended60M,
            Self::FirmwareTamperDetected => RetentionClass::Forever,
            Self::OperatorLocalFirmwareInstalled => RetentionClass::Forever,
            Self::TelemetryPipelineStarted => RetentionClass::Standard24M,
            Self::TelemetryCardinalityBreach => RetentionClass::Forever,
            Self::TelemetryRedactionFailed => RetentionClass::Forever,
            Self::TelemetryBackendUnavailable => RetentionClass::Extended60M,
            Self::TelemetryBackendDegraded => RetentionClass::Extended60M,
            Self::TelemetryLogInjectionDetected => RetentionClass::Forever,
            Self::TelemetryRetentionTierPromoted => RetentionClass::Standard24M,
            Self::TelemetrySamplingRateAdjusted => RetentionClass::Standard24M,
            Self::TelemetryEbpfProbeLoaded => RetentionClass::Standard24M,
            Self::TelemetryEbpfProbeRejected => RetentionClass::Forever,
            Self::PublisherOnboardingApplicationSubmitted => RetentionClass::Standard24M,
            Self::PublisherOnboardingIdentityVerified => RetentionClass::Extended60M,
            Self::PublisherOnboardingApproved => RetentionClass::Forever,
            Self::PublisherOnboardingRejected => RetentionClass::Forever,
            Self::PublisherOnboardingDeplatformed => RetentionClass::Forever,
            Self::CapabilityReviewRequested => RetentionClass::Standard24M,
            Self::CapabilityReviewApproved => RetentionClass::Extended60M,
            Self::CapabilityReviewDeceptiveRejected => RetentionClass::Forever,
            Self::ListingPublished => RetentionClass::Standard24M,
            Self::ListingVisibilityDowngraded => RetentionClass::Extended60M,
            Self::ListingVsManifestMismatch => RetentionClass::Forever,
            Self::MarketplaceReviewBypassAttempted => RetentionClass::Forever,
            Self::BridgeFetchStarted => RetentionClass::Standard24M,
            Self::BridgeFetchCompleted => RetentionClass::Standard24M,
            Self::BridgeUpstreamSignatureVerified => RetentionClass::Standard24M,
            Self::BridgeUpstreamSignatureFailed => RetentionClass::Forever,
            Self::BridgeRepackagedWithAiosKey => RetentionClass::Standard24M,
            Self::BridgeDeceptiveRejected => RetentionClass::Forever,
            Self::BridgeRateLimitExceeded => RetentionClass::Extended60M,
            Self::BridgeMetadataImported => RetentionClass::Standard24M,
            Self::BridgeRecipeImported => RetentionClass::Standard24M,
            Self::BridgeBlacklisted => RetentionClass::Forever,
            Self::BridgeDegradedUpstreamUnavailable => RetentionClass::Extended60M,
            Self::BridgeTrustClassDeceptionDetected => RetentionClass::Forever,
            // ─── Wave 10 (§28): orphans + Wave 9 + Cluster 13 ───
            Self::StatusTransition => RetentionClass::Standard24M,
            Self::ArtifactRecorded => RetentionClass::Standard24M,
            Self::BuildPassed => RetentionClass::Standard24M,
            Self::TestPassed => RetentionClass::Standard24M,
            Self::E2ePassed => RetentionClass::Extended60M,
            Self::RecoveryRehearsalPassed => RetentionClass::Forever,
            Self::ReleaseGatePassed => RetentionClass::Forever,
            Self::OperationalHealthy => RetentionClass::Standard24M,
            Self::InvariantBundleLoaded => RetentionClass::Forever,
            Self::WebExposureGranted => RetentionClass::Forever,
            Self::IdentityBundleLoaded => RetentionClass::Forever,
            Self::GroupRegistered => RetentionClass::Forever,
            Self::ReceiptForgeryDetected => RetentionClass::Forever,
            Self::ReceiptPayloadDuplicateObserved => RetentionClass::Standard24M,
            Self::ReceiptLineageDepthExceeded => RetentionClass::Standard24M,
            Self::ReceiptOrphanActionRefDetected => RetentionClass::Extended60M,
            Self::InvariantBundleRejected => RetentionClass::Forever,
            Self::PolicyBundleRejected => RetentionClass::Forever,
            Self::IdentityBundleRejected => RetentionClass::Forever,
            Self::CapabilityBundleRejected => RetentionClass::Forever,
            Self::SandboxBundleRejected => RetentionClass::Forever,
            Self::FailureObservedRateLimited => RetentionClass::Standard24M,
            Self::GraphEvaluationBudgetExceeded => RetentionClass::Extended60M,
            Self::TransitionBudgetExceeded => RetentionClass::Extended60M,
            Self::AdapterLifecycleIllegalTransition => RetentionClass::Forever,
            Self::PublisherTrustLevelObserved => RetentionClass::Standard24M,
            Self::PublisherKeyCollision => RetentionClass::Forever,
            Self::MarketplaceReviewBudgetExceeded => RetentionClass::Standard24M,
            Self::BridgeOperatorConsentGranted => RetentionClass::Standard24M,
            Self::BridgeDeferredNeedsReview => RetentionClass::Standard24M,
            Self::BridgeMetadataDriftDetected => RetentionClass::Extended60M,
            Self::BridgeBlacklistLifted => RetentionClass::Forever,
            Self::HardwareSubstrateAcceptOutsideRecoveryBlocked => RetentionClass::Forever,
            Self::FirstBootOperation => RetentionClass::Forever,
            Self::VaultBootstrapKeyUsed => RetentionClass::Forever,
            Self::BootstrapKeyUseAfterExhaustBlocked => RetentionClass::Forever,
            Self::VaultRekeyed => RetentionClass::Forever,
            Self::HardwareGraphDriftAccepted => RetentionClass::Forever,
            Self::VaultTpmResealRequired => RetentionClass::Forever,
            Self::AgentLifecycleTransitioned => RetentionClass::Standard24M,
            // ─── Wave 14+ constitutional additions (reserved 1000..=9999) ───
            // T-015 / §11.5: compaction-approval is constitutionally permanent
            // — the approval (or its absence) is part of the audit trail
            // forever.
            Self::CompactionApprovalRequired => RetentionClass::Forever,
        }
    }

    /// Return the stable wire ID (1..=427) for this `RecordType`.
    ///
    /// Mirrors the closed proto enum tag from S3.1 Appendix A. Useful for
    /// auditing and protobuf migration (Wave 14+).
    #[must_use]
    #[allow(
        clippy::too_many_lines,
        reason = "exhaustive 427-variant wire-ID table; 1:1 with S3.1 Appendix A"
    )]
    pub const fn wire_id(self) -> u16 {
        match self {
            // ─── Original Appendix A (§4) ───
            Self::ActionReceived => 1,
            Self::TranslationCreated => 2,
            Self::RoutingDecision => 3,
            Self::PolicyDecision => 4,
            Self::ApprovalRequested => 5,
            Self::ApprovalGranted => 6,
            Self::ApprovalDenied => 7,
            Self::ExecutionStarted => 8,
            Self::ExecutionCompleted => 9,
            Self::VerificationResult => 10,
            Self::RollbackCompleted => 11,
            Self::RecoveryEvent => 12,
            Self::ModelCall => 13,
            Self::ChainCheckpoint => 14,
            Self::GcPass => 15,
            Self::QuarantineEvent => 16,
            Self::ConflictEvent => 17,
            Self::EmergencyOverrideGrant => 18,
            Self::PolicyBundleLoad => 19,
            Self::SegmentSealed => 20,
            Self::ChainInconsistencyDetected => 21,
            Self::TamperDetected => 22,
            // ─── §23 Namespace integration (S4.1) ───
            Self::SystemAdminOperation => 23,
            Self::CrossGroupAccessDenied => 24,
            // ─── Wave 5 (§24): renderers + GPU ───
            Self::SurfaceCreated => 25,
            Self::SurfaceDestroyed => 26,
            Self::SurfaceGpuBudgetExceeded => 27,
            Self::CrossSurfaceReadDenied => 28,
            Self::CrossZoneViolationAttempted => 29,
            Self::RecoveryKindRejected => 30,
            Self::SurfaceNeverRendered => 31,
            Self::UiTreeValidationRejected => 32,
            Self::UiTrustBearingAuthorshipRefused => 33,
            Self::UiRecoveryNodeDropped => 34,
            Self::ThemeLoaded => 35,
            Self::ThemeRejected => 36,
            Self::ThemeSwitched => 37,
            Self::ThemeInvariantViolated => 38,
            Self::KdeRendererStarted => 39,
            Self::KdeRendererDegraded => 40,
            Self::KdeFrameDropped => 41,
            Self::KdeLayerShellRejected => 42,
            Self::KdeKwinScriptLoaded => 43,
            Self::KdeKwinScriptRejected => 44,
            Self::KdeRecoveryShellStarted => 45,
            Self::KdeRecoveryKindRejectedAtRenderer => 46,
            Self::KdePlasmaThemeOverridden => 47,
            Self::KdeRenderFailed => 48,
            Self::KdeTokenFallbackUsed => 49,
            Self::WebLanExposureGranted => 50,
            Self::WebPublicExposureGranted => 51,
            Self::WebRecoveryKindRejected => 52,
            Self::WebPublicExposureFirewallRecorded => 53,
            Self::WebRecoveryPageLoaded => 54,
            Self::WebRecoveryPageExited => 55,
            Self::WebRendererStarted => 56,
            Self::WebRendererDegraded => 57,
            Self::WebLanExposureActive => 58,
            Self::WebExposureRevoked => 59,
            Self::WebExtensionInterference => 60,
            Self::WebFullscreenRequested => 61,
            Self::WebThemeInjectionBlocked => 62,
            Self::WebThemeFallbackUsed => 63,
            Self::WebClientStorageQuotaBreach => 64,
            Self::WebRendererClsBreach => 65,
            Self::WebConstitutionalElementReregisterBlocked => 66,
            Self::GpuDeviceEnumerated => 67,
            Self::GpuDeviceDisconnected => 68,
            Self::GpuVkDeviceCreated => 69,
            Self::GpuVkDeviceDestroyed => 70,
            Self::GpuDmabufGranted => 71,
            Self::GpuDmabufDenied => 72,
            Self::GpuCapabilityDenied => 73,
            Self::GpuValidationDisabledRecovery => 74,
            Self::GpuValidationEnabledNormal => 75,
            Self::DriverUnavailable => 76,
            Self::GpuBudgetExceeded => 77,
            Self::GpuBudgetDowngraded => 78,
            Self::IommuUnavailableDegraded => 79,
            Self::HostCapabilityLie => 80,
            Self::GpuBindingForgery => 81,
            Self::GpuDeviceForceReclaimed => 82,
            // ─── Wave 6 (§25): vault / approval / override / recovery / capability runtime / network ───
            Self::VaultCapabilityIssued => 83,
            Self::VaultCapabilityRotated => 84,
            Self::VaultCapabilityRevoked => 85,
            Self::VaultOperation => 86,
            Self::VaultRawReveal => 87,
            Self::VaultCapabilityForgery => 88,
            Self::SubjectKindRejectedForVault => 89,
            Self::VaultRecoverySnapshotLoaded => 90,
            Self::ApprovalDelivered => 91,
            Self::ApprovalExpired => 92,
            Self::ApprovalConsumed => 93,
            Self::ApprovalRevoked => 94,
            Self::ApprovalDeliveryFailed => 95,
            Self::OverrideRequested => 96,
            Self::OverrideQuorumReceived => 97,
            Self::OverrideGranted => 98,
            Self::OverrideConsumed => 99,
            Self::OverrideDenied => 100,
            Self::OverrideExpired => 101,
            Self::OverrideRevoked => 102,
            Self::OverrideReview => 103,
            Self::RecoveryBootEntered => 104,
            Self::RecoveryOperatorAuthenticated => 105,
            Self::RecoveryOperationPerformed => 106,
            Self::RecoveryTtlExpiredAutoReboot => 107,
            Self::RecoveryBootExited => 108,
            Self::RecoveryL5StartBlocked => 109,
            Self::RecoveryNetworkLanEnabled => 110,
            Self::RecoveryNetworkLanDisabled => 111,
            Self::RecoveryForensicAttachPerformed => 112,
            Self::BootFailureAutoRecoveryTriggered => 113,
            Self::ActionValidated => 114,
            Self::ActionPolicyDecision => 115,
            Self::ActionDispatched => 116,
            Self::ExecutionSucceeded => 117,
            Self::ExecutionFailed => 118,
            Self::ExecutionVerificationFailed => 119,
            Self::RollbackAttempted => 120,
            Self::RollbackSucceeded => 121,
            Self::RollbackFailedRequiresOperator => 122,
            Self::AdapterRegistered => 123,
            Self::AdapterRegistrationRejected => 124,
            Self::AdapterDegraded => 125,
            Self::AdapterDeregistered => 126,
            Self::IdempotencyKeyReplayDetected => 127,
            Self::BindingVoidedActionRevised => 128,
            Self::AiInteractiveQueueDowngrade => 129,
            Self::DryRunSimulationRecorded => 130,
            Self::ExperimentalAdapterLiveDispatch => 131,
            Self::AdapterDeprecatedDispatch => 132,
            Self::NetworkPostureChanged => 133,
            Self::ExposureRequested => 134,
            Self::ExposureGranted => 135,
            Self::ExposureDenied => 136,
            Self::ExposureRevoked => 137,
            Self::ExposureTerminatedTtlExpired => 138,
            Self::PublicExposureHeartbeat => 139,
            Self::OutboundGrantIssued => 140,
            Self::OutboundGrantRevoked => 141,
            Self::OutboundOutsideManifest => 142,
            Self::OutboundDegradedToLoopbackAuto => 143,
            Self::AllowlistFqdnFanoutExceeded => 144,
            Self::LanSubnetDriftDetected => 145,
            Self::LanPeerDriftDetected => 146,
            Self::AiDirectInternetDenied => 147,
            Self::ExternalModelCallBrokered => 148,
            Self::BackendDegradedNftablesToIptables => 149,
            Self::RawSocketBypassAttempted => 150,
            // ─── Wave 7 (§26): repo / kernel pipeline / app runtime ───
            Self::PackageFetchStarted => 151,
            Self::PackageVerified => 152,
            Self::PackageVerificationFailed => 153,
            Self::PackageApprovalRequested => 154,
            Self::PackageInstalled => 155,
            Self::PackageInstallFailed => 156,
            Self::PackageQuarantined => 157,
            Self::PackageUninstalled => 158,
            Self::PackageDowngradeBlocked => 159,
            Self::CapabilityLieDetected => 160,
            Self::TrustChainBroken => 161,
            Self::TrustChainTooDeep => 162,
            Self::ManifestForged => 163,
            Self::MirrorHashMismatchBlacklisted => 164,
            Self::PublisherKeyRotated => 165,
            Self::PublisherDeplatformed => 166,
            Self::ExternalBridgePackageAdmitted => 167,
            Self::ExternalBridgeUpstreamSignatureFailed => 168,
            Self::AiosRootKeyRotated => 169,
            Self::KernelPipelineStarted => 170,
            Self::KernelBuildCompleted => 171,
            Self::KernelGateResult => 172,
            Self::KernelConverged => 173,
            Self::KernelDivergedRegression => 174,
            Self::KernelPromotedToA => 175,
            Self::KernelPromotedToB => 176,
            Self::KernelRollbackPerformed => 177,
            Self::KernelImageObserved => 178,
            Self::KernelImageDriftDetected => 179,
            Self::KernelRefreshScheduled => 180,
            Self::KernelRefreshPipelineFailed => 181,
            Self::PipelineDefinitionReplaced => 182,
            Self::AppObserveStarted => 183,
            Self::AppObserveCompleted => 184,
            Self::AppObserveTimeout => 185,
            Self::AppTranslateManifestProposed => 186,
            Self::AppTranslateManifestApproved => 187,
            Self::AppTranslateManifestRejected => 188,
            Self::AppRecipeContributed => 189,
            Self::AppRecipeImported => 190,
            Self::AppManifestDeltaProposed => 191,
            Self::AppManifestDeltaApproved => 192,
            Self::AppHonestyClassViolation => 193,
            Self::AppEcosystemRuntimeBreakoutAttempted => 194,
            Self::AppAiDirectInstallAttemptedBlocked => 195,
            Self::AppRecipeDeceptiveRejectedAtIngest => 196,
            // ─── Wave 8 (§27): Tier 1+2 cross-spec consolidation ───
            Self::FirstBootStarted => 197,
            Self::FirstBootStageCompleted => 198,
            Self::FirstBootFailed => 199,
            Self::VaultRootKeyGenerated => 200,
            Self::AiProviderModeSet => 201,
            Self::InitialFirewallPostureSet => 202,
            Self::FirstGroupRegistered => 203,
            Self::FirstUserRegistered => 204,
            Self::RecoveryOperatorRegistered => 205,
            Self::FirstBootComplete => 206,
            Self::ResetToFactoryInitiated => 207,
            Self::FailureObserved => 208,
            Self::DegradationLevelTransitioned => 209,
            Self::ComponentRestarted => 210,
            Self::ComponentRestartBudgetExhausted => 211,
            Self::CircuitBreakerOpened => 212,
            Self::CircuitBreakerClosed => 213,
            Self::HaltedPendingOperator => 214,
            Self::TimeDriftDetected => 215,
            Self::BackendVersionMismatch => 216,
            Self::RecoveryLoopDetected => 217,
            Self::ReceiptRedactionFailed => 218,
            Self::ReceiptIntegrityQuarantined => 219,
            Self::ReceiptLineageCycleDetected => 220,
            Self::ReceiptSequenceOutOfOrder => 221,
            Self::UnitRegistered => 222,
            Self::UnitStarted => 223,
            Self::UnitHealthy => 224,
            Self::UnitDegraded => 225,
            Self::UnitFailed => 226,
            Self::UnitStopped => 227,
            Self::UnitRollbackTriggered => 228,
            Self::UnitDependencyCycleDetected => 229,
            Self::GraphEvaluated => 230,
            Self::TransitionQueued => 231,
            Self::TransitionStarted => 232,
            Self::TransitionSucceeded => 233,
            Self::TransitionFailed => 234,
            Self::AbCanaryPromoted => 235,
            Self::AbRollbackPerformed => 236,
            Self::DependencyCycleDetected => 237,
            Self::TransitionConflict => 238,
            Self::ResourceBudgetDenied => 239,
            Self::GraphBlockedResource => 240,
            Self::GraphConverged => 241,
            Self::AdapterRegistrationRequested => 242,
            Self::AdapterHealthy => 243,
            Self::AdapterActionKindViolation => 244,
            Self::AdapterCapabilityViolation => 245,
            Self::AdapterHotReloaded => 246,
            Self::AdapterDowngradeRejected => 247,
            Self::ModelInvocationStarted => 248,
            Self::ModelInvocationSucceeded => 249,
            Self::ModelInvocationFailed => 250,
            Self::ModelBackendDegraded => 251,
            Self::ModelCircuitOpened => 252,
            Self::ModelPromptInjectionDetected => 253,
            Self::ModelResponseSignatureFailed => 254,
            Self::ModelVaultDeny => 255,
            Self::ModelNetworkDeny => 256,
            Self::ModelRateLimited => 257,
            Self::ModelBackendRegistered => 258,
            Self::ModelBackendRetired => 259,
            Self::AgentRegistered => 260,
            Self::AgentRetired => 261,
            Self::AgentInterruptedByRecovery => 262,
            Self::AgentProposalEmitted => 263,
            Self::AgentProposalApproved => 264,
            Self::AgentProposalDenied => 265,
            Self::AgentPlanBundledApproved => 266,
            Self::AgentPlanAbandoned => 267,
            Self::AgentMemoryWrite => 268,
            Self::AgentMemoryRead => 269,
            Self::AgentMemoryCrossUserDenied => 270,
            Self::AgentInterMessageSent => 271,
            Self::AgentInterMessageRejected => 272,
            Self::AgentSelfGradingBlocked => 273,
            Self::AgentDirectFsWriteBlocked => 274,
            Self::AgentCrossGroupCoordinationBlocked => 275,
            Self::AgentBackendDegraded => 276,
            Self::AgentPromptInjectionDetected => 277,
            Self::PackageObjectCreated => 278,
            Self::PackageObjectUpdated => 279,
            Self::PackageObjectRolledBack => 280,
            Self::PackageObjectQuarantined => 281,
            Self::PackagePrivateStateInitialized => 282,
            Self::PackagePrivateStateCorruptDetected => 283,
            Self::PackageVersionDowngradeBlocked => 284,
            Self::PackageObjectRetired => 285,
            Self::PackageObjectVerificationFailed => 286,
            Self::PackageRecoveryRestorePerformed => 287,
            Self::AppLaunchStarted => 288,
            Self::AppLaunchSucceeded => 289,
            Self::AppLaunchFailed => 290,
            Self::WinePrefixCreated => 291,
            Self::WinePrefixBreakoutAttempted => 292,
            Self::WaydroidContainerStarted => 293,
            Self::WaydroidEscapeAttempted => 294,
            Self::KvmVmBooted => 295,
            Self::KvmVmTerminated => 296,
            Self::OrchestrationKindMismatchRejected => 297,
            Self::ProfileContributed => 298,
            Self::ProfileRatingAggregated => 299,
            Self::ProfileOutlierDetected => 300,
            Self::ProfileRecommendationShown => 301,
            Self::ProfileImportedFromUpstream => 302,
            Self::ProfileReputationFarmSuspected => 303,
            Self::ProfileVisibilityDowngraded => 304,
            Self::ProfileRetired => 305,
            Self::CliRenderStarted => 306,
            Self::CliRenderFailed => 307,
            Self::CliNodeKindUnsupported => 308,
            Self::CliRecoveryKindRejected => 309,
            Self::CliAutoConfirmRejected => 310,
            Self::CliAnsiInjectionBlocked => 311,
            Self::CliDegradedNoTty => 312,
            Self::CliScriptingModeInvoked => 313,
            Self::CliOperatorAuthenticated => 314,
            Self::CliTrustIndicatorReordered => 315,
            Self::HardwareGraphRebuilt => 316,
            Self::DeviceDetected => 317,
            Self::DeviceDriverBound => 318,
            Self::DeviceDriverRejected => 319,
            Self::DeviceQuarantined => 320,
            Self::DeviceDisconnected => 321,
            Self::RemovableDeviceRequest => 322,
            Self::RemovableDeviceApproved => 323,
            Self::RemovableDeviceDenied => 324,
            Self::AiRemovableDeviceBlocked => 325,
            Self::HardwareGraphDriftDetected => 326,
            Self::FirmwareVersionDowngradeBlocked => 327,
            Self::IommuDmaProtectionDegraded => 328,
            Self::OutOfTreeDriverBlocked => 329,
            Self::DnsQueryPerformed => 330,
            Self::DnsResolverRebindingDetected => 331,
            Self::DnsPlainBlocked => 332,
            Self::DnsResolverSubstitutionRejected => 333,
            Self::VpnTunnelEstablished => 334,
            Self::VpnTunnelFailed => 335,
            Self::VpnProviderKeyRotated => 336,
            Self::VpnProviderKeyForgeryRejected => 337,
            Self::MdnsRequestReceived => 338,
            Self::MdnsBroadcastDenied => 339,
            Self::MdnsPoisoningDetected => 340,
            Self::ResolverBackendDegraded => 341,
            Self::FirmwareUpdateRequested => 342,
            Self::FirmwareVerificationPassed => 343,
            Self::FirmwareVerificationFailed => 344,
            Self::FirmwareDowngradeBlocked => 345,
            Self::FirmwareUnsignedRejected => 346,
            Self::FirmwareVendorDeplatformed => 347,
            Self::FirmwareApplied => 348,
            Self::FirmwareApplyFailed => 349,
            Self::FirmwareRollbackPerformed => 350,
            Self::BiosUefiUpdateDeferred => 351,
            Self::FirmwareTamperDetected => 352,
            Self::OperatorLocalFirmwareInstalled => 353,
            Self::TelemetryPipelineStarted => 354,
            Self::TelemetryCardinalityBreach => 355,
            Self::TelemetryRedactionFailed => 356,
            Self::TelemetryBackendUnavailable => 357,
            Self::TelemetryBackendDegraded => 358,
            Self::TelemetryLogInjectionDetected => 359,
            Self::TelemetryRetentionTierPromoted => 360,
            Self::TelemetrySamplingRateAdjusted => 361,
            Self::TelemetryEbpfProbeLoaded => 362,
            Self::TelemetryEbpfProbeRejected => 363,
            Self::PublisherOnboardingApplicationSubmitted => 364,
            Self::PublisherOnboardingIdentityVerified => 365,
            Self::PublisherOnboardingApproved => 366,
            Self::PublisherOnboardingRejected => 367,
            Self::PublisherOnboardingDeplatformed => 368,
            Self::CapabilityReviewRequested => 369,
            Self::CapabilityReviewApproved => 370,
            Self::CapabilityReviewDeceptiveRejected => 371,
            Self::ListingPublished => 372,
            Self::ListingVisibilityDowngraded => 373,
            Self::ListingVsManifestMismatch => 374,
            Self::MarketplaceReviewBypassAttempted => 375,
            Self::BridgeFetchStarted => 376,
            Self::BridgeFetchCompleted => 377,
            Self::BridgeUpstreamSignatureVerified => 378,
            Self::BridgeUpstreamSignatureFailed => 379,
            Self::BridgeRepackagedWithAiosKey => 380,
            Self::BridgeDeceptiveRejected => 381,
            Self::BridgeRateLimitExceeded => 382,
            Self::BridgeMetadataImported => 383,
            Self::BridgeRecipeImported => 384,
            Self::BridgeBlacklisted => 385,
            Self::BridgeDegradedUpstreamUnavailable => 386,
            Self::BridgeTrustClassDeceptionDetected => 387,
            // ─── Wave 10 (§28): orphans + Wave 9 + Cluster 13 ───
            Self::StatusTransition => 388,
            Self::ArtifactRecorded => 389,
            Self::BuildPassed => 390,
            Self::TestPassed => 391,
            Self::E2ePassed => 392,
            Self::RecoveryRehearsalPassed => 393,
            Self::ReleaseGatePassed => 394,
            Self::OperationalHealthy => 395,
            Self::InvariantBundleLoaded => 396,
            Self::WebExposureGranted => 397,
            Self::IdentityBundleLoaded => 398,
            Self::GroupRegistered => 399,
            Self::ReceiptForgeryDetected => 400,
            Self::ReceiptPayloadDuplicateObserved => 401,
            Self::ReceiptLineageDepthExceeded => 402,
            Self::ReceiptOrphanActionRefDetected => 403,
            Self::InvariantBundleRejected => 404,
            Self::PolicyBundleRejected => 405,
            Self::IdentityBundleRejected => 406,
            Self::CapabilityBundleRejected => 407,
            Self::SandboxBundleRejected => 408,
            Self::FailureObservedRateLimited => 409,
            Self::GraphEvaluationBudgetExceeded => 410,
            Self::TransitionBudgetExceeded => 411,
            Self::AdapterLifecycleIllegalTransition => 412,
            Self::PublisherTrustLevelObserved => 413,
            Self::PublisherKeyCollision => 414,
            Self::MarketplaceReviewBudgetExceeded => 415,
            Self::BridgeOperatorConsentGranted => 416,
            Self::BridgeDeferredNeedsReview => 417,
            Self::BridgeMetadataDriftDetected => 418,
            Self::BridgeBlacklistLifted => 419,
            Self::HardwareSubstrateAcceptOutsideRecoveryBlocked => 420,
            Self::FirstBootOperation => 421,
            Self::VaultBootstrapKeyUsed => 422,
            Self::BootstrapKeyUseAfterExhaustBlocked => 423,
            Self::VaultRekeyed => 424,
            Self::HardwareGraphDriftAccepted => 425,
            Self::VaultTpmResealRequired => 426,
            Self::AgentLifecycleTransitioned => 427,
            // ─── Wave 14+ constitutional additions (reserved 1000..=9999) ───
            // T-015 / §11.5: allocated at the start of the reserved range
            // so the dense 1..=427 Wave 13 block is preserved unchanged.
            Self::CompactionApprovalRequired => 1000,
        }
    }
}

/// Convenience alias around [`RecordType::retention_class`].
///
/// Provided per the T-008 contract for callers that prefer free-function style.
#[must_use]
pub const fn retention_class_for(record_type: RecordType) -> RetentionClass {
    record_type.retention_class()
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use strum::{EnumCount, IntoEnumIterator};

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

    // ---- RecordType — Wave 13 closed vocabulary --------------------------

    /// One representative record from each Wave / sub-section. Coverage probe:
    /// any drift between Rust identifier and spec wire name at any layer
    /// fails this test.
    const REPRESENTATIVE_VARIANTS: &[(RecordType, &str, RetentionClass, u16)] = &[
        // Original §4 (IDs 1..22)
        (
            RecordType::ActionReceived,
            "ACTION_RECEIVED",
            RetentionClass::Standard24M,
            1,
        ),
        (
            RecordType::TamperDetected,
            "TAMPER_DETECTED",
            RetentionClass::Forever,
            22,
        ),
        // §23 Namespace (23..24)
        (
            RecordType::SystemAdminOperation,
            "SYSTEM_ADMIN_OPERATION",
            RetentionClass::Standard24M,
            23,
        ),
        (
            RecordType::CrossGroupAccessDenied,
            "CROSS_GROUP_ACCESS_DENIED",
            RetentionClass::Standard24M,
            24,
        ),
        // Wave 5 (25..82)
        (
            RecordType::SurfaceCreated,
            "SURFACE_CREATED",
            RetentionClass::Standard24M,
            25,
        ),
        (
            RecordType::CrossSurfaceReadDenied,
            "CROSS_SURFACE_READ_DENIED",
            RetentionClass::Forever,
            28,
        ),
        (
            RecordType::ThemeInvariantViolated,
            "THEME_INVARIANT_VIOLATED",
            RetentionClass::Forever,
            38,
        ),
        (
            RecordType::KdeRendererStarted,
            "KDE_RENDERER_STARTED",
            RetentionClass::Standard24M,
            39,
        ),
        (
            RecordType::WebLanExposureGranted,
            "WEB_LAN_EXPOSURE_GRANTED",
            RetentionClass::Forever,
            50,
        ),
        (
            RecordType::GpuDeviceEnumerated,
            "GPU_DEVICE_ENUMERATED",
            RetentionClass::Standard24M,
            67,
        ),
        (
            RecordType::GpuDeviceForceReclaimed,
            "GPU_DEVICE_FORCE_RECLAIMED",
            RetentionClass::Forever,
            82,
        ),
        // Wave 6 (83..150)
        (
            RecordType::VaultCapabilityIssued,
            "VAULT_CAPABILITY_ISSUED",
            RetentionClass::Standard24M,
            83,
        ),
        (
            RecordType::VaultRawReveal,
            "VAULT_RAW_REVEAL",
            RetentionClass::Forever,
            87,
        ),
        (
            RecordType::OverrideRequested,
            "OVERRIDE_REQUESTED",
            RetentionClass::Forever,
            96,
        ),
        (
            RecordType::RecoveryBootEntered,
            "RECOVERY_BOOT_ENTERED",
            RetentionClass::Forever,
            104,
        ),
        (
            RecordType::AdapterRegistered,
            "ADAPTER_REGISTERED",
            RetentionClass::Standard24M,
            123,
        ),
        (
            RecordType::AdapterDegraded,
            "ADAPTER_DEGRADED",
            RetentionClass::Standard24M,
            125,
        ),
        (
            RecordType::BindingVoidedActionRevised,
            "BINDING_VOIDED_ACTION_REVISED",
            RetentionClass::Forever,
            128,
        ),
        (
            RecordType::NetworkPostureChanged,
            "NETWORK_POSTURE_CHANGED",
            RetentionClass::Forever,
            133,
        ),
        (
            RecordType::RawSocketBypassAttempted,
            "RAW_SOCKET_BYPASS_ATTEMPTED",
            RetentionClass::Forever,
            150,
        ),
        // Wave 7 (151..196)
        (
            RecordType::PackageFetchStarted,
            "PACKAGE_FETCH_STARTED",
            RetentionClass::Standard24M,
            151,
        ),
        (
            RecordType::AiosRootKeyRotated,
            "AIOS_ROOT_KEY_ROTATED",
            RetentionClass::Forever,
            169,
        ),
        (
            RecordType::KernelPipelineStarted,
            "KERNEL_PIPELINE_STARTED",
            RetentionClass::Standard24M,
            170,
        ),
        (
            RecordType::AppRecipeDeceptiveRejectedAtIngest,
            "APP_RECIPE_DECEPTIVE_REJECTED_AT_INGEST",
            RetentionClass::Forever,
            196,
        ),
        // Wave 8 (197..387)
        (
            RecordType::FirstBootStarted,
            "FIRST_BOOT_STARTED",
            RetentionClass::Forever,
            197,
        ),
        (
            RecordType::FailureObserved,
            "FAILURE_OBSERVED",
            RetentionClass::Standard24M,
            208,
        ),
        (
            RecordType::ReceiptRedactionFailed,
            "RECEIPT_REDACTION_FAILED",
            RetentionClass::Forever,
            218,
        ),
        (
            RecordType::UnitRegistered,
            "UNIT_REGISTERED",
            RetentionClass::Standard24M,
            222,
        ),
        (
            RecordType::DependencyCycleDetected,
            "DEPENDENCY_CYCLE_DETECTED",
            RetentionClass::Forever,
            237,
        ),
        (
            RecordType::AdapterCapabilityViolation,
            "ADAPTER_CAPABILITY_VIOLATION",
            RetentionClass::Forever,
            245,
        ),
        (
            RecordType::ModelInvocationStarted,
            "MODEL_INVOCATION_STARTED",
            RetentionClass::Standard24M,
            248,
        ),
        (
            RecordType::AgentRegistered,
            "AGENT_REGISTERED",
            RetentionClass::Standard24M,
            260,
        ),
        (
            RecordType::AgentSelfGradingBlocked,
            "AGENT_SELF_GRADING_BLOCKED",
            RetentionClass::Forever,
            273,
        ),
        (
            RecordType::PackageObjectCreated,
            "PACKAGE_OBJECT_CREATED",
            RetentionClass::Standard24M,
            278,
        ),
        (
            RecordType::WinePrefixBreakoutAttempted,
            "WINE_PREFIX_BREAKOUT_ATTEMPTED",
            RetentionClass::Forever,
            292,
        ),
        (
            RecordType::CliRenderStarted,
            "CLI_RENDER_STARTED",
            RetentionClass::Standard24M,
            306,
        ),
        (
            RecordType::HardwareGraphRebuilt,
            "HARDWARE_GRAPH_REBUILT",
            RetentionClass::Standard24M,
            316,
        ),
        (
            RecordType::OutOfTreeDriverBlocked,
            "OUT_OF_TREE_DRIVER_BLOCKED",
            RetentionClass::Forever,
            329,
        ),
        (
            RecordType::DnsResolverRebindingDetected,
            "DNS_RESOLVER_REBINDING_DETECTED",
            RetentionClass::Forever,
            331,
        ),
        (
            RecordType::FirmwareTamperDetected,
            "FIRMWARE_TAMPER_DETECTED",
            RetentionClass::Forever,
            352,
        ),
        (
            RecordType::TelemetryPipelineStarted,
            "TELEMETRY_PIPELINE_STARTED",
            RetentionClass::Standard24M,
            354,
        ),
        (
            RecordType::PublisherOnboardingApplicationSubmitted,
            "PUBLISHER_ONBOARDING_APPLICATION_SUBMITTED",
            RetentionClass::Standard24M,
            364,
        ),
        (
            RecordType::BridgeTrustClassDeceptionDetected,
            "BRIDGE_TRUST_CLASS_DECEPTION_DETECTED",
            RetentionClass::Forever,
            387,
        ),
        // Wave 10 (388..427)
        (
            RecordType::StatusTransition,
            "STATUS_TRANSITION",
            RetentionClass::Standard24M,
            388,
        ),
        (
            RecordType::BuildPassed,
            "BUILD_PASSED",
            RetentionClass::Standard24M,
            390,
        ),
        (
            RecordType::InvariantBundleLoaded,
            "INVARIANT_BUNDLE_LOADED",
            RetentionClass::Forever,
            396,
        ),
        (
            RecordType::WebExposureGranted,
            "WEB_EXPOSURE_GRANTED",
            RetentionClass::Forever,
            397,
        ),
        (
            RecordType::ReceiptForgeryDetected,
            "RECEIPT_FORGERY_DETECTED",
            RetentionClass::Forever,
            400,
        ),
        (
            RecordType::HardwareSubstrateAcceptOutsideRecoveryBlocked,
            "HARDWARE_SUBSTRATE_ACCEPT_OUTSIDE_RECOVERY_BLOCKED",
            RetentionClass::Forever,
            420,
        ),
        (
            RecordType::FirstBootOperation,
            "FIRST_BOOT_OPERATION",
            RetentionClass::Forever,
            421,
        ),
        (
            RecordType::VaultBootstrapKeyUsed,
            "VAULT_BOOTSTRAP_KEY_USED",
            RetentionClass::Forever,
            422,
        ),
        (
            RecordType::AgentLifecycleTransitioned,
            "AGENT_LIFECYCLE_TRANSITIONED",
            RetentionClass::Standard24M,
            427,
        ),
    ];

    /// Wave 13 closed vocabulary count: 427.
    /// T-015 / S3.1 §11.5 added `COMPACTION_APPROVAL_REQUIRED` (wire id 1000)
    /// as the first entry in the reserved Wave 14+ range; total enum variants
    /// is therefore 428. The §29.2 dense 1..=427 contract is preserved
    /// unchanged — Wave 14+ additions live in the reserved 1000..=9999 block
    /// and are counted separately by `WAVE_14_PLUS_VARIANTS`.
    const WAVE_13_VARIANTS: usize = 427;
    const WAVE_14_PLUS_VARIANTS: usize = 1;
    const TOTAL_RECORD_TYPE_VARIANTS: usize = WAVE_13_VARIANTS + WAVE_14_PLUS_VARIANTS;

    #[test]
    fn record_type_count_is_wave_13_plus_reserved_extensions() {
        assert_eq!(
            RecordType::COUNT,
            TOTAL_RECORD_TYPE_VARIANTS,
            "RecordType total = Wave 13 ({WAVE_13_VARIANTS}) + Wave 14+ reserved \
             ({WAVE_14_PLUS_VARIANTS})"
        );
    }

    #[test]
    fn every_variant_round_trips_through_serde_json() {
        // EnumIter walks every variant (Wave 13 + Wave 14+ reserved). Each
        // must serialize to its SCREAMING_SNAKE_CASE wire name and deserialize
        // back to the same variant.
        let mut count = 0_usize;
        for rt in RecordType::iter() {
            let s = serde_json::to_string(&rt).expect("ser");
            let back: RecordType = serde_json::from_str(&s).expect("de");
            assert_eq!(back, rt, "variant {rt:?} did not round-trip");
            // as_wire_str must match the serde token (minus quotes)
            assert_eq!(
                s,
                format!("\"{}\"", rt.as_wire_str()),
                "as_wire_str mismatch for {rt:?}"
            );
            count += 1;
        }
        assert_eq!(
            count, TOTAL_RECORD_TYPE_VARIANTS,
            "EnumIter walked {count} variants, expected {TOTAL_RECORD_TYPE_VARIANTS}"
        );
    }

    #[test]
    fn no_two_variants_share_a_wire_name() {
        let mut seen: HashSet<&'static str> = HashSet::with_capacity(TOTAL_RECORD_TYPE_VARIANTS);
        for rt in RecordType::iter() {
            let w = rt.as_wire_str();
            assert!(
                seen.insert(w),
                "duplicate wire name {w} — closed-vocabulary discipline broken"
            );
        }
        assert_eq!(seen.len(), TOTAL_RECORD_TYPE_VARIANTS);
    }

    #[test]
    fn no_two_variants_share_a_wire_id() {
        let mut seen: HashSet<u16> = HashSet::with_capacity(TOTAL_RECORD_TYPE_VARIANTS);
        for rt in RecordType::iter() {
            let id = rt.wire_id();
            assert!(
                seen.insert(id),
                "duplicate wire id {id} — Wave 13 ID-stability invariant broken"
            );
            // Wave 13 dense block: 1..=427. Wave 14+ reserved block:
            // 1000..=9999 (S3.1 §29 forward-growth contract).
            let in_wave_13 = (1..=427).contains(&id);
            let in_wave_14_reserved = (1000..=9999).contains(&id);
            assert!(
                in_wave_13 || in_wave_14_reserved,
                "wire id {id} is outside both the Wave 13 dense range (1..=427) and \
                 the Wave 14+ reserved range (1000..=9999) — §29 forward-growth \
                 contract broken"
            );
        }
        assert_eq!(seen.len(), TOTAL_RECORD_TYPE_VARIANTS);
    }

    #[test]
    fn representative_variants_have_expected_wire_name_retention_and_id() {
        for (rt, expected_wire, expected_retention, expected_id) in REPRESENTATIVE_VARIANTS {
            let s = serde_json::to_string(rt).expect("ser");
            assert_eq!(
                s,
                format!("\"{expected_wire}\""),
                "wire name drift for {rt:?}: expected {expected_wire}"
            );
            assert_eq!(rt.as_wire_str(), *expected_wire);
            assert_eq!(
                rt.retention_class(),
                *expected_retention,
                "retention drift for {rt:?}: expected {expected_retention:?}"
            );
            assert_eq!(retention_class_for(*rt), *expected_retention);
            assert_eq!(
                rt.wire_id(),
                *expected_id,
                "wire id drift for {rt:?}: expected {expected_id}"
            );
        }
    }

    #[test]
    fn original_22_variants_keep_wire_ids_1_through_22() {
        // §29 invariant: IDs 1..=22 are preserved verbatim from the original
        // Appendix A. Re-numbering ANY of these is the audit fail signal.
        let original_by_id: &[(u16, RecordType, &str)] = &[
            (1, RecordType::ActionReceived, "ACTION_RECEIVED"),
            (2, RecordType::TranslationCreated, "TRANSLATION_CREATED"),
            (3, RecordType::RoutingDecision, "ROUTING_DECISION"),
            (4, RecordType::PolicyDecision, "POLICY_DECISION"),
            (5, RecordType::ApprovalRequested, "APPROVAL_REQUESTED"),
            (6, RecordType::ApprovalGranted, "APPROVAL_GRANTED"),
            (7, RecordType::ApprovalDenied, "APPROVAL_DENIED"),
            (8, RecordType::ExecutionStarted, "EXECUTION_STARTED"),
            (9, RecordType::ExecutionCompleted, "EXECUTION_COMPLETED"),
            (10, RecordType::VerificationResult, "VERIFICATION_RESULT"),
            (11, RecordType::RollbackCompleted, "ROLLBACK_COMPLETED"),
            (12, RecordType::RecoveryEvent, "RECOVERY_EVENT"),
            (13, RecordType::ModelCall, "MODEL_CALL"),
            (14, RecordType::ChainCheckpoint, "CHAIN_CHECKPOINT"),
            (15, RecordType::GcPass, "GC_PASS"),
            (16, RecordType::QuarantineEvent, "QUARANTINE_EVENT"),
            (17, RecordType::ConflictEvent, "CONFLICT_EVENT"),
            (
                18,
                RecordType::EmergencyOverrideGrant,
                "EMERGENCY_OVERRIDE_GRANT",
            ),
            (19, RecordType::PolicyBundleLoad, "POLICY_BUNDLE_LOAD"),
            (20, RecordType::SegmentSealed, "SEGMENT_SEALED"),
            (
                21,
                RecordType::ChainInconsistencyDetected,
                "CHAIN_INCONSISTENCY_DETECTED",
            ),
            (22, RecordType::TamperDetected, "TAMPER_DETECTED"),
        ];
        assert_eq!(original_by_id.len(), 22);
        for (expected_id, rt, expected_wire) in original_by_id {
            assert_eq!(rt.wire_id(), *expected_id);
            assert_eq!(rt.as_wire_str(), *expected_wire);
        }
    }

    #[test]
    fn retention_distribution_matches_spec() {
        // Sanity check: every variant maps to one of the three classes; total
        // count must add up to TOTAL_RECORD_TYPE_VARIANTS. The per-class
        // counts derive from the spec's per-record tables and serve as a
        // regression sentinel — if any future edit silently re-classifies
        // a record, this number changes.
        //
        // T-015: +1 FOREVER for COMPACTION_APPROVAL_REQUIRED (Wave 14+
        // reserved-range constitutional addition).
        let mut s24 = 0_usize;
        let mut e60 = 0_usize;
        let mut fvr = 0_usize;
        for rt in RecordType::iter() {
            match rt.retention_class() {
                RetentionClass::Standard24M => s24 += 1,
                RetentionClass::Extended60M => e60 += 1,
                RetentionClass::Forever => fvr += 1,
            }
        }
        assert_eq!(s24 + e60 + fvr, TOTAL_RECORD_TYPE_VARIANTS);
        // These three numbers reflect the canonical S3.1 mapping at commit time
        // plus the T-015 +1 FOREVER addition.
        assert_eq!(s24, 175, "STANDARD_24M count drift");
        assert_eq!(e60, 78, "EXTENDED_60M count drift");
        assert_eq!(fvr, 174 + WAVE_14_PLUS_VARIANTS, "FOREVER count drift");
    }

    #[test]
    fn wave_13_id_range_is_contiguous_and_dense() {
        // §29.2 acceptance signal: the Wave 13 block of IDs 1..=427 is
        // densely allocated, no gaps. Wave 14+ additions (id >= 1000) are
        // counted separately and do not participate in this density check.
        let mut wave_13_ids: Vec<u16> = RecordType::iter()
            .map(RecordType::wire_id)
            .filter(|id| *id <= 427)
            .collect();
        wave_13_ids.sort_unstable();
        assert_eq!(wave_13_ids.len(), WAVE_13_VARIANTS);
        for (i, id) in wave_13_ids.iter().enumerate() {
            let expected = u16::try_from(i).expect("test fits in u16") + 1;
            assert_eq!(
                *id, expected,
                "gap or out-of-order in Wave 13 block at position {i}: got {id}, expected {expected}"
            );
        }
        assert_eq!(*wave_13_ids.first().expect("non-empty"), 1);
        assert_eq!(*wave_13_ids.last().expect("non-empty"), 427);
    }

    #[test]
    fn wave_14_plus_ids_live_in_the_reserved_block() {
        // §29 forward-growth contract: Wave 14+ additions land in the reserved
        // 1000..=9999 range. Each id in that band must be unique (covered by
        // `no_two_variants_share_a_wire_id`); here we verify they actually
        // sit in the reserved band and that the count matches the bookkeeping.
        let extended: Vec<u16> = RecordType::iter()
            .map(RecordType::wire_id)
            .filter(|id| *id > 427)
            .collect();
        assert_eq!(extended.len(), WAVE_14_PLUS_VARIANTS);
        for id in extended {
            assert!(
                (1000..=9999).contains(&id),
                "post-Wave-13 wire id {id} is not in the reserved 1000..=9999 range"
            );
        }
    }

    #[test]
    fn synonym_pairs_retained_as_distinct_variants_per_section_29_4() {
        // §29.4 explicitly retains four synonym/adjacency pairs as distinct
        // enum entries. This test pins that discipline so future edits cannot
        // silently collapse them.
        assert_ne!(
            RecordType::UnitDependencyCycleDetected.wire_id(),
            RecordType::DependencyCycleDetected.wire_id()
        );
        assert_eq!(RecordType::UnitDependencyCycleDetected.wire_id(), 229);
        assert_eq!(RecordType::DependencyCycleDetected.wire_id(), 237);

        assert_ne!(
            RecordType::PackageObjectQuarantined.wire_id(),
            RecordType::PackageQuarantined.wire_id()
        );
        assert_eq!(RecordType::PackageObjectQuarantined.wire_id(), 281);
        assert_eq!(RecordType::PackageQuarantined.wire_id(), 157);

        // Legacy coarse MODEL_CALL at ID 13 is retained alongside the
        // fine-grained S13.2 family at IDs 248..=259.
        assert_eq!(RecordType::ModelCall.wire_id(), 13);
        assert_eq!(RecordType::ModelInvocationStarted.wire_id(), 248);
        assert_eq!(RecordType::ModelBackendRetired.wire_id(), 259);

        assert_ne!(
            RecordType::OrchestrationKindMismatchRejected.wire_id(),
            RecordType::AppEcosystemRuntimeBreakoutAttempted.wire_id()
        );
    }

    #[test]
    fn t007_compatibility_subset_still_resolves() {
        // T-007 landed an initial 34-variant subset before T-008 expanded the
        // enum to the full 427. The 34 still-canonical names (one was the
        // synonym BINDING_VOIDED_ACTION_REVISED collapse target plus the
        // original §4 + §23 family) must still resolve, and the five non-
        // canonical names that T-007 ad-hoc-defined (EVIDENCE_LOG_TAMPER_DETECTED,
        // KERNEL_IMAGE_TAMPER_DETECTED, RECEIPT_SEALED, ACTION_FAILED,
        // ACTION_ROLLED_BACK) are gone per §29 closed-vocabulary discipline.
        let t007_canonical: &[(RecordType, &str)] = &[
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
            (RecordType::StatusTransition, "STATUS_TRANSITION"),
        ];
        assert_eq!(t007_canonical.len(), 34);
        for (rt, wire) in t007_canonical {
            assert_eq!(rt.as_wire_str(), *wire);
            let s = serde_json::to_string(rt).expect("ser");
            assert_eq!(s, format!("\"{wire}\""));
        }
    }

    #[test]
    fn retention_class_for_free_function_matches_method() {
        // The free-function alias must agree with the inherent method for
        // every variant; both surfaces are part of the public API.
        for rt in RecordType::iter() {
            assert_eq!(retention_class_for(rt), rt.retention_class());
        }
    }

    #[test]
    fn forever_retention_covers_every_constitutional_barrier_record() {
        // A curated set of constitutional / hard-deny / tamper records that
        // MUST be FOREVER per their spec sections. If any of these silently
        // shifts to STANDARD_24M / EXTENDED_60M, this test catches it.
        let forever_constitutional = [
            RecordType::TamperDetected,                                // §13
            RecordType::ChainInconsistencyDetected,                    // §13
            RecordType::RecoveryEvent,                                 // §13
            RecordType::EmergencyOverrideGrant,                        // §13
            RecordType::PolicyBundleLoad,                              // §13
            RecordType::SegmentSealed,                                 // §13
            RecordType::CrossSurfaceReadDenied,                        // §24.1.1
            RecordType::ThemeInvariantViolated,                        // §24.1.3
            RecordType::KdeRecoveryShellStarted,                       // §24.1.4
            RecordType::WebLanExposureGranted,                         // §24.1.5
            RecordType::WebPublicExposureGranted,                      // §24.1.5
            RecordType::HostCapabilityLie,                             // §24.1.6
            RecordType::GpuBindingForgery,                             // §24.1.6
            RecordType::VaultRawReveal,                                // §25.1
            RecordType::VaultCapabilityForgery,                        // §25.1
            RecordType::SubjectKindRejectedForVault,                   // §25.1
            RecordType::OverrideRequested,                             // §25.3 all FOREVER
            RecordType::OverrideGranted,                               // §25.3
            RecordType::OverrideDenied,                                // §25.3
            RecordType::RecoveryBootEntered,                           // §25.4 all FOREVER
            RecordType::RecoveryBootExited,                            // §25.4
            RecordType::BindingVoidedActionRevised,                    // §25.5
            RecordType::RawSocketBypassAttempted,                      // §25.6
            RecordType::AiDirectInternetDenied,                        // §25.6
            RecordType::CapabilityLieDetected,                         // §26.1
            RecordType::ManifestForged,                                // §26.1
            RecordType::AppEcosystemRuntimeBreakoutAttempted,          // §26.3
            RecordType::AppAiDirectInstallAttemptedBlocked,            // §26.3
            RecordType::WinePrefixBreakoutAttempted,                   // §27.10
            RecordType::WaydroidEscapeAttempted,                       // §27.10
            RecordType::FirmwareTamperDetected,                        // §27.15
            RecordType::ReceiptForgeryDetected,                        // §28.5
            RecordType::HardwareSubstrateAcceptOutsideRecoveryBlocked, // §28.11
            RecordType::VaultBootstrapKeyUsed,                         // §28.13
            RecordType::AgentSelfGradingBlocked,                       // §27.8
            RecordType::AgentDirectFsWriteBlocked,                     // §27.8
        ];
        for rt in forever_constitutional {
            assert_eq!(
                rt.retention_class(),
                RetentionClass::Forever,
                "{rt:?} must be FOREVER (constitutional barrier)"
            );
        }
    }

    #[test]
    fn extended_60m_retention_includes_known_budget_breach_records() {
        // §24.2 retention class summary lists 7 EXTENDED_60M records in Wave 5:
        // surface-budget, cross-zone, theme-rejected, kde-render-failed,
        // gpu-budget-exceeded, gpu-budget-downgraded, iommu-degraded.
        let extended_known = [
            RecordType::SurfaceGpuBudgetExceeded,
            RecordType::CrossZoneViolationAttempted,
            RecordType::ThemeRejected,
            RecordType::KdeRenderFailed,
            RecordType::GpuBudgetExceeded,
            RecordType::GpuBudgetDowngraded,
            RecordType::IommuUnavailableDegraded,
            // Wave 6+ EXTENDED_60M samples per §25.1 / §25.2
            RecordType::VaultCapabilityRevoked,
            RecordType::ApprovalDenied,
            RecordType::ApprovalRevoked,
            RecordType::ApprovalDeliveryFailed,
        ];
        for rt in extended_known {
            assert_eq!(
                rt.retention_class(),
                RetentionClass::Extended60M,
                "{rt:?} must be EXTENDED_60M per spec table"
            );
        }
    }

    #[test]
    fn standard_24m_retention_includes_routine_observability_records() {
        // Routine lifecycle records that must stay at STANDARD_24M baseline.
        let standard_known = [
            RecordType::SurfaceCreated,
            RecordType::SurfaceDestroyed,
            RecordType::KdeRendererStarted,
            RecordType::WebRendererStarted,
            RecordType::GpuDeviceEnumerated,
            RecordType::VaultCapabilityIssued,
            RecordType::ApprovalRequested,
            RecordType::ApprovalGranted,
            RecordType::AdapterRegistered,
            RecordType::AdapterDegraded, // §29.3: canonical Wave 6 ID wins, STANDARD_24M
            RecordType::PackageFetchStarted,
            RecordType::AppLaunchStarted,
            RecordType::CliRenderStarted,
            RecordType::TelemetryPipelineStarted,
            RecordType::FailureObserved,
        ];
        for rt in standard_known {
            assert_eq!(
                rt.retention_class(),
                RetentionClass::Standard24M,
                "{rt:?} must be STANDARD_24M per spec table"
            );
        }
    }

    #[test]
    fn wave_id_boundaries_match_spec_29_2_table() {
        // §29.2 table: each Wave occupies a contiguous ID range. Pin the
        // first + last variant of each range so re-ordering would fail.
        assert_eq!(RecordType::ActionReceived.wire_id(), 1); // Wave 0 first
        assert_eq!(RecordType::TamperDetected.wire_id(), 22); // Wave 0 last
        assert_eq!(RecordType::SystemAdminOperation.wire_id(), 23); // §23 first
        assert_eq!(RecordType::CrossGroupAccessDenied.wire_id(), 24); // §23 last
        assert_eq!(RecordType::SurfaceCreated.wire_id(), 25); // Wave 5 first
        assert_eq!(RecordType::GpuDeviceForceReclaimed.wire_id(), 82); // Wave 5 last
        assert_eq!(RecordType::VaultCapabilityIssued.wire_id(), 83); // Wave 6 first
        assert_eq!(RecordType::RawSocketBypassAttempted.wire_id(), 150); // Wave 6 last
        assert_eq!(RecordType::PackageFetchStarted.wire_id(), 151); // Wave 7 first
        assert_eq!(
            RecordType::AppRecipeDeceptiveRejectedAtIngest.wire_id(),
            196
        ); // Wave 7 last
        assert_eq!(RecordType::FirstBootStarted.wire_id(), 197); // Wave 8 first
        assert_eq!(RecordType::BridgeTrustClassDeceptionDetected.wire_id(), 387); // Wave 8 last
        assert_eq!(RecordType::StatusTransition.wire_id(), 388); // Wave 10 first
        assert_eq!(RecordType::AgentLifecycleTransitioned.wire_id(), 427); // Wave 10 last
    }

    #[test]
    fn wire_id_round_trips_via_iter() {
        // Given any wire ID in 1..=427, exactly one RecordType produces it.
        for target in 1_u16..=427 {
            let hits: Vec<RecordType> = RecordType::iter()
                .filter(|rt| rt.wire_id() == target)
                .collect();
            assert_eq!(
                hits.len(),
                1,
                "wire id {target} must map to exactly one RecordType, got {hits:?}"
            );
        }
    }

    #[test]
    fn original_22_retention_matches_section_13_table() {
        // §13 default-retention table maps the original 22 RecordTypes. Per
        // the closed RetentionClass enum (3-bucket) the spec's "Forever" rows
        // map FOREVER and the time-bounded rows (90/180/365 days) map to
        // STANDARD_24M. For records with split per-outcome retention
        // (POLICY_DECISION deny=forever, EXECUTION_COMPLETED failure=forever)
        // the stricter ceiling wins: FOREVER.
        let s13_table: &[(RecordType, RetentionClass)] = &[
            (RecordType::ActionReceived, RetentionClass::Standard24M),
            (RecordType::TranslationCreated, RetentionClass::Standard24M),
            (RecordType::RoutingDecision, RetentionClass::Standard24M),
            (RecordType::PolicyDecision, RetentionClass::Forever),
            (RecordType::ApprovalRequested, RetentionClass::Standard24M),
            (RecordType::ApprovalGranted, RetentionClass::Standard24M),
            (RecordType::ApprovalDenied, RetentionClass::Extended60M),
            (RecordType::ExecutionStarted, RetentionClass::Standard24M),
            (RecordType::ExecutionCompleted, RetentionClass::Forever),
            (RecordType::VerificationResult, RetentionClass::Standard24M),
            (RecordType::RollbackCompleted, RetentionClass::Standard24M),
            (RecordType::RecoveryEvent, RetentionClass::Forever),
            (RecordType::ModelCall, RetentionClass::Standard24M),
            (RecordType::ChainCheckpoint, RetentionClass::Forever),
            (RecordType::GcPass, RetentionClass::Standard24M),
            (RecordType::QuarantineEvent, RetentionClass::Standard24M),
            (RecordType::ConflictEvent, RetentionClass::Standard24M),
            (RecordType::EmergencyOverrideGrant, RetentionClass::Forever),
            (RecordType::PolicyBundleLoad, RetentionClass::Forever),
            (RecordType::SegmentSealed, RetentionClass::Forever),
            (
                RecordType::ChainInconsistencyDetected,
                RetentionClass::Forever,
            ),
            (RecordType::TamperDetected, RetentionClass::Forever),
        ];
        assert_eq!(s13_table.len(), 22);
        for (rt, expected) in s13_table {
            assert_eq!(
                rt.retention_class(),
                *expected,
                "§13 retention drift for {rt:?}"
            );
        }
    }

    #[test]
    fn enum_iter_visits_each_variant_exactly_once() {
        // Strum's EnumIter promises a permutation of all variants. This test
        // pins the invariant against future derive macro upgrades.
        let mut seen: HashSet<&'static str> = HashSet::with_capacity(TOTAL_RECORD_TYPE_VARIANTS);
        let mut visits = 0_usize;
        for rt in RecordType::iter() {
            assert!(seen.insert(rt.as_wire_str()), "duplicate visit: {rt:?}");
            visits += 1;
        }
        assert_eq!(visits, TOTAL_RECORD_TYPE_VARIANTS);
        assert_eq!(seen.len(), TOTAL_RECORD_TYPE_VARIANTS);
    }

    #[test]
    fn retention_class_for_all_variants_returns_a_valid_class() {
        // Defensive: every variant returns one of three canonical classes
        // (the closed 3-bucket enum). No "default" path can leak.
        for rt in RecordType::iter() {
            let rc = retention_class_for(rt);
            let ok = matches!(
                rc,
                RetentionClass::Standard24M | RetentionClass::Extended60M | RetentionClass::Forever
            );
            assert!(ok, "{rt:?} returned a non-canonical RetentionClass {rc:?}");
        }
    }

    #[test]
    fn equality_and_hash_consistent_for_all_variants() {
        // Reflexivity and that two distinct variants compare unequal.
        let mut prev: Option<RecordType> = None;
        for rt in RecordType::iter() {
            assert_eq!(rt, rt);
            if let Some(p) = prev {
                if p.wire_id() != rt.wire_id() {
                    assert_ne!(p, rt);
                }
            }
            prev = Some(rt);
        }
    }

    #[test]
    fn variant_names_serialize_without_surprising_characters() {
        // Every wire token must be ASCII SCREAMING_SNAKE_CASE: capital letters,
        // digits, underscores only. No leading/trailing underscore. No double
        // underscores. Length >= 3.
        for rt in RecordType::iter() {
            let s = rt.as_wire_str();
            assert!(s.len() >= 3, "wire name too short: {s}");
            assert!(
                !s.starts_with('_') && !s.ends_with('_'),
                "wire name has edge underscore: {s}"
            );
            assert!(!s.contains("__"), "wire name has double underscore: {s}");
            for ch in s.chars() {
                let ok = ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_';
                assert!(ok, "wire name {s} has non-SCREAMING_SNAKE char {ch:?}");
            }
        }
    }

    #[test]
    fn wave_10_spans_388_through_427_inclusive() {
        // §29.2: Wave 10 (§28) lands at IDs 388..=427 (40 records). Spot-pin
        // every Wave-10 boundary record.
        let wave10_first_per_subsection: &[(RecordType, u16)] = &[
            (RecordType::StatusTransition, 388),                  // §28.1
            (RecordType::ArtifactRecorded, 389),                  // §28.2
            (RecordType::InvariantBundleLoaded, 396),             // §28.3
            (RecordType::IdentityBundleLoaded, 398),              // §28.4
            (RecordType::ReceiptForgeryDetected, 400),            // §28.5
            (RecordType::InvariantBundleRejected, 404),           // §28.6
            (RecordType::GraphEvaluationBudgetExceeded, 410),     // §28.7
            (RecordType::AdapterLifecycleIllegalTransition, 412), // §28.8
            (RecordType::PublisherTrustLevelObserved, 413),       // §28.9
            (RecordType::BridgeOperatorConsentGranted, 416),      // §28.10
            (
                RecordType::HardwareSubstrateAcceptOutsideRecoveryBlocked,
                420,
            ), // §28.11
            (RecordType::FirstBootOperation, 421),                // §28.12
            (RecordType::VaultBootstrapKeyUsed, 422),             // §28.13
            (RecordType::HardwareGraphDriftAccepted, 425),        // §28.14
            (RecordType::AgentLifecycleTransitioned, 427),        // §28.15
        ];
        for (rt, expected) in wave10_first_per_subsection {
            assert_eq!(rt.wire_id(), *expected, "Wave 10 boundary drift for {rt:?}");
        }
    }

    #[test]
    fn wave_5_renderer_family_records_cover_all_subsections() {
        // §24.1.* sub-sections: confirm each surface / UI / theme / KDE / Web /
        // GPU family lands at the right ID range. This is a regression sentinel
        // for spec ordering.
        // S7.1 Surface (25..=31)
        assert_eq!(RecordType::SurfaceCreated.wire_id(), 25);
        assert_eq!(RecordType::SurfaceNeverRendered.wire_id(), 31);
        // S7.2 Shared UI Schema (32..=34)
        assert_eq!(RecordType::UiTreeValidationRejected.wire_id(), 32);
        assert_eq!(RecordType::UiRecoveryNodeDropped.wire_id(), 34);
        // S7.3 Visual Language (35..=38)
        assert_eq!(RecordType::ThemeLoaded.wire_id(), 35);
        assert_eq!(RecordType::ThemeInvariantViolated.wire_id(), 38);
        // S7.4 KDE Renderer (39..=49)
        assert_eq!(RecordType::KdeRendererStarted.wire_id(), 39);
        assert_eq!(RecordType::KdeTokenFallbackUsed.wire_id(), 49);
        // S7.5 Web Renderer (50..=66)
        assert_eq!(RecordType::WebLanExposureGranted.wire_id(), 50);
        assert_eq!(
            RecordType::WebConstitutionalElementReregisterBlocked.wire_id(),
            66
        );
        // S8.2 GPU Resource Model (67..=82)
        assert_eq!(RecordType::GpuDeviceEnumerated.wire_id(), 67);
        assert_eq!(RecordType::GpuDeviceForceReclaimed.wire_id(), 82);
    }

    #[test]
    fn wave_6_capability_runtime_family_lands_at_114_through_132() {
        // §25.5 S10.1 family — 19 new IDs (ACTION_RECEIVED reused at 1).
        assert_eq!(RecordType::ActionValidated.wire_id(), 114);
        assert_eq!(RecordType::ActionPolicyDecision.wire_id(), 115);
        assert_eq!(RecordType::AdapterDeprecatedDispatch.wire_id(), 132);
    }

    #[test]
    fn wave_8_cognitive_router_family_lands_at_248_through_259() {
        // §27.7 S13.2 family — 12 records.
        let family = [
            (RecordType::ModelInvocationStarted, 248_u16),
            (RecordType::ModelInvocationSucceeded, 249),
            (RecordType::ModelInvocationFailed, 250),
            (RecordType::ModelBackendDegraded, 251),
            (RecordType::ModelCircuitOpened, 252),
            (RecordType::ModelPromptInjectionDetected, 253),
            (RecordType::ModelResponseSignatureFailed, 254),
            (RecordType::ModelVaultDeny, 255),
            (RecordType::ModelNetworkDeny, 256),
            (RecordType::ModelRateLimited, 257),
            (RecordType::ModelBackendRegistered, 258),
            (RecordType::ModelBackendRetired, 259),
        ];
        for (rt, id) in family {
            assert_eq!(rt.wire_id(), id);
        }
        // Legacy MODEL_CALL at ID 13 is also retained alongside (§29.4).
        assert_eq!(RecordType::ModelCall.wire_id(), 13);
    }

    #[test]
    fn evidence_grade_marker_records_retention_split_per_section_28_2() {
        // §28.2 split: STANDARD_24M ×4 (artifact/build/test/operational),
        // EXTENDED_60M ×1 (e2e), FOREVER ×2 (recovery-rehearsal +
        // release-gate — constitutional release surfaces).
        let grade_markers: &[(RecordType, RetentionClass)] = &[
            (RecordType::ArtifactRecorded, RetentionClass::Standard24M),
            (RecordType::BuildPassed, RetentionClass::Standard24M),
            (RecordType::TestPassed, RetentionClass::Standard24M),
            (RecordType::E2ePassed, RetentionClass::Extended60M),
            (RecordType::RecoveryRehearsalPassed, RetentionClass::Forever),
            (RecordType::ReleaseGatePassed, RetentionClass::Forever),
            (RecordType::OperationalHealthy, RetentionClass::Standard24M),
        ];
        for (rt, expected) in grade_markers {
            assert_eq!(
                rt.retention_class(),
                *expected,
                "{rt:?} retention drift vs §28.2"
            );
        }
    }

    #[test]
    fn pascal_case_round_trip_for_every_variant_via_debug() {
        // Debug-formatted variant names are PascalCase identifiers. Pairing
        // Debug + as_wire_str gives an audit-friendly two-name probe for any
        // wire-ID. This test pins that the two surfaces agree on the
        // number-of-variants axis.
        let mut from_debug = HashSet::with_capacity(TOTAL_RECORD_TYPE_VARIANTS);
        let mut from_wire = HashSet::with_capacity(TOTAL_RECORD_TYPE_VARIANTS);
        for rt in RecordType::iter() {
            from_debug.insert(format!("{rt:?}"));
            from_wire.insert(rt.as_wire_str().to_owned());
        }
        assert_eq!(from_debug.len(), TOTAL_RECORD_TYPE_VARIANTS);
        assert_eq!(from_wire.len(), TOTAL_RECORD_TYPE_VARIANTS);
    }

    #[test]
    fn rejected_t007_only_names_no_longer_resolve_via_serde() {
        // The five T-007-only names that never made it into S3.1 Appendix A
        // (EVIDENCE_LOG_TAMPER_DETECTED, KERNEL_IMAGE_TAMPER_DETECTED,
        // RECEIPT_SEALED, ACTION_FAILED, ACTION_ROLLED_BACK) must FAIL to
        // deserialize against the closed enum — confirming the Wave 13 drop.
        let dead = [
            "\"EVIDENCE_LOG_TAMPER_DETECTED\"",
            "\"KERNEL_IMAGE_TAMPER_DETECTED\"",
            "\"RECEIPT_SEALED\"",
            "\"ACTION_FAILED\"",
            "\"ACTION_ROLLED_BACK\"",
        ];
        for token in dead {
            let r: Result<RecordType, _> = serde_json::from_str(token);
            assert!(
                r.is_err(),
                "{token} must NOT deserialize — it is not in S3.1 Appendix A"
            );
        }
    }

    #[test]
    fn wire_id_is_within_allocated_ranges_for_every_variant() {
        // Defensive: every variant ID must be in either the Wave 13 dense
        // block (1..=427) or the Wave 14+ reserved block (1000..=9999).
        for rt in RecordType::iter() {
            let id = rt.wire_id();
            assert!(id >= 1, "{rt:?} wire_id {id} must be positive");
            let in_wave_13 = (1..=427).contains(&id);
            let in_wave_14_reserved = (1000..=9999).contains(&id);
            assert!(
                in_wave_13 || in_wave_14_reserved,
                "{rt:?} wire_id {id} is outside both allocated ranges \
                 (Wave 13 = 1..=427, Wave 14+ reserved = 1000..=9999)"
            );
        }
    }
}
