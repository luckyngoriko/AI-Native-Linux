#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::unwrap_in_result,
    clippy::doc_markdown,
    clippy::significant_drop_tightening,
    clippy::wildcard_imports,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    missing_docs,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use aios_hardware::*;
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

struct KeyFixture {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
    fingerprint: String,
}

impl KeyFixture {
    fn generate() -> Self {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        let fingerprint = format!(
            "auth-{:02x}{:02x}",
            verifying_key.as_bytes()[0],
            verifying_key.as_bytes()[1]
        );
        Self {
            signing_key,
            verifying_key,
            fingerprint,
        }
    }
}

fn make_signed_binding(
    fixture: &KeyFixture,
    binding_id: &str,
    device_id: &str,
    module_name: &str,
    version: &str,
    provenance: DriverProvenance,
) -> DriverBinding {
    let mut binding = DriverBinding {
        binding_id: DriverBindingId(binding_id.into()),
        device_id: DeviceId(device_id.into()),
        driver_module_name: module_name.into(),
        kernel_module_version: version.into(),
        provenance,
        blake3_hash: blake3::hash(b"test-blob").to_hex().to_string(),
        signer_fingerprint: fixture.fingerprint.clone(),
        signature: Vec::new(),
        admitted_at: Utc::now(),
    };
    let canonical = binding.canonical_bytes();
    binding.signature = fixture.signing_key.sign(&canonical).to_bytes().to_vec();
    binding
}

fn make_registry_with_authority(fixture: &KeyFixture) -> DriverBindingRegistry {
    let mut registry = DriverBindingRegistry::new();
    registry.register_authority(&fixture.fingerprint, fixture.verifying_key);
    registry
}

fn sample_device_record(device_id: &str) -> HardwareDeviceRecord {
    HardwareDeviceRecord {
        device_id: DeviceId(device_id.into()),
        class: DeviceClass::GpuDiscrete,
        bus: BusKind::Pci,
        vendor_id: 0x10de,
        product_id: 0x2684,
        vendor_name: "NVIDIA".into(),
        product_name: "RTX 4090".into(),
        trust_class: DeviceTrustClass::Untrusted,
        lifecycle: DeviceLifecycleState::Detected,
        driver_provenance: None,
        firmware_version: None,
        removable: false,
        iommu_protected: true,
        probed_at: Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// admit_binding tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admit_binding_with_valid_signature_succeeds() {
    let fixture = KeyFixture::generate();
    let registry = make_registry_with_authority(&fixture);
    let binding = make_signed_binding(
        &fixture,
        "bind-001",
        "pci:10de:2684",
        "nvidia",
        "550.54.14",
        DriverProvenance::SignedKernelModule,
    );

    let result = registry.admit_binding(binding).await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");
}

#[tokio::test]
async fn admit_binding_with_invalid_signature_returns_driver_binding_failed() {
    let fixture = KeyFixture::generate();
    let registry = make_registry_with_authority(&fixture);
    let mut binding = make_signed_binding(
        &fixture,
        "bind-002",
        "pci:8086:a780",
        "i915",
        "1.0",
        DriverProvenance::SignedKernelModule,
    );
    // Corrupt the signature
    if !binding.signature.is_empty() {
        binding.signature[0] = binding.signature[0].wrapping_add(1);
    }

    let result = registry.admit_binding(binding).await;
    assert!(result.is_err(), "expected Err, got Ok");
    let err = result.unwrap_err();
    assert_eq!(err.code(), HardwareErrorCode::DriverBindingFailed);
}

#[tokio::test]
async fn admit_binding_with_unknown_authority_returns_driver_binding_failed() {
    let fixture = KeyFixture::generate();
    let registry = DriverBindingRegistry::new(); // no authority registered
    let binding = make_signed_binding(
        &fixture,
        "bind-003",
        "pci:8086:a780",
        "i915",
        "1.0",
        DriverProvenance::SignedKernelModule,
    );

    let result = registry.admit_binding(binding).await;
    assert!(result.is_err(), "expected Err, got Ok");
    let err = result.unwrap_err();
    assert_eq!(err.code(), HardwareErrorCode::DriverBindingFailed);
}

#[tokio::test]
async fn admit_binding_with_blacklisted_provenance_returns_driver_binding_failed() {
    let fixture = KeyFixture::generate();
    let registry = make_registry_with_authority(&fixture);
    let binding = make_signed_binding(
        &fixture,
        "bind-004",
        "pci:10de:2684",
        "badmodule",
        "1.0",
        DriverProvenance::OutOfTreeBlacklisted,
    );

    let result = registry.admit_binding(binding).await;
    assert!(result.is_err(), "expected Err, got Ok");
}

#[tokio::test]
async fn admit_binding_for_module_on_blacklist_returns_driver_binding_failed() {
    let fixture = KeyFixture::generate();
    let registry = make_registry_with_authority(&fixture);
    registry
        .add_to_blacklist("blocked-driver", "known vulnerable")
        .await
        .unwrap();

    let binding = make_signed_binding(
        &fixture,
        "bind-005",
        "pci:10de:2684",
        "blocked-driver",
        "1.0",
        DriverProvenance::DistroProvided,
    );

    let result = registry.admit_binding(binding).await;
    assert!(result.is_err(), "expected Err, got Ok");
    let err = result.unwrap_err();
    assert_eq!(err.code(), HardwareErrorCode::DriverBindingFailed);
}

#[tokio::test]
async fn admit_lower_priority_over_higher_returns_driver_binding_failed() {
    let fixture = KeyFixture::generate();
    let registry = make_registry_with_authority(&fixture);

    // Admit a high-priority (AiosVerified) binding first.
    let high = make_signed_binding(
        &fixture,
        "bind-high",
        "pci:10de:2684",
        "nvidia",
        "550.54.14",
        DriverProvenance::AiosVerified,
    );
    registry.admit_binding(high).await.unwrap();

    // Try to admit a lower-priority (DistroProvided) binding for the same device.
    let low = make_signed_binding(
        &fixture,
        "bind-low",
        "pci:10de:2684",
        "nvidia-dkms",
        "545.29.06",
        DriverProvenance::DistroProvided,
    );

    let result = registry.admit_binding(low).await;
    assert!(result.is_err(), "expected Err, got Ok");
    let err = result.unwrap_err();
    assert_eq!(err.code(), HardwareErrorCode::DriverBindingFailed);
}

#[tokio::test]
async fn admit_higher_priority_supersedes_lower_succeeds() {
    let fixture = KeyFixture::generate();
    let registry = make_registry_with_authority(&fixture);

    // Admit a low-priority (OperatorLocalSigned) binding first.
    let low = make_signed_binding(
        &fixture,
        "bind-low",
        "pci:10de:2684",
        "nvidia-open",
        "550.54.14",
        DriverProvenance::OperatorLocalSigned,
    );
    registry.admit_binding(low).await.unwrap();

    // Admit a higher-priority (SignedKernelModule) binding for the same device.
    let high = make_signed_binding(
        &fixture,
        "bind-high",
        "pci:10de:2684",
        "nvidia",
        "550.54.14",
        DriverProvenance::SignedKernelModule,
    );
    let result = registry.admit_binding(high).await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    // The device should now resolve to the high-priority binding.
    let looked_up = registry
        .lookup_binding(&DeviceId("pci:10de:2684".into()))
        .await;
    assert!(looked_up.is_some());
    assert_eq!(
        looked_up.unwrap().provenance,
        DriverProvenance::SignedKernelModule
    );
}

// ---------------------------------------------------------------------------
// lookup / list tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lookup_binding_known_device_returns_some() {
    let fixture = KeyFixture::generate();
    let registry = make_registry_with_authority(&fixture);
    let binding = make_signed_binding(
        &fixture,
        "bind-010",
        "pci:10de:2684",
        "nvidia",
        "550.54.14",
        DriverProvenance::AiosVerified,
    );
    registry.admit_binding(binding).await.unwrap();

    let result = registry
        .lookup_binding(&DeviceId("pci:10de:2684".into()))
        .await;
    assert!(result.is_some());
    assert_eq!(
        result.unwrap().binding_id,
        DriverBindingId("bind-010".into())
    );
}

#[tokio::test]
async fn lookup_binding_unknown_device_returns_none() {
    let registry = DriverBindingRegistry::new();
    let result = registry
        .lookup_binding(&DeviceId("pci:ffff:ffff".into()))
        .await;
    assert!(result.is_none());
}

#[tokio::test]
async fn list_bindings_after_3_admissions_returns_3() {
    let fixture = KeyFixture::generate();
    let registry = make_registry_with_authority(&fixture);

    for i in 0..3 {
        let binding = make_signed_binding(
            &fixture,
            &format!("bind-{i:03}"),
            &format!("pci:10de:{}", 1000 + i),
            "nvidia",
            "550.54.14",
            DriverProvenance::SignedKernelModule,
        );
        registry.admit_binding(binding).await.unwrap();
    }

    let all = registry.list_bindings().await;
    assert_eq!(all.len(), 3);
}

// ---------------------------------------------------------------------------
// revoke tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn revoke_binding_known_id_succeeds_then_lookup_returns_none() {
    let fixture = KeyFixture::generate();
    let registry = make_registry_with_authority(&fixture);
    let binding = make_signed_binding(
        &fixture,
        "bind-revoke",
        "pci:10de:2684",
        "nvidia",
        "550.54.14",
        DriverProvenance::SignedKernelModule,
    );
    registry.admit_binding(binding).await.unwrap();

    registry
        .revoke_binding(&DriverBindingId("bind-revoke".into()), "test revocation")
        .await
        .unwrap();

    let result = registry
        .lookup_binding(&DeviceId("pci:10de:2684".into()))
        .await;
    assert!(result.is_none());
}

#[tokio::test]
async fn revoke_binding_unknown_id_returns_internal_error() {
    let registry = DriverBindingRegistry::new();
    let result = registry
        .revoke_binding(&DriverBindingId("nonexistent".into()), "test")
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), HardwareErrorCode::Internal);
}

// ---------------------------------------------------------------------------
// blacklist tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn add_to_blacklist_then_admit_module_rejects() {
    let fixture = KeyFixture::generate();
    let registry = make_registry_with_authority(&fixture);
    registry
        .add_to_blacklist("evil-module", "backdoor detected")
        .await
        .unwrap();

    let binding = make_signed_binding(
        &fixture,
        "bind-bl",
        "pci:10de:2684",
        "evil-module",
        "1.0",
        DriverProvenance::SignedKernelModule,
    );

    let result = registry.admit_binding(binding).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn is_blacklisted_returns_true_after_add() {
    let registry = DriverBindingRegistry::new();
    assert!(!registry.is_blacklisted("evil-module").await);

    registry
        .add_to_blacklist("evil-module", "backdoor")
        .await
        .unwrap();

    assert!(registry.is_blacklisted("evil-module").await);
}

// ---------------------------------------------------------------------------
// priority_of tests
// ---------------------------------------------------------------------------

#[test]
fn priority_of_aios_verified_is_zero() {
    let registry = DriverBindingRegistry::new();
    assert_eq!(registry.priority_of(DriverProvenance::AiosVerified), 0);
}

#[test]
fn priority_of_out_of_tree_blacklisted_is_usize_max() {
    let registry = DriverBindingRegistry::new();
    assert_eq!(
        registry.priority_of(DriverProvenance::OutOfTreeBlacklisted),
        usize::MAX
    );
}

// ---------------------------------------------------------------------------
// upgrade_record_trust tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn upgrade_record_trust_with_aios_verified_sets_root_signed() {
    let fixture = KeyFixture::generate();
    let registry = make_registry_with_authority(&fixture);
    let binding = make_signed_binding(
        &fixture,
        "bind-upg-1",
        "pci:10de:2684",
        "nvidia",
        "550.54.14",
        DriverProvenance::AiosVerified,
    );
    registry.admit_binding(binding).await.unwrap();

    let mut record = sample_device_record("pci:10de:2684");
    registry.upgrade_record_trust(&mut record).await.unwrap();

    assert_eq!(
        record.driver_provenance,
        Some(DriverProvenance::AiosVerified)
    );
    assert_eq!(record.trust_class, DeviceTrustClass::RootSigned);
}

#[tokio::test]
async fn upgrade_record_trust_with_signed_kernel_module_sets_vendor_signed() {
    let fixture = KeyFixture::generate();
    let registry = make_registry_with_authority(&fixture);
    let binding = make_signed_binding(
        &fixture,
        "bind-upg-2",
        "pci:8086:a780",
        "i915",
        "1.0",
        DriverProvenance::SignedKernelModule,
    );
    registry.admit_binding(binding).await.unwrap();

    let mut record = sample_device_record("pci:8086:a780");
    registry.upgrade_record_trust(&mut record).await.unwrap();

    assert_eq!(
        record.driver_provenance,
        Some(DriverProvenance::SignedKernelModule)
    );
    assert_eq!(record.trust_class, DeviceTrustClass::VendorSigned);
}

#[tokio::test]
async fn upgrade_record_trust_with_operator_local_signed_sets_operator_local() {
    let fixture = KeyFixture::generate();
    let registry = make_registry_with_authority(&fixture);
    let binding = make_signed_binding(
        &fixture,
        "bind-upg-3",
        "usb:046d:c548",
        "logitech-hid",
        "2.0",
        DriverProvenance::OperatorLocalSigned,
    );
    registry.admit_binding(binding).await.unwrap();

    let mut record = sample_device_record("usb:046d:c548");
    registry.upgrade_record_trust(&mut record).await.unwrap();

    assert_eq!(
        record.driver_provenance,
        Some(DriverProvenance::OperatorLocalSigned)
    );
    assert_eq!(record.trust_class, DeviceTrustClass::OperatorLocal);
}

#[tokio::test]
async fn upgrade_record_trust_with_no_binding_leaves_untrusted() {
    let registry = DriverBindingRegistry::new();
    let mut record = sample_device_record("pci:dead:beef");
    // Pre-set to something else to confirm it gets overwritten.
    record.trust_class = DeviceTrustClass::VendorSigned;
    record.driver_provenance = Some(DriverProvenance::SignedKernelModule);

    registry.upgrade_record_trust(&mut record).await.unwrap();

    assert_eq!(record.driver_provenance, None);
    assert_eq!(record.trust_class, DeviceTrustClass::Untrusted);
}

// ---------------------------------------------------------------------------
// concurrency test
// ---------------------------------------------------------------------------

#[tokio::test]
async fn concurrent_admit_3_distinct_devices_no_panic() {
    let fixture = KeyFixture::generate();
    let registry = Arc::new(make_registry_with_authority(&fixture));

    let devices = ["pci:10de:1000", "pci:10de:1001", "pci:10de:1002"];

    let mut handles = Vec::new();
    for (i, dev) in devices.iter().enumerate() {
        let reg = Arc::clone(&registry);
        let fix_fp = fixture.fingerprint.clone();
        let fix_sk = fixture.signing_key.clone();
        let dev = dev.to_string();
        let handle = tokio::spawn(async move {
            let b = make_signed_binding_with_keys(
                &fix_fp,
                &fix_sk,
                &format!("bind-conc-{i}"),
                &dev,
                "nvidia",
                "550.54.14",
                DriverProvenance::SignedKernelModule,
            );
            reg.admit_binding(b).await
        });
        handles.push(handle);
    }

    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "concurrent admit failed: {result:?}");
    }

    let all = registry.list_bindings().await;
    assert_eq!(all.len(), 3);
}

fn make_signed_binding_with_keys(
    fingerprint: &str,
    signing_key: &SigningKey,
    binding_id: &str,
    device_id: &str,
    module_name: &str,
    version: &str,
    provenance: DriverProvenance,
) -> DriverBinding {
    let mut binding = DriverBinding {
        binding_id: DriverBindingId(binding_id.into()),
        device_id: DeviceId(device_id.into()),
        driver_module_name: module_name.into(),
        kernel_module_version: version.into(),
        provenance,
        blake3_hash: blake3::hash(b"test-blob").to_hex().to_string(),
        signer_fingerprint: fingerprint.into(),
        signature: Vec::new(),
        admitted_at: Utc::now(),
    };
    let canonical = binding.canonical_bytes();
    binding.signature = signing_key.sign(&canonical).to_bytes().to_vec();
    binding
}
