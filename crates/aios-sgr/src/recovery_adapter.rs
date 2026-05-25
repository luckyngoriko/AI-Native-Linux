//! Recovery-mode hook for SGR unit pausing.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the cross-crate recovery integration vocabulary"
)]

use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use aios_capability_runtime::RuntimeRecoveryHook;
use aios_recovery::RecoveryBoundary;

use crate::{ServiceGraph, ServiceUnit, SgrError, UnitFsmDriver, UnitId, UnitKind, UnitState};

/// Bridges the live recovery boundary into the runtime and pauses normal SGR units.
#[derive(Clone)]
pub struct SgrRecoveryHook {
    graph: Arc<dyn ServiceGraph>,
    fsm: Arc<UnitFsmDriver>,
    boundary: Arc<dyn RecoveryBoundary>,
    paused_normal_units: Arc<RwLock<HashSet<UnitId>>>,
}

impl fmt::Debug for SgrRecoveryHook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SgrRecoveryHook")
            .field("graph", &"<dyn ServiceGraph>")
            .field("fsm", &"<UnitFsmDriver>")
            .field("boundary", &"<dyn RecoveryBoundary>")
            .finish_non_exhaustive()
    }
}

impl SgrRecoveryHook {
    /// Construct a hook from already-erased graph and recovery-boundary handles.
    #[must_use]
    pub fn new(
        graph: Arc<dyn ServiceGraph>,
        fsm: Arc<UnitFsmDriver>,
        boundary: Arc<dyn RecoveryBoundary>,
    ) -> Self {
        Self {
            graph,
            fsm,
            boundary,
            paused_normal_units: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Construct a hook from a concrete recovery boundary.
    #[must_use]
    pub fn from_boundary<B>(
        graph: Arc<dyn ServiceGraph>,
        fsm: Arc<UnitFsmDriver>,
        boundary: Arc<B>,
    ) -> Self
    where
        B: RecoveryBoundary + 'static,
    {
        Self::new(graph, fsm, boundary)
    }

    /// Stop currently running normal-mode units while recovery mode is active.
    ///
    /// Units marked with the compatibility manifest label
    /// `recovery_mode_allowed = true`, or units of kind `RECOVERY_TASK`, remain live.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError`] when graph listing or an SGR FSM transition fails.
    pub async fn pause_normal_units(&self) -> Result<Vec<UnitId>, SgrError> {
        let units = self.graph.list_units().await?;
        let mut paused = Vec::new();
        for unit in units {
            if should_pause_for_recovery(&unit) {
                let stopped = self.fsm.stop(&unit.unit_id).await?;
                if stopped.state == UnitState::Stopped {
                    paused.push(unit.unit_id);
                }
            }
        }
        if !paused.is_empty() {
            let mut guard = self.paused_normal_units.write().await;
            guard.extend(paused.iter().cloned());
        }
        Ok(paused)
    }

    /// Restart units previously stopped by [`Self::pause_normal_units`].
    ///
    /// # Errors
    ///
    /// Returns [`SgrError`] when graph lookup or an SGR FSM transition fails.
    pub async fn resume_normal_units(&self) -> Result<Vec<UnitId>, SgrError> {
        let paused = {
            let guard = self.paused_normal_units.read().await;
            guard.iter().cloned().collect::<Vec<_>>()
        };
        let mut resumed = Vec::new();
        for unit_id in &paused {
            let unit = self.graph.get_unit(unit_id).await?;
            if unit.state == UnitState::Stopped {
                let running = self.fsm.start(unit_id).await?;
                if running.state == UnitState::Running {
                    resumed.push(unit_id.clone());
                }
            }
        }
        if !paused.is_empty() {
            let mut guard = self.paused_normal_units.write().await;
            for unit_id in &paused {
                guard.remove(unit_id);
            }
        }
        Ok(resumed)
    }

    /// Snapshot the unit ids paused by this hook and not yet resumed.
    pub async fn paused_units(&self) -> Vec<UnitId> {
        self.paused_normal_units
            .read()
            .await
            .iter()
            .cloned()
            .collect()
    }

    /// Return `true` while the wrapped S9.1 boundary is in recovery mode.
    pub async fn current_recovery_mode(&self) -> bool {
        self.boundary.is_recovery_active().await
    }
}

#[async_trait]
impl RuntimeRecoveryHook for SgrRecoveryHook {
    async fn current_recovery_mode(&self) -> bool {
        self.current_recovery_mode().await
    }
}

fn should_pause_for_recovery(unit: &ServiceUnit) -> bool {
    is_live_normal_unit(unit.state) && !recovery_mode_allowed(unit)
}

const fn is_live_normal_unit(state: UnitState) -> bool {
    matches!(
        state,
        UnitState::Running | UnitState::Healthy | UnitState::Degraded | UnitState::Unhealthy
    )
}

fn recovery_mode_allowed(unit: &ServiceUnit) -> bool {
    unit.manifest.unit_kind == UnitKind::RecoveryTask
        || unit
            .manifest
            .labels
            .as_ref()
            .and_then(|labels| labels.get("recovery_mode_allowed"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
}
