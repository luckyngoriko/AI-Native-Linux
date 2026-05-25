//! T-083 M9 closure invariants.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "closure tests fail loudly on milestone drift"
)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use aios_recovery::first_boot::FIRST_BOOT_PROVISIONING_PHASES;
use aios_recovery::{CandidateState, FirstBootPhase, RecoveryMode, DEFAULT_CODE_VERSION};
use strum::{EnumCount, IntoEnumIterator};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const FIRST_BOOT_PHASE_TEST_PATHS: [FirstBootPhase; 15] = [
    FirstBootPhase::StageInstallerMediaVerified,
    FirstBootPhase::StageDiskPartitioned,
    FirstBootPhase::StageKernelInstalled,
    FirstBootPhase::StageAiosFsInitialized,
    FirstBootPhase::StageVaultRootGenerated,
    FirstBootPhase::StageInvariantBundleLoaded,
    FirstBootPhase::StagePolicyBundleLoaded,
    FirstBootPhase::StageIdentityBundleLoaded,
    FirstBootPhase::StageRecoveryOperatorRegistration,
    FirstBootPhase::StageAiProviderConfiguration,
    FirstBootPhase::StageFirstGroupRegistration,
    FirstBootPhase::StageFirstUserRegistration,
    FirstBootPhase::StageRuntimeServicesStarted,
    FirstBootPhase::StageFirstBootComplete,
    FirstBootPhase::StageFailedRequiresRecovery,
];

const CANDIDATE_STATE_TEST_PATHS: [CandidateState; 9] = [
    CandidateState::Building,
    CandidateState::Built,
    CandidateState::Gating,
    CandidateState::GatePassed,
    CandidateState::GateFailed,
    CandidateState::APromoted,
    CandidateState::BDemotedToA,
    CandidateState::Rollback,
    CandidateState::Retired,
];

const RECOVERY_MODE_TEST_PATHS: [RecoveryMode; 4] = [
    RecoveryMode::Normal,
    RecoveryMode::Recovery,
    RecoveryMode::Degraded,
    RecoveryMode::FirstBoot,
];

const DEFERRED_SURFACE_NOTES: [&str; 5] = [
    "T-076: FirstBootPhase has 15 enum entries; FIRST_BOOT_PROVISIONING_PHASES has 14 happy-path stages because StageFailedRequiresRecovery is a terminal failure state.",
    "T-077: KernelPipelineDriver verifies the S9.3 FSM and signed manifest with a shallow gate-pass witness; deep six-gate attestation remains a later kernel-verification milestone, not a fake T-083 pass.",
    "T-080: recovery evidence uses the current S3.1 Rust vocabulary and maps candidate registration to KERNEL_PIPELINE_STARTED; no new evidence enum was added in M9.",
    "T-081: AdapterManifest has no requires_recovery field; RecoveryRuntimeAdapter gates S10.1 W8/W9 recovery-only action_kind values against live recovery mode.",
    "T-082: EnterRecovery gRPC returns RecoveryState, not an exit_token; in-process tests read current_exit_token from InMemoryRecoveryBoundary and the CLI renders the returned state.",
];

#[test]
fn crate_version_is_exactly_0_1_0() {
    let cargo_toml = include_str!("../Cargo.toml");

    assert!(
        cargo_toml
            .lines()
            .any(|line| line.trim() == "version = \"0.1.0\""),
        "aios-recovery Cargo.toml must carry the M9 closure version"
    );
}

#[test]
fn default_code_version_reflects_t083_closure_marker() {
    assert_eq!(DEFAULT_CODE_VERSION, "0.1.0-T083");
}

#[test]
fn no_status_unimplemented_remains_in_server_rs() {
    let server = include_str!("../src/service/server.rs");

    for forbidden in [
        "Status::unimplemented(",
        "Status::Unimplemented",
        "Code::Unimplemented",
        "unimplemented!(",
    ] {
        assert!(
            !server.contains(forbidden),
            "server.rs still contains {forbidden}"
        );
    }
}

#[test]
fn no_todo_or_unimplemented_macros_remain_in_src() -> TestResult {
    let todo_macro = ["todo", "!("].concat();
    let unimplemented_macro = ["unimplemented", "!("].concat();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");

    for file in rust_files(&root)? {
        let source = active_code(&fs::read_to_string(&file)?);
        assert!(
            !source.contains(&todo_macro),
            "{} contains a todo macro",
            file.display()
        );
        assert!(
            !source.contains(&unimplemented_macro),
            "{} contains an unimplemented macro",
            file.display()
        );
    }
    Ok(())
}

#[test]
fn every_proto_rpc_has_server_method_body_without_unimplemented_return() {
    let proto = include_str!("../proto/aios.recovery.v1alpha1.proto");
    let server = include_str!("../src/service/server.rs");
    let rpcs = proto
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix("rpc ")
                .and_then(|rest| rest.split_once('(').map(|(name, _)| name.to_owned()))
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rpcs,
        vec![
            "EnterRecovery",
            "ExitRecovery",
            "GetRecoveryState",
            "RegisterKernelCandidate",
            "VerifyKernelCandidate",
            "ActivateKernelCandidate",
            "RollbackKernelCandidate",
            "ListKernelCandidates",
            "GetActiveKernel",
            "RunFirstBootProvisioning",
            "GetFirstBootStatus",
            "CheckRecoveryMutation",
        ]
    );

    for rpc in rpcs {
        let method = format!("async fn {}(", to_snake_case(&rpc));
        assert!(server.contains(&method), "missing server impl for {rpc}");
    }
}

#[test]
fn every_recovery_mode_variant_is_exercised() -> TestResult {
    assert_eq!(RecoveryMode::COUNT, 4);
    assert_eq!(
        RecoveryMode::iter().collect::<Vec<_>>(),
        RECOVERY_MODE_TEST_PATHS
    );

    for mode in RecoveryMode::iter() {
        let wire = serde_json::to_string(&mode)?;
        let parsed: RecoveryMode = serde_json::from_str(&wire)?;
        assert_eq!(parsed, mode);
    }
    Ok(())
}

#[test]
fn every_candidate_state_variant_is_exercised() -> TestResult {
    assert_eq!(CandidateState::COUNT, 9);
    assert_eq!(
        CandidateState::iter().collect::<Vec<_>>(),
        CANDIDATE_STATE_TEST_PATHS
    );

    for state in CandidateState::iter() {
        assert!(!state.as_wire_str().is_empty());
        let wire = serde_json::to_string(&state)?;
        let parsed: CandidateState = serde_json::from_str(&wire)?;
        assert_eq!(parsed, state);
    }
    Ok(())
}

#[test]
fn every_first_boot_phase_variant_has_at_least_one_test_path() -> TestResult {
    assert_eq!(FirstBootPhase::COUNT, 15);
    assert_eq!(
        FirstBootPhase::iter().collect::<Vec<_>>(),
        FIRST_BOOT_PHASE_TEST_PATHS
    );

    for phase in FirstBootPhase::iter() {
        let wire = serde_json::to_string(&phase)?;
        let parsed: FirstBootPhase = serde_json::from_str(&wire)?;
        assert_eq!(parsed, phase);
    }
    Ok(())
}

#[test]
fn first_boot_happy_path_documents_fourteen_phases_plus_terminal_failure() {
    assert_eq!(FIRST_BOOT_PROVISIONING_PHASES.len(), 14);
    assert!(!FIRST_BOOT_PROVISIONING_PHASES.contains(&FirstBootPhase::StageFailedRequiresRecovery));
    assert!(FIRST_BOOT_PHASE_TEST_PATHS.contains(&FirstBootPhase::StageFailedRequiresRecovery));
}

#[test]
fn deferred_surfaces_are_documented_without_new_m9_debt() {
    assert_eq!(DEFERRED_SURFACE_NOTES.len(), 5);
    for note in DEFERRED_SURFACE_NOTES {
        assert!(note.contains("T-0"));
        assert!(!note.contains("UNKNOWN"));
    }
}

#[test]
fn closure_test_files_exist_for_mvp_acceptance_and_invariants() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for path in [
        "tests/mvp_full_real_path.rs",
        "tests/acceptance_fixtures.rs",
        "tests/m9_closure.rs",
    ] {
        assert!(manifest_dir.join(path).is_file(), "missing {path}");
    }
}

fn rust_files(root: &Path) -> TestResult<Vec<PathBuf>> {
    let mut out = Vec::new();
    collect_rust_files(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> TestResult {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

fn active_code(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n")
}

fn to_snake_case(name: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if idx > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}
