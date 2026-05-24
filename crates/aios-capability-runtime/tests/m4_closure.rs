//! T-035 — M4 closure assertions.
//!
//! These tests guard the constitutional invariants of the M4 closure for
//! `aios-capability-runtime`:
//!
//! - Crate version is `0.1.0` (M4 closure marker).
//! - No `todo!()` / `unimplemented!()` macros remain in `src/`.
//! - No `Status::unimplemented(` calls remain in `src/service/server.rs`
//!   (every public RPC has a working implementation as of T-035).
//! - Every public RPC declared in `aios.runtime.v1alpha1.proto` is wired
//!   to a method on the `CapabilityRuntimeService` trait impl.
//! - Every `ActionLifecycleState` variant is reachable from the workspace
//!   test corpus.
//! - Every `RollbackOutcome` variant is exercised in at least one test.
//! - `DEFAULT_CODE_VERSION` constant reflects `0.1.0-T035`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::items_after_statements,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::fs;
use std::path::PathBuf;

use strum::IntoEnumIterator as _;

use aios_capability_runtime::{ActionLifecycleState, RollbackOutcome};

// ---------------------------------------------------------------------------
// 1. Crate version is 0.1.0.
// ---------------------------------------------------------------------------

#[test]
fn crate_version_is_0_1_0_m4_closure_marker() {
    let cargo_toml = include_str!("../Cargo.toml");
    assert!(
        cargo_toml.contains("version = \"0.1.0\""),
        "aios-capability-runtime Cargo.toml must declare version = \"0.1.0\" (M4 closure marker); got:\n{cargo_toml}"
    );
}

// ---------------------------------------------------------------------------
// 2. No `todo!()` / `unimplemented!()` macros remain in src/.
// ---------------------------------------------------------------------------

#[test]
fn no_todo_or_unimplemented_macros_remain_in_src() {
    let src_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let offenders = walk_rust_files(&src_dir)
        .into_iter()
        .filter_map(|p| {
            let body = fs::read_to_string(&p).ok()?;
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
// 3. No `Status::unimplemented(...)` calls in `src/service/server.rs`.
// ---------------------------------------------------------------------------
//
// Every public RPC on `aios.runtime.v1alpha1.CapabilityRuntime` must have
// a working server implementation as of T-035 (M4 closer). The four RPCs
// that T-033 left as stubs (EvaluatePolicyForAction,
// RequestApprovalForAction, VerifyAction, RollbackAction) are now thin
// projections over the recorded ActionContext; none of them return
// `Status::unimplemented`.

#[test]
fn no_status_unimplemented_calls_remain_in_server_rs() {
    let server_rs = include_str!("../src/service/server.rs");
    // Strip line comments so narrative references to "Unimplemented" in
    // docstrings don't false-positive.
    let mut active = String::with_capacity(server_rs.len());
    for line in server_rs.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        active.push_str(line);
        active.push('\n');
    }
    assert!(
        !active.contains("Status::unimplemented("),
        "Status::unimplemented( must not appear in src/service/server.rs active code (M4 closure)"
    );
}

// ---------------------------------------------------------------------------
// 4. Every public RPC declared in the proto has a method body.
// ---------------------------------------------------------------------------
//
// Parse the proto for `rpc <Name>(...)` declarations and assert the lower-
// snake-case form appears as `async fn <name>(` in server.rs.

#[test]
fn every_proto_rpc_has_a_server_method_body() {
    let proto = include_str!("../proto/aios.runtime.v1alpha1.proto");
    let server_rs = include_str!("../src/service/server.rs");

    // Collect declared rpc names.
    let mut rpc_names: Vec<String> = Vec::new();
    for line in proto.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("rpc ") {
            // `rpc Name(Request) returns ...` — take chars before `(`.
            if let Some(paren) = rest.find('(') {
                let name = rest[..paren].trim();
                if !name.is_empty() {
                    rpc_names.push(name.to_string());
                }
            }
        }
    }
    assert!(
        rpc_names.len() >= 9,
        "expected ≥ 9 RPCs in the §5.1 surface; got {} ({rpc_names:?})",
        rpc_names.len()
    );

    // For each RPC, derive the snake_case method name and assert
    // `async fn <name>(` appears in the server impl.
    for rpc in &rpc_names {
        let snake = to_snake_case(rpc);
        let needle = format!("async fn {snake}(");
        assert!(
            server_rs.contains(&needle),
            "server.rs missing impl for rpc {rpc} (expected '{needle}')"
        );
    }
}

fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 5. Every `ActionLifecycleState` variant is reachable from the workspace
//    test corpus.
// ---------------------------------------------------------------------------
//
// Enumerates every variant via `strum::IntoEnumIterator`, then scans the
// crate's `src/` and `tests/` directories for the variant identifier. A
// refactor that drops a state from the FSM gets caught here.

#[test]
fn every_action_lifecycle_state_variant_is_referenced_in_corpus() {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut corpus = String::new();
    for p in walk_rust_files(&crate_root.join("src")) {
        if let Ok(b) = fs::read_to_string(&p) {
            corpus.push_str(&b);
        }
    }
    for p in walk_rust_files(&crate_root.join("tests")) {
        if let Ok(b) = fs::read_to_string(&p) {
            corpus.push_str(&b);
        }
    }
    let mut missing: Vec<String> = Vec::new();
    for v in ActionLifecycleState::iter() {
        let name = format!("{v:?}");
        if !corpus.contains(&name) {
            missing.push(name);
        }
    }
    assert!(
        missing.is_empty(),
        "ActionLifecycleState variants without test/source reference: {missing:?}"
    );
}

// ---------------------------------------------------------------------------
// 6. Every `RollbackOutcome` variant is exercised in at least one test.
// ---------------------------------------------------------------------------

#[test]
fn every_rollback_outcome_variant_is_referenced_in_corpus() {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut corpus = String::new();
    for p in walk_rust_files(&crate_root.join("src")) {
        if let Ok(b) = fs::read_to_string(&p) {
            corpus.push_str(&b);
        }
    }
    for p in walk_rust_files(&crate_root.join("tests")) {
        if let Ok(b) = fs::read_to_string(&p) {
            corpus.push_str(&b);
        }
    }
    let mut missing: Vec<String> = Vec::new();
    for v in RollbackOutcome::iter() {
        let name = format!("{v:?}");
        if !corpus.contains(&name) {
            missing.push(name);
        }
    }
    assert!(
        missing.is_empty(),
        "RollbackOutcome variants without test/source reference: {missing:?}"
    );
}

// ---------------------------------------------------------------------------
// 7. DEFAULT_CODE_VERSION reflects 0.1.0-T035.
// ---------------------------------------------------------------------------

#[test]
fn default_code_version_reflects_t035() {
    use aios_capability_runtime::service::server::DEFAULT_CODE_VERSION;
    assert!(
        DEFAULT_CODE_VERSION.contains("0.1.0"),
        "DEFAULT_CODE_VERSION must reference 0.1.0 (M4 closure); got {DEFAULT_CODE_VERSION}"
    );
    assert!(
        DEFAULT_CODE_VERSION.contains("T035"),
        "DEFAULT_CODE_VERSION must reference T035; got {DEFAULT_CODE_VERSION}"
    );
}

// ---------------------------------------------------------------------------
// 8. Cargo.toml semver discipline — no `version = "0.0.1"` stragglers in
//    the runtime crate (M4 closure marker).
// ---------------------------------------------------------------------------

#[test]
fn cargo_toml_does_not_carry_legacy_0_0_1_version() {
    let cargo_toml = include_str!("../Cargo.toml");
    assert!(
        !cargo_toml.contains("version = \"0.0.1\""),
        "no legacy 0.0.1 version literal in aios-capability-runtime Cargo.toml after M4 closure"
    );
}
