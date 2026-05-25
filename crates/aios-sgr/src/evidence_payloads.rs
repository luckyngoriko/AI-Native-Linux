//! Typed SGR evidence payloads for S15.x -> S3.1 emission.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S15.x evidence vocabulary"
)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{DependencyKind, GraphState, UnitId, UnitKind};

/// Payload for `UNIT_REGISTERED`.
///
/// Carries only the signing authority name plus a bounded signature hex prefix;
/// raw Ed25519 signature bytes remain on the manifest, not in evidence JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct UnitRegisteredPayload {
    /// Registered unit id.
    pub unit_id: UnitId,
    /// Registered unit kind.
    pub kind: UnitKind,
    /// Human-readable unit name.
    pub name: String,
    /// Manifest authority plus bounded signature prefix.
    pub signing_authority: String,
    /// UTC timestamp at which the graph accepted the unit.
    pub registered_at: DateTime<Utc>,
}

/// Payload for `UNIT_STARTED`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct UnitStartedPayload {
    /// Started unit id.
    pub unit_id: UnitId,
    /// UTC timestamp of the final `RUNNING` transition.
    pub started_at: DateTime<Utc>,
}

/// Payload for `UNIT_STOPPED`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct UnitStoppedPayload {
    /// Stopped unit id.
    pub unit_id: UnitId,
    /// UTC timestamp of the final `STOPPED` transition.
    pub stopped_at: DateTime<Utc>,
    /// True when the unit manifest's desired state was already `STOPPED`.
    pub requested_by_desired_state: bool,
}

/// Payload for `UNIT_FAILED`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct UnitFailedPayload {
    /// Failed unit id.
    pub unit_id: UnitId,
    /// Non-secret failure reason.
    pub reason: String,
    /// UTC timestamp of the final `FAILED` transition.
    pub failed_at: DateTime<Utc>,
}

/// Payload for dependency-edge declaration.
///
/// S3.1 has no dedicated `DEPENDENCY_DECLARED` record type today; the emitter
/// folds this payload into `GRAPH_EVALUATED`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct DependencyDeclaredPayload {
    /// Source unit id.
    pub from: UnitId,
    /// Target unit id.
    pub to: UnitId,
    /// Dependency edge kind.
    pub kind: DependencyKind,
    /// UTC timestamp when the edge was declared.
    pub declared_at: DateTime<Utc>,
}

/// Payload for `GRAPH_CONVERGED`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct GraphConvergedPayload {
    /// Observed graph state.
    pub graph_state: GraphState,
    /// Number of units in the converged graph.
    pub unit_count: u64,
    /// UTC timestamp when convergence was observed.
    pub converged_at: DateTime<Utc>,
}

/// Payload for `ADAPTER_REGISTERED`.
///
/// Carries only the signing authority name plus a bounded signature hex prefix;
/// raw Ed25519 signature bytes remain on the adapter capability/declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AdapterRegisteredPayload {
    /// Registered capability id.
    pub capability_id: String,
    /// Registry acceptance timestamp.
    pub registered_at: DateTime<Utc>,
    /// Adapter signing authority plus bounded signature prefix.
    pub signing_authority: String,
}
