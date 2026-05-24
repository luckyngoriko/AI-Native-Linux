//! Hard-deny engine — S2.3 §6 enforcement (T-018).
//!
//! Implements the constitutional 10-class hard-deny table from `01_policy_kernel.md`
//! §6. Each class is detected by a private `detect_*` method that inspects the
//! supplied [`ActionEnvelope`] + [`HydratedSubject`] **only** — no L4 hydration calls,
//! no AIOS-FS reads. The §26 / §27 / §28 Wave-4/5/6/9/12 hard-denies are NOT in scope
//! for T-018 (they bind to fields the conditions parser T-019 + bundle loader T-022
//! will expose); this engine enforces strictly the original 10 §6 rows.
//!
//! ## Wiring
//!
//! ```text
//!   DecisionPipeline::step_4_evaluate_hard_denies
//!         |
//!         v
//!   HardDenyEngine::check(envelope, subject)
//!         |
//!         +--> Some(HardDenyClass) => pipeline short-circuits with Decision::Deny
//!         +--> None                => pipeline continues to step 5
//! ```
//!
//! The engine is invoked through the optional `hard_deny_engine: Option<Arc<...>>`
//! field on [`crate::kernel::InMemoryPolicyKernel`] so the T-017 baseline tests
//! (which construct the kernel via `new()` without an engine) keep their stub
//! semantics. Production wiring always attaches an engine via
//! [`crate::kernel::InMemoryPolicyKernel::new_with_hard_deny`].
//!
//! ## Detection model
//!
//! Detection is **closed-vocabulary** on action names plus structured inspection
//! of `envelope.request.target` JSON. The engine ships with [`Self::new_with_defaults`]
//! that bakes in the spec's canonical action names and protected-root paths so the
//! out-of-the-box behaviour matches §6 without external configuration. Future tasks
//! (T-022 bundle loader) may extend the closed sets via signed configuration but
//! they cannot WEAKEN them — that is the constitutional invariant.
//!
//! ## Override paths
//!
//! Per §6 only two rows have an override path:
//! - `hd.modify_boot_chain` — recovery-mode operator approval
//! - `hd.aios_fs_pointer_rollback_on_recovery` — recovery-mode operator approval
//!
//! T-018 returns the hard-deny class regardless of recovery mode; the override
//! pipeline lives in T-025 (`05_emergency_override.md` mechanics). The §6 spec
//! is explicit: "**Emergency override cannot bypass evidence logging.** Even an
//! authorized override emits evidence with the override receipt." — so the deny
//! itself stands and the override path is a *separate* downstream flow that
//! produces an evidence-linked override receipt; it does not flip the engine's
//! verdict. The `reason_message` produced by the pipeline step 4 wrapper carries
//! an `(override path: recovery-mode operator approval)` suffix for the two
//! overridable classes so audit + future T-025 override engine can spot them.

use std::collections::HashSet;

use aios_action::ActionEnvelope;

use crate::hard_deny::HardDenyClass;
use crate::subject::HydratedSubject;

/// Closed-vocabulary configuration for [`HardDenyEngine`].
///
/// All fields are sets/lists of action names or path prefixes that the engine
/// considers in-scope for the matching hard-deny class. The defaults baked in
/// by [`HardDenyEngineConfig::spec_defaults`] match the canonical actions
/// implied by the §6 spec table.
///
/// The config is split out from the engine struct purely so test fixtures can
/// rebuild a config with custom action vocabularies without re-implementing
/// every detect method.
#[derive(Debug, Clone)]
pub struct HardDenyEngineConfig {
    /// Action names whose target reads a raw secret (triggers
    /// `hd.secret_raw_read_by_ai` when subject is AI).
    pub secret_raw_read_actions: HashSet<String>,
    /// Action names that perform recursive deletion of a path.
    pub recursive_delete_actions: HashSet<String>,
    /// Protected root paths that may not be recursively deleted under any
    /// circumstance (§6 row 2: `/`, `/home`, `/root`, `/aios`, recovery
    /// partitions). Path comparison is exact OR proper-prefix; a path matches
    /// when it equals the protected root or when the protected root is a prefix
    /// of the supplied path (including the trivial `/` case).
    pub protected_roots: HashSet<String>,
    /// Path prefixes treated as "recovery partition" for §6 row 2.
    pub recovery_partition_prefixes: Vec<String>,
    /// Actions that mutate the policy log (deletion, edit, append).
    pub policy_log_mutation_actions: HashSet<String>,
    /// Actions that mutate the evidence log (S3.1 invariant).
    pub evidence_log_mutation_actions: HashSet<String>,
    /// Actions that disable the Policy Kernel itself.
    pub disable_policy_kernel_actions: HashSet<String>,
    /// Actions that disable the recovery path.
    pub disable_recovery_path_actions: HashSet<String>,
    /// Actions that modify the boot chain.
    pub modify_boot_chain_actions: HashSet<String>,
    /// Actions that execute an untyped shell (free-form `argv`).
    pub untyped_shell_actions: HashSet<String>,
    /// Subject groups considered "privileged" for §6 row 8 (untyped shell).
    pub privileged_groups: HashSet<String>,
    /// Capability names considered "privileged" for §6 row 8 (untyped shell).
    pub privileged_capabilities: HashSet<String>,
    /// Actions that roll back AIOS-FS pointers.
    pub aiosfs_pointer_rollback_actions: HashSet<String>,
    /// Actions that downgrade an object's privacy class.
    pub privacy_class_downgrade_actions: HashSet<String>,
}

impl HardDenyEngineConfig {
    /// Build the canonical spec-default config — the closed sets implied by
    /// §6's row descriptions and the action vocabulary attested elsewhere in
    /// rev.2.
    #[must_use]
    pub fn spec_defaults() -> Self {
        Self {
            secret_raw_read_actions: into_set(&[
                "vault.read_raw",
                "secret.read_raw",
                "secret.reveal",
                "vault.reveal",
            ]),
            recursive_delete_actions: into_set(&[
                "aiosfs.recursive_delete",
                "aiosfs.delete_recursive",
                "fs.rm_rf",
                "fs.recursive_delete",
            ]),
            protected_roots: into_set(&["/", "/home", "/root", "/aios"]),
            recovery_partition_prefixes: vec![
                "/recovery".to_owned(),
                "/boot/recovery".to_owned(),
                "/aios/recovery".to_owned(),
            ],
            policy_log_mutation_actions: into_set(&[
                "policy.log.delete",
                "policy.log.mutate",
                "policy.log.truncate",
                "policy.log.rewrite",
            ]),
            evidence_log_mutation_actions: into_set(&[
                "evidence.log.delete",
                "evidence.log.mutate",
                "evidence.log.truncate",
                "evidence.log.rewrite",
                "evidence.tamper",
            ]),
            disable_policy_kernel_actions: into_set(&[
                "policy_kernel.disable",
                "policy.kernel.disable",
                "policy.kernel.stop",
            ]),
            disable_recovery_path_actions: into_set(&[
                "recovery.disable",
                "recovery.path.disable",
                "recovery.partition.disable",
            ]),
            modify_boot_chain_actions: into_set(&[
                "boot.chain.modify",
                "boot.loader.modify",
                "boot.efi.modify",
                "bootloader.modify",
            ]),
            untyped_shell_actions: into_set(&[
                "shell.exec_untyped",
                "shell.untyped",
                "shell.exec_free_form",
            ]),
            privileged_groups: into_set(&["root", "sudo", "wheel", "privileged", "operators"]),
            privileged_capabilities: into_set(&["shell.untyped", "system_admin", "root_shell"]),
            aiosfs_pointer_rollback_actions: into_set(&[
                "aiosfs.pointer.rollback",
                "aiosfs.pointer.revert",
            ]),
            privacy_class_downgrade_actions: into_set(&[
                "aiosfs.object.set_privacy_class",
                "object.privacy.downgrade",
                "aiosfs.privacy.downgrade",
            ]),
        }
    }
}

fn into_set(items: &[&str]) -> HashSet<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

/// The §6 hard-deny engine.
///
/// Stateless across evaluations; safe to share via `Arc<HardDenyEngine>` between
/// async tasks. Constructed once at kernel startup and re-used.
#[derive(Debug, Clone)]
pub struct HardDenyEngine {
    config: HardDenyEngineConfig,
}

impl HardDenyEngine {
    /// Construct a new engine with the supplied configuration.
    #[must_use]
    pub const fn new(config: HardDenyEngineConfig) -> Self {
        Self { config }
    }

    /// Construct an engine pre-loaded with the canonical §6 defaults.
    #[must_use]
    pub fn new_with_defaults() -> Self {
        Self::new(HardDenyEngineConfig::spec_defaults())
    }

    /// Borrow the active config (test inspection only).
    #[must_use]
    pub const fn config(&self) -> &HardDenyEngineConfig {
        &self.config
    }

    /// Run the 10 §6 detection methods in the spec's table order and return
    /// the first hard-deny class that matches. Returns `None` when no class
    /// fires; the pipeline then continues to step 5.
    ///
    /// Ordering matches the §6 table; the spec does not mandate evaluation
    /// order between the 10 rows (they are mutually exclusive in shape), but
    /// fixing it makes the engine's audit deterministic.
    #[must_use]
    pub fn check(
        &self,
        envelope: &ActionEnvelope,
        subject: &HydratedSubject,
    ) -> Option<HardDenyClass> {
        if self.detect_secret_raw_read_by_ai(envelope, subject) {
            return Some(HardDenyClass::SecretRawReadByAi);
        }
        if self.detect_recursive_delete_root(envelope) {
            return Some(HardDenyClass::RecursiveDeleteRoot);
        }
        if self.detect_policy_log_mutation(envelope) {
            return Some(HardDenyClass::PolicyLogMutation);
        }
        if self.detect_evidence_log_mutation(envelope) {
            return Some(HardDenyClass::EvidenceLogMutation);
        }
        if self.detect_disable_policy_kernel(envelope) {
            return Some(HardDenyClass::DisablePolicyKernel);
        }
        if self.detect_disable_recovery_path(envelope) {
            return Some(HardDenyClass::DisableRecoveryPath);
        }
        if self.detect_modify_boot_chain(envelope) {
            return Some(HardDenyClass::ModifyBootChain);
        }
        if self.detect_untyped_shell_privileged(envelope, subject) {
            return Some(HardDenyClass::UntypedShellPrivileged);
        }
        if self.detect_aios_fs_pointer_rollback_on_recovery(envelope) {
            return Some(HardDenyClass::AiosFsPointerRollbackOnRecovery);
        }
        if self.detect_privacy_class_downgrade(envelope) {
            return Some(HardDenyClass::PrivacyClassDowngrade);
        }
        None
    }

    // -----------------------------------------------------------------------
    // §6 row 1 — `hd.secret_raw_read_by_ai`
    // -----------------------------------------------------------------------
    //
    // "Raw secret read by `agent`/`application` subject." Detected by:
    //   subject.is_ai == true
    //   AND request.action is in `secret_raw_read_actions`
    //
    // The §6 row is constitutional: AI subjects can NEVER read raw secret
    // material; they must go through the Vault Broker (S2.3 §15 / L4 vault).
    fn detect_secret_raw_read_by_ai(
        &self,
        envelope: &ActionEnvelope,
        subject: &HydratedSubject,
    ) -> bool {
        subject.is_ai
            && self
                .config
                .secret_raw_read_actions
                .contains(&envelope.request.action)
    }

    // -----------------------------------------------------------------------
    // §6 row 2 — `hd.recursive_delete_root`
    // -----------------------------------------------------------------------
    //
    // "Recursive deletion of `/`, `/home`, `/root`, `/aios`, recovery partitions."
    // Detected by:
    //   request.action is in `recursive_delete_actions`
    //   AND request.target.path equals/begins one of `protected_roots`
    //       OR begins with one of `recovery_partition_prefixes`
    fn detect_recursive_delete_root(&self, envelope: &ActionEnvelope) -> bool {
        if !self
            .config
            .recursive_delete_actions
            .contains(&envelope.request.action)
        {
            return false;
        }
        let Some(path) = target_string_field(envelope, "path") else {
            return false;
        };
        let normalised = normalise_path(&path);
        if self.config.protected_roots.contains(&normalised) {
            return true;
        }
        // Match a protected root as a proper prefix — `/home/lucky` is under `/home`.
        for root in &self.config.protected_roots {
            if root == "/" {
                // The bare `/` matches everything; the equality check above
                // already covers the exact `/` case, and a recursive delete on
                // any absolute path is logically a delete under `/`.
                return true;
            }
            if normalised.starts_with(&format!("{root}/")) {
                return true;
            }
        }
        // Recovery partition prefixes.
        self.config
            .recovery_partition_prefixes
            .iter()
            .any(|prefix| normalised == *prefix || normalised.starts_with(&format!("{prefix}/")))
    }

    // -----------------------------------------------------------------------
    // §6 row 3 — `hd.policy_log_mutation`
    // -----------------------------------------------------------------------
    //
    // "Mutation or deletion of policy log." Detected by action-name membership.
    fn detect_policy_log_mutation(&self, envelope: &ActionEnvelope) -> bool {
        self.config
            .policy_log_mutation_actions
            .contains(&envelope.request.action)
    }

    // -----------------------------------------------------------------------
    // §6 row 4 — `hd.evidence_log_mutation`
    // -----------------------------------------------------------------------
    //
    // "Mutation of evidence log (§S3.1 invariant)." S3.1 forbids any in-place
    // mutation of evidence; append is allowed only through the typed Evidence
    // Log writer (T-007..T-011 landed in M2). Anything that names an
    // evidence-log mutation action gets denied here.
    fn detect_evidence_log_mutation(&self, envelope: &ActionEnvelope) -> bool {
        self.config
            .evidence_log_mutation_actions
            .contains(&envelope.request.action)
    }

    // -----------------------------------------------------------------------
    // §6 row 5 — `hd.disable_policy_kernel`
    // -----------------------------------------------------------------------
    fn detect_disable_policy_kernel(&self, envelope: &ActionEnvelope) -> bool {
        self.config
            .disable_policy_kernel_actions
            .contains(&envelope.request.action)
    }

    // -----------------------------------------------------------------------
    // §6 row 6 — `hd.disable_recovery_path`
    // -----------------------------------------------------------------------
    fn detect_disable_recovery_path(&self, envelope: &ActionEnvelope) -> bool {
        self.config
            .disable_recovery_path_actions
            .contains(&envelope.request.action)
    }

    // -----------------------------------------------------------------------
    // §6 row 7 — `hd.modify_boot_chain` (overridable in recovery mode)
    // -----------------------------------------------------------------------
    fn detect_modify_boot_chain(&self, envelope: &ActionEnvelope) -> bool {
        self.config
            .modify_boot_chain_actions
            .contains(&envelope.request.action)
    }

    // -----------------------------------------------------------------------
    // §6 row 8 — `hd.untyped_shell_privileged`
    // -----------------------------------------------------------------------
    //
    // "Untyped shell execution as privileged subject." Detected by:
    //   request.action is in `untyped_shell_actions`
    //   AND ( subject has any group in `privileged_groups`
    //         OR subject has any capability in `privileged_capabilities` )
    fn detect_untyped_shell_privileged(
        &self,
        envelope: &ActionEnvelope,
        subject: &HydratedSubject,
    ) -> bool {
        if !self
            .config
            .untyped_shell_actions
            .contains(&envelope.request.action)
        {
            return false;
        }
        let has_priv_group = subject
            .groups
            .iter()
            .any(|g| self.config.privileged_groups.contains(g));
        let has_priv_cap = subject
            .capabilities
            .iter()
            .any(|c| self.config.privileged_capabilities.contains(c));
        has_priv_group || has_priv_cap
    }

    // -----------------------------------------------------------------------
    // §6 row 9 — `hd.aios_fs_pointer_rollback_on_recovery` (overridable in recovery mode)
    // -----------------------------------------------------------------------
    //
    // "Rolling back recovery-essential pointers without operator approval."
    fn detect_aios_fs_pointer_rollback_on_recovery(&self, envelope: &ActionEnvelope) -> bool {
        self.config
            .aiosfs_pointer_rollback_actions
            .contains(&envelope.request.action)
    }

    // -----------------------------------------------------------------------
    // §6 row 10 — `hd.privacy_class_downgrade`
    // -----------------------------------------------------------------------
    //
    // "Lowering an object's privacy class." Detected by action-name membership
    // and (when both `current_class` and `new_class` are present in target)
    // by `new_class < current_class` per the S1.3 §4.1 monotonic privacy
    // ladder PUBLIC < INTERNAL < CONFIDENTIAL < SECRET < TOP_SECRET.
    //
    // If the target does not declare both fields, the action-name match alone
    // is sufficient — the §6 rule is conservative (downgrade attempts are
    // hard-denied) and the engine must fail closed.
    fn detect_privacy_class_downgrade(&self, envelope: &ActionEnvelope) -> bool {
        if !self
            .config
            .privacy_class_downgrade_actions
            .contains(&envelope.request.action)
        {
            return false;
        }
        match (
            target_string_field(envelope, "current_class"),
            target_string_field(envelope, "new_class"),
        ) {
            (Some(current), Some(new)) => privacy_rank(&new) < privacy_rank(&current),
            // Conservative default — action name alone is sufficient signal.
            _ => true,
        }
    }
}

impl Default for HardDenyEngine {
    fn default() -> Self {
        Self::new_with_defaults()
    }
}

/// Look up a string field by key inside `envelope.request.target` JSON.
/// Returns `None` if target is not a JSON object, the field is absent, or the
/// field value is not a string.
fn target_string_field(envelope: &ActionEnvelope, key: &str) -> Option<String> {
    envelope
        .request
        .target
        .as_object()
        .and_then(|m| m.get(key))
        .and_then(|v| v.as_str())
        .map(std::borrow::ToOwned::to_owned)
}

/// Normalise a POSIX path: strip a single trailing `/` (except on the bare `/`).
fn normalise_path(p: &str) -> String {
    if p.len() > 1 && p.ends_with('/') {
        p.trim_end_matches('/').to_owned()
    } else {
        p.to_owned()
    }
}

/// Privacy-class ladder per S1.3 §4.1 — unknown classes rank as `u8::MAX` so
/// they cannot accidentally be treated as "lower than" anything (fail-closed).
fn privacy_rank(class: &str) -> u8 {
    match class {
        "PUBLIC" => 0,
        "INTERNAL" => 1,
        "CONFIDENTIAL" => 2,
        "SECRET" => 3,
        "TOP_SECRET" => 4,
        _ => u8::MAX,
    }
}

/// Construct the canonical `reason_code` string a pipeline step emits when
/// `class` fires. Format: `"HardDeny:<PascalCaseVariant>"` (e.g.
/// `"HardDeny:SecretRawReadByAi"`).
///
/// The string is stable; downstream evidence + the gRPC `ExplainDecision`
/// surface key off it.
#[must_use]
pub fn reason_code_for(class: HardDenyClass) -> String {
    let variant = match class {
        HardDenyClass::SecretRawReadByAi => "SecretRawReadByAi",
        HardDenyClass::RecursiveDeleteRoot => "RecursiveDeleteRoot",
        HardDenyClass::PolicyLogMutation => "PolicyLogMutation",
        HardDenyClass::EvidenceLogMutation => "EvidenceLogMutation",
        HardDenyClass::DisablePolicyKernel => "DisablePolicyKernel",
        HardDenyClass::DisableRecoveryPath => "DisableRecoveryPath",
        HardDenyClass::ModifyBootChain => "ModifyBootChain",
        HardDenyClass::UntypedShellPrivileged => "UntypedShellPrivileged",
        HardDenyClass::AiosFsPointerRollbackOnRecovery => "AiosFsPointerRollbackOnRecovery",
        HardDenyClass::PrivacyClassDowngrade => "PrivacyClassDowngrade",
    };
    format!("HardDeny:{variant}")
}

/// Construct the human-readable `reason_message` string for a hard-deny decision.
///
/// For the two §6 rows with a recovery-mode override path, the
/// message ends with an explicit `(override path: recovery-mode operator
/// approval)` suffix so audit + future T-025 override engine can spot them.
#[must_use]
pub fn reason_message_for(class: HardDenyClass) -> String {
    let base = match class {
        HardDenyClass::SecretRawReadByAi => {
            "raw secret read by AI subject is constitutionally forbidden (S2.3 §6 row 1)"
        }
        HardDenyClass::RecursiveDeleteRoot => {
            "recursive deletion of a protected root path is constitutionally forbidden (S2.3 §6 row 2)"
        }
        HardDenyClass::PolicyLogMutation => {
            "mutation of the policy log is constitutionally forbidden (S2.3 §6 row 3)"
        }
        HardDenyClass::EvidenceLogMutation => {
            "mutation of the evidence log is constitutionally forbidden (S2.3 §6 row 4)"
        }
        HardDenyClass::DisablePolicyKernel => {
            "disabling the Policy Kernel is constitutionally forbidden (S2.3 §6 row 5)"
        }
        HardDenyClass::DisableRecoveryPath => {
            "disabling the recovery path is constitutionally forbidden (S2.3 §6 row 6)"
        }
        HardDenyClass::ModifyBootChain => {
            "modifying the boot chain is constitutionally forbidden (S2.3 §6 row 7)"
        }
        HardDenyClass::UntypedShellPrivileged => {
            "untyped shell execution as a privileged subject is constitutionally forbidden (S2.3 §6 row 8)"
        }
        HardDenyClass::AiosFsPointerRollbackOnRecovery => {
            "rolling back recovery-essential AIOS-FS pointers is constitutionally forbidden (S2.3 §6 row 9)"
        }
        HardDenyClass::PrivacyClassDowngrade => {
            "lowering an object's privacy class is constitutionally forbidden (S2.3 §6 row 10)"
        }
    };
    match class {
        HardDenyClass::ModifyBootChain | HardDenyClass::AiosFsPointerRollbackOnRecovery => {
            format!("{base} (override path: recovery-mode operator approval)")
        }
        _ => base.to_owned(),
    }
}

/// `true` if `class` has a recovery-mode operator-approval override path per §6.
///
/// Used by audit + future T-025 override engine to identify overridable
/// classes without round-tripping through `reason_message_for`.
#[must_use]
pub const fn has_recovery_override_path(class: HardDenyClass) -> bool {
    matches!(
        class,
        HardDenyClass::ModifyBootChain | HardDenyClass::AiosFsPointerRollbackOnRecovery
    )
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;

    #[test]
    fn privacy_rank_ladder_matches_s13_section_4_1() {
        assert!(privacy_rank("PUBLIC") < privacy_rank("INTERNAL"));
        assert!(privacy_rank("INTERNAL") < privacy_rank("CONFIDENTIAL"));
        assert!(privacy_rank("CONFIDENTIAL") < privacy_rank("SECRET"));
        assert!(privacy_rank("SECRET") < privacy_rank("TOP_SECRET"));
        // Unknown class fails closed (ranks higher than every known class).
        assert!(privacy_rank("BOGUS") > privacy_rank("TOP_SECRET"));
    }

    #[test]
    fn normalise_path_trims_trailing_slash_but_preserves_root() {
        assert_eq!(normalise_path("/home/lucky/"), "/home/lucky");
        assert_eq!(normalise_path("/"), "/");
        assert_eq!(normalise_path("/aios"), "/aios");
    }

    #[test]
    fn reason_code_for_matches_pascal_case_variant() {
        assert_eq!(
            reason_code_for(HardDenyClass::SecretRawReadByAi),
            "HardDeny:SecretRawReadByAi"
        );
        assert_eq!(
            reason_code_for(HardDenyClass::PrivacyClassDowngrade),
            "HardDeny:PrivacyClassDowngrade"
        );
    }

    #[test]
    fn has_recovery_override_path_matches_section_6_table() {
        // Exactly two rows are overridable per §6.
        assert!(has_recovery_override_path(HardDenyClass::ModifyBootChain));
        assert!(has_recovery_override_path(
            HardDenyClass::AiosFsPointerRollbackOnRecovery
        ));
        // Every other row is NOT overridable.
        assert!(!has_recovery_override_path(
            HardDenyClass::SecretRawReadByAi
        ));
        assert!(!has_recovery_override_path(
            HardDenyClass::RecursiveDeleteRoot
        ));
        assert!(!has_recovery_override_path(
            HardDenyClass::PolicyLogMutation
        ));
        assert!(!has_recovery_override_path(
            HardDenyClass::EvidenceLogMutation
        ));
        assert!(!has_recovery_override_path(
            HardDenyClass::DisablePolicyKernel
        ));
        assert!(!has_recovery_override_path(
            HardDenyClass::DisableRecoveryPath
        ));
        assert!(!has_recovery_override_path(
            HardDenyClass::UntypedShellPrivileged
        ));
        assert!(!has_recovery_override_path(
            HardDenyClass::PrivacyClassDowngrade
        ));
    }

    #[test]
    fn reason_message_for_overridable_classes_carries_override_suffix() {
        let msg = reason_message_for(HardDenyClass::ModifyBootChain);
        assert!(
            msg.contains("(override path: recovery-mode operator approval)"),
            "ModifyBootChain reason_message must carry override suffix; got: {msg}"
        );
        let msg2 = reason_message_for(HardDenyClass::AiosFsPointerRollbackOnRecovery);
        assert!(msg2.contains("(override path: recovery-mode operator approval)"));
        // Non-overridable class must NOT carry the suffix.
        let msg3 = reason_message_for(HardDenyClass::PolicyLogMutation);
        assert!(!msg3.contains("override path"));
    }
}
