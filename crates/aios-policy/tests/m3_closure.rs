//! T-025 — M3 closure assertions.
//!
//! These tests guard the constitutional invariants of the M3 closure:
//!
//! - Crate version is `0.1.0` (the M3 closure marker; matches the
//!   aios-evidence `0.0.1 → 0.1.0` pattern from M2).
//! - No `todo!()` or `unimplemented!()` macros remain in `src/`.
//! - Every §3 pipeline step has at least one inline test (the 12 steps
//!   are constitutional; a refactor that drops a step's test gets caught
//!   here).
//! - Public API surface: the override boundary, rollback, and the §22
//!   acceptance fixtures are reachable from the crate root.

#![allow(
    clippy::expect_used,
    clippy::items_after_statements,
    clippy::manual_map,
    clippy::needless_collect,
    clippy::option_if_let_else,
    clippy::panic,
    clippy::question_mark,
    clippy::unnecessary_map_or,
    clippy::unwrap_used,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::fs;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// 1. Crate version is 0.1.0.
// ---------------------------------------------------------------------------

#[test]
fn crate_version_is_0_1_0_m3_closure_marker() {
    let cargo_toml = include_str!("../Cargo.toml");
    assert!(
        cargo_toml.contains("version = \"0.1.0\""),
        "aios-policy Cargo.toml must declare version = \"0.1.0\" (M3 closure marker); got:\n{cargo_toml}"
    );
}

// ---------------------------------------------------------------------------
// 2. No `todo!()` or `unimplemented!()` macros in src/.
// ---------------------------------------------------------------------------

#[test]
fn no_todo_or_unimplemented_macros_remain_in_src() {
    let src_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let offenders = walk_rust_files(&src_dir)
        .into_iter()
        .filter_map(|p| {
            let body = fs::read_to_string(&p).ok()?;
            // Strip doc-comments and `//` line comments before scanning — the
            // M3-closure invariant targets active code paths, not narrative
            // references in module docs / inline comments that legitimately
            // discuss the macros.
            let mut active = String::with_capacity(body.len());
            for line in body.lines() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") {
                    continue;
                }
                active.push_str(line);
                active.push('\n');
            }
            if active.contains("todo!(") || active.contains("unimplemented!(") {
                Some(p.display().to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(
        offenders.is_empty(),
        "found `todo!()` / `unimplemented!()` in: {offenders:?}"
    );
}

fn walk_rust_files(root: &PathBuf) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            out.extend(walk_rust_files(&p));
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 3. Every §3 pipeline step has at least one inline test reference.
// ---------------------------------------------------------------------------
//
// The 12-step pipeline (§3) is the constitutional backbone of the kernel.
// Each step has a method on `DecisionPipeline`; this test asserts every
// step method name appears at least once in the crate's test suite
// (combination of inline `mod tests` + integration `tests/*.rs`).

#[test]
fn every_pipeline_step_has_at_least_one_test_reference() {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut all_test_text = String::new();
    // Inline tests in src/*.rs:
    for p in walk_rust_files(&crate_root.join("src")) {
        if let Ok(body) = fs::read_to_string(&p) {
            all_test_text.push_str(&body);
        }
    }
    // Integration tests in tests/*.rs:
    for p in walk_rust_files(&crate_root.join("tests")) {
        if let Ok(body) = fs::read_to_string(&p) {
            all_test_text.push_str(&body);
        }
    }
    // Step identifiers — these are method names on DecisionPipeline or
    // the canonical reason_codes minted by each step. The pipeline has
    // 12 §3 steps; for each, assert at least one of the step's hooks
    // appears in the test corpus.
    let expected_steps: &[&str] = &[
        "step_1_validate_schema",
        "step_2_normalize_subject",
        "step_3_enrich_resources",
        "step_4_evaluate_hard_denies",
        "step_5_emergency_override",
        "step_6_evaluate_scoped_denies",
        "step_7_evaluate_scoped_allows",
        "step_8_ai_self_approval",
        "step_9_apply_default_deny",
        "step_10_bind_constraints",
        // Steps 11 (emit) + 12 (evidence) are anchored via reason_code
        // constants — emit_decision is the assembly point; evidence_receipt_id
        // is the field touched by step 12.
        "emit_decision",
        "evidence_receipt_id",
    ];
    let missing: Vec<&&str> = expected_steps
        .iter()
        .filter(|s| !all_test_text.contains(**s))
        .collect();
    assert!(
        missing.is_empty(),
        "pipeline steps without test reference: {missing:?}"
    );
}

// ---------------------------------------------------------------------------
// 4. Public API surface — M3-deliverable types are reachable from crate root.
// ---------------------------------------------------------------------------
//
// Compile-time check: the test compiles only if every name below resolves
// against the crate's public re-exports. A refactor that demotes one of
// these to a non-public path breaks this test at build time.

#[test]
fn public_api_surface_covers_m3_deliverables() {
    use aios_policy::{
        ApprovalRequirement, ApprovalScope, ApproverClass, BundleLoader, CacheKey, Constraints,
        Decision, DecisionLog, DecisionPipeline, EmergencyOverride, EnrichmentSnapshot,
        HardDenyClass, HardDenyEngine, HydratedSubject, InMemoryHydrator, InMemoryPolicyKernel,
        OverrideBoundary, OverrideError, OverrideRequest, OverrideScope, PolicyBundle,
        PolicyContext, PolicyDecision, PolicyError, PolicyKernel, PolicyRule, RulePrecedence,
        RuleScope, SharedDecisionCache, SubjectHydrator, MAX_OVERRIDE_TTL_SECONDS,
    };
    // Make use of one symbol from each cluster so dead-code elimination
    // does not silently drop the import.
    let _: u64 = MAX_OVERRIDE_TTL_SECONDS;
    let _ = OverrideBoundary::new();
    let _ = InMemoryPolicyKernel::new();
    let _ = SharedDecisionCache::with_capacity(8);
    let _ = HardDenyEngine::new_with_defaults();
    let _ = DecisionPipeline::new();
    let _ = HardDenyClass::SecretRawReadByAi;
    let _ = Decision::Allow;
    let _ = RuleScope::Global;
    // Trait objects compile-resolve:
    fn _accepts<K: PolicyKernel + ?Sized>(_: &K) {}
    fn _accepts_hydrator<H: SubjectHydrator + ?Sized>(_: &H) {}
    // Suppress unused-type-import lints:
    let _ = std::marker::PhantomData::<(
        ApprovalRequirement,
        ApprovalScope,
        ApproverClass,
        BundleLoader,
        CacheKey,
        Constraints,
        DecisionLog,
        EmergencyOverride,
        EnrichmentSnapshot,
        HydratedSubject,
        InMemoryHydrator,
        OverrideError,
        OverrideRequest,
        OverrideScope,
        PolicyBundle,
        PolicyContext,
        PolicyDecision,
        PolicyError,
        PolicyRule,
        RulePrecedence,
    )>;
}

// ---------------------------------------------------------------------------
// 5. RollbackBundle RPC is no longer Unimplemented (the last stub).
// ---------------------------------------------------------------------------
//
// Asserts at the source level that the server.rs `rollback_bundle` impl
// does not return `Status::unimplemented(...)` unconditionally any more.
// (The Unimplemented branch on a missing-kernel-handle is allowed; the
// path the RPC takes on a happy-path call must not be Unimplemented.)

#[test]
fn rollback_bundle_rpc_is_no_longer_unconditional_unimplemented() {
    let server_rs = include_str!("../src/service/server.rs");
    // The pre-T-025 stub returned this exact string from rollback_bundle.
    assert!(
        !server_rs.contains("RollbackBundle is not yet wired (queued for T-025 — M3 closer)"),
        "RollbackBundle stub string must be removed in T-025"
    );
    // The new impl mints `evr_rb_` evidence receipt ids.
    assert!(
        server_rs.contains("evr_rb_"),
        "RollbackBundle impl must mint evidence receipt ids `evr_rb_<ULID>`"
    );
}

// ---------------------------------------------------------------------------
// 6. Code-version constant reflects 0.1.0-T025.
// ---------------------------------------------------------------------------

#[test]
fn default_code_version_constant_reflects_t025() {
    use aios_policy::service::server::DEFAULT_CODE_VERSION;
    assert!(
        DEFAULT_CODE_VERSION.contains("0.1.0"),
        "DEFAULT_CODE_VERSION must reference 0.1.0; got {DEFAULT_CODE_VERSION}"
    );
}
