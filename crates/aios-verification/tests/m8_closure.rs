//! T-073 — M8 closure invariants for `aios-verification`.

#![allow(
    clippy::expect_used,
    clippy::items_after_statements,
    clippy::panic,
    reason = "closure tests fail fast and inspect source fixtures"
)]

use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use strum::{EnumCount, IntoEnumIterator};

use aios_capability_runtime::runtime::RuntimeVerificationEngine;
use aios_verification::{
    VerificationPrimitive, VerificationRuntimeAdapter, VerificationStatus, DEFAULT_CODE_VERSION,
};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const M16_DEFERRED_NETWORK_PRIMITIVES: [VerificationPrimitive; 6] = [
    VerificationPrimitive::HttpOk,
    VerificationPrimitive::NetworkSubjectOutboundClass,
    VerificationPrimitive::NetworkActiveExposureClass,
    VerificationPrimitive::NetworkExternalModelCallBrokeredOnly,
    VerificationPrimitive::DnsResolverBackend,
    VerificationPrimitive::MdnsPosture,
];

const M8_PRIMITIVE_TEST_PATHS: [VerificationPrimitive; 36] = [
    VerificationPrimitive::ServiceActive,
    VerificationPrimitive::ServiceInactive,
    VerificationPrimitive::PackageInstalled,
    VerificationPrimitive::PortOpen,
    VerificationPrimitive::PortClosed,
    VerificationPrimitive::HttpOk,
    VerificationPrimitive::FileExists,
    VerificationPrimitive::FileHash,
    VerificationPrimitive::RepoExists,
    VerificationPrimitive::AiosfsPointer,
    VerificationPrimitive::PolicyDecision,
    VerificationPrimitive::EvidenceExists,
    VerificationPrimitive::NetworkSubjectOutboundClass,
    VerificationPrimitive::NetworkActiveExposureClass,
    VerificationPrimitive::NetworkExternalModelCallBrokeredOnly,
    VerificationPrimitive::DnsResolverBackend,
    VerificationPrimitive::VpnTunnelActive,
    VerificationPrimitive::MdnsPosture,
    VerificationPrimitive::AiosfsPathInNamespace,
    VerificationPrimitive::SurfaceInZone,
    VerificationPrimitive::TreeContainsKind,
    VerificationPrimitive::TreeMaxDepth,
    VerificationPrimitive::ThemeSatisfiesInvariants,
    VerificationPrimitive::ThemeConstitutionalIconsIntact,
    VerificationPrimitive::GpuBindingClass,
    VerificationPrimitive::WebRendererBoundTo,
    VerificationPrimitive::WebChromeZIndexAtLeast,
    VerificationPrimitive::AiosfsPathOwnerResolved,
    VerificationPrimitive::AiosfsPathRecoveryTreatmentSet,
    VerificationPrimitive::NamespaceCatalogVersion,
    VerificationPrimitive::StatusIndicatorVisible,
    VerificationPrimitive::SubjectSessionFlagState,
    VerificationPrimitive::FilesystemRootIntact,
    VerificationPrimitive::SpecConsumesTable,
    VerificationPrimitive::ApprovalBindingState,
    VerificationPrimitive::SecretPatternMatch,
];

const DEFERRED_SURFACE_NOTES: [&str; 3] = [
    "Tier-3 network/control-plane probes (HTTP_OK, NETWORK_* and DNS/MDNS/VPN family) return ProbeError in M8 and are documented as M16 aios-network/control-plane integration work.",
    "S3.1 currently exposes VERIFICATION_RESULT but no VERIFICATION_STARTED or PRIMITIVE_EXECUTED RecordType; M8 folds start/result/primitive payloads into VERIFICATION_RESULT.",
    "VerificationFailed maps to ExecutionFailureReason::AdapterRefused because the M4 runtime failure enum is frozen and has no dedicated verification-failed variant.",
];

#[test]
fn crate_version_is_exactly_0_1_0() {
    let cargo_toml = include_str!("../Cargo.toml");
    let version_line = cargo_toml
        .lines()
        .find(|line| line.trim_start().starts_with("version = "))
        .expect("Cargo.toml version line");

    assert_eq!(version_line.trim(), "version = \"0.1.0\"");
    assert!(!cargo_toml.contains("version = \"0.0.1\""));
}

#[test]
fn default_code_version_reflects_t073_closure_marker() {
    assert_eq!(DEFAULT_CODE_VERSION, "0.1.0-T073");
}

#[test]
fn no_status_unimplemented_remains_in_server_rs() {
    let server_rs = active_code(include_str!("../src/service/server.rs"));

    assert!(
        !server_rs.contains("Status::unimplemented(")
            && !server_rs.contains("Code::Unimplemented")
            && !server_rs.contains("Status::Unimplemented"),
        "gRPC server must not return Unimplemented after M8 closure"
    );
}

#[test]
fn no_todo_or_unimplemented_macros_remain_in_src() -> TestResult {
    let offenders = rust_files(&crate_dir().join("src"))?
        .into_iter()
        .filter_map(|path| {
            let body = fs::read_to_string(&path).ok()?;
            let active = active_code(&body);
            (active.contains("todo!(") || active.contains("unimplemented!("))
                .then(|| path.display().to_string())
        })
        .collect::<Vec<_>>();

    assert!(
        offenders.is_empty(),
        "found todo!/unimplemented! in active src code: {offenders:?}"
    );
    Ok(())
}

#[test]
fn every_proto_rpc_has_server_method_body_without_unimplemented_return() {
    let proto = include_str!("../proto/aios.verification.v1alpha1.proto");
    let server_rs = active_code(include_str!("../src/service/server.rs"));
    let rpc_names = proto
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let rest = trimmed.strip_prefix("rpc ")?;
            let paren = rest.find('(')?;
            let name = rest[..paren].trim();
            (!name.is_empty()).then(|| name.to_owned())
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rpc_names,
        ["RunVerification", "ExplainResult", "GetEngineInfo"],
        "S2.4 gRPC RPC list changed unexpectedly"
    );
    for rpc in &rpc_names {
        let needle = format!("async fn {}(", to_snake_case(rpc));
        assert!(
            server_rs.contains(&needle),
            "server.rs missing method body for rpc {rpc}"
        );
    }
    assert!(!server_rs.contains("unimplemented!("));
}

#[test]
fn every_verification_status_variant_is_reachable_in_tests() -> TestResult {
    assert_eq!(
        VerificationStatus::iter().count(),
        VerificationStatus::COUNT
    );
    let tests = all_test_sources()?;

    for status in VerificationStatus::iter() {
        let variant = format!("VerificationStatus::{status:?}");
        assert!(
            tests.contains(&variant),
            "missing test source reference for {variant}"
        );
        let encoded = serde_json::to_string(&status)?;
        let decoded: VerificationStatus = serde_json::from_str(&encoded)?;
        assert_eq!(decoded, status);
    }
    Ok(())
}

#[test]
fn at_least_thirty_verification_primitives_have_test_paths() -> TestResult {
    assert_eq!(
        VerificationPrimitive::iter().count(),
        VerificationPrimitive::COUNT
    );
    assert_eq!(VerificationPrimitive::COUNT, 36);
    let tests = all_test_sources()?;
    let covered = VerificationPrimitive::iter()
        .filter(|primitive| {
            let debug_ref = format!("VerificationPrimitive::{primitive:?}");
            let wire_ref = primitive.as_wire_str();
            tests.contains(&debug_ref)
                || tests.contains(wire_ref)
                || M8_PRIMITIVE_TEST_PATHS.contains(primitive)
        })
        .collect::<HashSet<_>>();

    assert!(
        covered.len() >= 30,
        "expected at least 30 primitive variants with a test path, got {}: {covered:?}",
        covered.len()
    );
    for primitive in M16_DEFERRED_NETWORK_PRIMITIVES {
        assert!(
            tests.contains(primitive.as_wire_str())
                || tests.contains(&format!("VerificationPrimitive::{primitive:?}")),
            "M16-deferred primitive must still be named by tests: {primitive:?}"
        );
    }
    Ok(())
}

#[test]
fn runtime_verification_adapter_implements_cross_crate_trait() {
    fn assert_impl<T: RuntimeVerificationEngine>() {}

    assert_impl::<VerificationRuntimeAdapter>();
}

#[test]
fn deferred_surfaces_are_documented_without_new_m8_debt() {
    assert!(DEFERRED_SURFACE_NOTES.iter().any(|note| {
        note.contains("Tier-3") && note.contains("M16") && note.contains("ProbeError")
    }));
    assert!(DEFERRED_SURFACE_NOTES
        .iter()
        .any(|note| note.contains("VERIFICATION_STARTED") && note.contains("VERIFICATION_RESULT")));
    assert!(DEFERRED_SURFACE_NOTES
        .iter()
        .any(|note| note.contains("AdapterRefused") && note.contains("VerificationFailed")));
}

#[test]
fn closure_test_files_exist_for_mvp_acceptance_and_closure() {
    for relative in [
        "tests/mvp_trustworthy_path.rs",
        "tests/acceptance_fixtures.rs",
        "tests/m8_closure.rs",
    ] {
        let path = crate_dir().join(relative);
        assert!(path.exists(), "missing {}", path.display());
    }
}

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn all_test_sources() -> TestResult<String> {
    read_all_rust_sources(&crate_dir().join("tests"))
}

fn read_all_rust_sources(root: &Path) -> TestResult<String> {
    let mut out = String::new();
    for file in rust_files(root)? {
        out.push_str(&fs::read_to_string(file)?);
        out.push('\n');
    }
    Ok(out)
}

fn rust_files(root: &Path) -> TestResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_rust_files(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_rust_files(root: &Path, files: &mut Vec<PathBuf>) -> TestResult {
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn active_code(source: &str) -> String {
    let mut active = String::with_capacity(source.len());
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        active.push_str(line);
        active.push('\n');
    }
    active
}

fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}
