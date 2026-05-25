//! `aios-sgr` - typed core skeleton for S15.1, S15.2, and S15.3.

#![forbid(unsafe_code)]

pub mod adapter;
pub mod adapter_registry;
pub mod dependency;
pub mod error;
pub mod evaluator;
pub mod graph;
pub mod in_memory_graph;
pub mod service;
pub mod state;
pub mod state_fsm;
pub mod unit;

pub use adapter::{
    AdapterActionDeclaration, AdapterCapability, AdapterCapabilityClass, AdapterDeclaration,
    AdapterDispatchKind, AdapterFailureMode, AdapterIOMode, AdapterManifest,
    AdapterRegistrationState as AdapterManifestRegistrationState, AdapterRollbackStrategy,
    AdapterStability,
};
pub use adapter_registry::{AdapterRegistrationState, RegisteredAdapter, SgrAdapterRegistry};
pub use dependency::{DependencyEdge, DependencyKind, UnitDependency};
pub use error::SgrError;
pub use evaluator::GraphEvaluator;
pub use graph::ServiceGraph;
pub use in_memory_graph::InMemoryServiceGraph;
pub use service::{
    build_router, serve, SgrServiceClient, SgrServiceGrpc, SgrServiceGrpcServer, SgrServiceImpl,
    SCHEMA_VERSION,
};
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

/// Default Rust crate code version reported by the T-089 gRPC service adapter.
pub const DEFAULT_CODE_VERSION: &str = "0.0.1-T089";
