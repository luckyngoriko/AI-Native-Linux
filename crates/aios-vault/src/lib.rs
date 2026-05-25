//! `aios-vault` — core types for the AIOS Vault Broker opening slice.
//!
//! This crate is the T-046 types-only skeleton for:
//!
//! - S5.2 vault capabilities and key-material redaction boundaries.
//! - S5.1 subject/session identity records consumed by vault authorization.
//! - S5.4 emergency override binding records.
//!
//! Real cryptographic operations and the capability lifecycle driver are now
//! present; gRPC, evidence emission, and cross-crate reconciliation land in
//! later M6 tasks.

#![forbid(unsafe_code)]

pub mod audit;
pub mod broker;
pub mod capability;
pub mod crypto;
pub mod error;
pub mod evidence_emit;
pub mod evidence_payloads;
pub mod hydrator;
pub mod identity;
pub mod identity_catalog;
pub mod in_memory_broker;
pub mod integration;
pub mod key_material;
pub mod lifecycle;
pub mod override_broker;
pub mod override_class;
pub mod service;

pub use audit::{CapabilityAuditEntry, CapabilityAuditLog};
pub use broker::{
    IssueCapabilityRequest, UseCapabilityRequest, UseCapabilityResult, VaultBroker, VaultOperation,
};
pub use capability::{
    CapabilityClass, CapabilityId, CapabilityState, KeyMaterialHandle, VaultCapability,
};
pub use error::VaultError;
pub use evidence_emit::{
    InMemoryVaultEvidenceLog, VaultEvidenceEmitter, VaultEvidenceLog, AIOS_VAULT_SUBJECT,
};
pub use evidence_payloads::{
    CapabilityExpiredPayload, CapabilityIssuedPayload, CapabilityRevokedPayload,
    CapabilityUsedPayload, OverrideConsumedPayload, OverrideGrantedPayload, OverrideRevokedPayload,
};
pub use hydrator::{HydratedSubjectSnapshot, VaultSubjectHydrator};
pub use identity::{Session, SessionState, Subject, SubjectRef, SubjectType};
pub use identity_catalog::IdentityCatalog;
pub use in_memory_broker::InMemoryVaultBroker;
pub use integration::{VaultPolicyHydrator, VaultPolicyOverrideBoundary};
pub use key_material::{KeyAlgorithm, KeyMaterial};
pub use lifecycle::{CapabilityLifecycleDriver, ExpirationPassReport};
pub use override_broker::{GrantOverrideRequest, InMemoryOverrideBroker, OverrideBroker};
pub use override_class::{OverrideBinding, OverrideBindingState, OverrideClass};
pub use service::{VaultBrokerClient, VaultBrokerGrpcServer, VaultBrokerService};

/// Default Rust crate code version for the T-052 `VaultBroker` service surface.
pub const DEFAULT_CODE_VERSION: &str = service::server::DEFAULT_CODE_VERSION;
