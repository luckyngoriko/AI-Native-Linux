//! Policy bridge: `HardwareError` → `aios_policy::PolicyDecision` denial pathway.
//!
//! Translates L8 hardware errors into structured policy denial records consumable by
//! the Capability Runtime when a typed action is denied at the hardware layer.

use aios_action::ActionId;
use aios_policy::{ApprovalRequirement, Constraints, Decision, PolicyDecision};

use crate::error::HardwareError;

/// Synthesise a structured `PolicyDecision` denial from a `HardwareError`.
///
/// The returned decision always has `decision: Deny` and carries a stable `reason_code`
/// matching the error's `HardwareErrorCode` discriminator so the pipeline can route to
/// hard-deny classification, explain-logs, and evidence linkage.
#[must_use]
pub fn hardware_error_to_policy_denial(err: &HardwareError, decision_id: &str) -> PolicyDecision {
    let (reason_code, reason_message) = match err {
        HardwareError::DeviceNotFound(id) => ("DeviceNotFound", format!("{id:?}")),
        HardwareError::ClassificationFailed { device, reason } => {
            ("ClassificationFailed", format!("{device:?}: {reason}"))
        }
        HardwareError::DriverBindingFailed { device, reason } => {
            ("DriverBindingFailed", format!("{device:?}: {reason}"))
        }
        HardwareError::DriftFromPriorBoot {
            prior_graph_id,
            current_graph_id,
            changed_devices,
        } => (
            "DriftFromPriorBoot",
            format!("{prior_graph_id:?} -> {current_graph_id:?}, changed={changed_devices:?}"),
        ),
        HardwareError::CapabilityLie {
            device,
            advertised,
            observed,
        } => (
            "CapabilityLie",
            format!("{device:?} advertised={advertised} observed={observed}"),
        ),
        HardwareError::ThunderboltUnauthorized(id) => {
            ("ThunderboltUnauthorized", format!("{id:?}"))
        }
        HardwareError::IommuMissing(id) => ("IommuMissing", format!("{id:?}")),
        HardwareError::RemovableDenied { device, policy } => {
            ("RemovableDenied", format!("{device:?} by {policy:?}"))
        }
        HardwareError::GpuVramExhausted {
            gpu,
            requested,
            available,
        } => (
            "GpuVramExhausted",
            format!("{gpu:?} req={requested} avail={available}"),
        ),
        HardwareError::GpuBindingInvalid { gpu, reason } => {
            ("GpuBindingInvalid", format!("{gpu:?}: {reason}"))
        }
        HardwareError::DmabufPeerUnauthorized { src, target } => {
            ("DmabufPeerUnauthorized", format!("{src:?} -> {target:?}"))
        }
        HardwareError::FirmwareUnsigned(id) => ("FirmwareUnsigned", format!("{id:?}")),
        HardwareError::FirmwareSignatureInvalid { blob, reason } => {
            ("FirmwareSignatureInvalid", format!("{blob:?}: {reason}"))
        }
        HardwareError::FirmwareVersionRegression {
            blob,
            attempted,
            installed,
        } => (
            "FirmwareVersionRegression",
            format!("{blob:?} attempted={attempted} installed={installed}"),
        ),
        HardwareError::FirmwareScopeMismatch {
            blob,
            expected,
            advertised,
        } => (
            "FirmwareScopeMismatch",
            format!("{blob:?} expected={expected:?} advertised={advertised:?}"),
        ),
        HardwareError::FirmwareRefusedConstitutional { blob, reason } => (
            "FirmwareRefusedConstitutional",
            format!("{blob:?}: {reason}"),
        ),
        HardwareError::FirmwareApplyFailed { blob, reason } => {
            ("FirmwareApplyFailed", format!("{blob:?}: {reason}"))
        }
        HardwareError::GraphSnapshotSignatureInvalid(id) => {
            ("GraphSnapshotSignatureInvalid", format!("{id:?}"))
        }
        HardwareError::Internal(detail) => ("Internal", detail.clone()),
    };

    PolicyDecision {
        policy_decision_id: decision_id.to_owned(),
        action_id: ActionId::default(),
        request_hash: String::new(),
        bundle_version: String::new(),
        enrichment_snapshot_id: String::new(),
        decision: Decision::Deny,
        reason_code: reason_code.to_owned(),
        reason_message,
        constraints: Constraints::default(),
        approval: ApprovalRequirement::default(),
        evidence_receipt_id: String::new(),
        evaluated_at: chrono::Utc::now(),
        rules_consulted: 1,
        simulated: false,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test"
)]
mod tests {
    use super::*;
    use crate::ids::{DeviceId, FirmwareBlobId, GpuId};
    use crate::removable::RemovableDevicePolicy;

    #[test]
    fn device_not_found_produces_policy_denial() {
        let err = HardwareError::DeviceNotFound(DeviceId("dev_test01".into()));
        let decision = hardware_error_to_policy_denial(&err, "poldec_test");
        assert_eq!(decision.decision, Decision::Deny);
        assert_eq!(decision.reason_code, "DeviceNotFound");
    }

    #[test]
    fn iommu_missing_produces_policy_denial() {
        let err = HardwareError::IommuMissing(DeviceId("dev_test02".into()));
        let decision = hardware_error_to_policy_denial(&err, "poldec_02");
        assert_eq!(decision.decision, Decision::Deny);
        assert_eq!(decision.reason_code, "IommuMissing");
    }

    #[test]
    fn capability_lie_produces_policy_denial() {
        let err = HardwareError::CapabilityLie {
            device: DeviceId("dev_test03".into()),
            advertised: "PCIe Gen4 x16".into(),
            observed: "PCIe Gen2 x4".into(),
        };
        let decision = hardware_error_to_policy_denial(&err, "poldec_03");
        assert_eq!(decision.decision, Decision::Deny);
        assert_eq!(decision.reason_code, "CapabilityLie");
    }

    #[test]
    fn gpu_vram_exhausted_produces_policy_denial() {
        let err = HardwareError::GpuVramExhausted {
            gpu: GpuId("gpu_test01".into()),
            requested: 4096,
            available: 512,
        };
        let decision = hardware_error_to_policy_denial(&err, "poldec_04");
        assert_eq!(decision.decision, Decision::Deny);
        assert_eq!(decision.reason_code, "GpuVramExhausted");
    }

    #[test]
    fn removable_denied_produces_policy_denial() {
        let err = HardwareError::RemovableDenied {
            device: DeviceId("dev_test05".into()),
            policy: RemovableDevicePolicy::DenyDefault,
        };
        let decision = hardware_error_to_policy_denial(&err, "poldec_05");
        assert_eq!(decision.decision, Decision::Deny);
        assert_eq!(decision.reason_code, "RemovableDenied");
    }

    #[test]
    fn firmware_unsigned_produces_policy_denial() {
        use crate::ids::FirmwareBlobId;
        let err = HardwareError::FirmwareUnsigned(FirmwareBlobId("fwb_test01".into()));
        let decision = hardware_error_to_policy_denial(&err, "poldec_06");
        assert_eq!(decision.decision, Decision::Deny);
        assert_eq!(decision.reason_code, "FirmwareUnsigned");
    }

    #[test]
    fn all_error_variants_map_to_non_empty_reason_code() {
        use crate::removable::RemovableDevicePolicy;
        let errors: Vec<HardwareError> = vec![
            HardwareError::DeviceNotFound(DeviceId("d".into())),
            HardwareError::ClassificationFailed {
                device: DeviceId("d".into()),
                reason: "r".into(),
            },
            HardwareError::DriverBindingFailed {
                device: DeviceId("d".into()),
                reason: "r".into(),
            },
            HardwareError::DriftFromPriorBoot {
                prior_graph_id: crate::ids::HardwareGraphId("pg".into()),
                current_graph_id: crate::ids::HardwareGraphId("cg".into()),
                changed_devices: vec![],
            },
            HardwareError::CapabilityLie {
                device: DeviceId("d".into()),
                advertised: "a".into(),
                observed: "o".into(),
            },
            HardwareError::ThunderboltUnauthorized(DeviceId("d".into())),
            HardwareError::IommuMissing(DeviceId("d".into())),
            HardwareError::RemovableDenied {
                device: DeviceId("d".into()),
                policy: RemovableDevicePolicy::DenyDefault,
            },
            HardwareError::GpuVramExhausted {
                gpu: GpuId("g".into()),
                requested: 1,
                available: 0,
            },
            HardwareError::GpuBindingInvalid {
                gpu: GpuId("g".into()),
                reason: "r".into(),
            },
            HardwareError::DmabufPeerUnauthorized {
                src: GpuId("s".into()),
                target: GpuId("t".into()),
            },
            HardwareError::FirmwareUnsigned(FirmwareBlobId("f".into())),
            HardwareError::FirmwareSignatureInvalid {
                blob: FirmwareBlobId("f".into()),
                reason: "r".into(),
            },
            HardwareError::FirmwareVersionRegression {
                blob: FirmwareBlobId("f".into()),
                attempted: "v2".into(),
                installed: "v1".into(),
            },
            HardwareError::FirmwareScopeMismatch {
                blob: FirmwareBlobId("f".into()),
                expected: crate::firmware::FirmwareScope::BiosUefi,
                advertised: crate::firmware::FirmwareScope::Gpu,
            },
            HardwareError::FirmwareRefusedConstitutional {
                blob: FirmwareBlobId("f".into()),
                reason: "r".into(),
            },
            HardwareError::FirmwareApplyFailed {
                blob: FirmwareBlobId("f".into()),
                reason: "r".into(),
            },
            HardwareError::GraphSnapshotSignatureInvalid(crate::ids::HardwareGraphId("g".into())),
            HardwareError::Internal("e".into()),
        ];
        // 19 variants
        assert_eq!(
            errors.len(),
            19,
            "all 19 HardwareError variants must be covered"
        );
        for err in &errors {
            let decision = hardware_error_to_policy_denial(err, "poldec_all");
            assert_eq!(decision.decision, Decision::Deny);
            assert!(
                !decision.reason_code.is_empty(),
                "reason_code must be non-empty for {err:?}"
            );
        }
    }
}
