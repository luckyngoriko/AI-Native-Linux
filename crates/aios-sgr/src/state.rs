//! S15.2 unit and graph state types.

use std::fmt;

use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// S15.1 closed per-unit FSM, eleven values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UnitState {
    /// `DRAFT` - manifest accepted but not admitted into the active graph.
    Draft,
    /// `QUEUED` - admitted and waiting on dependencies.
    Queued,
    /// `STARTING` - start action is in flight.
    Starting,
    /// `RUNNING` - live, before first healthy probe.
    Running,
    /// `HEALTHY` - health checks passed.
    Healthy,
    /// `DEGRADED` - serving with degraded health.
    Degraded,
    /// `UNHEALTHY` - health check failed.
    Unhealthy,
    /// `STOPPING` - stop action is in flight.
    Stopping,
    /// `STOPPED` - clean lifecycle completion or deliberate stop.
    Stopped,
    /// `FAILED` - terminal failure requiring operator action.
    Failed,
    /// `RETIRED` - withdrawn forensic state.
    Retired,
}

impl UnitState {
    /// Return true for runtime terminal outcomes.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Stopped | Self::Failed | Self::Retired)
    }

    /// Return the exact S15.1 wire token.
    #[must_use]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Queued => "QUEUED",
            Self::Starting => "STARTING",
            Self::Running => "RUNNING",
            Self::Healthy => "HEALTHY",
            Self::Degraded => "DEGRADED",
            Self::Unhealthy => "UNHEALTHY",
            Self::Stopping => "STOPPING",
            Self::Stopped => "STOPPED",
            Self::Failed => "FAILED",
            Self::Retired => "RETIRED",
        }
    }
}

impl fmt::Display for UnitState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

/// T-084 graph lifecycle vocabulary for the SGR runtime shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GraphState {
    /// `EMPTY` - no desired-state graph is active.
    Empty,
    /// `RESOLVING` - manifests and dependencies are being resolved.
    Resolving,
    /// `CONVERGING` - transition application is in progress.
    Converging,
    /// `CONVERGED` - live graph equals desired graph.
    Converged,
    /// `DEGRADED` - graph is serving with degraded units.
    Degraded,
    /// `FAILED` - graph evaluation or convergence failed.
    Failed,
}

/// S15.2 closed `GraphEvaluationResult` enum, five values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GraphEvaluationResult {
    /// `CONVERGED` - live graph equals desired graph.
    Converged,
    /// `IN_PROGRESS` - transitions can proceed.
    InProgress,
    /// `BLOCKED_DEPENDENCY` - dependency ordering blocks the plan.
    BlockedDependency,
    /// `BLOCKED_RESOURCE` - resource composition blocks the plan.
    BlockedResource,
    /// `FAILED` - deterministic evaluation failed.
    Failed,
}

/// S15.2 closed `TransitionKind` enum, ten values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransitionKind {
    /// `START` - start a unit.
    Start,
    /// `STOP` - stop a unit.
    Stop,
    /// `RESTART` - restart a unit.
    Restart,
    /// `UPGRADE_AB_PROMOTE` - promote B in an A/B upgrade.
    UpgradeAbPromote,
    /// `UPGRADE_AB_ROLLBACK` - roll back B in an A/B upgrade.
    UpgradeAbRollback,
    /// `SCALE_UP` - increase replica count.
    ScaleUp,
    /// `SCALE_DOWN` - decrease replica count.
    ScaleDown,
    /// `RECONFIGURE` - apply runtime configuration.
    Reconfigure,
    /// `DEPENDENCY_REORDER` - rewrite graph ordering metadata.
    DependencyReorder,
    /// `NO_OP` - evidence-only no-op transition.
    NoOp,
}

/// S15.2 closed `DependencySolveResult` enum, four values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DependencySolveResult {
    /// `SATISFIED` - plan is ordered.
    Satisfied,
    /// `WAITING` - at least one prerequisite is not ready.
    Waiting,
    /// `IMPOSSIBLE` - a prerequisite cannot exist.
    Impossible,
    /// `CYCLE` - dependency graph contains a cycle.
    Cycle,
}

/// S15.2 closed `ABPromotionState` enum, five values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ABPromotionState {
    /// `B` - variant B has started but receives no production traffic.
    B,
    /// `CANARY` - variant B receives canary traffic.
    Canary,
    /// `A_PROMOTED` - B is promoted to primary.
    #[serde(rename = "A_PROMOTED")]
    APromoted,
    /// `STABLE` - promotion is sealed.
    Stable,
    /// `ROLLBACK` - promotion failed or was rolled back.
    Rollback,
}

/// S15.2 transition failure reason companion enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransitionFailureReason {
    /// `ADAPTER_ERROR` - adapter returned an execution error.
    AdapterError,
    /// `HEALTH_PROBE_TIMEOUT` - health probe timed out.
    HealthProbeTimeout,
    /// `AB_HEALTHCHECK_THRESHOLD` - A/B failure threshold fired.
    AbHealthcheckThreshold,
    /// `VERIFICATION_FAILED` - verification failed.
    VerificationFailed,
    /// `BUDGET_EXCEEDED` - resource budget blocked the transition.
    BudgetExceeded,
    /// `ROLLED_BACK_BY_OPERATOR` - operator rolled back.
    RolledBackByOperator,
    /// `PRECONDITION_LOST` - prerequisite changed during dispatch.
    PreconditionLost,
}

/// S15.2 transition conflict companion enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConflictKind {
    /// `INCOMPATIBLE_KINDS` - transition kinds cannot overlap.
    IncompatibleKinds,
    /// `OVERLAPPING_RECONFIGURE` - reconfigure transitions overlap.
    OverlappingReconfigure,
    /// `OVERLAPPING_AB` - A/B promotions overlap.
    OverlappingAb,
}

/// S15.2 resource dimension companion enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResourceDimension {
    /// `CPU` - CPU quota.
    Cpu,
    /// `MEMORY` - memory budget.
    Memory,
    /// `GPU_COMPUTE` - GPU compute budget.
    GpuCompute,
    /// `FLOOR_SEAT` - sandbox floor seat.
    FloorSeat,
    /// `NETWORK_CAPABILITY` - network capability budget.
    NetworkCapability,
}

/// S15.2 resource source companion enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResourceSource {
    /// `DESIRED_GRAPH` - desired graph blocked the resource.
    DesiredGraph,
    /// `MANIFEST` - manifest budget blocked the resource.
    Manifest,
    /// `SANDBOX_FLOOR` - sandbox floor blocked the resource.
    SandboxFloor,
    /// `POLICY_FLOOR` - policy floor blocked the resource.
    PolicyFloor,
}
