//! Vault error taxonomy for T-046 and later broker operations.

use thiserror::Error;

use crate::broker::VaultOperation;
use crate::capability::{CapabilityClass, CapabilityId, CapabilityState};
use crate::key_material::KeyAlgorithm;

/// Typed vault error surface.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum VaultError {
    /// Capability id was not present in the capability catalog.
    #[error("capability not found: {0}")]
    CapabilityNotFound(CapabilityId),

    /// Capability has expired.
    #[error("capability expired: {0}")]
    CapabilityExpired(CapabilityId),

    /// Capability has been revoked.
    #[error("capability revoked: {0}")]
    CapabilityRevoked(CapabilityId),

    /// Subject id was not present in the identity catalog.
    #[error("subject not found: {0}")]
    SubjectNotFound(String),

    /// Subject id is already present in the identity catalog.
    #[error("subject already registered: {0}")]
    SubjectAlreadyRegistered(String),

    /// Session is expired.
    #[error("session expired: {0}")]
    SessionExpired(String),

    /// Subject already has an active session.
    #[error("session already active: {0}")]
    SessionAlreadyActive(String),

    /// Requested group membership mutation would not change stored state.
    #[error("group membership unchanged")]
    GroupMembershipUnchanged,

    /// Override binding id was not present in the override catalog.
    #[error("override binding not found: {0}")]
    OverrideBindingNotFound(String),

    /// Override binding has already been consumed.
    #[error("override binding already consumed")]
    OverrideAlreadyConsumed,

    /// Capability state transition is not permitted by S5.2.
    #[error("invalid capability transition: {from:?} -> {to:?}")]
    InvalidTransition {
        /// Current capability state.
        from: CapabilityState,
        /// Requested capability state.
        to: CapabilityState,
    },

    /// Requested operation is not valid for the capability's class.
    #[error("operation class mismatch: {capability_class:?} cannot run {operation_kind}")]
    OperationClassMismatch {
        /// Capability class on the stored capability.
        capability_class: CapabilityClass,
        /// Operation kind requested by the caller.
        operation_kind: String,
    },

    /// Operation is typed but intentionally not implemented until T-049+.
    #[error("operation unsupported in T-047: {0}")]
    OperationUnsupportedInT047(VaultOperation),

    /// Operation is typed but intentionally not implemented until after T-049.
    #[error("operation unsupported in T-049: {0}")]
    OperationUnsupportedInT049(VaultOperation),

    /// Capability class and requested key algorithm are incompatible.
    #[error("key algorithm mismatch: expected {expected:?}, found {found:?}")]
    KeyAlgorithmMismatch {
        /// Algorithm expected for the capability class.
        expected: KeyAlgorithm,
        /// Algorithm supplied by the caller.
        found: KeyAlgorithm,
    },

    /// Cryptographic primitive failed without exposing backend error types.
    #[error("crypto error: {0}")]
    CryptoError(String),

    /// Internal broker failure.
    #[error("vault internal error: {0}")]
    Internal(String),

    /// Defense-in-depth guard for attempts to serialize raw key material.
    #[error("key material serialization blocked")]
    KeyMaterialLeak,
}
