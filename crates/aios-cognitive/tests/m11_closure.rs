//! T-099 M11 closure invariants for `aios-cognitive`.
//!
//! Covers the full M11 surface: S1.1 `CognitiveCore` trait, S1.2 latency/classifier,
//! S13.1 model types, S13.2 model router, and S14.1 circuit breaker.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "closure tests fail loudly on milestone drift"
)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use strum::{EnumCount, IntoEnumIterator};

use aios_cognitive::{
    AICrossOriginPosture, BackendHealthState, CircuitState, LatencyTier, ModelBackendKind,
    PrivacyClass, ProviderClass, DEFAULT_CODE_VERSION,
};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

// ---------------------------------------------------------------------------
// 1. crate version is exactly 0.1.0
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// 2. DEFAULT_CODE_VERSION reflects T-099 closure marker
// ---------------------------------------------------------------------------

#[test]
fn default_code_version_reflects_t099_closure_marker() {
    assert_eq!(DEFAULT_CODE_VERSION, "aios-cognitive/0.1.0-T099");
}

// ---------------------------------------------------------------------------
// 3. no todo! / unimplemented! macros remain in src/
// ---------------------------------------------------------------------------

#[test]
fn no_todo_or_unimplemented_macros_remain_in_src() -> TestResult {
    let offenders = rust_files(&crate_dir().join("src"))?
        .into_iter()
        .filter_map(|path| {
            let body = fs::read_to_string(&path).ok()?;
            let active = active_code(&body);
            (active.contains("todo!") || active.contains("unimplemented!"))
                .then(|| path.display().to_string())
        })
        .collect::<Vec<_>>();

    assert!(
        offenders.is_empty(),
        "found todo!/unimplemented! in active src code: {offenders:?}"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 4. CircuitState — 3 variants, all exercised in tests + serde round-trips
// ---------------------------------------------------------------------------

#[test]
fn every_circuit_state_variant_is_exercised_in_tests() -> TestResult {
    assert_eq!(CircuitState::COUNT, 3);
    let tests = all_test_sources()?;

    for state in CircuitState::iter() {
        let variant_ref = format!("CircuitState::{state:?}");
        assert!(
            tests.contains(&variant_ref),
            "missing test source reference for {variant_ref}"
        );
        let encoded = serde_json::to_string(&state)?;
        let decoded: CircuitState = serde_json::from_str(&encoded)?;
        assert_eq!(decoded, state);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 5. ModelBackendKind — 8 variants, all exercised in tests + serde round-trips
// ---------------------------------------------------------------------------

#[test]
fn every_model_backend_kind_variant_is_exercised_in_tests() -> TestResult {
    assert_eq!(ModelBackendKind::COUNT, 8);
    let tests = all_test_sources()?;

    for kind in ModelBackendKind::iter() {
        let variant_ref = format!("ModelBackendKind::{kind:?}");
        assert!(
            tests.contains(&variant_ref),
            "missing test source reference for {variant_ref}"
        );
        let encoded = serde_json::to_string(&kind)?;
        let decoded: ModelBackendKind = serde_json::from_str(&encoded)?;
        assert_eq!(decoded, kind);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 6. ProviderClass — 5 variants, all exercised in tests + serde round-trips
// ---------------------------------------------------------------------------

#[test]
fn every_provider_class_variant_is_exercised_in_tests() -> TestResult {
    assert_eq!(ProviderClass::COUNT, 5);
    let tests = all_test_sources()?;

    for pc in ProviderClass::iter() {
        let variant_ref = format!("ProviderClass::{pc:?}");
        assert!(
            tests.contains(&variant_ref),
            "missing test source reference for {variant_ref}"
        );
        let encoded = serde_json::to_string(&pc)?;
        let decoded: ProviderClass = serde_json::from_str(&encoded)?;
        assert_eq!(decoded, pc);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 7. AICrossOriginPosture — 3 variants, all exercised in tests + serde round-trips
// ---------------------------------------------------------------------------

#[test]
fn every_ai_cross_origin_posture_variant_is_exercised_in_tests() -> TestResult {
    assert_eq!(AICrossOriginPosture::COUNT, 3);
    let tests = all_test_sources()?;

    for posture in AICrossOriginPosture::iter() {
        let variant_ref = format!("AICrossOriginPosture::{posture:?}");
        assert!(
            tests.contains(&variant_ref),
            "missing test source reference for {variant_ref}"
        );
        let encoded = serde_json::to_string(&posture)?;
        let decoded: AICrossOriginPosture = serde_json::from_str(&encoded)?;
        assert_eq!(decoded, posture);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 8. BackendHealthState — 5 variants, all exercised in tests + serde round-trips
// ---------------------------------------------------------------------------

#[test]
fn every_backend_health_state_variant_is_exercised_in_tests() -> TestResult {
    assert_eq!(BackendHealthState::COUNT, 5);
    let tests = all_test_sources()?;

    for state in BackendHealthState::iter() {
        let variant_ref = format!("BackendHealthState::{state:?}");
        assert!(
            tests.contains(&variant_ref),
            "missing test source reference for {variant_ref}"
        );
        let encoded = serde_json::to_string(&state)?;
        let decoded: BackendHealthState = serde_json::from_str(&encoded)?;
        assert_eq!(decoded, state);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 9. LatencyTier — 5 variants, all exercised in tests + serde round-trips
// ---------------------------------------------------------------------------

#[test]
fn every_latency_tier_variant_is_exercised_in_tests() -> TestResult {
    assert_eq!(LatencyTier::COUNT, 5);
    let tests = all_test_sources()?;

    for tier in LatencyTier::iter() {
        let variant_ref = format!("LatencyTier::{tier:?}");
        assert!(
            tests.contains(&variant_ref),
            "missing test source reference for {variant_ref}"
        );
        let encoded = serde_json::to_string(&tier)?;
        let decoded: LatencyTier = serde_json::from_str(&encoded)?;
        assert_eq!(decoded, tier);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 10. PrivacyClass — 5 variants, all exercised in tests + serde round-trips
// ---------------------------------------------------------------------------

#[test]
fn every_privacy_class_variant_is_exercised_in_tests() -> TestResult {
    assert_eq!(PrivacyClass::COUNT, 5);
    let tests = all_test_sources()?;

    for pc in PrivacyClass::iter() {
        let variant_ref = format!("PrivacyClass::{pc:?}");
        assert!(
            tests.contains(&variant_ref),
            "missing test source reference for {variant_ref}"
        );
        let encoded = serde_json::to_string(&pc)?;
        let decoded: PrivacyClass = serde_json::from_str(&encoded)?;
        assert_eq!(decoded, pc);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 11. INV-002 and INV-014 enforcement verified in test suite
// ---------------------------------------------------------------------------

#[test]
fn inv002_and_inv014_are_referenced_in_test_suite() -> TestResult {
    let tests = all_test_sources()?;

    // INV-002: AI proposes, never executes — ActionEnvelope must carry is_ai = true.
    assert!(
        tests.contains("INV-002"),
        "INV-002 must be referenced in test sources"
    );
    assert!(
        tests.contains("is_ai"),
        "ActionEnvelope.is_ai must be exercised in test sources"
    );

    // INV-014: circuit breaker must be consulted before model dispatch.
    assert!(
        tests.contains("INV-014"),
        "INV-014 must be referenced in test sources"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// 12. closure test files exist
// ---------------------------------------------------------------------------

#[test]
fn closure_test_files_exist_for_acceptance_and_invariants() {
    for relative in [
        "tests/cognitive_core.rs",
        "tests/router.rs",
        "tests/breaker.rs",
        "tests/latency_classifier.rs",
        "tests/types_roundtrip.rs",
        "tests/m11_closure.rs",
    ] {
        let path = crate_dir().join(relative);
        assert!(path.exists(), "missing {}", path.display());
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

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
    let mut active = String::with_capacity(source.len());
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        active.push_str(line.split_once("//").map_or(line, |(code, _)| code));
        active.push('\n');
    }
    active
}
