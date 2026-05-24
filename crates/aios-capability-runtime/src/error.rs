//! `RuntimeError` — typed error taxonomy for the orchestration RPCs.
//!
//! `RuntimeError` is the **internal** Rust error surface; it is wider than
//! the wire-form [`crate::RuntimeErrorCode`] enum because it carries
//! structured payloads (offending ids, transition pairs, manifest reasons)
//! that the wire form flattens to a single code.
//!
//! T-027 will introduce the mapping from `RuntimeError` to
//! [`crate::RuntimeErrorCode`] when the gRPC adapter lands.

use thiserror::Error;

use aios_action::ActionId;

use crate::status::ActionLifecycleState;

/// Closed error taxonomy for the L3 orchestration surface.
///
/// Every fallible operation in this crate returns
/// `Result<T, RuntimeError>`. The variants are deliberately specific so the
/// gRPC adapter (T-033) can mechanically map them to
/// [`crate::RuntimeErrorCode`] without ambiguity.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RuntimeError {
    /// The orchestration RPC referenced an `action_id` the runtime has no
    /// record of. Maps to [`crate::RuntimeErrorCode::RuntimeInternal`] when
    /// the runtime should have known the id, or `LIFECYCLE_TERMINAL` when
    /// the action has aged out of the working set.
    #[error("action not found: {0}")]
    ActionNotFound(ActionId),

    /// A request would drive the FSM through a transition not listed in
    /// §4.2. Carries both endpoints for forensic logging. Maps to
    /// [`crate::RuntimeErrorCode::LifecycleIllegalTransition`].
    #[error("illegal lifecycle transition: {from:?} -> {to:?}")]
    InvalidTransition {
        /// The state the runtime is currently in.
        from: ActionLifecycleState,
        /// The state the caller asked to transition into.
        to: ActionLifecycleState,
    },

    /// A lookup by `adapter_id` failed. Maps to
    /// [`crate::RuntimeErrorCode::UnknownAdapter`].
    #[error("unknown adapter: {0}")]
    AdapterUnknown(String),

    /// An adapter manifest registration failed Ed25519 signature
    /// verification. Maps to
    /// [`crate::RuntimeErrorCode::ManifestSignatureInvalid`].
    #[error("adapter manifest signature invalid")]
    AdapterSignatureInvalid,

    /// An adapter manifest registration referenced a `signing_key_id` not
    /// present in the runtime's adapter trust store (S10.1 §10.2 — the
    /// publisher key was not endorsed by the AIOS root or recognised
    /// publisher chain). T-028 surface; mirrors
    /// `PolicyError::BundleUnknownAuthority` in the M3 bundle loader.
    /// Maps to [`crate::RuntimeErrorCode::ManifestSignatureInvalid`].
    #[error("adapter manifest unknown signing authority: {0}")]
    AdapterUnknownAuthority(String),

    /// A second `runtime.adapter.register` attempted to bind an `adapter_id`
    /// already present in the registry. T-028 surface; enforces the
    /// uniqueness side of the §10.5 action-kind exclusivity rule (an
    /// `adapter_id` collision is rejected before the action-kind collision
    /// check has a chance to fire). Operators rotate a manifest by
    /// resubmitting with the **same** `adapter_id` per §10.4 — this variant
    /// is raised by the in-memory registry which models a single live
    /// registration per id; production rotation semantics are queued.
    #[error("adapter already registered: {0}")]
    AdapterAlreadyRegistered(String),

    /// A manifest failed structural validation (e.g. expired
    /// `manifest_expires_at`, unbound `${...}` token in `template_string`,
    /// duplicate `action_kind`). The string payload pins the offending
    /// reason. Maps to
    /// [`crate::RuntimeErrorCode::ManifestSignatureInvalid`] when the failure
    /// is signature-adjacent, otherwise `INVALID_ENVELOPE` semantics.
    #[error("adapter manifest invalid: {0}")]
    ManifestInvalid(String),

    /// Catch-all for unexpected internal faults. Maps to
    /// [`crate::RuntimeErrorCode::RuntimeInternal`]. Carries a free-form
    /// message; T-031 will replace this with the structured forensic record.
    #[error("runtime internal error: {0}")]
    Internal(String),
}
