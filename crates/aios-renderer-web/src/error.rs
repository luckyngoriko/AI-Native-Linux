//! `WebRendererError` taxonomy covering S7.5 invariants I2, I3, I4, I6, I7, I9, I10.
//!
//! Eleven closed variants. Adding a variant requires a versioned spec change per
//! the S7.5 closed-enum discipline.

use crate::exposure::ExposureLevelLabel;
use crate::types::WebSurfaceId;

/// Closed error taxonomy for the Web renderer (S7.5 §2).
///
/// Eleven variants covering spec invariants for surface management, origin
/// verification, exposure escalation, certificate checks, chrome shadow root
/// integrity, icon bundle verification, WebGPU adapter acquisition, and
/// extension interference.
#[derive(Debug, thiserror::Error)]
pub enum WebRendererError {
    /// The requested surface id does not exist in the renderer's surface set.
    #[error("surface not found: {0}")]
    SurfaceNotFound(WebSurfaceId),

    /// INV I4 — origin verification failed. The presented origin does not match
    /// the expected group-level origin pattern.
    #[error(
        "origin verification failed: expected group_id '{expected_group_id}', presented origin '{presented_origin}'"
    )]
    OriginVerificationFailed {
        /// The canonical group identifier whose origin was expected.
        expected_group_id: String,
        /// The origin that was actually presented at request time.
        presented_origin: String,
    },

    /// INV I3 — exposure level escalation denied. A transition was attempted
    /// without the required evidence or authorization.
    #[error("exposure escalation denied from {from} to {to}: {reason}")]
    ExposureEscalationDenied {
        /// The current exposure level.
        from: ExposureLevelLabel,
        /// The requested target exposure level.
        to: ExposureLevelLabel,
        /// Human-readable reason for the denial.
        reason: String,
    },

    /// INV I3 — LAN exposure was attempted without a `WEB_EXPOSURE_GRANTED`
    /// evidence receipt.
    #[error("LAN exposure attempted without WEB_EXPOSURE_GRANTED evidence")]
    LanExposureWithoutEvidence,

    /// INV I10 — the Chrome shadow root integrity check failed. The observed
    /// shadow root does not match the expected constitutionally-mandated
    /// properties (z-index, mode, or hash).
    #[error("chrome shadow root integrity failed: {reason}")]
    ChromeShadowRootIntegrityFailed {
        /// Human-readable reason for the integrity failure.
        reason: String,
    },

    /// INV I9 — TLS certificate verification failed for the origin binding.
    #[error("certificate verification failed: {0}")]
    CertificateVerificationFailed(String),

    /// INV I9 — a plain HTTP request was received on the HTTPS port. The
    /// renderer rejects with HTTP 421 Misdirected Request per RFC 7540 §9.1.2.
    #[error("plain HTTP rejected on HTTPS port: {0}")]
    PlainHttpRejected(String),

    /// INV I6 — constitutional icon bundle hash verification failed at load
    /// time. The served icon bundle does not match the expected integrity
    /// hash.
    #[error("icon bundle verification failed for theme '{theme_id}': {reason}")]
    IconBundleVerificationFailed {
        /// The theme identifier whose icon bundle failed verification.
        theme_id: String,
        /// Human-readable reason for the verification failure.
        reason: String,
    },

    /// WebGPU adapter acquisition failed. The renderer cannot obtain a GPU
    /// device for accelerated rendering.
    #[error("webgpu adapter unavailable: {0}")]
    WebgpuAdapterUnavailable(String),

    /// INV I10 — an untrusted browser extension interfered with the Chrome
    /// shadow root or the AIOS renderer surface.
    #[error("extension interference detected: {0}")]
    ExtensionInterferenceDetected(String),

    /// Internal renderer error (catch-all for unrecoverable failures not
    /// covered by the typed variants above).
    #[error("internal renderer error: {0}")]
    Internal(String),
}
