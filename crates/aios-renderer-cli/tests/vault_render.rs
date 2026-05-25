//! Tests for vault renderable implementations.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_action::ActionId;
use aios_renderer_cli::{OutputFormat, RenderContext, Renderable};
use aios_vault::{
    CapabilityClass, CapabilityId, CapabilityState, KeyMaterialHandle, OverrideBinding,
    OverrideBindingState, OverrideClass, SubjectRef, VaultCapability,
};
use chrono::{Duration, TimeZone, Utc};

const RAW_KEY_HEX: &str = "00112233445566778899aabbccddeeff";

fn ctx(color: bool, redact_secrets: bool) -> RenderContext {
    RenderContext {
        color,
        width: Some(220),
        redact_secrets,
        verbose: false,
        locale: "en_US.UTF-8".to_owned(),
    }
}

const fn formats() -> [OutputFormat; 4] {
    [
        OutputFormat::Text,
        OutputFormat::Json,
        OutputFormat::Tree,
        OutputFormat::Table,
    ]
}

fn capability() -> VaultCapability {
    VaultCapability {
        capability_id: CapabilityId::parse("cap_01HXY8K2JPQ7N3M4R5S6T7V8W9")
            .expect("valid capability id"),
        class: CapabilityClass::KeySign,
        issued_to: SubjectRef("human:operator".to_owned()),
        issued_at: Utc
            .with_ymd_and_hms(2026, 5, 25, 9, 5, 0)
            .single()
            .expect("valid timestamp"),
        expires_at: None,
        state: CapabilityState::Active,
        key_material_handle: KeyMaterialHandle(RAW_KEY_HEX.to_owned()),
    }
}

fn override_binding() -> OverrideBinding {
    let granted_at = Utc
        .with_ymd_and_hms(2026, 5, 25, 9, 10, 0)
        .single()
        .expect("valid timestamp");

    OverrideBinding {
        binding_id: "ovr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
        class: OverrideClass::DualHuman,
        granted_by: vec![
            SubjectRef("human:operator-a".to_owned()),
            SubjectRef("human:operator-b".to_owned()),
        ],
        granted_at,
        expires_at: granted_at + Duration::minutes(30),
        target_action_id: Some(
            ActionId::parse("act_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("valid action id"),
        ),
        state: OverrideBindingState::Granted,
    }
}

fn contains_32_hex_sequence(value: &str) -> bool {
    value
        .as_bytes()
        .windows(32)
        .any(|window| window.iter().all(u8::is_ascii_hexdigit))
}

#[test]
fn vault_capability_renders_in_all_formats_with_handle_marker() {
    let capability = capability();

    for format in formats() {
        let rendered = capability
            .render(format, &ctx(false, true))
            .expect("render vault capability");
        assert!(rendered.contains("cap_01HXY8K2JPQ7N3M4R5S6T7V8W9"));
        assert!(rendered.contains("KeySign") || rendered.contains("KEY_SIGN"));
        assert!(rendered.contains("human:operator"));
        assert!(rendered.contains("Active") || rendered.contains("ACTIVE"));
        assert!(rendered.contains("<vault-handle>"));
    }
}

#[test]
fn inv018_vault_capability_never_exposes_key_shaped_hex_even_unredacted() {
    let capability = capability();

    for format in formats() {
        let rendered = capability
            .render(format, &ctx(false, false))
            .expect("render vault capability");
        assert!(rendered.contains("<vault-handle>"), "{rendered}");
        assert!(!rendered.contains(RAW_KEY_HEX), "{rendered}");
        assert!(!contains_32_hex_sequence(&rendered), "{rendered}");
    }
}

#[test]
fn capability_class_and_state_render_directly() {
    for format in formats() {
        let class = CapabilityClass::SecretGet
            .render(format, &ctx(false, true))
            .expect("render capability class");
        let state = CapabilityState::Revoked
            .render(format, &ctx(false, true))
            .expect("render capability state");

        assert!(!class.is_empty());
        assert!(!state.is_empty());
    }
}

#[test]
fn override_class_renders_directly() {
    for format in formats() {
        let rendered = OverrideClass::TripleHuman
            .render(format, &ctx(false, true))
            .expect("render override class");

        assert!(!rendered.is_empty());
    }
}

#[test]
fn override_binding_renders_grant_subjects_and_target_action() {
    let binding = override_binding();

    for format in formats() {
        let rendered = binding
            .render(format, &ctx(false, true))
            .expect("render override binding");
        assert!(rendered.contains("ovr_01HXY8K2JPQ7N3M4R5S6T7V8W9"));
        assert!(rendered.contains("DualHuman") || rendered.contains("DUAL_HUMAN"));
        assert!(rendered.contains("human:operator-a"));
        assert!(rendered.contains("act_01HXY8K2JPQ7N3M4R5S6T7V8W9"));
    }
}

#[test]
fn key_material_handle_always_redacts_with_redact_secrets_false() {
    let handle = KeyMaterialHandle(RAW_KEY_HEX.to_owned());

    for format in formats() {
        let rendered = handle
            .render(format, &ctx(false, false))
            .expect("render key material handle");
        assert!(rendered.contains("<vault-handle>"), "{rendered}");
        assert!(!rendered.contains(RAW_KEY_HEX), "{rendered}");
        assert!(!contains_32_hex_sequence(&rendered), "{rendered}");
    }
}
