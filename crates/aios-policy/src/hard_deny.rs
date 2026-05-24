//! Hard-deny classes — S2.3 §6 constitutional table.
//!
//! The hard-deny list is part of L0 (constitutional truth). It cannot be overridden
//! except as listed in the override-path column of S2.3 §6. The enum below is the
//! canonical `PascalCase` form of the `policy_id` strings in the spec table.

use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// The 10 hard-deny classes enumerated in S2.3 §6.
///
/// Each variant corresponds 1:1 with a row in the spec table. The serde wire form is
/// the spec's `hd.<snake_case>` policy id, so external bundle YAML / proto encoding
/// continues to use the constitutional string keys.
///
/// `EnumCount` provides the compile-time invariant `HardDenyClass::COUNT == 10`
/// asserted by the round-trip tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
pub enum HardDenyClass {
    /// `hd.secret_raw_read_by_ai` — raw secret read by agent/application subject.
    #[serde(rename = "hd.secret_raw_read_by_ai")]
    SecretRawReadByAi,
    /// `hd.recursive_delete_root` — recursive deletion of `/`, `/home`, `/root`, `/aios`,
    /// recovery partitions.
    #[serde(rename = "hd.recursive_delete_root")]
    RecursiveDeleteRoot,
    /// `hd.policy_log_mutation` — mutation or deletion of policy log.
    #[serde(rename = "hd.policy_log_mutation")]
    PolicyLogMutation,
    /// `hd.evidence_log_mutation` — mutation of evidence log (S3.1 invariant).
    #[serde(rename = "hd.evidence_log_mutation")]
    EvidenceLogMutation,
    /// `hd.disable_policy_kernel` — disabling Policy Kernel (self-disable).
    #[serde(rename = "hd.disable_policy_kernel")]
    DisablePolicyKernel,
    /// `hd.disable_recovery_path` — disabling recovery path.
    #[serde(rename = "hd.disable_recovery_path")]
    DisableRecoveryPath,
    /// `hd.modify_boot_chain` — modifying boot chain without dedicated recovery approval.
    /// Overridable only via recovery-mode operator approval (`05_emergency_override.md`).
    #[serde(rename = "hd.modify_boot_chain")]
    ModifyBootChain,
    /// `hd.untyped_shell_privileged` — untyped shell execution as privileged subject.
    #[serde(rename = "hd.untyped_shell_privileged")]
    UntypedShellPrivileged,
    /// `hd.aios_fs_pointer_rollback_on_recovery` — rolling back recovery-essential pointers
    /// without operator approval. Overridable only via recovery-mode operator approval.
    #[serde(rename = "hd.aios_fs_pointer_rollback_on_recovery")]
    AiosFsPointerRollbackOnRecovery,
    /// `hd.privacy_class_downgrade` — lowering an object's privacy class (S1.3 §4.1).
    #[serde(rename = "hd.privacy_class_downgrade")]
    PrivacyClassDowngrade,
}
