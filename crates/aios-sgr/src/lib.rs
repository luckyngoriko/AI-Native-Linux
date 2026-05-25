//! `aios-sgr` - typed core skeleton for S15.1, S15.2, and S15.3.

#![forbid(unsafe_code)]

pub mod adapter;
pub mod dependency;
pub mod error;
pub mod graph;
pub mod in_memory_graph;
pub mod state;
pub mod state_fsm;
pub mod unit;

pub use adapter::{
    AdapterActionDeclaration, AdapterCapability, AdapterCapabilityClass, AdapterDeclaration,
    AdapterDispatchKind, AdapterFailureMode, AdapterIOMode, AdapterManifest,
    AdapterRegistrationState, AdapterRollbackStrategy, AdapterStability,
};
pub use dependency::{DependencyEdge, DependencyKind, UnitDependency};
pub use error::SgrError;
pub use graph::ServiceGraph;
pub use in_memory_graph::InMemoryServiceGraph;
pub use state::{
    ABPromotionState, ConflictKind, DependencySolveResult, GraphEvaluationResult, GraphState,
    ResourceDimension, ResourceSource, TransitionFailureReason, TransitionKind, UnitState,
};
pub use state_fsm::{is_legal_transition, UnitFsmDriver, TRANSITIONS};
pub use unit::{
    DesiredState, GpuBudget, HealthCheckKind, HealthCheckSpec, ResourceBudget, RestartBudget,
    RestartPolicy, RollbackPointer, RollbackTrigger, ServiceUnit, UnitId, UnitKind, UnitManifest,
    VerificationIntentRef,
};
