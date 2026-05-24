//! T-045 M5 closure invariants for `aios-fs`.

#![allow(
    clippy::expect_used,
    clippy::items_after_statements,
    clippy::panic,
    clippy::unwrap_used,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::fs;
use std::path::PathBuf;

use strum::{EnumCount, IntoEnumIterator as _};

use aios_fs::{NamespaceClass, VersionState};

#[test]
fn crate_version_is_0_1_0_m5_closure_marker() {
    let cargo_toml = include_str!("../Cargo.toml");
    assert!(
        cargo_toml.contains("version = \"0.1.0\""),
        "aios-fs Cargo.toml must declare version = \"0.1.0\"; got:\n{cargo_toml}"
    );
}

#[test]
fn default_code_version_reflects_t045() {
    use aios_fs::service::server::DEFAULT_CODE_VERSION;

    assert!(
        DEFAULT_CODE_VERSION.contains("0.1.0"),
        "DEFAULT_CODE_VERSION must reference 0.1.0; got {DEFAULT_CODE_VERSION}"
    );
    assert!(
        DEFAULT_CODE_VERSION.contains("T045"),
        "DEFAULT_CODE_VERSION must reference T045; got {DEFAULT_CODE_VERSION}"
    );
}

#[test]
fn no_status_unimplemented_remains_in_server_rs() {
    let server_rs = active_code(include_str!("../src/service/server.rs"));

    assert!(
        !server_rs.contains("Status::unimplemented(")
            && !server_rs.contains("Code::Unimplemented")
            && !server_rs.contains("Status::Unimplemented"),
        "gRPC server must not return Unimplemented after M5 closure"
    );
}

#[test]
fn no_todo_or_unimplemented_macros_remain_in_src() {
    let src_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let offenders = walk_rust_files(&src_dir)
        .into_iter()
        .filter_map(|path| {
            let body = fs::read_to_string(&path).ok()?;
            let active = active_code(&body);
            if active.contains("todo!(") || active.contains("unimplemented!(") {
                Some(path.display().to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    assert!(
        offenders.is_empty(),
        "found `todo!()` / `unimplemented!()` in active src code: {offenders:?}"
    );
}

#[test]
fn every_proto_rpc_has_server_method_body_without_unimplemented_return() {
    let proto = include_str!("../proto/aios.fs.v1alpha1.proto");
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

    assert!(
        rpc_names.len() >= 8,
        "expected at least 8 AIOS-FS RPCs, got {rpc_names:?}"
    );
    for rpc in &rpc_names {
        let needle = format!("async fn {}(", to_snake_case(rpc));
        assert!(
            server_rs.contains(&needle),
            "server.rs missing method body for rpc {rpc} (expected {needle})"
        );
    }
    assert!(
        !server_rs.contains("unimplemented"),
        "server.rs active code must not contain unimplemented branches"
    );
}

#[test]
fn every_version_state_variant_is_referenced_by_tests() {
    let corpus = tests_corpus();
    let missing = VersionState::iter()
        .filter_map(|variant| {
            let name = format!("VersionState::{variant:?}");
            (!corpus.contains(&name)).then_some(name)
        })
        .collect::<Vec<_>>();

    assert!(
        missing.is_empty(),
        "VersionState variants without workspace test reference: {missing:?}"
    );
}

#[test]
fn every_namespace_class_variant_is_referenced_by_tests() {
    let corpus = tests_corpus();
    let missing = NamespaceClass::iter()
        .filter_map(|variant| {
            let name = format!("NamespaceClass::{variant:?}");
            (!corpus.contains(&name)).then_some(name)
        })
        .collect::<Vec<_>>();

    assert_eq!(NamespaceClass::iter().count(), NamespaceClass::COUNT);
    assert!(
        missing.is_empty(),
        "NamespaceClass variants without workspace test reference: {missing:?}"
    );
}

#[test]
fn cargo_toml_does_not_carry_legacy_0_0_1_version() {
    let cargo_toml = include_str!("../Cargo.toml");

    assert!(
        !cargo_toml.contains("version = \"0.0.1\""),
        "no legacy 0.0.1 version literal in aios-fs Cargo.toml after M5 closure"
    );
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

fn tests_corpus() -> String {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut corpus = String::new();
    for path in walk_rust_files(&crate_root.join("tests")) {
        if let Ok(body) = fs::read_to_string(path) {
            corpus.push_str(&body);
        }
    }
    corpus
}

fn walk_rust_files(root: &PathBuf) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_rust_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    out
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
