//! Capability bridge: `DriverBinding` → typed `ActionEnvelope`.
//!
//! Converts an admitted driver binding into a `hardware.driver.bind_request` action
//! envelope so the Capability Runtime / Policy Kernel can audit it through the normal
//! pipeline.

use aios_action::envelope::ActionEnvelope;
use aios_action::identity::Identity;
use aios_action::request::{DryRunMode, Request};
use aios_action::trace::Trace;

use crate::driver_binding::DriverBinding;

/// Turn a `DriverBinding` into a typed `hardware.driver.bind_request` action envelope.
///
/// The envelope wraps the binding as a JSON payload under
/// `action = "hardware.driver.bind_request"`. The caller identity is set to the
/// requester with `is_ai: false` — the cognitive core may override this when an AI
/// agent requests the binding.
#[must_use]
pub fn driver_binding_to_action_envelope(
    binding: &DriverBinding,
    requester: &str,
) -> ActionEnvelope {
    let identity = Identity {
        subject_canonical_id: requester.to_owned(),
        is_ai: false,
        session_id: None,
    };

    let payload = serde_json::json!({
        "binding_id": binding.binding_id.0,
        "device_id": binding.device_id.0,
        "driver_module_name": binding.driver_module_name,
        "kernel_module_version": binding.kernel_module_version,
        "provenance": binding.provenance.label(),
        "blake3_hash": binding.blake3_hash,
        "signer_fingerprint": binding.signer_fingerprint,
    });

    let request = Request {
        action: "hardware.driver.bind_request".to_owned(),
        target: payload,
        idempotency_key: Some(format!("bind_{}", binding.binding_id.0)),
        parent_action_id: None,
        dry_run: DryRunMode::Live,
    };

    let trace = Trace {
        trace_id: String::new(),
        span_id: String::new(),
        parent_span_id: None,
    };

    ActionEnvelope::new(identity, request, trace)
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
    use crate::driver::DriverProvenance;
    use crate::ids::DriverBindingId;

    #[test]
    fn binding_produces_action_envelope() {
        let binding = DriverBinding {
            binding_id: DriverBindingId("drvb_test01".into()),
            device_id: crate::ids::DeviceId("dev_test".into()),
            driver_module_name: "i915".into(),
            kernel_module_version: "6.8.0".into(),
            provenance: DriverProvenance::AiosVerified,
            blake3_hash: "abc123".into(),
            signer_fingerprint: "aabbcc".into(),
            signature: vec![1, 2, 3],
            admitted_at: chrono::Utc::now(),
        };

        let env = driver_binding_to_action_envelope(&binding, "human:lucky");
        assert_eq!(
            env.request.action, "hardware.driver.bind_request",
            "action name must match"
        );
        assert_eq!(env.identity.subject_canonical_id, "human:lucky");
        assert!(!env.identity.is_ai);
    }

    #[test]
    fn ai_requester_sets_subject_canonical_id() {
        let binding = DriverBinding {
            binding_id: DriverBindingId("drvb_test02".into()),
            device_id: crate::ids::DeviceId("dev_test".into()),
            driver_module_name: "nvme".into(),
            kernel_module_version: "1.0".into(),
            provenance: DriverProvenance::SignedKernelModule,
            blake3_hash: "def456".into(),
            signer_fingerprint: "ddeeff".into(),
            signature: vec![4, 5, 6],
            admitted_at: chrono::Utc::now(),
        };

        let env = driver_binding_to_action_envelope(&binding, "agent:dev");
        assert_eq!(env.identity.subject_canonical_id, "agent:dev");
    }
}
