use serde::{Deserialize, Serialize};

use crate::ids::ComposedSystemId;

/// A directed dependency edge between two services.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ServiceDependency {
    /// Source service in the dependency edge.
    pub from_service: String,
    /// Target service in the dependency edge.
    pub to_service: String,
    /// Whether the dependency is mandatory for the source service.
    pub required: bool,
}

/// A single service in the composition graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposedService {
    /// Unique identifier within the composition.
    pub service_id: String,
    /// Name of the AIOS crate implementing this service.
    pub crate_name: String,
    /// gRPC / IPC binding endpoint.
    pub binding_endpoint: String,
    /// Service IDs this service depends on.
    pub depends_on: Vec<String>,
}

/// A directed acyclic graph of services and their dependencies.
///
/// The `topological_order` field is unverified at construction time;
/// T-181 will provide the verifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceComposition {
    /// Unique identifier for this composition.
    pub composition_id: ComposedSystemId,
    /// All services in the composition.
    pub services: Vec<ComposedService>,
    /// All dependency edges between services.
    pub dependencies: Vec<ServiceDependency>,
    /// Topologically sorted service IDs (verified at T-181).
    pub topological_order: Vec<String>,
}
