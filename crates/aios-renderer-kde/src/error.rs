//! `KdeRendererError` taxonomy covering S7.4 invariants I4, I5, I6, I7.
//!
//! Nine closed variants. Adding a variant requires a versioned spec change per
//! the S7.4 closed-enum discipline.

use crate::types::KdeSurfaceId;
use crate::zone::ZoneLayer;

/// Closed error taxonomy for the KDE renderer (S7.4 §2).
///
/// Nine variants covering the spec invariants and deferred features
/// (Wayland, `KWin` scripting, GPU binding, icon verification, degraded mode).
#[derive(Debug, thiserror::Error)]
pub enum KdeRendererError {
    /// The requested surface id does not exist in the renderer's surface set.
    #[error("surface not found: {0}")]
    SurfaceNotFound(KdeSurfaceId),

    /// A zone layer transition was requested that is not permitted (e.g.
    /// attempting to promote a content surface to overlay without chrome
    /// identity).
    #[error("invalid zone transition from {from:?} to {to:?}")]
    InvalidZoneTransition {
        /// Current layer.
        from: ZoneLayer,
        /// Requested target layer.
        to: ZoneLayer,
    },

    /// INV I4 — a non-AIOS-chrome client requested the overlay layer, which is
    /// reserved exclusively for the AIOS chrome service.
    #[error("overlay layer forbidden for client '{client_id}'")]
    OverlayLayerForbidden {
        /// The identity of the client that attempted the forbidden claim.
        client_id: String,
    },

    /// Deferred: Wayland compositor connection failed. Type is defined now so
    /// that T-131 can use it without changing the error taxonomy.
    #[error("wayland connect error: {0}")]
    WaylandConnectError(String),

    /// Deferred: a `KWin` script failed verification (unsigned, hash mismatch, or
    /// loaded from an unauthorized path per I8).
    #[error("kwin script verification failed for script '{script_id}': {reason}")]
    KwinScriptVerificationFailed {
        /// The script identifier.
        script_id: String,
        /// Human-readable reason for the verification failure.
        reason: String,
    },

    /// Deferred: a constitutional icon bundle failed hash verification at load
    /// time or at runtime (per I6 + §9.7).
    #[error("icon bundle verification failed for theme '{theme_id}': {reason}")]
    IconBundleVerificationFailed {
        /// The theme identifier whose icon bundle failed.
        theme_id: String,
        /// Human-readable reason for the verification failure.
        reason: String,
    },

    /// Deferred: GPU device acquisition failed (per I3 — cross-group
    /// `VkDevice` isolation). The renderer cannot obtain the per-group
    /// `VkDevice` from S8.2.
    #[error("gpu binding unavailable: {0}")]
    GpuBindingUnavailable(String),

    /// INV I7 — the renderer has entered degraded (text-only) fallback mode
    /// because a required runtime capability (`KWin`, wgpu, icon bundle) is
    /// unavailable.
    #[error("renderer degraded: {0}")]
    Degraded(String),

    /// Internal renderer error (catch-all for unrecoverable failures not
    /// covered by the typed variants above).
    #[error("internal renderer error: {0}")]
    Internal(String),
}
