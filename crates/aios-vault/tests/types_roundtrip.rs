//! T-046 round-trip + redaction tests for the `aios-vault` skeleton.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use chrono::{TimeZone, Utc};
use strum::{EnumCount, IntoEnumIterator};

use aios_action::ActionId;
use aios_vault::{
    CapabilityClass, CapabilityId, CapabilityState, KeyAlgorithm, KeyMaterial, KeyMaterialHandle,
    OverrideBinding, OverrideBindingState, OverrideClass, Session, SessionState, Subject,
    SubjectRef, SubjectType, VaultCapability, VaultError,
};

#[test]
fn capability_class_has_spec_count() {
    assert_eq!(CapabilityClass::COUNT, 9);
    assert_eq!(CapabilityClass::iter().count(), 9);
    assert!(CapabilityClass::iter().any(|class| class == CapabilityClass::BootstrapKeySign));
}

#[test]
fn capability_state_has_spec_variants() {
    assert_eq!(CapabilityState::COUNT, 6);
    assert_eq!(CapabilityState::iter().count(), 6);
    assert!(CapabilityState::iter().any(|state| state == CapabilityState::Rotated));
    assert!(CapabilityState::iter().any(|state| state == CapabilityState::Discarded));
}

#[test]
fn override_class_has_spec_count() {
    assert_eq!(OverrideClass::COUNT, 3);
    assert_eq!(OverrideClass::iter().count(), 3);
    assert!(OverrideClass::iter().any(|class| class == OverrideClass::DualHuman));
    assert!(OverrideClass::iter().any(|class| class == OverrideClass::TripleHuman));
}

#[test]
fn vault_capability_round_trips_without_key_bytes() {
    let capability = sample_capability();

    let json = serde_json::to_string(&capability).expect("serialise VaultCapability");
    let back: VaultCapability = serde_json::from_str(&json).expect("deserialise VaultCapability");

    assert_eq!(capability, back);
    assert!(json.contains("\"key_material_handle\""));
    assert!(json.contains("\"class\":\"KEY_SIGN\""));
    assert!(!json.contains("bytes"));
    assert!(!json.contains("secret"));
    assert!(!json.contains("material_bytes"));
}

#[test]
fn subject_round_trips() {
    let subject = sample_subject();

    let json = serde_json::to_string(&subject).expect("serialise Subject");
    let back: Subject = serde_json::from_str(&json).expect("deserialise Subject");

    assert_eq!(subject, back);
    assert!(json.contains("\"subject_type\":\"HUMAN_USER\""));
}

#[test]
fn session_round_trips() {
    let session = sample_session();

    let json = serde_json::to_string(&session).expect("serialise Session");
    let back: Session = serde_json::from_str(&json).expect("deserialise Session");

    assert_eq!(session, back);
    assert!(json.contains("\"state\":\"ACTIVE\""));
}

#[test]
fn override_binding_round_trips() {
    let binding = sample_override_binding();

    let json = serde_json::to_string(&binding).expect("serialise OverrideBinding");
    let back: OverrideBinding = serde_json::from_str(&json).expect("deserialise OverrideBinding");

    assert_eq!(binding, back);
    assert!(json.contains("\"class\":\"DUAL_HUMAN\""));
}

#[test]
fn key_material_handle_display_redacts() {
    let handle = KeyMaterialHandle("vault://tenant/family/root".to_owned());

    assert_eq!(handle.to_string(), "<vault-handle>");
}

#[test]
fn key_material_debug_redacts() {
    let key_material = KeyMaterial {
        algorithm: KeyAlgorithm::Aes256Gcm,
        created_at: sample_time(),
        bytes: vec![1, 2, 3, 4, 5],
    };

    let rendered = format!("{key_material:?}");

    assert_eq!(rendered, "<key-material-redacted>");
    assert!(!rendered.contains('1'));
    assert!(!rendered.contains("bytes"));
}

#[test]
fn key_material_serialization_is_blocked() {
    let key_material = KeyMaterial {
        algorithm: KeyAlgorithm::HmacSha256,
        created_at: sample_time(),
        bytes: vec![9, 8, 7],
    };

    let error = serde_json::to_string(&key_material).expect_err("KeyMaterial serialization fails");

    assert!(error
        .to_string()
        .contains("key material serialization blocked"));
}

#[test]
fn vault_error_display_strings_are_canonical() {
    let capability_id =
        CapabilityId::parse("cap_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("capability id");

    let cases = [
        (
            VaultError::CapabilityNotFound(capability_id.clone()),
            "capability not found: cap_01HXY8K2JPQ7N3M4R5S6T7V8W9",
        ),
        (
            VaultError::CapabilityExpired(capability_id.clone()),
            "capability expired: cap_01HXY8K2JPQ7N3M4R5S6T7V8W9",
        ),
        (
            VaultError::CapabilityRevoked(capability_id),
            "capability revoked: cap_01HXY8K2JPQ7N3M4R5S6T7V8W9",
        ),
        (
            VaultError::SubjectNotFound("family:alice".to_owned()),
            "subject not found: family:alice",
        ),
        (
            VaultError::SessionExpired("sess_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned()),
            "session expired: sess_01HXY8K2JPQ7N3M4R5S6T7V8W9",
        ),
        (
            VaultError::OverrideBindingNotFound("ovr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned()),
            "override binding not found: ovr_01HXY8K2JPQ7N3M4R5S6T7V8W9",
        ),
        (
            VaultError::OverrideAlreadyConsumed,
            "override binding already consumed",
        ),
        (
            VaultError::InvalidTransition {
                from: CapabilityState::Active,
                to: CapabilityState::Draft,
            },
            "invalid capability transition: Active -> Draft",
        ),
        (
            VaultError::Internal("sealed store unavailable".to_owned()),
            "vault internal error: sealed store unavailable",
        ),
        (
            VaultError::KeyMaterialLeak,
            "key material serialization blocked",
        ),
    ];

    for (error, expected) in cases {
        assert_eq!(error.to_string(), expected);
    }
}

fn sample_time() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 24, 12, 0, 0)
        .single()
        .expect("fixture timestamp is valid")
}

fn sample_capability() -> VaultCapability {
    VaultCapability {
        capability_id: CapabilityId::parse("cap_01HXY8K2JPQ7N3M4R5S6T7V8W9")
            .expect("capability id"),
        class: CapabilityClass::KeySign,
        issued_to: SubjectRef("family:alice".to_owned()),
        issued_at: sample_time(),
        expires_at: Some(sample_time()),
        state: CapabilityState::Active,
        key_material_handle: KeyMaterialHandle("vault-internal:slot-7".to_owned()),
    }
}

fn sample_subject() -> Subject {
    Subject {
        canonical_subject_id: "family:alice".to_owned(),
        subject_type: SubjectType::Human,
        provisional_name: "Alice".to_owned(),
        groups: vec!["family".to_owned()],
        is_ai: false,
        created_at: sample_time(),
    }
}

fn sample_session() -> Session {
    Session {
        session_id: "sess_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
        subject_id: "family:alice".to_owned(),
        started_at: sample_time(),
        expires_at: sample_time(),
        state: SessionState::Active,
    }
}

fn sample_override_binding() -> OverrideBinding {
    OverrideBinding {
        binding_id: "ovr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
        class: OverrideClass::DualHuman,
        granted_by: vec![
            SubjectRef("family:alice".to_owned()),
            SubjectRef("family:bob".to_owned()),
        ],
        granted_at: sample_time(),
        expires_at: sample_time(),
        target_action_id: Some(ActionId::parse("act_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("id")),
        state: OverrideBindingState::Granted,
    }
}
