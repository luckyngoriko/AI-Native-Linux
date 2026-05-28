#![allow(missing_docs)]

use chrono::{DateTime, Utc};

use crate::bus::BusKind;
use crate::device::DeviceClass;
use crate::device_record::HardwareDeviceRecord;
use crate::error::HardwareError;
use crate::ids::DeviceId;
use crate::lifecycle::DeviceLifecycleState;
use crate::observation::{EnumerationBatch, RawDeviceObservation};
use crate::trust_class::DeviceTrustClass;

/// Deterministic device classifier — maps `(BusKind, vendor_id, product_id,
/// class_hint)` to one of the 16 closed `DeviceClass` values (S8.3 §3.1).
///
/// Pure-Rust, no `/sys` reads. Observations must be provided by the caller
/// via `RawDeviceObservation`.
#[derive(Debug, Default)]
pub struct DeviceClassifier;

impl DeviceClassifier {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Classify a single raw device observation into a `DeviceClass`.
    ///
    /// # Errors
    ///
    /// Returns `Err(ClassificationFailed { ... })` when no classification
    /// rule matches the given `(BusKind, vendor_id, product_id, class_hint)`
    /// tuple.
    pub fn classify(obs: &RawDeviceObservation) -> Result<DeviceClass, HardwareError> {
        let fail = |reason: &str| {
            Err(HardwareError::ClassificationFailed {
                device: DeviceId(format!(
                    "{}:{:04x}:{:04x}",
                    obs.bus.label(),
                    obs.vendor_id,
                    obs.product_id
                )),
                reason: reason.into(),
            })
        };

        match obs.bus {
            BusKind::Pci | BusKind::Pcie => Self::classify_pci(obs, fail),
            BusKind::Usb2 | BusKind::Usb3 | BusKind::Usb4 => Self::classify_usb(obs, fail),
            BusKind::Thunderbolt => Ok(DeviceClass::ThunderboltController),
            BusKind::Nvme => Ok(DeviceClass::StorageNvme),
            BusKind::I2c => Ok(DeviceClass::SensorOrInputDevice),
        }
    }

    /// Classify and produce a fully-populated `HardwareDeviceRecord`.
    ///
    /// Trust is defaulted to `Untrusted`; lifecycle to `Detected`. Driver
    /// binding (T-166) will upgrade both fields once a signed driver
    /// registry is consulted.
    ///
    /// # Errors
    ///
    /// Returns `Err(ClassificationFailed { ... })` when the underlying
    /// `classify` call fails.
    pub fn classify_with_trust(
        obs: &RawDeviceObservation,
        ts: DateTime<Utc>,
    ) -> Result<HardwareDeviceRecord, HardwareError> {
        let class = Self::classify(obs)?;
        let device_id = DeviceId(format!(
            "{}:{:04x}:{:04x}@{}",
            obs.bus.label(),
            obs.vendor_id,
            obs.product_id,
            obs.bus_address
        ));

        Ok(HardwareDeviceRecord {
            device_id,
            class,
            bus: obs.bus,
            vendor_id: obs.vendor_id,
            product_id: obs.product_id,
            vendor_name: obs.vendor_name.clone().unwrap_or_default(),
            product_name: obs.product_name.clone().unwrap_or_default(),
            trust_class: DeviceTrustClass::Untrusted,
            lifecycle: DeviceLifecycleState::Detected,
            driver_provenance: None,
            firmware_version: obs.firmware_version_hint.clone(),
            removable: obs.removable_hint,
            iommu_protected: obs.iommu_protected_hint,
            probed_at: ts,
        })
    }

    // -- PCI / PCIe ---------------------------------------------------------

    fn classify_pci(
        obs: &RawDeviceObservation,
        fail: impl Fn(&str) -> Result<DeviceClass, HardwareError>,
    ) -> Result<DeviceClass, HardwareError> {
        let base_class = ((obs.class_hint >> 16) & 0xFF) as u8;
        let sub_class = ((obs.class_hint >> 8) & 0xFF) as u8;

        match base_class {
            // Display controller
            0x03 => {
                if obs.vendor_id == 0x10DE {
                    return Ok(DeviceClass::GpuDiscrete); // NVIDIA
                }
                if obs.vendor_id == 0x1002 {
                    // AMD: discrete ≥ 0x6600, APU below
                    if obs.product_id >= 0x6600 {
                        return Ok(DeviceClass::GpuDiscrete);
                    }
                    return Ok(DeviceClass::GpuIntegrated);
                }
                if obs.vendor_id == 0x8086 {
                    return Ok(DeviceClass::GpuIntegrated); // Intel
                }
                fail("unrecognised GPU vendor for class 0x03")
            }
            // Network controller
            0x02 => match sub_class {
                0x00 => Ok(DeviceClass::NetworkEthernet),
                0x80 => Ok(DeviceClass::NetworkWifi),
                _ => fail("unknown PCI network subclass"),
            },
            // Mass storage controller
            0x01 => match sub_class {
                0x08 => Ok(DeviceClass::StorageNvme),
                0x06 => Ok(DeviceClass::StorageSata),
                0x05 => Ok(DeviceClass::StorageMmc),
                _ => fail("unknown PCI storage subclass"),
            },
            // Multimedia audio
            0x04 => Ok(DeviceClass::AudioCard),
            // Serial bus controller
            0x0C => match sub_class {
                0x03 => Ok(DeviceClass::UsbController),
                0x04 => Ok(DeviceClass::ThunderboltController),
                _ => fail("unknown PCI serial-bus subclass"),
            },
            // Memory controller — only if vendor is a known host bridge
            0x06 => {
                if obs.vendor_id == 0x8086 || obs.vendor_id == 0x1022 {
                    Ok(DeviceClass::Memory)
                } else {
                    Ok(DeviceClass::SensorOrInputDevice)
                }
            }
            _ => fail("unrecognised PCI base class"),
        }
    }

    // -- USB ----------------------------------------------------------------

    fn classify_usb(
        obs: &RawDeviceObservation,
        fail: impl Fn(&str) -> Result<DeviceClass, HardwareError>,
    ) -> Result<DeviceClass, HardwareError> {
        let class = ((obs.class_hint >> 8) & 0xFF) as u8;

        match class {
            0x09 => Ok(DeviceClass::UsbController), // Hub
            0xE0 => {
                // Wireless controller
                Ok(DeviceClass::NetworkBluetooth)
            }
            0x01 => Ok(DeviceClass::AudioCard), // Audio
            0x07 => Ok(DeviceClass::PrinterOrScanner),
            0x03 => Ok(DeviceClass::SensorOrInputDevice), // HID
            _ => fail("unrecognised USB device class"),
        }
    }
}

/// Classify every observation in a batch independently. A single
/// unclassifiable device does **not** abort the batch.
#[must_use]
pub fn classify_batch(
    batch: &EnumerationBatch,
) -> Vec<Result<HardwareDeviceRecord, HardwareError>> {
    batch
        .observations
        .iter()
        .map(|obs| DeviceClassifier::classify_with_trust(obs, batch.observed_at))
        .collect()
}

/// Partition batch results into `(successes, failures)`.
#[must_use]
pub fn classify_batch_into_records(
    batch: &EnumerationBatch,
) -> (Vec<HardwareDeviceRecord>, Vec<HardwareError>) {
    let mut records = Vec::new();
    let mut errors = Vec::new();
    for result in classify_batch(batch) {
        match result {
            Ok(rec) => records.push(rec),
            Err(e) => errors.push(e),
        }
    }
    (records, errors)
}
