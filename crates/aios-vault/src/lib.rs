//! `aios-vault` — core types for the AIOS Vault Broker opening slice.
//!
//! This crate is the T-046 types-only skeleton for:
//!
//! - S5.2 vault capabilities and key-material redaction boundaries.
//! - S5.1 subject/session identity records consumed by vault authorization.
//! - S5.4 emergency override binding records.
//!
//! Lifecycle drivers, real cryptographic operations, gRPC, evidence emission,
//! and cross-crate reconciliation land in later M6 tasks.

#![forbid(unsafe_code)]

pub mod broker;
pub mod capability;
pub mod error;
pub mod hydrator;
pub mod identity;
pub mod identity_catalog;
pub mod in_memory_broker;
pub mod key_material;
pub mod override_class;

pub use broker::{
    IssueCapabilityRequest, UseCapabilityRequest, UseCapabilityResult, VaultBroker, VaultOperation,
};
pub use capability::{
    CapabilityClass, CapabilityId, CapabilityState, KeyMaterialHandle, VaultCapability,
};
pub use error::VaultError;
pub use hydrator::{HydratedSubjectSnapshot, VaultSubjectHydrator};
pub use identity::{Session, SessionState, Subject, SubjectRef, SubjectType};
pub use identity_catalog::IdentityCatalog;
pub use in_memory_broker::InMemoryVaultBroker;
pub use key_material::{KeyAlgorithm, KeyMaterial};
pub use override_class::{OverrideBinding, OverrideBindingState, OverrideClass};
