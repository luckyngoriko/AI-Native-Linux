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

use aios_hardware::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn dev(id: &str) -> DeviceId {
    DeviceId(id.into())
}

// ===========================================================================
// removable_policy tests
// ===========================================================================

#[tokio::test]
async fn default_policy_for_unset_device_is_deny_default() {
    let table = RemovableDevicePolicyTable::new();
    let policy = table.get_policy(&dev("usb:0000")).await;
    assert_eq!(policy, RemovableDevicePolicy::DenyDefault);
}

#[tokio::test]
async fn set_policy_by_human_setter_succeeds() {
    let table = RemovableDevicePolicyTable::new();
    table
        .set_policy(
            dev("usb:0001"),
            RemovableDevicePolicy::AllowReadOnly,
            "operator:alice",
        )
        .await
        .expect("human setter");
    assert_eq!(
        table.get_policy(&dev("usb:0001")).await,
        RemovableDevicePolicy::AllowReadOnly
    );
}

#[tokio::test]
async fn set_policy_by_ai_setter_returns_internal_error() {
    let table = RemovableDevicePolicyTable::new();
    let err = table
        .set_policy(
            dev("usb:0002"),
            RemovableDevicePolicy::AllowMount,
            "agent:gpt-7",
        )
        .await
        .expect_err("ai setter");
    assert_eq!(err.code(), HardwareErrorCode::Internal);
}

#[tokio::test]
async fn recovery_mode_forces_recovery_denied_on_get() {
    let table = RemovableDevicePolicyTable::with_recovery_mode(true);
    // Even though we set AllowMount for a device, recovery mode forces RecoveryDenied
    table
        .set_policy(
            dev("usb:0003"),
            RemovableDevicePolicy::AllowMount,
            "operator:alice",
        )
        .await
        .expect("set");
    // But recovery mode coerces the stored value to RecoveryDenied in set_policy
    // AND get_policy always returns RecoveryDenied when recovery is active
    assert_eq!(
        table.get_policy(&dev("usb:0003")).await,
        RemovableDevicePolicy::RecoveryDenied
    );
}

#[tokio::test]
async fn recovery_mode_set_policy_coerces_to_recovery_denied() {
    let table = RemovableDevicePolicyTable::with_recovery_mode(true);
    // Setting AllowReadWrite during recovery mode — coerced to RecoveryDenied
    table
        .set_policy(
            dev("usb:0004"),
            RemovableDevicePolicy::AllowReadWrite,
            "operator:bob",
        )
        .await
        .expect("set");
    // get_policy always returns RecoveryDenied when recovery is active
    assert_eq!(
        table.get_policy(&dev("usb:0004")).await,
        RemovableDevicePolicy::RecoveryDenied
    );
}

#[tokio::test]
async fn check_mount_with_deny_default_returns_removable_denied() {
    let table = RemovableDevicePolicyTable::new();
    let err = table
        .check_mount(&dev("usb:0005"), "operator:alice")
        .await
        .expect_err("deny default");
    assert_eq!(err.code(), HardwareErrorCode::RemovableDenied);
}

#[tokio::test]
async fn check_mount_with_allow_read_only_for_human_returns_ok() {
    let table = RemovableDevicePolicyTable::new();
    table
        .set_policy(
            dev("usb:0006"),
            RemovableDevicePolicy::AllowReadOnly,
            "operator:alice",
        )
        .await
        .expect("set");
    table
        .check_mount(&dev("usb:0006"), "operator:alice")
        .await
        .expect("human read-only");
}

#[tokio::test]
async fn check_mount_with_allow_read_only_for_ai_returns_removable_denied() {
    // INV-013: AI subjects are always denied, even when policy is AllowReadOnly
    let table = RemovableDevicePolicyTable::new();
    table
        .set_policy(
            dev("usb:0007"),
            RemovableDevicePolicy::AllowReadOnly,
            "operator:alice",
        )
        .await
        .expect("set");
    let err = table
        .check_mount(&dev("usb:0007"), "agent:gpt-7")
        .await
        .expect_err("ai read-only");
    assert_eq!(err.code(), HardwareErrorCode::RemovableDenied);
}

#[tokio::test]
async fn check_mount_with_allow_read_write_for_human_returns_ok() {
    let table = RemovableDevicePolicyTable::new();
    table
        .set_policy(
            dev("usb:0008"),
            RemovableDevicePolicy::AllowReadWrite,
            "operator:bob",
        )
        .await
        .expect("set");
    table
        .check_mount(&dev("usb:0008"), "operator:bob")
        .await
        .expect("human read-write");
}

#[tokio::test]
async fn check_mount_with_recovery_denied_for_human_returns_removable_denied() {
    let table = RemovableDevicePolicyTable::with_recovery_mode(true);
    // Even for a human, recovery mode forces RecoveryDenied
    let err = table
        .check_mount(&dev("usb:0009"), "operator:alice")
        .await
        .expect_err("recovery denied");
    assert_eq!(err.code(), HardwareErrorCode::RemovableDenied);
}

#[tokio::test]
async fn list_policies_after_3_returns_3() {
    let table = RemovableDevicePolicyTable::new();
    table
        .set_policy(
            dev("usb:a"),
            RemovableDevicePolicy::AllowReadOnly,
            "operator:alice",
        )
        .await
        .expect("set a");
    table
        .set_policy(
            dev("usb:b"),
            RemovableDevicePolicy::AllowMount,
            "operator:alice",
        )
        .await
        .expect("set b");
    table
        .set_policy(
            dev("usb:c"),
            RemovableDevicePolicy::AllowReadWrite,
            "operator:alice",
        )
        .await
        .expect("set c");
    let list = table.list_policies().await;
    assert_eq!(list.len(), 3);
}

// ===========================================================================
// iommu tests
// ===========================================================================

#[tokio::test]
async fn iommu_required_for_thunderbolt_returns_true() {
    assert!(IommuFloorEnforcer::iommu_required_for_bus(
        BusKind::Thunderbolt
    ));
}

#[tokio::test]
async fn iommu_required_for_usb4_returns_true() {
    assert!(IommuFloorEnforcer::iommu_required_for_bus(BusKind::Usb4));
}

#[tokio::test]
async fn iommu_required_for_pcie_returns_true() {
    assert!(IommuFloorEnforcer::iommu_required_for_bus(BusKind::Pcie));
}

#[tokio::test]
async fn iommu_required_for_usb2_returns_false() {
    assert!(!IommuFloorEnforcer::iommu_required_for_bus(BusKind::Usb2));
}

#[tokio::test]
async fn record_observation_iommu_present_for_thunderbolt_succeeds() {
    let enforcer = IommuFloorEnforcer::new();
    let result = enforcer
        .record_observation(dev("thunderbolt:00"), BusKind::Thunderbolt, true)
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn record_observation_iommu_missing_for_thunderbolt_returns_iommu_missing() {
    let enforcer = IommuFloorEnforcer::new();
    let err = enforcer
        .record_observation(dev("thunderbolt:01"), BusKind::Thunderbolt, false)
        .await
        .expect_err("iommu missing");
    assert_eq!(err.code(), HardwareErrorCode::IommuMissing);
}

#[tokio::test]
async fn record_observation_iommu_missing_for_usb2_succeeds_because_not_required() {
    let enforcer = IommuFloorEnforcer::new();
    let result = enforcer
        .record_observation(dev("usb2:00"), BusKind::Usb2, false)
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn lookup_requirement_known_device_returns_some() {
    let enforcer = IommuFloorEnforcer::new();
    enforcer
        .record_observation(dev("thunderbolt:02"), BusKind::Thunderbolt, true)
        .await
        .expect("record");
    let req = enforcer.lookup_requirement(&dev("thunderbolt:02")).await;
    assert!(req.is_some());
    let req = req.unwrap();
    assert!(req.iommu_required);
    assert!(req.iommu_observed);
}

#[tokio::test]
async fn quarantine_candidates_after_2_unprotected_thunderbolt_returns_2() {
    let enforcer = IommuFloorEnforcer::new();
    // Record 2 thunderbolt devices without IOMMU — both become quarantine candidates
    let _ = enforcer
        .record_observation(dev("thunderbolt:03"), BusKind::Thunderbolt, false)
        .await;
    let _ = enforcer
        .record_observation(dev("thunderbolt:04"), BusKind::Thunderbolt, false)
        .await;
    // Also record a protected one — should NOT be a candidate
    enforcer
        .record_observation(dev("thunderbolt:05"), BusKind::Thunderbolt, true)
        .await
        .expect("ok");
    let candidates = enforcer.quarantine_candidates().await;
    assert_eq!(candidates.len(), 2);
}

// ===========================================================================
// evaluate_removable_admission tests
// ===========================================================================

#[tokio::test]
async fn admission_with_iommu_required_but_missing_returns_iommu_missing_before_removable_check() {
    let removable = RemovableDevicePolicyTable::new();
    let iommu = IommuFloorEnforcer::new();
    // Record IOMMU as missing for a Thunderbolt device
    let _ = iommu
        .record_observation(dev("thunderbolt:10"), BusKind::Thunderbolt, false)
        .await;
    // Even if removable policy would allow it, IOMMU check comes first
    removable
        .set_policy(
            dev("thunderbolt:10"),
            RemovableDevicePolicy::AllowReadWrite,
            "operator:alice",
        )
        .await
        .expect("set policy");
    let err = evaluate_removable_admission(
        &removable,
        &iommu,
        &dev("thunderbolt:10"),
        BusKind::Thunderbolt,
        "operator:alice",
    )
    .await
    .expect_err("iommu missing");
    assert_eq!(err.code(), HardwareErrorCode::IommuMissing);
}

#[tokio::test]
async fn admission_with_iommu_present_then_removable_allow_human_succeeds() {
    let removable = RemovableDevicePolicyTable::new();
    let iommu = IommuFloorEnforcer::new();
    iommu
        .record_observation(dev("thunderbolt:11"), BusKind::Thunderbolt, true)
        .await
        .expect("record iommu");
    removable
        .set_policy(
            dev("thunderbolt:11"),
            RemovableDevicePolicy::AllowReadWrite,
            "operator:alice",
        )
        .await
        .expect("set policy");
    let result = evaluate_removable_admission(
        &removable,
        &iommu,
        &dev("thunderbolt:11"),
        BusKind::Thunderbolt,
        "operator:alice",
    )
    .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn admission_with_iommu_present_then_removable_deny_returns_removable_denied() {
    let removable = RemovableDevicePolicyTable::new();
    let iommu = IommuFloorEnforcer::new();
    iommu
        .record_observation(dev("thunderbolt:12"), BusKind::Thunderbolt, true)
        .await
        .expect("record iommu");
    // Default policy is DenyDefault — should be denied
    let err = evaluate_removable_admission(
        &removable,
        &iommu,
        &dev("thunderbolt:12"),
        BusKind::Thunderbolt,
        "operator:alice",
    )
    .await
    .expect_err("removable denied");
    assert_eq!(err.code(), HardwareErrorCode::RemovableDenied);
}
