//! Async S15.1 service-graph contract.

use async_trait::async_trait;

use crate::{
    DependencyEdge, DependencyKind, GraphState, ServiceUnit, SgrError, UnitId, UnitManifest,
    UnitState,
};

/// Service graph surface used by SGR callers.
#[async_trait]
pub trait ServiceGraph: Send + Sync {
    /// Verify and register a unit manifest into the active graph.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError`] when the manifest authority is unknown, the
    /// signature is invalid, the unit id is already present, or a declared
    /// dependency target is not registered.
    async fn register_unit(&self, manifest: UnitManifest) -> Result<ServiceUnit, SgrError>;

    /// Return a registered unit by id.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::UnitNotFound`] when `unit_id` is absent.
    async fn get_unit(&self, unit_id: &UnitId) -> Result<ServiceUnit, SgrError>;

    /// Return every registered unit.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError`] only for implementation-level failures.
    async fn list_units(&self) -> Result<Vec<ServiceUnit>, SgrError>;

    /// Declare a dependency edge from one registered unit to another.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::UnitNotFound`] when `from` is absent and
    /// [`SgrError::DependencyTargetNotRegistered`] when `to` is absent.
    async fn declare_dependency(
        &self,
        from: &UnitId,
        to: &UnitId,
        kind: DependencyKind,
    ) -> Result<DependencyEdge, SgrError>;

    /// Return dependency edges declared from `unit_id`.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::UnitNotFound`] when `unit_id` is absent.
    async fn list_dependencies(&self, unit_id: &UnitId) -> Result<Vec<DependencyEdge>, SgrError>;

    /// Derive the current graph state from registered unit states.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError`] only for implementation-level failures.
    async fn graph_state(&self) -> Result<GraphState, SgrError>;

    /// Apply a low-level unit state update after validating the S15.1 FSM edge.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::UnitNotFound`] when `unit_id` is absent or
    /// [`SgrError::InvalidStateTransition`] when the transition is forbidden.
    async fn set_unit_state(
        &self,
        unit_id: &UnitId,
        new_state: UnitState,
    ) -> Result<ServiceUnit, SgrError>;
}
