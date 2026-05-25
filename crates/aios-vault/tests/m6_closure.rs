//! T-055 — M6 closure invariants for `aios-vault`.

#![allow(
    clippy::expect_used,
    clippy::items_after_statements,
    clippy::panic,
    clippy::unwrap_used,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::fs;
use std::path::PathBuf;

use chrono::Utc;
use strum::{EnumCount, IntoEnumIterator as _};

use aios_vault::{
    CapabilityClass, CapabilityId, CapabilityState, KeyMaterialHandle, OverrideClass, SubjectRef,
    VaultCapability,
};

#[test]
fn crate_version_is_exactly_0_1_0_m6_closure_marker() {
    let cargo_toml = include_str!("../Cargo.toml");
    let version_line = cargo_toml
        .lines()
        .find(|line| line.trim_start().starts_with("version = "))
        .expect("Cargo.toml version line");

    assert_eq!(version_line.trim(), "version = \"0.1.0\"");
    assert!(
        !cargo_toml.contains("version = \"0.0.1\""),
        "legacy 0.0.1 version literal must not remain in aios-vault Cargo.toml"
    );
}

#[test]
fn default_code_version_reflects_t055() {
    use aios_vault::service::server::DEFAULT_CODE_VERSION;

    assert_eq!(DEFAULT_CODE_VERSION, "aios-vault/0.1.0-T055");
}

#[test]
fn no_status_unimplemented_remains_in_server_rs() {
    let server_rs = active_code(include_str!("../src/service/server.rs"));

    assert!(
        !server_rs.contains("Status::unimplemented(")
            && !server_rs.contains("Code::Unimplemented")
            && !server_rs.contains("Status::Unimplemented"),
        "gRPC server must not return Unimplemented after M6 closure"
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
    let proto = include_str!("../proto/aios.vault.v1alpha1.proto");
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
        rpc_names.len() >= 16,
        "expected at least 16 VaultBroker RPCs, got {rpc_names:?}"
    );
    for rpc in &rpc_names {
        let needle = format!("async fn {}(", to_snake_case(rpc));
        assert!(
            server_rs.contains(&needle),
            "server.rs missing method body for rpc {rpc} (expected {needle})"
        );
    }
    assert!(
        !server_rs.contains("Status::unimplemented(")
            && !server_rs.contains("Code::Unimplemented")
            && !server_rs.contains("unimplemented!("),
        "server.rs active code must not contain unimplemented returns"
    );
}

#[test]
fn every_capability_class_variant_has_a_test_path() {
    let exercised = [
        CapabilityClass::KeySign,
        CapabilityClass::KeyVerify,
        CapabilityClass::KeyEncrypt,
        CapabilityClass::KeyDecrypt,
        CapabilityClass::MacGenerate,
        CapabilityClass::MacVerify,
        CapabilityClass::RandomGenerate,
        CapabilityClass::SecretGet,
        CapabilityClass::BootstrapKeySign,
    ];

    assert_eq!(CapabilityClass::iter().count(), CapabilityClass::COUNT);
    for variant in CapabilityClass::iter() {
        assert!(
            exercised.contains(&variant),
            "missing CapabilityClass::{variant:?} test path"
        );
    }
}

#[test]
fn every_override_class_variant_has_a_test_path() {
    let exercised = [
        (OverrideClass::StrongSolo, 1_u32),
        (OverrideClass::DualHuman, 2_u32),
        (OverrideClass::TripleHuman, 3_u32),
    ];

    assert_eq!(OverrideClass::iter().count(), OverrideClass::COUNT);
    for variant in OverrideClass::iter() {
        assert!(
            exercised.iter().any(|(class, _count)| *class == variant),
            "missing OverrideClass::{variant:?} test path"
        );
    }
}

#[test]
fn every_capability_state_variant_is_reachable() {
    assert_eq!(CapabilityState::iter().count(), CapabilityState::COUNT);

    for state in CapabilityState::iter() {
        let capability = VaultCapability {
            capability_id: CapabilityId::new(),
            class: CapabilityClass::KeyEncrypt,
            issued_to: SubjectRef("family:alice".to_owned()),
            issued_at: Utc::now(),
            expires_at: None,
            state,
            key_material_handle: KeyMaterialHandle("vault-internal:test".to_owned()),
        };
        let json = serde_json::to_string(&capability).expect("capability state serializes");
        let back: VaultCapability =
            serde_json::from_str(&json).expect("capability state deserializes");
        assert_eq!(back.state, state);
    }
}

#[test]
fn deferred_surfaces_are_documented_for_m7_plus_without_new_debt_inside_m6() {
    let documented = [
        "SECRET_GET remains OperationUnsupportedInT049 until recovery + distinct human co-signer reveal mechanics land.",
        "KDF_DERIVE remains routed through KEY_ENCRYPT + HKDF because T-046 closed CapabilityClass without a dedicated KDF class.",
        "Vault-backed manifest verification remains M7+ runtime verifier work because L3 has no vault verifier hook today.",
    ];

    assert!(documented.iter().any(|line| {
        line.contains("OperationUnsupportedInT049") && line.contains("SECRET_GET")
    }));
    assert!(documented
        .iter()
        .any(|line| line.contains("KDF_DERIVE") && line.contains("KEY_ENCRYPT")));
    assert!(documented
        .iter()
        .any(|line| line.contains("manifest verification") && line.contains("M7+")));
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
