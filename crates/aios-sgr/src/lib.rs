//! `aios-sgr` - typed core skeleton for S15.1, S15.2, and S15.3.

#![forbid(unsafe_code)]

pub mod adapter;
pub mod dependency;
pub mod error;
pub mod state;
pub mod unit;

pub use adapter::{
    AdapterActionDeclaration, AdapterCapability, AdapterCapabilityClass, AdapterDeclaration,
    AdapterDispatchKind, AdapterFailureMode, AdapterIOMode, AdapterManifest,
    AdapterRegistrationState, AdapterRollbackStrategy, AdapterStability,
};
pub use dependency::{DependencyEdge, DependencyKind, UnitDependency};
pub use error::SgrError;
pub use state::{
    ABPromotionState, ConflictKind, DependencySolveResult, GraphEvaluationResult, GraphState,
    ResourceDimension, ResourceSource, TransitionFailureReason, TransitionKind, UnitState,
};
pub use unit::{
    DesiredState, GpuBudget, HealthCheckKind, HealthCheckSpec, ResourceBudget, RestartBudget,
    RestartPolicy, RollbackPointer, RollbackTrigger, ServiceUnit, UnitId, UnitKind, UnitManifest,
    VerificationIntentRef,
};
