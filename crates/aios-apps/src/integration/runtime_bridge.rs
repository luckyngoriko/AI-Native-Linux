//! Runtime bridge — translate apps lifecycle ops into typed
//! `ActionEnvelope` and dispatch via `aios_capability_runtime::CapabilityRuntime`.

use std::sync::Arc;

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_capability_runtime::{
    ActionContext, ActionLifecycleState, CapabilityRuntime, RuntimeContext,
};

use crate::error::AppsError;
use crate::package::PackageId;
use crate::session_driver::CapabilityHandle;
use crate::update_driver::UpdatePlanId;

/// Bridge that wraps a [`CapabilityRuntime`] and exposes apps-specific
/// lifecycle methods.
pub struct RuntimeBridge {
    runtime: Arc<dyn CapabilityRuntime + Send + Sync>,
}

impl RuntimeBridge {
    /// Create a new bridge over the supplied runtime.
    #[must_use]
    pub fn new(runtime: Arc<dyn CapabilityRuntime + Send + Sync>) -> Self {
        Self { runtime }
    }

    /// Build a typed `apps.install` action, submit it to the capability
    /// runtime, and return the resulting [`ActionContext`].
    ///
    /// # Errors
    ///
    /// Returns [`AppsError::RuntimeReject`] when the runtime returns a
    /// terminal status that is not `Succeeded` (policy denial, adapter
    /// refusal, execution failure, etc.).
    pub async fn dispatch_install(
        &self,
        package_id: &PackageId,
        requester: &str,
        capability_grants: &[CapabilityHandle],
    ) -> Result<ActionContext, AppsError> {
        let target = serde_json::json!({
            "package_id": package_id.0,
            "capability_grants": capability_grants
                .iter()
                .map(|c| &c.capability_id)
                .collect::<Vec<_>>(),
        });

        let envelope = ActionEnvelope::new(
            Identity::new(requester, false),
            Request::new("apps.install", target),
            Trace::new("00000000000000000000000000000000", "0000000000000000", None),
        );

        let context = RuntimeContext::new(requester, "aios-apps/0.0.1", "aios-apps/0.0.1");

        let ctx = self
            .runtime
            .submit_action(&envelope, &context)
            .await
            .map_err(|e| AppsError::RuntimeReject(format!("runtime error: {e}")))?;

        if ctx.status != ActionLifecycleState::Succeeded {
            return Err(AppsError::RuntimeReject(format!(
                "install action ended in state {ctx_status:?}",
                ctx_status = ctx.status,
            )));
        }

        Ok(ctx)
    }

    /// Build a typed `apps.update_activate` action for the given plan,
    /// submit it to the capability runtime, and return the resulting
    /// [`ActionContext`].
    ///
    /// # Errors
    ///
    /// Returns [`AppsError::RuntimeReject`] when the runtime returns a
    /// non-`Succeeded` terminal state.
    pub async fn dispatch_update_activation(
        &self,
        plan_id: &UpdatePlanId,
        target_version: &str,
        requester: &str,
    ) -> Result<ActionContext, AppsError> {
        let target = serde_json::json!({
            "plan_id": plan_id.0,
            "target_version": target_version,
        });

        let envelope = ActionEnvelope::new(
            Identity::new(requester, false),
            Request::new("apps.update_activate", target),
            Trace::new("00000000000000000000000000000000", "0000000000000000", None),
        );

        let context = RuntimeContext::new(requester, "aios-apps/0.0.1", "aios-apps/0.0.1");

        let ctx = self
            .runtime
            .submit_action(&envelope, &context)
            .await
            .map_err(|e| AppsError::RuntimeReject(format!("runtime error: {e}")))?;

        if ctx.status != ActionLifecycleState::Succeeded {
            return Err(AppsError::RuntimeReject(format!(
                "update activation ended in state {ctx_status:?}",
                ctx_status = ctx.status,
            )));
        }

        Ok(ctx)
    }
}
