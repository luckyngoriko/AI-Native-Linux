#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
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

fn obs(bus: BusKind, vendor_id: u16, product_id: u16, class_hint: u32) -> RawDeviceObservation {
    RawDeviceObservation {
        bus,
        bus_address: "test-addr".into(),
        vendor_id,
        product_id,
        class_hint,
        vendor_name: None,
        product_name: None,
        removable_hint: false,
        iommu_protected_hint: false,
        firmware_version_hint: None,
    }
}

#[allow(clippy::missing_const_for_fn)]
fn ts() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(1_700_000_000, 0).expect("valid fixed unix timestamp")
}

// -- PCI: class 0x03 (display) ------------------------------------------------

#[test]
fn classify_pci_class_0x03_nvidia_returns_gpu_discrete() {
    let o = obs(BusKind::Pci, 0x10DE, 0x2484, 0x03_00_00);
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::GpuDiscrete
    );
}

#[test]
fn classify_pci_class_0x03_amd_discrete_returns_gpu_discrete() {
    let o = obs(BusKind::Pci, 0x1002, 0x73FF, 0x03_00_00); // RX 6000 series
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::GpuDiscrete
    );
}

#[test]
fn classify_pci_class_0x03_amd_apu_returns_gpu_integrated() {
    let o = obs(BusKind::Pci, 0x1002, 0x1638, 0x03_00_00); // Ryzen APU
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::GpuIntegrated
    );
}

#[test]
fn classify_pci_class_0x03_intel_returns_gpu_integrated() {
    let o = obs(BusKind::Pci, 0x8086, 0x9A49, 0x03_00_00);
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::GpuIntegrated
    );
}

// -- PCI: class 0x02 (network) ------------------------------------------------

#[test]
fn classify_pci_class_0x02_subclass_0x00_returns_network_ethernet() {
    let o = obs(BusKind::Pcie, 0x8086, 0x15F3, 0x02_00_00);
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::NetworkEthernet
    );
}

#[test]
fn classify_pci_class_0x02_subclass_0x80_returns_network_wifi() {
    let o = obs(BusKind::Pcie, 0x8086, 0x2725, 0x02_80_00);
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::NetworkWifi
    );
}

// -- PCI: class 0x01 (storage) ------------------------------------------------

#[test]
fn classify_pci_class_0x01_subclass_0x08_returns_storage_nvme() {
    let o = obs(BusKind::Pcie, 0x144D, 0xA80A, 0x01_08_02); // Samsung NVMe
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::StorageNvme
    );
}

#[test]
fn classify_pci_class_0x01_subclass_0x06_returns_storage_sata() {
    let o = obs(BusKind::Pci, 0x8086, 0xA102, 0x01_06_01); // Intel SATA AHCI
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::StorageSata
    );
}

#[test]
fn classify_pci_class_0x01_subclass_0x05_returns_storage_mmc() {
    let o = obs(BusKind::Pci, 0x8086, 0x0000, 0x01_05_00);
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::StorageMmc
    );
}

// -- PCI: class 0x04 (audio) --------------------------------------------------

#[test]
fn classify_pci_class_0x04_returns_audio_card() {
    let o = obs(BusKind::Pci, 0x8086, 0xA0C8, 0x04_03_00);
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::AudioCard
    );
}

// -- PCI: class 0x0c (serial bus) ---------------------------------------------

#[test]
fn classify_pci_class_0x0c_subclass_0x03_returns_usb_controller() {
    let o = obs(BusKind::Pci, 0x8086, 0xA0ED, 0x0C_03_30);
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::UsbController
    );
}

#[test]
fn classify_pci_class_0x0c_subclass_0x04_returns_thunderbolt_controller() {
    let o = obs(BusKind::Pci, 0x8086, 0x9A1B, 0x0C_04_00);
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::ThunderboltController
    );
}

// -- PCI: class 0x06 (memory) -------------------------------------------------

#[test]
fn classify_pci_class_0x06_intel_host_bridge_returns_memory() {
    let o = obs(BusKind::Pci, 0x8086, 0x9A14, 0x06_00_00);
    assert_eq!(DeviceClassifier::classify(&o).unwrap(), DeviceClass::Memory);
}

#[test]
fn classify_pci_class_0x06_non_host_bridge_vendor_returns_sensor_or_input() {
    let o = obs(BusKind::Pci, 0x1234, 0x5678, 0x06_00_00);
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::SensorOrInputDevice
    );
}

// -- USB ----------------------------------------------------------------------

#[test]
fn classify_usb_class_0x09_returns_usb_controller() {
    let o = obs(BusKind::Usb3, 0x8087, 0x0029, 0x00_09_00);
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::UsbController
    );
}

#[test]
fn classify_usb_class_0xe0_returns_network_bluetooth() {
    let o = obs(BusKind::Usb2, 0x8087, 0x0032, 0x00_E0_01);
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::NetworkBluetooth
    );
}

#[test]
fn classify_usb_class_0x01_returns_audio_card() {
    let o = obs(BusKind::Usb2, 0x1234, 0x5678, 0x00_01_00);
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::AudioCard
    );
}

#[test]
fn classify_usb_class_0x03_returns_sensor_or_input_device() {
    let o = obs(BusKind::Usb3, 0x046D, 0xC077, 0x00_03_00); // Logitech mouse
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::SensorOrInputDevice
    );
}

#[test]
fn classify_usb_class_0x07_returns_printer_or_scanner() {
    let o = obs(BusKind::Usb2, 0x04A9, 0x1909, 0x00_07_01); // Canon printer
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::PrinterOrScanner
    );
}

// -- Other buses --------------------------------------------------------------

#[test]
fn classify_thunderbolt_returns_thunderbolt_controller() {
    let o = obs(BusKind::Thunderbolt, 0x8086, 0x15EB, 0x00_00_00);
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::ThunderboltController
    );
}

#[test]
fn classify_nvme_returns_storage_nvme() {
    let o = obs(BusKind::Nvme, 0x144D, 0xA80A, 0x00_00_00);
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::StorageNvme
    );
}

#[test]
fn classify_i2c_returns_sensor_or_input_device() {
    let o = obs(BusKind::I2c, 0x0000, 0x0000, 0x00_00_00);
    assert_eq!(
        DeviceClassifier::classify(&o).unwrap(),
        DeviceClass::SensorOrInputDevice
    );
}

#[test]
fn classify_unknown_returns_classification_failed() {
    let o = obs(BusKind::Pci, 0x9999, 0x0001, 0xFF_00_00);
    let err = DeviceClassifier::classify(&o).unwrap_err();
    assert_eq!(err.code(), HardwareErrorCode::ClassificationFailed);
}

// -- classify_with_trust ------------------------------------------------------

#[test]
fn classify_with_trust_default_trust_class_is_untrusted() {
    let o = obs(BusKind::Pcie, 0x8086, 0x9A49, 0x03_00_00);
    let record = DeviceClassifier::classify_with_trust(&o, ts()).unwrap();
    assert_eq!(record.trust_class, DeviceTrustClass::Untrusted);
}

#[test]
fn classify_with_trust_lifecycle_is_detected() {
    let o = obs(BusKind::Nvme, 0x144D, 0xA80A, 0x00_00_00);
    let record = DeviceClassifier::classify_with_trust(&o, ts()).unwrap();
    assert_eq!(record.lifecycle, DeviceLifecycleState::Detected);
}

#[test]
fn classify_with_trust_device_id_format_round_trip() {
    let o = RawDeviceObservation {
        bus: BusKind::Pci,
        bus_address: "0000:00:02.0".into(),
        vendor_id: 0x8086,
        product_id: 0x9A49,
        class_hint: 0x03_00_00,
        vendor_name: Some("Intel".into()),
        product_name: Some("Iris Xe".into()),
        removable_hint: false,
        iommu_protected_hint: true,
        firmware_version_hint: None,
    };
    let record = DeviceClassifier::classify_with_trust(&o, ts()).unwrap();
    assert_eq!(
        record.device_id,
        DeviceId("pci:8086:9a49@0000:00:02.0".into())
    );
    assert_eq!(record.vendor_name, "Intel");
    assert_eq!(record.product_name, "Iris Xe");
    assert!(record.iommu_protected);
}

#[test]
fn classify_with_trust_preserves_firmware_version_hint() {
    let o = RawDeviceObservation {
        bus: BusKind::Pcie,
        bus_address: "0000:01:00.0".into(),
        vendor_id: 0x10DE,
        product_id: 0x2484,
        class_hint: 0x03_00_00,
        vendor_name: None,
        product_name: None,
        removable_hint: false,
        iommu_protected_hint: false,
        firmware_version_hint: Some("525.116.04".into()),
    };
    let record = DeviceClassifier::classify_with_trust(&o, ts()).unwrap();
    assert_eq!(record.firmware_version, Some("525.116.04".into()));
}

// -- batch classification -----------------------------------------------------

#[test]
fn classify_batch_with_5_observations_returns_5_results() {
    let batch = EnumerationBatch {
        host_canonical_id: "host-01".into(),
        observed_at: ts(),
        observations: vec![
            obs(BusKind::Pci, 0x8086, 0x9A14, 0x06_00_00), // Memory
            obs(BusKind::Pcie, 0x144D, 0xA80A, 0x01_08_02), // NVMe
            obs(BusKind::Usb3, 0x8087, 0x0029, 0x00_09_00), // USB hub
            obs(BusKind::Pci, 0x8086, 0xA0C8, 0x04_03_00), // Audio
            obs(BusKind::Pci, 0x10DE, 0x2484, 0x03_00_00), // GPU discrete
        ],
    };
    let results = classify_batch(&batch);
    assert_eq!(results.len(), 5);
    assert!(results.iter().all(Result::is_ok));
}

#[test]
fn classify_batch_with_mixed_success_failure_partitions_correctly() {
    let good = obs(BusKind::Pcie, 0x8086, 0x9A49, 0x03_00_00);
    let bad = obs(BusKind::Pci, 0x9999, 0x0001, 0xFF_00_00);
    let batch = EnumerationBatch {
        host_canonical_id: "host-01".into(),
        observed_at: ts(),
        observations: vec![good.clone(), bad.clone(), good.clone(), bad, good],
    };
    let (records, errors) = classify_batch_into_records(&batch);
    assert_eq!(records.len(), 3);
    assert_eq!(errors.len(), 2);
    for rec in &records {
        assert_eq!(rec.class, DeviceClass::GpuIntegrated);
    }
    for err in &errors {
        assert_eq!(err.code(), HardwareErrorCode::ClassificationFailed);
    }
}
