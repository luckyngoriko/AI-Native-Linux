//! `aios-vault` — core types for the AIOS Vault Broker opening slice.
//!
//! This crate is the T-046 types-only skeleton for:
//!
//! - S5.2 vault capabilities and key-material redaction boundaries.
//! - S5.1 subject/session identity records consumed by vault authorization.
//! - S5.4 emergency override binding records.
//!
//! Broker traits, lifecycle drivers, cryptographic operations, gRPC, evidence
//! emission, and cross-crate reconciliation land in later M6 tasks.

#![forbid(unsafe_code)]

pub mod capability;
pub mod error;
pub mod identity;
pub mod key_material;
pub mod override_class;

pub use capability::{
    CapabilityClass, CapabilityId, CapabilityState, KeyMaterialHandle, VaultCapability,
};
pub use error::VaultError;
pub use identity::{Session, SessionState, Subject, SubjectRef, SubjectType};
pub use key_material::{KeyAlgorithm, KeyMaterial};
pub use override_class::{OverrideBinding, OverrideBindingState, OverrideClass};
