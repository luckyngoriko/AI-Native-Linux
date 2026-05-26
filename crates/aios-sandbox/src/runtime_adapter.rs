//! T-112 — `SandboxRuntimeAdapter` cross-crate integration point.
//!
//! Bridges [`InMemorySandboxComposer`] to the capability-runtime
//! [`RuntimeSandboxComposer`] trait (S10.1 §19.1 ↔ S3.2).

use std::sync::Arc;

use aios_capability_runtime::{RuntimeSandboxComposer, SandboxProfileSummary};

use crate::composer::{ComposeRequest, SandboxComposer, SubjectRef};
use crate::InMemorySandboxComposer;

/// Cross-crate adapter wrapping an [`InMemorySandboxComposer`].
///
/// Implements [`RuntimeSandboxComposer`] so the pipeline can request a
/// sandbox profile during the validate step.
pub struct SandboxRuntimeAdapter {
    composer: Arc<InMemorySandboxComposer>,
}

impl SandboxRuntimeAdapter {
    /// Wrap an [`InMemorySandboxComposer`] as a [`RuntimeSandboxComposer`].
    #[must_use]
    pub const fn new(composer: Arc<InMemorySandboxComposer>) -> Self {
        Self { composer }
    }
}

impl std::fmt::Debug for SandboxRuntimeAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SandboxRuntimeAdapter")
            .field("composer", &self.composer)
            .finish()
    }
}

#[async_trait::async_trait]
impl RuntimeSandboxComposer for SandboxRuntimeAdapter {
    async fn compose_for_action(
        &self,
        action_target: &str,
        subject: &str,
        is_ai: bool,
        recovery_mode: bool,
    ) -> Result<SandboxProfileSummary, String> {
        let request = ComposeRequest {
            subject: SubjectRef::new(subject),
            action_kind: action_target.to_string(),
            base_profile_id: None,
            adapter_default: None,
            app_manifest: None,
            user_request: None,
            policy_required: None,
            group_floor: None,
            runtime_safety_floor: None,
            recovery_mode,
            is_ai,
        };

        let result = self
            .composer
            .compose(request)
            .await
            .map_err(|e| format!("sandbox composition failed: {e}"))?;

        let profile = &result.profile;
        Ok(SandboxProfileSummary {
            profile_id: profile.profile_id.to_string(),
            isolation_kind: format!("{:?}", profile.isolation_kind),
            network_posture: format!("{:?}", profile.network_posture),
            gpu_capability_class: format!("{:?}", profile.gpu_policy.gpu_capability_class),
        })
    }
}
