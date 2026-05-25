//! S15.1 dependency graph types.

use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

use crate::unit::UnitId;

/// S15.1 closed dependency-kind enum, three values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DependencyKind {
    /// `REQUIRES_HEALTHY` - target unit must be `HEALTHY`.
    RequiresHealthy,
    /// `REQUIRES_RUNNING` - target unit must be running enough to serve.
    RequiresRunning,
    /// `ORDERS_AFTER` - soft ordering edge.
    OrdersAfter,
}

impl DependencyKind {
    /// Return true for hard dependency edges.
    #[must_use]
    pub const fn is_hard(self) -> bool {
        matches!(self, Self::RequiresHealthy | Self::RequiresRunning)
    }
}

/// Dependency entry embedded inside a `UnitManifest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitDependency {
    /// Target unit id.
    pub unit_id: UnitId,
    /// Dependency kind.
    pub kind: DependencyKind,
}

/// Pair-typed dependency edge for graph views and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyEdge {
    /// Source unit id.
    pub from_unit_id: UnitId,
    /// Target unit id.
    pub to_unit_id: UnitId,
    /// Edge kind.
    pub kind: DependencyKind,
}
