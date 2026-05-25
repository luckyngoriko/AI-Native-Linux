//! T-093 M10 closure invariants for `aios-sgr`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "closure tests fail loudly on milestone drift"
)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use strum::{EnumCount, IntoEnumIterator};

use aios_sgr::{DependencyKind, UnitKind, UnitState, DEFAULT_CODE_VERSION};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const UNIT_STATE_TEST_PATHS: [UnitState; 11] = [
    UnitState::Draft,
    UnitState::Queued,
    UnitState::Starting,
    UnitState::Running,
    UnitState::Healthy,
    UnitState::Degraded,
    UnitState::Unhealthy,
    UnitState::Stopping,
    UnitState::Stopped,
    UnitState::Failed,
    UnitState::Retired,
];

const UNIT_KIND_TEST_PATHS: [UnitKind; 10] = [
    UnitKind::Service,
    UnitKind::OneShotJob,
    UnitKind::Timer,
    UnitKind::Mount,
    UnitKind::Device,
    UnitKind::AppSession,
    UnitKind::AgentWorker,
    UnitKind::ModelServer,
    UnitKind::RecoveryTask,
    UnitKind::Observer,
];

const DEPENDENCY_KIND_TEST_PATHS: [DependencyKind; 3] = [
    DependencyKind::RequiresHealthy,
    DependencyKind::RequiresRunning,
    DependencyKind::OrdersAfter,
];

const DEFERRED_SURFACE_NOTES: [&str; 3] = [
    "T-088: UnitManifest has no typed requires field; SgrAdapterRegistry reads adapter_target.requires as a compatibility shim. Promote to typed UnitManifest.requires in an M-later S15.1 schema revision if the manifest is unfrozen.",
    "T-091: UnitManifest has no recovery_mode_allowed field; SgrRecoveryHook uses labels.recovery_mode_allowed == true or UnitKind::RecoveryTask. Promote to a typed manifest field in an M-later S15.1 schema revision if needed.",
    "T-090: aios-evidence has no DEPENDENCY_DECLARED RecordType; SGR dependency declarations fold to GRAPH_EVALUATED with typed payload. Promote only in a future evidence-vocabulary wave.",
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
fn default_code_version_reflects_t093_closure_marker() {
    assert_eq!(DEFAULT_CODE_VERSION, "0.1.0-T093");
}

#[test]
fn no_status_unimplemented_remains_in_server_rs() {
    let server_rs = active_code(include_str!("../src/service/server.rs"));

    for forbidden in [
        "Status::unimplemented(",
        "Status::Unimplemented",
        "Code::Unimplemented",
        "unimplemented!(",
    ] {
        assert!(
            !server_rs.contains(forbidden),
            "server.rs still contains {forbidden}"
        );
    }
}

#[test]
fn no_todo_or_unimplemented_macros_remain_in_src() -> TestResult {
    let todo_macro = ["todo", "!("].concat();
    let unimplemented_macro = ["unimplemented", "!("].concat();
    let offenders = rust_files(&crate_dir().join("src"))?
        .into_iter()
        .filter_map(|path| {
            let body = fs::read_to_string(&path).ok()?;
            let active = active_code(&body);
            (active.contains(&todo_macro) || active.contains(&unimplemented_macro))
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
    let proto = include_str!("../proto/aios.sgr.v1alpha1.proto");
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
        [
            "RegisterUnit",
            "GetUnit",
            "ListUnits",
            "DeclareDependency",
            "ListDependencies",
            "TraverseGraph",
            "GetGraphState",
            "EvaluateGraph",
            "StartUnit",
            "StopUnit",
            "RestartUnit",
            "MarkUnitFailed",
            "RegisterAdapter",
            "LookupAdapter",
            "ListAdapters",
            "FindAdapterForUnit",
        ]
    );
    for rpc in &rpc_names {
        let method = format!("async fn {}(", to_snake_case(rpc));
        assert!(
            server_rs.contains(&method),
            "server.rs missing method body for rpc {rpc}"
        );
    }
    assert!(!server_rs.contains("unimplemented"));
}

#[test]
fn every_unit_state_variant_is_exercised_in_workspace_tests() -> TestResult {
    assert_eq!(UnitState::COUNT, 11);
    assert_eq!(UnitState::iter().collect::<Vec<_>>(), UNIT_STATE_TEST_PATHS);
    let tests = all_test_sources()?;

    for state in UnitState::iter() {
        let variant_ref = format!("UnitState::{state:?}");
        assert!(
            tests.contains(&variant_ref),
            "missing test source reference for {variant_ref}"
        );
        assert!(!state.as_wire_str().is_empty());
        let encoded = serde_json::to_string(&state)?;
        let decoded: UnitState = serde_json::from_str(&encoded)?;
        assert_eq!(decoded, state);
    }
    Ok(())
}

#[test]
fn every_unit_kind_variant_is_exercised_in_workspace_tests() -> TestResult {
    assert_eq!(UnitKind::COUNT, 10);
    assert_eq!(UnitKind::iter().collect::<Vec<_>>(), UNIT_KIND_TEST_PATHS);
    let tests = all_test_sources()?;

    for kind in UnitKind::iter() {
        let variant_ref = format!("UnitKind::{kind:?}");
        assert!(
            tests.contains(&variant_ref),
            "missing test source reference for {variant_ref}"
        );
        let encoded = serde_json::to_string(&kind)?;
        let decoded: UnitKind = serde_json::from_str(&encoded)?;
        assert_eq!(decoded, kind);
    }
    Ok(())
}

#[test]
fn every_dependency_kind_variant_is_exercised_in_workspace_tests() -> TestResult {
    assert_eq!(DependencyKind::COUNT, 3);
    assert_eq!(
        DependencyKind::iter().collect::<Vec<_>>(),
        DEPENDENCY_KIND_TEST_PATHS
    );
    let tests = all_test_sources()?;

    for kind in DependencyKind::iter() {
        let variant_ref = format!("DependencyKind::{kind:?}");
        assert!(
            tests.contains(&variant_ref),
            "missing test source reference for {variant_ref}"
        );
        let encoded = serde_json::to_string(&kind)?;
        let decoded: DependencyKind = serde_json::from_str(&encoded)?;
        assert_eq!(decoded, kind);
    }
    Ok(())
}

#[test]
fn deferred_surfaces_are_documented_without_m10_debt() {
    assert_eq!(DEFERRED_SURFACE_NOTES.len(), 3);
    assert!(DEFERRED_SURFACE_NOTES
        .iter()
        .any(|note| note.contains("adapter_target.requires") && note.contains("M-later")));
    assert!(DEFERRED_SURFACE_NOTES
        .iter()
        .any(|note| note.contains("recovery_mode_allowed")
            && note.contains("UnitKind::RecoveryTask")));
    assert!(DEFERRED_SURFACE_NOTES
        .iter()
        .any(|note| note.contains("DEPENDENCY_DECLARED") && note.contains("GRAPH_EVALUATED")));
}

#[test]
fn closure_test_files_exist_for_acceptance_and_invariants() {
    for relative in [
        "tests/mvp_service_graph.rs",
        "tests/acceptance_fixtures.rs",
        "tests/m10_closure.rs",
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
