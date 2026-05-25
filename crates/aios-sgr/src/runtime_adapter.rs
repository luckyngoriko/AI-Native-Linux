//! Capability Runtime adapter for SGR unit lifecycle actions.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the cross-crate runtime integration vocabulary"
)]

use std::fmt;
use std::sync::Arc;

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_capability_runtime::{
    ActionContext, ActionLifecycleState, CapabilityRuntime, RuntimeContext, RuntimeError,
};

use crate::{ServiceGraph, ServiceUnit, SgrError, UnitFsmDriver, UnitId, DEFAULT_CODE_VERSION};

const SGR_RUNTIME_SUBJECT: &str = "_system:sgr";
const UNIT_START_ACTION: &str = "unit.start";
const UNIT_STOP_ACTION: &str = "unit.stop";

/// Converts an SGR unit lifecycle intent into a typed runtime action envelope.
pub trait UnitActionFactory: Send + Sync {
    /// Build the `unit.start` action envelope for `unit`.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError`] when the unit manifest cannot be mapped to a
    /// runtime action.
    fn build_action(&self, unit: &ServiceUnit) -> Result<ActionEnvelope, SgrError>;
}

/// Default SGR unit-action envelope builder.
pub struct DefaultUnitActionFactory;

impl UnitActionFactory for DefaultUnitActionFactory {
    fn build_action(&self, unit: &ServiceUnit) -> Result<ActionEnvelope, SgrError> {
        build_unit_action(unit, UNIT_START_ACTION, "start")
    }
}

/// Cross-crate bridge from SGR unit state to the Capability Runtime pipeline.
#[derive(Clone)]
pub struct SgrCapabilityAdapter {
    graph: Arc<dyn ServiceGraph>,
    fsm: Arc<UnitFsmDriver>,
    runtime: Arc<dyn CapabilityRuntime>,
    action_envelope_factory: Arc<dyn UnitActionFactory>,
}

impl fmt::Debug for SgrCapabilityAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SgrCapabilityAdapter")
            .field("graph", &"<dyn ServiceGraph>")
            .field("fsm", &"<UnitFsmDriver>")
            .field("runtime", &"<dyn CapabilityRuntime>")
            .field("action_envelope_factory", &"<dyn UnitActionFactory>")
            .finish()
    }
}

impl SgrCapabilityAdapter {
    /// Construct an adapter from already-wired graph, FSM, runtime, and factory handles.
    #[must_use]
    pub fn new(
        graph: Arc<dyn ServiceGraph>,
        fsm: Arc<UnitFsmDriver>,
        runtime: Arc<dyn CapabilityRuntime>,
        action_envelope_factory: Arc<dyn UnitActionFactory>,
    ) -> Self {
        Self {
            graph,
            fsm,
            runtime,
            action_envelope_factory,
        }
    }

    /// Construct an adapter using [`DefaultUnitActionFactory`].
    #[must_use]
    pub fn with_default_factory(
        graph: Arc<dyn ServiceGraph>,
        fsm: Arc<UnitFsmDriver>,
        runtime: Arc<dyn CapabilityRuntime>,
    ) -> Self {
        Self::new(graph, fsm, runtime, Arc::new(DefaultUnitActionFactory))
    }

    /// Fetch a unit, submit its `unit.start` action through L3, then converge
    /// the SGR unit FSM from `STARTING` to `RUNNING` on runtime success.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError`] when the unit is absent, action construction fails,
    /// the runtime returns an orchestration error, or the SGR FSM rejects the
    /// resulting transition.
    pub async fn start_unit_via_runtime(
        &self,
        unit_id: &UnitId,
    ) -> Result<ActionContext, SgrError> {
        let unit = self.graph.get_unit(unit_id).await?;
        let envelope = match self.action_envelope_factory.build_action(&unit) {
            Ok(envelope) => envelope,
            Err(err) => {
                self.mark_failed_best_effort(unit_id, &err.to_string())
                    .await?;
                return Err(err);
            }
        };
        let context = runtime_context();
        let ctx = self.submit_or_fail(unit_id, &envelope, &context).await?;
        self.apply_start_result(unit_id, &ctx).await?;
        Ok(ctx)
    }

    /// Fetch a unit, submit its `unit.stop` action through L3, then converge
    /// the SGR unit FSM to `STOPPED` on runtime success.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError`] when the unit is absent, action construction fails,
    /// the runtime returns an orchestration error, or the SGR FSM rejects the
    /// resulting transition.
    pub async fn stop_unit_via_runtime(&self, unit_id: &UnitId) -> Result<ActionContext, SgrError> {
        let unit = self.graph.get_unit(unit_id).await?;
        let mut envelope = match self.action_envelope_factory.build_action(&unit) {
            Ok(envelope) => envelope,
            Err(err) => {
                self.mark_failed_best_effort(unit_id, &err.to_string())
                    .await?;
                return Err(err);
            }
        };
        retarget_for_stop(&mut envelope);
        let context = runtime_context();
        let ctx = self.submit_or_fail(unit_id, &envelope, &context).await?;
        self.apply_stop_result(unit_id, &ctx).await?;
        Ok(ctx)
    }

    async fn submit_or_fail(
        &self,
        unit_id: &UnitId,
        envelope: &ActionEnvelope,
        context: &RuntimeContext,
    ) -> Result<ActionContext, SgrError> {
        match self.runtime.submit_action(envelope, context).await {
            Ok(ctx) => Ok(ctx),
            Err(err) => {
                self.mark_failed_best_effort(unit_id, &err.to_string())
                    .await?;
                Err(runtime_error_to_sgr(&err))
            }
        }
    }

    async fn apply_start_result(
        &self,
        unit_id: &UnitId,
        ctx: &ActionContext,
    ) -> Result<(), SgrError> {
        match ctx.status {
            ActionLifecycleState::Succeeded => {
                self.fsm.start(unit_id).await?;
            }
            ActionLifecycleState::Failed
            | ActionLifecycleState::PolicyDenied
            | ActionLifecycleState::OverrideDenied
            | ActionLifecycleState::RollbackFailed => {
                self.fsm.mark_failed(unit_id, failure_reason(ctx)).await?;
            }
            ActionLifecycleState::RolledBack => {
                self.fsm
                    .mark_failed(unit_id, "runtime rolled back".to_owned())
                    .await?;
            }
            ActionLifecycleState::Created
            | ActionLifecycleState::PolicyPending
            | ActionLifecycleState::ApprovalPending
            | ActionLifecycleState::OverridePending
            | ActionLifecycleState::Approved
            | ActionLifecycleState::Queued
            | ActionLifecycleState::Executing
            | ActionLifecycleState::Verifying => {}
        }
        Ok(())
    }

    async fn apply_stop_result(
        &self,
        unit_id: &UnitId,
        ctx: &ActionContext,
    ) -> Result<(), SgrError> {
        match ctx.status {
            ActionLifecycleState::Succeeded => {
                self.fsm.stop(unit_id).await?;
            }
            ActionLifecycleState::Failed
            | ActionLifecycleState::PolicyDenied
            | ActionLifecycleState::OverrideDenied
            | ActionLifecycleState::RollbackFailed => {
                self.fsm.mark_failed(unit_id, failure_reason(ctx)).await?;
            }
            ActionLifecycleState::RolledBack => {
                self.fsm
                    .mark_failed(unit_id, "runtime rolled back".to_owned())
                    .await?;
            }
            ActionLifecycleState::Created
            | ActionLifecycleState::PolicyPending
            | ActionLifecycleState::ApprovalPending
            | ActionLifecycleState::OverridePending
            | ActionLifecycleState::Approved
            | ActionLifecycleState::Queued
            | ActionLifecycleState::Executing
            | ActionLifecycleState::Verifying => {}
        }
        Ok(())
    }

    async fn mark_failed_best_effort(
        &self,
        unit_id: &UnitId,
        reason: &str,
    ) -> Result<(), SgrError> {
        self.fsm.mark_failed(unit_id, reason.to_owned()).await?;
        Ok(())
    }
}

fn build_unit_action(
    unit: &ServiceUnit,
    action: &str,
    lifecycle_intent: &str,
) -> Result<ActionEnvelope, SgrError> {
    let adapter_id =
        unit.manifest
            .adapter_id
            .as_ref()
            .ok_or_else(|| SgrError::AdapterCapabilityMismatch {
                manifest: unit.unit_id.clone(),
                missing: vec!["adapter_id".to_owned()],
            })?;
    let mut request = Request::new(
        action,
        serde_json::json!({
            "unit_id": unit.unit_id.as_str(),
            "adapter_id": adapter_id,
            "adapter_target": unit.manifest.adapter_target,
            "lifecycle_intent": lifecycle_intent,
            "manifest": {
                "display_name": unit.manifest.display_name,
                "unit_kind": unit.manifest.unit_kind,
                "sandbox_profile_ref": unit.manifest.sandbox_profile_ref,
                "startup_deadline_seconds": unit.manifest.startup_deadline_seconds,
                "stop_deadline_seconds": unit.manifest.stop_deadline_seconds,
            },
        }),
    );
    request.idempotency_key = Some(format!("{action}:{}", unit.unit_id));
    Ok(ActionEnvelope::new(
        Identity::new(SGR_RUNTIME_SUBJECT, false),
        request,
        Trace::new("00000000000000000000000000000091", "0000000000000091", None),
    ))
}

fn retarget_for_stop(envelope: &mut ActionEnvelope) {
    UNIT_STOP_ACTION.clone_into(&mut envelope.request.action);
    envelope.request.idempotency_key = Some(format!(
        "{UNIT_STOP_ACTION}:{}",
        envelope
            .request
            .target
            .get("unit_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown")
    ));
    if let Some(target) = envelope.request.target.as_object_mut() {
        target.insert(
            "lifecycle_intent".to_owned(),
            serde_json::Value::String("stop".to_owned()),
        );
    }
}

fn runtime_context() -> RuntimeContext {
    RuntimeContext::new(
        SGR_RUNTIME_SUBJECT,
        "sgr-runtime-adapter",
        DEFAULT_CODE_VERSION,
    )
}

fn runtime_error_to_sgr(err: &RuntimeError) -> SgrError {
    SgrError::Internal(format!("capability runtime submission failed: {err}"))
}

fn failure_reason(ctx: &ActionContext) -> String {
    ctx.error.map_or_else(
        || format!("capability runtime action ended as {:?}", ctx.status),
        |reason| format!("capability runtime action failed: {reason:?}"),
    )
}
