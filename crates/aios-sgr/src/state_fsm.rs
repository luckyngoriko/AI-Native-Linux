//! S15.1 unit-state FSM driver used by the S15.2 graph transition layer.

use std::sync::Arc;

use crate::{ServiceGraph, ServiceUnit, SgrError, UnitId, UnitState};

/// Exhaustive S15.1 §3.2.1 per-unit state transition table.
pub const TRANSITIONS: &[(UnitState, UnitState)] = &[
    (UnitState::Draft, UnitState::Queued),
    (UnitState::Draft, UnitState::Retired),
    (UnitState::Queued, UnitState::Starting),
    (UnitState::Queued, UnitState::Retired),
    (UnitState::Starting, UnitState::Running),
    (UnitState::Starting, UnitState::Failed),
    (UnitState::Running, UnitState::Healthy),
    (UnitState::Running, UnitState::Stopped),
    (UnitState::Running, UnitState::Failed),
    (UnitState::Healthy, UnitState::Degraded),
    (UnitState::Healthy, UnitState::Unhealthy),
    (UnitState::Healthy, UnitState::Stopping),
    (UnitState::Degraded, UnitState::Healthy),
    (UnitState::Degraded, UnitState::Unhealthy),
    (UnitState::Degraded, UnitState::Stopping),
    (UnitState::Unhealthy, UnitState::Healthy),
    (UnitState::Unhealthy, UnitState::Starting),
    (UnitState::Unhealthy, UnitState::Failed),
    (UnitState::Unhealthy, UnitState::Stopping),
    (UnitState::Stopping, UnitState::Stopped),
    (UnitState::Stopping, UnitState::Failed),
    (UnitState::Stopped, UnitState::Queued),
    (UnitState::Stopped, UnitState::Retired),
    (UnitState::Failed, UnitState::Starting),
    (UnitState::Failed, UnitState::Retired),
];

/// Return true when `(from, to)` is a legal S15.1 §3.2.1 unit-state edge.
#[must_use]
pub fn is_legal_transition(from: UnitState, to: UnitState) -> bool {
    TRANSITIONS.contains(&(from, to))
}

/// High-level unit FSM driver layered over [`ServiceGraph::set_unit_state`].
pub struct UnitFsmDriver {
    graph: Arc<dyn ServiceGraph>,
}

impl UnitFsmDriver {
    /// Construct a driver over a service graph implementation.
    #[must_use]
    pub fn new(graph: Arc<dyn ServiceGraph>) -> Self {
        Self { graph }
    }

    /// Drive a unit through legal intermediate states until it reaches `RUNNING`.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::UnitNotFound`] when the unit is absent or
    /// [`SgrError::InvalidStateTransition`] when the current state has no
    /// legal start path.
    pub async fn start(&self, unit_id: &UnitId) -> Result<ServiceUnit, SgrError> {
        self.drive_to_running(unit_id, FailedStartPolicy::Reject)
            .await
    }

    /// Drive a unit through legal intermediate states until it reaches `STOPPED`.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::UnitNotFound`] when the unit is absent or
    /// [`SgrError::InvalidStateTransition`] when the current state has no
    /// legal stop path.
    pub async fn stop(&self, unit_id: &UnitId) -> Result<ServiceUnit, SgrError> {
        loop {
            let unit = self.graph.get_unit(unit_id).await?;
            let next = match unit.state {
                UnitState::Stopped => return Ok(unit),
                UnitState::Running | UnitState::Stopping => UnitState::Stopped,
                UnitState::Healthy | UnitState::Degraded | UnitState::Unhealthy => {
                    UnitState::Stopping
                }
                UnitState::Draft
                | UnitState::Queued
                | UnitState::Starting
                | UnitState::Failed
                | UnitState::Retired => {
                    return Err(invalid_transition(unit.state, UnitState::Stopped));
                }
            };
            self.apply_or_observe(unit_id, unit.state, next).await?;
        }
    }

    /// Stop and then start a unit, using the spec retry edge for `FAILED`.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::UnitNotFound`] when the unit is absent or
    /// [`SgrError::InvalidStateTransition`] when the current state cannot be
    /// restarted.
    pub async fn restart(&self, unit_id: &UnitId) -> Result<ServiceUnit, SgrError> {
        let unit = self.graph.get_unit(unit_id).await?;
        match unit.state {
            UnitState::Failed => {
                self.drive_to_running(unit_id, FailedStartPolicy::Retry)
                    .await
            }
            UnitState::Retired => Err(invalid_transition(UnitState::Retired, UnitState::Running)),
            UnitState::Running
            | UnitState::Healthy
            | UnitState::Degraded
            | UnitState::Unhealthy
            | UnitState::Stopping => {
                self.stop(unit_id).await?;
                self.drive_to_running(unit_id, FailedStartPolicy::Reject)
                    .await
            }
            UnitState::Draft | UnitState::Queued | UnitState::Starting | UnitState::Stopped => {
                self.drive_to_running(unit_id, FailedStartPolicy::Reject)
                    .await
            }
        }
    }

    /// Mark a unit as failed, following legal intermediate edges where needed.
    ///
    /// The current typed core does not yet persist failure reasons; T-090 owns
    /// evidence emission and durable reason capture.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::UnitNotFound`] when the unit is absent or
    /// [`SgrError::InvalidStateTransition`] when the current state has no
    /// legal failure path.
    pub async fn mark_failed(
        &self,
        unit_id: &UnitId,
        reason: String,
    ) -> Result<ServiceUnit, SgrError> {
        drop(reason);

        loop {
            let unit = self.graph.get_unit(unit_id).await?;
            let next = match unit.state {
                UnitState::Failed => return Ok(unit),
                UnitState::Draft | UnitState::Queued => UnitState::Starting,
                UnitState::Starting
                | UnitState::Running
                | UnitState::Unhealthy
                | UnitState::Stopping => UnitState::Failed,
                UnitState::Healthy | UnitState::Degraded => UnitState::Unhealthy,
                UnitState::Stopped | UnitState::Retired => {
                    return Err(invalid_transition(unit.state, UnitState::Failed));
                }
            };
            self.apply_or_observe(unit_id, unit.state, next).await?;
        }
    }

    /// Apply a single generic state transition.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::UnitNotFound`] when the unit is absent or
    /// [`SgrError::InvalidStateTransition`] when the direct edge is forbidden.
    pub async fn transition(
        &self,
        unit_id: &UnitId,
        target: UnitState,
    ) -> Result<ServiceUnit, SgrError> {
        let unit = self.graph.get_unit(unit_id).await?;
        if unit.state == target {
            return Ok(unit);
        }
        if !is_legal_transition(unit.state, target) {
            return Err(invalid_transition(unit.state, target));
        }
        self.graph.set_unit_state(unit_id, target).await
    }

    async fn drive_to_running(
        &self,
        unit_id: &UnitId,
        failed_policy: FailedStartPolicy,
    ) -> Result<ServiceUnit, SgrError> {
        loop {
            let unit = self.graph.get_unit(unit_id).await?;
            let next = match unit.state {
                UnitState::Running => return Ok(unit),
                UnitState::Draft | UnitState::Stopped => UnitState::Queued,
                UnitState::Queued => UnitState::Starting,
                UnitState::Starting => UnitState::Running,
                UnitState::Failed if failed_policy == FailedStartPolicy::Retry => {
                    UnitState::Starting
                }
                UnitState::Failed => {
                    return Err(invalid_transition(UnitState::Failed, UnitState::Running))
                }
                UnitState::Healthy
                | UnitState::Degraded
                | UnitState::Unhealthy
                | UnitState::Stopping
                | UnitState::Retired => {
                    return Err(invalid_transition(unit.state, UnitState::Running));
                }
            };
            self.apply_or_observe(unit_id, unit.state, next).await?;
        }
    }

    async fn apply_or_observe(
        &self,
        unit_id: &UnitId,
        expected_from: UnitState,
        next: UnitState,
    ) -> Result<ServiceUnit, SgrError> {
        if !is_legal_transition(expected_from, next) {
            return Err(invalid_transition(expected_from, next));
        }

        match self.graph.set_unit_state(unit_id, next).await {
            Ok(unit) => Ok(unit),
            Err(SgrError::InvalidStateTransition { from, to })
                if from != expected_from && to == next =>
            {
                self.graph.get_unit(unit_id).await
            }
            Err(err) => Err(err),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailedStartPolicy {
    Reject,
    Retry,
}

const fn invalid_transition(from: UnitState, to: UnitState) -> SgrError {
    SgrError::InvalidStateTransition { from, to }
}
