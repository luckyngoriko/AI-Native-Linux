//! T-018 integration tests for [`aios_policy::HardDenyEngine`] — S2.3 §6 enforcement.
//!
//! One test per §6 row (10 tests) plus a clean-envelope test, a pipeline-wiring
//! test that asserts step 4 short-circuits with the right `reason_code`, and a
//! recovery-override-suffix test for the two overridable rows. Total: 13 tests.
//!
//! The §6 row order is fixed in the spec; the tests assert both the matched
//! class AND the canonical `reason_code` produced by
//! [`aios_policy::reason_code_for`] so a future refactor that drops a row or
//! reorders the engine's checks gets caught here.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_policy::{
    has_recovery_override_path, reason_code_for, Decision, EnrichmentSnapshot, HardDenyClass,
    HardDenyEngine, HydratedSubject, InMemoryPolicyKernel, PolicyContext, PolicyKernel,
    SubjectType,
};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn human_subject() -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: "human:lucky:01HX0000000000000000000000".to_owned(),
        subject_type: SubjectType::Human,
        groups: vec!["operators".to_owned()],
        capabilities: vec![],
        session_class: "INTERNAL".to_owned(),
        recovery_mode: false,
        is_ai: false,
    }
}

fn ai_subject() -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: "agent:dev:01HX0000000000000000000001".to_owned(),
        subject_type: SubjectType::Agent,
        groups: vec!["agents".to_owned()],
        capabilities: vec![],
        session_class: "INTERNAL".to_owned(),
        recovery_mode: false,
        is_ai: true,
    }
}

fn unprivileged_human_subject() -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: "human:guest:01HX0000000000000000000002".to_owned(),
        subject_type: SubjectType::Human,
        groups: vec!["users".to_owned()],
        capabilities: vec![],
        session_class: "INTERNAL".to_owned(),
        recovery_mode: false,
        is_ai: false,
    }
}

fn envelope(
    action: &str,
    target: serde_json::Value,
    subject_id: &str,
    is_ai: bool,
) -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new(subject_id, is_ai),
        Request::new(action, target),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn make_context(subject: HydratedSubject) -> PolicyContext {
    PolicyContext::new(
        subject,
        EnrichmentSnapshot {
            snapshot_id: "snap_t018".to_owned(),
        },
        "polb_t018_test_bundle_v1",
        "code_t018_test",
    )
}

fn engine() -> HardDenyEngine {
    HardDenyEngine::new_with_defaults()
}

// ---------------------------------------------------------------------------
// 1. §6 row 1 — hd.secret_raw_read_by_ai
// ---------------------------------------------------------------------------

#[test]
fn secret_raw_read_by_ai_fires_for_ai_subject_on_vault_read_raw() {
    let env = envelope(
        "vault.read_raw",
        serde_json::json!({"capability": "vault-cap-7"}),
        "agent:dev",
        true,
    );
    let class = engine().check(&env, &ai_subject());
    assert_eq!(class, Some(HardDenyClass::SecretRawReadByAi));
}

#[test]
fn secret_raw_read_by_ai_does_not_fire_for_human_subject() {
    let env = envelope(
        "vault.read_raw",
        serde_json::json!({"capability": "vault-cap-7"}),
        "human:lucky",
        false,
    );
    // Human subject reading raw secret hits other policies (vault gate), but
    // not §6 row 1 — that row is AI-only.
    let class = engine().check(&env, &human_subject());
    assert_ne!(class, Some(HardDenyClass::SecretRawReadByAi));
}

// ---------------------------------------------------------------------------
// 2. §6 row 2 — hd.recursive_delete_root
// ---------------------------------------------------------------------------

#[test]
fn recursive_delete_root_fires_on_protected_root() {
    let env = envelope(
        "aiosfs.recursive_delete",
        serde_json::json!({"path": "/home"}),
        "human:lucky",
        false,
    );
    let class = engine().check(&env, &human_subject());
    assert_eq!(class, Some(HardDenyClass::RecursiveDeleteRoot));
}

#[test]
fn recursive_delete_root_fires_on_protected_root_prefix() {
    // `/home/lucky/photos` is under the protected root `/home`; §6 row 2
    // protects the root AND everything under it.
    let env = envelope(
        "aiosfs.recursive_delete",
        serde_json::json!({"path": "/home/lucky/photos"}),
        "human:lucky",
        false,
    );
    let class = engine().check(&env, &human_subject());
    assert_eq!(class, Some(HardDenyClass::RecursiveDeleteRoot));
}

#[test]
fn recursive_delete_root_fires_on_recovery_partition_prefix() {
    let env = envelope(
        "fs.rm_rf",
        serde_json::json!({"path": "/recovery/grub"}),
        "human:lucky",
        false,
    );
    let class = engine().check(&env, &human_subject());
    assert_eq!(class, Some(HardDenyClass::RecursiveDeleteRoot));
}

// ---------------------------------------------------------------------------
// 3. §6 row 3 — hd.policy_log_mutation
// ---------------------------------------------------------------------------

#[test]
fn policy_log_mutation_fires_on_truncate() {
    let env = envelope(
        "policy.log.truncate",
        serde_json::json!({}),
        "human:lucky",
        false,
    );
    let class = engine().check(&env, &human_subject());
    assert_eq!(class, Some(HardDenyClass::PolicyLogMutation));
}

// ---------------------------------------------------------------------------
// 4. §6 row 4 — hd.evidence_log_mutation
// ---------------------------------------------------------------------------

#[test]
fn evidence_log_mutation_fires_on_tamper() {
    let env = envelope(
        "evidence.tamper",
        serde_json::json!({"record_id": "evr_001"}),
        "human:lucky",
        false,
    );
    let class = engine().check(&env, &human_subject());
    assert_eq!(class, Some(HardDenyClass::EvidenceLogMutation));
}

// ---------------------------------------------------------------------------
// 5. §6 row 5 — hd.disable_policy_kernel
// ---------------------------------------------------------------------------

#[test]
fn disable_policy_kernel_fires_on_kernel_stop() {
    let env = envelope(
        "policy.kernel.stop",
        serde_json::json!({}),
        "human:lucky",
        false,
    );
    let class = engine().check(&env, &human_subject());
    assert_eq!(class, Some(HardDenyClass::DisablePolicyKernel));
}

// ---------------------------------------------------------------------------
// 6. §6 row 6 — hd.disable_recovery_path
// ---------------------------------------------------------------------------

#[test]
fn disable_recovery_path_fires_on_recovery_disable() {
    let env = envelope(
        "recovery.disable",
        serde_json::json!({}),
        "human:lucky",
        false,
    );
    let class = engine().check(&env, &human_subject());
    assert_eq!(class, Some(HardDenyClass::DisableRecoveryPath));
}

// ---------------------------------------------------------------------------
// 7. §6 row 7 — hd.modify_boot_chain (overridable in recovery mode)
// ---------------------------------------------------------------------------

#[test]
fn modify_boot_chain_fires_on_bootloader_modify_even_in_recovery_mode() {
    let env = envelope(
        "bootloader.modify",
        serde_json::json!({"target": "grub.cfg"}),
        "human:lucky",
        false,
    );
    // §6 row 7 has an override path (recovery-mode operator approval), but
    // the engine's verdict stands; the override is a separate downstream
    // flow (T-025) that produces an evidence-linked override receipt.
    let mut sub = human_subject();
    sub.recovery_mode = true;
    let class = engine().check(&env, &sub);
    assert_eq!(class, Some(HardDenyClass::ModifyBootChain));
    // The class is overridable — confirm via the helper.
    assert!(has_recovery_override_path(HardDenyClass::ModifyBootChain));
}

// ---------------------------------------------------------------------------
// 8. §6 row 8 — hd.untyped_shell_privileged
// ---------------------------------------------------------------------------

#[test]
fn untyped_shell_privileged_fires_when_subject_in_privileged_group() {
    let env = envelope(
        "shell.exec_untyped",
        serde_json::json!({"argv": ["rm", "-rf", "/"]}),
        "human:lucky",
        false,
    );
    // Default human_subject() is in "operators" group, which is in
    // privileged_groups per the spec defaults.
    let class = engine().check(&env, &human_subject());
    assert_eq!(class, Some(HardDenyClass::UntypedShellPrivileged));
}

#[test]
fn untyped_shell_privileged_does_not_fire_for_unprivileged_subject() {
    let env = envelope(
        "shell.exec_untyped",
        serde_json::json!({"argv": ["ls"]}),
        "human:guest",
        false,
    );
    // unprivileged_human_subject is in "users" group only, no privileged
    // capabilities — §6 row 8 does NOT fire.
    let class = engine().check(&env, &unprivileged_human_subject());
    assert_ne!(class, Some(HardDenyClass::UntypedShellPrivileged));
}

// ---------------------------------------------------------------------------
// 9. §6 row 9 — hd.aios_fs_pointer_rollback_on_recovery (overridable)
// ---------------------------------------------------------------------------

#[test]
fn aios_fs_pointer_rollback_fires_on_pointer_rollback_action() {
    let env = envelope(
        "aiosfs.pointer.rollback",
        serde_json::json!({"pointer_id": "ptr_recovery_root"}),
        "human:lucky",
        false,
    );
    let class = engine().check(&env, &human_subject());
    assert_eq!(class, Some(HardDenyClass::AiosFsPointerRollbackOnRecovery));
    // Overridable per §6.
    assert!(has_recovery_override_path(
        HardDenyClass::AiosFsPointerRollbackOnRecovery
    ));
}

// ---------------------------------------------------------------------------
// 10. §6 row 10 — hd.privacy_class_downgrade
// ---------------------------------------------------------------------------

#[test]
fn privacy_class_downgrade_fires_when_new_class_lower_than_current() {
    let env = envelope(
        "aiosfs.object.set_privacy_class",
        serde_json::json!({"current_class": "SECRET", "new_class": "INTERNAL"}),
        "human:lucky",
        false,
    );
    let class = engine().check(&env, &human_subject());
    assert_eq!(class, Some(HardDenyClass::PrivacyClassDowngrade));
}

#[test]
fn privacy_class_downgrade_does_not_fire_when_new_class_higher() {
    // Upgrade (INTERNAL -> SECRET) is NOT a downgrade; §6 row 10 does not
    // fire. The engine returns None and the pipeline continues.
    let env = envelope(
        "aiosfs.object.set_privacy_class",
        serde_json::json!({"current_class": "INTERNAL", "new_class": "SECRET"}),
        "human:lucky",
        false,
    );
    let class = engine().check(&env, &human_subject());
    assert_eq!(class, None);
}

// ---------------------------------------------------------------------------
// 11. Clean envelope — no §6 class fires
// ---------------------------------------------------------------------------

#[test]
fn clean_envelope_does_not_trigger_any_hard_deny() {
    // A perfectly normal action (service.restart on nginx) by a human subject
    // touches none of the 10 §6 rows — the engine returns None and the
    // pipeline proceeds to step 5 (and ultimately, in T-017 baseline, to the
    // default-deny floor in step 9 because no bundle rules are loaded).
    let env = envelope(
        "service.restart",
        serde_json::json!({"service": "nginx"}),
        "human:lucky",
        false,
    );
    let class = engine().check(&env, &human_subject());
    assert_eq!(class, None);
}

// ---------------------------------------------------------------------------
// 12. Pipeline integration — kernel + engine short-circuits step 4
// ---------------------------------------------------------------------------

#[tokio::test]
async fn kernel_with_hard_deny_engine_short_circuits_with_canonical_reason_code() {
    // Build a kernel with the engine attached, fire a §6 row, and assert that
    // the pipeline short-circuits at step 4 with a Decision::Deny carrying
    // the canonical "HardDeny:<Variant>" reason_code.
    let kernel = InMemoryPolicyKernel::new_with_hard_deny(HardDenyEngine::new_with_defaults());
    assert!(kernel.has_hard_deny_engine());

    let env = envelope(
        "policy.kernel.stop",
        serde_json::json!({}),
        "human:lucky",
        false,
    );
    let ctx = make_context(human_subject());

    let decision = kernel
        .evaluate_policy(&env, &ctx)
        .await
        .expect("evaluation must not raise");

    assert_eq!(decision.decision, Decision::Deny);
    assert_eq!(
        decision.reason_code,
        reason_code_for(HardDenyClass::DisablePolicyKernel),
        "S2.3 §6 row 5: must short-circuit with HardDeny:DisablePolicyKernel"
    );
    // The engine counted exactly one constitutional rule consultation; the
    // pipeline carries that through to the decision's rules_consulted field.
    assert_eq!(decision.rules_consulted, 1);
    // Decision is NOT the default-deny floor — step 4 fired before step 9.
    assert_ne!(decision.reason_code, "DefaultDeny");
}

// ---------------------------------------------------------------------------
// 13. Override-path-allowed classes still produce DENY but reason_message
//     carries the override-path suffix.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn modify_boot_chain_decision_carries_recovery_override_suffix_in_message() {
    let kernel = InMemoryPolicyKernel::new_with_hard_deny(HardDenyEngine::new_with_defaults());
    let env = envelope(
        "boot.chain.modify",
        serde_json::json!({"slot": "A"}),
        "human:lucky",
        false,
    );
    // Even though subject is in recovery mode (the override path), the
    // engine's verdict stands — DENY with reason_code HardDeny:ModifyBootChain.
    // The reason_message carries the override-path suffix so audit + T-025
    // override engine can spot the overridable class.
    let mut sub = human_subject();
    sub.recovery_mode = true;
    let ctx = make_context(sub);

    let decision = kernel
        .evaluate_policy(&env, &ctx)
        .await
        .expect("evaluation must not raise");

    assert_eq!(decision.decision, Decision::Deny);
    assert_eq!(
        decision.reason_code,
        reason_code_for(HardDenyClass::ModifyBootChain)
    );
    assert!(
        decision
            .reason_message
            .contains("override path: recovery-mode operator approval"),
        "S2.3 §6 row 7 is overridable; reason_message must surface the override path; got: {}",
        decision.reason_message
    );
}

// ---------------------------------------------------------------------------
// 14. Baseline kernel without engine — step 4 still stub-passes (T-017 contract preserved)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn baseline_kernel_without_engine_does_not_short_circuit_at_step_4() {
    // The bare `new()` kernel has no engine attached; even a clearly hard-deny
    // envelope (policy log mutation) falls through step 4 and lands at the
    // default-deny floor. This preserves the T-017 baseline contract: the
    // T-018 engine is OPT-IN at construction time.
    let kernel = InMemoryPolicyKernel::new();
    assert!(!kernel.has_hard_deny_engine());

    let env = envelope(
        "policy.log.truncate",
        serde_json::json!({}),
        "human:lucky",
        false,
    );
    let ctx = make_context(human_subject());

    let decision = kernel
        .evaluate_policy(&env, &ctx)
        .await
        .expect("evaluation must not raise");

    // Without the engine the §6 step is a no-op pass-through; the evaluation
    // lands at the constitutional default-deny floor (S2.3 §11).
    assert_eq!(decision.decision, Decision::Deny);
    assert_eq!(decision.reason_code, "DefaultDeny");
}
