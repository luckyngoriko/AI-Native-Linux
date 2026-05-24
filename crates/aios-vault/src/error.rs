//! Vault error taxonomy for T-046 and later broker operations.

use thiserror::Error;

use crate::capability::{CapabilityId, CapabilityState};

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

    /// Session is expired.
    #[error("session expired: {0}")]
    SessionExpired(String),

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

    /// Internal broker failure.
    #[error("vault internal error: {0}")]
    Internal(String),

    /// Defense-in-depth guard for attempts to serialize raw key material.
    #[error("key material serialization blocked")]
    KeyMaterialLeak,
}
