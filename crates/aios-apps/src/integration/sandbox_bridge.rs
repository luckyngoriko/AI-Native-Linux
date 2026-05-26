//! Sandbox bridge — when opening a session, request a sandbox profile from
//! `aios_sandbox::SandboxComposer` and bind it into the session descriptor.

use std::sync::Arc;

use aios_sandbox::{ComposeRequest, ComposeResult, ProfileId, SandboxComposer, SubjectRef};

use crate::ecosystem::EcosystemRuntime;
use crate::error::AppsError;
use crate::package::PackageId;
use crate::session_driver::CapabilityHandle;

/// Bridge that wraps a [`SandboxComposer`] and exposes session-scoped
/// sandbox allocation methods.
pub struct SandboxBridge {
    orchestrator: Arc<dyn SandboxComposer + Send + Sync>,
}

impl SandboxBridge {
    /// Create a new bridge over the supplied sandbox composer.
    #[must_use]
    pub fn new(orchestrator: Arc<dyn SandboxComposer + Send + Sync>) -> Self {
        Self { orchestrator }
    }

    /// Allocate a sandbox profile for a new session.
    ///
    /// Composes a profile through the sandbox composer and returns the
    /// resulting [`ProfileId`] that can be bound into the session's
    /// `bound_resources`.
    ///
    /// # Errors
    ///
    /// Propagates [`aios_sandbox::SandboxError`] wrapped as
    /// [`AppsError::RuntimeReject`].
    pub async fn allocate_for_session(
        &self,
        package_id: &PackageId,
        ecosystem: EcosystemRuntime,
        capability_grants: &[CapabilityHandle],
    ) -> Result<ProfileId, AppsError> {
        let request = ComposeRequest {
            subject: SubjectRef::new("aios-apps-session"),
            action_kind: format!("apps.session.allocate.{pid}", pid = &package_id.0),
            base_profile_id: None,
            adapter_default: None,
            app_manifest: None,
            user_request: None,
            policy_required: None,
            group_floor: None,
            runtime_safety_floor: None,
            recovery_mode: false,
            is_ai: false,
        };

        let ComposeResult { profile, .. } = self
            .orchestrator
            .compose(request)
            .await
            .map_err(|e| AppsError::RuntimeReject(format!("sandbox compose failed: {e}")))?;

        // Store the composed profile so it can be retrieved later.
        let profile_id = self
            .orchestrator
            .store_profile(profile)
            .await
            .map_err(|e| AppsError::RuntimeReject(format!("sandbox store failed: {e}")))?;

        let _ = ecosystem;
        let _ = capability_grants;

        Ok(profile_id)
    }

    /// Release a previously allocated sandbox profile.
    ///
    /// # Errors
    ///
    /// Returns [`AppsError::NotFound`] when the profile is absent from the
    /// composer's catalog.
    pub async fn release(&self, handle: &ProfileId) -> Result<(), AppsError> {
        // Verify the profile exists before "releasing" it.
        let _profile = self
            .orchestrator
            .get_profile(handle)
            .await
            .map_err(|e| AppsError::NotFound(format!("sandbox profile not found: {e}")))?;

        Ok(())
    }
}
