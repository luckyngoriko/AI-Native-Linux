//! `aios-container` — S24 Container and Kubernetes Native Plane.
//!
//! Provides the typed core skeleton for container admission, engine selection,
//! isolation mapping, workload import detection, and security profile gates.
//! Full gRPC surface, evidence emission, and runtime integration land in
//! later tasks.

#![forbid(unsafe_code)]

pub mod ecosystem_adapters;
pub mod engine_policy;
pub mod enums;
pub mod evidence;
pub mod importer;
pub mod isolation;
pub mod passport;
pub mod profile_gates;

pub use ecosystem_adapters::{is_ai_allowed_runtime, map_runtime_to_isolation};
pub use engine_policy::ContainerEnginePolicy;
pub use enums::{
    ContainerAdmissionDecision, ContainerEngine, EcosystemRuntimeAdapter, ImageBuildEngine,
    IsolationLevel, K8sProfile, WorkloadImporter,
};
pub use evidence::{
    encode_admission_evidence, ContainerAdmittedPayload, ContainerBlockedPayload,
    ContainerQuarantinedPayload,
};
pub use importer::parse_workload;
pub use isolation::SecureRuntimeSelector;
pub use passport::CloudNativePassport;
pub use profile_gates::{is_privileged_allowed, is_unsigned_allowed, requires_digest_pin};
