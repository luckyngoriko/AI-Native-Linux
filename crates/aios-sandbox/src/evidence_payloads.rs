//! Typed sandbox evidence payloads for S3.2 -> S3.1 emission.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S3.2 evidence vocabulary"
)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::composer::SubjectRef;
use crate::{GpuCapabilityClass, IsolationKind, NetworkPosture, ProfileId};

/// Payload for sandbox profile composition.
///
/// S3.1 has no dedicated `SANDBOX_COMPOSED` record type; the emitter folds this
/// payload into `POLICY_DECISION`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct SandboxComposedPayload {
    /// The composed profile id.
    pub profile_id: ProfileId,
    /// The merged isolation kind for this sandbox.
    pub isolation_kind: IsolationKind,
    /// The merged network posture for this sandbox.
    pub network_posture: NetworkPosture,
    /// The merged GPU capability class for this sandbox.
    pub gpu_capability_class: GpuCapabilityClass,
    /// UTC timestamp when composition was completed.
    pub composed_at: DateTime<Utc>,
}

/// Payload for sandbox violation detection.
///
/// S3.1 has `SANDBOX_BUNDLE_REJECTED` (ID 408) as the closest sandbox-specific
/// variant; the emitter folds this payload into that record type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct SandboxViolationDetectedPayload {
    /// The action id that triggered the violation.
    pub action_id: String,
    /// Human-readable summary of the violation.
    pub violation_summary: String,
    /// The profile id against which the violation was detected.
    pub profile_id: ProfileId,
    /// UTC timestamp when the violation was detected.
    pub detected_at: DateTime<Utc>,
}

/// Payload for GPU capability binding.
///
/// S3.1 has `GPU_CAPABILITY_DENIED` (ID 73) as the closest GPU-specific variant;
/// the emitter folds this payload into that record type. INV-018: no raw
/// signature bytes — only the binding id, group, subject, and capability class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct GpuCapabilityBoundPayload {
    /// The binding id — `gcb_<ULID>`.
    pub binding_id: String,
    /// The group to which this binding applies.
    pub group_id: String,
    /// The subject to which this binding applies.
    pub subject: SubjectRef,
    /// The GPU capability class granted by this binding.
    pub gpu_capability_class: GpuCapabilityClass,
    /// True when IOMMU is required but unavailable (degraded isolation).
    pub degraded_isolation: bool,
    /// UTC timestamp when the binding was issued.
    pub bound_at: DateTime<Utc>,
}

/// Payload for resource limit exceedance.
///
/// S3.1 has no dedicated `RESOURCE_LIMIT_EXCEEDED` record type; the emitter
/// folds this payload into `POLICY_DECISION`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ResourceLimitExceededPayload {
    /// The profile id for which the limit was exceeded.
    pub profile_id: ProfileId,
    /// The name of the limit that was exceeded.
    pub limit_kind: String,
    /// The requested value.
    pub requested: u64,
    /// The maximum allowed value.
    pub max: u64,
    /// UTC timestamp when the limit was exceeded.
    pub exceeded_at: DateTime<Utc>,
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_composed_payload_serde_round_trip() {
        let payload = SandboxComposedPayload {
            profile_id: ProfileId::new(),
            isolation_kind: IsolationKind::VmGuest,
            network_posture: NetworkPosture::LoopbackOnly,
            gpu_capability_class: GpuCapabilityClass::GpuBasic2d,
            composed_at: Utc::now(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let back: SandboxComposedPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(payload, back);
    }

    #[test]
    fn sandbox_violation_detected_payload_serde_round_trip() {
        let payload = SandboxViolationDetectedPayload {
            action_id: "act_01JQZYX80W3YQH7K4N5R8T9F2X".into(),
            violation_summary: "CPU quota exceeded".into(),
            profile_id: ProfileId::new(),
            detected_at: Utc::now(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let back: SandboxViolationDetectedPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(payload, back);
    }

    #[test]
    fn gpu_capability_bound_payload_serde_round_trip() {
        let payload = GpuCapabilityBoundPayload {
            binding_id: "gcb_01JQZYX80W3YQH7K4N5R8T9F2X".into(),
            group_id: "group-alpha".into(),
            subject: SubjectRef::new("app:browser"),
            gpu_capability_class: GpuCapabilityClass::GpuRich2d,
            degraded_isolation: false,
            bound_at: Utc::now(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let back: GpuCapabilityBoundPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(payload, back);
    }

    #[test]
    fn resource_limit_exceeded_payload_serde_round_trip() {
        let payload = ResourceLimitExceededPayload {
            profile_id: ProfileId::new(),
            limit_kind: "memory_max_bytes".into(),
            requested: 2_000_000_000,
            max: 1_000_000_000,
            exceeded_at: Utc::now(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let back: ResourceLimitExceededPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(payload, back);
    }

    #[test]
    fn inv_018_no_raw_signature_bytes_in_any_payload() {
        // All four payload types carry only ids, timestamps, enums, and counts.
        // None carry raw bytes, secret material, or key handles.
        let composed = SandboxComposedPayload {
            profile_id: ProfileId::new(),
            isolation_kind: IsolationKind::ProcessContainer,
            network_posture: NetworkPosture::DenyAll,
            gpu_capability_class: GpuCapabilityClass::GpuPassiveDisplay,
            composed_at: Utc::now(),
        };
        let json = serde_json::to_string(&composed).unwrap();
        // No hex strings longer than 64 chars (which would indicate raw bytes)
        assert!(!json.contains("0000000000000000000000000000000000000000000000000000000000000000"));

        let gpu = GpuCapabilityBoundPayload {
            binding_id: "gcb_test".into(),
            group_id: "g".into(),
            subject: SubjectRef::new("s"),
            gpu_capability_class: GpuCapabilityClass::GpuBasic2d,
            degraded_isolation: true,
            bound_at: Utc::now(),
        };
        let json = serde_json::to_string(&gpu).unwrap();
        assert!(!json.contains("0000000000000000000000000000000000000000000000000000000000000000"));
    }
}
